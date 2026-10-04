//! Two players at an air hockey table, through the room (seats.rs): the table has two seats,
//! "hockey:x,y". Both play on the same rink, seat 0's goal at its bottom and seat 1's at its
//! top; each sees it turned so that their own goal is at the bottom.
//!
//! Each player's machine runs the whole game, a fixed frame at a time (hockey's `play_frame`),
//! from both players' inputs: where their paddle should be. GGRS carries the inputs between
//! them a few frames behind (`input_delay_for`), and where the other player's hasn't come yet it
//! guesses their paddle carried on as it was going (`Input::guess_next`); when it comes and the
//! guess was wrong, it goes back to the frame before, plays the frames since again with it, and
//! catches up, all before the next picture.
//! So everyone's own paddle answers at once, and both rinks end up the same: GGRS checks that
//! every second. The packets go straight from one browser to the other where WebRTC can
//! connect them, and through the room until it does (web/room.js).
//!
//! Both rinks start alike, from player 1's settings (`Message::Begin`), which hold for the
//! match.

use std::cell::RefCell;
use std::time::Duration;

use bevy::prelude::*;
use ggrs::{
    Config, DesyncDetection, GgrsError, GgrsEvent, GgrsRequest, InputPredictor, NonBlockingSocket,
    P2PSession, PlayerType, SessionBuilder, SessionState,
};
use hockey::{FRAME, Input, Rink};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Frames between a player moving their mouse and their paddle moving, on both rinks, at most:
/// the fewer, the sooner it answers, and the further back the other rink has to put things
/// right when it guessed wrong.
const MOST_INPUT_DELAY: usize = 3;
/// A round trip that gets one more frame of input delay, in seconds: one frame up to this, two
/// up to twice it, and so on.
const ROUND_TRIP_PER_FRAME: f32 = 0.06;

/// The input delay for a match whose round trip is `round_trip` seconds: a frame for a quick
/// connection, more for a slow one, so corrections stay short, but never so many the paddle
/// feels slow.
pub fn input_delay_for(round_trip: f32) -> usize {
    // Timed a frame at a time, each answer is seen up to a frame late at each end: about one
    // frame's worth too slow, on the whole.
    let round_trip = round_trip - FRAME;
    let frames = (round_trip / ROUND_TRIP_PER_FRAME).ceil().max(1.0) as usize;
    frames.min(MOST_INPUT_DELAY)
}
/// How many frames ahead of the other player's last input a rink may run on guesses, before it
/// waits for them: enough for a round trip of about 200 ms.
const MAX_PREDICTION: usize = 12;
/// Frames between checks that both rinks still match.
const DESYNC_INTERVAL: u32 = 60;
/// At most this many frames are played in one go, catching up when the browser draws slowly
/// (a picture every 1/8 s at worst) or after a slow moment; beyond that, the time is dropped (as
/// when the tab was in the background).
const MOST_FRAMES_AT_ONCE: u32 = 8;
/// At least this many frames between frames skipped to let the other player catch up, so the
/// slowing down is spread thin.
const SKIP_SPREAD: u32 = 20;

/// What one player at the table tells the other, besides inputs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A match begins, playing by these settings (player 1's): the puck's top speed and
    /// friction, how bouncy the rails and paddles are, and the bot's speed; with this input
    /// delay, from the round trip player 1 timed.
    Begin {
        settings: [f32; 5],
        input_delay: usize,
    },
}

/// How the room names an air hockey table: "hockey:x,y", its first cell.
pub fn table_id(cell: IVec2) -> String {
    format!("hockey:{},{}", cell.x, cell.y)
}

/// A rink's settings as they go in `Begin`, and back.
pub fn settings_to_message(settings: &hockey::Settings) -> [f32; 5] {
    [
        settings.max_speed,
        settings.friction,
        settings.rail_restitution,
        settings.paddle_restitution,
        settings.bot_speed,
    ]
}

pub fn settings_from_message(values: [f32; 5]) -> hockey::Settings {
    let [
        max_speed,
        friction,
        rail_restitution,
        paddle_restitution,
        bot_speed,
    ] = values;
    hockey::Settings {
        max_speed,
        friction,
        rail_restitution,
        paddle_restitution,
        bot_speed,
    }
}

struct Hockey;

/// While the other player's input is on its way, their paddle carries on as it was going.
struct CarryOn;

impl InputPredictor<Input> for CarryOn {
    fn predict(previous: Input) -> Input {
        previous.guess_next()
    }
}

impl Config for Hockey {
    type Input = Input;
    type InputPredictor = CarryOn;
    /// The whole rink: a few numbers, so rollback keeps copies of it.
    type State = Rink;
    /// The other player's seat.
    type Address = usize;
}

/// GGRS's packets, through the page's link to the other player.
struct Socket {
    /// The other player's id in the room, and seat.
    partner: u32,
    seat: usize,
}

impl NonBlockingSocket<usize> for Socket {
    fn send_to(&mut self, message: &ggrs::Message, _seat: &usize) {
        if let Ok(bytes) = bincode::serialize(message) {
            table_link_send(self.partner, &bytes);
        }
    }

    fn receive_all_messages(&mut self) -> Vec<(usize, ggrs::Message)> {
        let partner = self.partner;
        PACKETS
            .take()
            .into_iter()
            .filter(|(from, bytes)| *from == partner && ping(bytes).is_none())
            .filter_map(|(_, bytes)| Some((self.seat, bincode::deserialize(&bytes).ok()?)))
            .collect()
    }
}

/// Before a match, player 1 times round trips to player 2 over the link itself (the path the
/// match's packets take), with tiny packets of their own: "vabp", then the ping's number, then
/// 0 going out and 1 coming back. GGRS's own are longer, and never start like that.
const PING_MARK: &[u8; 4] = b"vabp";

/// A ping packet's number and whether it's the answer, if `bytes` is one.
fn ping(bytes: &[u8]) -> Option<(u8, bool)> {
    match bytes {
        [mark @ .., n, kind] if mark == PING_MARK => Some((*n, *kind == 1)),
        _ => None,
    }
}

fn ping_packet(n: u8, answer: bool) -> [u8; 6] {
    let [a, b, c, d] = *PING_MARK;
    [a, b, c, d, n, u8::from(answer)]
}

thread_local! {
    /// The other player linked to for packets (see `link`), before and through a match.
    static LINKED: RefCell<Option<u32>> = const { RefCell::new(None) };
}

/// A match in progress.
struct Net {
    session: P2PSession<Hockey>,
    me: usize,
    partner: u32,
    /// Time not yet played as frames.
    time: f32,
    /// Frames GGRS says to skip, for the other player to catch up, and frames until the next
    /// may be.
    to_skip: u32,
    since_skip: u32,
    /// The other player's packets stopped coming, for now.
    interrupted: bool,
    input_delay: usize,
    /// This player's last input.
    last_input: Option<Input>,
    counts: Counts,
}

/// What the match has been doing, counted for `/netstats`.
#[derive(Clone, Copy, Default)]
struct Counts {
    frames: u32,
    rollbacks: u32,
    replayed: u32,
    skipped: u32,
    stalled: u32,
    desyncs: u32,
}

/// How the match is getting on, over the last second or so (`/netstats`).
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub ping: Option<u128>,
    /// Frames between a player's mouse and their paddle.
    pub input_delay: usize,
    /// Packets go straight to the other browser, not through the room.
    pub direct: bool,
    pub frames_ahead: i32,
    /// Per second: frames played, rollbacks, frames played again in them, frames skipped for the
    /// other player to catch up, and frames waited on them.
    pub frames: u32,
    pub rollbacks: u32,
    pub replayed: u32,
    pub skipped: u32,
    pub stalled: u32,
    /// Times the two rinks were found to differ, all match.
    pub desyncs: u32,
}

thread_local! {
    /// Packets from other players, oldest first: who from, and the bytes.
    static PACKETS: RefCell<Vec<(u32, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
    static NET: RefCell<Option<Net>> = const { RefCell::new(None) };
}

/// Called by index.html with a packet from another player at a table.
#[wasm_bindgen]
pub fn table_packet(from: u32, bytes: Vec<u8>) {
    PACKETS.with_borrow_mut(|packets| packets.push((from, bytes)));
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Links this player to `partner` for packets (web/room.js): `offers` says which one sets up
    /// WebRTC, and both name the link `link`.
    #[wasm_bindgen(js_name = tableLink)]
    fn table_link(partner: u32, offers: bool, link: &str);
    #[wasm_bindgen(js_name = tableLinkSend)]
    fn table_link_send(partner: u32, bytes: &[u8]);
    #[wasm_bindgen(js_name = tableUnlink)]
    fn table_unlink(partner: u32);
    /// Whether packets to `partner` go straight to their browser (WebRTC).
    #[wasm_bindgen(js_name = tableLinkDirect)]
    fn table_link_direct(partner: u32) -> bool;
}

/// Links this player, in seat `me` at `table`, to `partner` for packets: the round trips are
/// timed over it, and then the match is played over it. Player 1 sets up WebRTC.
pub fn link(table: &str, me: usize, partner: u32, my_id: u32) {
    stop();
    let link = format!("{table}/{}/{}", partner.min(my_id), partner.max(my_id));
    table_link(partner, me == 0, &link);
    LINKED.set(Some(partner));
}

/// Whether the packets to the linked player go straight to their browser yet.
pub fn link_direct() -> bool {
    LINKED.with_borrow(|partner| partner.is_some_and(table_link_direct))
}

/// Sends ping number `n` to the linked player, to time the round trip.
pub fn send_ping(n: u8) {
    if let Some(partner) = LINKED.with_borrow(|partner| *partner) {
        table_link_send(partner, &ping_packet(n, false));
    }
}

/// The answers to this player's pings that came in, by number. Pings from the other player are
/// answered straight away. Anything else (the match's first packets, ahead of it here) waits.
pub fn take_pongs() -> Vec<u8> {
    let Some(partner) = LINKED.with_borrow(|partner| *partner) else {
        return Vec::new();
    };
    let mut pongs = Vec::new();
    PACKETS.with_borrow_mut(|packets| {
        packets.retain(|(from, bytes)| match ping(bytes) {
            Some((n, false)) if *from == partner => {
                table_link_send(partner, &ping_packet(n, true));
                false
            }
            Some((n, true)) if *from == partner => {
                pongs.push(n);
                false
            }
            _ => true,
        });
    });
    pongs
}

/// Begins the match with the linked player, this player in seat `me`, inputs going
/// `input_delay` frames late.
pub fn start(me: usize, input_delay: usize) {
    let Some(partner) = LINKED.with_borrow(|partner| *partner) else {
        return;
    };
    let input_delay = input_delay.clamp(1, MOST_INPUT_DELAY);
    NET.set(None);
    let other = 1 - me;
    let builder = SessionBuilder::<Hockey>::new()
        .with_num_players(2)
        .and_then(|builder| builder.with_fps(60))
        .and_then(|builder| builder.add_player(PlayerType::Local, me))
        .and_then(|builder| builder.add_player(PlayerType::Remote(other), other))
        .map(|builder| {
            builder
                .with_input_delay(input_delay)
                .with_max_prediction_window(MAX_PREDICTION)
                .with_desync_detection_mode(DesyncDetection::On {
                    interval: DESYNC_INTERVAL,
                })
                // Players leave through the room, which ends the match, never through GGRS.
                .with_disconnect_timeout(Duration::from_secs(24 * 3600))
                .with_disconnect_notify_delay(Duration::from_secs(1))
        });
    let session = builder.and_then(|builder| {
        builder.start_p2p_session(Socket {
            partner,
            seat: other,
        })
    });
    match session {
        Ok(session) => NET.set(Some(Net {
            session,
            me,
            partner,
            time: 0.0,
            to_skip: 0,
            since_skip: 0,
            interrupted: false,
            input_delay,
            last_input: None,
            counts: Counts::default(),
        })),
        Err(error) => warn!("Air hockey: no match: {error}"),
    }
}

/// Ends the match, if there is one, and the link to the other player.
pub fn stop() {
    NET.set(None);
    if let Some(partner) = LINKED.take() {
        table_unlink(partner);
    }
    PACKETS.with_borrow_mut(Vec::clear);
}

/// How the match is getting on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No match.
    None,
    /// Getting in step with the other player.
    Connecting,
    Running,
    /// The other player's packets stopped coming: the game waits for them.
    Interrupted,
}

pub fn state() -> State {
    NET.with_borrow(|net| match net {
        None => State::None,
        Some(net) if net.session.current_state() != SessionState::Running => State::Connecting,
        Some(net) if net.interrupted => State::Interrupted,
        Some(_) => State::Running,
    })
}

/// Plays the frames `seconds` add up to on `rink`, this player's paddle going to `target`
/// (and player 1 pressing for a new game, with `new_game`), rolling back as GGRS says. Skips a
/// frame now and then when GGRS says this player is too far ahead. Says how many frames it
/// played, and whether it rolled back: then things the other player moved may have jumped.
pub fn play(rink: &mut Rink, target: Vec2, new_game: bool, seconds: f32) -> Played {
    let mut played = Played::default();
    NET.with_borrow_mut(|net| {
        let Some(net) = net else {
            return;
        };
        net.session.poll_remote_clients();
        for event in net.session.events() {
            match event {
                GgrsEvent::DesyncDetected { frame, .. } => {
                    net.counts.desyncs += 1;
                    warn!("Air hockey: the two rinks differ at frame {frame}");
                }
                GgrsEvent::NetworkInterrupted { .. } => net.interrupted = true,
                GgrsEvent::NetworkResumed { .. } => net.interrupted = false,
                GgrsEvent::WaitRecommendation { skip_frames } => net.to_skip += skip_frames,
                _ => {}
            }
        }
        if net.session.current_state() != SessionState::Running {
            net.time = 0.0;
            return;
        }
        net.time = (net.time + seconds).min(FRAME * MOST_FRAMES_AT_ONCE as f32);
        while net.time >= FRAME {
            net.time -= FRAME;
            net.since_skip += 1;
            if net.to_skip > 0 && net.since_skip >= SKIP_SPREAD {
                net.to_skip -= 1;
                net.since_skip = 0;
                net.counts.skipped += 1;
                continue;
            }
            let input = Input::new(target, net.last_input, new_game);
            if net.session.add_local_input(net.me, input).is_err() {
                return;
            }
            let requests = match net.session.advance_frame() {
                Ok(requests) => requests,
                // Too far ahead of the other player's inputs: wait for them.
                Err(GgrsError::PredictionThreshold) => {
                    net.counts.stalled += 1;
                    return;
                }
                Err(error) => {
                    warn!("Air hockey: {error}");
                    return;
                }
            };
            net.last_input = Some(input);
            net.counts.frames += 1;
            played.frames += 1;
            let mut loaded = false;
            for request in requests {
                match request {
                    GgrsRequest::SaveGameState { cell, frame } => {
                        let checksum = u128::from(rink.checksum());
                        cell.save(frame, Some(rink.clone()), Some(checksum));
                    }
                    GgrsRequest::LoadGameState { cell, .. } => {
                        if let Some(saved) = cell.load() {
                            *rink = saved;
                        }
                        loaded = true;
                        net.counts.rollbacks += 1;
                    }
                    GgrsRequest::AdvanceFrame { inputs } => {
                        if loaded {
                            net.counts.replayed += 1;
                        }
                        rink.play_frame([inputs[0].0, inputs[1].0]);
                    }
                }
            }
            played.rolled_back |= loaded;
        }
    });
    played
}

/// What `play` did.
#[derive(Clone, Copy, Default)]
pub struct Played {
    pub frames: u32,
    pub rolled_back: bool,
}

/// How the match has been getting on since the last call (meant about once a second).
pub fn stats() -> Option<Stats> {
    NET.with_borrow_mut(|net| {
        let net = net.as_mut()?;
        // Per second, but desyncs all match.
        let counts = std::mem::take(&mut net.counts);
        net.counts.desyncs = counts.desyncs;
        let other = 1 - net.me;
        Some(Stats {
            ping: net
                .session
                .network_stats(other)
                .ok()
                .map(|stats| stats.ping),
            input_delay: net.input_delay,
            direct: table_link_direct(net.partner),
            frames_ahead: net.session.frames_ahead(),
            frames: counts.frames,
            rollbacks: counts.rollbacks,
            replayed: counts.replayed,
            skipped: counts.skipped,
            stalled: counts.stalled,
            desyncs: counts.desyncs,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_go_through_begin_unchanged() {
        let settings = hockey::Settings {
            max_speed: 437.5,
            friction: 18.0,
            rail_restitution: 0.91,
            paddle_restitution: 0.87,
            bot_speed: 45.0,
        };
        let begin = Message::Begin {
            settings: settings_to_message(&settings),
            input_delay: 2,
        };
        let json = serde_json::to_string(&begin).unwrap();
        let Ok(Message::Begin {
            settings: back,
            input_delay: 2,
        }) = serde_json::from_str(&json)
        else {
            panic!("{json}");
        };
        assert_eq!(settings_from_message(back), settings);
    }

    #[test]
    fn ping_packets_are_told_apart_from_the_match_s() {
        assert_eq!(ping(&ping_packet(3, false)), Some((3, false)));
        assert_eq!(ping(&ping_packet(4, true)), Some((4, true)));
        assert_eq!(ping(b"vabp"), None);
        assert_eq!(ping(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), None);
    }

    #[test]
    fn slower_connections_get_more_input_delay_up_to_three_frames() {
        assert_eq!(input_delay_for(0.02), 1);
        assert_eq!(input_delay_for(0.07), 1);
        assert_eq!(input_delay_for(0.09), 2);
        assert_eq!(input_delay_for(0.15), 3);
        assert_eq!(input_delay_for(0.233), 3);
        assert_eq!(input_delay_for(2.0), 3);
    }
}
