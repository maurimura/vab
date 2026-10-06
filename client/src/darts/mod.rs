//! Dartboards (objects/dartboard): next to one, a hint says so and how many play and watch at
//! it, and E (or the Play button on a touch screen) steps up to it to play (game.rs), against
//! whoever is at the other seat (online.rs). F (or Watch) watches the game being played there,
//! as does E once both seats are taken.

mod game;
mod online;

use bevy::prelude::*;
use world::{Map, Placed};

use crate::Mode;
use crate::chat::chat_closed;
use crate::help::help_closed;
use crate::nearby::{self, Kind, Nearby, Use, hint_label, place_hint};
use crate::seats;
use crate::settings::settings_closed;
use crate::touch::{Touch, TouchButton};

const TILE: &str = "objects/dartboard";
/// Two play at a board.
const SEATS: u32 = 2;
/// Where the hint sits: a little above the board, in world pixels from its cell.
const HINT_HEIGHT: f32 = 44.0;

pub struct DartsPlugin;

impl Plugin for DartsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(game::GamePlugin)
            .add_systems(Startup, spawn_hint)
            .add_systems(
                Update,
                (
                    show_hint,
                    step_up
                        .run_if(chat_closed)
                        .run_if(help_closed)
                        .run_if(settings_closed),
                )
                    .run_if(in_state(Mode::Walking)),
            )
            .add_systems(OnExit(Mode::Walking), hide_hint);
    }
}

/// The dartboards in the bar.
#[derive(Resource)]
pub struct Dartboards(Vec<Placed>);

impl Dartboards {
    pub fn placed(&self) -> &[Placed] {
        &self.0
    }

    pub fn from_map(map: &Map) -> Self {
        Self(
            map.objects
                .iter()
                .filter(|placed| placed.tile == TILE)
                .cloned()
                .collect(),
        )
    }
}

#[derive(Component)]
struct Hint;

fn spawn_hint(mut commands: Commands) {
    commands.spawn((Hint, hint_label()));
}

fn show_hint(
    boards: Option<Res<Dartboards>>,
    nearby: Res<Nearby>,
    touch: Res<Touch>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let Some(board) = boards
        .as_ref()
        .and_then(|boards| nearby.of(Kind::Darts, &boards.0))
    else {
        *visibility = Visibility::Hidden;
        return;
    };
    let id = online::table_id(board.cell());
    let (seated, watching) = (seats::seated(&id) as u32, seats::watching(&id) as u32);
    nearby::offer(&mut buttons, seated < SEATS, seated > 0);
    let label = nearby::hint_text("Darts", seated, SEATS, watching, touch.is_on());
    place_hint(
        &label,
        board.center() + Vec2::Y * HINT_HEIGHT,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

fn step_up(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    boards: Option<Res<Dartboards>>,
    nearby: Res<Nearby>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let play = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Play);
    let watch = keys.just_pressed(KeyCode::KeyF) || touch.tapped(TouchButton::Watch);
    let Some(board) = boards
        .as_ref()
        .and_then(|boards| nearby.of(Kind::Darts, &boards.0))
    else {
        return;
    };
    let id = online::table_id(board.cell());
    let Some(what) = nearby::chosen(play, watch, seats::seated(&id) as u32, SEATS) else {
        return;
    };
    commands.insert_resource(game::AtBoard {
        id,
        watching: what == Use::Watch,
    });
    mode.set(Mode::Darts);
}

fn hide_hint(mut hint: Single<&mut Visibility, With<Hint>>) {
    **hint = Visibility::Hidden;
}
