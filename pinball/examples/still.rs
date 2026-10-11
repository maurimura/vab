// Finds dead spots: drops a ball, still, all over the playfield, and says where balls come to
// rest (anywhere but on the plunger, or held by the scoop or the mine for a moment). Prints them
// in tools/pinball-art/trace.py's form, to draw over the art:
//   cargo run --release -p pinball --example still > still.json && python3 tools/pinball-art/trace.py still.json
use pinball::{Camera, Layout, Place, Settings, Table};

fn main() {
    let layout =
        Layout::from_json(include_str!("../../assets/pinball/saloon/layout.json")).unwrap();
    let camera = Camera::new(layout.camera, layout.size);
    let screen = |p: glam::Vec2| {
        let s = camera.project(p.extend(0.0));
        [s.x.round(), s.y.round()]
    };
    let table = Table::new(layout.clone(), Settings::default());
    let outlines: Vec<_> = table
        .outlines()
        .into_iter()
        .map(|(a, b, r)| {
            let scale = camera.pixels_per_inch(a.extend(0.0));
            serde_json::json!({ "a": screen(a), "b": screen(b), "r": r * scale })
        })
        .collect();
    let mut paths = Vec::new();
    for spot in pinball::dead_spots(&layout, 0.25) {
        paths.push(serde_json::json!({
            "pull": 0.0,
            "points": [screen(spot.from), screen(spot.at)],
            "events": [format!("from {:.2} {:.2}", spot.from.x, spot.from.y)],
            "end": [spot.at.x, spot.at.y],
        }));
    }
    eprintln!("{} dead spots", paths.len());
    let _ = Place::Playfield;
    println!(
        "{}",
        serde_json::json!({ "outlines": outlines, "paths": paths })
    );
}
