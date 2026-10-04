//! Air hockey tables (objects/air_hockey, two cells long): next to one, a hint says so, and E
//! (or the Play button on a touch screen) sits the player at it to play against a bot
//! (game.rs).

mod game;

use bevy::prelude::*;
use world::{Map, Placed};

use crate::Mode;
use crate::cabinets::{hint_label, place_hint};
use crate::chat::chat_closed;
use crate::help::help_closed;
use crate::player::Player;
use crate::pool::next_to;
use crate::settings::settings_closed;
use crate::touch::{self, Touch, TouchButton};

const TILE: &str = "objects/air_hockey";
/// Where the hint sits: a little above the table's top, in world pixels from its middle.
const HINT_HEIGHT: f32 = 30.0;

pub struct HockeyPlugin;

impl Plugin for HockeyPlugin {
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

/// The air hockey tables in the bar.
#[derive(Resource)]
pub struct HockeyTables(Vec<Placed>);

impl HockeyTables {
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
        // Where the cabinets' and the pool table's Play buttons go; one is offered at a time.
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                bottom: Val::Px(24.0),
                ..default()
            },
            children![(
                touch::button(TouchButton::Hockey, "Play", 96.0, 48.0),
                Visibility::Hidden,
            )],
        ));
    }
    commands.spawn((Hint, hint_label()));
}

fn show_hint(
    tables: Option<Res<HockeyTables>>,
    touch: Res<Touch>,
    player: Single<&Player>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let near = tables
        .as_ref()
        .and_then(|tables| next_to(&tables.0, player.feet));
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Hockey {
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
    let label = if touch.is_on() {
        "Air hockey"
    } else {
        "E  Air hockey"
    };
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
    tables: Option<Res<HockeyTables>>,
    player: Single<&Player>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let sit = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Hockey);
    let near = tables.is_some_and(|tables| next_to(&tables.0, player.feet).is_some());
    if sit && near {
        mode.set(Mode::Hockey);
    }
}

fn hide_hint(
    mut hint: Single<&mut Visibility, With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    **hint = Visibility::Hidden;
    for (button, mut shown) in &mut buttons {
        if *button == TouchButton::Hockey {
            *shown = Visibility::Hidden;
        }
    }
}
