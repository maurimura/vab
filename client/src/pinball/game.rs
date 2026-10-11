//! Playing the Saloon pinball machine: the table as its art draws it (art.rs), in parts stacked
//! by how far down the table they reach, so the ball passes in front of and behind them, played
//! by the pinball crate's physics from the same layout.
//!
//! The flippers are Shift (left and right), or Z and /, A and D, or the arrow keys; on a touch
//! screen, a finger on the left or right half of the screen. Holding Space (or Down, S, Enter)
//! pulls the plunger back, and letting go launches the ball; on a touch screen, a finger held
//! while the ball waits on the plunger. The table's rules (pinball::rules) score the game, light
//! the lamps that say where to shoot, and say on the backbox's display what to go for. Esc goes
//! back to the bar.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pinball::rules::{self, Body, Rules, Screen};
use pinball::{BALL_RADIUS, Layout, Place, Side, Table};

use super::art::{Placed, Sprites};
use super::dmd::{self, Dots};
use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::settings::{NewPinballGame, Settings};
use crate::touch::{self, Touch, TouchButton};

/// Where the table's parts are (under the site's assets), and its art's manifest and layout.
const ASSETS: &str = "pinball/saloon";
const SPRITES: &str = include_str!("../../../assets/pinball/saloon/sprites.json");
const LAYOUT: &str = include_str!("../../../assets/pinball/saloon/layout.json");
/// The table is drawn by a camera of its own, on its own render layer.
const LAYER: usize = 1;
/// Balls that can be drawn at once: three in multiball, and one more coming out of the mine.
const MOST_BALLS: usize = 5;
/// Drawing order: the floor's lamps and the balls' shadows over the background, the table's
/// parts by depth over those, and the cabinet's front and the backbox over everything.
const LAMPS_Z: f32 = 1.0;
const SHADOWS_Z: f32 = 2.0;
const FRONT_Z: f32 = 40.0;
const BACKBOX_Z: f32 = 41.0;

fn depth_z(depth: f32) -> f32 {
    10.0 + depth * 0.1
}

/// The art's manifest, read once.
fn sprites() -> &'static Sprites {
    static SPRITES_: OnceLock<Sprites> = OnceLock::new();
    SPRITES_.get_or_init(|| Sprites::from_json(SPRITES).expect("the Saloon's sprites.json reads"))
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Pinball), show_machine)
            .add_systems(
                Update,
                (new_game, play, draw, leave.run_if(chat_closed))
                    .chain()
                    .run_if(in_state(Mode::Pinball)),
            )
            .add_systems(OnExit(Mode::Pinball), hide_machine);
        #[cfg(feature = "test-hooks")]
        app.add_systems(
            Update,
            test_hooks.after(play).run_if(in_state(Mode::Pinball)),
        );
    }
}

/// The machine's game, kept between visits.
#[derive(Resource)]
struct Machine {
    table: Table,
    camera: pinball::Camera,
    rules: Rules,
    time: f32,
}

impl Machine {
    fn new() -> Self {
        let layout = Layout::from_json(LAYOUT).expect("the Saloon's layout.json reads");
        let camera = pinball::Camera::new(layout.camera, layout.size);
        let mut machine = Self {
            table: Table::new(layout, pinball::Settings::default()),
            camera,
            rules: Rules::new(),
            time: 0.0,
        };
        machine.new_game();
        machine
    }

    fn new_game(&mut self) {
        self.rules.start(&mut self.table);
    }

    /// The ball waits on the plunger to be launched.
    fn on_plunger(&self) -> bool {
        self.rules.on_plunger(&self.table)
    }

    fn lit(&self, lamp: &str) -> bool {
        if self.rules.over {
            // Waiting for a game: the lamps chase.
            let order = sprites()
                .lamps
                .keys()
                .position(|name| name == lamp)
                .unwrap_or(0);
            return ((self.time * 10.0) as usize + order).is_multiple_of(7);
        }
        self.rules.lit(lamp, &self.table)
    }

    /// What the display shows: the ball and the score along the top, and under it big news, or
    /// what to go for.
    fn dots(&self, touch: bool) -> Dots {
        let mut dots = Dots::default();
        let width = dmd::COLUMNS as i32;
        match self.rules.screen(&self.table, touch) {
            Screen::Over { score } => {
                dots.centred("GAME OVER", 2, 2);
                dots.centred(&score, 17, 1);
                dots.centred(
                    if touch {
                        "TOUCH TO PLAY"
                    } else {
                        "PRESS SPACE TO PLAY"
                    },
                    25,
                    1,
                );
            }
            Screen::Playing { left, score, body } => {
                dots.text(&left, 1, 0, 1);
                dots.right(&score, 0, 1);
                dots.dotted(8);
                match body {
                    Body::Callout { big, line } => {
                        let scale = if dmd::text_width(&big, 2) <= width {
                            2
                        } else {
                            1
                        };
                        let y = if line.is_empty() { 14 } else { 10 };
                        dots.centred(&big, if scale == 2 { y } else { y + 4 }, scale);
                        dots.centred(&line, 25, 1);
                    }
                    Body::Text { title, text } => {
                        let mut lines: Vec<String> = title.into_iter().collect();
                        lines.extend(dmd::wrap(&text, width));
                        lines.truncate(3);
                        let top = match lines.len() {
                            1 => 17,
                            2 => 13,
                            _ => 9,
                        };
                        for (i, line) in lines.iter().enumerate() {
                            dots.centred(line, top + 8 * i as i32, 1);
                        }
                    }
                }
            }
        }
        dots
    }
}

/// What a drawn part is.
#[derive(Component)]
enum Part {
    /// A moving part, by name, showing one of its states.
    Moving(String),
    Lamp(String),
    Ball(usize),
    Shadow(usize),
    Captive,
    /// One of the wanted posters, which wobble when hit.
    Poster(usize),
}

/// Everything the machine's screen spawned, to despawn on leaving.
#[derive(Component)]
struct View;

/// A moving part's image for each of its states, and where it goes.
type Frames = Vec<Option<(Handle<Image>, Placed)>>;

/// The images of each moving part's states, and the display's image.
#[derive(Resource)]
struct Art {
    frames: HashMap<String, Frames>,
    dmd: Handle<Image>,
    dots: Dots,
}

/// The screen rectangle `placed` covers, as a sprite's middle (at depth `z`) and size, in the
/// camera's world: 1920x1080, y up, origin in the middle.
fn placement(placed: &Placed, z: f32) -> (Transform, Vec2) {
    let middle = Vec2::new(
        placed.x + placed.w / 2.0 - 960.0,
        540.0 - (placed.y + placed.h / 2.0),
    );
    (
        Transform::from_translation(middle.extend(z)),
        Vec2::new(placed.w, placed.h),
    )
}

/// From the art's screen pixels (y down) to the camera's world (y up, origin in the middle).
fn to_world(screen: Vec2) -> Vec2 {
    Vec2::new(screen.x - 960.0, 540.0 - screen.y)
}

fn show_machine(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    machine: Option<Res<Machine>>,
    touch: Res<Touch>,
) {
    if machine.is_none() {
        commands.insert_resource(Machine::new());
    }
    let art = sprites();
    let load = |file: &str| -> Handle<Image> {
        asset_server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::linear();
            })
            .load(format!("{ASSETS}/{file}"))
    };
    let layer = RenderLayers::layer(LAYER);
    commands.spawn((
        View,
        Camera2d,
        Camera {
            // Over the bar, which it hides.
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.06, 0.03, 0.02)),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: art.size[0],
                min_height: art.size[1],
            },
            ..OrthographicProjection::default_2d()
        }),
        layer.clone(),
    ));
    let mut sprite = |placed: &Placed, z: f32, extra: Option<Part>, visible: bool| {
        let (transform, size) = placement(placed, z);
        let mut entity = commands.spawn((
            View,
            Sprite {
                image: load(&placed.file),
                custom_size: Some(size),
                ..default()
            },
            transform,
            layer.clone(),
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
        if let Some(part) = extra {
            entity.insert(part);
        }
    };
    sprite(&art.background, 0.0, None, true);
    for (i, (name, placed)) in art.lamps.iter().enumerate() {
        sprite(
            placed,
            LAMPS_Z + i as f32 * 0.001,
            Some(Part::Lamp(name.clone())),
            false,
        );
    }
    for item in &art.items {
        let poster = item
            .name
            .strip_prefix("poster-")
            .and_then(|i| i.parse().ok())
            .map(Part::Poster);
        sprite(&item.placed, depth_z(item.depth), poster, true);
    }
    sprite(&art.front, FRONT_Z, None, true);
    sprite(&art.backbox.placed, BACKBOX_Z, None, true);
    for (i, placed) in art.ranks.iter().enumerate() {
        sprite(
            placed,
            BACKBOX_Z + 0.5,
            Some(Part::Lamp(format!("backbox-rank-{i}"))),
            false,
        );
    }

    // The moving parts: every state's image loaded, the first shown.
    let mut frames = HashMap::new();
    for (name, states) in &art.states {
        let loaded: Frames = states
            .frames
            .iter()
            .map(|frame| {
                frame
                    .as_ref()
                    .map(|placed| (load(&placed.file), placed.clone()))
            })
            .collect();
        if let Some(Some((image, placed))) = loaded.first() {
            let (transform, size) = placement(placed, depth_z(states.depth));
            commands.spawn((
                View,
                Part::Moving(name.clone()),
                Sprite {
                    image: image.clone(),
                    custom_size: Some(size),
                    ..default()
                },
                transform,
                layer.clone(),
            ));
        }
        frames.insert(name.clone(), loaded);
    }

    // The balls, the captive ball, and their shadows: placed as they move.
    let ball = load(&art.ball.placed.file);
    let shadow = images.add(shadow_image());
    for i in 0..MOST_BALLS {
        for (part, image) in [
            (Part::Ball(i), ball.clone()),
            (Part::Shadow(i), shadow.clone()),
        ] {
            commands.spawn((
                View,
                part,
                Sprite::from_image(image),
                Transform::default(),
                layer.clone(),
                Visibility::Hidden,
            ));
        }
    }
    commands.spawn((
        View,
        Part::Captive,
        Sprite::from_image(ball),
        Transform::default(),
        layer.clone(),
    ));

    // The display: its lit dots, over the backbox's dark ones.
    let place = art.backbox.dmd;
    let mut image = Image::new_fill(
        Extent3d {
            width: (dmd::COLUMNS * dmd::DOT) as u32,
            height: (dmd::ROWS * dmd::DOT) as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    let dmd_image = images.add(image);
    let size = Vec2::new(
        dmd::COLUMNS as f32 * place.pitch,
        dmd::ROWS as f32 * place.pitch,
    );
    let corner = to_world(Vec2::new(place.x, place.y));
    commands.spawn((
        View,
        Sprite {
            image: dmd_image.clone(),
            custom_size: Some(size),
            ..default()
        },
        Transform::from_translation(
            (corner + Vec2::new(size.x, -size.y) / 2.0).extend(BACKBOX_Z + 1.0),
        ),
        layer.clone(),
    ));
    commands.insert_resource(Art {
        frames,
        dmd: dmd_image,
        dots: Dots::default(),
    });

    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), as at the tables.
        commands.spawn((
            View,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(80.0),
                ..default()
            },
            GlobalZIndex(1),
            children![touch::button(TouchButton::Leave, "Leave", 64.0, 36.0)],
        ));
    }
}

/// A soft, dark, round shadow for the balls.
fn shadow_image() -> Image {
    const SIZE: usize = 64;
    let mut data = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d =
                ((x as f32 + 0.5 - 32.0).powi(2) + (y as f32 + 0.5 - 32.0).powi(2)).sqrt() / 32.0;
            let alpha = (1.0 - ((d - 0.75) / 0.25).clamp(0.0, 1.0)) * 0.42;
            let p = (y * SIZE + x) * 4;
            data[p..p + 4].copy_from_slice(&[58, 18, 6, (alpha * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

/// The New game button in the settings: a new game, wherever this one is at.
fn new_game(mut asked: MessageReader<NewPinballGame>, mut machine: ResMut<Machine>) {
    if asked.read().count() > 0 {
        machine.new_game();
    }
}

/// The flippers and the plunger from the keys or fingers; the table played on; and the game
/// moved on: points, lamps, the next ball, a new game.
#[allow(clippy::too_many_arguments)]
fn play(
    keys: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
    touch: Res<Touch>,
    window: Single<&Window>,
    chat: Res<Chat>,
    help: Res<Help>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut machine: ResMut<Machine>,
) {
    let machine = &mut *machine;
    let seconds = time.delta_secs().min(1.0 / 20.0);
    machine.time += seconds;
    machine.table.settings = settings.pinball();
    let busy = chat.is_open() || help.is_open() || settings.is_open();
    let held = |codes: &[KeyCode]| !busy && keys.any_pressed(codes.iter().copied());
    let pressed = |codes: &[KeyCode]| !busy && keys.any_just_pressed(codes.iter().copied());
    const LEFT: [KeyCode; 4] = [
        KeyCode::ShiftLeft,
        KeyCode::KeyZ,
        KeyCode::ArrowLeft,
        KeyCode::KeyA,
    ];
    const RIGHT: [KeyCode; 4] = [
        KeyCode::ShiftRight,
        KeyCode::Slash,
        KeyCode::ArrowRight,
        KeyCode::KeyD,
    ];
    const PLUNGE: [KeyCode; 4] = [
        KeyCode::Space,
        KeyCode::ArrowDown,
        KeyCode::KeyS,
        KeyCode::Enter,
    ];
    // Fingers: the left or right half of the screen flips that flipper; with the ball on the
    // plunger, any finger pulls it.
    let fingers: Vec<f32> = if busy || !touch.is_on() {
        Vec::new()
    } else {
        touches.iter().map(|finger| finger.position().x).collect()
    };
    let half = window.width() / 2.0;
    let on_plunger = machine.on_plunger();
    let finger_plunge = on_plunger && !fingers.is_empty();
    let left = held(&LEFT) || (!finger_plunge && fingers.iter().any(|&x| x < half));
    let right = held(&RIGHT) || (!finger_plunge && fingers.iter().any(|&x| x >= half));
    let plunge = held(&PLUNGE) || finger_plunge;
    // A flipper pressed (not held): it moves the lit top lanes along.
    let tapped: Vec<f32> = if busy || !touch.is_on() || finger_plunge {
        Vec::new()
    } else {
        touches
            .iter_just_pressed()
            .map(|finger| finger.position().x)
            .collect()
    };
    let input = rules::Input {
        left: pressed(&LEFT) || tapped.iter().any(|&x| x < half),
        right: pressed(&RIGHT) || tapped.iter().any(|&x| x >= half),
    };

    if machine.rules.over {
        let start = pressed(&PLUNGE) || (touch.is_on() && touches.any_just_pressed());
        if start {
            machine.new_game();
        }
    } else {
        machine.table.flip(Side::Left, left);
        machine.table.flip(Side::Right, right);
        machine.table.hold_plunger(plunge);
    }

    let game_seconds = seconds * settings.pinball_speed();
    let events = machine.table.advance(game_seconds);
    machine
        .rules
        .update(&mut machine.table, &events, input, game_seconds);
}

/// Puts every part where the game has it: the moving parts in their states, the lamps on or off,
/// the balls and their shadows, and the display.
fn draw(
    machine: Res<Machine>,
    touch: Res<Touch>,
    mut art: ResMut<Art>,
    mut images: ResMut<Assets<Image>>,
    mut parts: Query<(&Part, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let table = &machine.table;
    let sprites = sprites();
    let camera = &machine.camera;
    let ramp_z = sprites
        .items
        .iter()
        .find(|item| item.name == "ramp")
        .map_or(12.15, |item| depth_z(item.depth));
    // Which state each moving part shows.
    let state = |name: &str| -> usize {
        let states = &sprites.states[name];
        let last = states.frames.len().saturating_sub(1);
        match name {
            "flipper-left" => (table.flippers[0].raised() * last as f32).round() as usize,
            "flipper-right" => (table.flippers[1].raised() * last as f32).round() as usize,
            "doors" => states.nearest(table.doors_angle),
            "plunger" => states.nearest(table.plunger),
            "bull" => match table.bull_flash {
                t if t < 0.07 => 1,
                t if t < 0.14 => 2,
                t if t < 0.24 => 3,
                t if t < 0.34 => 2,
                t if t < 0.42 => 1,
                _ => 0,
            },
            _ => {
                if let Some(i) = name
                    .strip_prefix("bumper-")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    usize::from(table.bumper_flash.get(i).is_some_and(|&t| t < 0.12))
                } else if let Some(i) = name
                    .strip_prefix("bottle-")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    usize::from(table.drops_down.get(i).copied().unwrap_or(false))
                } else {
                    0
                }
            }
        }
        .min(last)
    };

    for (part, mut sprite, mut transform, mut visibility) in &mut parts {
        match part {
            Part::Moving(name) => {
                let Some(frames) = art.frames.get(name) else {
                    continue;
                };
                match &frames[state(name)] {
                    Some((image, placed)) => {
                        if sprite.image != *image {
                            sprite.image = image.clone();
                            let (at, size) = placement(placed, transform.translation.z);
                            transform.translation = at.translation;
                            sprite.custom_size = Some(size);
                        }
                        visibility.set_if_neq(Visibility::Inherited);
                    }
                    None => {
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                }
            }
            Part::Lamp(name) => {
                visibility.set_if_neq(if machine.lit(name) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            Part::Poster(i) => {
                // A hit poster shivers for a moment.
                let t = table
                    .standup_flash
                    .get(*i)
                    .copied()
                    .unwrap_or(f32::INFINITY);
                let placed = &sprites
                    .items
                    .iter()
                    .find(|item| item.name == format!("poster-{i}"))
                    .map(|item| &item.placed);
                if let Some(placed) = placed {
                    let wobble = if t < 0.35 {
                        (t * 70.0).sin() * 3.0 * (1.0 - t / 0.35)
                    } else {
                        0.0
                    };
                    let (at, _) = placement(placed, transform.translation.z);
                    transform.translation.x = at.translation.x + wobble;
                }
            }
            Part::Ball(i) | Part::Shadow(i) => {
                let ball = table
                    .balls
                    .get(*i)
                    .filter(|ball| !matches!(ball.place, Place::Mine { .. }));
                let Some(ball) = ball else {
                    visibility.set_if_neq(Visibility::Hidden);
                    continue;
                };
                let at = table.ball_at(ball);
                if matches!(part, Part::Ball(_)) {
                    let z = match ball.place {
                        Place::Ramp { .. } => depth_z(at.y).max(ramp_z + 0.01),
                        _ => depth_z(at.y + BALL_RADIUS * 0.5),
                    };
                    place_ball(camera, at, z, &mut sprite, &mut transform);
                    visibility.set_if_neq(Visibility::Inherited);
                } else if ball.place == Place::Playfield {
                    place_shadow(camera, at, &mut sprite, &mut transform);
                    visibility.set_if_neq(Visibility::Inherited);
                } else {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
            Part::Captive => {
                let at = table.ball_at(&table.captive);
                place_ball(
                    camera,
                    at,
                    depth_z(at.y + BALL_RADIUS * 0.5),
                    &mut sprite,
                    &mut transform,
                );
            }
        }
    }

    // The display, redrawn when what it says changes.
    let dots = machine.dots(touch.is_on());
    if dots != art.dots
        && let Some(mut image) = images.get_mut(&art.dmd)
    {
        image.data = Some(dots.rgba());
        art.dots = dots;
    }
}

/// The ball's sprite where the ball is, as big as it looks there.
fn place_ball(
    camera: &pinball::Camera,
    at: Vec3,
    z: f32,
    sprite: &mut Sprite,
    transform: &mut Transform,
) {
    let art = &sprites().ball;
    let scale = camera.pixels_per_inch(at) / art.scale;
    sprite.custom_size = Some(Vec2::new(art.placed.w, art.placed.h) * scale);
    transform.translation = to_world(camera.project(at)).extend(z);
}

/// The ball's shadow on the floor, a little down and right of it, as the table's other shadows
/// fall.
fn place_shadow(
    camera: &pinball::Camera,
    at: Vec3,
    sprite: &mut Sprite,
    transform: &mut Transform,
) {
    let floor = Vec3::new(at.x + 0.25, at.y + 0.26, 0.0);
    let across = camera.pixels_per_inch(floor) * 2.0 * BALL_RADIUS;
    let along = (camera.project(floor + Vec3::Y * BALL_RADIUS)
        - camera.project(floor - Vec3::Y * BALL_RADIUS))
    .y;
    sprite.custom_size = Some(Vec2::new(across, along));
    transform.translation = to_world(camera.project(floor)).extend(SHADOWS_Z);
}

/// Esc, or Leave on a touch screen, goes back to the bar.
fn leave(keys: Res<ButtonInput<KeyCode>>, touch: Res<Touch>, mut mode: ResMut<NextState<Mode>>) {
    if keys.just_pressed(KeyCode::Escape) || touch.tapped(TouchButton::Leave) {
        mode.set(Mode::Walking);
    }
}

fn hide_machine(
    mut commands: Commands,
    mut machine: ResMut<Machine>,
    view: Query<Entity, With<View>>,
) {
    machine.table.flip(Side::Left, false);
    machine.table.flip(Side::Right, false);
    machine.table.hold_plunger(false);
    for entity in &view {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<Art>();
}

/// For browser tests (testing.rs): what the machine is doing.
#[cfg(feature = "test-hooks")]
fn test_hooks(machine: Res<Machine>) {
    use serde_json::json;

    let table = &machine.table;
    let balls: Vec<_> = table
        .balls
        .iter()
        .map(|ball| {
            let place = match ball.place {
                Place::Playfield => "playfield",
                Place::Ramp { .. } => "ramp",
                Place::Scoop { .. } => "scoop",
                Place::Mine { .. } => "mine",
            };
            json!({ "at": [ball.position.x, ball.position.y], "speed": ball.velocity.length(), "place": place })
        })
        .collect();
    crate::testing::report(
        "pinball",
        json!({
            "ball": machine.rules.ball,
            "score": machine.rules.score,
            "over": machine.rules.over,
            // What the display says, as text.
            "screen": format!("{:?}", machine.rules.screen(table, false)),
            "on_plunger": machine.on_plunger(),
            "balls": balls,
            "flippers": table.flippers.iter().map(|f| f.raised()).collect::<Vec<_>>(),
            "plunger": table.plunger,
        }),
    );
}
