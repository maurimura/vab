//! Playing pool: the table seen from above, drawn pixel by pixel into a small image that fills
//! the screen at a whole-number zoom. The cue follows the mouse (or the finger) around the cue
//! ball. Holding the button pulls it back, further the longer it's held, and letting go shoots:
//! once pulled, the shot is coming. Esc goes back to the bar, and the table stays as it was
//! for next time. It's 8-ball, the player taking both sides in turn as the rules say, and a
//! panel for each player under the table, player 1's on the left and player 2's on the right,
//! shows their group and their balls that are down, in order, the player at the table lit up;
//! between them a line says what the last shot did. After a foul the next
//! player has ball in hand: the cue ball follows the pointer (or the arrow keys) until a click,
//! a tap or Space puts it down. The physics and the
//! rules are the billiards crate's, and how it plays can be tuned with `/settings`
//! (settings.rs).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use billiards::rules::{self, Foul, Group, WinBy};
use billiards::{BALL_RADIUS, HEIGHT, Pocket, RAIL, STEP, Table, WIDTH, pockets};

use wasm_bindgen::prelude::*;

use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
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

/// The balls racked again, each time a little differently: the browser picks the seed.
fn new_rack() -> Table {
    Table::racked((random() * f64::from(u32::MAX)) as u32)
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Pool), show_table)
            .add_systems(
                Update,
                (
                    fit_canvas,
                    aim,
                    play,
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
        }
    }
}

impl Game {
    /// A new game on a fresh rack, the other player breaking this time.
    fn start_over(&mut self) {
        let breaker = 1 - self.rules.breaker;
        *self = Self {
            rules: rules::Game::new(breaker),
            aim: self.aim,
            ..default()
        };
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
    let pointer = touch.finger().or(window.cursor_position());
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
        if towards.length() > 1.0 {
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
    game.table.settings = settings.pool();
    for drop in &mut game.dropping {
        drop.age += delta;
    }
    game.dropping.retain(|drop| drop.age < DROP_TIME);
    let full_pull = settings.get(Knob::FullPull);
    game.cue = match game.cue {
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
            } else {
                game.finish_shot();
                game.cue
            }
        }
        Cue::Over if holding.now && !holding.before && !holding.blocked => {
            game.start_over();
            Cue::Aiming
        }
        cue @ (Cue::Aiming | Cue::Over) => cue,
    };
}

fn draw(
    game: Res<Game>,
    settings: Res<Settings>,
    canvas: Single<&ImageNode, With<Canvas>>,
    mut images: ResMut<Assets<Image>>,
    mut table_art: Local<Option<(f32, Vec<u8>)>>,
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
    let mut pixels = Pixels(art.clone());
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
    if !matches!(game.cue, Cue::Rolling | Cue::Over | Cue::Placing { .. }) {
        let cue_ball = game.table.cue_ball().position + FELT;
        let pull_back = settings.get(Knob::PullBack);
        let gap = BALL_RADIUS + CUE_GAP + game.cue.pull() * pull_back;
        pixels.cue(cue_ball - game.aim * gap, -game.aim);
    }
    draw_panels(&mut pixels, &game.rules);
    image.data = Some(pixels.0);
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
    let placing = matches!(game.cue, Cue::Placing { .. });
    let line = describe(&game.rules, placing, touch.is_on());
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
        let mut line = format!("Player {} · {group}", player + 1);
        if *player == lit && rules.win.is_none() {
            if rules.breaking {
                line = format!("Player {} · to break", player + 1);
            } else if placing {
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
/// ball in hand), or who won; how to play before anything has happened.
fn describe(game: &rules::Game, placing: bool, touch: bool) -> String {
    let name = |player: usize| format!("Player {}", player + 1);
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
        let again = if touch { "Tap" } else { "Click" };
        return format!("{} wins: {why}! {again} for a new game.", name(win.winner));
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
        // Nothing has happened yet: how to play.
        said.push(if touch {
            "Your finger aims. Hold to pull the cue back, lift to shoot.".to_string()
        } else {
            "The mouse aims. Hold to pull the cue back, let go to shoot. Esc leaves.".to_string()
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
    // A shot half taken is put down, and one rolling finishes where nobody sees.
    match game.cue {
        Cue::Rolling => {
            while game.table.is_moving() {
                game.table.step();
            }
            game.finish_shot();
        }
        Cue::Pulling(_) | Cue::Striking { .. } => game.cue = Cue::Aiming,
        Cue::Placing { .. } => game.cue = Cue::Placing { held: false },
        Cue::Aiming | Cue::Over => {}
    }
    game.dropping.clear();
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
}

/// The canvas's RGBA bytes.
struct Pixels(Vec<u8>);

impl Pixels {
    fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if (0..CANVAS.x as i32).contains(&x) && (0..CANVAS.y as i32).contains(&y) {
            let i = (y as usize * CANVAS.x as usize + x as usize) * 4;
            self.0[i..i + 4].copy_from_slice(&color);
        }
    }

    fn rect(&mut self, from: IVec2, to: IVec2, color: [u8; 4]) {
        for y in from.y..to.y {
            for x in from.x..to.x {
                self.set(x, y, color);
            }
        }
    }

    /// A convex polygon, its corners in order either way round.
    fn polygon(&mut self, corners: &[Vec2], color: [u8; 4]) {
        let low = corners
            .iter()
            .copied()
            .reduce(Vec2::min)
            .unwrap_or_default()
            .floor();
        let high = corners
            .iter()
            .copied()
            .reduce(Vec2::max)
            .unwrap_or_default()
            .ceil();
        let side = |a: Vec2, b: Vec2, p: Vec2| (b - a).perp_dot(p - a);
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                let sides = corners
                    .iter()
                    .zip(corners.iter().cycle().skip(1))
                    .map(|(&a, &b)| side(a, b, middle));
                let (mut left, mut right) = (false, false);
                for s in sides {
                    left |= s < 0.0;
                    right |= s > 0.0;
                }
                if !(left && right) {
                    self.set(x, y, color);
                }
            }
        }
    }

    /// Every pixel within `reach` of the line from `from` to `to`.
    fn thick_line(&mut self, from: Vec2, to: Vec2, reach: f32, color: [u8; 4]) {
        let (low, high) = (
            (from.min(to) - reach).floor(),
            (from.max(to) + reach).ceil(),
        );
        let along = to - from;
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                let t = ((middle - from).dot(along) / along.length_squared()).clamp(0.0, 1.0);
                if middle.distance(from + along * t) <= reach {
                    self.set(x, y, color);
                }
            }
        }
    }

    /// A circle one pixel thick.
    fn ring(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        let (low, high) = (
            (center - radius - 1.0).floor(),
            (center + radius + 1.0).ceil(),
        );
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let distance = (Vec2::new(x as f32, y as f32) + 0.5).distance(center);
                if (radius - 0.5..radius + 0.5).contains(&distance) {
                    self.set(x, y, color);
                }
            }
        }
    }

    fn disc(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        let (low, high) = ((center - radius).floor(), (center + radius).ceil());
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                if middle.distance(center) <= radius {
                    self.set(x, y, color);
                }
            }
        }
    }

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
fn draw_table(pockets: &[Pocket; 6]) -> Vec<u8> {
    let mut pixels = Pixels(
        std::iter::repeat_n(CLEAR, (CANVAS.x * CANVAS.y) as usize)
            .flatten()
            .collect(),
    );
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
    pixels.0
}
