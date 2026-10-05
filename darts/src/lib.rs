//! Darts: what a dart scores where it lands on the board, a game of 301, and the hand throwing
//! it: how it sways about where it's aimed, and how the flick that throws it sends it off. Without Bevy, so it tests natively; the client draws the board and throws
//! (client/src/darts).
//!
//! Units are millimetres on the board's face, from its middle, y up, as a regulation board is
//! measured.

use glam::Vec2;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The rings, by their outer radius: the bull (50), the outer bull (25), the treble ring between
/// its inner and outer radius, and the double ring, the board's scoring edge.
pub const BULL: f32 = 6.35;
pub const OUTER_BULL: f32 = 15.9;
pub const TREBLE_INNER: f32 = 99.0;
pub const TREBLE_OUTER: f32 = 107.0;
pub const DOUBLE_INNER: f32 = 162.0;
pub const DOUBLE_OUTER: f32 = 170.0;
/// The numbers round the board, clockwise from the top.
pub const NUMBERS: [u32; 20] = [
    20, 1, 18, 4, 13, 6, 10, 15, 2, 17, 3, 19, 7, 16, 8, 11, 14, 9, 12, 5,
];
/// Where each player starts, counting down to zero.
pub const START: u32 = 301;
pub const DARTS_PER_TURN: usize = 3;

/// What a dart hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Hit {
    Miss,
    Single(u32),
    Double(u32),
    Treble(u32),
    OuterBull,
    Bull,
}

impl Hit {
    pub fn points(self) -> u32 {
        match self {
            Hit::Miss => 0,
            Hit::Single(number) => number,
            Hit::Double(number) => 2 * number,
            Hit::Treble(number) => 3 * number,
            Hit::OuterBull => 25,
            Hit::Bull => 50,
        }
    }

    /// The doubles, and the bull, which counts as one.
    pub fn is_double(self) -> bool {
        matches!(self, Hit::Double(_) | Hit::Bull)
    }

    /// As a scoreboard writes it: "T20", "D16", "7", "25", "Bull", "Miss".
    pub fn name(self) -> String {
        match self {
            Hit::Miss => "Miss".to_string(),
            Hit::Single(number) => number.to_string(),
            Hit::Double(number) => format!("D{number}"),
            Hit::Treble(number) => format!("T{number}"),
            Hit::OuterBull => "25".to_string(),
            Hit::Bull => "Bull".to_string(),
        }
    }
}

/// The number of the slice a point on the board is in.
pub fn number_at(at: Vec2) -> u32 {
    // Clockwise from the top, in degrees; each slice is 18 wide, the 20 centred at the top.
    let angle = at.x.atan2(at.y).to_degrees();
    let slice = ((angle + 9.0).rem_euclid(360.0) / 18.0) as usize % NUMBERS.len();
    NUMBERS[slice]
}

/// What a dart landing at `at` scores.
pub fn hit(at: Vec2) -> Hit {
    let from_middle = at.length();
    if from_middle <= BULL {
        Hit::Bull
    } else if from_middle <= OUTER_BULL {
        Hit::OuterBull
    } else if from_middle > DOUBLE_OUTER {
        Hit::Miss
    } else if from_middle >= DOUBLE_INNER {
        Hit::Double(number_at(at))
    } else if (TREBLE_INNER..=TREBLE_OUTER).contains(&from_middle) {
        Hit::Treble(number_at(at))
    } else {
        Hit::Single(number_at(at))
    }
}

/// What a dart did to the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Scored,
    /// Below zero (or, finishing on a double, left on 1, or on zero without one): the turn's
    /// darts don't count, and it's over.
    Bust,
    Won,
}

/// A game of 301: each player counts down from 301 by what their darts score, three a turn, and
/// the first to exactly zero wins.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Game {
    /// What each player has left.
    pub remaining: [u32; 2],
    /// Whose turn it is, and who threw first this game.
    pub turn: usize,
    pub first: usize,
    /// What each dart thrown this turn hit.
    pub darts: Vec<Hit>,
    /// What the player had at the start of the turn, which a bust takes them back to.
    pub turn_start: u32,
    pub busted: bool,
    /// Whether the last dart must be a double (or the bull). Off: any dart that makes zero wins.
    pub double_out: bool,
    pub win: Option<usize>,
}

impl Game {
    pub fn new(first: usize) -> Self {
        Self {
            remaining: [START; 2],
            turn: first,
            first,
            darts: Vec::new(),
            turn_start: START,
            busted: false,
            double_out: false,
            win: None,
        }
    }

    /// The turn is over: three darts thrown, or a bust, or a win.
    pub fn turn_over(&self) -> bool {
        self.busted || self.win.is_some() || self.darts.len() >= DARTS_PER_TURN
    }

    /// A dart thrown by whoever's turn it is, hitting `hit`.
    pub fn throw(&mut self, hit: Hit) -> Outcome {
        let player = self.turn;
        self.darts.push(hit);
        let left = i64::from(self.remaining[player]) - i64::from(hit.points());
        let bust = left < 0 || (self.double_out && (left == 1 || (left == 0 && !hit.is_double())));
        if bust {
            self.remaining[player] = self.turn_start;
            self.busted = true;
            return Outcome::Bust;
        }
        self.remaining[player] = left as u32;
        if left == 0 {
            self.win = Some(player);
            Outcome::Won
        } else {
            Outcome::Scored
        }
    }

    /// What the turn has scored so far: nothing, after a bust.
    pub fn turn_points(&self) -> u32 {
        if self.busted {
            0
        } else {
            self.turn_start - self.remaining[self.turn]
        }
    }

    /// The darts come out of the board, and the other player's turn begins.
    pub fn next_turn(&mut self) {
        self.turn = 1 - self.turn;
        self.darts.clear();
        self.busted = false;
        self.turn_start = self.remaining[self.turn];
    }
}

/// How the throwing hand moves, and throws.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Hand {
    /// About how far the hand drifts from where it's aimed, in millimetres (up and down and side
    /// to side together, a little further now and then).
    pub drift: f32,
    /// How much of the drift is left while the throw is held: the hand steadies.
    pub steady: f32,
    /// How long it stays steady, in seconds; held longer, it starts to shake, more every
    /// second, by `shake` millimetres.
    pub steady_for: f32,
    pub shake: f32,
    /// How far, at random, a dart lands from where the hand let it go: about this many
    /// millimetres, mostly less.
    pub scatter: f32,
    /// How fast a flick straight up sends a dart exactly where the hand has it, in millimetres
    /// a second; slower drops it lower, faster sends it higher.
    pub flick_speed: f32,
    /// How far a flick that's off sends the dart off: this many millimetres for a flick twice
    /// as fast as `flick_speed`, or as far to the side as it is fast.
    pub flick_spread: f32,
}

impl Default for Hand {
    fn default() -> Self {
        Self {
            drift: 40.0,
            steady: 0.4,
            steady_for: 1.5,
            shake: 40.0,
            scatter: 4.0,
            flick_speed: 900.0,
            flick_spread: 60.0,
        }
    }
}

/// How long holding the throw takes to steady the hand fully, in seconds.
const STEADYING: f32 = 0.3;

impl Hand {
    /// How far from where it's aimed the hand is at the time `t` (seconds), having held the
    /// throw for `held` seconds (0 while it isn't held): a slow, uneven loop, as a hand drifts.
    pub fn offset(&self, t: f32, held: f32) -> Vec2 {
        let drift = Vec2::new(
            (t * 1.3).sin() + 0.4 * (t * 2.9 + 1.0).sin(),
            0.8 * (t * 1.7 + 0.5).sin() + 0.3 * (t * 3.7).sin(),
        ) / 1.3
            * self.drift;
        let steadied = (held / STEADYING).clamp(0.0, 1.0);
        let calm = 1.0 + (self.steady - 1.0) * steadied;
        let shaking = (held - self.steady_for).max(0.0) * self.shake;
        let shake = Vec2::new((t * 23.0).sin(), (t * 19.0 + 0.7).sin()) * shaking;
        drift * calm + shake
    }

    /// Where a dart lands, at random, from where the hand let it go: from two numbers picked
    /// evenly between 0 and 1, a spread that's mostly within `scatter`.
    pub fn scatter(&self, first: f32, second: f32) -> Vec2 {
        // Box and Muller's: two even numbers make a point spread as a bell curve is.
        let distance = (-2.0 * first.clamp(1e-6, 1.0).ln()).sqrt() * self.scatter / 2.0;
        let angle = second * std::f32::consts::TAU;
        Vec2::new(angle.cos(), angle.sin()) * distance
    }

    /// Whether a flick at `velocity` (millimetres a second, up the board positive) throws at
    /// all: one hardly going up lets the dart be.
    pub fn throws(&self, velocity: Vec2) -> bool {
        velocity.y >= MIN_FLICK * self.flick_speed
    }

    /// How far off where the hand has it a flick at `velocity` sends the dart: not at all for one
    /// straight up at `flick_speed`, lower for a slower one, higher for a faster, and to the side
    /// it leans.
    pub fn flick_error(&self, velocity: Vec2) -> Vec2 {
        Vec2::new(velocity.x, velocity.y - self.flick_speed) / self.flick_speed * self.flick_spread
    }
}

/// A flick slower than this much of the right speed doesn't throw.
const MIN_FLICK: f32 = 0.3;
/// A flick's speed is from the hand's moves over this long before it lets go, and a hand that
/// hasn't moved for `STILL_AFTER` has stopped (seconds).
pub const FLICK_WINDOW: f32 = 0.1;
pub const STILL_AFTER: f32 = 0.05;

/// The hand's moves since the throw was held, for how fast and which way it was going when it
/// let go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flick {
    /// When (seconds) and where (millimetres) it was, frame by frame, oldest first.
    seen: Vec<(f32, Vec2)>,
    /// When it last moved.
    moved_at: f32,
}

impl Flick {
    pub fn new(now: f32, at: Vec2) -> Self {
        Self {
            seen: vec![(now, at)],
            moved_at: now,
        }
    }

    /// The hand is at `to`, at the time `now`.
    pub fn moved(&mut self, now: f32, to: Vec2) {
        if self.seen.last().is_none_or(|&(_, at)| at != to) {
            self.moved_at = now;
        }
        self.seen.push((now, to));
        // Those in the window, and the one before, where the first move in it started from.
        while self.seen.len() > 2 && now - self.seen[1].0 > FLICK_WINDOW {
            self.seen.remove(0);
        }
    }

    /// How fast the hand was going at the time `now`: over its last `FLICK_WINDOW`, or zero if
    /// it has stopped.
    pub fn velocity(&self, now: f32) -> Vec2 {
        let (Some(&(from_at, from)), Some(&(to_at, to))) = (self.seen.first(), self.seen.last())
        else {
            return Vec2::ZERO;
        };
        if now - self.moved_at > STILL_AFTER || to_at <= from_at {
            return Vec2::ZERO;
        }
        (to - from) / (to_at - from_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point `distance` from the middle, `degrees` clockwise from the top.
    fn at(distance: f32, degrees: f32) -> Vec2 {
        let angle = degrees.to_radians();
        Vec2::new(angle.sin(), angle.cos()) * distance
    }

    #[test]
    fn darts_score_by_the_ring_and_the_slice_they_land_in() {
        assert_eq!(hit(Vec2::ZERO), Hit::Bull);
        assert_eq!(hit(at(10.0, 45.0)), Hit::OuterBull);
        assert_eq!(hit(at(50.0, 0.0)), Hit::Single(20));
        assert_eq!(hit(at(103.0, 0.0)), Hit::Treble(20));
        assert_eq!(hit(at(166.0, 0.0)), Hit::Double(20));
        assert_eq!(hit(at(175.0, 0.0)), Hit::Miss);
        // Round the board: the 6 at three o'clock, the 3 at six, the 11 at nine.
        assert_eq!(hit(at(50.0, 90.0)), Hit::Single(6));
        assert_eq!(hit(at(50.0, 180.0)), Hit::Single(3));
        assert_eq!(hit(at(50.0, 270.0)), Hit::Single(11));
        // Either side of the 20's edges.
        assert_eq!(hit(at(50.0, 8.9)), Hit::Single(20));
        assert_eq!(hit(at(50.0, 9.1)), Hit::Single(1));
        assert_eq!(hit(at(50.0, -9.1)), Hit::Single(5));
        assert_eq!(Hit::Treble(20).points(), 60);
        assert_eq!(Hit::Double(16).name(), "D16");
        assert_eq!(Hit::OuterBull.points(), 25);
    }

    #[test]
    fn a_turn_scores_its_three_darts_then_passes() {
        let mut game = Game::new(0);
        assert_eq!(game.throw(Hit::Treble(20)), Outcome::Scored);
        assert_eq!(game.throw(Hit::Single(5)), Outcome::Scored);
        assert!(!game.turn_over());
        assert_eq!(game.throw(Hit::Miss), Outcome::Scored);
        assert!(game.turn_over());
        assert_eq!((game.remaining, game.turn_points()), ([236, 301], 65));
        game.next_turn();
        assert_eq!((game.turn, game.darts.len(), game.turn_start), (1, 0, 301));
    }

    #[test]
    fn going_below_zero_is_a_bust_and_exactly_zero_wins() {
        let mut game = Game::new(0);
        game.remaining = [40, 301];
        game.turn_start = 40;
        assert_eq!(game.throw(Hit::Single(20)), Outcome::Scored);
        // 20 left: a treble 20 goes below.
        assert_eq!(game.throw(Hit::Treble(20)), Outcome::Bust);
        assert!(game.turn_over());
        assert_eq!((game.remaining[0], game.turn_points()), (40, 0));
        game.next_turn();
        game.next_turn();
        assert_eq!(game.throw(Hit::Double(20)), Outcome::Won);
        assert_eq!(game.win, Some(0));
        assert!(game.turn_over());
    }

    #[test]
    fn finishing_on_a_double_when_asked() {
        let mut game = Game::new(0);
        game.double_out = true;
        game.remaining = [40, 301];
        game.turn_start = 40;
        // Zero, but not on a double.
        assert_eq!(game.throw(Hit::Single(20)), Outcome::Scored);
        assert_eq!(game.throw(Hit::Single(20)), Outcome::Bust);
        game.next_turn();
        game.next_turn();
        // Left on 1: no double makes that.
        assert_eq!(game.throw(Hit::Single(39)), Outcome::Bust);
        game.next_turn();
        game.next_turn();
        assert_eq!(game.throw(Hit::Double(20)), Outcome::Won);
    }

    #[test]
    fn holding_steadies_the_hand_for_a_while_then_it_shakes() {
        let sway = Hand::default();
        // The furthest the hand strays over two seconds, from `from`, holding since `since`.
        let furthest = |held: &dyn Fn(f32) -> f32| {
            (0..400)
                .map(|step| {
                    let t = 10.0 + step as f32 * 0.005;
                    sway.offset(t, held(t - 10.0)).length()
                })
                .fold(0.0, f32::max)
        };
        let loose = furthest(&|_| 0.0);
        let steady = furthest(&|t| 0.4 + t * 0.5);
        let shaking = furthest(&|t| 2.5 + t);
        assert!(
            loose > 0.6 * sway.drift && loose <= sway.drift * 1.3,
            "{loose}"
        );
        assert!(steady < loose * (sway.steady + 0.1), "{steady} {loose}");
        assert!(shaking > loose, "{shaking} {loose}");
    }

    #[test]
    fn the_right_flick_sends_the_dart_where_the_hand_has_it() {
        let hand = Hand::default();
        let ideal = Vec2::new(0.0, hand.flick_speed);
        assert_eq!(hand.flick_error(ideal), Vec2::ZERO);
        // Half as fast: half the spread low. Leaning right: right.
        assert_eq!(
            hand.flick_error(ideal * 0.5),
            Vec2::new(0.0, -hand.flick_spread / 2.0)
        );
        assert!(hand.flick_error(ideal * 1.2).y > 0.0);
        assert!(hand.flick_error(ideal + Vec2::X * 100.0).x > 0.0);
        // Hardly up the board, or down it: no throw.
        assert!(hand.throws(ideal * 0.5));
        assert!(!hand.throws(ideal * 0.1));
        assert!(!hand.throws(-ideal));
    }

    #[test]
    fn a_flick_is_as_fast_as_the_hand_was_going_when_it_let_go() {
        // Still for a while, then up at 900 millimetres a second, at 60 frames a second.
        let mut flick = Flick::new(0.0, Vec2::ZERO);
        let mut y = 0.0;
        for frame in 1..=40 {
            let now = frame as f32 / 60.0;
            if frame > 30 {
                y += 900.0 / 60.0;
            }
            flick.moved(now, Vec2::new(0.0, y));
        }
        let now = 40.0 / 60.0;
        assert!(
            (flick.velocity(now).y - 900.0).abs() < 1.0,
            "{}",
            flick.velocity(now)
        );
        // Stopped before letting go: nothing.
        for frame in 41..=50 {
            flick.moved(frame as f32 / 60.0, Vec2::new(0.0, y));
        }
        assert_eq!(flick.velocity(50.0 / 60.0), Vec2::ZERO);
    }

    #[test]
    fn darts_scatter_a_little_round_where_they_were_let_go() {
        let sway = Hand::default();
        let points: Vec<Vec2> = (0..400)
            .map(|i| {
                let first = (i as f32 + 0.5) / 400.0;
                let second = ((i * 7919) % 400) as f32 / 400.0;
                sway.scatter(first, second)
            })
            .collect();
        let middle = points.iter().sum::<Vec2>() / points.len() as f32;
        let within = points
            .iter()
            .filter(|point| point.length() <= sway.scatter)
            .count();
        assert!(middle.length() < 0.5, "{middle}");
        // Mostly within `scatter`, some further.
        assert!(within > 300 && within < 400, "{within}");
    }
}
