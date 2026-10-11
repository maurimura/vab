// Plunges a ball at each pull of the plunger and follows it for a few seconds, flippers still:
// prints the paths, with what the ball bounces off, as JSON in screen pixels (the art's 1920 by
// 1080). tools/pinball-art/trace.py draws them over the table's art:
//   cargo run -p pinball --example trace > trace.json && python3 tools/pinball-art/trace.py trace.json
use pinball::{Camera, Layout, Settings, Table};

const SECONDS: f32 = 5.0;
const FRAME: f32 = 1.0 / 120.0;

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
    let max_pull = layout.plunger.max_pull;
    let mut paths = Vec::new();
    for tenth in 1..=12 {
        let pull = max_pull * tenth as f32 / 12.0;
        let mut table = Table::new(layout.clone(), Settings::default());
        table.serve();
        table.advance(0.5);
        table.hold_plunger(true);
        while table.plunger < pull - 1e-4 {
            table.advance(FRAME);
        }
        table.hold_plunger(false);
        let mut points = Vec::new();
        let mut events = Vec::new();
        let mut t = 0.0;
        while t < SECONDS {
            for event in table.advance(FRAME) {
                events.push(format!("{t:.2}s {event:?}"));
            }
            let Some(ball) = table.balls.first() else {
                break;
            };
            let at = camera.project(table.ball_at(ball));
            points.push([at.x.round(), at.y.round()]);
            t += FRAME;
        }
        let end = table.balls.first().map(|b| [b.position.x, b.position.y]);
        paths.push(
            serde_json::json!({ "pull": pull, "points": points, "events": events, "end": end }),
        );
    }
    println!(
        "{}",
        serde_json::json!({ "outlines": outlines, "paths": paths })
    );
}
