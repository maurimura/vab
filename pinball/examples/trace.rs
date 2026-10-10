// Prints a plunged ball's path, for looking into the physics: cargo run -p pinball --example trace
use pinball::{Layout, Settings, Table};

fn main() {
    let layout =
        Layout::from_json(include_str!("../../assets/pinball/saloon/layout.json")).unwrap();
    let mut table = Table::new(layout, Settings::default());
    table.serve();
    for _ in 0..30 {
        table.advance(1.0 / 60.0);
    }
    table.hold_plunger(true);
    for _ in 0..60 {
        table.advance(1.0 / 60.0);
    }
    table.hold_plunger(false);
    for frame in 0..90 {
        let events = table.advance(1.0 / 120.0);
        if let Some(b) = table.balls.first() {
            println!(
                "{frame:3} {:6.2} {:6.2}  v {:7.1} {:7.1} {:?}",
                b.position.x, b.position.y, b.velocity.x, b.velocity.y, events
            );
        }
    }
}
