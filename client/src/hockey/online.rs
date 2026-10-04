//! Two players at an air hockey table, through the room (seats.rs): the table has two seats,
//! "hockey:x,y". Both play on the same rink, seat 0's goal at its bottom and seat 1's at its
//! top; each sees it turned so that their own goal is at the bottom.
//!
//! Each player's machine runs the whole game, a fixed frame at a time (hockey's `play_frame`),
//! from both players' inputs: where their paddle should be. GGRS carries the inputs between
//! them, a frame behind (`INPUT_DELAY`), and where the other player's hasn't come yet it
//! guesses they kept still; when it comes and the guess was wrong, it goes back to the frame
//! before, plays the frames since again with it, and catches up, all before the next picture.
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
    Config, DesyncDetection, GgrsError, GgrsEvent, GgrsRequest, NonBlockingSocket, P2PSession,
    PlayerType, PredictRepeatLast, SessionBuilder, SessionState,
};
use hockey::{FRAME, Input, Rink};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Frames between a player moving their mouse and their paddle moving, on both rinks: the
/// fewer, the sooner it answers, and the more often the other rink guesses wrong and puts
/// things right.
const INPUT_DELAY: usize = 1;
/// How many frames ahead of the other player's last input a rink may run on guesses.
const MAX_PREDICTION: usize = 8;
/// Frames between checks that both rinks still match.
const DESYNC_INTERVAL: u32 = 60;
/// At most this many frames are played in one go, catching up after a slow moment.
const MOST_FRAMES_AT_ONCE: u32 = 4;
/// Frames between slowing down a frame, while ahead of the other player.
const SLOW_DOWN_EVERY: u32 = 10;

/// What one player at the table tells the other, besides inputs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A match begins, playing by these settings (player 1's): the puck's top speed and
    /// friction, how bouncy the rails and paddles are, and the bot's speed.
    Begin { settings: [f32; 5] },
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

impl Config for Hockey {
    type Input = Input;
    type InputPredictor = PredictRepeatLast;
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
            .filter(|(from, _)| *from == partner)
            .filter_map(|(_, bytes)| Some((self.seat, bincode::deserialize(&bytes).ok()?)))
            .collect()
    }
}

/// A match in progress.
struct Net {
    session: P2PSession<Hockey>,
    me: usize,
    partner: u32,
    /// Time not yet played as frames.
    time: f32,
    /// Frames until slowing down is allowed again.
    slowed: u32,
    /// The other player's packets stopped coming, for now.
    interrupted: bool,
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
}

/// Begins a match at `table`: this player in seat `me`, against `partner`.
pub fn start(table: &str, me: usize, partner: u32, my_id: u32) {
    stop();
    let link = format!("{table}/{}/{}", partner.min(my_id), partner.max(my_id));
    table_link(partner, me == 0, &link);
    let other = 1 - me;
    let builder = SessionBuilder::<Hockey>::new()
        .with_num_players(2)
        .and_then(|builder| builder.with_fps(60))
        .and_then(|builder| builder.add_player(PlayerType::Local, me))
        .and_then(|builder| builder.add_player(PlayerType::Remote(other), other))
        .map(|builder| {
            builder
                .with_input_delay(INPUT_DELAY)
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
            slowed: 0,
            interrupted: false,
        })),
        Err(error) => warn!("Air hockey: no match: {error}"),
    }
}

/// Ends the match, if there is one.
pub fn stop() {
    if let Some(net) = NET.take() {
        table_unlink(net.partner);
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

/// Plays the frames `seconds` add up to on `rink`, this player's paddle going to `input`'s,
/// rolling back as GGRS says. Slows down a frame now and then while ahead of the other player,
/// so neither gets too far ahead.
pub fn play(rink: &mut Rink, input: Input, seconds: f32) {
    NET.with_borrow_mut(|net| {
        let Some(net) = net else {
            return;
        };
        net.session.poll_remote_clients();
        for event in net.session.events() {
            match event {
                GgrsEvent::DesyncDetected { frame, .. } => {
                    warn!("Air hockey: the two rinks differ at frame {frame}");
                }
                GgrsEvent::NetworkInterrupted { .. } => net.interrupted = true,
                GgrsEvent::NetworkResumed { .. } => net.interrupted = false,
                _ => {}
            }
        }
        if net.session.current_state() != SessionState::Running {
            net.time = 0.0;
            return;
        }
        net.time = (net.time + seconds).min(FRAME * MOST_FRAMES_AT_ONCE as f32);
        net.slowed = net.slowed.saturating_sub(1);
        while net.time >= FRAME {
            net.time -= FRAME;
            if net.session.frames_ahead() > 0 && net.slowed == 0 {
                net.slowed = SLOW_DOWN_EVERY;
                continue;
            }
            if net.session.add_local_input(net.me, input).is_err() {
                return;
            }
            let requests = match net.session.advance_frame() {
                Ok(requests) => requests,
                // Too far ahead of the other player's inputs: wait for them.
                Err(GgrsError::PredictionThreshold) => return,
                Err(error) => {
                    warn!("Air hockey: {error}");
                    return;
                }
            };
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
                    }
                    GgrsRequest::AdvanceFrame { inputs } => {
                        rink.play_frame([inputs[0].0, inputs[1].0]);
                    }
                }
            }
        }
    });
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
        };
        let json = serde_json::to_string(&begin).unwrap();
        let Message::Begin { settings: back } = serde_json::from_str(&json).unwrap();
        assert_eq!(settings_from_message(back), settings);
    }
}
