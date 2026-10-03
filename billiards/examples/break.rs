// How well breaks spread, for tuning: a full-power break at the rack's front ball, over 21
// racks. Prints the share of the cue ball's energy the other balls have just after
// the hit, how far from their middle they end up on average, how many go down, and how many
// are still where the rack was.
//
//     cargo run -p billiards --example break -- <hardest shot> <friction> <ball bounce> <cushion bounce>
//     cargo run -p billiards --example break -- 500 100 0.95 0.75
use billiards::{STEP, Table};
use glam::Vec2;

fn energy(table: &Table, from: usize) -> f32 {
    table.balls[from..]
        .iter()
        .filter(|b| !b.pocketed)
        .map(|b| b.velocity.length_squared())
        .sum()
}

fn main() {
    let args: Vec<f32> = std::env::args()
        .skip(1)
        .map(|a| a.parse().unwrap())
        .collect();
    let (mut kept, mut spread, mut down, mut clumped) = (0.0, 0.0, 0, 0);
    let aims = 21;
    for k in 0..aims {
        let mut table = Table::racked(k);
        table.settings.max_speed = args[0];
        table.settings.friction = args[1];
        table.settings.ball_restitution = args[2];
        table.settings.cushion_restitution = args[3];
        let aim = table.balls[1].position - table.cue_ball().position;
        table.shoot(aim, 1.0);
        let start = energy(&table, 0);
        // until the hit has gone through the rack: 0.1 s after the front ball first moves
        while table.balls[1].velocity == Vec2::ZERO {
            table.step();
        }
        for _ in 0..(0.1 / STEP) as u32 {
            table.step();
        }
        kept += energy(&table, 1) / start;
        while table.is_moving() {
            table.step();
        }
        let on: Vec<Vec2> = table.balls[1..]
            .iter()
            .filter(|b| !b.pocketed)
            .map(|b| b.position)
            .collect();
        let middle = on.iter().copied().sum::<Vec2>() / on.len() as f32;
        spread += on.iter().map(|p| p.distance(middle)).sum::<f32>() / on.len() as f32;
        down += 15 - on.len();
        let rack = billiards::FOOT_SPOT + Vec2::X * 12.0;
        clumped += on.iter().filter(|p| p.distance(rack) < 18.0).count();
    }
    let n = aims as f32;
    println!(
        "rack gets {:.0}% of the energy, ends {:.1}px from its middle, {:.2} down, {:.1} of 15 still in the rack area",
        kept / n * 100.0,
        spread / n,
        down as f32 / n,
        clumped as f32 / n
    );
}
