//! Pool physics on a top-down table, in felt pixels: x runs right along the table's length, y
//! down across it, and (0, 0) is the felt's top-left corner, where the cushions meet.
//!
//! Balls are equal circles that roll in straight lines, slowing down evenly, and bounce off
//! each other and the cushions losing a little speed. The cushions stop short of the pockets
//! and end in jaws angled in towards each pocket's hole, set back in the rail: a ball rolls in
//! between the jaws, maybe bouncing off them, and drops once its middle is over the hole. Time
//! moves in fixed steps, small enough that the fastest ball can't pass through another, and
//! nothing here depends on the frame rate or on anything but the shot, so the same shot on the
//! same table always ends the same way.

pub mod rules;

use glam::Vec2;

/// The felt between the cushions.
pub const WIDTH: f32 = 256.0;
pub const HEIGHT: f32 = 128.0;
pub const BALL_RADIUS: f32 = 3.5;
/// A corner pocket's jaws: how far back they reach from the mouth, and how much they close in
/// on each side on the way, so the opening narrows towards the hole. The hole's middle is
/// further back than the jaws reach, and it's a little wider than where they end.
const CORNER_JAW_DEPTH: f32 = 8.0;
const CORNER_JAW_NARROWING: f32 = 2.0;
const CORNER_HOLE_DISTANCE: f32 = 12.0;
/// A side pocket's mouth, next to a corner's, and its jaws, shaped like a corner's but
/// shorter, the rail being narrower than a corner is deep.
const SIDE_MOUTH: f32 = 0.9;
const SIDE_JAW_DEPTH: f32 = 5.0;
const SIDE_JAW_NARROWING: f32 = 1.5;
const SIDE_HOLE_DISTANCE: f32 = 7.5;
/// How much wider a hole is than the jaws' far ends, each side.
const HOLE_MARGIN: f32 = 1.5;
/// How far the rail reaches past the felt: a ball that somehow got that far fell off.
pub const RAIL: f32 = 15.0;
/// Seconds per step.
pub const STEP: f32 = 1.0 / 480.0;

/// Where the cue ball starts and comes back to, and where the rack's front ball sits.
pub const HEAD_SPOT: Vec2 = Vec2::new(WIDTH / 4.0, HEIGHT / 2.0);
/// The rack sits a ball short of three quarters down the table: right on that mark, a full
/// break straight at it sinks a back corner ball two times out of three.
pub const FOOT_SPOT: Vec2 = Vec2::new(WIDTH * 3.0 / 4.0 - BALL_RADIUS * 2.0, HEIGHT / 2.0);
/// Places in the rack, counted front to back and top to bottom in each row: the middle,
/// where the 8 goes, and the two back corners, one solid and one stripe.
const RACK_MIDDLE: usize = 4;
const RACK_CORNERS: [usize; 2] = [10, 14];
/// How far apart racked balls are, and how far each may sit off its place: tiny, but enough
/// that no two breaks go alike. The gap is wide enough that two balls nudged towards each
/// other still don't touch.
const RACK_GAP: f32 = 0.06;
const RACK_WOBBLE: f32 = 0.02;
/// How the table plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// How fast the cue ball leaves the cue at the least and the most power, in pixels per
    /// second.
    pub min_speed: f32,
    pub max_speed: f32,
    /// How fast a rolling ball slows down, in pixels per second per second.
    pub friction: f32,
    /// How much of their closing speed two balls keep, and a ball off a cushion.
    pub ball_restitution: f32,
    pub cushion_restitution: f32,
    /// How wide a corner pocket's mouth is, between the points where its cushions end. The
    /// side pockets' are a little narrower, and the holes grow with them.
    pub pocket_mouth: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            min_speed: 30.0,
            max_speed: 800.0,
            friction: 100.0,
            ball_restitution: 0.95,
            cushion_restitution: 0.75,
            pocket_mouth: 15.5,
        }
    }
}

/// A pocket: the points where the cushions on either side of it end, the far ends of the jaws
/// running back from them, and the hole behind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pocket {
    pub points: [Vec2; 2],
    pub jaw_ends: [Vec2; 2],
    pub hole: Vec2,
    pub hole_radius: f32,
}

/// The four corner pockets, then the two side ones.
pub fn pockets(settings: &Settings) -> [Pocket; 6] {
    let corner = |at: Vec2, along_a: Vec2, along_b: Vec2| {
        // The mouth runs across the corner, at 45 degrees to both cushions.
        let reach = settings.pocket_mouth / 2f32.sqrt();
        let points = [at + along_a * reach, at + along_b * reach];
        let middle = (points[0] + points[1]) / 2.0;
        let back = -(along_a + along_b).normalize();
        let across = (points[0] - points[1]).normalize();
        let jaw_ends = [
            points[0] + back * CORNER_JAW_DEPTH - across * CORNER_JAW_NARROWING,
            points[1] + back * CORNER_JAW_DEPTH + across * CORNER_JAW_NARROWING,
        ];
        Pocket {
            points,
            jaw_ends,
            hole: middle + back * CORNER_HOLE_DISTANCE,
            hole_radius: jaw_ends[0].distance(jaw_ends[1]) / 2.0 + HOLE_MARGIN,
        }
    };
    let side = |at: Vec2, back: Vec2| {
        let half = settings.pocket_mouth * SIDE_MOUTH / 2.0;
        let points = [at + Vec2::X * half, at - Vec2::X * half];
        let jaw_ends = [
            points[0] + back * SIDE_JAW_DEPTH - Vec2::X * SIDE_JAW_NARROWING,
            points[1] + back * SIDE_JAW_DEPTH + Vec2::X * SIDE_JAW_NARROWING,
        ];
        Pocket {
            points,
            jaw_ends,
            hole: at + back * SIDE_HOLE_DISTANCE,
            hole_radius: jaw_ends[0].distance(jaw_ends[1]) / 2.0 + HOLE_MARGIN,
        }
    };
    [
        corner(Vec2::ZERO, Vec2::X, Vec2::Y),
        corner(Vec2::new(WIDTH, 0.0), Vec2::NEG_X, Vec2::Y),
        corner(Vec2::new(0.0, HEIGHT), Vec2::X, Vec2::NEG_Y),
        corner(Vec2::new(WIDTH, HEIGHT), Vec2::NEG_X, Vec2::NEG_Y),
        side(Vec2::new(WIDTH / 2.0, 0.0), Vec2::NEG_Y),
        side(Vec2::new(WIDTH / 2.0, HEIGHT), Vec2::Y),
    ]
}

/// Every cushion as a line a ball's edge can't cross: the four long stretches between the
/// pockets, and the two jaws of each pocket.
fn cushions(pockets: &[Pocket; 6]) -> Vec<[Vec2; 2]> {
    let [top_left, top_right, bottom_left, bottom_right, top, bottom] = pockets;
    let mut lines = vec![
        [top_left.points[0], top.points[1]],
        [top.points[0], top_right.points[0]],
        [bottom_left.points[0], bottom.points[1]],
        [bottom.points[0], bottom_right.points[0]],
        [top_left.points[1], bottom_left.points[1]],
        [top_right.points[1], bottom_right.points[1]],
    ];
    for pocket in pockets {
        for (point, end) in pocket.points.iter().zip(pocket.jaw_ends) {
            lines.push([*point, end]);
        }
    }
    lines
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ball {
    /// 0 is the cue ball, then 1 to 7 solids, 8 the black, and 9 to 15 stripes.
    pub number: u8,
    pub position: Vec2,
    /// In pixels per second.
    pub velocity: Vec2,
    pub pocketed: bool,
}

impl Ball {
    fn new(number: u8, position: Vec2) -> Self {
        Self {
            number,
            position,
            velocity: Vec2::ZERO,
            pocketed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    /// The cue ball first, then the others.
    pub balls: Vec<Ball>,
    pub settings: Settings,
    /// What has happened since the last shot was taken, for the rules to judge.
    pub shot: Shot,
}

/// What a shot did: the ball the cue ball hit first, and every ball that dropped, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shot {
    pub first_hit: Option<u8>,
    pub pocketed: Vec<u8>,
}

impl Table {
    /// The cue ball on the head spot and the other fifteen in a triangle on the foot spot,
    /// racked as for 8-ball: the 8 in the middle, a solid and a stripe in the back corners, the
    /// rest anywhere, and every ball a tiny bit off its place. `seed` decides all of that, so
    /// the same seed always racks the same way (on every player's table).
    pub fn racked(seed: u32) -> Self {
        let mut random = Random::new(seed);
        let spacing = BALL_RADIUS * 2.0 + RACK_GAP;
        let row_step = spacing * 3f32.sqrt() / 2.0;
        let mut balls = vec![Ball::new(0, HEAD_SPOT)];
        let numbers = rack_order(&mut random);
        let mut numbers = numbers.iter();
        for row in 0..5 {
            for i in 0..=row {
                let place = Vec2::new(
                    row as f32 * row_step,
                    (i as f32 - row as f32 / 2.0) * spacing,
                );
                let wobble = Vec2::new(random.between(-1.0, 1.0), random.between(-1.0, 1.0));
                let number = *numbers.next().expect("the rack has 15 balls");
                balls.push(Ball::new(number, FOOT_SPOT + place + wobble * RACK_WOBBLE));
            }
        }
        Self {
            balls,
            settings: Settings::default(),
            shot: Shot::default(),
        }
    }

    pub fn cue_ball(&self) -> &Ball {
        &self.balls[0]
    }

    /// Hits the cue ball towards `direction` with `power` from 0 (the softest tap) to 1.
    pub fn shoot(&mut self, direction: Vec2, power: f32) {
        let Settings {
            min_speed,
            max_speed,
            ..
        } = self.settings;
        let speed = min_speed + (max_speed - min_speed) * power.clamp(0.0, 1.0);
        self.balls[0].velocity = direction.normalize_or_zero() * speed;
        self.shot = Shot::default();
    }

    /// Something on the table is still rolling.
    pub fn is_moving(&self) -> bool {
        self.balls
            .iter()
            .any(|ball| !ball.pocketed && ball.velocity != Vec2::ZERO)
    }

    /// Every ball but the cue ball is down.
    pub fn cleared(&self) -> bool {
        self.balls[1..].iter().all(|ball| ball.pocketed)
    }

    /// Whether the cue ball could be put at `at`: on the felt, clear of every other ball.
    pub fn cue_ball_fits(&self, at: Vec2) -> bool {
        let on_felt = (BALL_RADIUS..=WIDTH - BALL_RADIUS).contains(&at.x)
            && (BALL_RADIUS..=HEIGHT - BALL_RADIUS).contains(&at.y);
        on_felt
            && self.balls[1..]
                .iter()
                .all(|ball| ball.pocketed || ball.position.distance(at) >= BALL_RADIUS * 2.0 + 0.02)
    }

    /// Brings a pocketed cue ball back to the head spot, or the nearest free place along the
    /// table's middle line from there.
    pub fn respot_cue_ball(&mut self) {
        if self.balls[0].pocketed {
            self.respot(0);
        }
    }

    /// Puts a ball back on the table: the cue ball on the head spot and any other on the foot
    /// spot, or the nearest free place along the table's middle line from there, towards the
    /// near end for the cue ball and the far end for the others.
    pub fn respot(&mut self, number: u8) {
        let Some(index) = self.balls.iter().position(|ball| ball.number == number) else {
            return;
        };
        let free = |at: Vec2| {
            self.balls.iter().enumerate().all(|(other, ball)| {
                other == index
                    || ball.pocketed
                    || ball.position.distance(at) >= BALL_RADIUS * 2.0 + 0.02
            })
        };
        let (spot, onwards) = if number == 0 {
            (HEAD_SPOT, -1.0)
        } else {
            (FOOT_SPOT, 1.0)
        };
        let spot = (0..WIDTH as i32)
            .flat_map(|step| [step, -step])
            .map(|step| spot + Vec2::X * step as f32 * onwards)
            .filter(|at| (BALL_RADIUS..=WIDTH - BALL_RADIUS).contains(&at.x))
            .find(|&at| free(at))
            .unwrap_or(spot);
        self.balls[index] = Ball::new(number, spot);
    }

    /// Moves time on by one step.
    pub fn step(&mut self) {
        let settings = self.settings;
        let pockets = pockets(&settings);
        let cushions = cushions(&pockets);
        for ball in self.balls.iter_mut().filter(|ball| !ball.pocketed) {
            ball.position += ball.velocity * STEP;
            let speed = ball.velocity.length();
            let slowdown = settings.friction * STEP;
            ball.velocity = if speed <= slowdown {
                Vec2::ZERO
            } else {
                ball.velocity * ((speed - slowdown) / speed)
            };
            bounce_off_cushions(ball, &cushions, settings.cushion_restitution);
        }
        self.collide_balls();
        for ball in self.balls.iter_mut().filter(|ball| !ball.pocketed) {
            if falls_in(ball.position, &pockets) {
                ball.pocketed = true;
                ball.velocity = Vec2::ZERO;
                self.shot.pocketed.push(ball.number);
            }
        }
    }

    fn collide_balls(&mut self) {
        let touching = BALL_RADIUS * 2.0;
        for i in 0..self.balls.len() {
            for j in i + 1..self.balls.len() {
                if self.balls[i].pocketed || self.balls[j].pocketed {
                    continue;
                }
                let apart = self.balls[j].position - self.balls[i].position;
                let distance = apart.length();
                if distance >= touching {
                    continue;
                }
                // From i to j, and any way at all for two balls exactly on top of each other.
                let normal = if distance > 0.0 {
                    apart / distance
                } else {
                    Vec2::X
                };
                // Out of each other, half the overlap each.
                let push = normal * (touching - distance) / 2.0;
                self.balls[i].position -= push;
                self.balls[j].position += push;
                let closing = (self.balls[i].velocity - self.balls[j].velocity).dot(normal);
                if closing > 0.0 {
                    if i == 0 && self.shot.first_hit.is_none() {
                        self.shot.first_hit = Some(self.balls[j].number);
                    }
                    let restitution = self.settings.ball_restitution;
                    let impulse = normal * closing * (1.0 + restitution) / 2.0;
                    self.balls[i].velocity -= impulse;
                    self.balls[j].velocity += impulse;
                }
            }
        }
    }
}

/// The fifteen balls in their places in the rack (see `RACK_MIDDLE` and `RACK_CORNERS`).
fn rack_order(random: &mut Random) -> [u8; 15] {
    let mut solids: Vec<u8> = (1..=7).collect();
    let mut stripes: Vec<u8> = (9..=15).collect();
    let solid = solids.remove(random.below(solids.len()));
    let stripe = stripes.remove(random.below(stripes.len()));
    let corners = if random.below(2) == 0 {
        [solid, stripe]
    } else {
        [stripe, solid]
    };
    // The other twelve, shuffled (Fisher-Yates), into the places left.
    let mut rest: Vec<u8> = solids.into_iter().chain(stripes).collect();
    for i in (1..rest.len()).rev() {
        rest.swap(i, random.below(i + 1));
    }
    let mut rest = rest.into_iter();
    std::array::from_fn(|place| match place {
        RACK_MIDDLE => 8,
        _ if place == RACK_CORNERS[0] => corners[0],
        _ if place == RACK_CORNERS[1] => corners[1],
        _ => rest.next().expect("twelve balls for twelve places"),
    })
}

/// Numbers that look random but follow from the seed alone (SplitMix32), so every player's
/// table can rack the same way from it.
struct Random(u32);

impl Random {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9);
        let mut z = self.0;
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        z ^ (z >> 16)
    }

    /// From 0 up to, not including, `n`.
    fn below(&mut self, n: usize) -> usize {
        ((self.next() as u64 * n as u64) >> 32) as usize
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Bounces a ball off any cushion its edge has gone past. A cushion's ends are round, so a
/// ball clipping the point of a jaw glances off it.
fn bounce_off_cushions(ball: &mut Ball, cushions: &[[Vec2; 2]], restitution: f32) {
    // Twice over, for a ball in the angle where two cushions meet.
    for _ in 0..2 {
        for &[from, to] in cushions {
            let along = to - from;
            let t = ((ball.position - from).dot(along) / along.length_squared()).clamp(0.0, 1.0);
            let nearest = from + along * t;
            let away = ball.position - nearest;
            let distance = away.length();
            if distance >= BALL_RADIUS || distance == 0.0 {
                continue;
            }
            let normal = away / distance;
            ball.position = nearest + normal * BALL_RADIUS;
            let into = ball.velocity.dot(normal);
            if into < 0.0 {
                ball.velocity -= normal * into * (1.0 + restitution);
            }
        }
    }
}

/// Over a pocket's hole, or somehow past the rail.
fn falls_in(position: Vec2, pockets: &[Pocket; 6]) -> bool {
    let off_table = position.x < -RAIL
        || position.x > WIDTH + RAIL
        || position.y < -RAIL
        || position.y > HEIGHT + RAIL;
    off_table
        || pockets
            .iter()
            .any(|pocket| pocket.hole.distance(position) < pocket.hole_radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table with just these balls on it, the first being the cue ball.
    fn table_with(positions: &[Vec2]) -> Table {
        Table {
            balls: positions
                .iter()
                .enumerate()
                .map(|(number, &at)| Ball::new(number as u8, at))
                .collect(),
            settings: Settings::default(),
            shot: Shot::default(),
        }
    }

    fn run_until_still(table: &mut Table) -> u32 {
        let mut steps = 0;
        while table.is_moving() {
            table.step();
            steps += 1;
            assert!(steps < 60 * 480, "still rolling after a minute");
        }
        steps
    }

    #[test]
    fn a_straight_hit_passes_the_speed_on() {
        let mut table = table_with(&[Vec2::new(64.0, 64.0), Vec2::new(100.0, 64.0)]);
        table.shoot(Vec2::X, 0.5);
        while table.balls[1].velocity == Vec2::ZERO {
            table.step();
        }
        let (cue, object) = (table.balls[0].velocity, table.balls[1].velocity);
        assert!(object.x > 200.0, "{object}");
        assert!(object.y.abs() < 1e-3);
        assert!(cue.x.abs() < object.x * 0.05, "{cue} {object}");
        assert_eq!(table.shot.first_hit, Some(1));
    }

    #[test]
    fn balls_bounce_off_cushions() {
        let mut table = table_with(&[Vec2::new(WIDTH - 20.0, HEIGHT / 2.0)]);
        table.shoot(Vec2::X, 0.5);
        for _ in 0..60 {
            table.step();
        }
        let ball = &table.balls[0];
        assert!(ball.velocity.x < 0.0);
        assert!(ball.position.x <= WIDTH - BALL_RADIUS);
        assert!(!ball.pocketed);
    }

    #[test]
    fn balls_drop_in_corner_and_side_pockets() {
        let mut corner = table_with(&[Vec2::new(30.0, 30.0)]);
        corner.shoot(Vec2::new(-1.0, -1.0), 0.3);
        run_until_still(&mut corner);
        assert!(corner.balls[0].pocketed);
        assert_eq!(corner.shot.pocketed, [0]);

        let mut side = table_with(&[Vec2::new(WIDTH / 2.0, HEIGHT / 2.0)]);
        side.shoot(Vec2::Y, 0.3);
        run_until_still(&mut side);
        assert!(side.balls[0].pocketed);
    }

    #[test]
    fn a_ball_rolling_along_a_cushion_drops_in_the_corner() {
        let mut table = table_with(&[Vec2::new(60.0, BALL_RADIUS)]);
        table.shoot(Vec2::NEG_X, 0.3);
        run_until_still(&mut table);
        assert!(table.balls[0].pocketed);
    }

    #[test]
    fn a_ball_clipping_a_jaw_comes_back_out() {
        // From a shallow angle, at the corner's jaw rather than its hole.
        let mut table = table_with(&[Vec2::new(40.0, 20.0)]);
        let towards = Vec2::new(8.0, -2.0) - table.balls[0].position;
        table.shoot(towards, 0.3);
        let mut came_back = false;
        while table.is_moving() {
            table.step();
            came_back |= table.balls[0].velocity.dot(towards) < 0.0;
        }
        assert!(came_back);
        assert!(!table.balls[0].pocketed);
    }

    #[test]
    fn holes_sit_back_from_the_felt_behind_their_jaws() {
        for pocket in pockets(&Settings::default()) {
            let Vec2 { x, y } = pocket.hole;
            let outside = !(0.0..=WIDTH).contains(&x) || !(0.0..=HEIGHT).contains(&y);
            assert!(outside, "{pocket:?}");
            for end in pocket.jaw_ends {
                assert!(end.distance(pocket.hole) < pocket.hole_radius, "{pocket:?}");
            }
            // Wide enough at the jaws' far ends for a ball to get through.
            assert!(pocket.jaw_ends[0].distance(pocket.jaw_ends[1]) > BALL_RADIUS * 2.0);
        }
    }

    #[test]
    fn a_break_comes_to_rest_with_every_ball_on_the_felt_or_down() {
        let mut table = Table::racked(7);
        table.shoot(Vec2::X, 1.0);
        run_until_still(&mut table);
        let on_table: Vec<&Ball> = table.balls.iter().filter(|ball| !ball.pocketed).collect();
        for ball in &on_table {
            let Vec2 { x, y } = ball.position;
            assert!(
                (0.0..=WIDTH).contains(&x) && (0.0..=HEIGHT).contains(&y),
                "{ball:?}"
            );
        }
        for (i, a) in on_table.iter().enumerate() {
            for b in &on_table[i + 1..] {
                assert!(a.position.distance(b.position) > BALL_RADIUS * 2.0 - 0.5);
            }
        }
    }

    #[test]
    fn the_same_shot_ends_the_same_way() {
        let shot = |table: &mut Table| {
            table.shoot(Vec2::new(1.0, 0.03), 0.9);
            run_until_still(table)
        };
        let (mut a, mut b) = (Table::racked(3), Table::racked(3));
        assert_eq!(shot(&mut a), shot(&mut b));
        assert_eq!(a, b);
    }

    #[test]
    fn racks_follow_the_rules_and_differ_by_seed() {
        for seed in 0..200 {
            let table = Table::racked(seed);
            let rack = &table.balls[1..];
            let mut numbers: Vec<u8> = rack.iter().map(|ball| ball.number).collect();
            assert_eq!(rack[RACK_MIDDLE].number, 8);
            let corners = RACK_CORNERS.map(|place| rack[place].number);
            assert_eq!(corners.iter().filter(|&&n| n < 8).count(), 1, "{corners:?}");
            numbers.sort();
            assert_eq!(numbers, (1..=15).collect::<Vec<u8>>());
            for (i, a) in table.balls.iter().enumerate() {
                for b in &table.balls[i + 1..] {
                    assert!(
                        a.position.distance(b.position) > BALL_RADIUS * 2.0,
                        "{seed}"
                    );
                }
            }
        }
        assert_ne!(Table::racked(1), Table::racked(2));
        assert_eq!(Table::racked(5), Table::racked(5));
    }

    #[test]
    fn a_pocketed_cue_ball_comes_back_to_a_free_spot() {
        let mut table = table_with(&[Vec2::new(30.0, 30.0), HEAD_SPOT]);
        table.balls[0].pocketed = true;
        table.respot_cue_ball();
        let cue = table.cue_ball();
        assert!(!cue.pocketed);
        assert!(cue.position.distance(HEAD_SPOT) >= BALL_RADIUS * 2.0);
        assert_eq!(cue.position.y, HEAD_SPOT.y);
    }
}
