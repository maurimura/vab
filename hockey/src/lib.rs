//! Air hockey physics on an upright rink, in rink pixels: x runs right across it, y down along
//! it, and (0, 0) is its top-left corner. Player 0 defends the goal at the bottom and player 1
//! the one at the top; each moves a paddle in their own half.
//!
//! Paddles go exactly where they're told, sliding there over the frame in small steps, so a
//! fast paddle can't pass through the puck. Between two players, the rink moves on a fixed frame
//! at a time from both their inputs (`play_frame`), the same way on both their machines, so
//! each can run the whole game (with rollback: client/src/hockey). The puck glides, slowing down a little, bounces
//! off the rails, and takes on a paddle's speed when one hits it, up to a top speed. A puck
//! that goes all the way into a goal scores, and is served to the player who let it in.

use glam::Vec2;

/// The rink inside the rails.
pub const WIDTH: f32 = 96.0;
pub const HEIGHT: f32 = 160.0;
pub const PUCK_RADIUS: f32 = 4.0;
pub const PADDLE_RADIUS: f32 = 7.0;
/// The gap in the middle of each end rail.
pub const GOAL_WIDTH: f32 = 32.0;
/// Goals that win the game.
pub const WINNING_SCORE: u32 = 7;
/// Seconds per step, at most: a frame is split into steps no longer than this.
pub const STEP: f32 = 1.0 / 480.0;
/// A frame of a game between two players, in seconds.
pub const FRAME: f32 = 1.0 / 60.0;
/// Input positions are in this many parts of a pixel.
const INPUT_SCALE: f32 = 64.0;
/// At most this much time is played in one go, after the tab was in the background.
const MAX_ADVANCE: f32 = 0.1;

/// Where the bot's paddle patrols, across the rink in front of its goal.
const BOT_LINE: f32 = 22.0;
/// How long the puck sits still on the bot's side before the bot goes and hits it.
const BOT_PATIENCE: f32 = 1.0;
/// Slower than this, the puck counts as still.
const STILL_SPEED: f32 = 10.0;

/// How the rink plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The puck's top speed, in pixels per second.
    pub max_speed: f32,
    /// How fast the gliding puck slows down, in pixels per second per second.
    pub friction: f32,
    /// How much of its speed the puck keeps off a rail, and off a paddle.
    pub rail_restitution: f32,
    pub paddle_restitution: f32,
    /// How fast the bot's paddle moves, in pixels per second.
    pub bot_speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_speed: 420.0,
            friction: 20.0,
            rail_restitution: 0.9,
            paddle_restitution: 0.9,
            bot_speed: 45.0,
        }
    }
}

/// Something that happened while time moved on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// This player scored.
    Goal(usize),
    /// A paddle hit the puck.
    Hit,
    /// The puck bounced off a rail.
    Rail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rink {
    pub puck: Vec2,
    /// In pixels per second.
    pub velocity: Vec2,
    /// Player 0's paddle (the bottom half), then player 1's (the top half).
    pub paddles: [Vec2; 2],
    pub score: [u32; 2],
    pub settings: Settings,
}

impl Default for Rink {
    fn default() -> Self {
        Self::new()
    }
}

impl Rink {
    /// A new game: the paddles by their goals, and the puck served to player 0.
    pub fn new() -> Self {
        Self {
            puck: serve_spot(0),
            velocity: Vec2::ZERO,
            paddles: [
                Vec2::new(WIDTH / 2.0, HEIGHT - BOT_LINE),
                Vec2::new(WIDTH / 2.0, BOT_LINE),
            ],
            score: [0, 0],
            settings: Settings::default(),
        }
    }

    /// The player who has won, once someone has.
    pub fn winner(&self) -> Option<usize> {
        self.score.iter().position(|&goals| goals >= WINNING_SCORE)
    }

    /// Where `player`'s paddle may be: their half, clear of the rails.
    pub fn half(player: usize) -> (Vec2, Vec2) {
        let low = Vec2::splat(PADDLE_RADIUS);
        let high = Vec2::new(WIDTH, HEIGHT) - PADDLE_RADIUS;
        if player == 0 {
            (Vec2::new(low.x, HEIGHT / 2.0 + PADDLE_RADIUS), high)
        } else {
            (low, Vec2::new(high.x, HEIGHT / 2.0 - PADDLE_RADIUS))
        }
    }

    /// Moves time on by `seconds`, the paddles going to `targets` (kept in their halves) on the
    /// way.
    pub fn advance(&mut self, targets: [Vec2; 2], seconds: f32) -> Vec<Event> {
        let mut events = Vec::new();
        let seconds = seconds.clamp(0.0, MAX_ADVANCE);
        if seconds == 0.0 || self.winner().is_some() {
            return events;
        }
        let steps = (seconds / STEP).ceil().max(1.0);
        let step = seconds / steps;
        let from = self.paddles;
        let to = [0, 1].map(|player| {
            let (low, high) = Self::half(player);
            targets[player].clamp(low, high)
        });
        for i in 1..=steps as u32 {
            let along = i as f32 / steps;
            let mut paddle_velocities = [Vec2::ZERO; 2];
            for player in 0..2 {
                let next = from[player].lerp(to[player], along);
                paddle_velocities[player] = (next - self.paddles[player]) / step;
                self.paddles[player] = next;
            }
            self.step(step, paddle_velocities, &mut events);
            if events.iter().any(|event| matches!(event, Event::Goal(_))) {
                // The puck has been served: the rest of the frame is for the paddles alone.
                self.paddles = to;
                break;
            }
        }
        events
    }

    fn step(&mut self, step: f32, paddle_velocities: [Vec2; 2], events: &mut Vec<Event>) {
        self.puck += self.velocity * step;
        let speed = self.velocity.length();
        let slowdown = self.settings.friction * step;
        self.velocity = if speed <= slowdown {
            Vec2::ZERO
        } else {
            self.velocity * ((speed - slowdown) / speed)
        };

        let touching = PUCK_RADIUS + PADDLE_RADIUS;
        for (paddle, paddle_velocity) in self.paddles.iter().zip(paddle_velocities) {
            let apart = self.puck - *paddle;
            let distance = apart.length();
            if distance >= touching {
                continue;
            }
            let normal = if distance > 0.0 {
                apart / distance
            } else {
                Vec2::Y
            };
            self.puck = *paddle + normal * touching;
            // The paddle doesn't give: the puck bounces off it as off a moving wall.
            let closing = (self.velocity - paddle_velocity).dot(normal);
            if closing < 0.0 {
                self.velocity -= normal * closing * (1.0 + self.settings.paddle_restitution);
                events.push(Event::Hit);
            }
        }

        // Into a goal: past the end rail, inside its mouth.
        let in_mouth = (self.puck.x - WIDTH / 2.0).abs() < GOAL_WIDTH / 2.0;
        let past_line = self.puck.y < -PUCK_RADIUS || self.puck.y > HEIGHT + PUCK_RADIUS;
        if in_mouth && past_line {
            let scorer = if self.puck.y < 0.0 { 0 } else { 1 };
            self.score[scorer] += 1;
            self.puck = serve_spot(1 - scorer);
            self.velocity = Vec2::ZERO;
            events.push(Event::Goal(scorer));
            return;
        }

        // The rails, last: wherever the puck got to, a paddle squeezing it into one included,
        // it's back on the rink, bouncing off if it was heading out.
        let bounce = -self.settings.rail_restitution;
        let mut railed = false;
        let mut hold = |at: &mut f32, speed: &mut f32, low: f32, high: f32| {
            let out = if *at < low {
                Some((low, *speed < 0.0))
            } else if *at > high {
                Some((high, *speed > 0.0))
            } else {
                None
            };
            if let Some((edge, heading_out)) = out {
                *at = edge;
                if heading_out {
                    *speed *= bounce;
                    railed = true;
                }
            }
        };
        hold(
            &mut self.puck.x,
            &mut self.velocity.x,
            PUCK_RADIUS,
            WIDTH - PUCK_RADIUS,
        );
        if !in_mouth {
            hold(
                &mut self.puck.y,
                &mut self.velocity.y,
                PUCK_RADIUS,
                HEIGHT - PUCK_RADIUS,
            );
        }
        if railed {
            events.push(Event::Rail);
        }
        self.velocity = self.velocity.clamp_length_max(self.settings.max_speed);
    }
}

impl Rink {
    /// One frame of a game between two players, from both their inputs (seat 0's, then seat
    /// 1's): the same inputs on the same rink always give the same rink. A player with no input
    /// yet leaves their paddle where it is. Once someone has won, player 1's new-game press
    /// starts the next game.
    pub fn play_frame(&mut self, inputs: [Input; 2]) -> Vec<Event> {
        if self.winner().is_some() {
            if inputs[0].new_game() {
                *self = Rink {
                    settings: self.settings,
                    ..Rink::new()
                };
            }
            return Vec::new();
        }
        let targets = [0, 1].map(|seat| inputs[seat].target().unwrap_or(self.paddles[seat]));
        self.advance(targets, FRAME)
    }

    /// A hash of everything about the rink, to check two players' rinks still match.
    pub fn checksum(&self) -> u64 {
        let rink = self;
        let numbers = [
            rink.puck.x,
            rink.puck.y,
            rink.velocity.x,
            rink.velocity.y,
            rink.paddles[0].x,
            rink.paddles[0].y,
            rink.paddles[1].x,
            rink.paddles[1].y,
        ];
        // FNV-1a over their bits and the score.
        let words = numbers
            .iter()
            .map(|number| number.to_bits())
            .chain(rink.score);
        words.fold(0xcbf2_9ce4_8422_2325, |hash: u64, word| {
            word.to_le_bytes().iter().fold(hash, |hash, &byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
            })
        })
    }
}

/// One player's input for a frame of a game between two players, as it goes between them:
/// where their paddle should be, as seat 0 sees the rink, in 64ths of a pixel, how far it moved
/// since their last input (so the other player's machine can guess where it goes next while
/// their next input is on its way), and what else they're doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Input {
    pub x: u16,
    pub y: u16,
    pub dx: i16,
    pub dy: i16,
    pub flags: u8,
}

impl Input {
    /// An input, as opposed to none yet (all zeros).
    const PRESENT: u8 = 1;
    /// Starts the next game, once someone has won (player 1's).
    const NEW_GAME: u8 = 2;

    /// How much of its movement a guessed paddle keeps from one frame to the next: it glides to
    /// a stop rather than on and on, as a real one would, more or less.
    const GUESS_KEEPS: i32 = 3;
    const GUESS_OUT_OF: i32 = 4;

    /// The paddle going to `target`, from `last` (the player's previous input, if any).
    pub fn new(target: Vec2, last: Option<Input>, new_game: bool) -> Self {
        let scaled = (target * INPUT_SCALE)
            .round()
            .clamp(Vec2::ZERO, Vec2::splat(f32::from(u16::MAX)));
        let (x, y) = (scaled.x as u16, scaled.y as u16);
        let moved = |now: u16, before: u16| {
            (i32::from(now) - i32::from(before)).clamp(i16::MIN.into(), i16::MAX.into()) as i16
        };
        let (dx, dy) = match last.filter(|last| last.target().is_some()) {
            Some(last) => (moved(x, last.x), moved(y, last.y)),
            None => (0, 0),
        };
        let new_game = if new_game { Self::NEW_GAME } else { 0 };
        Self {
            x,
            y,
            dx,
            dy,
            flags: Self::PRESENT | new_game,
        }
    }

    /// A guess at the next input from this one, while the real one is on its way: the paddle
    /// carries on as it was going, slowing down. Never a new-game press.
    pub fn guess_next(&self) -> Self {
        if self.target().is_none() {
            return *self;
        }
        let along =
            |at: u16, by: i16| (i32::from(at) + i32::from(by)).clamp(0, u16::MAX.into()) as u16;
        let slow = |by: i16| (i32::from(by) * Self::GUESS_KEEPS / Self::GUESS_OUT_OF) as i16;
        Self {
            x: along(self.x, self.dx),
            y: along(self.y, self.dy),
            dx: slow(self.dx),
            dy: slow(self.dy),
            flags: Self::PRESENT,
        }
    }

    /// Where the paddle should be; none before the player has said.
    pub fn target(&self) -> Option<Vec2> {
        (self.flags & Self::PRESENT != 0)
            .then(|| Vec2::new(f32::from(self.x), f32::from(self.y)) / INPUT_SCALE)
    }

    pub fn new_game(&self) -> bool {
        self.flags & Self::NEW_GAME != 0
    }
}

/// Where the puck is served to `player`: the middle of their half.
fn serve_spot(player: usize) -> Vec2 {
    let y = if player == 0 {
        HEIGHT * 3.0 / 4.0
    } else {
        HEIGHT / 4.0
    };
    Vec2::new(WIDTH / 2.0, y)
}

/// A simple opponent for player 1: it slides its paddle back and forth across its goal at a
/// steady pace, whatever the puck does, except that a puck sitting still on its side gets
/// fetched and hit, so the game never stalls.
#[derive(Clone, Debug, PartialEq)]
pub struct Bot {
    /// Which way it's sliding: 1 to the right, -1 to the left.
    way: f32,
    /// How long the puck has sat still on its side.
    still_for: f32,
}

impl Default for Bot {
    fn default() -> Self {
        Self {
            way: 1.0,
            still_for: 0.0,
        }
    }
}

impl Bot {
    /// Where the bot puts its paddle next, `seconds` from now.
    pub fn target(&mut self, rink: &Rink, seconds: f32) -> Vec2 {
        let paddle = rink.paddles[1];
        let speed = rink.settings.bot_speed;
        let on_my_side = rink.puck.y < HEIGHT / 2.0;
        if on_my_side && rink.velocity.length() < STILL_SPEED {
            self.still_for += seconds;
        } else {
            self.still_for = 0.0;
        }

        if self.still_for > BOT_PATIENCE {
            // Behind the puck (between it and the bot's goal), then through it, at twice the pace.
            let touching = PUCK_RADIUS + PADDLE_RADIUS;
            let behind = paddle.y < rink.puck.y - touching + 1.0
                && (paddle.x - rink.puck.x).abs() < PUCK_RADIUS;
            let goal = if behind {
                rink.puck + Vec2::Y * PADDLE_RADIUS * 2.0
            } else {
                rink.puck - Vec2::Y * (touching + 3.0)
            };
            return step_towards(paddle, goal, speed * 2.0 * seconds);
        }
        let (low, high) = Rink::half(1);
        if paddle.x >= high.x - 1.0 {
            self.way = -1.0;
        } else if paddle.x <= low.x + 1.0 {
            self.way = 1.0;
        }
        let goal = Vec2::new(paddle.x + self.way * speed * seconds, BOT_LINE);
        step_towards(paddle, goal, speed * seconds)
    }
}

/// From `from` towards `to`, at most `most` of the way.
fn step_towards(from: Vec2, to: Vec2, most: f32) -> Vec2 {
    from + (to - from).clamp_length_max(most)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rink with the puck at `at` going `velocity`, the paddles out of the way.
    fn rink_with(at: Vec2, velocity: Vec2) -> Rink {
        let mut rink = Rink::new();
        rink.puck = at;
        rink.velocity = velocity;
        rink.paddles = [
            Vec2::new(PADDLE_RADIUS, HEIGHT - PADDLE_RADIUS),
            Vec2::new(PADDLE_RADIUS, PADDLE_RADIUS),
        ];
        rink
    }

    fn wait(rink: &mut Rink, seconds: f32) -> Vec<Event> {
        let paddles = rink.paddles;
        let mut events = Vec::new();
        for _ in 0..(seconds * 60.0) as u32 {
            events.extend(rink.advance(paddles, 1.0 / 60.0));
        }
        events
    }

    #[test]
    fn the_puck_bounces_off_the_side_rails() {
        let mut rink = rink_with(Vec2::new(80.0, 80.0), Vec2::new(200.0, 0.0));
        let events = wait(&mut rink, 0.2);
        assert!(events.contains(&Event::Rail));
        assert!(rink.velocity.x < 0.0);
        assert!(rink.puck.x <= WIDTH - PUCK_RADIUS);
    }

    #[test]
    fn a_puck_into_the_top_goal_scores_for_player_0_and_is_served_to_player_1() {
        let mut rink = rink_with(Vec2::new(WIDTH / 2.0, 40.0), Vec2::new(0.0, -300.0));
        let events = wait(&mut rink, 0.5);
        assert!(events.contains(&Event::Goal(0)));
        assert_eq!(rink.score, [1, 0]);
        assert!(rink.puck.y < HEIGHT / 2.0);
        assert_eq!(rink.velocity, Vec2::ZERO);
    }

    #[test]
    fn the_end_rail_beside_the_goal_bounces() {
        let mut rink = rink_with(Vec2::new(10.0, 40.0), Vec2::new(0.0, -300.0));
        let events = wait(&mut rink, 0.3);
        assert!(!events.iter().any(|event| matches!(event, Event::Goal(_))));
        assert!(rink.velocity.y > 0.0);
    }

    #[test]
    fn a_moving_paddle_hits_the_puck_away_up_to_the_top_speed() {
        let mut rink = rink_with(Vec2::new(WIDTH / 2.0, 110.0), Vec2::ZERO);
        rink.paddles[0] = Vec2::new(WIDTH / 2.0, 140.0);
        // 30 pixels in a sixtieth of a second: 1800 pixels a second.
        let events = rink.advance([Vec2::new(WIDTH / 2.0, 110.0), rink.paddles[1]], 1.0 / 60.0);
        assert!(events.contains(&Event::Hit));
        assert!(rink.velocity.y < -100.0);
        assert!(rink.velocity.length() <= rink.settings.max_speed + 1e-3);
    }

    #[test]
    fn a_fast_paddle_does_not_pass_through_the_puck() {
        let mut rink = rink_with(Vec2::new(WIDTH / 2.0, 120.0), Vec2::ZERO);
        rink.paddles[0] = Vec2::new(WIDTH / 2.0, HEIGHT - PADDLE_RADIUS);
        // Across the whole half, past the puck, in one frame.
        rink.advance([Vec2::new(WIDTH / 2.0, 0.0), rink.paddles[1]], 1.0 / 60.0);
        assert!(rink.velocity.y < 0.0);
        assert!(rink.puck.y < rink.paddles[0].y);
    }

    #[test]
    fn paddles_stay_in_their_own_half() {
        let mut rink = Rink::new();
        rink.advance(
            [Vec2::new(-50.0, 0.0), Vec2::new(500.0, HEIGHT)],
            1.0 / 60.0,
        );
        assert!(rink.paddles[0].y >= HEIGHT / 2.0 + PADDLE_RADIUS);
        assert!(rink.paddles[0].x >= PADDLE_RADIUS);
        assert!(rink.paddles[1].y <= HEIGHT / 2.0 - PADDLE_RADIUS);
        assert!(rink.paddles[1].x <= WIDTH - PADDLE_RADIUS);
    }

    #[test]
    fn a_puck_squeezed_into_a_corner_stays_on_the_rink() {
        let mut rink = rink_with(Vec2::new(6.0, HEIGHT - 6.0), Vec2::ZERO);
        rink.paddles[0] = Vec2::new(30.0, HEIGHT - 30.0);
        // Shoving the paddle into the corner, again and again, from different sides.
        for i in 0..240 {
            let target = if i % 20 < 10 {
                Vec2::new(0.0, HEIGHT)
            } else {
                Vec2::new(30.0, HEIGHT - 30.0)
            };
            rink.advance([target, rink.paddles[1]], 1.0 / 60.0);
            let Vec2 { x, y } = rink.puck;
            let in_mouth = (x - WIDTH / 2.0).abs() < GOAL_WIDTH / 2.0;
            assert!(
                (PUCK_RADIUS..=WIDTH - PUCK_RADIUS).contains(&x),
                "{:?}",
                rink.puck
            );
            assert!(
                in_mouth || (PUCK_RADIUS..=HEIGHT - PUCK_RADIUS).contains(&y),
                "{:?}",
                rink.puck
            );
        }
    }

    /// A long, busy game between two made-up players, a frame at a time: their paddles sweep
    /// across their halves at different paces, hitting the puck about.
    fn busy_inputs(frame: u32) -> [Input; 2] {
        let t = frame as f32 / 60.0;
        let x = |pace: f32| WIDTH / 2.0 + (t * pace).sin() * (WIDTH / 2.0 - PADDLE_RADIUS);
        let bottom = Vec2::new(x(2.3), HEIGHT * 0.75 + (t * 3.1).sin() * 30.0);
        let top = Vec2::new(x(1.7), HEIGHT * 0.25 + (t * 2.6).cos() * 30.0);
        [
            Input::new(bottom, None, false),
            Input::new(top, None, false),
        ]
    }

    #[test]
    fn the_same_inputs_play_out_the_same_way() {
        let (mut a, mut b) = (Rink::new(), Rink::new());
        for frame in 0..3600 {
            a.play_frame(busy_inputs(frame));
            b.play_frame(busy_inputs(frame));
        }
        assert_eq!(a, b);
        assert_eq!(a.checksum(), b.checksum());
        // And it was a game: the puck got around.
        assert_ne!(a.checksum(), Rink::new().checksum());
    }

    #[test]
    fn a_rink_saved_and_played_on_again_matches_one_that_never_stopped() {
        // What rollback does: keep a copy, play on, go back to the copy, play the same again.
        let mut rink = Rink::new();
        for frame in 0..600 {
            rink.play_frame(busy_inputs(frame));
        }
        let saved = rink.clone();
        for frame in 600..1200 {
            rink.play_frame(busy_inputs(frame));
        }
        let mut again = saved;
        for frame in 600..1200 {
            again.play_frame(busy_inputs(frame));
        }
        assert_eq!(again, rink);
    }

    #[test]
    fn inputs_carry_the_paddle_and_the_new_game_press() {
        let input = Input::new(Vec2::new(48.25, 140.5), None, true);
        assert_eq!(input.target(), Some(Vec2::new(48.25, 140.5)));
        assert!(input.new_game());
        assert_eq!(Input::default().target(), None);

        // A guess carries a moving paddle on, slowing down, and never presses new game.
        let before = Input::new(Vec2::new(40.0, 140.0), None, false);
        let moving = Input::new(Vec2::new(44.0, 138.0), Some(before), true);
        let guess = moving.guess_next();
        assert_eq!(guess.target(), Some(Vec2::new(48.0, 136.0)));
        assert!(!guess.new_game());
        let further = guess.guess_next().target().unwrap();
        assert_eq!(further, Vec2::new(51.0, 134.5));

        // A paddle with no input yet stays put; player 1's press starts the next game.
        let mut rink = Rink::new();
        let paddles = rink.paddles;
        rink.play_frame([Input::default(); 2]);
        assert_eq!(rink.paddles, paddles);
        rink.score = [7, 2];
        rink.play_frame([Input::new(paddles[0], None, true), Input::default()]);
        assert_eq!(rink.score, [0, 0]);
    }

    #[test]
    fn seven_goals_win() {
        let mut rink = Rink::new();
        rink.score = [6, 3];
        assert_eq!(rink.winner(), None);
        rink.score[0] = 7;
        assert_eq!(rink.winner(), Some(0));
    }

    #[test]
    fn the_bot_patrols_its_goal_and_fetches_a_still_puck() {
        let mut rink = rink_with(Vec2::new(WIDTH / 2.0, 120.0), Vec2::ZERO);
        rink.paddles[1] = Vec2::new(WIDTH / 2.0, BOT_LINE);
        let mut bot = Bot::default();
        let mut xs = Vec::new();
        for _ in 0..240 {
            let target = bot.target(&rink, 1.0 / 60.0);
            rink.advance([rink.paddles[0], target], 1.0 / 60.0);
            xs.push(rink.paddles[1].x);
            assert!((rink.paddles[1].y - BOT_LINE).abs() < 1e-3);
        }
        let (low, high) = xs
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(high - low > 40.0, "it slides across: {low}..{high}");

        // A puck left still on the bot's side gets hit back towards player 0.
        let mut rink = rink_with(Vec2::new(30.0, 50.0), Vec2::ZERO);
        rink.paddles[1] = Vec2::new(WIDTH / 2.0, BOT_LINE);
        let mut bot = Bot::default();
        for _ in 0..600 {
            let target = bot.target(&rink, 1.0 / 60.0);
            rink.advance([rink.paddles[0], target], 1.0 / 60.0);
            if rink.puck.y > HEIGHT / 2.0 {
                break;
            }
        }
        assert!(rink.puck.y > HEIGHT / 2.0, "{:?}", rink.puck);
    }
}
