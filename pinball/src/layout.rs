//! A table's layout, as its art exports it (tools/pinball-art, assets/pinball/<table>/layout.json):
//! what the ball collides with and where, in inches on the playfield, x across and y down the
//! table toward the player.

use glam::Vec2;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Width and length of the playfield.
    pub size: [f32; 2],
    pub camera: CameraParams,
    pub flipper: FlipperParams,
    /// Wooden and steel walls: polylines this thick.
    pub walls: Vec<Wall>,
    /// Posts, each with a rubber ring round it (`rubbers`).
    pub posts: Vec<Circle>,
    /// Rubber rings stretched round one or more posts (the circles are the ring's outside).
    pub rubbers: Vec<Vec<Circle>>,
    /// Solid things standing on the playfield: their outline.
    pub blocks: Vec<Block>,
    pub slings: Vec<Sling>,
    pub bumpers: Vec<Circle>,
    pub ramp: Ramp,
    pub doors: Doors,
    pub drop_targets: Vec<Target>,
    pub standups: Vec<Target>,
    pub scoop: Circle,
    pub captive: Captive,
    pub rollovers: Vec<Point>,
    /// Where the mine at the top of the ramp lets the ball back out onto the playfield.
    pub mine_exit: [f32; 2],
    pub gate: Gate,
    pub plunger: Plunger,
    pub drain: Drain,
    pub flippers: Vec<FlipperPlace>,
}

impl Layout {
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CameraParams {
    /// Degrees the playfield tilts away from the camera, and how far the camera is (inches).
    pub tilt: f32,
    pub dist: f32,
    /// Screen rows (of 1080) the playfield's far and near ends land on, and its middle column.
    pub fit_top: f32,
    pub fit_bottom: f32,
    pub center_x: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FlipperParams {
    pub length: f32,
    pub pivot_r: f32,
    pub tip_r: f32,
    /// Degrees, y down the table, for the left flipper: at rest pointing down and right, up
    /// pointing up and right. The right flipper mirrors them.
    pub rest: f32,
    pub up: f32,
    /// How many drawings of each flipper there are, from rest to up.
    pub frames: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Wall {
    pub points: Vec<[f32; 2]>,
    pub width: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Circle {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl Circle {
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Block {
    pub name: String,
    pub points: Vec<[f32; 2]>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Sling {
    /// The kicking rubber runs from `top` to `bottom`, facing the playfield.
    pub top: [f32; 2],
    pub bottom: [f32; 2],
    pub posts: Vec<[f32; 2]>,
    /// The posts' radius with their rubber.
    pub r: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Ramp {
    /// Its middle, from the mouth on the playfield to the top: x, y and height.
    pub path: Vec<[f32; 3]>,
    pub half_width: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Doors {
    /// Across the lane from x0 to x1, at y.
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub x: f32,
    pub y: f32,
    /// Its face runs along `along`, `half_width` each way, facing `facing`.
    pub along: [f32; 2],
    pub facing: [f32; 2],
    pub half_width: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Captive {
    pub x: f32,
    pub y: f32,
    pub lane: Lane,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Lane {
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Gate {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plunger {
    /// Where the tip is at rest, in the shooter lane, and how far it pulls back (inches).
    pub x: f32,
    pub y: f32,
    pub max_pull: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Drain {
    pub y: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct FlipperPlace {
    pub pivot: [f32; 2],
    pub side: Side,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}
