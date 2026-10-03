//! Pool tables (objects/pool_table, two cells long): next to one, a hint says so, and E (or
//! the Pool button on a touch screen) sits the player at it to play (game.rs).

mod game;

use bevy::prelude::*;
use world::{Map, Placed, world_to_cell};

use crate::Mode;
use crate::cabinets::{hint_label, place_hint};
use crate::chat::chat_closed;
use crate::help::help_closed;
use crate::player::Player;
use crate::settings::settings_closed;
use crate::touch::{self, Touch, TouchButton};

const TILE: &str = "objects/pool_table";
/// Where the hint sits: a little above the table's top, in world pixels from its middle.
const HINT_HEIGHT: f32 = 30.0;

pub struct PoolPlugin;

impl Plugin for PoolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(game::GamePlugin)
            .add_systems(Startup, spawn_hint)
            .add_systems(
                Update,
                (
                    show_hint,
                    sit.run_if(chat_closed)
                        .run_if(help_closed)
                        .run_if(settings_closed),
                )
                    .run_if(in_state(Mode::Walking)),
            )
            .add_systems(OnExit(Mode::Walking), hide_hint);
    }
}

/// The pool tables in the bar.
#[derive(Resource)]
pub struct PoolTables(Vec<Placed>);

impl PoolTables {
    pub fn from_map(map: &Map) -> Self {
        Self(
            map.objects
                .iter()
                .filter(|placed| placed.tile == TILE)
                .cloned()
                .collect(),
        )
    }

    /// The nearest table with a cell in one of the 8 cells around the player's feet.
    fn next_to(&self, feet: Vec2) -> Option<&Placed> {
        let cell = world_to_cell(feet);
        self.0
            .iter()
            .filter(|table| {
                table
                    .cells()
                    .any(|covered| (covered - cell).abs().max_element() == 1)
            })
            .min_by(|a, b| {
                let da = a.center().distance_squared(feet);
                let db = b.center().distance_squared(feet);
                da.total_cmp(&db)
            })
    }
}

#[derive(Component)]
struct Hint;

fn spawn_hint(mut commands: Commands, touch: Res<Touch>) {
    if touch.is_on() {
        // Where the cabinets' Play button goes; the two are never offered at once.
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(24.0),
                ..default()
            },
            children![(
                touch::button(TouchButton::Pool, "Play", 96.0, 48.0),
                Visibility::Hidden,
            )],
        ));
    }
    commands.spawn((Hint, hint_label()));
}

fn show_hint(
    tables: Res<PoolTables>,
    touch: Res<Touch>,
    player: Single<&Player>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let near = tables.next_to(player.feet);
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Pool {
            shown.set_if_neq(if near.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    let Some(table) = near else {
        *visibility = Visibility::Hidden;
        return;
    };
    // The button says what to press.
    let label = if touch.is_on() { "Pool" } else { "E  Pool" };
    place_hint(
        label,
        table.center() + Vec2::Y * HINT_HEIGHT,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

fn sit(
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    tables: Res<PoolTables>,
    player: Single<&Player>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let sit = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Pool);
    if sit && tables.next_to(player.feet).is_some() {
        mode.set(Mode::Pool);
    }
}

fn hide_hint(
    mut hint: Single<&mut Visibility, With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    **hint = Visibility::Hidden;
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Pool {
            *shown = Visibility::Hidden;
        }
    }
}
