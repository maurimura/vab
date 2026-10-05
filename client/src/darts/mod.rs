//! Dartboards (objects/dartboard): next to one, a hint says so and how many play at it, and E
//! (or the Play button on a touch screen) steps up to it to play (game.rs), against whoever is at
//! the other seat (online.rs).

mod game;
mod online;

use bevy::prelude::*;
use world::{Map, Placed};

use crate::Mode;
use crate::cabinets::{hint_label, place_hint};
use crate::chat::chat_closed;
use crate::help::help_closed;
use crate::nearby::{Kind, Nearby};
use crate::seats;
use crate::settings::settings_closed;
use crate::touch::{self, Touch, TouchButton};

const TILE: &str = "objects/dartboard";
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

fn spawn_hint(mut commands: Commands, touch: Res<Touch>) {
    if touch.is_on() {
        // Where the other tables' Play buttons go; one is offered at a time.
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(24.0),
                ..default()
            },
            children![(
                touch::button(TouchButton::Darts, "Play", 96.0, 48.0),
                Visibility::Hidden,
            )],
        ));
    }
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
    let near = boards
        .as_ref()
        .and_then(|boards| nearby.of(Kind::Darts, &boards.0));
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Darts {
            shown.set_if_neq(if near.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    let Some(board) = near else {
        *visibility = Visibility::Hidden;
        return;
    };
    let seated = seats::seated(&online::table_id(board.cell()));
    // The button says what to press.
    let label = match (seated, touch.is_on()) {
        (0, true) => "Darts",
        (0, false) => "E  Darts",
        (1, true) => "Darts - 1 of 2 playing",
        (1, false) => "E  Darts - 1 of 2 playing, join in",
        _ => "Darts - 2 playing",
    };
    place_hint(
        label,
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
    let pressed = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Darts);
    let Some(board) = boards
        .as_ref()
        .and_then(|boards| nearby.of(Kind::Darts, &boards.0))
        .filter(|_| pressed)
    else {
        return;
    };
    let id = online::table_id(board.cell());
    // Both seats taken: nowhere to play.
    if seats::seated(&id) >= 2 {
        return;
    }
    commands.insert_resource(game::AtBoard(id));
    mode.set(Mode::Darts);
}

fn hide_hint(
    mut hint: Single<&mut Visibility, With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    **hint = Visibility::Hidden;
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Darts {
            *shown = Visibility::Hidden;
        }
    }
}
