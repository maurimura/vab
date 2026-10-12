//! A placeholder player that walks the bar with the arrow keys or WASD (holding Shift runs), or
//! a thumb dragged on a touch screen (touch.rs). It can stand on floor tiles without an object,
//! and slides along whatever blocks it.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use world::{Map, TILE_HEIGHT, TILE_WIDTH, cell_to_world, world_to_cell};

use crate::Mode;
use crate::chat::chat_closed;
use crate::controls::controls_closed;
use crate::help::help_closed;
use crate::room;
use crate::settings::settings_closed;
use crate::touch::Touch;

/// Walking speed in world pixels per second.
const SPEED: f32 = 64.0;
/// How many times faster the player runs, holding Shift.
const RUN: f32 = 2.0;
/// How far back from a wall the player keeps, gliding along it, in world pixels: right on its
/// edge, rounding could put their feet in the wall's cell.
const HAIR: f32 = 0.01;
/// Roughly how many world pixels tall the view is; the zoom is the whole number closest to it.
const VIEW_HEIGHT: f32 = 270.0;
/// In a window much taller than wide (a phone held upright), how many it is wide instead.
const VIEW_WIDTH: f32 = 240.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Zoom(1.0)).add_systems(
            Update,
            (
                walk.run_if(
                    in_state(Mode::Walking)
                        .and_then(chat_closed)
                        .and_then(help_closed)
                        .and_then(settings_closed)
                        // The arrows are the controls panel's while it's up (in the bar,
                        // every game's).
                        .and_then(controls_closed),
                ),
                follow,
            )
                .chain(),
        );
    }
}

/// Cells the player can stand on: floor without an object on it.
#[derive(Resource)]
pub struct Walkable(HashSet<IVec2>);

impl Walkable {
    pub fn from_map(map: &Map) -> Self {
        let blocked: HashSet<IVec2> = map.objects.iter().flat_map(|p| p.cells()).collect();
        Self(
            map.floor
                .iter()
                .map(|p| IVec2::new(p.x, p.y))
                .filter(|cell| !blocked.contains(cell))
                .collect(),
        )
    }

    fn allows(&self, feet: Vec2) -> bool {
        self.contains(world_to_cell(feet))
    }

    /// How far a step from `feet` gets: the whole step if it's free, otherwise along the wall
    /// it ran into. Walls run along the floor's diagonals, so it glides along those first (the
    /// one closer to the way it was going, at a corner), then sideways or up and down, along a
    /// wall that zigzags.
    fn slide(&self, feet: Vec2, step: Vec2) -> Vec2 {
        let mut diagonals = [
            along(step, Vec2::new(TILE_WIDTH, TILE_HEIGHT)),
            along(step, Vec2::new(TILE_WIDTH, -TILE_HEIGHT)),
        ];
        if diagonals[1].length_squared() > diagonals[0].length_squared() {
            diagonals.reverse();
        }
        // Along one diagonal, and a hair back from the wall the rest of the step went into.
        let glide = |along: Vec2, into: Vec2| {
            (along.length() > HAIR).then(|| along - into.normalize_or_zero() * HAIR)
        };
        let [closer, farther] = diagonals;
        [
            Some(step),
            glide(closer, farther),
            glide(farther, closer),
            Some(Vec2::new(step.x, 0.0)),
            Some(Vec2::new(0.0, step.y)),
        ]
        .into_iter()
        .flatten()
        .find(|attempt| *attempt != Vec2::ZERO && self.allows(feet + *attempt))
        .unwrap_or(Vec2::ZERO)
    }

    pub fn contains(&self, cell: IVec2) -> bool {
        self.0.contains(&cell)
    }

    pub fn cells(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.0.iter().copied()
    }

    /// The free cell nearest the middle of the bar.
    pub fn start(&self) -> Option<IVec2> {
        let middle = self.0.iter().map(|cell| cell.as_vec2()).sum::<Vec2>() / self.0.len() as f32;
        self.0.iter().copied().min_by(|a, b| {
            let da = a.as_vec2().distance_squared(middle);
            let db = b.as_vec2().distance_squared(middle);
            da.total_cmp(&db).then(a.x.cmp(&b.x)).then(a.y.cmp(&b.y))
        })
    }
}

/// How many of the screen's own pixels a world pixel takes up: a whole number, set from the
/// window's size.
#[derive(Resource)]
pub struct Zoom(pub f32);

#[derive(Component)]
pub struct Player {
    /// Where the player's feet are, in world pixels.
    pub feet: Vec2,
}

pub fn spawn_player(commands: &mut Commands, asset_server: &AssetServer, walkable: &Walkable) {
    let Some(cell) = walkable.start() else {
        return;
    };
    let feet = cell_to_world(cell.x, cell.y);
    room::room_move(feet.x, feet.y, false);
    commands.spawn((
        Player { feet },
        Sprite::from_image(asset_server.load("characters/player.png")),
        Anchor::BOTTOM_CENTER,
        Transform::from_translation(translation(feet, 1.0)),
    ));
}

fn walk(
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    time: Res<Time>,
    walkable: Res<Walkable>,
    mut players: Query<(&mut Player, &mut Sprite)>,
) {
    let mut direction = Vec2::ZERO;
    for (key_a, key_b, towards) in [
        (KeyCode::ArrowUp, KeyCode::KeyW, Vec2::Y),
        (KeyCode::ArrowDown, KeyCode::KeyS, Vec2::NEG_Y),
        (KeyCode::ArrowLeft, KeyCode::KeyA, Vec2::NEG_X),
        (KeyCode::ArrowRight, KeyCode::KeyD, Vec2::X),
    ] {
        if keys.any_pressed([key_a, key_b]) {
            direction += towards;
        }
    }
    // A thumb walks the way it points on the screen, which the halving below would flatten.
    direction += touch.stick() * Vec2::new(1.0, 2.0);
    if direction == Vec2::ZERO {
        return;
    }
    let speed = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        SPEED * RUN
    } else {
        SPEED
    };
    // Up and down at half speed, so walking looks even on the 2:1 isometric floor.
    let step = direction.normalize() * Vec2::new(1.0, 0.5) * speed * time.delta_secs();

    for (mut player, mut sprite) in &mut players {
        let moved = walkable.slide(player.feet, step);
        player.feet += moved;
        // Facing the way the player goes, or wants to when a wall stops them.
        let sideways = if moved.x != 0.0 { moved.x } else { step.x };
        if sideways != 0.0 {
            sprite.flip_x = sideways < 0.0;
        }
        room::room_move(player.feet.x, player.feet.y, sprite.flip_x);
    }
}

/// The part of a step that runs along a wall going `wall`'s way, measured on the floor rather
/// than on the screen, where up and down are squashed to half: walking into a wall at 45° keeps
/// 70% of the speed rather than a quarter.
fn along(step: Vec2, wall: Vec2) -> Vec2 {
    let floor = Vec2::new(1.0, TILE_WIDTH / TILE_HEIGHT);
    (step * floor).project_onto(wall * floor) / floor
}

/// Snapped to whole screen pixels, with the same depth rule as map objects (lower on screen
/// draws on top), nudged ahead of objects on the same row. Snapping to screen pixels rather
/// than world pixels keeps diagonal walking smooth: x and y cross world pixels on different
/// frames, which shakes the view by a whole world pixel at a time.
pub fn translation(feet: Vec2, zoom: f32) -> Vec3 {
    let row = -feet.y / (TILE_HEIGHT / 2.0);
    snap(feet, zoom).extend(1.0 + row * 0.001 + 0.0005)
}

fn snap(position: Vec2, zoom: f32) -> Vec2 {
    (position * zoom).round() / zoom
}

/// Keeps the player in the middle of the view, at a whole number of the screen's own pixels to
/// each of the world's so they stay square (a phone has three or so to a logical one).
fn follow(
    window: Single<&Window>,
    mut zoom: ResMut<Zoom>,
    player: Single<(&Player, &mut Transform), Without<Camera2d>>,
    camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let tall = window.physical_height() as f32 / VIEW_HEIGHT;
    let wide = window.physical_width() as f32 / VIEW_WIDTH;
    let scale = tall.min(wide).round().max(1.0);
    if zoom.0 != scale {
        zoom.0 = scale;
    }
    let (player, mut player_transform) = player.into_inner();
    player_transform.translation = translation(player.feet, scale);
    let (mut transform, mut projection) = camera.into_inner();
    // Aim at the player's middle rather than their feet, snapped the same way so they stay put.
    let target = snap(player.feet + Vec2::Y * TILE_HEIGHT, scale);
    transform.translation = target.extend(transform.translation.z);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = window.scale_factor() / scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 by 20 floor, but for the cells `wall` says are walls.
    fn floor(wall: impl Fn(IVec2) -> bool) -> Walkable {
        let cells = (0..20).flat_map(|x| (0..20).map(move |y| IVec2::new(x, y)));
        Walkable(cells.filter(|cell| !wall(*cell)).collect())
    }

    /// Where the player's feet end up, walking from the middle of `cell` with the keys for
    /// `direction` held, for `frames` sixtieths of a second.
    fn walk(walkable: &Walkable, cell: IVec2, direction: Vec2, frames: usize) -> Vec2 {
        let step = direction.normalize() * Vec2::new(1.0, 0.5) * SPEED / 60.0;
        let mut feet = cell_to_world(cell.x, cell.y);
        for _ in 0..frames {
            feet += walkable.slide(feet, step);
            assert!(walkable.allows(feet), "walked into a wall at {feet}");
        }
        feet
    }

    #[test]
    fn walking_up_into_a_diagonal_wall_glides_along_it() {
        // Walls up and to the right, running from the bottom right to the top left.
        let to_the_right = floor(|cell| cell.y == 4);
        let start = IVec2::new(10, 5);
        let from = cell_to_world(start.x, start.y);
        let feet = walk(&to_the_right, start, Vec2::Y, 60);
        assert!(
            feet.y > from.y + 8.0 && feet.x < from.x - 16.0,
            "{from} to {feet}"
        );

        // And the mirror: walls up and to the left, running from the bottom left to the top right.
        let to_the_left = floor(|cell| cell.x == 4);
        let start = IVec2::new(5, 10);
        let from = cell_to_world(start.x, start.y);
        let feet = walk(&to_the_left, start, Vec2::Y, 60);
        assert!(
            feet.y > from.y + 8.0 && feet.x > from.x + 16.0,
            "{from} to {feet}"
        );
    }

    #[test]
    fn walking_straight_at_a_wall_stops_at_it() {
        let walls = floor(|cell| cell.y == 4);
        let feet = walk(&walls, IVec2::new(10, 6), Vec2::new(1.0, 1.0), 60);
        let wall = cell_to_world(10, 4);
        // Not slid off sideways: still about in line with the cell it walked towards.
        let off_line = (feet - wall).perp_dot(Vec2::new(TILE_WIDTH, TILE_HEIGHT).normalize());
        assert!(off_line.abs() < 2.0, "slid {off_line} off to {feet}");
    }

    #[test]
    fn gliding_off_the_end_of_a_wall_walks_on_past_it() {
        let walls = floor(|cell| cell.y == 4 && cell.x >= 8);
        let feet = walk(&walls, IVec2::new(10, 5), Vec2::Y, 120);
        assert!(world_to_cell(feet).y < 4, "stuck at {feet}");
    }
}
