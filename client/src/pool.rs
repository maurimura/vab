//! Pool tables (objects/pool_table, two cells long): next to one, a hint says what it is. The
//! game itself comes later.

use bevy::prelude::*;
use world::{Map, Placed, world_to_cell};

use crate::Mode;
use crate::cabinets::{hint_label, place_hint};
use crate::player::Player;

const TILE: &str = "objects/pool_table";
/// Where the hint sits: a little above the table's top, in world pixels from its middle.
const HINT_HEIGHT: f32 = 30.0;

pub struct PoolPlugin;

impl Plugin for PoolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hint)
            .add_systems(Update, show_hint.run_if(in_state(Mode::Walking)))
            .add_systems(OnEnter(Mode::Playing), hide_hint);
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

fn spawn_hint(mut commands: Commands) {
    commands.spawn((Hint, hint_label()));
}

fn show_hint(
    tables: Res<PoolTables>,
    player: Single<&Player>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let Some(table) = tables.next_to(player.feet) else {
        *visibility = Visibility::Hidden;
        return;
    };
    place_hint(
        "Pool - coming soon",
        table.center() + Vec2::Y * HINT_HEIGHT,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

fn hide_hint(mut hint: Single<&mut Visibility, With<Hint>>) {
    **hint = Visibility::Hidden;
}
