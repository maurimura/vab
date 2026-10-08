//! Cabinets with a game (assigned in the editor): next to one, a hint shows its title and how
//! many play and watch it. E sits you at it (online with whoever sits at the other seats), and F
//! watches the game being played there, as does E once every seat is taken. On a touch screen
//! the Play and Watch buttons show instead (nearby.rs).

use std::cell::RefCell;
use std::collections::HashMap;

use bevy::prelude::*;
use wasm_bindgen::prelude::*;
use world::{Game, Map, cell_to_world, games_from_ron};

use crate::Mode;
use crate::chat::chat_closed;
use crate::emulator;
use crate::help::help_closed;
use crate::nearby::{self, Kind, Nearby, Use, hint_label, place_hint};
use crate::settings::settings_closed;
use crate::touch::{Touch, TouchButton};

/// The games cabinets can run, built into the client like the map.
const GAMES: &str = include_str!("../../assets/games.ron");
/// Where the hint sits: a little above a cabinet's top, in world pixels from its cell.
const HINT_HEIGHT: f32 = 44.0;

thread_local! {
    /// How many sit at each cabinet ("x,y"), from the room.
    static SEATED: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
    /// How many watch each cabinet's game.
    static WATCHING: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
}

/// Called by index.html when the players at a cabinet change.
#[wasm_bindgen]
pub fn cabinet_seats(cabinet: String, count: u32) {
    SEATED.with_borrow_mut(|seated| seated.insert(cabinet, count));
}

/// Called by index.html when the people watching a cabinet's game change.
#[wasm_bindgen]
pub fn cabinet_watchers(cabinet: String, count: u32) {
    WATCHING.with_borrow_mut(|watching| watching.insert(cabinet, count));
}

/// How many sit at the cabinet in `cell`, and how many watch its game.
fn people_at(cell: IVec2) -> (u32, u32) {
    let cabinet = emulator::cabinet_id(cell);
    let count = |counts: &HashMap<String, u32>| counts.get(&cabinet).copied().unwrap_or(0);
    (SEATED.with_borrow(count), WATCHING.with_borrow(count))
}

pub struct CabinetsPlugin;

impl Plugin for CabinetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hint)
            .add_systems(
                Update,
                (
                    show_hint,
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
    nearby::offer(&mut buttons, seated < game.players, seated > 0);
    let label = nearby::hint_text(&game.title, seated, game.players, watching, touch.is_on());
    let above = cell_to_world(cell.x, cell.y) + Vec2::Y * HINT_HEIGHT;
    place_hint(
        &label,
        above,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

fn play(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    cabinets: Res<Cabinets>,
    nearby: Res<Nearby>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let sit = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Play);
    let watch = keys.just_pressed(KeyCode::KeyF) || touch.tapped(TouchButton::Watch);
    if !sit && !watch {
        return;
    }
    let Some((cell, game)) = cabinets.nearby(&nearby) else {
        return;
    };
    let (seated, _) = people_at(*cell);
    let Some(what) = nearby::chosen(sit, watch, seated, game.players) else {
        return;
    };
    let watching = what == Use::Watch;
    if watching {
        emulator::watch(*cell, game);
    } else {
        emulator::play(*cell, game);
    }
    let title = game.title.clone();
    commands.insert_resource(emulator::PlayingGame {
        title,
        watching,
        gun: game.gun,
    });
    mode.set(Mode::Playing);
}

fn hide_hint(mut hint: Single<&mut Visibility, With<Hint>>) {
    **hint = Visibility::Hidden;
}
