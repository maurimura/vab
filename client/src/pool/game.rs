//! Playing pool: the table seen from above, drawn pixel by pixel into a small image that fills
//! the screen at a whole-number zoom. The cue follows the mouse (or the finger) around the cue
//! ball. Holding the button pulls it back, further the longer it's held, and letting go shoots:
//! once pulled, the shot is coming. Esc goes back to the bar, and the table stays as it was
//! for next time. The physics is the billiards crate's, and how it plays can be tuned with
//! `/settings` (settings.rs).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use billiards::{BALL_RADIUS, HEIGHT, Pocket, RAIL, STEP, Table, WIDTH, pockets};

use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::settings::{Knob, NewRack, Settings};
use crate::touch::{self, Touch, TouchButton};

/// The image the table is drawn into, and where the felt's corner is in it.
const CANVAS: UVec2 = UVec2::new(320, 180);
const FELT: Vec2 = Vec2::new(32.0, 26.0);
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

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Pool), show_table)
            .add_systems(
                Update,
                (fit_canvas, aim, play, draw, leave.run_if(chat_closed))
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
            table: Table::racked(),
            aim: Vec2::X,
            cue: Cue::Aiming,
            pending: 0.0,
            dropping: Vec::new(),
        }
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
    let how = if touch.is_on() {
        "Your finger aims. Hold to pull the cue back, lift to shoot."
    } else {
        "The mouse aims. Hold the button to pull the cue back, let go to shoot. Esc leaves."
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
            (
                Text::new(how),
                TextFont {
                    font_size: FontSize::Px(if touch.is_on() { 11.0 } else { 14.0 }),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(8.0),
                    left: Val::Px(8.0),
                    max_width: if touch.is_on() {
                        Val::Percent(60.0)
                    } else {
                        Val::Auto
                    },
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
            ),
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
        // Typing or tuning: a cue being aimed or pulled back stays put.
        Cue::Aiming | Cue::Pulling(_) if holding.blocked => game.cue,
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
                if game.table.cleared() {
                    game.table = Table::racked();
                }
                game.table.respot_cue_ball();
                Cue::Aiming
            }
        }
        Cue::Aiming => Cue::Aiming,
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
    if !matches!(game.cue, Cue::Rolling) {
        let cue_ball = game.table.cue_ball().position + FELT;
        let pull_back = settings.get(Knob::PullBack);
        let gap = BALL_RADIUS + CUE_GAP + game.cue.pull() * pull_back;
        pixels.cue(cue_ball - game.aim * gap, -game.aim);
    }
    image.data = Some(pixels.0);
}

/// The New rack button in the settings: every ball back, wherever the game is at.
fn rack_again(mut asked: MessageReader<NewRack>, game: Option<ResMut<Game>>) {
    if asked.read().count() == 0 {
        return;
    }
    if let Some(mut game) = game {
        game.table = Table::racked();
        game.cue = Cue::Aiming;
        game.dropping.clear();
    }
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
    while game.table.is_moving() {
        game.table.step();
    }
    if game.table.cleared() {
        game.table = Table::racked();
    }
    game.table.respot_cue_ball();
    game.cue = Cue::Aiming;
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
