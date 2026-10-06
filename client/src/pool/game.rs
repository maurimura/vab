//! Playing pool: the table seen from above, drawn pixel by pixel into a small image that fills
//! the screen at a whole-number zoom. The cue follows the mouse (or the finger) around the cue
//! ball. Holding the button pulls it back, further the longer it's held, and letting go shoots:
//! once pulled, the shot is coming. Esc goes back to the bar, and the table stays as it was
//! for next time.
//!
//! It's 8-ball. Alone at the table, the player takes both sides in turn; when someone sits at
//! the other seat, they play each other (online.rs), each shooting on their own turn and
//! watching the other's cue on theirs. Someone sitting down carries on the game as it is, and
//! someone leaving leaves it as it is, their seat waiting for them: a reload doesn't end it.
//! New rack (in `/settings`) starts over. A panel for each player under the table, player 1's on
//! the left and player 2's on the right, shows their group and their balls that are down, in
//! order, the player at the table lit up; between them a line says what the last shot did.
//! After a foul the next player has ball in hand: the cue ball follows the pointer (or the
//! arrow keys) until a click, a tap or Space puts it down. The physics and the rules are the
//! billiards crate's, and how it plays can be tuned with `/settings` (settings.rs).
//!
//! Anyone can watch the game at a table (F in the bar): they see the table as the players do,
//! hands off the cue. The lowest seat gives each watcher the game as it is, between shots, and
//! from then on whatever the players send each other (where the cue points, each shot, where it
//! ended) goes to the watchers too, so their table plays each shot out as the players' do.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use billiards::rules::{self, Foul, Group, WinBy};
use billiards::{BALL_RADIUS, HEIGHT, Pocket, RAIL, STEP, Table, WIDTH, pockets};

use wasm_bindgen::prelude::*;

use super::online::Message;
use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::pixels::Pixels;
use crate::seats;
use crate::settings::{Knob, NewRack, Settings};
use crate::touch::{self, Touch, TouchButton};

/// The image the table is drawn into, and where the felt's corner is in it.
const CANVAS: UVec2 = UVec2::new(320, 180);
const FELT: Vec2 = Vec2::new(32.0, 15.0);
/// The players' panels under the table, lined up with its outer edges: from their left edge to
/// their right, and from their top to their bottom. Their name and group on top, and a tray of
/// their balls that are down below.
const PANELS: [(i32, i32); 2] = [(17, 83), (237, 303)];
const PANEL_TOP: i32 = 159;
const PANEL_BOTTOM: i32 = 173;
/// Where each panel's name line starts (player 1's) or ends (player 2's), and its top.
const LABEL_EDGES: [f32; 2] = [23.0, 297.0];
const LABEL_TOP: f32 = 159.5;
/// The trays: where each starts, the middle of their balls, how far apart they are, and the
/// extra room before the 8's place at the end. Their balls are the small ones, 5 across.
const TRAY_LEFT: [f32; 2] = [23.0, 248.0];
const TRAY_MIDDLE: f32 = 169.5;
const TRAY_SLOT: f32 = 6.0;
const TRAY_EIGHT_GAP: f32 = 2.0;
const TRAY_BALL_RADIUS: f32 = 2.5;
/// The line between the panels, from its left edge to its right.
const STATUS_SPAN: (f32, f32) = (87.0, 233.0);
/// Text heights, in canvas pixels, and the smallest they get on a small screen, in logical
/// pixels.
const LABEL_SIZE: f32 = 3.5;
const STATUS_SIZE: f32 = 3.0;
const SMALLEST_TEXT: f32 = 7.0;
/// How thick the cushions are, in pixels. The rail is the rest of the way out to the
/// physics' edge of the table.
const CUSHION: i32 = 3;
/// Holes are drawn this much smaller than where balls drop, so they end where the jaws do.
const HOLE_DRAWN_SMALLER: f32 = 1.5;

/// How long the cue takes to come forward to the ball once let go.
const STRIKE_TIME: f32 = 0.05;
/// The gap between the cue's tip and the ball while aiming, and the cue's length.
const CUE_GAP: f32 = 2.0;
const CUE_LENGTH: f32 = 110.0;
/// Turning the cue with the arrow keys, in radians per second (holding Shift: slowly).
const TURN_SPEED: f32 = 1.2;
const FINE_TURN_SPEED: f32 = 0.15;
/// Moving the cue ball with the arrow keys for ball in hand, in pixels per second.
const PLACE_SPEED: f32 = 60.0;
/// Sat down with someone already at the table, how long to wait for their game before player 1
/// racks a new one (neither had one), in seconds.
const NO_GAME_COMING: f32 = 1.5;
/// How often the cue (or a ball in hand) goes to the other player while it moves, in seconds.
const AIM_SEND_EVERY: f32 = 1.0 / 15.0;
/// At most this much time is caught up on in one frame, after the tab was in the background.
const MAX_CATCH_UP: f32 = 0.1;

const CLEAR: [u8; 4] = [0, 0, 0, 0];
const FELT_COLOR: [u8; 4] = [34, 120, 64, 255];
const CUSHION_COLOR: [u8; 4] = [24, 92, 48, 255];
const RAIL_COLOR: [u8; 4] = [110, 62, 32, 255];
const RAIL_EDGE: [u8; 4] = [62, 34, 16, 255];
const RAIL_LIGHT: [u8; 4] = [140, 86, 46, 255];
const SIGHT: [u8; 4] = [232, 220, 180, 255];
const POCKET: [u8; 4] = [12, 10, 8, 255];
const WHITE: [u8; 4] = [240, 236, 220, 255];
const SHINE: [u8; 4] = [255, 255, 255, 255];
const CUE_TIP: [u8; 4] = [80, 130, 200, 255];
const CUE_SHAFT: [u8; 4] = [226, 196, 140, 255];
const CUE_BUTT: [u8; 4] = [92, 50, 26, 255];
/// The ring around a cue ball that can be moved (ball in hand).
const IN_HAND: [u8; 4] = [150, 200, 160, 255];
/// The panels: the player at the table's and the other's, an empty place in a tray, and the
/// arrow by the name of the player at the table.
const PANEL_ACTIVE: [u8; 4] = [78, 64, 44, 255];
const PANEL_IDLE: [u8; 4] = [30, 26, 22, 235];
const TRAY_EMPTY: [u8; 4] = [14, 12, 10, 255];
const ARROW: [u8; 4] = [244, 204, 84, 255];
/// Balls 1 to 8; 9 to 15 are 1 to 7's colors with a white band each side.
const BALL_COLORS: [[u8; 3]; 8] = [
    [240, 196, 32],
    [36, 72, 200],
    [214, 40, 40],
    [112, 48, 160],
    [240, 120, 24],
    [24, 140, 64],
    [130, 30, 30],
    [28, 28, 32],
];
/// A ball, 7 pixels across, and smaller as it drops into a pocket.
const BALL_SHAPE: [&str; 7] = [
    "..###..", ".#####.", "#######", "#######", "#######", ".#####.", "..###..",
];
const SMALL_BALL_SHAPE: [&str; 5] = [".###.", "#####", "#####", "#####", ".###."];
const TINY_BALL_SHAPE: [&str; 3] = [".#.", "###", ".#."];
/// How long a ball takes to drop out of sight into a pocket, and how much darker it is by
/// the end.
const DROP_TIME: f32 = 0.25;
const DROP_DARKENING: f32 = 0.6;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Math)]
    fn random() -> f64;
}

/// A seed for a new rack, so each is a little different: the browser picks it.
fn new_seed() -> u32 {
    (random() * f64::from(u32::MAX)) as u32
}

/// The balls racked again, each time a little differently.
fn new_rack() -> Table {
    Table::racked(new_seed())
}

/// A table's settings as they go in a shot, and back.
fn settings_to_message(settings: &billiards::Settings) -> [f32; 6] {
    [
        settings.min_speed,
        settings.max_speed,
        settings.friction,
        settings.ball_restitution,
        settings.cushion_restitution,
        settings.pocket_mouth,
    ]
}

fn settings_from_message(values: [f32; 6]) -> billiards::Settings {
    let [
        min_speed,
        max_speed,
        friction,
        ball_restitution,
        cushion_restitution,
        pocket_mouth,
    ] = values;
    billiards::Settings {
        min_speed,
        max_speed,
        friction,
        ball_restitution,
        cushion_restitution,
        pocket_mouth,
    }
}

/// The pool table the player is at, as the room names it (online::table_id, seats.rs), and
/// whether they watch the game there rather than play. Set before switching to `Mode::Pool`.
#[derive(Resource)]
pub struct AtTable {
    pub id: String,
    pub watching: bool,
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Pool), show_table)
            .add_systems(
                Update,
                (
                    fit_canvas,
                    sync,
                    aim,
                    play,
                    send_aim,
                    draw,
                    show_status,
                    place_text,
                    leave.run_if(chat_closed),
                )
                    .chain()
                    .run_if(in_state(Mode::Pool)),
            )
            .add_systems(OnExit(Mode::Pool), hide_table)
            .add_systems(Update, rack_again);
        #[cfg(feature = "test-hooks")]
        app.add_systems(Update, test_hooks.after(play).run_if(in_state(Mode::Pool)));
    }
}

/// The game, kept between visits to the table.
#[derive(Resource)]
struct Game {
    table: Table,
    /// Where the cue points, from the cue ball: the way the shot goes.
    aim: Vec2,
    cue: Cue,
    /// Time not yet run through the physics.
    pending: f32,
    /// Balls that just went down, still being drawn on their way into the hole.
    dropping: Vec<Drop>,
    /// The game of 8-ball being played on the table, by players 0 and 1.
    rules: rules::Game,
    /// The table, as the room names it, while the player sits at it.
    table_id: Option<String>,
    /// The other player at the table, when there is one: the game is between them.
    opponent: Option<Opponent>,
    /// Just sat down at a table with someone at it, and waiting for the game from them (or,
    /// both new to it, for player 1 to rack one), for this many seconds.
    waiting_for_start: bool,
    waiting_since: f32,
    /// This player's seat in a game against someone, and both players' names: kept while the
    /// other seat is empty, so the game waits for them.
    match_seat: Option<(usize, [String; 2])>,
    /// The room last had this player at the table on their own: whoever sits down next gets
    /// the game from them.
    alone_here: bool,
    /// The game is owed to these players, who just sat down or started watching: it goes once
    /// no shot is under way.
    owe_game: Vec<u32>,
    /// The watchers given the game so far (or owed it), to spot new ones.
    streamed_to: Vec<u32>,
    /// Watching the game at the table, not playing: its players send what they do.
    watching: bool,
    /// Who sits at the table, by seat, while watching.
    watched_names: [String; 2],
    /// The settings the other player shot with, while their shot rolls here.
    their_settings: Option<billiards::Settings>,
    /// Where the other player's shot ended on their table, when it came in before it stopped
    /// rolling here.
    their_result: Option<Message>,
    /// The cue (or ball in hand) as last sent to the other player, and how long ago.
    sent: (Message, f32),
}

/// Who the player is playing against.
struct Opponent {
    /// The player's own seat (0 for player 1), and the other player's id in the room.
    me: usize,
    id: u32,
    /// Both seats' names.
    names: [String; 2],
}

/// A ball falling into a pocket: from where it went down to the hole's middle.
struct Drop {
    number: u8,
    from: Vec2,
    hole: Vec2,
    /// Seconds since it went down.
    age: f32,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            table: new_rack(),
            aim: Vec2::X,
            cue: Cue::Aiming,
            pending: 0.0,
            dropping: Vec::new(),
            rules: rules::Game::new(0),
            table_id: None,
            opponent: None,
            waiting_for_start: false,
            waiting_since: 0.0,
            match_seat: None,
            alone_here: false,
            owe_game: Vec::new(),
            streamed_to: Vec::new(),
            watching: false,
            watched_names: ["Player 1".to_string(), "Player 2".to_string()],
            their_settings: None,
            their_result: None,
            sent: (Message::Place { at: [0.0; 2] }, 0.0),
        }
    }
}

impl Game {
    /// A new game on the rack from `seed`, `breaker` to break.
    fn begin(&mut self, seed: u32, breaker: usize) {
        self.table = Table::racked(seed);
        self.rules = rules::Game::new(breaker);
        self.cue = Cue::Aiming;
        self.pending = 0.0;
        self.dropping.clear();
        self.waiting_for_start = false;
        self.their_settings = None;
        self.their_result = None;
    }

    /// A new game on a fresh rack, the other player breaking this time. Against someone, only
    /// player 1 starts games, and tells player 2 (and the watchers). Alone at a game whose other
    /// seat is empty, it ends: the player takes both sides again. A watcher starts nothing.
    fn start_over(&mut self) {
        if !self.may_start_over() {
            return;
        }
        let breaker = 1 - self.rules.breaker;
        let seed = new_seed();
        if self.opponent.is_none() {
            self.match_seat = None;
        }
        self.begin(seed, breaker);
        self.tell(Message::Start { seed, breaker });
    }

    /// Whether this player may start the next game: alone, or as player 1 against someone.
    fn may_start_over(&self) -> bool {
        !self.watching
            && self
                .opponent
                .as_ref()
                .is_none_or(|opponent| opponent.me == 0)
    }

    /// The whole game, for a player sitting down: every ball, and where the 8-ball game is at.
    fn whole_game(&self) -> Message {
        Message::Sync {
            balls: self
                .table
                .balls
                .iter()
                .map(|ball| {
                    let down = if ball.pocketed { 1.0 } else { 0.0 };
                    [
                        f32::from(ball.number),
                        ball.position.x,
                        ball.position.y,
                        down,
                    ]
                })
                .collect(),
            rules: self.rules.clone(),
        }
    }

    /// Carries on the game as the other player had it: the same balls in the same places (their
    /// rack may not have been this one's).
    fn take_whole_game(&mut self, balls: Vec<[f32; 4]>, rules: rules::Game) {
        for (ball, [number, x, y, down]) in self.table.balls.iter_mut().zip(balls) {
            ball.number = number as u8;
            ball.position = Vec2::new(x, y);
            ball.velocity = Vec2::ZERO;
            ball.pocketed = down != 0.0;
        }
        self.rules = rules;
        self.dropping.clear();
        self.waiting_for_start = false;
        self.their_settings = None;
        self.their_result = None;
        self.cue = if self.rules.win.is_some() {
            Cue::Over
        } else if self.rules.ball_in_hand {
            Cue::Placing { held: false }
        } else {
            Cue::Aiming
        };
    }

    fn send(&self, to: u32, message: Message) {
        if let Some(table) = &self.table_id {
            seats::send(table, to, &message);
        }
    }

    /// Tells the other player, and everyone watching the table, `message`.
    fn tell(&self, message: Message) {
        let Some(table) = &self.table_id else {
            return;
        };
        if let Some(opponent) = &self.opponent {
            seats::send(table, opponent.id, &message);
        }
        if seats::watching(table) > 0 {
            seats::send_watchers(table, &message);
        }
    }

    /// Whether anyone follows what this player does: an opponent, or watchers.
    fn has_audience(&self) -> bool {
        self.opponent.is_some()
            || self
                .table_id
                .as_deref()
                .is_some_and(|table| seats::watching(table) > 0)
    }

    /// Whether this player takes the next shot: always when alone, on their turn in a game
    /// against someone (who may have left their seat: then it waits for them). Never watching.
    fn my_turn(&self) -> bool {
        !self.watching
            && !self.waiting_for_start
            && self
                .match_seat
                .as_ref()
                .is_none_or(|(me, _)| self.rules.turn == *me)
    }

    /// Both players' names: their names in the room in a game against someone, or watched.
    fn names(&self) -> [String; 2] {
        match &self.match_seat {
            Some((_, names)) => names.clone(),
            None if self.watching => self.watched_names.clone(),
            None => ["Player 1".to_string(), "Player 2".to_string()],
        }
    }

    /// Follows what another player at the table did (on their turn, when it's a shot): the game
    /// they hand over, and their cue, cue ball, shot and where it ended.
    fn follow(&mut self, message: Message) {
        let theirs = !self.my_turn();
        let cue_free = matches!(
            self.cue,
            Cue::Aiming | Cue::Pulling(_) | Cue::Placing { .. }
        );
        match message {
            Message::Start { seed, breaker } => self.begin(seed, breaker),
            Message::Sync { balls, rules } => self.take_whole_game(balls, rules),
            // Nothing else counts until the game is here.
            _ if self.waiting_for_start => {}
            Message::Aim { aim, pull } if theirs && cue_free => {
                self.aim = Vec2::from(aim);
                self.cue = if pull > 0.0 {
                    Cue::Pulling(pull)
                } else {
                    Cue::Aiming
                };
            }
            Message::Place { at } if theirs && cue_free => {
                self.table.balls[0].position = Vec2::from(at);
                self.cue = Cue::Placing { held: false };
            }
            Message::Shot {
                cue,
                aim,
                power,
                settings,
            } if theirs => {
                self.table.balls[0].position = Vec2::from(cue);
                self.aim = Vec2::from(aim);
                self.their_settings = Some(settings_from_message(settings));
                self.cue = Cue::Striking {
                    pull: power,
                    time: 0.0,
                };
            }
            result @ Message::Settled { .. } if theirs => {
                if let Cue::Waiting = self.cue {
                    self.take_their_result(result);
                } else {
                    self.their_result = Some(result);
                }
            }
            _ => {}
        }
    }

    /// Where the other player's shot ended, on their table: everything goes there, and the
    /// rules judge it from there, as they did on that table.
    fn take_their_result(&mut self, result: Message) {
        let Message::Settled {
            balls,
            first_hit,
            pocketed,
        } = result
        else {
            return;
        };
        for (ball, [x, y, down]) in self.table.balls.iter_mut().zip(balls) {
            ball.position = Vec2::new(x, y);
            ball.velocity = Vec2::ZERO;
            ball.pocketed = down != 0.0;
        }
        self.table.shot = billiards::Shot {
            first_hit,
            pocketed,
        };
        self.their_settings = None;
        self.finish_shot();
    }

    /// Once the balls have stopped: the rules judge the shot, and a pocketed cue ball comes back.
    fn finish_shot(&mut self) {
        self.rules.judge(&mut self.table);
        self.table.respot_cue_ball();
        self.cue = if self.rules.win.is_some() {
            Cue::Over
        } else if self.rules.ball_in_hand {
            Cue::Placing { held: false }
        } else {
            Cue::Aiming
        };
    }
}

#[derive(Clone, Copy)]
enum Cue {
    Aiming,
    /// Coming back, this far so far (0 to 1).
    Pulling(f32),
    /// Let go: coming forward from `pull` (0 to 1), for `time` seconds so far.
    Striking {
        pull: f32,
        time: f32,
    },
    /// Away while the balls roll.
    Rolling,
    /// Away: someone won. A press starts the next game.
    Over,
    /// Ball in hand: the cue ball follows the pointer, and goes down when a press (`held`) is
    /// let go.
    Placing {
        held: bool,
    },
    /// Away: the other player's shot has stopped here, and waits for where it ended on their
    /// table.
    Waiting,
}

impl Cue {
    /// How far back the cue is, from 0 to 1.
    fn pull(&self) -> f32 {
        match *self {
            Cue::Pulling(pull) => pull,
            Cue::Striking { pull, time } => pull * (1.0 - time / STRIKE_TIME).max(0.0),
            _ => 0.0,
        }
    }
}

/// What the player's doing with the cue this frame: the mouse button, Space or a finger.
#[derive(Resource, Default)]
struct Holding {
    now: bool,
    before: bool,
    /// The chat or a panel has the keys and the pointer: the cue waits as it is.
    blocked: bool,
}

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct Canvas;

/// The line between the players' panels: what the last shot did, or who won.
#[derive(Component)]
struct Status;

/// A player's name and group, over their panel.
#[derive(Component)]
struct Label(usize);

fn show_table(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Option<Res<Game>>,
    touch: Res<Touch>,
) {
    if game.is_none() {
        commands.init_resource::<Game>();
    }
    commands.init_resource::<Holding>();
    let image = Image::new_fill(
        Extent3d {
            width: CANVAS.x,
            height: CANVAS.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &CLEAR,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let text = |size: f32, justify: Justify| {
        (
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout {
                justify,
                ..default()
            },
            // Placed over the canvas by place_text.
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
            (Label(0), text(14.0, Justify::Left)),
            (Label(1), text(14.0, Justify::Right)),
            (Status, text(12.0, Justify::Center)),
        ],
    ));
    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), as with a game.
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

/// As big as fits, at a whole number of the screen's own pixels per table pixel.
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

/// Sits at the table in the room, starts a game when someone sits at the other seat (player 1
/// racks it), goes back to both sides when they leave, and follows what they do on their turn.
/// Watching, follows whatever the players there do.
fn sync(at: Option<Res<AtTable>>, time: Res<Time>, mut game: ResMut<Game>) {
    let Some(at) = at else {
        return;
    };
    let game = &mut *game;
    if game.table_id.as_deref() != Some(at.id.as_str()) {
        game.table_id = Some(at.id.clone());
        game.watching = at.watching;
        if at.watching {
            // The game comes from whoever plays there, between shots; until then, nothing.
            game.waiting_for_start = true;
            game.waiting_since = 0.0;
            game.cue = Cue::Aiming;
            game.dropping.clear();
            game.their_settings = None;
            game.their_result = None;
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
    match (&mut game.opponent, now) {
        (Some(opponent), Some((me, id, names))) if opponent.id == id && opponent.me == me => {
            game.match_seat = Some((me, names.clone()));
            opponent.names = names;
        }
        (_, Some((me, id, names))) => {
            game.opponent = Some(Opponent {
                me,
                id,
                names: names.clone(),
            });
            game.match_seat = Some((me, names));
            if was_alone {
                // They sat down with this player already here: they get the game as it is.
                game.owe_game.push(id);
            } else {
                // Sat down with them already here: the game comes from them.
                game.waiting_for_start = true;
                game.waiting_since = 0.0;
            }
        }
        (Some(opponent), None) => {
            // They left: the game waits for them as it is, their seat empty. Their shot, if
            // it's still rolling, ends here.
            let left = opponent.id;
            game.opponent = None;
            game.waiting_for_start = false;
            game.owe_game.retain(|id| *id != left);
            game.their_settings = None;
            game.their_result = None;
            if let Cue::Waiting = game.cue {
                game.finish_shot();
            }
        }
        (None, None) => {}
    }

    // Both new to the table, nobody had a game to give: player 1 racks one.
    if game.waiting_for_start {
        game.waiting_since += time.delta_secs();
        if let Some(opponent) = game.opponent.as_ref().filter(|opponent| opponent.me == 0)
            && game.waiting_since > NO_GAME_COMING
        {
            let (id, seed) = (opponent.id, new_seed());
            game.begin(seed, 0);
            game.send(id, Message::Start { seed, breaker: 0 });
        }
    }
    // The lowest seat gives each new watcher the game, as it does a player who sits down.
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
    // The game owed to those who sat down or started watching goes once no shot is under way,
    // and the cue as it is right after, moved or not.
    let between_shots = !matches!(game.cue, Cue::Striking { .. } | Cue::Rolling | Cue::Waiting);
    if between_shots && !game.owe_game.is_empty() {
        for id in std::mem::take(&mut game.owe_game) {
            let whole = game.whole_game();
            game.send(id, whole);
        }
        game.sent = (
            Message::Start {
                seed: 0,
                breaker: 0,
            },
            AIM_SEND_EVERY,
        );
    }

    let messages = seats::take_messages::<Message>(&at.id);
    let Some(id) = game.opponent.as_ref().map(|opponent| opponent.id) else {
        return;
    };
    for (_, message) in messages.into_iter().filter(|(from, _)| *from == id) {
        game.follow(message);
    }
}

/// Watching: follows what the players at the table do, and who they are.
fn watch(game: &mut Game, table: &str, seats: Option<seats::Seats>) {
    let players: Vec<u32> = match &seats {
        Some(seats) => {
            game.watched_names = seats.names();
            seats.ids().collect()
        }
        None => Vec::new(),
    };
    for (_, message) in seats::take_messages::<Message>(table)
        .into_iter()
        .filter(|(from, _)| players.contains(from))
    {
        game.follow(message);
    }
}

/// On this player's turn, with someone to see it: where their cue points, or where they have
/// the cue ball, goes to the other player and the watchers as it changes, a few times a second.
fn send_aim(time: Res<Time>, mut game: ResMut<Game>) {
    if !game.has_audience() || !game.my_turn() {
        return;
    }
    game.sent.1 += time.delta_secs();
    let message = match game.cue {
        Cue::Aiming | Cue::Pulling(_) => Message::Aim {
            aim: game.aim.into(),
            pull: game.cue.pull(),
        },
        Cue::Placing { .. } => Message::Place {
            at: game.table.cue_ball().position.into(),
        },
        _ => return,
    };
    if message != game.sent.0 && game.sent.1 >= AIM_SEND_EVERY {
        game.sent = (message.clone(), 0.0);
        game.tell(message);
    }
}

/// Points the cue where the mouse or finger is, or turns it with the arrow keys, and reads
/// whether the player is holding it.
#[allow(clippy::too_many_arguments)]
fn aim(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    chat: Res<Chat>,
    help: Res<Help>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut game: ResMut<Game>,
    mut holding: ResMut<Holding>,
    mut last_pointer: Local<Option<Vec2>>,
) {
    holding.before = holding.now;
    holding.blocked = chat.is_open() || help.is_open() || settings.is_open();
    if holding.blocked {
        // As if held all along, so the click or tap that closes a panel doesn't start a shot:
        // only a fresh press after it does.
        holding.before = true;
        holding.now = true;
        return;
    }
    let (node, transform) = *canvas;
    // From the window's logical pixels to the felt's.
    let to_felt = |at: Vec2| {
        let size = node.size();
        let corner = transform.translation - size / 2.0;
        (at * window.scale_factor() - corner) / size * CANVAS.as_vec2() - FELT
    };
    // Until the canvas has been laid out it has no size, and nowhere on it to point at.
    let pointer = touch
        .finger()
        .or(window.cursor_position())
        .filter(|_| node.size().min_element() > 0.0);
    if !game.my_turn() {
        // The other player's turn: hands off the cue.
        *last_pointer = pointer;
        holding.now = mouse.pressed(MouseButton::Left)
            || keys.pressed(KeyCode::Space)
            || touch.finger().is_some();
        return;
    }
    if let Cue::Placing { .. } = game.cue {
        let mut to = pointer.filter(|at| Some(*at) != *last_pointer).map(to_felt);
        let way = [
            (KeyCode::ArrowLeft, Vec2::NEG_X),
            (KeyCode::KeyA, Vec2::NEG_X),
            (KeyCode::ArrowRight, Vec2::X),
            (KeyCode::KeyD, Vec2::X),
            (KeyCode::ArrowUp, Vec2::NEG_Y),
            (KeyCode::KeyW, Vec2::NEG_Y),
            (KeyCode::ArrowDown, Vec2::Y),
            (KeyCode::KeyS, Vec2::Y),
        ]
        .iter()
        .filter(|(key, _)| keys.pressed(*key))
        .map(|(_, way)| *way)
        .sum::<Vec2>();
        if way != Vec2::ZERO {
            let step = way.normalize() * PLACE_SPEED * time.delta_secs();
            to = Some(game.table.cue_ball().position + step);
        }
        // Kept on the felt; where it would sit on another ball, it stays where it was.
        let margin = Vec2::splat(BALL_RADIUS);
        if let Some(to) = to.map(|to| to.clamp(margin, Vec2::new(WIDTH, HEIGHT) - margin))
            && to.is_finite()
            && game.table.cue_ball_fits(to)
        {
            game.table.balls[0].position = to;
        }
        *last_pointer = pointer;
        holding.now = mouse.pressed(MouseButton::Left)
            || keys.pressed(KeyCode::Space)
            || touch.finger().is_some();
        return;
    }
    if pointer != *last_pointer
        && let Some(at) = pointer
    {
        let towards = to_felt(at) - game.table.cue_ball().position;
        if towards.length() > 1.0 && towards.is_finite() {
            game.aim = towards.normalize();
        }
    }
    *last_pointer = pointer;

    let turn = [
        (KeyCode::ArrowLeft, -1.0),
        (KeyCode::KeyA, -1.0),
        (KeyCode::ArrowRight, 1.0),
        (KeyCode::KeyD, 1.0),
    ]
    .iter()
    .filter(|(key, _)| keys.pressed(*key))
    .map(|(_, way)| way)
    .sum::<f32>()
    .clamp(-1.0, 1.0);
    if turn != 0.0 {
        let fine = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        let speed = if fine { FINE_TURN_SPEED } else { TURN_SPEED };
        game.aim = Vec2::from_angle(turn * speed * time.delta_secs()).rotate(game.aim);
    }

    holding.now = mouse.pressed(MouseButton::Left)
        || keys.pressed(KeyCode::Space)
        || touch.finger().is_some();
}

/// Pulls the cue back while held, shoots once let go, and runs the table until it's still.
fn play(time: Res<Time>, holding: Res<Holding>, settings: Res<Settings>, mut game: ResMut<Game>) {
    let game = &mut *game;
    let delta = time.delta_secs();
    // The other player's shot plays out with their settings.
    game.table.settings = game.their_settings.unwrap_or_else(|| settings.pool());
    let mine = game.my_turn();
    for drop in &mut game.dropping {
        drop.age += delta;
    }
    game.dropping.retain(|drop| drop.age < DROP_TIME);
    let full_pull = settings.get(Knob::FullPull);
    game.cue = match game.cue {
        // The other player's turn: their cue moves as they say (sync).
        cue @ (Cue::Aiming | Cue::Pulling(_) | Cue::Placing { .. }) if !mine => cue,
        // Typing or tuning: a cue being aimed or pulled back, or a ball in hand, stays put.
        Cue::Aiming | Cue::Pulling(_) | Cue::Placing { .. } if holding.blocked => game.cue,
        Cue::Placing { held: false } if holding.now && !holding.before => {
            Cue::Placing { held: true }
        }
        Cue::Placing { held: true } if !holding.now => Cue::Aiming,
        cue @ Cue::Placing { .. } => cue,
        // A fresh press: one held from before the balls stopped doesn't count.
        Cue::Aiming if holding.now && !holding.before => Cue::Pulling(0.0),
        Cue::Pulling(pull) if holding.now => Cue::Pulling((pull + delta / full_pull).min(1.0)),
        Cue::Pulling(_) => Cue::Striking {
            pull: game.cue.pull(),
            time: 0.0,
        },
        Cue::Striking { pull, time } if time + delta >= STRIKE_TIME => {
            if mine {
                let shot = Message::Shot {
                    cue: game.table.cue_ball().position.into(),
                    aim: game.aim.into(),
                    power: pull,
                    settings: settings_to_message(&game.table.settings),
                };
                game.tell(shot);
            }
            game.table.shoot(game.aim, pull);
            game.pending = 0.0;
            Cue::Rolling
        }
        Cue::Striking { pull, time } => Cue::Striking {
            pull,
            time: time + delta,
        },
        Cue::Rolling => {
            game.pending = (game.pending + delta).min(MAX_CATCH_UP);
            while game.pending >= STEP && game.table.is_moving() {
                let down_before: Vec<bool> = game.table.balls.iter().map(|b| b.pocketed).collect();
                game.table.step();
                game.pending -= STEP;
                // A ball that went down this step starts falling in from where it is.
                let pockets = pockets(&game.table.settings);
                for (ball, was_down) in game.table.balls.iter().zip(down_before) {
                    if ball.pocketed && !was_down {
                        let hole = pockets
                            .iter()
                            .map(|pocket| pocket.hole)
                            .min_by(|a, b| {
                                let (da, db) =
                                    (a.distance(ball.position), b.distance(ball.position));
                                da.total_cmp(&db)
                            })
                            .unwrap_or(ball.position);
                        game.dropping.push(Drop {
                            number: ball.number,
                            from: ball.position,
                            hole,
                            age: 0.0,
                        });
                    }
                }
            }
            if game.table.is_moving() {
                Cue::Rolling
            } else if game.their_settings.is_some() {
                // The other player's shot: it's judged where it ended on their table.
                match game.their_result.take() {
                    Some(result) => {
                        game.take_their_result(result);
                        game.cue
                    }
                    None => Cue::Waiting,
                }
            } else {
                let settled = Message::Settled {
                    balls: game
                        .table
                        .balls
                        .iter()
                        .map(|ball| {
                            let down = if ball.pocketed { 1.0 } else { 0.0 };
                            [ball.position.x, ball.position.y, down]
                        })
                        .collect(),
                    first_hit: game.table.shot.first_hit,
                    pocketed: game.table.shot.pocketed.clone(),
                };
                game.tell(settled);
                game.finish_shot();
                game.cue
            }
        }
        // A new game, if this player may start one (player 1, against someone).
        Cue::Over if holding.now && !holding.before && !holding.blocked => {
            game.start_over();
            game.cue
        }
        cue @ (Cue::Aiming | Cue::Over | Cue::Waiting) => cue,
    };
}

fn draw(
    game: Res<Game>,
    settings: Res<Settings>,
    canvas: Single<&ImageNode, With<Canvas>>,
    mut images: ResMut<Assets<Image>>,
    mut table_art: Local<Option<(f32, Pixels)>>,
) {
    let Some(mut image) = images.get_mut(&canvas.image) else {
        return;
    };
    // Drawn again only when the pockets change size.
    let mouth = settings.get(Knob::PocketMouth);
    if table_art.as_ref().is_none_or(|(drawn, _)| *drawn != mouth) {
        *table_art = Some((mouth, draw_table(&pockets(&settings.pool()))));
    }
    let Some((_, art)) = table_art.as_ref() else {
        return;
    };
    let mut pixels = art.clone();
    for drop in &game.dropping {
        // Quickly at first, then settling into the middle of the hole.
        let t = drop.age / DROP_TIME;
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        let at = drop.from.lerp(drop.hole, eased);
        let shape: &[&str] = match t {
            t if t < 1.0 / 3.0 => &BALL_SHAPE,
            t if t < 2.0 / 3.0 => &SMALL_BALL_SHAPE,
            _ => &TINY_BALL_SHAPE,
        };
        pixels.ball(drop.number, at + FELT, shape, 1.0 - DROP_DARKENING * t);
    }
    for ball in game.table.balls.iter().filter(|ball| !ball.pocketed) {
        pixels.ball(ball.number, ball.position + FELT, &BALL_SHAPE, 1.0);
    }
    if let Cue::Placing { .. } = game.cue {
        pixels.ring(
            game.table.cue_ball().position + FELT,
            BALL_RADIUS + 2.0,
            IN_HAND,
        );
    }
    if !matches!(
        game.cue,
        Cue::Rolling | Cue::Over | Cue::Placing { .. } | Cue::Waiting
    ) {
        let cue_ball = game.table.cue_ball().position + FELT;
        let pull_back = settings.get(Knob::PullBack);
        let gap = BALL_RADIUS + CUE_GAP + game.cue.pull() * pull_back;
        pixels.cue(cue_ball - game.aim * gap, -game.aim);
    }
    draw_panels(&mut pixels, &game.rules);
    image.data = Some(pixels.into_bytes());
}

/// The player at the table, or the winner once the game is over: their panel is lit up.
fn lit_player(rules: &rules::Game) -> usize {
    rules.win.map_or(rules.turn, |win| win.winner)
}

/// The players' panels: lit up for the player at the table, with an arrow by their name, and
/// each a tray of their group's balls that are down, in order, then the 8 if they sank it.
fn draw_panels(pixels: &mut Pixels, rules: &rules::Game) {
    let lit = lit_player(rules);
    for (player, (left, right)) in PANELS.into_iter().enumerate() {
        let color = if player == lit {
            PANEL_ACTIVE
        } else {
            PANEL_IDLE
        };
        pixels.rect(
            IVec2::new(left, PANEL_TOP),
            IVec2::new(right, PANEL_BOTTOM),
            color,
        );
        let down = rules.down_of(player);
        for slot in 0..8 {
            let gap = if slot == 7 { TRAY_EIGHT_GAP } else { 0.0 };
            let x = TRAY_LEFT[player] + TRAY_BALL_RADIUS + slot as f32 * TRAY_SLOT + gap;
            let at = Vec2::new(x, TRAY_MIDDLE);
            let ball = match slot {
                7 => rules.sank_eight(player).then_some(8),
                _ => down.get(slot).copied(),
            };
            match ball {
                Some(number) => pixels.ball(number, at, &SMALL_BALL_SHAPE, 1.0),
                None => pixels.disc(at, TRAY_BALL_RADIUS, TRAY_EMPTY),
            }
        }
    }
    // A little arrow by the lit player's name, pointing in at it: its back three pixels tall,
    // narrowing to its point.
    let (back, way) = if lit == 0 {
        (PANELS[0].0 + 2, 1)
    } else {
        (PANELS[1].1 - 3, -1)
    };
    let middle = LABEL_TOP as i32 + 2;
    for row in -1..=1_i32 {
        for step in 0..2 - row.abs() {
            pixels.set(back + way * step, middle + row, ARROW);
        }
    }
}

/// Puts the players' names and the status line where they go over the canvas, sized with it.
fn place_text(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mut texts: Query<(&mut Node, &mut TextFont, Option<&Label>, Has<Status>)>,
) {
    let (node, transform) = *canvas;
    let scale_factor = window.scale_factor();
    // Logical pixels per canvas pixel, and where the canvas's corner is.
    let scale = node.size().x / CANVAS.x as f32 / scale_factor;
    let corner = (transform.translation - node.size() / 2.0) / scale_factor;
    let at = |x: f32, y: f32| corner + Vec2::new(x, y) * scale;
    for (mut text_node, mut font, label, is_status) in &mut texts {
        let (size, left, right, width) = match (label, is_status) {
            (Some(Label(0)), _) => (LABEL_SIZE, Some(at(LABEL_EDGES[0], 0.0).x), None, None),
            (Some(_), _) => {
                let edge = at(LABEL_EDGES[1], 0.0).x;
                (LABEL_SIZE, None, Some(window.width() - edge), None)
            }
            (None, true) => {
                let (from, to) = STATUS_SPAN;
                (
                    STATUS_SIZE,
                    Some(at(from, 0.0).x),
                    None,
                    Some((to - from) * scale),
                )
            }
            (None, false) => continue,
        };
        let font_size = FontSize::Px((size * scale).max(SMALLEST_TEXT).round());
        if font.font_size != font_size {
            font.font_size = font_size;
        }
        let top = Val::Px(at(0.0, LABEL_TOP).y.round());
        let left = left.map_or(Val::Auto, |x| Val::Px(x.round()));
        let right = right.map_or(Val::Auto, |x| Val::Px(x.round()));
        let width = width.map_or(Val::Auto, |w| Val::Px(w.round()));
        if text_node.top != top || text_node.left != left || text_node.right != right {
            text_node.top = top;
            text_node.left = left;
            text_node.right = right;
            text_node.width = width;
        }
    }
}

/// The New rack button in the settings: every ball back, wherever the game is at.
fn rack_again(mut asked: MessageReader<NewRack>, game: Option<ResMut<Game>>) {
    if asked.read().count() == 0 {
        return;
    }
    if let Some(mut game) = game {
        game.start_over();
    }
}

fn show_status(
    game: Res<Game>,
    touch: Res<Touch>,
    mut status: Single<&mut Text, (With<Status>, Without<Label>)>,
    mut labels: Query<(&Label, &mut Text, &mut TextColor), Without<Status>>,
) {
    let placing = matches!(game.cue, Cue::Placing { .. }) && game.my_turn();
    let names = game.names();
    // The game waits on a player who left their seat, on their turn.
    let waiting_on = match (&game.match_seat, &game.opponent) {
        (Some((me, _)), None) if game.rules.turn != *me && game.rules.win.is_none() => {
            Some(&names[game.rules.turn])
        }
        _ => None,
    };
    let back = if touch.is_on() { "Leave" } else { "Esc" };
    let line = if game.watching && game.table_id.as_deref().map_or(0, seats::seated) == 0 {
        format!("Nobody is playing right now. {back} goes back.")
    } else if game.watching && game.waiting_for_start {
        "Watching. Waiting for the game...".to_string()
    } else if game.waiting_for_start {
        "Joining the game...".to_string()
    } else if let Some(name) = waiting_on {
        format!("Waiting for {name} to come back. New rack (in /settings) starts over.")
    } else {
        // Against someone, only player 1 starts the next game.
        let can_start_over = game.may_start_over();
        let mut line = describe(
            &game.rules,
            &names,
            placing,
            touch.is_on(),
            can_start_over,
            game.watching,
        );
        if game.rules.win.is_some() && !can_start_over && !game.watching {
            line.push_str(&format!(" Waiting for {} to rack again.", names[0]));
        }
        line
    };
    if status.0 != line {
        status.0 = line;
    }
    let rules = &game.rules;
    let lit = lit_player(rules);
    for (Label(player), mut text, mut color) in &mut labels {
        let group = match rules.group_of(*player) {
            Some(Group::Solids) => "solids",
            Some(Group::Stripes) => "stripes",
            None => "table open",
        };
        let name = &names[*player];
        let mut line = format!("{name} · {group}");
        if *player == lit && rules.win.is_none() {
            if rules.breaking {
                line = format!("{name} · to break");
            } else if matches!(game.cue, Cue::Placing { .. }) {
                line.push_str(" · ball in hand");
            }
        }
        if rules.win.is_some_and(|win| win.winner == *player) {
            line.push_str(" · wins!");
        }
        if text.0 != line {
            text.0 = line;
        }
        let shade = if *player == lit {
            Color::WHITE
        } else {
            Color::srgb(0.55, 0.55, 0.55)
        };
        color.set_if_neq(TextColor(shade));
    }
}

/// What the status line says: what the last shot did (and how to put the cue ball down, with
/// ball in hand), or who won; how to play before anything has happened, or that one is
/// `watching`.
fn describe(
    game: &rules::Game,
    names: &[String; 2],
    placing: bool,
    touch: bool,
    can_start_over: bool,
    watching: bool,
) -> String {
    let name = |player: usize| names[player].clone();
    let group_name = |group: Group| match group {
        Group::Solids => "solids",
        Group::Stripes => "stripes",
    };
    if let Some(win) = game.win {
        let loser = name(1 - win.winner);
        let why = match win.how {
            WinBy::Eight => "the 8 is down".to_string(),
            WinBy::EarlyEight => format!("{loser} sank the 8 too early"),
            WinBy::FoulOnEight => format!("{loser} fouled on the 8"),
        };
        let again = match (can_start_over, touch) {
            (false, _) => "",
            (true, true) => " Tap for a new game.",
            (true, false) => " Click for a new game.",
        };
        return format!("{} wins: {why}!{again}", name(win.winner));
    }
    let mut said = Vec::new();
    if let Some(last) = game.last {
        let shooter = if last.again { game.turn } else { 1 - game.turn };
        match last.foul {
            Some(Foul::Scratch) => said.push("Foul: the cue ball went in.".to_string()),
            Some(Foul::NoHit) => said.push("Foul: no ball hit.".to_string()),
            Some(Foul::WrongBall(8)) => said.push("Foul: the 8 was hit first.".to_string()),
            Some(Foul::WrongBall(number)) => {
                said.push(format!("Foul: the {number} was hit first."))
            }
            None => {}
        }
        if let Some(group) = last.took {
            said.push(format!("{} takes {}.", name(shooter), group_name(group)));
        }
        if last.eight_respotted {
            said.push("The 8 goes back on its spot.".to_string());
        }
    }
    if placing {
        said.push(if touch {
            "Drag the cue ball, lift to put it down.".to_string()
        } else {
            "Move the cue ball, click to put it down.".to_string()
        });
    } else if game.last.is_some_and(|last| last.again) && !game.breaking {
        said.push(format!("{} goes again.", name(game.turn)));
    }
    if said.is_empty() {
        // Nothing has happened yet: how to play, or that one only watches.
        said.push(match (watching, touch) {
            (true, true) => "Watching.".to_string(),
            (true, false) => "Watching. Esc leaves.".to_string(),
            (false, true) => {
                "Your finger aims. Hold to pull the cue back, lift to shoot.".to_string()
            }
            (false, false) => {
                "The mouse aims. Hold to pull the cue back, let go to shoot. Esc leaves."
                    .to_string()
            }
        });
    }
    said.join(" ")
}

fn leave(keys: Res<ButtonInput<KeyCode>>, touch: Res<Touch>, mut mode: ResMut<NextState<Mode>>) {
    if keys.just_pressed(KeyCode::Escape) || touch.tapped(TouchButton::Leave) {
        mode.set(Mode::Walking);
    }
}

fn hide_table(
    mut commands: Commands,
    mut game: ResMut<Game>,
    overlays: Query<Entity, With<Overlay>>,
) {
    // Up from the table: the game stays as it is for whoever is left there, their seat
    // waiting. This player carries on from it if they come back alone, playing both sides, and
    // gets it from the other player if they're still there.
    seats::stand();
    game.table_id = None;
    game.opponent = None;
    game.match_seat = None;
    game.alone_here = false;
    game.owe_game.clear();
    game.streamed_to.clear();
    game.watching = false;
    game.waiting_for_start = false;
    game.their_settings = None;
    game.their_result = None;
    // A shot half taken is put down, and one rolling finishes where nobody sees.
    match game.cue {
        Cue::Rolling => {
            while game.table.is_moving() {
                game.table.step();
            }
            game.finish_shot();
        }
        Cue::Waiting => game.finish_shot(),
        Cue::Pulling(_) | Cue::Striking { .. } => game.cue = Cue::Aiming,
        Cue::Placing { .. } => game.cue = Cue::Placing { held: false },
        Cue::Aiming | Cue::Over => {}
    }
    game.dropping.clear();
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
}

impl Pixels {
    /// A ball with its middle at `center`, on whole pixels, in `shape`, its colors scaled by
    /// `light` (1 as it is, less as it sinks into a pocket).
    fn ball(&mut self, number: u8, center: Vec2, shape: &[&str], light: f32) {
        let size = shape.len();
        let corner = (center - size as f32 / 2.0).round().as_ivec2();
        let color = match number {
            0 => WHITE,
            n => {
                // Stripes have the colors of the solids eight below them.
                let solid = if n <= 8 { n } else { n - 8 };
                let [r, g, b] = BALL_COLORS[solid as usize - 1];
                [r, g, b, 255]
            }
        };
        let striped = number >= 9;
        // The white bands are the top and bottom two rows of seven, one of five, none of three.
        let band_rows = (size - 1) / 3;
        for (y, row) in shape.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell != b'#' {
                    continue;
                }
                let band = striped && (y < band_rows || y >= size - band_rows);
                let pixel = match (x, y) {
                    (2, 1) if size == BALL_SHAPE.len() => SHINE,
                    _ if band => WHITE,
                    _ => color,
                };
                let [r, g, b, a] = pixel;
                let dim = |c: u8| (c as f32 * light) as u8;
                self.set(
                    corner.x + x as i32,
                    corner.y + y as i32,
                    [dim(r), dim(g), dim(b), a],
                );
            }
        }
    }

    /// The cue, its tip at `tip` and running back along `back`.
    fn cue(&mut self, tip: Vec2, back: Vec2) {
        let mut along = 0.0;
        while along < CUE_LENGTH {
            let at = tip + back * along;
            let (color, thick) = match along {
                a if a < 2.0 => (CUE_TIP, false),
                a if a < CUE_LENGTH * 0.6 => (CUE_SHAFT, false),
                _ => (CUE_BUTT, true),
            };
            if thick {
                let corner = (at - 0.5).floor().as_ivec2();
                self.rect(corner, corner + 2, color);
            } else {
                self.set(at.x.floor() as i32, at.y.floor() as i32, color);
            }
            along += 0.35;
        }
    }
}

/// The table without balls: rails, cushions, felt, the pockets cut into them, and the sights
/// on the rails.
fn draw_table(pockets: &[Pocket; 6]) -> Pixels {
    let mut pixels = Pixels::new(CANVAS);
    let felt_from = FELT.as_ivec2();
    let felt_to = felt_from + IVec2::new(WIDTH as i32, HEIGHT as i32);
    let rail = RAIL as i32;
    let (rail_from, rail_to) = (felt_from - rail, felt_to + rail);
    pixels.rect(rail_from, rail_to, RAIL_EDGE);
    pixels.rect(rail_from + 1, rail_to - 1, RAIL_COLOR);
    // Light catches the rails' top edges.
    pixels.rect(
        rail_from + 1,
        IVec2::new(rail_to.x - 1, rail_from.y + 2),
        RAIL_LIGHT,
    );
    pixels.rect(felt_from - CUSHION, felt_to + CUSHION, CUSHION_COLOR);
    // The jaws are cushion, cut at an angle, running back into the rail. Before the felt, so
    // the strips' round ends at the points don't stick out onto it.
    for pocket in pockets {
        for (point, end) in pocket.points.iter().zip(pocket.jaw_ends) {
            pixels.thick_line(*point + FELT, end + FELT, CUSHION as f32, CUSHION_COLOR);
        }
    }
    pixels.rect(felt_from, felt_to, FELT_COLOR);
    // Diamonds along the rails, every eighth of the length and quarter of the width.
    let rail_middle = (CUSHION + (rail - CUSHION) / 2) as f32 + 0.5;
    for i in (1..8).filter(|&i| i != 4) {
        let x = FELT.x + WIDTH * i as f32 / 8.0;
        pixels.set(x as i32, (FELT.y - rail_middle) as i32, SIGHT);
        pixels.set(x as i32, (FELT.y + HEIGHT + rail_middle) as i32, SIGHT);
    }
    for i in 1..4 {
        let y = FELT.y + HEIGHT * i as f32 / 4.0;
        pixels.set((FELT.x - rail_middle) as i32, y as i32, SIGHT);
        pixels.set((FELT.x + WIDTH + rail_middle) as i32, y as i32, SIGHT);
    }
    for pocket in pockets {
        // The felt between the jaws, out to the hole.
        let [a, b] = pocket.points.map(|point| point + FELT);
        let [a_end, b_end] = pocket.jaw_ends.map(|end| end + FELT);
        pixels.polygon(&[a, a_end, b_end, b], FELT_COLOR);
        let radius = pocket.hole_radius - HOLE_DRAWN_SMALLER;
        pixels.disc(pocket.hole + FELT, radius, POCKET);
    }
    pixels
}

/// For browser tests (testing.rs): what the game is doing.
#[cfg(feature = "test-hooks")]
fn test_hooks(game: Res<Game>) {
    use serde_json::json;

    let cue = match game.cue {
        Cue::Aiming => "aiming",
        Cue::Pulling(_) => "pulling",
        Cue::Striking { .. } => "striking",
        Cue::Rolling => "rolling",
        Cue::Over => "over",
        Cue::Placing { .. } => "placing",
        Cue::Waiting => "waiting",
    };
    let balls: Vec<_> = game
        .table
        .balls
        .iter()
        .map(|ball| {
            json!({
                "number": ball.number,
                "at": [ball.position.x, ball.position.y],
                "down": ball.pocketed,
            })
        })
        .collect();
    crate::testing::report(
        "pool",
        json!({
            "cue": cue,
            "turn": game.rules.turn,
            "my_turn": game.my_turn(),
            "win": game.rules.win,
            "balls": balls,
            "seat": game.match_seat.as_ref().map(|(me, _)| *me),
            "names": game.names(),
            "opponent": game.opponent.is_some(),
            "waiting_for_start": game.waiting_for_start,
            "watching": game.watching,
            "watchers": game.table_id.as_deref().map_or(0, seats::watching),
        }),
    );
}
