//! Playing darts: the board seen straight on, drawn pixel by pixel into a small image that fills
//! the screen at a whole-number zoom, as the tables are. The mouse aims a crosshair, which drifts
//! about where it's aimed as a hand does (the darts crate); the page locks the pointer (a click
//! on the board does), so a flick can't run off the window, and Esc lets it go, as a second Esc
//! leaves. Pressing the button holds the aim and steadies the hand, for a moment, before it
//! starts to shake; a flick up as the button is let go throws. The right speed of flick sends the
//! dart where the crosshair was, give or take a little; slower drops it low, faster sends it
//! high, and one leaning sends it to the side. On a touch screen the finger aims, a little below
//! the crosshair, and flicks up as it lifts.
//!
//! A game of 301: three darts a turn, each taking what it scores off the player's 301, and the
//! first to exactly zero wins; going below zero is a bust, and the turn's darts don't count.
//! Alone at the board, the player throws for both sides; when someone is at the other seat, they
//! play each other (online.rs), the first seat throwing red. Esc goes back to the bar, and New
//! game in `/settings` starts over.
//!
//! Anyone can watch the game at a board (F in the bar): they see it as the players do, with no
//! dart of their own. The lowest seat gives each watcher the game as it is, between darts, and
//! from then on whatever the players send each other (the thrower's hand, each dart) goes to the
//! watchers too, so their board scores each dart as the players' do.

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use bevy::window::CursorOptions;
use darts::{DARTS_PER_TURN, DOUBLE_OUTER, Flick, Hit, NUMBERS, START};
use wasm_bindgen::prelude::*;

use super::online::Message;
use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::pixels::Pixels;
use crate::pointer_lock;
use crate::seats;
use crate::settings::{NewDartsGame, Settings};
use crate::touch::{self, Touch, TouchButton};

/// The image the board is drawn into, the board's middle in it, and how many of its pixels to a
/// millimetre of the board.
const CANVAS: UVec2 = UVec2::new(320, 180);
const MIDDLE: Vec2 = Vec2::new(160.0, 90.0);
const SCALE: f32 = 0.38;
/// The ring round the scoring area the numbers are on, and the wood behind it all, out to
/// these radii (millimetres).
const NUMBER_RING: f32 = 200.0;
const BACKING: f32 = 207.0;
/// How far out the numbers sit (millimetres).
const NUMBERS_AT: f32 = 186.0;
/// On a touch screen, the crosshair is this far above the finger (canvas pixels), so the finger
/// doesn't hide it.
const FINGER_BELOW: f32 = 24.0;
/// How long a dart takes to reach the board, and how long a turn's darts stay in it once the
/// turn is over (seconds; a click moves on).
const FLIGHT: f32 = 0.18;
const TURN_SHOWN_FOR: f32 = 1.8;
/// Where a thrown dart comes from: the bottom right, by the throwing hand (canvas pixels).
const HAND: Vec2 = Vec2::new(250.0, 190.0);
/// How far from the board's middle the crosshair can go (millimetres).
const AIM_REACH: f32 = 230.0;
/// How long "flick up to throw" shows after a let-go that didn't throw (seconds).
const NO_FLICK_SHOWN_FOR: f32 = 2.5;
/// Arrived with someone already at the board, how long to wait for their game before player 1
/// starts a new one (neither had one), in seconds.
const NO_GAME_COMING: f32 = 1.5;
/// How often the thrower's hand goes to the other player while they aim, in seconds.
const AIM_SEND_EVERY: f32 = 1.0 / 15.0;
/// An Esc this soon after the pointer was let go is the one that let it go (the browser takes
/// it, and may pass it on too): it doesn't also leave.
const ESC_AFTER_UNLOCK: f32 = 0.3;

/// Players' names, by their darts' colour, while one player throws for both.
const NAMES: [&str; 2] = ["Red", "Blue"];
/// The texts: the players' panels, one above the other, and this turn's darts, left of the
/// board, and the status line, right of it (from their left edges, how wide, and their tops, in
/// canvas pixels).
const PANELS: [(f32, f32, f32); 2] = [(4.0, 74.0, 22.0), (4.0, 74.0, 70.0)];
const TURN: (f32, f32, f32) = (4.0, 74.0, 124.0);
const STATUS: (f32, f32, f32) = (244.0, 72.0, 36.0);
/// How far under a panel's top its darts left are drawn (canvas pixels).
const DARTS_LEFT_BELOW: f32 = 34.0;
/// Text heights, in canvas pixels, and the smallest they get, in logical pixels.
const NAME_SIZE: f32 = 6.0;
const TURN_SIZE: f32 = 4.5;
const STATUS_SIZE: f32 = 3.5;
const SMALLEST_TEXT: f32 = 8.0;

const BLACK: [u8; 4] = [26, 24, 24, 255];
const CREAM: [u8; 4] = [228, 214, 178, 255];
const RED: [u8; 4] = [200, 42, 38, 255];
const GREEN: [u8; 4] = [34, 140, 66, 255];
const EDGE: [u8; 4] = [16, 14, 14, 255];
const RIM: [u8; 4] = [150, 150, 158, 255];
const NUMBER: [u8; 4] = [236, 236, 236, 255];
const WOOD: [u8; 4] = [92, 58, 32, 255];
const STEEL: [u8; 4] = [200, 204, 214, 255];
const STEEL_DARK: [u8; 4] = [90, 92, 102, 255];
const OUTLINE: [u8; 4] = [10, 10, 14, 255];
/// Each player's darts' flights.
const FLIGHTS: [[u8; 4]; 2] = [[226, 58, 52, 255], [60, 110, 226, 255]];
/// The crosshair: drifting, held steady, and shaking.
const LOOSE: [u8; 4] = [250, 250, 250, 255];
const STEADY: [u8; 4] = [130, 236, 130, 255];
const SHAKING: [u8; 4] = [246, 130, 104, 255];
/// Behind the texts (canvas pixels, corner to corner).
const BACKDROP: [u8; 4] = [14, 12, 18, 255];
const BACKDROPS: [(IVec2, IVec2); 2] = [
    (IVec2::new(2, 16), IVec2::new(80, 162)),
    (IVec2::new(242, 30), IVec2::new(318, 128)),
];

// Defined by the browser.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Math)]
    fn random() -> f64;
}

/// The dartboard the player is at, as the room names it (online::table_id, seats.rs), and
/// whether they watch the game there rather than play. Set before switching to `Mode::Darts`.
#[derive(Resource)]
pub struct AtBoard {
    pub id: String,
    pub watching: bool,
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Darts), show_board)
            .add_systems(
                Update,
                (
                    fit_canvas,
                    sync,
                    play,
                    send_aim,
                    draw,
                    show_text,
                    place_text,
                    leave.run_if(chat_closed),
                )
                    .chain()
                    .run_if(in_state(Mode::Darts)),
            )
            .add_systems(Update, new_game)
            .add_systems(OnExit(Mode::Darts), hide_board);
        #[cfg(feature = "test-hooks")]
        app.add_systems(Update, test_hooks.after(play).run_if(in_state(Mode::Darts)));
    }
}

/// Where the game is at.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    /// The player whose turn it is aims their next dart.
    Aiming,
    /// A dart on its way to `to` (on the board, millimetres), `since` seconds ago.
    Flying { since: f32, to: Vec2, hit: Hit },
    /// The turn is over (or the game won), its darts still in the board.
    TurnOver { since: f32 },
}

/// How the player aims.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    /// Their finger aims, and flicks.
    Finger,
    /// The mouse, the pointer locked.
    Locked,
    /// The pointer isn't locked (yet, or after an Esc): the game waits for a click.
    Paused,
    /// The browser won't lock the pointer: the crosshair follows it, as best it can.
    Unlocked,
}

impl Control {
    fn now(touch: &Touch) -> Self {
        if touch.is_on() {
            Control::Finger
        } else if pointer_lock::locked() {
            Control::Locked
        } else if pointer_lock::failed() {
            Control::Unlocked
        } else {
            Control::Paused
        }
    }
}

/// The game, kept between visits to the board.
#[derive(Resource)]
struct Game {
    rules: darts::Game,
    phase: Phase,
    /// Where this turn's darts are in the board (millimetres).
    stuck: Vec<Vec2>,
    /// Where the player aims, and where the hand has the dart, swaying about it (millimetres).
    aim: Vec2,
    hand: Vec2,
    /// How long the throw has been held, while it is, and the hand's moves since: the flick.
    held: Option<f32>,
    flick: Option<Flick>,
    /// Where the player's hand is (millimetres): where they aim, or, while they hold a throw,
    /// where the flick has taken it.
    pointer: Vec2,
    /// How long ago a let-go didn't throw, for a while after.
    no_flick: Option<f32>,
    /// How fast the last flick was (millimetres a second), for browser tests.
    last_flick: Vec2,
    /// The pointer was down last frame: only a fresh press holds a throw, or moves the game on.
    was_pressed: bool,
    /// The pointer was locked last frame, and when it was last let go (in seconds of `Time`).
    was_locked: bool,
    unlocked_at: Option<f32>,
    /// The board, as the room names it, while the player is at it.
    board_id: Option<String>,
    /// The other player at the board, when there is one: the game is between them.
    opponent: Option<Opponent>,
    /// Just arrived at a board with someone at it, and waiting for the game from them (or, both
    /// new to it, for player 1 to start one), for `waiting_since` seconds.
    waiting_for_start: bool,
    waiting_since: f32,
    /// This player's seat in a game against someone, and both players' names: kept while the
    /// other seat is empty, so the game waits for them.
    match_seat: Option<(usize, [String; 2])>,
    /// The room last had this player at the board on their own: whoever arrives next gets the
    /// game from them.
    alone_here: bool,
    /// The game is owed to these players, who just arrived or started watching: it goes between
    /// darts.
    owe_game: Vec<u32>,
    /// The watchers given the game so far (or owed it), to spot new ones.
    streamed_to: Vec<u32>,
    /// Watching the game at the board, not playing: its players send what they do.
    watching: bool,
    /// Who is at the board, by seat, while watching.
    watched_names: [String; 2],
    /// Where the other player's hand has the dart, and whether they hold the throw, on their
    /// turn.
    their_hand: Option<(Vec2, bool)>,
    /// The hand as last sent to the other player, and how long ago.
    sent: (Option<(Vec2, bool)>, f32),
}

/// Who the player is playing against.
struct Opponent {
    /// The player's own seat (0 for player 1), and the other player's id in the room.
    me: usize,
    id: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            rules: darts::Game::new(0),
            phase: Phase::Aiming,
            stuck: Vec::new(),
            aim: Vec2::ZERO,
            hand: Vec2::ZERO,
            held: None,
            flick: None,
            pointer: Vec2::ZERO,
            no_flick: None,
            last_flick: Vec2::ZERO,
            was_pressed: true,
            was_locked: false,
            unlocked_at: None,
            board_id: None,
            opponent: None,
            waiting_for_start: false,
            waiting_since: 0.0,
            match_seat: None,
            alone_here: false,
            owe_game: Vec::new(),
            streamed_to: Vec::new(),
            watching: false,
            watched_names: NAMES.map(String::from),
            their_hand: None,
            sent: (None, 0.0),
        }
    }
}

impl Game {
    /// A new game, `first` throwing first.
    fn begin(&mut self, first: usize) {
        self.rules = darts::Game::new(first);
        self.phase = Phase::Aiming;
        self.stuck.clear();
        self.held = None;
        self.waiting_for_start = false;
        self.their_hand = None;
    }

    /// A new game, `first` throwing first. Against someone, only player 1 starts games, and
    /// tells player 2 (and the watchers). Alone at a game whose other seat is empty, it ends:
    /// the player takes both sides again. A watcher starts nothing.
    fn start_over(&mut self, first: usize) {
        if !self.may_start_over() {
            return;
        }
        if self.opponent.is_none() {
            self.match_seat = None;
        }
        self.begin(first);
        self.tell(&Message::Start { first });
    }

    /// Whether this player may start the next game: alone, or as player 1 against someone.
    fn may_start_over(&self) -> bool {
        !self.watching
            && self
                .opponent
                .as_ref()
                .is_none_or(|opponent| opponent.me == 0)
    }

    fn send(&self, to: u32, message: &Message) {
        if let Some(board) = &self.board_id {
            seats::send(board, to, message);
        }
    }

    /// Tells the other player, and everyone watching the board, `message`.
    fn tell(&self, message: &Message) {
        let Some(board) = &self.board_id else {
            return;
        };
        if let Some(opponent) = &self.opponent {
            seats::send(board, opponent.id, message);
        }
        if seats::watching(board) > 0 {
            seats::send_watchers(board, message);
        }
    }

    /// Whether anyone follows what this player does: an opponent, or watchers.
    fn has_audience(&self) -> bool {
        self.opponent.is_some()
            || self
                .board_id
                .as_deref()
                .is_some_and(|board| seats::watching(board) > 0)
    }

    /// Whether this player throws next: always when alone, on their turn in a game against
    /// someone (who may have left their seat: then it waits for them). Never watching.
    fn my_turn(&self) -> bool {
        !self.watching
            && !self.waiting_for_start
            && self
                .match_seat
                .as_ref()
                .is_none_or(|(me, _)| self.rules.turn == *me)
    }

    /// Both players' names: their names in the room in a game against someone, or watched;
    /// their darts' colours when one player throws for both.
    fn names(&self) -> [String; 2] {
        match &self.match_seat {
            Some((_, names)) => names.clone(),
            None if self.watching => self.watched_names.clone(),
            None => NAMES.map(String::from),
        }
    }

    /// Follows what another player at the board did: the game they hand over, and on their turn
    /// their hand and each dart.
    fn follow(&mut self, message: Message) {
        match message {
            Message::Start { first } => self.begin(first),
            Message::Sync { rules, stuck } => {
                self.rules = rules;
                self.stuck = stuck;
                self.phase = if self.rules.turn_over() {
                    Phase::TurnOver { since: 0.0 }
                } else {
                    Phase::Aiming
                };
                self.waiting_for_start = false;
                self.their_hand = None;
            }
            // Nothing else counts until the game is here.
            _ if self.waiting_for_start => {}
            Message::Aim { at, held } => {
                self.catch_up();
                if !self.my_turn() {
                    self.their_hand = Some((at, held));
                }
            }
            Message::Throw { at } => {
                self.catch_up();
                if !self.my_turn() && self.rules.win.is_none() {
                    self.fly(at);
                    self.their_hand = None;
                }
            }
        }
    }

    /// Finishes whatever this board is still showing (a dart in the air, the end of a turn), for
    /// the other player's next dart: they've moved on to it.
    fn catch_up(&mut self) {
        loop {
            match self.phase {
                Phase::Flying { to, hit, .. } => self.landed(to, hit),
                Phase::TurnOver { .. } if self.rules.win.is_none() => self.next_turn(),
                Phase::TurnOver { .. } | Phase::Aiming => return,
            }
        }
    }

    /// This player throws a dart that lands at `at`, telling the other player and the watchers.
    fn throw(&mut self, at: Vec2) {
        self.tell(&Message::Throw { at });
        self.fly(at);
    }

    /// A dart that lands at `at` (millimetres): it flies there, and scores once it's in.
    fn fly(&mut self, at: Vec2) {
        self.held = None;
        self.flick = None;
        self.phase = Phase::Flying {
            since: 0.0,
            to: at,
            hit: darts::hit(at),
        };
    }

    /// The dart has landed: it's in the board, and scores.
    fn landed(&mut self, at: Vec2, hit: Hit) {
        self.stuck.push(at);
        self.rules.throw(hit);
        self.phase = if self.rules.turn_over() {
            Phase::TurnOver { since: 0.0 }
        } else {
            Phase::Aiming
        };
    }

    /// The turn's darts come out of the board, and the next turn begins.
    fn next_turn(&mut self) {
        self.rules.next_turn();
        self.stuck.clear();
        self.phase = Phase::Aiming;
    }
}

/// From the board's millimetres (y up) to the canvas's pixels (y down), and back.
fn to_canvas(at: Vec2) -> Vec2 {
    MIDDLE + Vec2::new(at.x, -at.y) * SCALE
}

fn to_board(at: Vec2) -> Vec2 {
    let off = (at - MIDDLE) / SCALE;
    Vec2::new(off.x, -off.y)
}

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct Canvas;

/// Text placed over the canvas: from `left`, `width` across, from `top` down (canvas pixels),
/// `size` tall.
#[derive(Component)]
struct OnCanvas {
    left: f32,
    width: f32,
    top: f32,
    size: f32,
}

/// What a text says.
#[derive(Component, Clone, Copy, PartialEq)]
enum Says {
    Player(usize),
    Turn,
    Status,
}

fn show_board(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Option<ResMut<Game>>,
    at: Option<Res<AtBoard>>,
    touch: Res<Touch>,
) {
    match game {
        // Picked up where it was left, but not with a throw held.
        Some(mut game) => {
            game.held = None;
            game.flick = None;
            game.was_pressed = true;
        }
        None => commands.init_resource::<Game>(),
    }
    // A watcher's pointer is their own.
    if !touch.is_on() && !at.is_some_and(|at| at.watching) {
        pointer_lock::wanted(true);
    }
    let image = Image::new_fill(
        Extent3d {
            width: CANVAS.x,
            height: CANVAS.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let text = |says: Says, (left, width, top): (f32, f32, f32), size: f32| {
        (
            says,
            OnCanvas {
                left,
                width,
                top,
                size,
            },
            Text::new(""),
            TextFont::default(),
            TextColor(Color::WHITE),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        )
    };
    let mut overlay = commands.spawn((
        Overlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // The bar stays in sight, dimmed.
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        GlobalZIndex(1),
        children![
            (Canvas, ImageNode::new(images.add(image)), Node::default()),
            text(Says::Player(0), PANELS[0], NAME_SIZE),
            text(Says::Player(1), PANELS[1], NAME_SIZE),
            text(Says::Turn, TURN, TURN_SIZE),
            text(Says::Status, STATUS, STATUS_SIZE),
        ],
    ));
    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), as at the tables.
        overlay.with_child((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(80.0),
                ..default()
            },
            children![touch::button(TouchButton::Leave, "Leave", 64.0, 36.0)],
        ));
    }
}

/// As big as fits, at a whole number of the screen's own pixels per canvas pixel.
fn fit_canvas(window: Single<&Window>, mut canvas: Single<&mut Node, With<Canvas>>) {
    let fits = Vec2::new(
        window.physical_width() as f32,
        window.physical_height() as f32,
    ) / CANVAS.as_vec2();
    let zoom = fits.min_element().floor().max(1.0);
    let size = CANVAS.as_vec2() * zoom / window.scale_factor();
    let (width, height) = (Val::Px(size.x), Val::Px(size.y));
    if canvas.width != width || canvas.height != height {
        canvas.width = width;
        canvas.height = height;
    }
}

/// Aims where the pointer is, the hand swaying about it; holds, and throws with a flick; and
/// moves the game on: the dart landing, the next turn, a new game.
#[allow(clippy::too_many_arguments)]
fn play(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    touch: Res<Touch>,
    chat: Res<Chat>,
    help: Res<Help>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut game: ResMut<Game>,
    mut cursor: Query<&mut CursorOptions>,
) {
    let game = &mut *game;
    let seconds = time.delta_secs();
    let now = time.elapsed_secs();
    let hand = settings.darts();
    let busy = chat.is_open() || help.is_open() || settings.is_open();
    let control = Control::now(&touch);
    let just_locked = control == Control::Locked && !game.was_locked;
    if game.was_locked && control != Control::Locked {
        game.unlocked_at = Some(now);
    }
    game.was_locked = control == Control::Locked;
    // Without the lock, the crosshair stands in for the pointer, except while a panel is up.
    let hidden = control == Control::Unlocked && !busy;
    for mut options in &mut cursor {
        if options.visible == hidden {
            options.visible = !hidden;
        }
    }
    // A press throws only with the pointer locked (the click that locks it doesn't), or on a
    // screen without the lock.
    let can_throw = !busy
        && match control {
            Control::Locked => !just_locked,
            Control::Paused => false,
            Control::Finger | Control::Unlocked => true,
        };
    let pressed = mouse.pressed(MouseButton::Left) || touch.finger().is_some();
    // As if held all along while a panel is up, so the click that closes it does nothing here.
    let fresh_press = pressed && !game.was_pressed && can_throw;
    game.was_pressed = pressed || !can_throw;

    // Where the hand is: moved by the mouse while the pointer is locked, under the pointer (or
    // a little above the finger) otherwise.
    let (node, transform) = *canvas;
    // Physical pixels per canvas pixel; until the canvas has been laid out it has no size.
    let zoom = node.size().x / CANVAS.x as f32;
    match control {
        _ if busy || zoom <= 0.0 => {}
        Control::Locked if just_locked => game.pointer = game.aim,
        Control::Locked => {
            game.pointer += Vec2::new(motion.delta.x, -motion.delta.y) / zoom / SCALE;
        }
        Control::Finger | Control::Unlocked => {
            let finger = touch.finger();
            let lifted = if finger.is_some() { FINGER_BELOW } else { 0.0 };
            if let Some(at) = finger.or(window.cursor_position()) {
                let corner = transform.translation - node.size() / 2.0;
                let on_canvas = (at * window.scale_factor() - corner) / zoom;
                let pointed = to_board(on_canvas - Vec2::Y * lifted);
                if pointed.is_finite() {
                    game.pointer = pointed;
                }
            }
        }
        Control::Paused => {}
    }
    if game.held.is_none() {
        game.pointer = game.pointer.clamp_length_max(AIM_REACH);
        game.aim = game.pointer;
    }
    if let Some(since) = &mut game.no_flick {
        *since += seconds;
    }

    match game.phase {
        Phase::Aiming if game.rules.win.is_some() => {}
        Phase::Aiming if !game.my_turn() => {
            // The other player's throw: their hand moves as they say (sync).
            game.held = None;
            game.flick = None;
        }
        Phase::Aiming => {
            if busy {
                game.held = None;
                game.flick = None;
            } else if fresh_press {
                // The aim holds where it is; the hand's moves from here are the flick.
                game.held = Some(0.0);
                game.flick = Some(Flick::new(now, game.pointer));
            } else if pressed && let (Some(held), Some(flick)) = (&mut game.held, &mut game.flick) {
                *held += seconds;
                flick.moved(now, game.pointer);
            }
            game.hand = game.aim + hand.offset(now, game.held.unwrap_or(0.0));
            if game.held.is_some() && !pressed && !busy {
                let velocity = game
                    .flick
                    .as_ref()
                    .map_or(Vec2::ZERO, |flick| flick.velocity(now));
                game.last_flick = velocity;
                if hand.throws(velocity) {
                    // Off it goes: where the hand had it, off by however the flick was, and a
                    // little at random.
                    let scatter = hand.scatter(random() as f32, random() as f32);
                    let at = game.hand + hand.flick_error(velocity) + scatter;
                    game.throw(at);
                    game.no_flick = None;
                } else {
                    game.held = None;
                    game.flick = None;
                    game.no_flick = Some(0.0);
                }
                // The hand comes back to where it aimed.
                game.pointer = game.aim;
            }
        }
        Phase::Flying { since, to, hit } => {
            let since = since + seconds;
            game.phase = Phase::Flying { since, to, hit };
            if since >= FLIGHT {
                game.landed(to, hit);
            }
        }
        Phase::TurnOver { since } => {
            let since = since + seconds;
            game.phase = Phase::TurnOver { since };
            match game.rules.win {
                // A new game, the other player throwing first, if this player may start one.
                Some(_) if fresh_press && game.may_start_over() => {
                    let first = 1 - game.rules.first;
                    game.start_over(first);
                }
                Some(_) => {}
                None if since >= TURN_SHOWN_FOR || fresh_press => game.next_turn(),
                None => {}
            }
        }
    }
}

/// Sits at the board in the room, starts a game when someone arrives at the other seat (player 1
/// starts it, or the one already there hands theirs over), goes back to both sides when they
/// leave, and follows what they do on their turn. Watching, follows whatever the players do.
fn sync(at: Option<Res<AtBoard>>, time: Res<Time>, mut game: ResMut<Game>) {
    let Some(at) = at else {
        return;
    };
    let game = &mut *game;
    if game.board_id.as_deref() != Some(at.id.as_str()) {
        game.board_id = Some(at.id.clone());
        game.watching = at.watching;
        if at.watching {
            // The game comes from whoever plays there, between darts; until then, nothing.
            game.waiting_for_start = true;
            game.waiting_since = 0.0;
            game.held = None;
            game.flick = None;
            game.their_hand = None;
            seats::watch(&at.id);
        } else {
            seats::sit(&at.id);
        }
    }

    let seats = seats::seats_at(&at.id);
    if game.watching {
        watch(game, &at.id, seats);
        return;
    }
    let now = seats.as_ref().and_then(|seats| {
        let me = seats.mine()?;
        let (_, id) = seats.opponent()?;
        Some((me, id, seats.names()))
    });
    let was_alone = game.alone_here;
    game.alone_here = seats
        .as_ref()
        .is_some_and(|seats| seats.mine().is_some() && seats.opponent().is_none());
    match (&game.opponent, now) {
        (Some(opponent), Some((me, id, names))) if opponent.id == id && opponent.me == me => {
            game.match_seat = Some((me, names));
        }
        (_, Some((me, id, names))) => {
            game.opponent = Some(Opponent { me, id });
            game.match_seat = Some((me, names));
            game.held = None;
            game.flick = None;
            if was_alone {
                // They arrived with this player already here: they get the game as it is.
                game.owe_game.push(id);
            } else {
                // Arrived with them already here: the game comes from them.
                game.waiting_for_start = true;
                game.waiting_since = 0.0;
            }
        }
        (Some(opponent), None) => {
            // They left: the game waits for them as it is, their seat empty.
            let left = opponent.id;
            game.opponent = None;
            game.waiting_for_start = false;
            game.owe_game.retain(|id| *id != left);
            game.their_hand = None;
        }
        (None, None) => {}
    }

    // Both new to the board, nobody had a game to give: player 1 starts one.
    if game.waiting_for_start {
        game.waiting_since += time.delta_secs();
        if let Some(opponent) = game.opponent.as_ref().filter(|opponent| opponent.me == 0)
            && game.waiting_since > NO_GAME_COMING
        {
            let id = opponent.id;
            game.begin(0);
            game.send(id, &Message::Start { first: 0 });
        }
    }
    // The lowest seat gives each new watcher the game, as it does a player who arrives.
    let watchers = seats::watchers_of(&at.id);
    if seats.as_ref().is_some_and(seats::Seats::am_first) && !game.waiting_for_start {
        for &watcher in &watchers {
            if !game.streamed_to.contains(&watcher) {
                game.streamed_to.push(watcher);
                game.owe_game.push(watcher);
            }
        }
    }
    game.streamed_to
        .retain(|watcher| watchers.contains(watcher));
    // The game owed to those who arrived or started watching goes between darts, and the hand
    // as it is right after, moved or not.
    if !matches!(game.phase, Phase::Flying { .. }) && !game.owe_game.is_empty() {
        for id in std::mem::take(&mut game.owe_game) {
            let whole = Message::Sync {
                rules: game.rules.clone(),
                stuck: game.stuck.clone(),
            };
            game.send(id, &whole);
        }
        game.sent = (None, AIM_SEND_EVERY);
    }

    let messages = seats::take_messages::<Message>(&at.id);
    let Some(id) = game.opponent.as_ref().map(|opponent| opponent.id) else {
        return;
    };
    for (_, message) in messages.into_iter().filter(|(from, _)| *from == id) {
        game.follow(message);
    }
}

/// Watching: follows what the players at the board do, and who they are.
fn watch(game: &mut Game, board: &str, seats: Option<seats::Seats>) {
    let players: Vec<u32> = match &seats {
        Some(seats) => {
            game.watched_names = seats.names();
            seats.ids().collect()
        }
        None => Vec::new(),
    };
    for (_, message) in seats::take_messages::<Message>(board)
        .into_iter()
        .filter(|(from, _)| players.contains(from))
    {
        game.follow(message);
    }
}

/// On this player's turn, with someone to see it: where their hand has the dart goes to the
/// other player and the watchers as it moves, a few times a second.
fn send_aim(time: Res<Time>, mut game: ResMut<Game>) {
    if !game.has_audience()
        || !game.my_turn()
        || game.phase != Phase::Aiming
        || game.rules.win.is_some()
    {
        return;
    }
    game.sent.1 += time.delta_secs();
    let now = (game.hand, game.held.is_some());
    if game.sent.0 != Some(now) && game.sent.1 >= AIM_SEND_EVERY {
        game.sent = (Some(now), 0.0);
        let (at, held) = now;
        game.tell(&Message::Aim { at, held });
    }
}

fn draw(
    game: Res<Game>,
    settings: Res<Settings>,
    canvas: Single<&ImageNode, With<Canvas>>,
    mut images: ResMut<Assets<Image>>,
    mut board: Local<Option<Pixels>>,
) {
    let Some(mut image) = images.get_mut(&canvas.image) else {
        return;
    };
    let mut pixels = board.get_or_insert_with(draw_board).clone();
    let turn = game.rules.turn;
    for at in &game.stuck {
        draw_dart(&mut pixels, to_canvas(*at), turn, 1.0);
    }
    match game.phase {
        Phase::Flying { since, to, .. } => {
            // From the hand to the board, getting smaller as it goes away.
            let done = (since / FLIGHT).clamp(0.0, 1.0);
            let at = HAND.lerp(to_canvas(to), 1.0 - (1.0 - done) * (1.0 - done));
            draw_dart(&mut pixels, at, turn, 3.0 - 2.0 * done);
        }
        Phase::Aiming if game.rules.win.is_none() && !game.my_turn() => {
            if let Some((at, held)) = game.their_hand {
                let color = if held { STEADY } else { LOOSE };
                draw_crosshair(&mut pixels, to_canvas(at), color);
            }
        }
        Phase::Aiming if game.rules.win.is_none() => {
            let sway = settings.darts();
            let color = match game.held {
                Some(held) if held > sway.steady_for => SHAKING,
                Some(_) => STEADY,
                None => LOOSE,
            };
            draw_crosshair(&mut pixels, to_canvas(game.hand), color);
        }
        _ => {}
    }
    // Each player's darts left this turn, under their name.
    for (player, (left_edge, width, top)) in PANELS.into_iter().enumerate() {
        let left = if player == turn && game.rules.win.is_none() {
            DARTS_PER_TURN - game.rules.darts.len()
        } else {
            0
        };
        let first = Vec2::new(left_edge + width / 2.0 - 8.0, top + DARTS_LEFT_BELOW);
        for slot in 0..left {
            draw_dart(
                &mut pixels,
                first + Vec2::X * 8.0 * slot as f32,
                player,
                1.0,
            );
        }
    }
    image.data = Some(pixels.into_bytes());
}

/// The backdrops behind the texts, the wood behind the board, and the board: its slices in
/// black and cream, the doubles and trebles in red and green, the bulls, and the numbers round
/// the edge.
fn draw_board() -> Pixels {
    let mut pixels = Pixels::new(CANVAS);
    for (from, to) in BACKDROPS {
        pixels.rect(from, to, BACKDROP);
    }
    let reach = (BACKING * SCALE).ceil() as i32 + 1;
    let middle = MIDDLE.as_ivec2();
    for y in middle.y - reach..middle.y + reach {
        for x in middle.x - reach..middle.x + reach {
            let at = to_board(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
            let distance = at.length();
            let color = if distance > BACKING {
                continue;
            } else if distance > NUMBER_RING {
                WOOD
            } else if distance > DOUBLE_OUTER + 4.0 {
                EDGE
            } else if distance > DOUBLE_OUTER {
                RIM
            } else {
                // The slices alternate, the 20 black with red rings.
                let dark =
                    |number: u32| NUMBERS.iter().position(|n| *n == number).unwrap_or(0) % 2 == 0;
                match darts::hit(at) {
                    Hit::Bull => RED,
                    Hit::OuterBull => GREEN,
                    Hit::Single(number) if dark(number) => BLACK,
                    Hit::Single(_) => CREAM,
                    Hit::Double(number) | Hit::Treble(number) if dark(number) => RED,
                    Hit::Double(_) | Hit::Treble(_) => GREEN,
                    Hit::Miss => EDGE,
                }
            };
            pixels.set(x, y, color);
        }
    }
    for (slice, number) in NUMBERS.iter().enumerate() {
        let angle = (slice as f32 * 18.0).to_radians();
        let at = to_canvas(Vec2::new(angle.sin(), angle.cos()) * NUMBERS_AT);
        draw_number(&mut pixels, *number, at, NUMBER);
    }
    pixels
}

/// A number in a 3 by 5 pixel font, its middle at `at`.
fn draw_number(pixels: &mut Pixels, number: u32, at: Vec2, color: [u8; 4]) {
    const DIGITS: [[u8; 5]; 10] = [
        [0b111, 0b101, 0b101, 0b101, 0b111],
        [0b010, 0b110, 0b010, 0b010, 0b111],
        [0b111, 0b001, 0b111, 0b100, 0b111],
        [0b111, 0b001, 0b011, 0b001, 0b111],
        [0b101, 0b101, 0b111, 0b001, 0b001],
        [0b111, 0b100, 0b111, 0b001, 0b111],
        [0b111, 0b100, 0b111, 0b101, 0b111],
        [0b111, 0b001, 0b010, 0b010, 0b010],
        [0b111, 0b101, 0b111, 0b101, 0b111],
        [0b111, 0b101, 0b111, 0b001, 0b111],
    ];
    let text = number.to_string();
    let width = text.len() as i32 * 4 - 1;
    let corner = at.round().as_ivec2() - IVec2::new(width / 2, 2);
    for (place, digit) in text.bytes().enumerate() {
        let rows = DIGITS[usize::from(digit - b'0')];
        for (y, bits) in rows.iter().enumerate() {
            for x in 0..3 {
                if bits & (0b100 >> x) != 0 {
                    pixels.set(corner.x + place as i32 * 4 + x, corner.y + y as i32, color);
                }
            }
        }
    }
}

/// A dart with its point at `at`, its barrel and flight out to the upper right, `size` times
/// as big as one in the board (a dart in the air is nearer).
fn draw_dart(pixels: &mut Pixels, at: Vec2, player: usize, size: f32) {
    let way = Vec2::new(1.0, -1.0).normalize();
    let barrel = at + way * 4.0 * size;
    let flight = at + way * 7.0 * size;
    pixels.thick_line(at, barrel, 0.5 * size, STEEL_DARK);
    pixels.thick_line(at + way * size, barrel, 0.3 * size, STEEL);
    pixels.thick_line(barrel, flight, 1.0 * size, FLIGHTS[player]);
    pixels.set(at.x as i32, at.y as i32, OUTLINE);
}

/// A ring with a dot in it, outlined so it shows on any colour of the board.
fn draw_crosshair(pixels: &mut Pixels, at: Vec2, color: [u8; 4]) {
    pixels.ring(at, 4.0, OUTLINE);
    pixels.ring(at, 2.0, OUTLINE);
    pixels.ring(at, 3.0, color);
    for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        pixels.set(at.x as i32 + dx, at.y as i32 + dy, OUTLINE);
    }
    pixels.set(at.x as i32, at.y as i32, color);
}

fn show_text(
    game: Res<Game>,
    touch: Res<Touch>,
    mut texts: Query<(&Says, &mut Text, &mut TextColor)>,
) {
    let rules = &game.rules;
    let turn = rules.turn;
    let names = game.names();
    let me = game.match_seat.as_ref().map(|(me, _)| *me);
    let seated = game.board_id.as_deref().map_or(0, seats::seated);
    let back = if touch.is_on() { "Leave" } else { "Esc" };
    // The game waits on a player who left their seat, on their turn.
    let waiting_on = match (me, &game.opponent) {
        (Some(me), None) if turn != me && rules.win.is_none() => Some(&names[turn]),
        _ => None,
    };
    for (says, mut text, mut color) in &mut texts {
        let line = match *says {
            Says::Player(player) => {
                // Whose turn it is stands out.
                let lit = rules.win.is_none() && turn == player;
                let wanted = if lit {
                    Color::WHITE
                } else {
                    Color::srgb(0.6, 0.6, 0.65)
                };
                if color.0 != wanted {
                    color.0 = wanted;
                }
                format!("{}\n{}", names[player], rules.remaining[player])
            }
            Says::Turn => {
                // This turn's darts, and what they make.
                let mut line = rules
                    .darts
                    .iter()
                    .map(|hit| hit.name())
                    .collect::<Vec<_>>()
                    .join("  ");
                if rules.busted {
                    line.push_str("\nBust!");
                } else if !rules.darts.is_empty() {
                    line.push_str(&format!("\n= {}", rules.turn_points()));
                }
                line
            }
            Says::Status if game.watching && seated == 0 => {
                format!("Nobody is playing\nright now.\n\n{back} goes back.")
            }
            Says::Status if game.watching && game.waiting_for_start => {
                "Watching.\nWaiting for the game...".to_string()
            }
            Says::Status if game.waiting_for_start => "Joining the game...".to_string(),
            Says::Status if let Some(name) = waiting_on => {
                format!("Waiting for {name} to come back.\n\nNew game (in /settings) starts over.")
            }
            Says::Status => match (rules.win, game.phase) {
                (Some(winner), _) if game.watching => format!("{} wins!", names[winner]),
                (Some(winner), _) => {
                    let again = if !game.may_start_over() {
                        format!("Waiting for {} to play again.", names[0])
                    } else if touch.is_on() {
                        "Tap to play again.".to_string()
                    } else {
                        "Click to play again.".to_string()
                    };
                    format!("{} wins!\n\n{again}", names[winner])
                }
                (None, Phase::TurnOver { .. }) if rules.busted => {
                    format!("Bust! {} stays on {}.", names[turn], rules.remaining[turn])
                }
                (None, Phase::TurnOver { .. }) => {
                    format!("{} scores {}.", names[turn], rules.turn_points())
                }
                (None, _) if game.watching => format!(
                    "{}'s throw, dart {} of {DARTS_PER_TURN}.\n\nFrom {START} to exactly 0 wins.\n\nWatching. {back} leaves.",
                    names[turn],
                    rules.darts.len() + 1,
                ),
                (None, _) if !game.my_turn() => format!(
                    "{}'s throw, dart {} of {DARTS_PER_TURN}.\n\nFrom {START} to exactly 0 wins.",
                    names[turn],
                    rules.darts.len() + 1,
                ),
                (None, _) if Control::now(&touch) == Control::Paused => {
                    "Click the board to take aim.\n\nEsc leaves.".to_string()
                }
                (None, _) => {
                    let how = if touch.is_on() {
                        "Put your finger down to aim, then flick it up as you lift it."
                    } else {
                        "Aim, hold the button, and flick the mouse up as you let go. Too soft drops low, too hard goes high.\n\nEsc pauses."
                    };
                    let flick_up = match game.no_flick {
                        Some(since) if since < NO_FLICK_SHOWN_FOR => "Flick up to throw!\n\n",
                        _ => "",
                    };
                    let whose = match me {
                        Some(_) => "Your throw".to_string(),
                        None => format!("{}'s throw", names[turn]),
                    };
                    format!(
                        "{flick_up}{whose}, dart {} of {DARTS_PER_TURN}.\n\n{how}\n\nFrom {START} to exactly 0 wins.",
                        rules.darts.len() + 1,
                    )
                }
            },
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// Puts the texts where they go over the canvas, sized with it.
fn place_text(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mut texts: Query<(&OnCanvas, &mut Node, &mut TextFont)>,
) {
    let (node, transform) = *canvas;
    let scale_factor = window.scale_factor();
    // Logical pixels per canvas pixel, and where the canvas's corner is.
    let scale = node.size().x / CANVAS.x as f32 / scale_factor;
    let corner = (transform.translation - node.size() / 2.0) / scale_factor;
    for (on, mut text_node, mut font) in &mut texts {
        let font_size = FontSize::Px((on.size * scale).max(SMALLEST_TEXT).round());
        if font.font_size != font_size {
            font.font_size = font_size;
        }
        let left = Val::Px((corner.x + on.left * scale).round());
        let top = Val::Px((corner.y + on.top * scale).round());
        let width = Val::Px((on.width * scale).round());
        if text_node.left != left || text_node.top != top || text_node.width != width {
            text_node.left = left;
            text_node.top = top;
            text_node.width = width;
        }
    }
}

/// The New game button in the settings: a new game, wherever this one is at.
fn new_game(mut asked: MessageReader<NewDartsGame>, game: Option<ResMut<Game>>) {
    if asked.read().count() == 0 {
        return;
    }
    if let Some(mut game) = game {
        game.start_over(0);
    }
}

/// Esc leaves, unless the pointer is locked or was just let go: then it pauses (letting the
/// pointer go, if the browser didn't). Leave on a touch screen leaves.
fn leave(
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    time: Res<Time>,
    game: Res<Game>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let esc = keys.just_pressed(KeyCode::Escape);
    if esc && pointer_lock::locked() {
        pointer_lock::release();
        return;
    }
    let just_let_go = game
        .unlocked_at
        .is_some_and(|at| time.elapsed_secs() - at < ESC_AFTER_UNLOCK);
    if (esc && !just_let_go) || touch.tapped(TouchButton::Leave) {
        mode.set(Mode::Walking);
    }
}

fn hide_board(
    mut commands: Commands,
    mut game: ResMut<Game>,
    overlays: Query<Entity, With<Overlay>>,
    mut cursor: Query<&mut CursorOptions>,
) {
    pointer_lock::wanted(false);
    for mut options in &mut cursor {
        options.visible = true;
    }
    // A dart in the air lands where nobody sees.
    if let Phase::Flying { to, hit, .. } = game.phase {
        game.landed(to, hit);
    }
    game.held = None;
    game.flick = None;
    // Away from the board: the game stays as it is for whoever is left there, their seat
    // waiting. This player carries on from it if they come back alone, throwing for both, and
    // gets it from the other player if they're still there.
    seats::stand();
    game.board_id = None;
    game.opponent = None;
    game.match_seat = None;
    game.alone_here = false;
    game.owe_game.clear();
    game.streamed_to.clear();
    game.watching = false;
    game.waiting_for_start = false;
    game.their_hand = None;
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
}

/// For browser tests (testing.rs): a dart asked for, landing exactly where asked, and what the
/// game is doing.
#[cfg(feature = "test-hooks")]
fn test_hooks(mut game: ResMut<Game>) {
    use serde_json::json;

    use crate::testing;

    let game = &mut *game;
    if game.phase == Phase::Aiming
        && game.rules.win.is_none()
        && game.my_turn()
        && let Some(at) = testing::take_dart()
    {
        game.throw(at);
    }
    let phase = match game.phase {
        Phase::Aiming => "aiming",
        Phase::Flying { .. } => "flying",
        Phase::TurnOver { .. } => "turn_over",
    };
    let rules = &game.rules;
    testing::report(
        "darts",
        json!({
            "phase": phase,
            "turn": rules.turn,
            "remaining": rules.remaining,
            "darts": rules.darts.iter().map(|hit| hit.name()).collect::<Vec<_>>(),
            "turn_points": rules.turn_points(),
            "busted": rules.busted,
            "win": rules.win,
            "stuck": game.stuck.iter().map(|at| [at.x, at.y]).collect::<Vec<_>>(),
            "hand": [game.hand.x, game.hand.y],
            "aim": [game.aim.x, game.aim.y],
            "last_flick": [game.last_flick.x, game.last_flick.y],
            "held": game.held,
            "my_turn": game.my_turn(),
            "seat": game.match_seat.as_ref().map(|(me, _)| *me),
            "names": game.names(),
            "opponent": game.opponent.is_some(),
            "waiting_for_start": game.waiting_for_start,
            "watching": game.watching,
            "watchers": game.board_id.as_deref().map_or(0, seats::watching),
            "their_hand": game.their_hand.map(|(at, held)| json!({ "at": [at.x, at.y], "held": held })),
            "locked": pointer_lock::locked(),
            "lock_failed": pointer_lock::failed(),
        }),
    );
}
