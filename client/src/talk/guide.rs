//! The way to somewhere in the bar, once someone on the staff points it out: an arrow by the
//! player's feet points there, and a marker bobs over it with its name. Both go once the player
//! is next to it, or sits down anywhere.

use bevy::prelude::*;
use world::{TILE_HEIGHT, world_to_cell};

use crate::Mode;
use crate::nearby::{hint_label, place_hint};
use crate::player::Player;

/// The arrow's art (tools/art/build_staff.py), pointing right.
const ARROW: &str = "ui/guide_arrow.png";
/// How far from the player's feet the arrow sits, in world pixels, and how far up.
const ARROW_OUT: f32 = 18.0;
const ARROW_UP: f32 = 4.0;
/// Over everything drawn in the bar.
const OVER: f32 = 50.0;
/// How far the marker bobs, in world pixels, and how often a second.
const BOB: f32 = 3.0;
const BOBS: f32 = 1.5;

/// Somewhere to show the way to: its name, where the marker goes over it, and the cells it
/// covers, next to any of which the player has got there.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub label: String,
    pub above: Vec2,
    pub cells: Vec<IVec2>,
}

impl Place {
    /// Whether `feet` are in or next to one of its cells.
    fn reached(&self, feet: Vec2) -> bool {
        let standing = world_to_cell(feet);
        self.cells
            .iter()
            .any(|cell| (*cell - standing).abs().max_element() <= 1)
    }
}

/// Where the player is being shown the way to, if anywhere.
#[derive(Resource, Default)]
pub struct Guide(pub Option<Place>);

/// The arrow by the player's feet, or the marker over the place.
#[derive(Component, PartialEq)]
enum Pointer {
    Arrow,
    Marker,
}

#[derive(Component)]
struct Label;

pub fn plugin(app: &mut App) {
    app.init_resource::<Guide>()
        .add_systems(Startup, spawn)
        .add_systems(Update, guide.run_if(in_state(Mode::Walking)))
        .add_systems(OnExit(Mode::Walking), forget);
}

/// Sitting down anywhere, the player is done being shown the way.
fn forget(
    mut guide: ResMut<Guide>,
    mut pointers: Query<&mut Visibility, With<Pointer>>,
    mut label: Single<&mut Visibility, (With<Label>, Without<Pointer>)>,
) {
    guide.0 = None;
    for mut visibility in &mut pointers {
        *visibility = Visibility::Hidden;
    }
    **label = Visibility::Hidden;
}

fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    for pointer in [Pointer::Arrow, Pointer::Marker] {
        let mut sprite = Sprite::from_image(asset_server.load(ARROW));
        sprite.color = Color::srgb(1.0, 0.85, 0.45);
        commands.spawn((pointer, sprite, Transform::default(), Visibility::Hidden));
    }
    commands.spawn((Label, hint_label()));
}

fn guide(
    mut guide: ResMut<Guide>,
    time: Res<Time>,
    player: Single<&Player>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut pointers: Query<(&Pointer, &mut Transform, &mut Visibility), Without<Label>>,
    label: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Label>>,
) {
    let (mut text, mut node, mut label_visibility, computed) = label.into_inner();
    if guide
        .0
        .as_ref()
        .is_some_and(|place| place.reached(player.feet))
    {
        guide.0 = None;
    }
    let Some(place) = &guide.0 else {
        for (_, _, mut visibility) in &mut pointers {
            *visibility = Visibility::Hidden;
        }
        *label_visibility = Visibility::Hidden;
        return;
    };
    // The arrow, pointing from the player's middle toward the place; the marker over the
    // place, pointing down at it.
    let middle = player.feet + Vec2::Y * TILE_HEIGHT;
    let toward = (place.above - middle).normalize_or_zero();
    let bob = BOB * (time.elapsed_secs() * BOBS * std::f32::consts::TAU).sin();
    for (pointer, mut transform, mut visibility) in &mut pointers {
        let (at, angle) = match pointer {
            Pointer::Arrow => (
                player.feet + Vec2::Y * ARROW_UP + toward * ARROW_OUT,
                toward.to_angle(),
            ),
            Pointer::Marker => (
                place.above + Vec2::Y * (BOB + bob),
                -std::f32::consts::FRAC_PI_2,
            ),
        };
        *transform = Transform::from_translation(at.extend(OVER))
            .with_rotation(Quat::from_rotation_z(angle));
        *visibility = Visibility::Inherited;
    }
    // Its name over the marker.
    place_hint(
        &place.label,
        place.above + Vec2::Y * (2.0 * BOB + 12.0),
        *camera,
        (&mut text, &mut node, &mut label_visibility, computed),
    );
}
