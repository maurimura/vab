//! Hooks for tests in a browser (tools/e2e), in dev builds only: the `test-hooks` feature, which
//! `make client` turns on for every profile but wasm-release, so the site never has them. The
//! page puts them on `window.vab` (web/index.html):
//!
//! - `vab.state()`: what the game is doing, as JSON: the mode, where the player stands, and
//!   what each game reports of itself (`report`), refreshed every frame.
//! - `vab.goTo(name)`: puts the player next to the first object whose tile ends with `name`
//!   ("shuffleboard", "pool_table", "air_hockey") or whose cabinet runs the game `name`, for E
//!   to use, instead of walking there, which a headless browser's few frames a second make
//!   uneven.
//! - `vab.standAt(x, y, towardX, towardY)`: puts the player in the cell `x`, `y`, a little to
//!   the side of the cell `towardX`, `towardY` (to tell apart which of two things next to it is
//!   nearer: nearby.rs).
//! - `vab.throw(vx, vy)`: at the shuffleboard table, on this player's throw, throws the waiting
//!   puck from where it is at exactly that velocity (table pixels per second), as a push would.

use std::cell::RefCell;

use bevy::prelude::*;
use serde_json::{Map as Json, Value, json};
use wasm_bindgen::prelude::*;
use world::{Placed, cell_to_world, world_to_cell};

use crate::Mode;
use crate::nearby::Nearby;
use crate::player::{Player, Walkable};
use crate::room;

thread_local! {
    /// What `vab.state()` returns, by key.
    static STATE: RefCell<Json<String, Value>> = RefCell::new(Json::new());
    /// Where `vab.goTo` asked the player to go, until they're there.
    static GO_TO: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Where `vab.standAt` asked the player to stand, until they're there.
    static STAND_AT: RefCell<Option<Vec2>> = const { RefCell::new(None) };
    /// The throw `vab.throw` asked for, until the shuffleboard game takes it.
    static THROW: RefCell<Option<Vec2>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn vab_state() -> String {
    STATE.with_borrow(|state| Value::Object(state.clone()).to_string())
}

#[wasm_bindgen]
pub fn vab_go_to(name: String) {
    GO_TO.with_borrow_mut(|go_to| *go_to = Some(name));
}

#[wasm_bindgen]
pub fn vab_stand_at(x: i32, y: i32, toward_x: i32, toward_y: i32) {
    let (at, toward) = (cell_to_world(x, y), cell_to_world(toward_x, toward_y));
    let feet = at + (toward - at).normalize_or_zero() * 5.0;
    STAND_AT.with_borrow_mut(|stand_at| *stand_at = Some(feet));
}

#[wasm_bindgen]
pub fn vab_throw(vx: f32, vy: f32) {
    THROW.with_borrow_mut(|throw| *throw = Some(Vec2::new(vx, vy)));
}

/// Sets what `vab.state()` says under `key`.
pub fn report(key: &str, value: Value) {
    STATE.with_borrow_mut(|state| {
        state.insert(key.to_string(), value);
    });
}

/// The throw asked for, if any: taken, so it's thrown once.
pub fn take_throw() -> Option<Vec2> {
    THROW.with_borrow_mut(Option::take)
}

/// Every object in the bar, for `vab.goTo`.
#[derive(Resource)]
pub struct Objects(pub Vec<Placed>);

pub struct TestingPlugin;

impl Plugin for TestingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (go_to, stand_at).run_if(in_state(Mode::Walking)),
                report_player,
            ),
        );
    }
}

fn report_player(
    mode: Res<State<Mode>>,
    time: Res<Time>,
    nearby: Res<Nearby>,
    mut frames: Local<u64>,
    player: Query<&Player>,
) {
    *frames += 1;
    report("mode", json!(format!("{:?}", mode.get())));
    report("frame", json!(*frames));
    report("seconds", json!(time.elapsed_secs()));
    let near = nearby
        .get()
        .map(|(kind, cell)| json!({ "kind": format!("{kind:?}"), "cell": [cell.x, cell.y] }));
    report("nearby", json!(near));
    if let Ok(player) = player.single() {
        let cell = world_to_cell(player.feet);
        report(
            "player",
            json!({ "feet": [player.feet.x, player.feet.y], "cell": [cell.x, cell.y] }),
        );
    }
}

/// Puts the player on a free cell next to the object asked for: one next to nothing else if
/// there is one, so E uses that object, and the nearest its middle of those.
fn go_to(
    objects: Option<Res<Objects>>,
    walkable: Option<Res<Walkable>>,
    mut player: Query<&mut Player>,
) {
    let (Some(objects), Some(walkable), Ok(mut player)) = (objects, walkable, player.single_mut())
    else {
        return;
    };
    let Some(name) = GO_TO.with_borrow_mut(Option::take) else {
        return;
    };
    let Some(object) = objects
        .0
        .iter()
        .find(|placed| placed.tile.ends_with(&name) || placed.game.as_deref() == Some(&name))
    else {
        report("go_to", json!({ "name": name, "found": false }));
        return;
    };
    let around = |cell: IVec2| {
        (-1..=1).flat_map(move |dx| (-1..=1).map(move |dy| cell + IVec2::new(dx, dy)))
    };
    // How many other objects a cell is next to.
    let others = |cell: IVec2| {
        objects
            .0
            .iter()
            .filter(|other| !std::ptr::eq(*other, object))
            .filter(|other| around(cell).any(|near| other.covers(near)))
            .count()
    };
    let middle = object.center();
    let next_to = object
        .cells()
        .flat_map(around)
        .filter(|cell| walkable.contains(*cell))
        .min_by(|a, b| {
            let (a_at, b_at) = (cell_to_world(a.x, a.y), cell_to_world(b.x, b.y));
            let (da, db) = (a_at.distance(middle), b_at.distance(middle));
            others(*a)
                .cmp(&others(*b))
                .then(da.total_cmp(&db))
                .then(a.x.cmp(&b.x))
                .then(a.y.cmp(&b.y))
        });
    let Some(cell) = next_to else {
        report(
            "go_to",
            json!({ "name": name, "found": true, "free": false }),
        );
        return;
    };
    player.feet = cell_to_world(cell.x, cell.y);
    room::room_move(player.feet.x, player.feet.y, false);
    report(
        "go_to",
        json!({ "name": name, "found": true, "cell": [cell.x, cell.y] }),
    );
}

/// Puts the player where `vab.standAt` asked.
fn stand_at(mut player: Query<&mut Player>) {
    let Ok(mut player) = player.single_mut() else {
        return;
    };
    if let Some(feet) = STAND_AT.with_borrow_mut(Option::take) {
        player.feet = feet;
        room::room_move(feet.x, feet.y, false);
    }
}
