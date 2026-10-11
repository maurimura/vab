// What each flipper can hit: a ball rolls down a flipper from its base (as from the inlane), and
// the flipper flips at one moment after another; prints each shot's path for a second and a half
// and what it hit, in tools/pinball-art/trace.py's form, to draw over the art:
//   cargo run --release -p pinball --example shots > shots.json && python3 tools/pinball-art/trace.py shots.json
use glam::Vec2;
use pinball::{BALL_RADIUS, Camera, Layout, Side, Table};

fn main() {
    let layout =
        Layout::from_json(include_str!("../../assets/pinball/saloon/layout.json")).unwrap();
    let camera = Camera::new(layout.camera, layout.size);
    let screen = |p: glam::Vec3| {
        let s = camera.project(p);
        [s.x.round(), s.y.round()]
    };
    let settings = pinball::Settings { flipper_speed: 26.0, flipper_bounce: 0.3, ..pinball::Settings::default() };
    let table = Table::new(layout.clone(), settings);
    let outlines: Vec<_> = table
        .outlines()
        .into_iter()
        .map(|(a, b, r)| {
            let scale = camera.pixels_per_inch(a.extend(0.0));
            serde_json::json!({ "a": screen(a.extend(0.0)), "b": screen(b.extend(0.0)), "r": r * scale })
        })
        .collect();
    let mut paths = Vec::new();
    for (i, side) in [Side::Left, Side::Right].into_iter().enumerate() {
        for step in 0..14 {
            let wait = 0.05 + step as f32 * 0.025;
            let mut table = Table::new(layout.clone(), settings);
            let flipper = table.flippers[i];
            let along = Vec2::from_angle(flipper.angle);
            let above = Vec2::new(along.y, -along.x) * if i == 0 { 1.0 } else { -1.0 };
            let at = flipper.pivot + along * 0.5 + above * (0.4 + BALL_RADIUS);
            table.balls.push(pinball::Ball::new(at));
            table.advance_by(wait);
            let rolled = table.balls[0].position;
            table.flip(side, true);
            let mut points = Vec::new();
            let mut events = Vec::new();
            for frame in 0..90 {
                for event in table.advance(1.0 / 60.0) {
                    events.push(format!("{:.2}s {event:?}", frame as f32 / 60.0));
                }
                if frame == 20 {
                    table.flip(side, false);
                }
                let Some(ball) = table.balls.first() else {
                    break;
                };
                points.push(screen(table.ball_at(ball)));
            }
            events.insert(
                0,
                format!(
                    "{side:?} flip at {:.2} along",
                    (rolled - flipper.pivot).length()
                ),
            );
            paths.push(serde_json::json!({ "pull": wait, "points": points, "events": events, "end": table.balls.first().map(|b| [b.position.x, b.position.y]) }));
        }
    }
    println!(
        "{}",
        serde_json::json!({ "outlines": outlines, "paths": paths })
    );
}
