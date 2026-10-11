//! The controls panel: which key presses each of a game's buttons, and how a driving game's
//! wheel turns, for every game or for one. The gear in the top right corner opens it: in the
//! bar for every game's, while playing a cabinet's game for that game's own, over every game's
//! (what isn't set for the game is as set for every game, and what isn't set for every game as
//! the game has it: emulator::KEYS, assets/games.ron). `/controls` in the chat does the same,
//! and Tab at a cabinet opens the game's.
//!
//! It lists the buttons (named as the game names them, or by their place on a cabinet's panel
//! in the bar) and the key that presses each: Up and Down pick one, Enter (or a click) and then
//! the key puts it there, and a button that had that key gets the old one. At a driving game
//! (`wheel`: Out Run, Cruis'n USA), and in the bar for all of them, it also has the wheel the
//! steering keys turn (web/emulator/wheel.js): how long a held key takes to full lock, how soon
//! the wheel is straight again once let go, and how little a tap turns it (the curve), changed
//! with - and + or the buttons. The steering keys still turn the wheel while the panel is up,
//! and the panel draws it, how far it's turned and the last two seconds of it, so a tap or a
//! hold shows what it does; a line says what a tap and a hold come to. At a game the wheel is
//! the game's, as the worker has it; in the bar, where no game runs, the panel turns one as the
//! worker would (world::Wheel::turn). While driving, a small wheel in the corner shows it too,
//! unless turned off here.
//!
//! On top, the player's name, as `/name` in the chat sets it: a click (or Enter) on it opens
//! the page's name box (web/index.html), where Enter saves it and Esc doesn't; a canvas can't
//! take typing as a text box does, nor bring up a phone's keyboard.
//!
//! What a player sets is theirs, kept in the browser for next time (localStorage,
//! web/index.html); R puts what the panel is for back as it was. `/wheel` in the chat changes
//! the game's wheel with numbers (emulator.rs).

use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};

use bevy::input::keyboard::KeyboardInput;
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::ui::{UiGlobalTransform, UiTransform};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use world::{Wheel, games_from_ron};

use crate::Mode;
use crate::cabinets::GAMES;
use crate::chat::{Chat, type_in_chat};
use crate::emulator::{self, KEYS, PlayingGame, TOP_BAR};
use crate::help::{GameButtons, close_panel};
use crate::room::OwnName;
use crate::settings::Settings;
use crate::touch::{Touch, TouchButton};

/// The RetroPad's Left and Right (libretro.h), which turn a driving game's wheel.
pub const LEFT: u16 = 6;
pub const RIGHT: u16 = 7;
/// The game's buttons in the order the panel lists them, by RetroPad id: Coin and Start, the
/// stick, then the buttons as their keys lie, A S D over Z X C.
const ORDER: [u16; 12] = [2, 3, 4, 5, 6, 7, 1, 9, 10, 0, 8, 11];
/// What to call a button the game doesn't name, by RetroPad id: the six by their place on a
/// cabinet's panel, three over three, as the keys and the touch pad have them.
const UNNAMED: [&str; 12] = [
    "Bottom left",
    "Top left",
    "Coin",
    "Start",
    "Up",
    "Down",
    "Left",
    "Right",
    "Bottom middle",
    "Top middle",
    "Top right",
    "Bottom right",
];

/// Keys a button can go on, by the name the panel shows and the browser keeps. Not the bar's
/// own (THE_BARS), nor Shift, Ctrl, Alt or Cmd: Shift with a number mutes someone (voice.rs),
/// and the others make the browser's shortcuts (Ctrl+W closes the tab, Alt+Left goes back).
const KEY_NAMES: &[(KeyCode, &str)] = &[
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Space, "Space"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::Backquote, "`"),
    (KeyCode::Numpad0, "Num 0"),
    (KeyCode::Numpad1, "Num 1"),
    (KeyCode::Numpad2, "Num 2"),
    (KeyCode::Numpad3, "Num 3"),
    (KeyCode::Numpad4, "Num 4"),
    (KeyCode::Numpad5, "Num 5"),
    (KeyCode::Numpad6, "Num 6"),
    (KeyCode::Numpad7, "Num 7"),
    (KeyCode::Numpad8, "Num 8"),
    (KeyCode::Numpad9, "Num 9"),
    (KeyCode::NumpadEnter, "Num Enter"),
    (KeyCode::NumpadAdd, "Num +"),
    (KeyCode::NumpadSubtract, "Num -"),
    (KeyCode::NumpadMultiply, "Num *"),
    (KeyCode::NumpadDivide, "Num /"),
    (KeyCode::NumpadDecimal, "Num ."),
];
/// The bar's keys at a cabinet, which no button can have, and why.
const THE_BARS: [(KeyCode, &str); 3] = [
    (KeyCode::Tab, "Tab opens this panel"),
    (KeyCode::KeyY, "Y opens the chat"),
    (KeyCode::KeyM, "M is your microphone"),
];

/// How far the drawn wheel turns at full lock, in degrees.
const LOCK_DEGREES: f32 = 90.0;
/// How far back the trace under the panel's wheel goes, in seconds, and its lines.
const TRACE_FOR: f32 = 2.0;
const TRACE_LINES: usize = 40;
/// A tap, as the panel works out what one does: about the shortest press a hand makes.
const TAP: f64 = 0.1;

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::srgba(1.0, 1.0, 1.0, 0.6);
const KEY_COLOR: Color = Color::srgb(1.0, 0.82, 0.3);
const HEADING: Color = Color::srgb(0.6, 0.85, 0.6);
const SELECTED: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
const BUTTON: Color = Color::srgba(1.0, 1.0, 1.0, 0.15);
const TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
const WHEEL: Color = Color::srgb(0.85, 0.85, 0.9);

thread_local! {
    static TURNED: Cell<Option<f32>> = const { Cell::new(None) };
}

/// Called by index.html with each frame of a driving game the player steers: where the worker
/// has its wheel, -127 full left to 127 full right (web/emulator/wheel.js).
#[wasm_bindgen]
pub fn wheel_turned(position: f32) {
    TURNED.set(Some(position));
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// What the browser keeps of the player's controls (`Kept` as JSON), if anything.
    #[wasm_bindgen(js_name = controlsKept)]
    fn controls_kept() -> Option<String>;
    #[wasm_bindgen(js_name = keepControls)]
    fn keep_controls(json: &str);
    /// Opens the page's name box, with the player's name in it to change.
    #[wasm_bindgen(js_name = askName)]
    fn ask_name();
}

/// The name of a key a button can go on (KEY_NAMES).
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    KEY_NAMES
        .iter()
        .find(|(named, _)| *named == key)
        .map(|(_, name)| *name)
}

fn named_key(name: &str) -> Option<KeyCode> {
    KEY_NAMES
        .iter()
        .find(|(_, named)| *named == name)
        .map(|(key, _)| *key)
}

/// Which key presses each of a game's buttons, by RetroPad id, in emulator::KEYS's order.
pub type Layout = [(KeyCode, u16); 12];

/// What the browser keeps of a player's controls.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct Kept {
    /// What the player set for every game.
    #[serde(default, skip_serializing_if = "Mine::is_empty")]
    every: Mine,
    /// What the player set for each game of its own, by ROM set.
    #[serde(default)]
    games: BTreeMap<String, Mine>,
    /// The small wheel in the corner while driving is off.
    #[serde(default)]
    no_corner_wheel: bool,
}

/// Controls as a player set them, for every game or for one.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
struct Mine {
    /// Buttons on other keys than those under these (every game's under a game's, the usual
    /// ones, emulator::KEYS, under every game's): RetroPad id to key name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    keys: BTreeMap<u16, String>,
    /// How the wheel turns, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wheel: Option<Ramp>,
}

impl Mine {
    fn is_empty(&self) -> bool {
        *self == Mine::default()
    }
}

/// A world::Wheel's ramp: what of it is the player's (its `span` is the game's).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Ramp {
    lock: f64,
    back: f64,
    curve: f64,
}

impl Ramp {
    fn of(wheel: Wheel) -> Self {
        let Wheel {
            lock, back, curve, ..
        } = wheel;
        Self { lock, back, curve }
    }

    /// `wheel` turning so.
    fn on(self, wheel: Wheel) -> Wheel {
        let Self { lock, back, curve } = self;
        Wheel {
            lock,
            back,
            curve,
            ..wheel
        }
    }
}

/// What the panel sets: every game's controls (in the bar), or one game's (by ROM set) over
/// those (at its cabinet).
#[derive(Clone, PartialEq, Debug, Default)]
enum Scope {
    #[default]
    Every,
    Game(String),
}

/// Puts each of `mine`'s buttons on its key in `keys`, and a button that had that key on the
/// one the button had, so that no key presses two buttons whatever is under them.
fn put_keys(mut keys: Layout, mine: Option<&Mine>) -> Layout {
    for (&id, name) in mine.into_iter().flat_map(|mine| &mine.keys) {
        if let Some(key) = named_key(name) {
            swap(&mut keys, id, key);
        }
    }
    keys
}

/// Puts button `id` on `key`, and a button that had `key` on the one `id` had.
fn swap(keys: &mut Layout, id: u16, key: KeyCode) {
    let Some(&(old, _)) = keys.iter().find(|(_, on)| *on == id) else {
        return;
    };
    for (had, on) in keys {
        if *on == id {
            *had = key;
        } else if *had == key {
            *had = old;
        }
    }
}

#[derive(Resource, Default)]
pub struct Controls {
    open: bool,
    /// What the panel up sets.
    scope: Scope,
    /// The row picked, and whether the next key goes on its button.
    selected: usize,
    binding: bool,
    /// A word for the player under the rows: why a key didn't go on a button.
    note: String,
    kept: Kept,
    /// The first driving game in assets/games.ron, by title, and its wheel: every game's wheel
    /// is shown from it until the player sets one.
    first_wheel: Option<(String, Wheel)>,
}

impl Controls {
    /// The panel is up: keys are for it, but for the steering keys.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The panel waits for a key to put a button on: every key is for that.
    pub fn is_binding(&self) -> bool {
        self.open && self.binding
    }

    /// The keys that press `rom`'s buttons, with the RetroPad id each presses.
    pub fn keys(&self, rom: &str) -> Layout {
        put_keys(self.every_keys(), self.kept.games.get(rom))
    }

    /// The keys as set for every game.
    fn every_keys(&self) -> Layout {
        put_keys(KEYS, Some(&self.kept.every))
    }

    /// The keys as `scope` has them, and as what's under it has them.
    fn keys_of(&self, scope: &Scope) -> (Layout, Layout) {
        match scope {
            Scope::Every => (self.every_keys(), KEYS),
            Scope::Game(rom) => (self.keys(rom), self.every_keys()),
        }
    }

    /// How the keys turn the wheel of `rom`, whose own is `usual` (assets/games.ron): as the
    /// player set it for the game, or else for every game.
    pub fn wheel(&self, rom: &str, usual: Option<Wheel>) -> Option<Wheel> {
        let usual = usual?;
        let mine = self.kept.games.get(rom).and_then(|mine| mine.wheel);
        Some(mine.map_or_else(|| self.every_wheel(usual), |ramp| ramp.on(usual)))
    }

    /// Every game's wheel, as the player set it, on `wheel` (a game's own).
    fn every_wheel(&self, wheel: Wheel) -> Wheel {
        self.kept.every.wheel.map_or(wheel, |ramp| ramp.on(wheel))
    }

    /// Every game's wheel as the panel in the bar shows it: as set, or the first driving game's.
    fn shown_every_wheel(&self) -> Option<Wheel> {
        let (_, first) = self.first_wheel.as_ref()?;
        Some(self.every_wheel(*first))
    }

    /// Has the wheel turn as `wheel` does: every game's, or a game's whose own is `usual`.
    fn set_wheel(&mut self, scope: &Scope, wheel: Wheel, usual: Option<Wheel>) {
        let under = match scope {
            Scope::Every => None,
            Scope::Game(_) => usual.map(|usual| self.every_wheel(usual)),
        };
        let set = (Some(wheel) != under).then_some(Ramp::of(wheel));
        self.mine(scope).wheel = set;
        self.forget_if_empty(scope);
    }

    /// Has `rom`'s wheel, whose own is `usual`, turn as `wheel` does (`/wheel` in the chat).
    pub fn set_game_wheel(&mut self, rom: &str, wheel: Wheel, usual: Option<Wheel>) {
        self.set_wheel(&Scope::Game(rom.to_string()), wheel, usual);
    }

    /// The small wheel shows in the corner while driving.
    pub fn corner_wheel(&self) -> bool {
        !self.kept.no_corner_wheel
    }

    /// Puts button `id` on `key` for `scope`; a button that had it gets the key `id` had.
    fn bind(&mut self, scope: &Scope, id: u16, key: KeyCode) {
        let (mut keys, under) = self.keys_of(scope);
        swap(&mut keys, id, key);
        // What's not as under them, which put_keys puts back so.
        self.mine(scope).keys = keys
            .iter()
            .zip(under)
            .filter(|((key, _), (was, _))| key != was)
            .filter_map(|((key, id), _)| Some((*id, key_name(*key)?.to_string())))
            .collect();
        self.forget_if_empty(scope);
    }

    /// What `scope` set, forgotten.
    fn reset(&mut self, scope: &Scope) {
        match scope {
            Scope::Every => self.kept.every = Mine::default(),
            Scope::Game(rom) => {
                self.kept.games.remove(rom);
            }
        }
    }

    fn mine(&mut self, scope: &Scope) -> &mut Mine {
        match scope {
            Scope::Every => &mut self.kept.every,
            Scope::Game(rom) => self.kept.games.entry(rom.clone()).or_default(),
        }
    }

    fn forget_if_empty(&mut self, scope: &Scope) {
        if let Scope::Game(rom) = scope
            && self.kept.games.get(rom).is_some_and(Mine::is_empty)
        {
            self.kept.games.remove(rom);
        }
    }

    fn open(&mut self, scope: Scope) {
        self.open = true;
        self.scope = scope;
        self.selected = 0;
        self.binding = false;
        self.note.clear();
    }

    /// Has the browser keep it all, for next time.
    pub fn keep(&self) {
        if let Ok(json) = serde_json::to_string(&self.kept) {
            keep_controls(&json);
        }
    }
}

/// Run condition: the panel isn't up (in the bar, the arrows are for it, not for walking).
pub fn controls_closed(controls: Res<Controls>) -> bool {
    !controls.open
}

/// Opens the panel (`/controls` in the chat).
#[derive(Message)]
pub struct ShowControls;

/// A line of the panel: the player's name, a button's key, a number of the wheel's, or the
/// corner wheel.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    Name,
    Key(u16),
    Lock,
    Back,
    Curve,
    Corner,
}

/// The panel's lines: the player's name, then the buttons of a game whose buttons are
/// `buttons` (all, until the game says, and in the bar), and the wheel if the player `steers`
/// one. A touch screen has no keys to put buttons on.
fn rows(buttons: &[(u16, String)], steers: bool, touch: bool) -> Vec<Row> {
    let named = |id: &u16| buttons.is_empty() || buttons.iter().any(|(on, _)| on == id);
    let mut rows = vec![Row::Name];
    if !touch {
        rows.extend(ORDER.into_iter().filter(named).map(Row::Key));
    }
    if steers {
        rows.extend([Row::Lock, Row::Back, Row::Curve, Row::Corner]);
    }
    rows
}

/// What the panel shows, for what it sets.
struct View {
    /// "Every game", or the game's title.
    title: String,
    rows: Vec<Row>,
    /// What the game calls its buttons, by RetroPad id (none in the bar).
    names: Vec<(u16, String)>,
    keys: Layout,
    /// The wheel the steering keys turn, as set: a driving game's, or every game's.
    wheel: Option<Wheel>,
}

impl View {
    /// What the panel up shows: in the bar, every game's controls; at a game, the game's, if
    /// the player plays `game` (not watching).
    fn of(
        controls: &Controls,
        game: Option<&PlayingGame>,
        buttons: &[(u16, String)],
        touch: bool,
    ) -> Option<View> {
        match &controls.scope {
            Scope::Every => {
                let wheel = controls.shown_every_wheel();
                Some(View {
                    title: "Every game".into(),
                    rows: rows(&[], wheel.is_some(), touch),
                    names: Vec::new(),
                    keys: controls.every_keys(),
                    wheel,
                })
            }
            Scope::Game(rom) => {
                let game = game.filter(|game| !game.watching && game.rom == *rom)?;
                Some(View {
                    title: game.title.clone(),
                    rows: rows(buttons, game.steers(), touch),
                    names: buttons.to_vec(),
                    keys: controls.keys(rom),
                    wheel: game.wheel.filter(|_| game.steers()),
                })
            }
        }
    }

    /// What button `id` is called.
    fn name(&self, id: u16) -> &str {
        self.names
            .iter()
            .find(|(on, _)| *on == id)
            .map_or(UNNAMED[id as usize], |(_, name)| name.as_str())
    }
}

/// A number of the wheel's, as the panel changes it.
struct Knob {
    label: &'static str,
    unit: &'static str,
    min: f64,
    max: f64,
    step: f64,
    decimals: usize,
}

impl Row {
    fn knob(self) -> Option<Knob> {
        let (label, unit, min, max, step, decimals) = match self {
            Row::Lock => ("Full lock in", " s", 0.05, 2.0, 0.05, 2),
            Row::Back => ("Straight again in", " s", 0.0, 2.0, 0.05, 2),
            Row::Curve => ("Tap curve", "", 0.5, 4.0, 0.1, 1),
            _ => return None,
        };
        Some(Knob {
            label,
            unit,
            min,
            max,
            step,
            decimals,
        })
    }

    fn of(self, wheel: &mut Wheel) -> Option<&mut f64> {
        match self {
            Row::Lock => Some(&mut wheel.lock),
            Row::Back => Some(&mut wheel.back),
            Row::Curve => Some(&mut wheel.curve),
            _ => None,
        }
    }
}

/// `wheel` with `row`'s number moved by `steps` of its step, within its range.
fn nudge(mut wheel: Wheel, row: Row, steps: f64) -> Wheel {
    let (Some(knob), Some(value)) = (row.knob(), row.of(&mut wheel)) else {
        return wheel;
    };
    // On whole steps, so repeated nudges don't drift, and as it reads.
    let moved = ((*value + knob.step * steps) / knob.step).round() * knob.step;
    *value = (moved.clamp(knob.min, knob.max) * 100.0).round() / 100.0;
    wheel
}

/// What the wheel comes to: a tap, and a key held and let go.
fn foretell(wheel: &Wheel) -> String {
    let half = wheel.lock * 0.5_f64.powf(1.0 / wheel.curve);
    format!(
        "A tap ({TAP} s) turns it {:.0}%. Held: half lock in {half:.2} s, full lock in {:.2} \
         s. Let go: straight in {:.2} s.",
        wheel.held(TAP) * 100.0,
        wheel.lock,
        wheel.back,
    )
}

/// Where the wheel the panel draws is: the game's as the worker has it, or in the bar the
/// panel's own.
#[derive(Resource, Default)]
struct Turned {
    /// -1 full left to 1 full right.
    now: f32,
    /// Where it was, by when (seconds since start), the latest last, as far back as the trace
    /// goes.
    trace: VecDeque<(f32, f32)>,
}

impl Turned {
    /// Where it was `ago` seconds before `now`: straight before anything was heard.
    fn was(&self, now: f32, ago: f32) -> f32 {
        self.trace
            .iter()
            .rev()
            .find(|(at, _)| now - at >= ago)
            .map_or(0.0, |(_, turned)| *turned)
    }
}

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Controls>()
            .init_resource::<Turned>()
            .add_message::<ShowControls>()
            .add_systems(Startup, (load, spawn_panel, spawn_gear))
            // After the chat and the help panel have had the keys, and before anything else
            // reads them, so the keys for the panel do nothing else.
            .add_systems(
                PreUpdate,
                use_keys
                    .after(InputSystems)
                    .after(type_in_chat)
                    .after(close_panel),
            )
            .add_systems(OnEnter(Mode::Playing), spawn_corner_wheel)
            .add_systems(
                Update,
                (
                    open_from_chat,
                    use_gear,
                    show_gear,
                    use_pointer,
                    follow_wheel,
                    show_panel,
                    show_wheels,
                    show_corner_wheel,
                )
                    .chain(),
            )
            .add_systems(OnExit(Mode::Playing), close);
        #[cfg(feature = "test-hooks")]
        app.add_systems(Update, report.after(follow_wheel));
    }
}

/// For browser tests (testing.rs, tools/e2e/controls.mjs): the panel and what it sets ("every"
/// or the game's ROM set), and at a game (in the bar, for every game) its keys by RetroPad id,
/// its wheel and where it is, how many buttons the game named, and the player's name.
#[cfg(feature = "test-hooks")]
fn report(
    controls: Res<Controls>,
    turned: Res<Turned>,
    touch: Res<Touch>,
    buttons: Res<GameButtons>,
    mode: Res<State<Mode>>,
    own: Res<OwnName>,
    game: Option<Res<PlayingGame>>,
) {
    use serde_json::json;
    let game = game.filter(|game| *mode.get() == Mode::Playing && !game.watching);
    let (keys, rows, wheel) = match &game {
        Some(game) => (
            controls.keys(&game.rom),
            rows(&buttons.0, game.steers(), touch.is_on()),
            game.wheel,
        ),
        None => (
            controls.every_keys(),
            rows(&[], true, touch.is_on()),
            controls.shown_every_wheel(),
        ),
    };
    let keys: BTreeMap<String, &str> = keys
        .iter()
        .map(|(key, id)| (id.to_string(), key_name(*key).unwrap_or("?")))
        .collect();
    let rows: Vec<String> = rows.iter().map(|row| format!("{row:?}")).collect();
    let wheel =
        wheel.map(|wheel| json!({ "lock": wheel.lock, "back": wheel.back, "curve": wheel.curve }));
    let scope = match &controls.scope {
        Scope::Every => "every",
        Scope::Game(rom) => rom.as_str(),
    };
    crate::testing::report(
        "controls",
        json!({
            "open": controls.open,
            "scope": scope,
            "binding": controls.binding,
            "selected": controls.selected,
            "note": controls.note,
            "rows": rows,
            "named": buttons.0.len(),
            "keys": keys,
            "wheel": wheel,
            "turned": turned.now,
            "corner": controls.corner_wheel(),
            "name": own.0,
        }),
    );
}

fn load(mut controls: ResMut<Controls>) {
    let kept = controls_kept().and_then(|json| serde_json::from_str(&json).ok());
    controls.kept = kept.unwrap_or_default();
    let games = games_from_ron(GAMES).unwrap_or_default();
    controls.first_wheel = games
        .into_iter()
        .find_map(|game| Some((game.title, game.wheel?)));
}

/// The panel's screen-wide layer; its box is built each time something in it changes.
#[derive(Component)]
struct Panel;

/// A line of the panel, by its place among the rows.
#[derive(Component)]
struct PanelRow(usize);

/// The panel's buttons: - and + of a row, and which way they go, Reset and Close.
#[derive(Component, Clone, Copy)]
enum PanelButton {
    Step(usize, f64),
    Reset,
    Close,
}

/// A drawn wheel, turned as the game's is.
#[derive(Component)]
struct Dial;

/// How far the game's wheel is turned, over a bar `across` wide: now, or a line of the trace,
/// `line` lines back.
#[derive(Component)]
struct Fill {
    across: f32,
    line: Option<usize>,
}

/// "Left 45%".
#[derive(Component)]
struct Reading;

/// The small wheel in the corner, while driving.
#[derive(Component)]
struct CornerWheel;

/// The gear that opens the panel, in the bar and at a game the player plays (not watching).
/// Its hover takes the mouse off a lightgun game's screen (emulator.rs), so a click on it
/// doesn't also shoot.
#[derive(Component)]
pub struct Gear;

/// "Controls", by the gear while the mouse is on it.
#[derive(Component)]
struct GearLabel;

/// The gear across, and its box.
const GEAR: f32 = 22.0;
const GEAR_BOX: f32 = 36.0;
const GEAR_FILL: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
const GEAR_HOVER: Color = Color::srgba(0.25, 0.25, 0.3, 0.9);

/// The gear, in its box, top right: on a touch screen left of the page's Chat button (and at a
/// game, of Leave), where a finger's tap is a button's, not the walking stick's.
fn spawn_gear(mut commands: Commands) {
    commands.spawn((
        Gear,
        TouchButton::Controls,
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            width: Val::Px(GEAR_BOX),
            height: Val::Px(GEAR_BOX),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(GEAR_FILL),
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.5)),
        // Over a game's screen, under the panels.
        GlobalZIndex(2),
        Visibility::Hidden,
        children![
            gear(GEAR),
            (
                GearLabel,
                text("Controls", 13.0, WHITE),
                TextLayout::no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(GEAR_BOX + 6.0),
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                Visibility::Hidden,
            ),
        ],
    ));
}

/// A gear `size` across: four bars crossed for its eight teeth, a disc over them, and a hole.
fn gear(size: f32) -> impl Bundle {
    let tooth = (size * 0.24).round();
    let centred = |across: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Px((size - across) / 2.0),
        top: Val::Px((size - across) / 2.0),
        width: Val::Px(across),
        height: Val::Px(across),
        border_radius: BorderRadius::MAX,
        ..default()
    };
    let bar = |degrees: f32| {
        (
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px((size - tooth) / 2.0),
                width: Val::Px(size),
                height: Val::Px(tooth),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            UiTransform::from_rotation(Rot2::degrees(degrees)),
            BackgroundColor(WHEEL),
        )
    };
    (
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            ..default()
        },
        children![
            bar(0.0),
            bar(45.0),
            bar(90.0),
            bar(135.0),
            (centred((size * 0.72).round()), BackgroundColor(WHEEL)),
            (
                centred((size * 0.3).round()),
                BackgroundColor(Color::srgb(0.1, 0.1, 0.12))
            ),
        ],
    )
}

/// A click or tap on the gear opens the panel, every game's in the bar and the game's at one,
/// or closes it.
fn use_gear(
    clicked: Query<&Interaction, (Changed<Interaction>, With<Gear>)>,
    touch: Res<Touch>,
    mode: Res<State<Mode>>,
    game: Option<Res<PlayingGame>>,
    mut controls: ResMut<Controls>,
) {
    // A finger's tap is the touch controls' (touch.rs); the mouse's, the UI's.
    let pressed = if touch.is_on() {
        touch.tapped(TouchButton::Controls)
    } else {
        clicked
            .iter()
            .any(|interaction| *interaction == Interaction::Pressed)
    };
    if !pressed {
        return;
    }
    if controls.open {
        controls.open = false;
        return;
    }
    match (*mode.get(), game.filter(|game| !game.watching)) {
        (Mode::Walking, _) => controls.open(Scope::Every),
        (Mode::Playing, Some(game)) => controls.open(Scope::Game(game.rom.clone())),
        _ => {}
    }
}

/// Shows the gear in the bar and at a game the player plays, out of the way of the touch
/// screen's buttons there, lit and named while the mouse is on it.
fn show_gear(
    mode: Res<State<Mode>>,
    game: Option<Res<PlayingGame>>,
    touch: Res<Touch>,
    controls: Res<Controls>,
    gear: Single<
        (
            &mut Node,
            &mut Visibility,
            &mut BackgroundColor,
            &Interaction,
        ),
        With<Gear>,
    >,
    mut label: Single<&mut Visibility, (With<GearLabel>, Without<Gear>)>,
) {
    let (mut node, mut visibility, mut fill, interaction) = gear.into_inner();
    let playing = game.is_some_and(|game| !game.watching);
    // On a touch screen: left of the page's Chat button, and at a game of Leave too.
    let right = match (*mode.get(), touch.is_on()) {
        (Mode::Walking, false) => Some(8.0),
        (Mode::Walking, true) => Some(80.0),
        (Mode::Playing, false) if playing => Some(8.0),
        (Mode::Playing, true) if playing => Some(152.0),
        _ => None,
    };
    visibility.set_if_neq(if right.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    let right = Val::Px(right.unwrap_or(8.0));
    if node.right != right {
        node.right = right;
    }
    if touch.is_on() {
        return;
    }
    let hovered = *interaction != Interaction::None;
    let wanted = if hovered { GEAR_HOVER } else { GEAR_FILL };
    if fill.0 != wanted {
        fill.0 = wanted;
    }
    label.set_if_neq(if hovered && !controls.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}

fn spawn_panel(mut commands: Commands, touch: Res<Touch>) {
    commands.spawn((
        Panel,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            // A phone held upright keeps the pad under it, to steer with.
            align_items: if touch.is_on() {
                AlignItems::Start
            } else {
                AlignItems::Center
            },
            padding: UiRect::top(Val::Px(if touch.is_on() { TOP_BAR } else { 0.0 })),
            ..default()
        },
        // Over the game, and its card.
        GlobalZIndex(3),
        Visibility::Hidden,
    ));
}

/// `/controls` in the chat: every game's in the bar, the game's while playing one.
fn open_from_chat(
    mut asked: MessageReader<ShowControls>,
    mode: Res<State<Mode>>,
    game: Option<Res<PlayingGame>>,
    settings: Res<Settings>,
    mut controls: ResMut<Controls>,
    mut chat: ResMut<Chat>,
    time: Res<Time>,
) {
    if asked.read().count() == 0 || settings.is_open() {
        return;
    }
    match *mode.get() {
        Mode::Walking => controls.open(Scope::Every),
        Mode::Playing => match game.filter(|game| !game.watching) {
            Some(game) => controls.open(Scope::Game(game.rom.clone())),
            None => chat.say(
                "* /controls sets the keys of a game you play, or every game's in the bar.".into(),
                time.elapsed_secs(),
            ),
        },
        _ => chat.say(
            "* /controls sets every game's keys in the bar, or a game's at its cabinet.".into(),
            time.elapsed_secs(),
        ),
    }
}

fn close(mut controls: ResMut<Controls>, mut turned: ResMut<Turned>) {
    controls.open = false;
    controls.binding = false;
    *turned = Turned::default();
}

/// Tab opens the panel while playing; while it's up, the keys pick a row, put a button on a
/// key, change the wheel and close it.
#[allow(clippy::too_many_arguments)]
fn use_keys(
    mut keyboard: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    chat: Res<Chat>,
    settings: Res<Settings>,
    mode: Res<State<Mode>>,
    touch: Res<Touch>,
    buttons: Res<GameButtons>,
    mut game: Option<ResMut<PlayingGame>>,
    mut controls: ResMut<Controls>,
) {
    let playing = *mode.get() == Mode::Playing && game.as_ref().is_some_and(|game| !game.watching);
    // Every game's panel is the bar's, a game's that game's.
    let in_place = match &controls.scope {
        Scope::Every => *mode.get() == Mode::Walking,
        Scope::Game(rom) => playing && game.as_ref().is_some_and(|game| game.rom == *rom),
    };
    if controls.open && (!in_place || settings.is_open()) {
        controls.open = false;
        controls.binding = false;
    }
    if chat.is_open() || settings.is_open() {
        keyboard.clear();
        return;
    }
    if !controls.open {
        let tab = keyboard
            .read()
            .any(|key| key.state == ButtonState::Pressed && key.key_code == KeyCode::Tab);
        if let Some(game) = game.as_ref().filter(|_| tab && playing) {
            controls.open(Scope::Game(game.rom.clone()));
            keys.clear_just_pressed(KeyCode::Tab);
        }
        return;
    }
    let Some(view) = View::of(&controls, game.as_deref(), &buttons.0, touch.is_on()) else {
        return;
    };
    let five = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let steps = if five { 5.0 } else { 1.0 };
    for key in keyboard.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        let row = view.rows.get(controls.selected).copied();
        if controls.binding {
            if key.repeat {
                continue;
            }
            let (Some(Row::Key(id)), Some(_)) = (row, key_name(key.key_code)) else {
                // Esc changes nothing; the bar's keys are taken.
                controls.note = match THE_BARS.iter().find(|(bars, _)| *bars == key.key_code) {
                    _ if key.key_code == KeyCode::Escape => String::new(),
                    Some((_, why)) => format!("{why}: pick another key."),
                    None => "That key can't be used: pick another.".into(),
                };
                controls.binding = key.key_code != KeyCode::Escape;
                continue;
            };
            let scope = controls.scope.clone();
            controls.bind(&scope, id, key.key_code);
            controls.binding = false;
            controls.note.clear();
            controls.keep();
            continue;
        }
        let count = view.rows.len().max(1);
        match key.key_code {
            KeyCode::ArrowUp => controls.selected = (controls.selected + count - 1) % count,
            KeyCode::ArrowDown => controls.selected = (controls.selected + 1) % count,
            KeyCode::Enter | KeyCode::NumpadEnter if !key.repeat => {
                if let Some(row) = row {
                    choose(&mut controls, row);
                }
            }
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                if let Some(row) = row {
                    step(&mut controls, game.as_mut(), row, -steps);
                }
            }
            KeyCode::Equal | KeyCode::NumpadAdd => {
                if let Some(row) = row {
                    step(&mut controls, game.as_mut(), row, steps);
                }
            }
            KeyCode::KeyR if !key.repeat => reset(&mut controls, game.as_mut()),
            KeyCode::Escape | KeyCode::Tab => controls.open = false,
            _ => {}
        }
    }
    // The keys were for the panel: Esc doesn't also stand up, nor E sit, nor M turn the
    // microphone.
    let pressed: Vec<KeyCode> = keys.get_just_pressed().copied().collect();
    for key in pressed {
        keys.clear_just_pressed(key);
    }
}

/// Enter or a click on `row`: the name opens the page's name box, a button's waits for its
/// key, the corner wheel goes on or off.
fn choose(controls: &mut Controls, row: Row) {
    match row {
        Row::Name => ask_name(),
        Row::Key(_) => {
            controls.binding = true;
            controls.note.clear();
        }
        Row::Corner => {
            controls.kept.no_corner_wheel = !controls.kept.no_corner_wheel;
            controls.keep();
        }
        _ => {}
    }
}

/// Moves `row`'s number of the wheel by `steps` of its step, and keeps it: every game's, or
/// the game being played's (`game`, borrowed mutably only to change it), from its next frame
/// on.
fn step(controls: &mut Controls, game: Option<&mut ResMut<PlayingGame>>, row: Row, steps: f64) {
    let scope = controls.scope.clone();
    let Scope::Game(_) = scope else {
        let Some(wheel) = controls.shown_every_wheel() else {
            return;
        };
        controls.set_wheel(&scope, nudge(wheel, row, steps), None);
        controls.keep();
        return;
    };
    let Some(game) = game else {
        return;
    };
    let Some(wheel) = game.wheel else {
        return;
    };
    let turned = nudge(wheel, row, steps);
    if turned != wheel {
        controls.set_wheel(&scope, turned, game.usual_wheel);
        controls.keep();
        emulator::turn_wheel(game, turned);
    }
}

/// What the panel sets, as it was: every game's controls as usual, or the game's as every
/// game's.
fn reset(controls: &mut Controls, game: Option<&mut ResMut<PlayingGame>>) {
    let scope = controls.scope.clone();
    controls.reset(&scope);
    controls.binding = false;
    controls.note.clear();
    controls.keep();
    if let (Scope::Game(rom), Some(game)) = (&scope, game)
        && let Some(wheel) = controls.wheel(rom, game.usual_wheel)
        && game.wheel != Some(wheel)
    {
        emulator::turn_wheel(game, wheel);
    }
}

/// A click or tap on a line picks it (and a button's waits for its key, or the corner wheel
/// goes on or off); on - or +, Reset or Close, does that.
#[allow(clippy::too_many_arguments)]
fn use_pointer(
    window: Single<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    touch: Res<Touch>,
    buttons: Res<GameButtons>,
    mut game: Option<ResMut<PlayingGame>>,
    mut controls: ResMut<Controls>,
    rows_at: Query<(&PanelRow, &ComputedNode, &UiGlobalTransform)>,
    buttons_at: Query<(&PanelButton, &ComputedNode, &UiGlobalTransform)>,
) {
    if !controls.open {
        return;
    }
    let Some(view) = View::of(&controls, game.as_deref(), &buttons.0, touch.is_on()) else {
        return;
    };
    let clicks = mouse
        .just_pressed(MouseButton::Left)
        .then(|| window.cursor_position())
        .flatten();
    let taps = touches.iter_just_pressed().map(|finger| finger.position());
    for at in clicks.into_iter().chain(taps) {
        let point = at * window.scale_factor();
        let button = buttons_at
            .iter()
            .find(|(_, node, transform)| node.contains_point(**transform, point))
            .map(|(button, ..)| *button);
        let row = rows_at
            .iter()
            .find(|(_, node, transform)| node.contains_point(**transform, point))
            .map(|(row, ..)| row.0);
        if let Some(index) = row {
            controls.selected = index;
            controls.binding = false;
            if button.is_none()
                && let Some(&row) = view.rows.get(index)
            {
                choose(&mut controls, row);
            }
        }
        match button {
            Some(PanelButton::Step(index, way)) => {
                if let Some(&row) = view.rows.get(index) {
                    step(&mut controls, game.as_mut(), row, way);
                }
            }
            Some(PanelButton::Reset) => reset(&mut controls, game.as_mut()),
            Some(PanelButton::Close) => controls.open = false,
            None => {}
        }
    }
}

/// Follows the wheel and keeps the last of it for the trace: at a game, where the worker has
/// it; in the bar, with the panel up, a wheel of the panel's own, turned by every game's
/// steering keys as the worker would turn a game's.
fn follow_wheel(
    time: Res<Time>,
    mode: Res<State<Mode>>,
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<Chat>,
    controls: Res<Controls>,
    mut turned: ResMut<Turned>,
) {
    if *mode.get() == Mode::Playing {
        if let Some(position) = TURNED.take() {
            turned.now = (position / 127.0).clamp(-1.0, 1.0);
        }
    } else if let Some(wheel) = controls
        .shown_every_wheel()
        .filter(|_| controls.open && controls.scope == Scope::Every)
    {
        let steering = controls.every_keys();
        let held = |id: u16| {
            !chat.is_open()
                && steering
                    .iter()
                    .any(|(key, on)| *on == id && keys.pressed(*key))
        };
        let toward = match (held(LEFT), held(RIGHT)) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        let fps = 1.0 / time.delta_secs_f64().max(0.001);
        turned.now = wheel.turn(turned.now as f64, toward, fps) as f32;
    } else {
        return;
    }
    let now = time.elapsed_secs();
    let at = turned.now;
    turned.trace.push_back((now, at));
    while turned
        .trace
        .front()
        .is_some_and(|(then, _)| now - then > TRACE_FOR + 0.25)
    {
        turned.trace.pop_front();
    }
}

fn text(content: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(content),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// A box with `label` in it, to click.
fn button(button: PanelButton, label: &str) -> impl Bundle {
    (
        button,
        Node {
            padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(BUTTON),
        children![text(label, 13.0, WHITE)],
    )
}

/// A steering wheel `size` across: a rim, three spokes and a mark on top, which is straight up
/// with the wheel straight.
fn dial(size: f32) -> impl Bundle {
    let rim = (size * 0.11).round().max(3.0);
    let inner = size - 2.0 * rim;
    let spoke = (rim * 0.8).round().max(2.0);
    let hub = (size * 0.28).round();
    let absolute = |left: f32, top: f32, width: f32, height: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Px(left),
        top: Val::Px(top),
        width: Val::Px(width),
        height: Val::Px(height),
        ..default()
    };
    (
        Dial,
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            border: UiRect::all(Val::Px(rim)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BorderColor::all(WHEEL),
        UiTransform::default(),
        children![
            (
                absolute(0.0, (inner - spoke) / 2.0, inner, spoke),
                BackgroundColor(WHEEL),
            ),
            (
                absolute((inner - spoke) / 2.0, inner / 2.0, spoke, inner / 2.0),
                BackgroundColor(WHEEL),
            ),
            (
                Node {
                    border_radius: BorderRadius::MAX,
                    ..absolute((inner - hub) / 2.0, (inner - hub) / 2.0, hub, hub)
                },
                BackgroundColor(WHEEL),
            ),
            (
                absolute(inner / 2.0 - rim * 0.6, -rim, rim * 1.2, rim),
                BackgroundColor(KEY_COLOR),
            ),
        ],
    )
}

/// A bar `across` wide, filled from its middle as far to either side as the wheel is turned.
fn bar(across: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(across),
            height: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(TRACK),
        children![
            (
                Fill { across, line: None },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(across / 2.0),
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(KEY_COLOR),
            ),
            middle(12.0, -2.0),
        ],
    )
}

/// The wheel's last two seconds, newest on top: each line as far to either side as it was
/// turned then.
fn trace(parent: &mut ChildSpawnerCommands, across: f32, high: f32) {
    let line = high / TRACE_LINES as f32;
    parent
        .spawn((
            Node {
                width: Val::Px(across),
                height: Val::Px(high),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
        ))
        .with_children(|trace| {
            for back in 0..TRACE_LINES {
                let fade = 1.0 - 0.75 * back as f32 / TRACE_LINES as f32;
                trace.spawn((
                    Fill {
                        across,
                        line: Some(back),
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(back as f32 * line),
                        left: Val::Px(across / 2.0),
                        width: Val::Px(0.0),
                        height: Val::Px(line.ceil()),
                        ..default()
                    },
                    BackgroundColor(KEY_COLOR.with_alpha(fade)),
                ));
            }
            trace.spawn(middle(high, 0.0));
        });
}

/// The line down the middle of a bar or the trace: the wheel straight.
fn middle(high: f32, top: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-1.0)),
            top: Val::Px(top),
            width: Val::Px(2.0),
            height: Val::Px(high),
            ..default()
        },
        BackgroundColor(DIM),
    )
}

/// A line of the panel: its label, then whatever `contents` puts after it.
fn row(
    parent: &mut ChildSpawnerCommands,
    index: usize,
    selected: bool,
    label: &str,
    contents: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            PanelRow(index),
            Node {
                height: Val::Px(26.0),
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(if selected { SELECTED } else { Color::NONE }),
        ))
        .with_children(|line| {
            line.spawn((
                text(label, 14.0, WHITE),
                TextLayout::no_wrap(),
                Node {
                    width: Val::Px(150.0),
                    ..default()
                },
            ));
            contents(line);
        });
}

/// What's in a box after a label: a key, or On or Off; lit while it waits for a key.
fn key_box(label: &str, lit: bool) -> impl Bundle {
    (
        Node {
            min_width: Val::Px(64.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BorderColor::all(if lit { KEY_COLOR } else { TRACK }),
        BackgroundColor(if lit {
            KEY_COLOR.with_alpha(0.2)
        } else {
            Color::NONE
        }),
        children![(text(label, 14.0, KEY_COLOR), TextLayout::no_wrap())],
    )
}

/// Builds the panel again whenever what it shows changes (but the wheel, which `show_wheels`
/// turns every frame).
fn show_panel(
    mut commands: Commands,
    controls: Res<Controls>,
    game: Option<Res<PlayingGame>>,
    buttons: Res<GameButtons>,
    own: Res<OwnName>,
    touch: Res<Touch>,
    panel: Single<(Entity, &mut Visibility), With<Panel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    let view = controls
        .open
        .then(|| View::of(&controls, game.as_deref(), &buttons.0, touch.is_on()))
        .flatten();
    let Some(view) = view else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let shown = visibility.set_if_neq(Visibility::Inherited);
    let game_changed = game.as_ref().is_some_and(|game| game.is_changed());
    if !shown
        && !controls.is_changed()
        && !game_changed
        && !buttons.is_changed()
        && !own.is_changed()
    {
        return;
    }
    let every = controls.scope == Scope::Every;
    commands
        .entity(panel)
        .despawn_children()
        .with_children(|screen| {
            screen
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        padding: UiRect::all(Val::Px(16.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        max_width: Val::Percent(96.0),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    // Opaque: the UI blends in linear light, where even a nearly opaque
                    // black lets a bright game show through.
                    BackgroundColor(Color::srgb(0.03, 0.03, 0.05)),
                    BorderColor::all(TRACK),
                ))
                .with_children(|panel| {
                    panel
                        .spawn(Node {
                            column_gap: Val::Px(10.0),
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|title| {
                            title.spawn(text("Controls", 18.0, WHITE));
                            title.spawn(text(&view.title, 14.0, KEY_COLOR));
                        });
                    let whose = if every {
                        "For every game; the gear at a cabinet sets that game's own.".to_string()
                    } else {
                        format!(
                            "For {} only; the gear in the bar sets every game's.",
                            view.title
                        )
                    };
                    // The player's name, everywhere the same.
                    if let Some(index) = view.rows.iter().position(|row| *row == Row::Name) {
                        let selected = index == controls.selected;
                        let name = if own.0.is_empty() {
                            "..."
                        } else {
                            own.0.as_str()
                        };
                        let change = if touch.is_on() {
                            "tap to change"
                        } else {
                            "click to change"
                        };
                        row(panel, index, selected, "Your name", |line| {
                            line.spawn(key_box(name, false));
                            line.spawn(text(change, 12.0, DIM));
                        });
                    }
                    panel.spawn(text(&whose, 12.0, DIM));
                    panel
                        .spawn(Node {
                            column_gap: Val::Px(28.0),
                            row_gap: Val::Px(10.0),
                            align_items: AlignItems::Start,
                            flex_wrap: FlexWrap::Wrap,
                            margin: UiRect::top(Val::Px(4.0)),
                            ..default()
                        })
                        .with_children(|columns| {
                            let column = Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(2.0),
                                ..default()
                            };
                            if view.rows.iter().any(|row| matches!(row, Row::Key(_))) {
                                columns.spawn(column.clone()).with_children(|column| {
                                    column.spawn(text("Buttons", 13.0, HEADING));
                                    for (index, &row_is) in view.rows.iter().enumerate() {
                                        let Row::Key(id) = row_is else {
                                            continue;
                                        };
                                        let selected = index == controls.selected;
                                        let waiting = selected && controls.binding;
                                        let key = view
                                            .keys
                                            .iter()
                                            .find(|(_, on)| *on == id)
                                            .and_then(|(key, _)| key_name(*key))
                                            .unwrap_or("?");
                                        let shown = if waiting { "press a key" } else { key };
                                        row(column, index, selected, view.name(id), |line| {
                                            line.spawn(key_box(shown, waiting));
                                        });
                                    }
                                });
                            }
                            if let Some(wheel) = view.wheel {
                                // Until set, every game's wheel is each game's own.
                                let unset = every && controls.kept.every.wheel.is_none();
                                let first = controls.first_wheel.as_ref().map(|(title, _)| title);
                                let note = first.filter(|_| unset).map(|title| {
                                    format!(
                                        "Until you change these, each driving game turns its \
                                         own way; they start as {title}'s."
                                    )
                                });
                                columns.spawn(column).with_children(|column| {
                                    let heading = if every {
                                        "Wheel, every driving game"
                                    } else {
                                        "Wheel"
                                    };
                                    column.spawn(text(heading, 13.0, HEADING));
                                    wheel_rows(column, &controls, &view.rows, &wheel, note);
                                });
                            }
                        });
                    if !controls.note.is_empty() {
                        panel.spawn(text(&controls.note, 13.0, KEY_COLOR));
                    }
                    let resets = if every {
                        "R resets"
                    } else {
                        "R resets the game's"
                    };
                    let help = match (touch.is_on(), view.wheel.is_some()) {
                        (true, _) => "- and + change it; steer with the pad to try it.".to_string(),
                        (false, true) => format!(
                            "Up/Down pick  Enter: another key  -/+ change (Shift: x5)\n\
                             Steer to try the wheel  {resets}  Tab or Esc closes"
                        ),
                        (false, false) => {
                            format!("Up/Down pick  Enter: another key\n{resets}  Tab or Esc closes")
                        }
                    };
                    panel.spawn((
                        text(&help, 12.0, DIM),
                        Node {
                            margin: UiRect::top(Val::Px(4.0)),
                            ..default()
                        },
                    ));
                    panel
                        .spawn(Node {
                            justify_content: JustifyContent::End,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|buttons| {
                            buttons.spawn(button(PanelButton::Reset, "Reset"));
                            buttons.spawn(button(PanelButton::Close, "Close"));
                        });
                });
        });
}

/// The wheel's part of the panel: the wheel as it is now, its numbers, the corner wheel, and
/// what it comes to, after `note` if any.
fn wheel_rows(
    column: &mut ChildSpawnerCommands,
    controls: &Controls,
    rows: &[Row],
    wheel: &Wheel,
    note: Option<String>,
) {
    column
        .spawn(Node {
            column_gap: Val::Px(14.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(6.0), Val::Px(6.0)),
            ..default()
        })
        .with_children(|live| {
            live.spawn(dial(72.0));
            live.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(5.0),
                ..default()
            })
            .with_children(|right| {
                right.spawn((Reading, text("Straight", 14.0, WHITE)));
                right.spawn(bar(160.0));
                trace(right, 160.0, 48.0);
                right.spawn(text("the last 2 s, newest on top", 11.0, DIM));
            });
        });
    for (index, &row_is) in rows.iter().enumerate() {
        let selected = index == controls.selected;
        if row_is == Row::Corner {
            let on = if controls.corner_wheel() { "On" } else { "Off" };
            row(column, index, selected, "Corner wheel", |line| {
                line.spawn(key_box(on, false));
            });
            continue;
        }
        let mut numbers = *wheel;
        let (Some(knob), Some(value)) = (row_is.knob(), row_is.of(&mut numbers)) else {
            continue;
        };
        let value = format!("{:.*}{}", knob.decimals, *value, knob.unit);
        row(column, index, selected, knob.label, |line| {
            let step = |way: f64| {
                (
                    PanelButton::Step(index, way),
                    Node {
                        width: Val::Px(26.0),
                        height: Val::Px(22.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                    children![text(if way < 0.0 { "-" } else { "+" }, 14.0, WHITE)],
                )
            };
            line.spawn(step(-1.0));
            line.spawn((
                text(&value, 14.0, KEY_COLOR),
                TextLayout {
                    justify: Justify::Center,
                    linebreak: LineBreak::NoWrap,
                },
                Node {
                    width: Val::Px(64.0),
                    ..default()
                },
            ));
            line.spawn(step(1.0));
        });
    }
    let about = match note {
        Some(note) => format!("{note}\n{}", foretell(wheel)),
        None => foretell(wheel),
    };
    column.spawn((
        text(&about, 12.0, DIM),
        Node {
            max_width: Val::Px(320.0),
            margin: UiRect::top(Val::Px(4.0)),
            ..default()
        },
    ));
}

/// Turns every drawn wheel, fills every bar and the trace, and says how far it's turned.
fn show_wheels(
    time: Res<Time>,
    turned: Res<Turned>,
    mut dials: Query<&mut UiTransform, With<Dial>>,
    mut fills: Query<(&Fill, &mut Node)>,
    mut readings: Query<&mut Text, With<Reading>>,
) {
    let rotation = Rot2::degrees(turned.now * LOCK_DEGREES);
    for mut transform in &mut dials {
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
    let now = time.elapsed_secs();
    for (fill, mut node) in &mut fills {
        let at = match fill.line {
            None => turned.now,
            Some(back) => turned.was(now, back as f32 * TRACE_FOR / TRACE_LINES as f32),
        };
        let half = fill.across / 2.0;
        let (left, width) = (half + at.min(0.0) * half, at.abs() * half);
        let (left, width) = (Val::Px(left), Val::Px(width));
        if node.left != left || node.width != width {
            node.left = left;
            node.width = width;
        }
    }
    let percent = (turned.now.abs() * 100.0).round();
    let reading = if percent == 0.0 {
        "Straight".to_string()
    } else if turned.now < 0.0 {
        format!("Left {percent}%")
    } else {
        format!("Right {percent}%")
    };
    for mut text in &mut readings {
        if text.0 != reading {
            text.0 = reading.clone();
        }
    }
}

/// The small wheel in the corner, at a driving game the player steers: bottom right, or on a
/// phone held upright at the top, under Leave.
fn spawn_corner_wheel(
    mut commands: Commands,
    game: Option<Res<PlayingGame>>,
    touch: Res<Touch>,
    mut turned: ResMut<Turned>,
) {
    *turned = Turned::default();
    if !game.is_some_and(|game| game.steers()) {
        return;
    }
    let (top, bottom) = if touch.is_on() {
        (Val::Px(TOP_BAR + 8.0), Val::Auto)
    } else {
        (Val::Auto, Val::Px(12.0))
    };
    commands.spawn((
        CornerWheel,
        DespawnOnExit(Mode::Playing),
        Node {
            position_type: PositionType::Absolute,
            top,
            bottom,
            right: Val::Px(12.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        // Over the game.
        GlobalZIndex(2),
        children![dial(44.0), bar(56.0)],
    ));
}

fn show_corner_wheel(
    controls: Res<Controls>,
    mut corner: Query<&mut Visibility, With<CornerWheel>>,
) {
    for mut visibility in &mut corner {
        visibility.set_if_neq(if controls.corner_wheel() && !controls.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT_RUN: Wheel = Wheel {
        lock: 0.3,
        back: 0.3,
        curve: 1.0,
        span: (10600, 23500),
    };
    const CRUISN: Wheel = Wheel {
        lock: 0.6,
        back: 0.1,
        curve: 2.0,
        span: (5200, 28900),
    };

    fn game(rom: &str) -> Scope {
        Scope::Game(rom.into())
    }

    fn key_of(keys: &Layout, id: u16) -> KeyCode {
        keys.iter().find(|(_, on)| *on == id).unwrap().0
    }

    #[test]
    fn a_key_put_on_a_button_swaps_with_the_one_that_had_it() {
        let mut controls = Controls::default();
        // Left steers; A was Y's.
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyA);
        let keys = controls.keys("outrun");
        assert!(keys.contains(&(KeyCode::KeyA, LEFT)));
        assert!(keys.contains(&(KeyCode::ArrowLeft, 1)));
        // Other games keep theirs.
        assert_eq!(controls.keys("mk2"), KEYS);
        // Back where they were, nothing is kept.
        controls.bind(&game("outrun"), LEFT, KeyCode::ArrowLeft);
        assert_eq!(controls.keys("outrun"), KEYS);
        assert_eq!(controls.kept, Kept::default());
        // A key no button had just moves the button.
        controls.bind(&game("outrun"), 4, KeyCode::Space);
        let keys = controls.keys("outrun");
        assert!(keys.contains(&(KeyCode::Space, 4)));
        assert!(!keys.iter().any(|(key, _)| *key == KeyCode::ArrowUp));
    }

    #[test]
    fn every_games_keys_are_under_a_games_own() {
        let mut controls = Controls::default();
        controls.bind(&Scope::Every, LEFT, KeyCode::KeyK);
        // Every game has K steer left, Out Run too until it says otherwise.
        assert_eq!(key_of(&controls.keys("mk2"), LEFT), KeyCode::KeyK);
        assert_eq!(key_of(&controls.keys("outrun"), LEFT), KeyCode::KeyK);
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyJ);
        assert_eq!(key_of(&controls.keys("outrun"), LEFT), KeyCode::KeyJ);
        assert_eq!(key_of(&controls.keys("mk2"), LEFT), KeyCode::KeyK);
        // Out Run keeps only what differs from every game's.
        assert_eq!(controls.kept.games["outrun"].keys.len(), 1);
        // Put back as every game has it, Out Run has nothing of its own.
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyK);
        assert!(!controls.kept.games.contains_key("outrun"));
        // A game's key that every game's later takes for another button: still one button a
        // key, the game's own on it, the other on the key the game's had under it.
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyJ);
        controls.bind(&Scope::Every, 3, KeyCode::KeyJ);
        let keys = controls.keys("outrun");
        assert_eq!(key_of(&keys, LEFT), KeyCode::KeyJ);
        assert_eq!(key_of(&keys, 3), KeyCode::KeyK);
        let all: std::collections::HashSet<_> = keys.iter().map(|(key, _)| *key).collect();
        assert_eq!(all.len(), 12);
    }

    #[test]
    fn a_games_reset_leaves_every_games() {
        let mut controls = Controls::default();
        controls.bind(&Scope::Every, LEFT, KeyCode::KeyK);
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyJ);
        controls.reset(&game("outrun"));
        assert_eq!(key_of(&controls.keys("outrun"), LEFT), KeyCode::KeyK);
        controls.reset(&Scope::Every);
        assert_eq!(controls.keys("outrun"), KEYS);
        assert_eq!(controls.kept, Kept::default());
    }

    #[test]
    fn every_games_wheel_is_under_a_games_own() {
        let mut controls = Controls {
            first_wheel: Some(("Out Run".into(), OUT_RUN)),
            ..default()
        };
        // Unset, each game turns its own way, and the bar shows the first's.
        assert_eq!(controls.wheel("outrun", Some(OUT_RUN)), Some(OUT_RUN));
        assert_eq!(controls.wheel("crusnusa41", Some(CRUISN)), Some(CRUISN));
        assert_eq!(controls.wheel("mk2", None), None);
        assert_eq!(controls.shown_every_wheel(), Some(OUT_RUN));
        // Set for every game, every driving game turns so, on its own span.
        let slower = nudge(OUT_RUN, Row::Lock, 2.0);
        assert_eq!(slower.lock, 0.4);
        controls.set_wheel(&Scope::Every, slower, None);
        let cruisn = controls.wheel("crusnusa41", Some(CRUISN)).unwrap();
        assert_eq!((cruisn.lock, cruisn.back, cruisn.curve), (0.4, 0.3, 1.0));
        assert_eq!(cruisn.span, CRUISN.span);
        // A game's own over it, kept only while it differs.
        let softer = nudge(cruisn, Row::Curve, 10.0);
        controls.set_game_wheel("crusnusa41", softer, Some(CRUISN));
        assert_eq!(controls.wheel("crusnusa41", Some(CRUISN)), Some(softer));
        assert_eq!(controls.wheel("outrun", Some(OUT_RUN)), Some(slower));
        controls.set_game_wheel("crusnusa41", cruisn, Some(CRUISN));
        assert!(!controls.kept.games.contains_key("crusnusa41"));
    }

    #[test]
    fn nudges_stay_on_steps_and_in_range() {
        let mut wheel = OUT_RUN;
        for _ in 0..3 {
            wheel = nudge(wheel, Row::Lock, 1.0);
        }
        assert_eq!(wheel.lock, 0.45);
        assert_eq!(nudge(wheel, Row::Lock, -100.0).lock, 0.05);
        assert_eq!(nudge(wheel, Row::Back, -100.0).back, 0.0);
        assert_eq!(nudge(wheel, Row::Curve, 1.0).curve, 1.1);
        assert_eq!(nudge(wheel, Row::Corner, 1.0), wheel);
    }

    #[test]
    fn what_a_tap_and_a_hold_come_to() {
        assert_eq!(
            foretell(&OUT_RUN),
            "A tap (0.1 s) turns it 33%. Held: half lock in 0.15 s, full lock in 0.30 s. Let \
             go: straight in 0.30 s."
        );
        assert!(
            foretell(&CRUISN).starts_with("A tap (0.1 s) turns it 3%. Held: half lock in 0.42 s")
        );
    }

    #[test]
    fn rows_list_what_the_game_names() {
        let named = vec![(3, "Start".to_string()), (6, "Steer left".to_string())];
        assert_eq!(
            rows(&named, false, false),
            [Row::Name, Row::Key(3), Row::Key(LEFT)]
        );
        assert_eq!(rows(&[], false, false).len(), 13);
        assert_eq!(
            rows(&named, true, true),
            [Row::Name, Row::Lock, Row::Back, Row::Curve, Row::Corner]
        );
    }

    #[test]
    fn what_the_browser_keeps_reads_back() {
        let mut controls = Controls::default();
        controls.bind(&game("outrun"), LEFT, KeyCode::KeyJ);
        controls.set_game_wheel("outrun", nudge(OUT_RUN, Row::Curve, 5.0), Some(OUT_RUN));
        controls.kept.no_corner_wheel = true;
        let json = serde_json::to_string(&controls.kept).unwrap();
        assert_eq!(
            json,
            r#"{"games":{"outrun":{"keys":{"6":"J"},"wheel":{"lock":0.3,"back":0.3,"curve":1.5}}},"no_corner_wheel":true}"#
        );
        assert_eq!(serde_json::from_str::<Kept>(&json).unwrap(), controls.kept);
        controls.bind(&Scope::Every, 3, KeyCode::Space);
        let json = serde_json::to_string(&controls.kept).unwrap();
        assert!(json.starts_with(r#"{"every":{"keys":{"3":"Space"}},"games""#));
        assert_eq!(serde_json::from_str::<Kept>(&json).unwrap(), controls.kept);
        // Anything it doesn't know is left out.
        let odd: Kept = serde_json::from_str(r#"{"games":{"x":{"keys":{"6":"Hyper"}}}}"#).unwrap();
        let controls = Controls {
            kept: odd,
            ..default()
        };
        assert_eq!(controls.keys("x"), KEYS);
    }
}
