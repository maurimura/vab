//! Air hockey physics on an upright rink, in rink pixels: x runs right across it, y down along
//! it, and (0, 0) is its top-left corner. Player 0 defends the goal at the bottom and player 1
//! the one at the top; each moves a paddle in their own half.
//!
//! Paddles go exactly where they're told, sliding there over the frame in small steps, so a
//! fast paddle can't pass through the puck. The puck glides, slowing down a little, bounces
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
        self.advance_scoring(targets, seconds, true)
    }

    /// As `advance`, but a puck reaching a goal only stops there, without scoring: for a rink
    /// that follows another one, which says when goals are scored.
    pub fn advance_following(&mut self, targets: [Vec2; 2], seconds: f32) -> Vec<Event> {
        self.advance_scoring(targets, seconds, false)
    }

    fn advance_scoring(&mut self, targets: [Vec2; 2], seconds: f32, scoring: bool) -> Vec<Event> {
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
            self.step(step, paddle_velocities, scoring, &mut events);
            if events.iter().any(|event| matches!(event, Event::Goal(_))) {
                // The puck has been served: the rest of the frame is for the paddles alone.
                self.paddles = to;
                break;
            }
        }
        events
    }

    fn step(
        &mut self,
        step: f32,
        paddle_velocities: [Vec2; 2],
        scoring: bool,
        events: &mut Vec<Event>,
    ) {
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
        if in_mouth && past_line && !scoring {
            self.puck.y = self.puck.y.clamp(-PUCK_RADIUS, HEIGHT + PUCK_RADIUS);
            self.velocity = Vec2::ZERO;
            return;
        }
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

    #[test]
    fn a_following_rink_holds_the_puck_in_a_goal_without_scoring() {
        let mut rink = rink_with(Vec2::new(WIDTH / 2.0, 40.0), Vec2::new(0.0, -300.0));
        let paddles = rink.paddles;
        let mut events = Vec::new();
        for _ in 0..30 {
            events.extend(rink.advance_following(paddles, 1.0 / 60.0));
        }
        assert!(!events.iter().any(|event| matches!(event, Event::Goal(_))));
        assert_eq!(rink.score, [0, 0]);
        assert!(rink.puck.y <= 0.0);
        assert_eq!(rink.velocity, Vec2::ZERO);
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
