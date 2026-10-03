// How often a full-power break straight at the rack's front ball sinks a back-corner ball, over
// 50 racks: if it's most of them, breaking is a trick rather than a shot.
//
//     cargo run -p billiards --example trick
use billiards::Table;
fn main() {
    let (mut corner_downs, mut downs, racks) = (0, 0, 50);
    for seed in 0..racks {
        let mut table = Table::racked(seed);
        let corners = [table.balls[11].number, table.balls[15].number];
        let aim = table.balls[1].position - table.cue_ball().position;
        table.shoot(aim, 1.0);
        while table.is_moving() {
            table.step();
        }
        corner_downs += table
            .balls
            .iter()
            .filter(|b| b.pocketed && corners.contains(&b.number))
            .count();
        downs += table.balls[1..].iter().filter(|b| b.pocketed).count();
    }
    println!(
        "{:.0}% of breaks sink a back-corner ball, {:.2} balls down per break",
        corner_downs as f32 / racks as f32 * 100.0,
        downs as f32 / racks as f32
    );
}
