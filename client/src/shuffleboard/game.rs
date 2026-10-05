//! Playing shuffleboard: the long table seen from above, upright, the end the pucks are thrown
//! from at the bottom and the scoring end at the top, drawn pixel by pixel into a small image
//! that fills the screen at a whole-number zoom, as the other tables are. The table is much
//! longer than the screen is tall, so the view slides along it: it waits at the near end for a
//! throw, follows the puck down, and looks at the far end once everything stops; a map of the
//! whole table, by its side, shows where every puck is. Holding Up (or W) looks down the table
//! between throws.
//!
//! A throw is a flick: grab the waiting puck, slide it about behind the foul line, and let go
//! while moving it. The puck goes as fast as the hand was going (a little faster: `/settings`
//! has the flick's strength), and crossing the foul line lets it go. Each player throws four
//! pucks a round, in turn; once all eight have stopped, the player with the puck furthest down
//! scores each of theirs further than the other's best (the shuffleboard crate). First to 15.
//! Alone at the table, the player throws for both sides; when someone is at the other seat,
//! they play each other (online.rs), the first seat throwing red. Esc goes back to the bar, and
//! New game in `/settings` starts over.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use shuffleboard::{
    FOUL_LINE, Fall, Hand, LENGTH, LINES, PUCK_RADIUS, PUCKS_EACH, Puck, Table, WIDTH,
    WINNING_SCORE,
};

use super::online::Message;
use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::pixels::Pixels;
use crate::seats;
use crate::settings::{Knob, NewShuffleboardGame, Settings};
use crate::touch::{self, Touch, TouchButton};

/// The image the table is drawn into.
const CANVAS: UVec2 = UVec2::new(320, 180);
/// Where the table's left edge is across the canvas, and the canvas row the view's `y` (along
/// the table) is drawn on.
const TABLE_LEFT: f32 = 136.0;
const NEAR_ROW: f32 = 170.0;
/// The gutters either side of the playing surface, and the rails outside them (pixels).
const GUTTER: i32 = 6;
const RAIL: i32 = 4;
/// Past the far end, the pit pucks fall into and the rail behind it; before the near end, the
/// table's end (pixels).
const PIT: i32 = 8;
const FAR_RAIL: i32 = 6;
const NEAR_END: i32 = 10;
/// The whole table, rails and all, as drawn once (`draw_board`).
const BOARD: IVec2 = IVec2::new(
    WIDTH as i32 + 2 * (GUTTER + RAIL),
    PIT + FAR_RAIL + LENGTH as i32 + NEAR_END,
);
/// How far along the table the view goes: from the near end, under the thrower's hand, to the
/// far end at the top.
const VIEW_NEAR: f32 = -12.0;
const VIEW_FAR: f32 = 328.0;
/// How quickly the view slides to where it's going (per second).
const VIEW_SPEED: f32 = 7.0;
/// While a puck slides, the view keeps it this far from the canvas's bottom row.
const FOLLOW_AT: f32 = 110.0;
/// The map of the whole table: its top-left corner, and how many pixels of the table to each of
/// its own.
const MAP: Vec2 = Vec2::new(206.0, 6.0);
const MAP_SCALE: i32 = 3;
/// Where the waiting puck sits, and how near it a press grabs it (table pixels).
const READY: Vec2 = Vec2::new(WIDTH / 2.0, 24.0);
const GRAB_REACH: f32 = PUCK_RADIUS + 4.0;
/// Let go slower than this up the table (pixels per second), the puck is put down, not thrown.
const SOFTEST_THROW: f32 = 20.0;
/// How long the view looks at the far end once everything stops, and how long a round's score
/// shows (seconds; a click moves on).
const LOOK_FOR: f32 = 1.0;
const ROUND_SHOWN_FOR: f32 = 3.0;
/// Arrived with someone already at the table, how long to wait for their game before player 1
/// starts a new one (neither had one), in seconds.
const NO_GAME_COMING: f32 = 1.5;
/// How often the waiting puck goes to the other player while the thrower slides it about, in
/// seconds.
const HOLD_SEND_EVERY: f32 = 1.0 / 15.0;

/// Players' names, by their pucks' colour, while one player throws for both.
const NAMES: [&str; 2] = ["Red", "Blue"];
/// The texts: the players' panels, left of the table, and the status line, right of the map
/// (from their left edges, how wide, and their tops, in canvas pixels).
const PANELS: [(f32, f32, f32); 2] = [(16.0, 104.0, 36.0), (16.0, 104.0, 96.0)];
const STATUS: (f32, f32, f32) = (236.0, 80.0, 60.0);
/// Where each panel's pucks left are drawn (the middle of the first; canvas pixels).
const PUCKS_LEFT: [Vec2; 2] = [Vec2::new(55.0, 66.0), Vec2::new(55.0, 126.0)];
/// Text heights, in canvas pixels, and the smallest they get, in logical pixels.
const NAME_SIZE: f32 = 6.0;
const STATUS_SIZE: f32 = 3.5;
const SMALLEST_TEXT: f32 = 8.0;

const WOOD: [u8; 4] = [120, 72, 38, 255];
const WOOD_DARK: [u8; 4] = [86, 50, 26, 255];
const WOOD_LIGHT: [u8; 4] = [150, 94, 52, 255];
const GUTTER_COLOR: [u8; 4] = [52, 30, 16, 255];
const PIT_COLOR: [u8; 4] = [36, 22, 12, 255];
const SURFACE: [u8; 4] = [226, 198, 146, 255];
/// The grain along the surface, a stripe now and then.
const GRAIN: [u8; 4] = [216, 186, 134, 255];
const LINE: [u8; 4] = [150, 96, 54, 255];
const FOUL: [u8; 4] = [180, 60, 50, 255];
const NUMBER: [u8; 4] = [176, 128, 80, 255];
const STEEL: [u8; 4] = [176, 180, 190, 255];
const STEEL_DARK: [u8; 4] = [110, 112, 122, 255];
const SHINE: [u8; 4] = [255, 255, 255, 255];
/// Each player's pucks' caps.
const CAPS: [[u8; 4]; 2] = [[206, 52, 52, 255], [52, 96, 214, 255]];
/// Round the pucks that score, while a round's score shows; the part of the table on screen,
/// on the map; the arrow at whose throw it is.
const COUNTED: [u8; 4] = [255, 240, 120, 255];
const IN_VIEW: [u8; 4] = [240, 240, 240, 255];
const TURN_ARROW: [u8; 4] = [255, 255, 255, 255];
const EMPTY_SLOT: [u8; 4] = [70, 70, 80, 255];
/// Behind the players' panels and the status line (canvas pixels, corner to corner).
const BACKDROP: [u8; 4] = [14, 12, 18, 255];
const BACKDROPS: [(IVec2, IVec2); 2] = [
    (IVec2::new(10, 28), IVec2::new(126, 140)),
    (IVec2::new(232, 52), IVec2::new(318, 120)),
];

/// The shuffleboard table the player is at, as the room names it (online::table_id, seats.rs).
/// Set before switching to `Mode::Shuffleboard`.
#[derive(Resource)]
pub struct AtTable(pub String);

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Shuffleboard), show_table)
            .add_systems(
                Update,
                (
                    fit_canvas,
                    sync,
                    play,
                    send_hold,
                    look,
                    draw,
                    show_text,
                    place_text,
                    leave.run_if(chat_closed),
                )
                    .chain()
                    .run_if(in_state(Mode::Shuffleboard)),
            )
            .add_systems(Update, new_game)
            .add_systems(OnExit(Mode::Shuffleboard), hide_table);
        #[cfg(feature = "test-hooks")]
        app.add_systems(
            Update,
            test_hooks.after(play).run_if(in_state(Mode::Shuffleboard)),
        );
    }
}

/// Where the game is at.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    /// A puck waits behind the foul line for whoever's turn it is.
    Throwing,
    /// Thrown: everything slides until it stops.
    Sliding,
    /// The other player's throw has stopped here, and waits for where it stopped on their table.
    Waiting,
    /// Everything has stopped; the view looks at the far end for a moment.
    Stopped { since: f32 },
    /// The round is over, its score showing.
    Scored {
        since: f32,
        scored: Option<(usize, u32)>,
    },
}

/// The waiting puck in the player's hand: how far it is from the pointer, and where the pointer
/// has been lately, for how fast it was going.
struct Grab {
    offset: Vec2,
    hand: Hand,
}

/// The game, kept between visits to the table.
#[derive(Resource)]
struct Game {
    table: Table,
    rules: shuffleboard::Game,
    phase: Phase,
    /// Where the waiting puck is.
    ready: Vec2,
    grab: Option<Grab>,
    /// Pucks off the table this round, where they lie: in a gutter, or in the pit past the end.
    fallen: Vec<Puck>,
    /// How far along the table the view is: the `y` drawn on `NEAR_ROW`.
    view: f32,
    /// The pointer was down last frame: only a fresh press grabs, or moves the game on.
    was_pressed: bool,
    /// The table, as the room names it, while the player is at it.
    table_id: Option<String>,
    /// The other player at the table, when there is one: the game is between them.
    opponent: Option<Opponent>,
    /// Just arrived at a table with someone at it, and waiting for the game from them (or, both
    /// new to it, for player 1 to start one), for `waiting_since` seconds.
    waiting_for_start: bool,
    waiting_since: f32,
    /// This player's seat in a game against someone, and both players' names: kept while the
    /// other seat is empty, so the game waits for them.
    match_seat: Option<(usize, [String; 2])>,
    /// The room last had this player at the table on their own: whoever arrives next gets the
    /// game from them.
    alone_here: bool,
    /// The game is owed to this player, who just arrived: it goes between throws.
    owe_game: Option<u32>,
    /// The settings the other player threw with, while their throw slides here.
    their_settings: Option<shuffleboard::Settings>,
    /// Where the other player's throw ended on their table (the pucks on it, and those off it),
    /// when that came in before it stopped here.
    their_result: Option<(Vec<Puck>, Vec<Puck>)>,
    /// The waiting puck as last sent to the other player, and how long ago.
    sent: (Vec2, f32),
}

/// Who the player is playing against.
struct Opponent {
    /// The player's own seat (0 for player 1), and the other player's id in the room.
    me: usize,
    id: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Game {
    fn new(first: usize) -> Self {
        Self {
            table: Table::new(shuffleboard::Settings::default()),
            rules: shuffleboard::Game::new(first),
            phase: Phase::Throwing,
            ready: READY,
            grab: None,
            fallen: Vec::new(),
            view: VIEW_NEAR,
            was_pressed: true,
            table_id: None,
            opponent: None,
            waiting_for_start: false,
            waiting_since: 0.0,
            match_seat: None,
            alone_here: false,
            owe_game: None,
            their_settings: None,
            their_result: None,
            sent: (READY, 0.0),
        }
    }

    /// A new game, `first` throwing first: the table cleared, and the scores back to nothing.
    fn begin(&mut self, first: usize) {
        self.table.pucks.clear();
        self.rules = shuffleboard::Game::new(first);
        self.phase = Phase::Throwing;
        self.ready = READY;
        self.grab = None;
        self.fallen.clear();
        self.waiting_for_start = false;
        self.their_settings = None;
        self.their_result = None;
    }

    /// A new game, `first` throwing first. Against someone, only player 1 starts games, and
    /// tells player 2. Alone at a game whose other seat is empty, it ends: the player takes both
    /// sides again.
    fn start_over(&mut self, first: usize) {
        match &self.opponent {
            None => {
                self.match_seat = None;
                self.begin(first);
            }
            Some(opponent) if opponent.me == 0 => {
                let id = opponent.id;
                self.begin(first);
                self.send(id, &Message::Start { first });
            }
            Some(_) => {}
        }
    }

    /// Whether this player may start the next game: alone, or as player 1 against someone.
    fn may_start_over(&self) -> bool {
        self.opponent
            .as_ref()
            .is_none_or(|opponent| opponent.me == 0)
    }

    fn send(&self, to: u32, message: &Message) {
        if let Some(table) = &self.table_id {
            seats::send(table, to, message);
        }
    }

    /// Whether this player throws next: always when alone, on their turn in a game against
    /// someone (who may have left their seat: then it waits for them).
    fn my_turn(&self) -> bool {
        !self.waiting_for_start
            && self
                .match_seat
                .as_ref()
                .is_none_or(|(me, _)| self.rules.turn() == *me)
    }

    /// Both players' names: their names in the room in a game against someone, their pucks'
    /// colours when one player throws for both.
    fn names(&self) -> [String; 2] {
        match &self.match_seat {
            Some((_, names)) => names.clone(),
            None => NAMES.map(String::from),
        }
    }

    /// The whole game, for a player arriving: every puck, and where the game is at.
    fn whole_game(&self) -> Message {
        Message::Sync {
            pucks: self.table.pucks.clone(),
            fallen: self.fallen.clone(),
            rules: self.rules.clone(),
        }
    }

    /// Carries on the game as the other player had it.
    fn take_whole_game(&mut self, pucks: Vec<Puck>, fallen: Vec<Puck>, rules: shuffleboard::Game) {
        self.table.pucks = pucks;
        self.fallen = fallen;
        self.rules = rules;
        self.phase = Phase::Throwing;
        self.ready = READY;
        self.grab = None;
        self.waiting_for_start = false;
        self.their_settings = None;
        self.their_result = None;
    }

    /// Where the other player's throw ended on their table: everything goes there.
    fn take_their_result(&mut self, (pucks, fallen): (Vec<Puck>, Vec<Puck>)) {
        self.table.pucks = pucks;
        self.fallen = fallen;
        self.their_settings = None;
        self.phase = Phase::Stopped { since: 0.0 };
    }

    /// Everything slides on until it stops, at once.
    fn finish_slide(&mut self) {
        let fell = self.table.settle();
        self.fell(fell);
        self.table.clear_short();
        self.their_settings = None;
        self.phase = Phase::Stopped { since: 0.0 };
    }

    /// Once everything has stopped and been looked at: the round's score, or the next throw.
    fn after_stop(&mut self) {
        self.phase = if self.rules.round_over() {
            Phase::Scored {
                since: 0.0,
                scored: self.table.round_score(),
            }
        } else {
            Phase::Throwing
        };
        self.ready = READY;
    }

    /// Once the round's score has shown: it counts, and the next round starts.
    fn after_score(&mut self) {
        self.rules.score_round(&mut self.table);
        self.fallen.clear();
        self.ready = READY;
        self.phase = Phase::Throwing;
    }

    /// Finishes whatever this table is still showing (a throw sliding, the look at the far end,
    /// a round's score), for the other player's next throw: they've moved on to it.
    fn catch_up(&mut self) {
        loop {
            match self.phase {
                Phase::Throwing => return,
                Phase::Sliding | Phase::Waiting => match self.their_result.take() {
                    Some(result) => self.take_their_result(result),
                    None => self.finish_slide(),
                },
                Phase::Stopped { .. } => self.after_stop(),
                Phase::Scored { .. } => self.after_score(),
            }
        }
    }

    /// The view, on whole pixels.
    fn view(&self) -> f32 {
        self.view.round()
    }

    /// Where a point on the table is drawn on the canvas.
    fn to_canvas(&self, at: Vec2) -> Vec2 {
        Vec2::new(TABLE_LEFT + at.x, NEAR_ROW - (at.y - self.view()))
    }

    fn to_table(&self, at: Vec2) -> Vec2 {
        Vec2::new(at.x - TABLE_LEFT, self.view() + NEAR_ROW - at.y)
    }

    /// Puts the pucks that went off the table where they lie: in the gutter they fell in, or
    /// in the pit.
    fn fell(&mut self, fell: Vec<shuffleboard::Fell>) {
        for fell in fell {
            let mut puck = fell.puck;
            let Vec2 { x, y } = puck.position;
            puck.velocity = Vec2::ZERO;
            puck.position = match fell.over {
                Fall::Gutter => Vec2::new(
                    if x < 0.0 {
                        -GUTTER as f32 / 2.0
                    } else {
                        WIDTH + GUTTER as f32 / 2.0
                    },
                    y.clamp(0.0, LENGTH),
                ),
                // Off the near end, back past the thrower: gone.
                Fall::End if y < 0.0 => continue,
                Fall::End => Vec2::new(x.clamp(0.0, WIDTH), LENGTH + PIT as f32 / 2.0),
            };
            self.fallen.push(puck);
        }
    }
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
    Status,
}

fn show_table(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Option<ResMut<Game>>,
    touch: Res<Touch>,
) {
    match game {
        // Picked up where it was left, but not with a puck in hand.
        Some(mut game) => {
            game.grab = None;
            game.was_pressed = true;
        }
        None => commands.init_resource::<Game>(),
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
            text(Says::Status, STATUS, STATUS_SIZE),
        ],
    ));
    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), as at the other tables.
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

/// Grabs, slides and throws the waiting puck, runs the table while anything moves, and moves
/// the game on: the next throw, the round's score, the next round, a new game.
#[allow(clippy::too_many_arguments)]
fn play(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touch: Res<Touch>,
    chat: Res<Chat>,
    help: Res<Help>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut game: ResMut<Game>,
) {
    let game = &mut *game;
    let seconds = time.delta_secs();
    // The other player's throw slides with their settings.
    game.table.settings = game
        .their_settings
        .unwrap_or_else(|| settings.shuffleboard());
    let pressed = mouse.pressed(MouseButton::Left) || touch.finger().is_some();
    let busy = chat.is_open() || help.is_open() || settings.is_open();
    // As if held all along while a panel is up, so the click that closes it does nothing here.
    let fresh_press = pressed && !game.was_pressed && !busy;
    game.was_pressed = pressed || busy;

    let (node, transform) = *canvas;
    // Until the canvas has been laid out it has no size, and nowhere on it to point at.
    let pointer = touch
        .finger()
        .or(window.cursor_position())
        .filter(|_| node.size().min_element() > 0.0)
        .map(|at| {
            let size = node.size();
            let corner = transform.translation - size / 2.0;
            game.to_table((at * window.scale_factor() - corner) / size * CANVAS.as_vec2())
        })
        .filter(|at| at.is_finite());

    match game.phase {
        Phase::Throwing => {
            if let Some(winner) = game.rules.win {
                // A new game, the winner throwing first, if this player may start one.
                if fresh_press && game.may_start_over() {
                    game.start_over(winner);
                }
                return;
            }
            if !game.my_turn() {
                // The other player's throw: their puck moves as they say (sync).
                game.grab = None;
                return;
            }
            throw(
                game,
                pointer,
                pressed && !busy,
                fresh_press,
                &settings,
                &time,
            );
        }
        Phase::Sliding => {
            let fell = game.table.advance(seconds);
            game.fell(fell);
            if game.table.moving() {
                return;
            }
            if game.their_settings.is_some() {
                // The other player's throw: it ends where it ended on their table.
                match game.their_result.take() {
                    Some(result) => game.take_their_result(result),
                    None => game.phase = Phase::Waiting,
                }
            } else {
                game.table.clear_short();
                if let Some(opponent) = &game.opponent {
                    let settled = Message::Settled {
                        pucks: game.table.pucks.clone(),
                        fallen: game.fallen.clone(),
                    };
                    game.send(opponent.id, &settled);
                }
                game.phase = Phase::Stopped { since: 0.0 };
            }
        }
        Phase::Waiting => {}
        Phase::Stopped { since } => {
            let since = since + seconds;
            game.phase = Phase::Stopped { since };
            if since >= LOOK_FOR || fresh_press {
                game.after_stop();
            }
        }
        Phase::Scored { since, scored } => {
            let since = since + seconds;
            game.phase = Phase::Scored { since, scored };
            if since >= ROUND_SHOWN_FOR || fresh_press {
                game.after_score();
            }
        }
    }
}

/// The waiting puck: a press on it picks it up, it follows the pointer behind the foul line,
/// and letting go (or reaching the line) throws it as fast as the pointer was going.
fn throw(
    game: &mut Game,
    pointer: Option<Vec2>,
    pressed: bool,
    fresh_press: bool,
    settings: &Settings,
    time: &Time,
) {
    let now = time.elapsed_secs();
    // Only with the view at the near end, where the puck is.
    let settled = (game.view - VIEW_NEAR).abs() < 0.5;
    if game.grab.is_none()
        && fresh_press
        && settled
        && let Some(at) = pointer.filter(|at| at.distance(game.ready) <= GRAB_REACH)
    {
        game.grab = Some(Grab {
            offset: game.ready - at,
            hand: Hand::new(now, at),
        });
    }
    let Some(grab) = &mut game.grab else {
        return;
    };
    // Not on the frame it lets go: where the pointer goes after that isn't the throw.
    if pressed && let Some(at) = pointer {
        grab.hand.moved(now, at);
    }
    let wanted = pointer.map_or(game.ready, |at| at + grab.offset);
    game.ready = wanted.clamp(
        Vec2::splat(PUCK_RADIUS),
        Vec2::new(WIDTH - PUCK_RADIUS, FOUL_LINE),
    );
    if pressed && wanted.y <= FOUL_LINE {
        return;
    }
    // Let go, or over the line.
    let velocity = grab.hand.velocity(now) * settings.get(Knob::Flick);
    game.grab = None;
    if velocity.y < SOFTEST_THROW {
        // Put down: it waits where it is.
        return;
    }
    launch(game, velocity);
}

/// Throws the waiting puck from where it is at `velocity`, telling the other player.
fn launch(game: &mut Game, velocity: Vec2) {
    if let Some(opponent) = &game.opponent {
        let throw = Message::Throw {
            from: game.ready,
            velocity,
            settings: game.table.settings,
        };
        game.send(opponent.id, &throw);
    }
    let player = game.rules.turn();
    game.table.throw(player, game.ready, velocity);
    game.rules.threw();
    game.phase = Phase::Sliding;
}

/// Sits at the table in the room, starts a game when someone arrives at the other seat (player
/// 1 starts it, or the one already there hands theirs over), goes back to both sides when they
/// leave, and follows what they do on their turn.
fn sync(at: Option<Res<AtTable>>, time: Res<Time>, mut game: ResMut<Game>) {
    let Some(at) = at else {
        return;
    };
    let game = &mut *game;
    if game.table_id.as_deref() != Some(at.0.as_str()) {
        game.table_id = Some(at.0.clone());
        seats::sit(&at.0);
    }

    let seats = seats::seats_at(&at.0);
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
            game.grab = None;
            if was_alone {
                // They arrived with this player already here: they get the game as it is.
                game.owe_game = Some(id);
            } else {
                // Arrived with them already here: the game comes from them.
                game.waiting_for_start = true;
                game.waiting_since = 0.0;
            }
        }
        (Some(_), None) => {
            // They left: the game waits for them as it is, their seat empty. Their throw, if
            // it's still sliding, ends here.
            game.opponent = None;
            game.waiting_for_start = false;
            game.owe_game = None;
            game.their_settings = None;
            game.their_result = None;
            if game.phase == Phase::Waiting {
                game.table.clear_short();
                game.phase = Phase::Stopped { since: 0.0 };
            }
        }
        (None, None) => {}
    }

    // Both new to the table, nobody had a game to give: player 1 starts one.
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
    // The game owed to someone who arrived goes between throws.
    if game.phase == Phase::Throwing
        && let Some(id) = game.owe_game.take()
    {
        let whole = game.whole_game();
        game.send(id, &whole);
    }

    let messages = seats::take_messages::<Message>(&at.0);
    let Some(id) = game.opponent.as_ref().map(|opponent| opponent.id) else {
        return;
    };
    for (_, message) in messages.into_iter().filter(|(from, _)| *from == id) {
        match message {
            Message::Start { first } => game.begin(first),
            Message::Sync {
                pucks,
                fallen,
                rules,
            } => game.take_whole_game(pucks, fallen, rules),
            Message::Hold { at } => {
                game.catch_up();
                if !game.my_turn() {
                    game.ready = at;
                }
            }
            Message::Throw {
                from,
                velocity,
                settings,
            } => {
                game.catch_up();
                if !game.my_turn() && game.rules.win.is_none() {
                    game.their_settings = Some(settings);
                    game.table.settings = settings;
                    let player = game.rules.turn();
                    game.table.throw(player, from, velocity);
                    game.rules.threw();
                    game.ready = from;
                    game.phase = Phase::Sliding;
                }
            }
            Message::Settled { pucks, fallen } => {
                if game.phase == Phase::Waiting {
                    game.take_their_result((pucks, fallen));
                } else if game.their_settings.is_some() {
                    game.their_result = Some((pucks, fallen));
                }
            }
        }
    }
}

/// On this player's turn against someone: where they have the waiting puck goes to the other
/// player as it moves, a few times a second.
fn send_hold(time: Res<Time>, mut game: ResMut<Game>) {
    let Some(id) = game.opponent.as_ref().map(|opponent| opponent.id) else {
        return;
    };
    if !game.my_turn() || game.phase != Phase::Throwing || game.rules.win.is_some() {
        return;
    }
    game.sent.1 += time.delta_secs();
    if game.ready != game.sent.0 && game.sent.1 >= HOLD_SEND_EVERY {
        let at = game.ready;
        game.sent = (at, 0.0);
        game.send(id, &Message::Hold { at });
    }
}

/// The New game button in the settings: a new game, wherever this one is at.
fn new_game(mut asked: MessageReader<NewShuffleboardGame>, game: Option<ResMut<Game>>) {
    if asked.read().count() == 0 {
        return;
    }
    if let Some(mut game) = game {
        game.start_over(0);
    }
}

/// Slides the view along the table: to the near end for a throw (or down it, while Up or W is
/// held), with the leading puck while they slide, and to the far end once they stop.
fn look(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut game: ResMut<Game>) {
    let looking_down = keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]);
    let target = match game.phase {
        Phase::Throwing if looking_down && game.grab.is_none() => VIEW_FAR,
        Phase::Throwing => VIEW_NEAR,
        Phase::Sliding => game
            .table
            .pucks
            .iter()
            .filter(|puck| puck.velocity != Vec2::ZERO)
            .map(|puck| puck.position.y)
            .max_by(f32::total_cmp)
            .map_or(game.view, |lead| lead - FOLLOW_AT),
        Phase::Waiting | Phase::Stopped { .. } | Phase::Scored { .. } => VIEW_FAR,
    }
    .clamp(VIEW_NEAR, VIEW_FAR);
    let towards = 1.0 - (-VIEW_SPEED * time.delta_secs()).exp();
    game.view += (target - game.view) * towards;
    if (game.view - target).abs() < 0.5 {
        game.view = target;
    }
}

fn draw(
    game: Res<Game>,
    canvas: Single<&ImageNode, With<Canvas>>,
    mut images: ResMut<Assets<Image>>,
    mut board: Local<Option<Pixels>>,
) {
    let Some(mut image) = images.get_mut(&canvas.image) else {
        return;
    };
    let board = board.get_or_insert_with(draw_board);
    let mut pixels = Pixels::new(CANVAS);
    // Dark behind the texts, so the bar doesn't show through them.
    for (from, to) in BACKDROPS {
        pixels.rect(from, to, BACKDROP);
    }
    // The board's top-left corner on the canvas: its far rail's top row is LENGTH + PIT +
    // FAR_RAIL along the table.
    let corner = game.to_canvas(Vec2::new(
        -(GUTTER + RAIL) as f32,
        LENGTH + (PIT + FAR_RAIL) as f32,
    ));
    pixels.draw(board, corner.as_ivec2());

    let counted = match game.phase {
        Phase::Scored { .. } => game
            .table
            .counted()
            .map(|(_, counted)| counted)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for puck in &game.fallen {
        draw_puck(&mut pixels, game.to_canvas(puck.position), puck.player, 3.0);
    }
    for (i, puck) in game.table.pucks.iter().enumerate() {
        let at = game.to_canvas(puck.position);
        if counted.contains(&i) {
            pixels.ring(at, PUCK_RADIUS + 1.5, COUNTED);
        }
        draw_puck(&mut pixels, at, puck.player, PUCK_RADIUS);
    }
    let waiting = game.phase == Phase::Throwing && game.rules.win.is_none();
    if waiting {
        draw_puck(
            &mut pixels,
            game.to_canvas(game.ready),
            game.rules.turn(),
            PUCK_RADIUS,
        );
    }

    draw_map(&mut pixels, board, &game);

    // Each player's pucks left this round, and an arrow at whose throw it is.
    for (player, first) in PUCKS_LEFT.into_iter().enumerate() {
        let left = game.rules.left(player);
        for slot in 0..PUCKS_EACH {
            let at = first + Vec2::X * 10.0 * slot as f32;
            if slot < left {
                draw_puck(&mut pixels, at, player, PUCK_RADIUS);
            } else {
                pixels.ring(at, PUCK_RADIUS - 0.5, EMPTY_SLOT);
            }
        }
        if waiting && game.rules.turn() == player {
            let tip = first - Vec2::X * 10.0;
            pixels.polygon(
                &[tip, tip - Vec2::new(4.0, 4.0), tip - Vec2::new(4.0, -4.0)],
                TURN_ARROW,
            );
        }
    }
    image.data = Some(pixels.into_bytes());
}

/// A steel puck with its player's colour on top.
fn draw_puck(pixels: &mut Pixels, at: Vec2, player: usize, radius: f32) {
    pixels.disc(at, radius, STEEL_DARK);
    pixels.disc(at, radius - 0.5, STEEL);
    pixels.disc(at, radius - 1.5, CAPS[player]);
    pixels.set((at.x - 2.0) as i32, (at.y - 2.0) as i32, SHINE);
}

/// The whole table, once: the rails, the gutters either side of the sanded surface, the pit
/// past the far end, the foul line near the thrower and the scoring lines, numbered.
fn draw_board() -> Pixels {
    let mut pixels = Pixels::new(BOARD.as_uvec2());
    let far = PIT + FAR_RAIL;
    // Rows down the board from the table's `y`.
    let row = |y: f32| far + (LENGTH - y) as i32;
    let left = GUTTER + RAIL;
    let right = left + WIDTH as i32;
    pixels.rect(IVec2::ZERO, BOARD, WOOD);
    // Light along the rails' inner edges, shade along their outer ones.
    pixels.rect(IVec2::ZERO, IVec2::new(1, BOARD.y), WOOD_DARK);
    pixels.rect(IVec2::new(BOARD.x - 1, 0), BOARD, WOOD_DARK);
    pixels.rect(IVec2::ZERO, IVec2::new(BOARD.x, 1), WOOD_LIGHT);
    pixels.rect(
        IVec2::new(RAIL, FAR_RAIL),
        IVec2::new(BOARD.x - RAIL, far + LENGTH as i32),
        GUTTER_COLOR,
    );
    pixels.rect(
        IVec2::new(RAIL, FAR_RAIL),
        IVec2::new(BOARD.x - RAIL, far),
        PIT_COLOR,
    );
    pixels.rect(
        IVec2::new(left, far),
        IVec2::new(right, far + LENGTH as i32),
        SURFACE,
    );
    // The grain: stripes along the boards the surface is made of, on and off.
    for x in (left + 3..right - 1).step_by(5) {
        let mut on = (x * 7) % 3 == 0;
        for y in far..far + LENGTH as i32 {
            if (y * 13 + x * 31) % 97 == 0 {
                on = !on;
            }
            if on {
                pixels.set(x, y, GRAIN);
            }
        }
    }
    let across = |pixels: &mut Pixels, y: f32, color| {
        let y = row(y);
        pixels.rect(IVec2::new(left, y), IVec2::new(right, y + 1), color);
    };
    across(&mut pixels, FOUL_LINE, FOUL);
    for line in LINES {
        across(&mut pixels, line, LINE);
    }
    // Each zone's number, in its middle.
    let ends = [LINES[0], LINES[1], LINES[2], LENGTH];
    for (zone, pair) in ends.windows(2).enumerate() {
        let middle = (pair[0] + pair[1]) / 2.0;
        draw_digit(
            &mut pixels,
            zone + 1,
            IVec2::new(left + WIDTH as i32 / 2 - 3, row(middle) - 5),
        );
    }
    pixels
}

/// A digit, 1 to 3, at twice the size of a 3 by 5 pixel font, its top-left corner at `at`.
fn draw_digit(pixels: &mut Pixels, digit: usize, at: IVec2) {
    const DIGITS: [[u8; 5]; 3] = [
        [0b010, 0b110, 0b010, 0b010, 0b111],
        [0b111, 0b001, 0b111, 0b100, 0b111],
        [0b111, 0b001, 0b011, 0b001, 0b111],
    ];
    for (y, bits) in DIGITS[digit - 1].iter().enumerate() {
        for x in 0..3 {
            if bits & (0b100 >> x) != 0 {
                let from = at + IVec2::new(x * 2, y as i32 * 2);
                pixels.rect(from, from + 2, NUMBER);
            }
        }
    }
}

/// The whole table, small, right of it: the board, every puck as a dot, and brackets round the
/// part of it on screen.
fn draw_map(pixels: &mut Pixels, board: &Pixels, game: &Game) {
    let corner = MAP.as_ivec2();
    let size = board.size().as_ivec2() / MAP_SCALE;
    for y in 0..size.y {
        for x in 0..size.x {
            let color = board.get(x * MAP_SCALE + 1, y * MAP_SCALE + 1);
            pixels.set(corner.x + x, corner.y + y, color);
        }
    }
    // From the table's `x`, `y` to the map's pixels.
    let on_map = |at: Vec2| {
        let on_board = Vec2::new(
            at.x + (GUTTER + RAIL) as f32,
            (PIT + FAR_RAIL) as f32 + LENGTH - at.y,
        );
        MAP + on_board / MAP_SCALE as f32
    };
    let mut dots: Vec<(Vec2, usize)> = game
        .table
        .pucks
        .iter()
        .chain(&game.fallen)
        .map(|puck| (puck.position, puck.player))
        .collect();
    if game.phase == Phase::Throwing && game.rules.win.is_none() {
        dots.push((game.ready, game.rules.turn()));
    }
    for (at, player) in dots {
        let dot = on_map(at).floor().as_ivec2();
        pixels.rect(dot - 1, dot + 1, CAPS[player]);
    }
    // The rows on screen, between brackets either side.
    let top = (on_map(Vec2::new(0.0, game.view() + NEAR_ROW)).y as i32).max(corner.y);
    let bottom = (on_map(Vec2::new(0.0, game.view() + NEAR_ROW - CANVAS.y as f32)).y as i32)
        .min(corner.y + size.y);
    for (x, inward) in [(corner.x - 2, 1), (corner.x + size.x + 1, -1)] {
        pixels.rect(IVec2::new(x, top), IVec2::new(x + 1, bottom), IN_VIEW);
        for y in [top, bottom - 1] {
            pixels.set(x + inward, y, IN_VIEW);
        }
    }
}

fn show_text(
    game: Res<Game>,
    touch: Res<Touch>,
    mut texts: Query<(&Says, &mut Text, &mut TextColor)>,
) {
    let rules = &game.rules;
    let turn = rules.turn();
    let names = game.names();
    let me = game.match_seat.as_ref().map(|(me, _)| *me);
    // The game waits on a player who left their seat, on their throw.
    let waiting_on = match (me, &game.opponent) {
        (Some(me), None) if turn != me && rules.win.is_none() => Some(&names[turn]),
        _ => None,
    };
    for (says, mut text, mut color) in &mut texts {
        let line = match *says {
            Says::Player(player) => {
                // Whose throw it is stands out.
                let lit = game.phase == Phase::Throwing && rules.win.is_none() && turn == player;
                let wanted = if lit {
                    Color::WHITE
                } else {
                    Color::srgb(0.6, 0.6, 0.65)
                };
                if color.0 != wanted {
                    color.0 = wanted;
                }
                format!("{}\n{}", names[player], rules.scores[player])
            }
            Says::Status if game.waiting_for_start => "Joining the game...".to_string(),
            Says::Status if let Some(name) = waiting_on => {
                format!("Waiting for {name} to come back.\n\nNew game (in /settings) starts over.")
            }
            Says::Status => match (rules.win, game.phase) {
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
                (None, Phase::Scored { scored, .. }) => match scored {
                    Some((player, points)) => {
                        let s = if points == 1 { "" } else { "s" };
                        format!("{} scores {points} point{s}.", names[player])
                    }
                    None => "Nobody scores.".to_string(),
                },
                (None, Phase::Throwing) if !game.my_turn() => {
                    format!("{}'s throw.\nFirst to {WINNING_SCORE}.", names[turn])
                }
                (None, Phase::Throwing) => {
                    let whose = match me {
                        Some(_) => "Your throw.".to_string(),
                        None => format!("{}'s throw.", names[turn]),
                    };
                    let how = if touch.is_on() {
                        "Drag the puck and flick it up the table."
                    } else {
                        "Drag the puck and flick it up the table.\n\nUp looks down the table. Esc leaves."
                    };
                    format!("{whose}\nFirst to {WINNING_SCORE}.\n\n{how}")
                }
                (None, _) => String::new(),
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

/// Esc, or Leave on a touch screen, goes back to the bar.
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
    // Away from the table: the game stays as it is for whoever is left there, their seat
    // waiting. This player carries on from it if they come back alone, throwing for both, and
    // gets it from the other player if they're still there.
    seats::stand();
    game.table_id = None;
    game.opponent = None;
    game.match_seat = None;
    game.alone_here = false;
    game.owe_game = None;
    game.waiting_for_start = false;
    game.grab = None;
    game.their_result = None;
    // A throw still sliding finishes where nobody sees.
    if matches!(game.phase, Phase::Sliding | Phase::Waiting) {
        game.finish_slide();
    }
    game.their_settings = None;
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
}

/// For browser tests (testing.rs): a throw asked for, on this player's throw, and what the game
/// is doing.
#[cfg(feature = "test-hooks")]
fn test_hooks(mut game: ResMut<Game>) {
    use serde_json::json;

    use crate::testing;

    let game = &mut *game;
    let throwing = game.phase == Phase::Throwing && game.rules.win.is_none();
    if throwing
        && game.my_turn()
        && game.grab.is_none()
        && let Some(velocity) = testing::take_throw()
    {
        launch(game, velocity);
    }
    let phase = match game.phase {
        Phase::Throwing => "throwing",
        Phase::Sliding => "sliding",
        Phase::Waiting => "waiting",
        Phase::Stopped { .. } => "stopped",
        Phase::Scored { .. } => "scored",
    };
    let pucks: Vec<_> = game
        .table
        .pucks
        .iter()
        .map(|puck| {
            json!({
                "player": puck.player,
                "at": [puck.position.x, puck.position.y],
                "points": puck.points(),
            })
        })
        .collect();
    testing::report(
        "shuffleboard",
        json!({
            "phase": phase,
            "turn": game.rules.turn(),
            "my_turn": game.my_turn(),
            "scores": game.rules.scores,
            "thrown": game.rules.thrown,
            "first": game.rules.first,
            "win": game.rules.win,
            "pucks": pucks,
            "fallen": game.fallen.len(),
            "ready": [game.ready.x, game.ready.y],
            "seat": game.match_seat.as_ref().map(|(me, _)| *me),
            "names": game.names(),
            "opponent": game.opponent.is_some(),
            "waiting_for_start": game.waiting_for_start,
        }),
    );
}
