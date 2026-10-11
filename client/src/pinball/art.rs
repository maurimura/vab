//! A pinball table's art, in parts (tools/pinball-art exports them into assets/pinball/<table>/):
//! where each image goes on a 1920x1080 screen, how far down the table it reaches (its depth, so
//! the ball is drawn in front of or behind it), and the moving parts' images, one per state.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Sprites {
    /// The screen the parts are placed on.
    pub size: [f32; 2],
    pub background: Placed,
    pub items: Vec<Item>,
    /// By name: "flipper-left", "bumper-0", "bottle-3", "doors", "bull", "plunger".
    pub states: HashMap<String, States>,
    /// Each lamp, lit, by name.
    pub lamps: HashMap<String, Placed>,
    /// The backbox's rank lamps, lit, from the lowest rank up.
    pub ranks: Vec<Placed>,
    pub front: Placed,
    pub backbox: Backbox,
    pub ball: BallArt,
}

impl Sprites {
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

/// An image, and the screen rectangle it covers.
#[derive(Deserialize, Clone)]
pub struct Placed {
    pub file: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Deserialize)]
pub struct Item {
    #[serde(flatten)]
    pub placed: Placed,
    pub name: String,
    pub depth: f32,
}

#[derive(Deserialize)]
pub struct States {
    pub depth: f32,
    /// What each state is (an angle, lit or not, ...), as the art drew it.
    pub states: Vec<serde_json::Value>,
    /// An image per state; none for a state with nothing to draw.
    pub frames: Vec<Option<Placed>>,
}

impl States {
    /// The state nearest `value` (for numbered states: angles, pulls).
    pub fn nearest(&self, value: f32) -> usize {
        let distance =
            |state: &serde_json::Value| (state.as_f64().unwrap_or(0.0) as f32 - value).abs();
        (0..self.states.len())
            .min_by(|&a, &b| distance(&self.states[a]).total_cmp(&distance(&self.states[b])))
            .unwrap_or(0)
    }
}

#[derive(Deserialize)]
pub struct Backbox {
    #[serde(flatten)]
    pub placed: Placed,
    pub dmd: DmdPlace,
}

/// The display's top-left corner on screen, and the distance between its dots.
#[derive(Deserialize, Clone, Copy)]
pub struct DmdPlace {
    pub x: f32,
    pub y: f32,
    pub pitch: f32,
}

#[derive(Deserialize)]
pub struct BallArt {
    #[serde(flatten)]
    pub placed: Placed,
    /// How many screen pixels to an inch where the ball was drawn: the game scales it to
    /// wherever the ball is.
    pub scale: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saloon_has_its_parts() {
        let sprites =
            Sprites::from_json(include_str!("../../../assets/pinball/saloon/sprites.json"))
                .unwrap();
        assert!(sprites.items.len() > 100);
        for part in [
            "flipper-left",
            "flipper-right",
            "bumper-0",
            "bottle-4",
            "doors",
            "bull",
            "plunger",
        ] {
            assert!(sprites.states.contains_key(part), "{part}");
        }
        let doors = &sprites.states["doors"];
        assert_eq!(doors.states[doors.nearest(1.0)], serde_json::json!(0));
        assert!(sprites.lamps.contains_key("ramp-arrow"));
    }
}
