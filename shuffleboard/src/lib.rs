//! Table shuffleboard: weighted pucks slid down a long table with sand on it, towards the
//! scoring lines at the far end. Without Bevy, so it tests natively; the client draws it and
//! throws the pucks (client/src/shuffleboard).
//!
//! Units are canvas pixels. x runs across the table, 0 at its left edge; y runs along it, 0 at
//! the end the pucks are thrown from and LENGTH at the far end. A puck whose middle goes past
//! a side falls in the gutter; one whose middle goes past an end falls off it.

use glam::Vec2;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub const WIDTH: f32 = 48.0;
pub const LENGTH: f32 = 480.0;
pub const PUCK_RADIUS: f32 = 4.0;
/// Pucks are thrown from behind this line, and let go before it.
pub const FOUL_LINE: f32 = 62.0;
/// The lines across the far end, nearest first. A puck wholly past one scores 1, 2 or 3; a
/// puck touching a line scores the lower; a puck short of the first scores nothing, and comes
/// off the table once everything stops.
pub const LINES: [f32; 3] = [LENGTH - 120.0, LENGTH - 60.0, LENGTH - 24.0];
/// What a puck hanging over the far end scores.
pub const HANGER: u32 = 4;
pub const PUCKS_EACH: usize = 4;
pub const WINNING_SCORE: u32 = 15;
/// Physics advances in steps this long, so a fast puck can't pass through another.
pub const STEP: f32 = 1.0 / 480.0;
/// The hand's speed is measured over its moves in about this long before it lets go, each
/// counting less the longer ago it was, by half every `HAND_HALF_LIFE` (seconds).
pub const HAND_WINDOW: f32 = 0.3;
pub const HAND_HALF_LIFE: f32 = 0.15;
/// A hand that hasn't moved for this long has stopped (seconds).
pub const HAND_STILL_AFTER: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Settings {
    /// How quickly the sand slows a puck, in pixels per second per second, however fast it goes.
    pub friction: f32,
    /// How much harder the sand grips a faster puck: it slows it by this much of its speed
    /// every second, besides `friction`. So a puck thrown twice as fast goes about twice as
    /// far, not four times, as it would on `friction` alone.
    pub grip: f32,
    /// How much of their closing speed two pucks keep, bouncing apart.
    pub bounce: f32,
    /// The fastest a puck can be thrown, in pixels per second.
    pub max_speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            friction: 30.0,
            grip: 0.7,
            bounce: 0.85,
            max_speed: 580.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Puck {
    /// Whose it is, 0 or 1.
    pub player: usize,
    pub position: Vec2,
    pub velocity: Vec2,
}

impl Puck {
    pub fn points(&self) -> u32 {
        points(self.position)
    }
}

/// What a puck at `position`, on the table, scores.
pub fn points(position: Vec2) -> u32 {
    if position.y + PUCK_RADIUS > LENGTH {
        return HANGER;
    }
    LINES
        .iter()
        .filter(|&&line| position.y - PUCK_RADIUS >= line)
        .count() as u32
}

/// Where a puck went off the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fall {
    /// Over a side, into the gutter.
    Gutter,
    /// Over an end.
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fell {
    /// The puck as it went over.
    pub puck: Puck,
    pub over: Fall,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Table {
    pub pucks: Vec<Puck>,
    pub settings: Settings,
    /// Time not yet stepped through, less than a STEP.
    carry: f32,
}

impl Table {
    pub fn new(settings: Settings) -> Self {
        Self {
            pucks: Vec::new(),
            settings,
            carry: 0.0,
        }
    }

    /// Slides a puck of `player`'s from `position`, behind the foul line, at `velocity`, up to
    /// the top speed.
    pub fn throw(&mut self, player: usize, position: Vec2, velocity: Vec2) {
        let position = position.clamp(
            Vec2::splat(PUCK_RADIUS),
            Vec2::new(WIDTH - PUCK_RADIUS, FOUL_LINE),
        );
        let velocity = velocity.clamp_length_max(self.settings.max_speed);
        self.pucks.push(Puck {
            player,
            position,
            velocity,
        });
    }

    pub fn moving(&self) -> bool {
        self.pucks.iter().any(|puck| puck.velocity != Vec2::ZERO)
    }

    /// Moves everything on by `seconds`, in whole steps, and says which pucks went off the
    /// table.
    pub fn advance(&mut self, seconds: f32) -> Vec<Fell> {
        let mut fell = Vec::new();
        self.carry += seconds;
        while self.carry >= STEP {
            self.carry -= STEP;
            self.step(&mut fell);
        }
        fell
    }

    /// Moves everything on until it all stops.
    pub fn settle(&mut self) -> Vec<Fell> {
        let mut fell = Vec::new();
        while self.moving() {
            self.step(&mut fell);
        }
        self.carry = 0.0;
        fell
    }

    fn step(&mut self, fell: &mut Vec<Fell>) {
        let Settings { friction, grip, .. } = self.settings;
        for puck in &mut self.pucks {
            let speed = puck.velocity.length();
            let slowing = (friction + grip * speed) * STEP;
            puck.velocity = if speed <= slowing {
                Vec2::ZERO
            } else {
                puck.velocity * ((speed - slowing) / speed)
            };
            puck.position += puck.velocity * STEP;
        }
        for i in 0..self.pucks.len() {
            for j in i + 1..self.pucks.len() {
                self.collide(i, j);
            }
        }
        self.pucks.retain(|puck| {
            let Vec2 { x, y } = puck.position;
            let over = if !(0.0..=WIDTH).contains(&x) {
                Fall::Gutter
            } else if !(0.0..=LENGTH).contains(&y) {
                Fall::End
            } else {
                return true;
            };
            fell.push(Fell { puck: *puck, over });
            false
        });
    }

    /// Two pucks of the same weight that touch: they're pushed apart, and bounce if they're
    /// closing.
    fn collide(&mut self, i: usize, j: usize) {
        let (a, b) = (self.pucks[i], self.pucks[j]);
        let apart = b.position - a.position;
        let distance = apart.length();
        if distance >= 2.0 * PUCK_RADIUS || distance == 0.0 {
            return;
        }
        let normal = apart / distance;
        let overlap = 2.0 * PUCK_RADIUS - distance;
        self.pucks[i].position -= normal * overlap / 2.0;
        self.pucks[j].position += normal * overlap / 2.0;
        let closing = (b.velocity - a.velocity).dot(normal);
        if closing < 0.0 {
            let impulse = normal * (-(1.0 + self.settings.bounce) * closing / 2.0);
            self.pucks[i].velocity -= impulse;
            self.pucks[j].velocity += impulse;
        }
    }

    /// Takes the pucks short of the first line off the table, once everything has stopped,
    /// and gives them back.
    pub fn clear_short(&mut self) -> Vec<Puck> {
        let (scoring, short) = self.pucks.iter().partition(|puck| puck.points() > 0);
        self.pucks = scoring;
        short
    }

    /// The pucks that score the round as they lie, by their place in `pucks`, and whose they
    /// are: the player with the puck furthest down the table scores each of their pucks
    /// further than the other player's furthest.
    pub fn counted(&self) -> Option<(usize, Vec<usize>)> {
        let furthest = |player| {
            self.pucks
                .iter()
                .filter(|puck| puck.player == player && puck.points() > 0)
                .map(|puck| puck.position.y)
                .max_by(f32::total_cmp)
        };
        let (best, other) = match (furthest(0), furthest(1)) {
            (None, None) => return None,
            (Some(_), None) => (0, f32::MIN),
            (None, Some(_)) => (1, f32::MIN),
            (Some(a), Some(b)) if a > b => (0, b),
            (Some(a), Some(b)) if b > a => (1, a),
            // Level: neither is ahead.
            _ => return None,
        };
        let counted = (0..self.pucks.len())
            .filter(|&i| {
                let puck = &self.pucks[i];
                puck.player == best && puck.position.y > other && puck.points() > 0
            })
            .collect();
        Some((best, counted))
    }

    /// Who scores the round as the pucks lie, and how much (`counted`).
    pub fn round_score(&self) -> Option<(usize, u32)> {
        let (player, counted) = self.counted()?;
        let points = counted.iter().map(|&i| self.pucks[i].points()).sum();
        Some((player, points))
    }
}

/// Where the hand throwing a puck has been lately, for how fast it's going when it lets go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hand {
    /// When (seconds) and where (table pixels) it was, frame by frame, oldest first.
    seen: Vec<(f32, Vec2)>,
    /// When it last moved.
    moved_at: f32,
}

impl Hand {
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
        while self.seen.len() > 2 && now - self.seen[1].0 > HAND_WINDOW {
            self.seen.remove(0);
        }
    }

    /// How fast the hand is going, at the time `now`: its speed from frame to frame, averaged
    /// over the time each frame took, the more recent counting the more. Zero if it has stopped.
    pub fn velocity(&self, now: f32) -> Vec2 {
        if now - self.moved_at > HAND_STILL_AFTER {
            return Vec2::ZERO;
        }
        let (mut sum, mut weights) = (Vec2::ZERO, 0.0);
        for pair in self.seen.windows(2) {
            let ((from_at, from), (to_at, to)) = (pair[0], pair[1]);
            let took = to_at - from_at;
            if took <= 0.0 {
                continue;
            }
            // By how long it took, so a frame twice as long counts twice, and by how long ago.
            // A frame the mouse didn't report in (they report a few hundred times a second at
            // most) is made up for by the next, which sees twice the move.
            let ago = now - (from_at + to_at) / 2.0;
            let weight = took * 0.5f32.powf(ago / HAND_HALF_LIFE);
            sum += (to - from) / took * weight;
            weights += weight;
        }
        if weights > 0.0 {
            sum / weights
        } else {
            Vec2::ZERO
        }
    }
}

/// A game to 15, in rounds of four pucks each, thrown in turn.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Game {
    pub scores: [u32; 2],
    /// Who throws first this round; the other throws last, which is the better.
    pub first: usize,
    /// Pucks thrown this round.
    pub thrown: usize,
    /// The last round's scorer and what they scored, or None when nobody did.
    pub last: Option<(usize, u32)>,
    /// Who won, once someone has.
    pub win: Option<usize>,
}

impl Game {
    pub fn new(first: usize) -> Self {
        Self {
            scores: [0, 0],
            first,
            thrown: 0,
            last: None,
            win: None,
        }
    }

    /// Whose throw it is.
    pub fn turn(&self) -> usize {
        (self.first + self.thrown) % 2
    }

    /// How many pucks `player` has left to throw this round.
    pub fn left(&self, player: usize) -> usize {
        let theirs = if player == self.first {
            self.thrown.div_ceil(2)
        } else {
            self.thrown / 2
        };
        PUCKS_EACH - theirs
    }

    /// Every puck of the round has been thrown.
    pub fn round_over(&self) -> bool {
        self.thrown == 2 * PUCKS_EACH
    }

    /// The player whose turn it was has thrown.
    pub fn threw(&mut self) {
        self.thrown = (self.thrown + 1).min(2 * PUCKS_EACH);
    }

    /// Once the round's last puck stops: scores the round, clears the table and starts the
    /// next, which whoever scored throws first. Says who scored what.
    pub fn score_round(&mut self, table: &mut Table) -> Option<(usize, u32)> {
        let scored = table.round_score();
        if let Some((player, points)) = scored {
            self.scores[player] += points;
            self.first = player;
            if self.scores[player] >= WINNING_SCORE {
                self.win = Some(player);
            }
        }
        self.last = scored;
        self.thrown = 0;
        table.pucks.clear();
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(player: usize, x: f32, y: f32) -> Puck {
        Puck {
            player,
            position: Vec2::new(x, y),
            velocity: Vec2::ZERO,
        }
    }

    fn table(pucks: &[Puck]) -> Table {
        let mut table = Table::new(Settings::default());
        table.pucks = pucks.to_vec();
        table
    }

    /// How far a puck thrown at `speed` from the near end slides, on the default sand.
    fn slides(speed: f32) -> f32 {
        let mut table = table(&[]);
        table.throw(0, Vec2::new(24.0, 4.0), Vec2::new(0.0, speed));
        assert!(table.settle().is_empty(), "{speed} goes off the end");
        table.pucks[0].position.y - 4.0
    }

    #[test]
    fn the_sand_stops_a_puck_as_far_as_its_speed_takes_it() {
        let Settings { friction, grip, .. } = Settings::default();
        for speed in [150.0, 300.0, 400.0] {
            // Slowing by friction + grip × speed: v/k - (c/k²) ln(1 + kv/c), give or take a step.
            let expected =
                speed / grip - friction / (grip * grip) * (1.0 + grip * speed / friction).ln();
            let slid = slides(speed);
            assert!(
                (slid - expected).abs() < 1.0,
                "{speed}: {slid}, not {expected}"
            );
        }
    }

    #[test]
    fn twice_as_fast_goes_about_twice_as_far() {
        let ratio = slides(400.0) / slides(200.0);
        // On friction alone it would be four times.
        assert!((2.0..2.5).contains(&ratio), "{ratio}");
    }

    #[test]
    fn throws_start_behind_the_foul_line_and_no_faster_than_the_top_speed() {
        let mut table = table(&[]);
        table.throw(1, Vec2::new(-5.0, 90.0), Vec2::new(0.0, 9000.0));
        let puck = table.pucks[0];
        assert_eq!(puck.position, Vec2::new(PUCK_RADIUS, FOUL_LINE));
        assert!((puck.velocity.length() - Settings::default().max_speed).abs() < 1e-3);
    }

    #[test]
    fn a_puck_hit_full_on_takes_most_of_the_speed() {
        let mut table = table(&[at(1, 24.0, 200.0)]);
        table.throw(0, Vec2::new(24.0, 40.0), Vec2::new(0.0, 300.0));
        let mut hit = None;
        while table.moving() && hit.is_none() {
            table.advance(STEP);
            // The one struck went down first; the thrown one is after it.
            if table.pucks[0].velocity != Vec2::ZERO {
                hit = Some((table.pucks[1].velocity.y, table.pucks[0].velocity.y));
            }
        }
        let (thrower, struck) = hit.unwrap();
        assert!(struck > 4.0 * thrower.abs(), "{thrower} {struck}");
        table.settle();
        assert!(table.pucks[0].position.y > table.pucks[1].position.y + 2.0 * PUCK_RADIUS - 0.01);
    }

    #[test]
    fn pucks_fall_in_the_gutters_and_off_the_end() {
        let mut table = table(&[]);
        table.throw(0, Vec2::new(24.0, 20.0), Vec2::new(-60.0, 200.0));
        table.throw(1, Vec2::new(24.0, 20.0), Vec2::new(0.0, 520.0));
        // The second sits on the first to begin with: let them part.
        table.pucks[1].position.x = 40.0;
        let fell = table.settle();
        let overs: Vec<_> = fell
            .iter()
            .map(|fell| (fell.puck.player, fell.over))
            .collect();
        assert_eq!(overs, [(0, Fall::Gutter), (1, Fall::End)]);
        assert!(table.pucks.is_empty());
    }

    #[test]
    fn a_puck_scores_the_lines_it_is_wholly_past() {
        let [one, two, three] = LINES;
        let y = |y| Vec2::new(24.0, y);
        assert_eq!(points(y(one - 1.0)), 0);
        // Touching the line: the lower.
        assert_eq!(points(y(one + PUCK_RADIUS - 0.5)), 0);
        assert_eq!(points(y(one + PUCK_RADIUS)), 1);
        assert_eq!(points(y(two + PUCK_RADIUS + 1.0)), 2);
        assert_eq!(points(y(three + PUCK_RADIUS + 1.0)), 3);
        assert_eq!(points(y(LENGTH - PUCK_RADIUS)), 3);
        assert_eq!(points(y(LENGTH - 1.0)), HANGER);
    }

    #[test]
    fn pucks_short_of_the_first_line_come_off() {
        let mut table = table(&[at(0, 24.0, 200.0), at(1, 24.0, 400.0)]);
        let short = table.clear_short();
        assert_eq!(short, [at(0, 24.0, 200.0)]);
        assert_eq!(table.pucks, [at(1, 24.0, 400.0)]);
    }

    #[test]
    fn only_the_furthest_scores_for_each_puck_past_the_others_best() {
        let three = LENGTH - 10.0;
        let two = LENGTH - 40.0;
        let one = LENGTH - 100.0;
        // Red has the furthest and another past blue's best; red's third is behind it.
        let table = table(&[
            at(0, 10.0, three),
            at(1, 20.0, two),
            at(0, 30.0, two + 5.0),
            at(0, 40.0, one),
            at(1, 30.0, LENGTH - 1.0 - PUCK_RADIUS + 2.0),
        ]);
        // Blue's hanger is furthest of all, and blue's other puck is behind red's best.
        assert_eq!(table.round_score(), Some((1, HANGER)));
        let mut without_hanger = table.clone();
        without_hanger.pucks.pop();
        assert_eq!(without_hanger.round_score(), Some((0, 3 + 2)));
        assert_eq!(super::tests::table(&[]).round_score(), None);
        // Level: nobody.
        let level = super::tests::table(&[at(0, 10.0, two), at(1, 30.0, two)]);
        assert_eq!(level.round_score(), None);
    }

    #[test]
    fn turns_alternate_and_the_scorer_throws_first_next_round() {
        let mut game = Game::new(0);
        let mut table = table(&[]);
        let mut turns = Vec::new();
        while !game.round_over() {
            turns.push(game.turn());
            game.threw();
        }
        assert_eq!(turns, [0, 1, 0, 1, 0, 1, 0, 1]);
        assert_eq!((game.left(0), game.left(1)), (0, 0));
        table.pucks = vec![at(1, 24.0, LENGTH - 10.0), at(0, 24.0, LENGTH - 100.0)];
        assert_eq!(game.score_round(&mut table), Some((1, 3)));
        assert_eq!(game.scores, [0, 3]);
        assert_eq!((game.first, game.thrown, game.turn()), (1, 0, 1));
        assert!(table.pucks.is_empty());
        game.threw();
        assert_eq!((game.left(0), game.left(1)), (4, 3));
        // Nobody scores: the same player throws first again.
        while !game.round_over() {
            game.threw();
        }
        assert_eq!(game.score_round(&mut table), None);
        assert_eq!((game.first, game.last), (1, None));
    }

    /// A hand going up the table at `speed(t)` (pixels per second) from t = 0, seen in frames at
    /// `frames`, reported only when the mouse's own reports (every `report` seconds) say so.
    fn push(frames: &[f32], report: f32, speed: impl Fn(f32) -> f32) -> Hand {
        let mut hand = Hand::new(0.0, Vec2::ZERO);
        let (mut y, mut t, mut last_report) = (0.0, 0.0, 0.0);
        let mut reported_y = 0.0;
        let dt = 0.0005;
        for &frame in frames {
            while t + dt <= frame {
                t += dt;
                y += speed(t) * dt;
                if t - last_report >= report {
                    last_report = t;
                    reported_y = y;
                }
            }
            // What the frame sees: where the mouse last said it was.
            hand.moved(frame, Vec2::new(0.0, reported_y));
        }
        hand
    }

    #[test]
    fn a_steady_hand_is_measured_steadily_however_the_frames_fall() {
        // Uneven frames, about 60 a second, and a mouse reporting 125 times a second.
        let frames: Vec<f32> = (1..=30)
            .map(|i| i as f32 / 60.0 + if i % 3 == 0 { 0.006 } else { 0.0 })
            .collect();
        let hand = push(&frames, 0.008, |_| 300.0);
        let now = *frames.last().unwrap();
        let measured = hand.velocity(now).y;
        assert!((measured - 300.0).abs() < 300.0 * 0.06, "{measured}");
    }

    #[test]
    fn the_latest_moves_count_most() {
        // Speeding up steadily, from 100 to 400 pixels per second in half a second.
        let frames: Vec<f32> = (1..=30).map(|i| i as f32 / 60.0).collect();
        let hand = push(&frames, 0.004, |t| 100.0 + 600.0 * t);
        let now = 0.5;
        let measured = hand.velocity(now).y;
        // Faster than the average over the window: the end of it counts the most.
        let at_end = 400.0;
        let over_window = 100.0 + 600.0 * (0.5 - HAND_WINDOW / 2.0);
        assert!(measured > over_window && measured < at_end, "{measured}");
    }

    #[test]
    fn a_hand_that_stops_throws_nothing() {
        let frames: Vec<f32> = (1..=30).map(|i| i as f32 / 60.0).collect();
        let stopping = |t| if t < 0.3 { 300.0 } else { 0.0 };
        // Still going at 0.3 seconds, stopped by half a second.
        let going = push(&frames[..18], 0.004, stopping);
        assert!(going.velocity(0.3).y > 250.0);
        let stopped = push(&frames, 0.004, stopping);
        assert_eq!(stopped.velocity(0.5), Vec2::ZERO);
    }

    #[test]
    fn fifteen_wins() {
        let mut game = Game::new(0);
        game.scores = [13, 14];
        game.thrown = 2 * PUCKS_EACH;
        let mut table = table(&[at(0, 24.0, LENGTH - 10.0), at(0, 30.0, LENGTH - 40.0)]);
        assert_eq!(game.score_round(&mut table), Some((0, 5)));
        assert_eq!((game.scores, game.win), ([18, 14], Some(0)));
    }

    #[test]
    fn the_same_throws_end_the_same_however_the_time_is_cut() {
        let throws = |chunk: f32| {
            let mut table = table(&[at(1, 20.0, 380.0), at(0, 26.0, 395.0)]);
            table.throw(0, Vec2::new(22.0, 30.0), Vec2::new(1.5, 330.0));
            while table.moving() {
                table.advance(chunk);
            }
            table.pucks
        };
        assert_eq!(throws(1.0 / 60.0), throws(1.0 / 144.0));
    }
}
