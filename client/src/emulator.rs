//! Plays a cabinet's game with the page's emulator worker (web/emulator/worker.js). The page
//! sits the player at the cabinet and plays alone or online with whoever takes the other seat,
//! or has them watch the game the players there are playing. Frames come in through
//! `push_frame` and fill the screen, a status line (who you play with, the connection) through
//! `game_status`, the player's buttons go out through `emulatorInput`, and Esc stops the game
//! and returns to the bar. On a touch screen the buttons are on the screen (touch.rs).
//!
//! At a lightgun game (Time Crisis II) the mouse aims, over the game's screen, where a crosshair
//! shows instead of the pointer: a click is the trigger and the right button or Space the
//! other (Time Crisis II's pedal). On a touch screen a finger on the game aims and shoots there.
//! The aim goes out with the buttons, in the high half of the same u32. Two players at its twin
//! cabinet (`linked`) each aim on their own screen: each browser runs that player's own board.
//!
//! At a game whose seats the player picks (cabinets.rs), 1 to 4 are the players' Starts, as on
//! an arcade's control panel: the player's own seat's is Start, and another's, if that seat is
//! free, moves them there and starts them (alone, their game goes on; with others, they rejoin
//! it from there: web/index.html).
//!
//! Which key presses which of the game's buttons is the player's to change, game by game, in
//! the controls panel (Tab: controls.rs). At a driving game (`wheel`: Out Run, Cruis'n USA)
//! the steering keys turn a steering wheel, in the worker (web/emulator/wheel.js), as the
//! player has it turn in the panel. `/wheel` in the chat shows how it turns and changes it with
//! numbers (`/wheel lock=0.5 back=0.1 curve=2`), as the panel does, and prints the line for
//! assets/games.ron.

use std::cell::{Cell, RefCell};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use bevy::window::{CursorOptions, PrimaryWindow};
use wasm_bindgen::prelude::*;
use world::{Game, Wheel};

use crate::Mode;
use crate::chat::{Chat, TuneWheel, chat_closed};
use crate::controls::{Controls, Gear, LEFT, RIGHT};
use crate::settings::Settings;
use crate::touch::{self, Touch, TouchButton};
use crate::voice::SEAT_KEYS;

/// Keys and the RetroPad button ids (libretro.h) they press, unless the player put a game's
/// buttons on others (controls.rs). FBNeo maps MK's panel to A S D = high punch, high kick,
/// block and Z X C = low punch, low kick, block. 1 to 4 are the players' Starts (`STARTS`).
pub const KEYS: [(KeyCode, u16); 15] = [
    (KeyCode::ArrowUp, 4),
    (KeyCode::ArrowDown, 5),
    (KeyCode::ArrowLeft, 6),
    (KeyCode::ArrowRight, 7),
    (KeyCode::Digit5, 2), // coin (SELECT)
    (KeyCode::Digit1, 3), // start, P1's
    (KeyCode::Digit2, 16),
    (KeyCode::Digit3, 17),
    (KeyCode::Digit4, 18),
    (KeyCode::KeyA, 1),  // Y
    (KeyCode::KeyS, 9),  // X
    (KeyCode::KeyD, 10), // L
    (KeyCode::KeyZ, 0),  // B
    (KeyCode::KeyX, 8),  // A
    (KeyCode::KeyC, 11), // R
];
/// The RetroPad buttons a lightgun game's mouse presses, which press its gun's
/// (web/emulator/libretro.js): the trigger is B, as Z, and Aux A (Time Crisis II's pedal) is A,
/// as X.
pub const TRIGGER: u16 = 0;
pub const PEDAL: u16 = 8;
/// Player 1 to 4's Start, as the keys have them: the RetroPad's (START) for P1, then ids past
/// its 16 buttons for the others. At a game whose seats the player picks (`PlayingGame::starts`)
/// the player's own seat's presses Start, and another's sits them there if it's free, and
/// starts them, as at an arcade's control panel; elsewhere P1's is Start and the others nothing.
pub const STARTS: [u16; 4] = [START, 16, 17, 18];
const START: u16 = 3;
/// The crosshair across, in logical pixels.
const CROSSHAIR: f32 = 22.0;

thread_local! {
    static LATEST_FRAME: RefCell<Option<(UVec2, Vec<u8>)>> = const { RefCell::new(None) };
    static LATEST_STATUS: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The seat the player plays in, once the page says.
    static SEAT: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Called by index.html with each RGBA frame the emulator worker posts.
#[wasm_bindgen]
pub fn push_frame(rgba: Vec<u8>, width: u32, height: u32) {
    LATEST_FRAME.set(Some((UVec2::new(width, height), rgba)));
}

/// Called by index.html with a line about the game: who you play with, how the connection is.
#[wasm_bindgen]
pub fn game_status(text: String) {
    LATEST_STATUS.set(Some(text));
}

/// Called by index.html with the seat the player plays in, when they sit and when they move to
/// another (pressing its Start): 0 for player 1.
#[wasm_bindgen]
pub fn game_seat(seat: u32) {
    SEAT.set(Some(seat));
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Sits the player at `cabinet` ("x,y") and starts its game, for up to `players` at once;
    /// `files` are what the core needs next to the ROM besides the BIOS (paths under /roms/, a
    /// NAOMI game's disc), `lockstep` games don't roll back online, `gun` games answer the
    /// core's lightgun from the aim, `linked` games give each player their own board, linked to
    /// the others' (paced like lockstep), `options` (JSON) are the core's settings, `arcade`
    /// games are linked cabinets, each player's browser running only their own, free-running,
    /// a driving game's `wheel` (JSON) has the arrows turn a wheel, and at a game whose seats
    /// are `characters` the player sits as the one in `seat`, if it's still free (world::Game).
    #[wasm_bindgen(js_name = emulatorPlay)]
    fn emulator_play(
        core: &str,
        rom: &str,
        bios: Option<String>,
        files: Vec<String>,
        cabinet: &str,
        turns: bool,
        players: u32,
        lockstep: bool,
        gun: bool,
        linked: bool,
        options: &str,
        arcade: bool,
        wheel: Option<String>,
        characters: Vec<String>,
        seat: Option<u32>,
    );
    /// Watches the game at `cabinet` ("x,y"), streamed from one of its players (at a `linked`
    /// game, the lowest seat's own board; an `arcade` game's: one player's cabinet at a time).
    #[wasm_bindgen(js_name = emulatorWatch)]
    fn emulator_watch(
        core: &str,
        rom: &str,
        bios: Option<String>,
        files: Vec<String>,
        cabinet: &str,
        turns: bool,
        gun: bool,
        linked: bool,
        options: &str,
        arcade: bool,
        wheel: Option<String>,
    );
    #[wasm_bindgen(js_name = emulatorStop)]
    fn emulator_stop();
    /// Sends the player's controls to the worker, for their seat's controller: the RetroPad
    /// mask in the low 16 bits, and at a lightgun game where it aims in the high 16, x in bits
    /// 16-23 (0 the screen's left edge, 255 its right) and y in bits 24-31 (0 the top).
    #[wasm_bindgen(js_name = emulatorInput)]
    fn emulator_input(input: u32);
    /// Moves the player to `seat` at the cabinet they play at, if it's free, and starts them
    /// there: they pressed that player's Start.
    #[wasm_bindgen(js_name = emulatorStartAs)]
    fn emulator_start_as(seat: u32);
    /// How the arrows turn the wheel of the driving game being played, from now on: a
    /// world::Wheel as JSON, whose `lock`, `back` and `curve` the worker takes (its `span` is
    /// the game's, the same on every machine).
    #[wasm_bindgen(js_name = emulatorWheel)]
    fn emulator_wheel(ramp: &str);
}

/// The game being played or watched, while `Mode::Playing`.
#[derive(Resource)]
pub struct PlayingGame {
    pub title: String,
    /// Its ROM set, which names it in assets/games.ron.
    pub rom: String,
    pub watching: bool,
    /// A lightgun game: the player aims (world::Game).
    pub gun: bool,
    /// A driving game: how the steering keys turn its wheel, as the player has it now, and as
    /// assets/games.ron has it.
    pub wheel: Option<Wheel>,
    pub usual_wheel: Option<Wheel>,
    /// How many players' Starts sit the player in that seat (`STARTS`): the seats of a game
    /// they pick a seat at (cabinets.rs), none at others.
    pub starts: u32,
}

impl PlayingGame {
    /// The player aims a lightgun at the game (not when watching).
    pub fn aims(&self) -> bool {
        self.gun && !self.watching
    }

    /// The player steers a wheel the arrows turn (not when watching).
    pub fn steers(&self) -> bool {
        self.wheel.is_some() && !self.watching
    }
}

/// Starts the game at the cabinet in `cell`, its wheel (a driving game's) turning as `wheel`
/// says, in `seat` if it's free (a character picked) or else the first free one; switch to
/// `Mode::Playing` to show it.
pub fn play(cell: IVec2, game: &Game, wheel: Option<Wheel>, seat: Option<u32>) {
    LATEST_FRAME.set(None);
    LATEST_STATUS.set(None);
    SEAT.set(None);
    let cabinet = cabinet_id(cell);
    let bios = game.bios.clone();
    emulator_play(
        &game.core,
        &game.rom,
        bios,
        game.files.clone(),
        &cabinet,
        game.turns,
        game.players,
        game.lockstep,
        game.gun,
        game.linked,
        &options(game),
        game.arcade,
        wheel.as_ref().map(to_json),
        game.characters.clone(),
        seat,
    );
}

/// Watches the game being played at the cabinet in `cell`; switch to `Mode::Playing` to show it.
pub fn watch(cell: IVec2, game: &Game) {
    LATEST_FRAME.set(None);
    LATEST_STATUS.set(None);
    let cabinet = cabinet_id(cell);
    let bios = game.bios.clone();
    emulator_watch(
        &game.core,
        &game.rom,
        bios,
        game.files.clone(),
        &cabinet,
        game.turns,
        game.gun,
        game.linked,
        &options(game),
        game.arcade,
        game.wheel.as_ref().map(to_json),
    );
}

/// The game's settings for its core, as JSON for the page.
fn options(game: &Game) -> String {
    serde_json::to_string(&game.options).unwrap_or_else(|_| "{}".into())
}

/// A wheel as JSON for the page.
fn to_json(wheel: &Wheel) -> String {
    serde_json::to_string(wheel).unwrap_or_default()
}

/// Has the steering keys turn the wheel of the driving game being played as `wheel` does, from
/// the next frame on. Only this player's wheel turns otherwise: it goes to the others in the
/// input, already turned.
pub fn turn_wheel(game: &mut PlayingGame, wheel: Wheel) {
    game.wheel = Some(wheel);
    emulator_wheel(&to_json(&wheel));
}

/// How the page and the room name a cabinet: its cell, "x,y".
pub fn cabinet_id(cell: IVec2) -> String {
    format!("{},{}", cell.x, cell.y)
}

pub struct EmulatorPlugin;

impl Plugin for EmulatorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>()
            .init_resource::<Sent>()
            .add_systems(Update, tune_wheel)
            .add_systems(OnEnter(Mode::Playing), show_screen)
            .add_systems(
                Update,
                (
                    show_latest_frame,
                    fit_screen,
                    show_status,
                    aim.after(fit_screen),
                    send_input.after(aim),
                    leave.run_if(chat_closed),
                )
                    .run_if(in_state(Mode::Playing)),
            )
            .add_systems(OnExit(Mode::Playing), stop);
    }
}

/// The black overlay covering the bar while a game plays.
#[derive(Component)]
struct Overlay;

/// The image node that shows the game.
#[derive(Component)]
struct Screen;

/// The line above the game about who you play with.
#[derive(Component)]
struct Status;

/// Where a lightgun game's gun points, over the screen.
#[derive(Component)]
struct Crosshair;

/// Where the player aims at a lightgun game: 0 to 255 across the screen and down it, and
/// whether the pointer (or a finger) is on the screen now, where a click (or the finger) shoots.
#[derive(Resource)]
struct Aim {
    x: u8,
    y: u8,
    on_screen: bool,
    /// A finger aims, which also holds the trigger.
    finger: bool,
}

impl Default for Aim {
    /// The middle of the screen, until the player points.
    fn default() -> Self {
        Self {
            x: 128,
            y: 128,
            on_screen: false,
            finger: false,
        }
    }
}

impl Aim {
    /// The aim in the high half of an input (web/emulator/worker.js).
    fn bits(&self) -> u32 {
        (self.x as u32) << 16 | (self.y as u32) << 24
    }
}

/// The controls last sent to this game's worker, none yet at first.
#[derive(Resource, Default)]
struct Sent(Option<u32>);

fn show_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    touch: Res<Touch>,
    game: Option<Res<PlayingGame>>,
    mut aim: ResMut<Aim>,
    mut sent: ResMut<Sent>,
) {
    *aim = Aim::default();
    sent.0 = None;
    let mut overlay = commands.spawn((
        Overlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            // A phone held upright has the game at the top, and the thumbs below it.
            align_items: if touch.is_on() {
                AlignItems::Start
            } else {
                AlignItems::Center
            },
            ..default()
        },
        BackgroundColor(Color::BLACK),
        GlobalZIndex(1),
        children![
            (
                Screen,
                ImageNode::new(images.add(screen_image(4, 3))),
                Node {
                    height: Val::Percent(100.0),
                    aspect_ratio: Some(4.0 / 3.0),
                    ..default()
                },
            ),
            (
                Status,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(if touch.is_on() { 11.0 } else { 14.0 }),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(8.0),
                    left: Val::Px(8.0),
                    // On a phone, clear of the buttons in the other corner.
                    max_width: if touch.is_on() {
                        Val::Percent(60.0)
                    } else {
                        Val::Auto
                    },
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                // Over the game, which may run under it.
                ZIndex(1),
            ),
        ],
    ));
    if game.as_ref().is_some_and(|game| game.aims()) {
        // A ring with a dot, white edged in black so it shows on any picture; placed by `aim`.
        let edge = Outline::new(Val::Px(1.0), Val::ZERO, Color::BLACK);
        overlay.with_child((
            Crosshair,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR),
                height: Val::Px(CROSSHAIR),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(Color::WHITE),
            edge,
            // Over the game.
            ZIndex(1),
            Visibility::Hidden,
            children![(
                Node {
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::WHITE),
                edge,
            )],
        ));
    }
    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), with the controls
        // panel's gear left of it (controls.rs).
        overlay.with_child((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(80.0),
                ..default()
            },
            ZIndex(1),
            children![touch::button(TouchButton::Leave, "Leave", 64.0, 36.0)],
        ));
    }
}

/// How much of the top of a phone held upright is kept for the status line and Leave.
pub const TOP_BAR: f32 = 52.0;

/// Makes the screen as big as fits: the window's height, or its width in a window taller than
/// the game's monitor.
fn fit_screen(
    window: Single<&Window>,
    touch: Res<Touch>,
    mut overlay: Single<&mut Node, (With<Overlay>, Without<Screen>)>,
    mut screens: Query<&mut Node, With<Screen>>,
) {
    for mut node in &mut screens {
        let Some(aspect) = node.aspect_ratio else {
            continue;
        };
        let tall = window.width() < window.height() * aspect;
        let (width, height) = if tall {
            (Val::Percent(100.0), Val::Auto)
        } else {
            (Val::Auto, Val::Percent(100.0))
        };
        if node.width != width {
            node.width = width;
            node.height = height;
        }
        let top = if tall && touch.is_on() {
            Val::Px(TOP_BAR)
        } else {
            Val::Px(0.0)
        };
        if overlay.padding.top != top {
            overlay.padding.top = top;
        }
    }
}

/// A black image the size of a game frame, for the screen to start with.
fn screen_image(width: u32, height: u32) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn show_latest_frame(
    mut images: ResMut<Assets<Image>>,
    mut screens: Query<(&ImageNode, &mut Node), With<Screen>>,
) {
    let Some((size, rgba)) = LATEST_FRAME.take() else {
        return;
    };
    for (image_node, mut node) in &mut screens {
        let Some(mut image) = images.get_mut(&image_node.image) else {
            continue;
        };
        if image.size() != size {
            *image = screen_image(size.x, size.y);
            // Arcade monitors were 4:3, or 3:4 when mounted vertically (Pac-Man).
            node.aspect_ratio = Some(if size.x >= size.y {
                4.0 / 3.0
            } else {
                3.0 / 4.0
            });
        }
        image.data = Some(rgba.clone());
    }
}

fn show_status(mut status: Single<&mut Text, With<Status>>) {
    if let Some(text) = LATEST_STATUS.take() {
        // For browser tests (testing.rs): the line as shown.
        #[cfg(feature = "test-hooks")]
        crate::testing::report("status", serde_json::Value::String(text.clone()));
        status.0 = text;
    }
}

/// At a lightgun game, follows the mouse (or a finger) over the screen: where it aims, the
/// crosshair there, and the pointer hidden over the screen.
#[allow(clippy::too_many_arguments)]
fn aim(
    game: Option<Res<PlayingGame>>,
    controls: Res<Controls>,
    gear: Query<&Interaction, With<Gear>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    touch: Res<Touch>,
    screen: Single<(&ComputedNode, &UiGlobalTransform), With<Screen>>,
    mut crosshair: Query<(&mut Node, &mut Visibility), With<Crosshair>>,
    mut aim: ResMut<Aim>,
) {
    if !game.is_some_and(|game| game.aims()) {
        return;
    }
    let (computed, transform) = *screen;
    let size = computed.size();
    let pointer = if touch.is_on() {
        touch.finger()
    } else {
        window.cursor_position()
    };
    // Where the pointer is on the screen, 0 to 1 across and down (physical pixels, as UI
    // nodes are laid out in, from the screen's middle).
    let on = pointer
        .filter(|_| size.min_element() > 0.0)
        .and_then(|at| {
            Some(
                transform
                    .try_inverse()?
                    .transform_point2(at * window.scale_factor()),
            )
        })
        .map(|from_middle| from_middle / size + Vec2::splat(0.5));
    // With the controls panel up, or on its gear, the pointer is for that, and shows.
    let on_gear = gear
        .iter()
        .any(|interaction| *interaction != Interaction::None);
    aim.on_screen = !controls.is_open()
        && !on_gear
        && on.is_some_and(|at| at.cmpge(Vec2::ZERO).all() && at.cmple(Vec2::ONE).all());
    aim.finger = touch.is_on() && aim.on_screen;
    // The mouse off the screen aims at its nearest edge; a finger off it (on the black around
    // the game) doesn't aim.
    if let Some(at) = on.filter(|_| aim.on_screen || !touch.is_on()) {
        let at = at.clamp(Vec2::ZERO, Vec2::ONE);
        aim.x = (at.x * 255.0).round() as u8;
        aim.y = (at.y * 255.0).round() as u8;
    }
    if let Ok((mut node, mut visibility)) = crosshair.single_mut() {
        let at = Vec2::new(aim.x as f32, aim.y as f32) / 255.0 - Vec2::splat(0.5);
        let middle = transform.transform_point2(at * size) * computed.inverse_scale_factor();
        let (left, top) = (
            Val::Px(middle.x - CROSSHAIR / 2.0),
            Val::Px(middle.y - CROSSHAIR / 2.0),
        );
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        visibility.set_if_neq(if aim.on_screen {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    // The crosshair stands in for the pointer over the screen; off it, the pointer is back.
    if let Ok(mut cursor) = cursor.single_mut()
        && cursor.visible == aim.on_screen
        && !touch.is_on()
    {
        cursor.visible = !aim.on_screen;
    }
}

#[allow(clippy::too_many_arguments)]
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    chat: Res<Chat>,
    settings: Res<Settings>,
    controls: Res<Controls>,
    touch: Res<Touch>,
    game: Option<Res<PlayingGame>>,
    aim: Res<Aim>,
    mut sent: ResMut<Sent>,
) {
    // While typing in the chat, tuning settings or putting a button on a key, the player's
    // hands are off the controls. With the controls panel up, the steering keys still turn the
    // wheel, to try it. Shift with a number mutes a player (voice.rs), so it isn't a button.
    let busy = chat.is_open() || settings.is_open() || controls.is_binding();
    let trying = controls.is_open();
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let bindings = game.as_ref().map_or(KEYS, |game| controls.keys(&game.rom));
    let usable = |key: &KeyCode, id: &u16| {
        !busy && (!trying || matches!(*id, LEFT | RIGHT)) && !(shift && SEAT_KEYS.contains(key))
    };
    let mut mask = bindings
        .iter()
        .filter(|(key, id)| usable(key, id) && !STARTS.contains(id) && keys.pressed(*key))
        .fold(touch.pad(), |mask, (_, id)| mask | 1 << id);
    // The players' Starts: the player's own seat's is Start, another's moves them there.
    let starts = game
        .as_ref()
        .filter(|game| !game.watching)
        .map_or(0, |game| game.starts);
    let start_keys = bindings
        .iter()
        .filter(|(key, id)| usable(key, id))
        .filter_map(|(key, id)| Some((*key, STARTS.iter().position(|start| start == id)? as u32)));
    for (key, player) in start_keys {
        let own = if starts < 2 {
            player == 0
        } else {
            SEAT.get() == Some(player)
        };
        if own && keys.pressed(key) {
            mask |= 1 << START;
        } else if !own && player < starts && keys.just_pressed(key) {
            emulator_start_as(player);
        }
    }
    let mut input = 0;
    if game.is_some_and(|game| game.aims()) {
        // A click shoots where it aims, on the screen only; a finger on it shoots for as long
        // as it's down.
        let shoot = aim.finger || (aim.on_screen && mouse.pressed(MouseButton::Left));
        let pedal = mouse.pressed(MouseButton::Right) || keys.pressed(KeyCode::Space);
        if shoot && !busy && !trying {
            mask |= 1 << TRIGGER;
        }
        if pedal && !busy && !trying {
            mask |= 1 << PEDAL;
        }
        input = aim.bits();
    }
    input |= mask as u32;
    // A new game's worker starts with nothing pressed, aiming nowhere.
    if sent.0 != Some(input) && (sent.0.is_some() || input != 0) {
        emulator_input(input);
        sent.0 = Some(input);
    }
}

/// `/wheel` in the chat: shows how the steering keys turn the wheel of the driving game being
/// played, and with numbers (`lock=0.5 back=0.1 curve=2`, any of them) turns it so from now on,
/// as the controls panel does, and keeps it so for next time.
fn tune_wheel(
    mut asked: MessageReader<TuneWheel>,
    mode: Res<State<Mode>>,
    mut game: Option<ResMut<PlayingGame>>,
    mut controls: ResMut<Controls>,
    mut chat: ResMut<Chat>,
    time: Res<Time>,
) {
    for TuneWheel(settings) in asked.read() {
        let playing = *mode.get() == Mode::Playing;
        let driving = game.as_deref_mut().filter(|game| playing && !game.watching);
        let said = match driving.and_then(|game| Some((game.wheel?, game))) {
            Some((wheel, game)) => tune(game, wheel, settings, &mut controls),
            None => "* /wheel tunes the steering of a driving game (Out Run, Cruis'n USA), \
                     sitting at it."
                .to_string(),
        };
        chat.say(said, time.elapsed_secs());
    }
}

/// What `/wheel <settings>` does to the `wheel` of the driving game being played, and says.
fn tune(game: &mut PlayingGame, wheel: Wheel, settings: &str, controls: &mut Controls) -> String {
    if settings.trim().is_empty() {
        return format!(
            "* {}'s wheel: {} (seconds to full lock held, seconds back to the middle let go; \
             curve 1 turns evenly, 2 barely for a tap). /wheel lock=0.5 changes any of them, \
             or Tab. For games.ron: {}",
            game.title,
            wheel.ramp(),
            wheel.to_ron()
        );
    }
    match wheel.tuned(settings) {
        Err(error) => format!("* {error}. E.g. /wheel lock=0.6 back=0.1 curve=2"),
        Ok(tuned) => {
            controls.set_game_wheel(&game.rom, tuned, game.usual_wheel);
            controls.keep();
            turn_wheel(game, tuned);
            format!(
                "* {}'s wheel now: {} (kept for next time). For games.ron: {}",
                game.title,
                tuned.ramp(),
                tuned.to_ron()
            )
        }
    }
}

fn leave(keys: Res<ButtonInput<KeyCode>>, touch: Res<Touch>, mut mode: ResMut<NextState<Mode>>) {
    if keys.just_pressed(KeyCode::Escape) || touch.tapped(TouchButton::Leave) {
        mode.set(Mode::Walking);
    }
}

fn stop(
    mut commands: Commands,
    overlays: Query<Entity, With<Overlay>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    emulator_stop();
    LATEST_FRAME.set(None);
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
    // The pointer, if a lightgun game hid it.
    if let Ok(mut cursor) = cursor.single_mut()
        && !cursor.visible
    {
        cursor.visible = true;
    }
}
