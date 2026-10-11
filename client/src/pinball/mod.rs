//! Pinball machines (objects/pinball_saloon, two cells long): next to one, a hint says so, and E
//! (or the Play button on a touch screen) steps up to it to play (game.rs).

mod art;
mod dmd;
mod game;

use bevy::prelude::*;
use world::{Map, Placed};

use crate::Mode;
use crate::chat::chat_closed;
use crate::help::help_closed;
use crate::nearby::{self, Kind, Nearby, hint_label, place_hint};
use crate::settings::settings_closed;
use crate::touch::{Touch, TouchButton};

const TILE: &str = "objects/pinball_saloon";
/// Where the hint sits: a little above the machine's backbox, in world pixels from its middle.
const HINT_HEIGHT: f32 = 52.0;

pub struct PinballPlugin;

impl Plugin for PinballPlugin {
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

/// The pinball machines in the bar.
#[derive(Resource)]
pub struct PinballMachines(Vec<Placed>);

impl PinballMachines {
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
    machines: Option<Res<PinballMachines>>,
    nearby: Res<Nearby>,
    touch: Res<Touch>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let Some(machine) = machines
        .as_ref()
        .and_then(|machines| nearby.of(Kind::Pinball, &machines.0))
    else {
        *visibility = Visibility::Hidden;
        return;
    };
    nearby::offer(&mut buttons, true, false);
    let label = nearby::hint_text("Saloon pinball", 0, 1, 0, touch.is_on());
    place_hint(
        &label,
        machine.center() + Vec2::Y * HINT_HEIGHT,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

fn step_up(
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    machines: Option<Res<PinballMachines>>,
    nearby: Res<Nearby>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let play = keys.just_pressed(KeyCode::KeyE) || touch.tapped(TouchButton::Play);
    let near = machines
        .as_ref()
        .and_then(|machines| nearby.of(Kind::Pinball, &machines.0));
    if play && near.is_some() {
        mode.set(Mode::Pinball);
    }
}

fn hide_hint(mut hint: Single<&mut Visibility, With<Hint>>) {
    **hint = Visibility::Hidden;
}
