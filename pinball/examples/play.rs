// A bot plays the table, flipping whenever a ball comes down onto a flipper, and says how it
// went: how fast the ball goes, how long a ball lasts, where they drain, and what they hit. For
// tuning the physics; any setting can be changed for the run:
//   cargo run --release -p pinball --example play -- bumper_kick=120 plunger_speed=150
use glam::Vec2;
use pinball::{Event, Layout, Place, Settings, Side, Table};

const BALLS: usize = 60;
const LONGEST: f32 = 90.0;
const FRAME: f32 = 1.0 / 60.0;

fn main() {
    let mut settings = Settings::default();
    for arg in std::env::args().skip(1) {
        let (name, value) = arg.split_once('=').expect("name=value");
        let value: f32 = value.parse().expect("a number");
        let field = match name {
            "slope" => &mut settings.slope,
            "flipper_speed" => &mut settings.flipper_speed,
            "flipper_return" => &mut settings.flipper_return,
            "flipper_bounce" => &mut settings.flipper_bounce,
            "wall_bounce" => &mut settings.wall_bounce,
            "rubber_bounce" => &mut settings.rubber_bounce,
            "bumper_kick" => &mut settings.bumper_kick,
            "sling_kick" => &mut settings.sling_kick,
            "plunger_speed" => &mut settings.plunger_speed,
            "rolling" => &mut settings.rolling,
            "ramp_speed" => &mut settings.ramp_speed,
            "max_speed" => &mut settings.max_speed,
            _ => panic!("no setting {name}"),
        };
        *field = value;
    }
    let layout =
        Layout::from_json(include_str!("../../assets/pinball/saloon/layout.json")).unwrap();
    let mut seed = 12345u32;
    let mut random = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };

    let mut speeds = Vec::new();
    let mut lives = Vec::new();
    let (mut left_out, mut right_out, mut middle, mut timed_out) = (0, 0, 0, 0);
    let mut hits = std::collections::BTreeMap::<String, usize>::new();
    let mut plunges = Vec::new();
    for n in 0..BALLS {
        let mut table = Table::new(layout.clone(), settings);
        table.serve();
        table.advance(0.3);
        // Every strength, in turn.
        let pull = layout.plunger.max_pull * (0.05 + 0.95 * (n % 12) as f32 / 11.0);
        table.hold_plunger(true);
        while table.plunger < pull - 1e-4 {
            table.advance(FRAME);
        }
        table.hold_plunger(false);
        let mut time = 0.0;
        let mut first_flip = None;
        // How long each flipper stays up, then down before it flips again (so a ball resting
        // on it rolls off toward the tip, rather than being cradled for ever).
        let mut held = [0.0f32; 2];
        let mut rest = [0.0f32; 2];
        let mut last = Vec2::ZERO;
        while time < LONGEST {
            // The bot: a flipper flips while a ball comes down onto it, a moment late or early.
            for (i, side) in [Side::Left, Side::Right].into_iter().enumerate() {
                let flipper = table.flippers[i];
                let tip = flipper.pivot + Vec2::from_angle(flipper.angle) * 2.75;
                let (x0, x1) = (
                    flipper.pivot.x.min(tip.x) - 0.4,
                    flipper.pivot.x.max(tip.x) + 0.4,
                );
                let coming = table.balls.iter().any(|b| {
                    b.place == Place::Playfield
                        && b.position.x > x0
                        && b.position.x < x1
                        && b.position.y > flipper.pivot.y - 2.2 - random() * 0.6
                        && b.position.y < flipper.pivot.y + 1.0
                        && b.velocity.y > -30.0
                });
                if coming && held[i] <= 0.0 && rest[i] <= 0.0 {
                    held[i] = 0.2;
                    rest[i] = 0.2 + 0.3 * random();
                }
                if held[i] > 0.0 {
                    held[i] -= FRAME;
                } else {
                    rest[i] -= FRAME;
                }
                table.flip(side, held[i] > 0.0);
            }
            for event in table.advance(FRAME) {
                let name = format!("{event:?}");
                let name = name.split(['(', ' ']).next().unwrap().to_string();
                *hits.entry(name).or_default() += 1;
                if event == Event::Drain {
                    if last.x < 1.9 {
                        left_out += 1;
                    } else if last.x > 17.1 {
                        right_out += 1;
                    } else {
                        middle += 1;
                    }
                }
            }
            time += FRAME;
            let Some(ball) = table.balls.first() else {
                break;
            };
            last = ball.position;
            if ball.place == Place::Playfield && !table.in_shooter_lane(ball) {
                speeds.push(ball.velocity.length());
                if first_flip.is_none() && ball.position.y > 30.0 {
                    first_flip = Some(time);
                }
            }
            if n < 12
                && plunges.len() == n
                && !table.in_shooter_lane(ball)
                && ball.position.y > 24.0
            {
                plunges.push(format!(
                    "{pull:.2}: down to y 24 after {time:.1}s at x {:.1}",
                    ball.position.x
                ));
            }
        }
        if time >= LONGEST {
            timed_out += 1;
        }
        lives.push(time);
    }
    speeds.sort_by(f32::total_cmp);
    let at = |q: f32| speeds[((speeds.len() - 1) as f32 * q) as usize];
    let minutes = lives.iter().sum::<f32>() / 60.0;
    lives.sort_by(f32::total_cmp);
    println!(
        "speed (in/s): median {:.0}, 90% {:.0}, 99% {:.0}, top {:.0}; over 150: {:.0}% of the time",
        at(0.5),
        at(0.9),
        at(0.99),
        at(1.0),
        100.0 * speeds.iter().filter(|&&s| s > 150.0).count() as f32 / speeds.len() as f32
    );
    println!(
        "a ball lasts: median {:.0}s (shortest {:.0}s, longest {:.0}s); {timed_out} of {BALLS} still going after {LONGEST:.0}s",
        lives[lives.len() / 2],
        lives[0],
        lives[lives.len() - 1]
    );
    println!(
        "drained: left outlane {left_out}, right outlane {right_out}, between the flippers {middle}"
    );
    let per_minute: Vec<String> = hits
        .iter()
        .map(|(name, n)| format!("{name} {:.0}", *n as f32 / minutes))
        .collect();
    println!("a minute: {}", per_minute.join(", "));
    for plunge in plunges {
        println!("plunge {plunge}");
    }
}
