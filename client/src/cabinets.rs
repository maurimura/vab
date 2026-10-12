//! Cabinets with a game (assigned in the editor): next to one, a hint shows its title and how
//! many play and watch it. E sits you at it (online with whoever sits at the other seats), and F
//! watches the game being played there, as does E once every seat is taken. On a touch screen
//! the Play and Watch buttons show instead (nearby.rs).
//!
//! At a game whose seats are characters (The Simpsons: Marge, Homer, Bart and Lisa, each on
//! their own controls), the hint also says who plays each, and a free one's number key sits
//! you as them (1 for the first); E takes the first free. On a touch screen each free one has
//! a button instead of Play.

use std::cell::RefCell;
use std::collections::HashMap;

use bevy::prelude::*;
use wasm_bindgen::prelude::*;
use world::{Game, Map, cell_to_world, games_from_ron};

use crate::Mode;
use crate::chat::chat_closed;
use crate::controls::Controls;
use crate::emulator;
use crate::help::help_closed;
use crate::nearby::{self, Kind, Nearby, Use, hint_label, place_hint};
use crate::settings::settings_closed;
use crate::touch::{Touch, TouchButton};
use crate::voice::SEAT_KEYS;

/// The games cabinets can run, built into the client like the map.
pub const GAMES: &str = include_str!("../../assets/games.ron");
/// Where the hint sits: a little above a cabinet's top, in world pixels from its cell.
pub const HINT_HEIGHT: f32 = 44.0;

thread_local! {
    /// Who sits at each cabinet ("x,y"), from the room: their names by seat, None where free.
    static SEATED: RefCell<HashMap<String, Vec<Option<String>>>> = RefCell::new(HashMap::new());
    /// How many watch each cabinet's game.
    static WATCHING: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

/// Called by index.html when the players at a cabinet change, or their names: whether each
/// seat is `taken` (1) and the `names` of who sits there.
#[wasm_bindgen]
pub fn cabinet_seats(cabinet: String, taken: Vec<u8>, names: Vec<String>) {
    let seats = taken
        .iter()
        .enumerate()
        .map(|(seat, taken)| (*taken != 0).then(|| names.get(seat).cloned().unwrap_or_default()))
        .collect();
    SEATED.with_borrow_mut(|seated| seated.insert(cabinet, seats));
}

/// Called by index.html when the people watching a cabinet's game change.
#[wasm_bindgen]
pub fn cabinet_watchers(cabinet: String, count: u32) {
    WATCHING.with_borrow_mut(|watching| watching.insert(cabinet, count));
}

/// How many sit at the cabinet in `cell`, and how many watch its game.
pub fn people_at(cell: IVec2) -> (u32, u32) {
    let cabinet = emulator::cabinet_id(cell);
    let seated = seats_at(cell).iter().flatten().count() as u32;
    let watching = WATCHING.with_borrow(|counts| counts.get(&cabinet).copied().unwrap_or(0));
    (seated, watching)
}

/// Who sits at the cabinet in `cell`: their names by seat, None where free (and past the last
/// seat taken).
fn seats_at(cell: IVec2) -> Vec<Option<String>> {
    let cabinet = emulator::cabinet_id(cell);
    SEATED.with_borrow(|seated| seated.get(&cabinet).cloned().unwrap_or_default())
}

/// Which of a game's `characters` are free, as `seats` are taken.
fn free(characters: &[String], seats: &[Option<String>]) -> Vec<bool> {
    (0..characters.len())
        .map(|seat| seats.get(seat).is_none_or(Option::is_none))
        .collect()
}

/// The hint's line at a game whose seats are characters: who plays each, and before a free one
/// the number key that picks them. A touch screen has their buttons, and says only who plays.
fn characters_line(characters: &[String], seats: &[Option<String>], touch: bool) -> String {
    let each = characters
        .iter()
        .enumerate()
        .filter_map(
            |(seat, character)| match seats.get(seat).cloned().flatten() {
                Some(name) if name.is_empty() => Some(format!("{character}: taken")),
                Some(name) => Some(format!("{character}: {name}")),
                None if touch => None,
                None => Some(format!("{}  {character}", seat + 1)),
            },
        );
    each.collect::<Vec<_>>().join("    ")
}

pub struct CabinetsPlugin;

impl Plugin for CabinetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hint)
            .add_systems(
                Update,
                (
                    show_hint,
                    name_characters,
                    play.run_if(chat_closed)
                        .run_if(help_closed)
                        .run_if(settings_closed),
                )
                    .run_if(in_state(Mode::Walking)),
            )
            .add_systems(OnEnter(Mode::Playing), hide_hint);
    }
}

/// Cabinets that have a game, with their cells.
#[derive(Resource)]
pub struct Cabinets(Vec<(IVec2, Game)>);

impl Cabinets {
    /// Cabinets whose game isn't in assets/games.ron are left out.
    pub fn from_map(map: &Map) -> Self {
        let games = games_from_ron(GAMES).expect("assets/games.ron is a valid game list");
        Self(
            map.objects
                .iter()
                .filter_map(|placed| {
                    let rom = placed.game.as_ref()?;
                    let game = games.iter().find(|game| &game.rom == rom)?;
                    Some((IVec2::new(placed.x, placed.y), game.clone()))
                })
                .collect(),
        )
    }

    pub fn cells(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.0.iter().map(|(cell, _)| *cell)
    }

    /// Every cabinet with a game, and the game.
    pub fn all(&self) -> &[(IVec2, Game)] {
        &self.0
    }

    /// The cabinet nearest the player, if it's nearer than anything else (nearby.rs).
    fn nearby(&self, nearby: &Nearby) -> Option<&(IVec2, Game)> {
        let cell = nearby.cell(Kind::Cabinet)?;
        self.0.iter().find(|(cabinet, _)| *cabinet == cell)
    }
}

#[derive(Component)]
struct Hint;

fn spawn_hint(mut commands: Commands) {
    commands.spawn((Hint, hint_label()));
}

/// Shows the hint above the cabinet next to the player, centered on it, and on a touch screen
/// the buttons for what can be done there.
fn show_hint(
    cabinets: Res<Cabinets>,
    nearby: Res<Nearby>,
    touch: Res<Touch>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let Some((cell, game)) = cabinets.nearby(&nearby) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let (seated, watching) = people_at(*cell);
    let seats = seats_at(*cell);
    let characters = &game.characters;
    let play = seated < game.players && characters.is_empty();
    let free = free(characters, &seats);
    nearby::offer_characters(&mut buttons, play, seated > 0, &free);
    let mut label = nearby::hint_text(&game.title, seated, game.players, watching, touch.is_on());
    let line = characters_line(characters, &seats, touch.is_on());
    if !line.is_empty() {
        label = format!("{label}\n{line}");
    }
    // For browser tests (testing.rs): the hint as shown.
    #[cfg(feature = "test-hooks")]
    crate::testing::report("hint", serde_json::Value::String(label.clone()));
    let above = cell_to_world(cell.x, cell.y) + Vec2::Y * HINT_HEIGHT;
    place_hint(
        &label,
        above,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

/// On a touch screen the characters' buttons say who each is, for the game nearby.
fn name_characters(
    cabinets: Res<Cabinets>,
    nearby: Res<Nearby>,
    buttons: Query<(&TouchButton, &Children)>,
    mut labels: Query<&mut Text, Without<Hint>>,
) {
    let Some((_, game)) = cabinets.nearby(&nearby) else {
        return;
    };
    for (button, children) in &buttons {
        let TouchButton::Seat(seat) = button else {
            continue;
        };
        let Some(character) = game.characters.get(usize::from(*seat)) else {
            continue;
        };
        let mut texts = labels.iter_many_mut(children);
        while let Some(mut text) = texts.fetch_next() {
            if text.0 != *character {
                text.0.clone_from(character);
            }
        }
    }
}

fn play(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    cabinets: Res<Cabinets>,
    nearby: Res<Nearby>,
    controls: Res<Controls>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let Some((cell, game)) = cabinets.nearby(&nearby) else {
        return;
    };
    let picked = picked(&keys, &touch, &free(&game.characters, &seats_at(*cell)));
    let sit =
        keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Play) || picked.is_some();
    let watch = keys.just_pressed(KeyCode::KeyF) || touch.tapped(TouchButton::Watch);
    if !sit && !watch {
        return;
    }
    let (seated, _) = people_at(*cell);
    let Some(what) = nearby::chosen(sit, watch, seated, game.players) else {
        return;
    };
    let watching = what == Use::Watch;
    // A driving game's wheel turns as the player has it (controls.rs).
    let wheel = controls.wheel(&game.rom, game.wheel);
    if watching {
        emulator::watch(*cell, game);
    } else {
        emulator::play(*cell, game, wheel, picked);
    }
    commands.insert_resource(emulator::PlayingGame {
        title: game.title.clone(),
        rom: game.rom.clone(),
        watching,
        gun: game.gun,
        wheel,
        usual_wheel: game.wheel,
    });
    mode.set(Mode::Playing);
}

/// The free character picked, by seat: their number key (Shift with one mutes someone instead:
/// voice.rs), or on a touch screen their button.
fn picked(keys: &ButtonInput<KeyCode>, touch: &Touch, free: &[bool]) -> Option<u32> {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let key = |seat: usize| {
        !shift
            && SEAT_KEYS
                .get(seat)
                .is_some_and(|key| keys.just_pressed(*key))
    };
    let seat = (0..free.len())
        .find(|&seat| free[seat] && (key(seat) || touch.tapped(TouchButton::Seat(seat as u8))))?;
    Some(seat as u32)
}

fn hide_hint(mut hint: Single<&mut Visibility, With<Hint>>) {
    **hint = Visibility::Hidden;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simpsons() -> Vec<String> {
        ["Marge", "Homer", "Bart", "Lisa"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn the_hint_says_who_plays_each_character_and_the_keys_for_the_others() {
        // The room lists seats up to the last one taken.
        let seats = [None, None, Some("Ana".to_string())];
        assert_eq!(
            characters_line(&simpsons(), &seats, false),
            "1  Marge    2  Homer    Bart: Ana    4  Lisa"
        );
        // Fingers have the free ones' buttons.
        assert_eq!(characters_line(&simpsons(), &seats, true), "Bart: Ana");
        assert_eq!(characters_line(&simpsons(), &[], true), "");
        assert_eq!(free(&simpsons(), &seats), [true, true, false, true]);
        // Games without characters have no such line, and nothing to pick.
        assert_eq!(characters_line(&[], &seats, false), "");
        assert!(free(&[], &seats).is_empty());
    }

    #[test]
    fn a_number_key_picks_a_free_character() {
        let free = [true, true, false, true];
        let touch = Touch::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Digit4);
        assert_eq!(picked(&keys, &touch, &free), Some(3));
        // Bart is taken: his key does nothing.
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Digit3);
        assert_eq!(picked(&keys, &touch, &free), None);
        // Shift with a number mutes someone instead.
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ShiftLeft);
        keys.press(KeyCode::Digit1);
        assert_eq!(picked(&keys, &touch, &free), None);
        // No characters, nothing to pick.
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Digit1);
        assert_eq!(picked(&keys, &touch, &[]), None);
    }
}
