//! Pinball physics, without Bevy, so it tests natively; the client draws it (client/src/pinball).
//!
//! A table comes from its layout (layout.rs): the same shapes its art is drawn from. Units are
//! inches on the playfield, x across and y down the table toward the player; the playfield slopes
//! down toward the player, so the ball rolls that way. The ball is a 1 1/16" steel ball, moving in
//! small fixed steps so it can't pass through anything, and bouncing off what it meets:
//! - walls, posts and rubber rings (rubber bounces more), and solid blocks (the bar, the piano);
//! - the flippers, which swing up while their button is held and carry the ball with them;
//! - slingshots and pop bumpers, which kick it away;
//! - drop targets, which fall when hit, and standup targets, which don't;
//! - a captive ball in its lane, which the ball knocks up into the bull at the top;
//! - a one-way gate at the top of the shooter lane.
//!
//! It rolls over rollovers and through the swinging doors (which slow it a little), up the ramp
//! if it's going fast enough (and back down if not), into the scoop if it's going slowly enough
//! (which holds it, then kicks it out), and off the bottom into the drain. The plunger launches
//! it up the shooter lane. What it does along the way comes out as events.

pub mod camera;
pub mod layout;

use glam::{Vec2, Vec3};

pub use camera::Camera;
pub use layout::{Layout, Side};

/// The ball's radius, in inches.
pub const BALL_RADIUS: f32 = 0.53;
/// Physics advances in steps this long: a fast ball moves less than its own size in one.
pub const STEP: f32 = 1.0 / 2000.0;
/// Gravity, in inches per second per second, and the playfield's slope (degrees).
pub const GRAVITY: f32 = 386.1;
pub const SLOPE_DEGREES: f32 = 6.5;

/// How the table plays: tunable (`/settings`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// How hard the slope pulls the ball toward the player (inches per second per second).
    pub slope: f32,
    /// How fast the flippers swing up, and back down (radians per second).
    pub flipper_speed: f32,
    pub flipper_return: f32,
    /// How much speed a bounce keeps, off a flipper, off wood and steel, and off rubber.
    pub flipper_bounce: f32,
    pub wall_bounce: f32,
    pub rubber_bounce: f32,
    /// How hard pop bumpers and slingshots kick the ball (inches per second).
    pub bumper_kick: f32,
    pub sling_kick: f32,
    /// How fast the plunger launches the ball, pulled all the way back.
    pub plunger_speed: f32,
    /// How fast a ball must go into the ramp to make it to the top (inches per second): it's
    /// steep, and rough going.
    pub ramp_speed: f32,
    /// The ball never goes faster than this.
    pub max_speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            slope: GRAVITY * SLOPE_DEGREES.to_radians().sin(),
            flipper_speed: 38.0,
            flipper_return: 22.0,
            flipper_bounce: 0.5,
            wall_bounce: 0.45,
            rubber_bounce: 0.72,
            bumper_kick: 210.0,
            sling_kick: 165.0,
            plunger_speed: 470.0,
            ramp_speed: 170.0,
            max_speed: 900.0,
        }
    }
}

/// What happened, for the rules, the lamps and the sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Bumper(usize),
    Sling(usize),
    DropTarget(usize),
    Standup(usize),
    Rollover(usize),
    /// Through the swinging doors: going in (up the table), or coming out.
    Doors {
        inward: bool,
    },
    /// Into the scoop, and kicked out of it.
    Scoop,
    ScoopKick,
    /// Up the ramp: made it to the top (into the mine), or rolled back down.
    RampMade,
    RampBack,
    /// Out of the mine again, onto the playfield.
    MineKick,
    /// The captive ball hit the bull.
    Bull,
    /// The plunger let go of a ball.
    Launch,
    /// A ball went down the drain: `Table::balls` no longer has it.
    Drain,
}

/// Where a ball is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// Rolling on the playfield.
    Playfield,
    /// On the ramp, this far along it (inches), going this fast up it (negative: back down).
    Ramp {
        along: f32,
        speed: f32,
    },
    /// Held in the scoop, or in the mine at the top of the ramp, for this long so far.
    Scoop {
        held: f32,
    },
    Mine {
        held: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub position: Vec2,
    pub velocity: Vec2,
    pub place: Place,
    /// Over a rollover (by its number) or the doors' line, last step: entering one is an event.
    over: Option<usize>,
}

impl Ball {
    pub fn new(position: Vec2) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
            place: Place::Playfield,
            over: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flipper {
    pub side: Side,
    pub pivot: Vec2,
    /// Radians, y down the table.
    pub angle: f32,
    pub angular_velocity: f32,
    rest: f32,
    up: f32,
    pub pressed: bool,
}

impl Flipper {
    /// How far up it is, from 0 (at rest) to 1 (all the way up).
    pub fn raised(&self) -> f32 {
        ((self.angle - self.rest) / (self.up - self.rest)).clamp(0.0, 1.0)
    }
}

/// Something the ball bounces off, standing still.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// A line from `a` to `b`, `r` thick each side, with round ends.
    Segment {
        a: Vec2,
        b: Vec2,
        r: f32,
    },
    Circle {
        c: Vec2,
        r: f32,
    },
}

impl Shape {
    /// The point on the shape's middle nearest `p`, and its thickness there.
    fn nearest(&self, p: Vec2) -> (Vec2, f32) {
        match *self {
            Shape::Segment { a, b, r } => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                (a + ab * t, r)
            }
            Shape::Circle { c, r } => (c, r),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Wall,
    Rubber,
    Bumper(usize),
    Sling(usize),
    DropTarget(usize),
    Standup(usize),
}

#[derive(Clone, Copy, Debug)]
struct Collider {
    shape: Shape,
    kind: Kind,
}

/// The ramp's middle, measured along: each point with how far along it is.
#[derive(Clone, Debug)]
struct RampPath {
    points: Vec<(Vec3, f32)>,
    half_width: f32,
}

impl RampPath {
    fn length(&self) -> f32 {
        self.points.last().map_or(0.0, |(_, s)| *s)
    }

    /// Where the ramp is `along` inches from its mouth, and which way it goes there.
    fn at(&self, along: f32) -> (Vec3, Vec3) {
        let i = self
            .points
            .windows(2)
            .position(|w| w[1].1 >= along)
            .unwrap_or(self.points.len().saturating_sub(2));
        let ((a, sa), (b, sb)) = (self.points[i], self.points[i + 1]);
        let t = ((along - sa) / (sb - sa).max(1e-6)).clamp(0.0, 1.0);
        (a.lerp(b, t), (b - a).normalize_or_zero())
    }

    fn mouth(&self) -> (Vec2, Vec2) {
        let (p, d) = self.at(0.0);
        (p.truncate(), d.truncate().normalize_or_zero())
    }
}

/// How long the scoop and the mine hold a ball before kicking it out (seconds).
const SCOOP_HOLD: f32 = 1.0;
const MINE_HOLD: f32 = 0.8;
/// The scoop catches a ball going slower than this (inches per second); a faster one skims over.
const SCOOP_CATCH: f32 = 160.0;
/// The scoop kicks the ball out toward the flippers; the mine drops it out toward the bumpers.
const SCOOP_KICK: Vec2 = Vec2::new(-95.0, 82.0);
const MINE_DROP: Vec2 = Vec2::new(30.0, 70.0);
/// Rolling back down the ramp, the ball speeds up at this rate (inches per second per second).
const RAMP_BACK: f32 = 120.0;
/// The plunger pulls back at this rate (inches per second) while held.
const PLUNGER_PULL: f32 = 1.4;
/// A rollover counts the ball within this of its middle.
const ROLLOVER_REACH: f32 = 0.45;
/// The ball just touching something doesn't bounce (so it can rest on a flipper).
const RESTING: f32 = 12.0;
/// A slingshot or bumper hit slower than this doesn't fire.
const KICK_THRESHOLD: f32 = 25.0;

#[derive(Clone, Debug)]
pub struct Table {
    pub settings: Settings,
    colliders: Vec<Collider>,
    ramp: RampPath,
    layout: Layout,
    /// The balls in play; the captive ball is separate.
    pub balls: Vec<Ball>,
    pub captive: Ball,
    pub flippers: [Flipper; 2],
    /// Which drop targets are down.
    pub drops_down: Vec<bool>,
    /// The doors' swing (degrees, positive: pushed in, up the table) and how fast it's swinging.
    pub doors_angle: f32,
    doors_speed: f32,
    /// How far the plunger is pulled back (inches), and whether it's being pulled.
    pub plunger: f32,
    plunger_held: bool,
    /// Seconds since each of these last fired, for the drawing: bumpers, slingshots, the bull,
    /// and the standups (which wobble when hit).
    pub bumper_flash: Vec<f32>,
    pub sling_flash: [f32; 2],
    pub bull_flash: f32,
    pub standup_flash: Vec<f32>,
    carry: f32,
}

impl Table {
    pub fn new(layout: Layout, settings: Settings) -> Self {
        let mut colliders = Vec::new();
        for wall in &layout.walls {
            for pair in wall.points.windows(2) {
                segment(
                    &mut colliders,
                    pair[0],
                    pair[1],
                    wall.width / 2.0,
                    Kind::Wall,
                );
            }
        }
        for block in &layout.blocks {
            let n = block.points.len();
            for i in 0..n {
                segment(
                    &mut colliders,
                    block.points[i],
                    block.points[(i + 1) % n],
                    0.02,
                    Kind::Wall,
                );
            }
        }
        // Rubber rings: round one post, a circle; round several, the lines between them (whose
        // round ends are the posts). A slingshot's kicking side kicks.
        for ring in &layout.rubbers {
            let n = ring.len();
            if n == 1 {
                colliders.push(Collider {
                    shape: Shape::Circle {
                        c: ring[0].center(),
                        r: ring[0].r,
                    },
                    kind: Kind::Rubber,
                });
                continue;
            }
            let pairs = if n == 2 { 1 } else { n };
            for k in 0..pairs {
                let (a, b) = (ring[k], ring[(k + 1) % n]);
                let kind = layout
                    .slings
                    .iter()
                    .position(|s| {
                        let same = |p: [f32; 2], c: layout::Circle| {
                            (Vec2::from(p) - c.center()).length() < 0.01
                        };
                        (same(s.top, a) && same(s.bottom, b))
                            || (same(s.top, b) && same(s.bottom, a))
                    })
                    .map_or(Kind::Rubber, Kind::Sling);
                segment(&mut colliders, [a.x, a.y], [b.x, b.y], a.r.min(b.r), kind);
            }
        }
        for (i, bumper) in layout.bumpers.iter().enumerate() {
            colliders.push(Collider {
                shape: Shape::Circle {
                    c: bumper.center(),
                    r: bumper.r,
                },
                kind: Kind::Bumper(i),
            });
        }
        let face = |t: &layout::Target| {
            let c = Vec2::new(t.x, t.y);
            let along = Vec2::from(t.along) * t.half_width;
            (c - along, c + along)
        };
        for (i, t) in layout.drop_targets.iter().enumerate() {
            let (a, b) = face(t);
            segment(
                &mut colliders,
                a.into(),
                b.into(),
                0.08,
                Kind::DropTarget(i),
            );
        }
        for (i, t) in layout.standups.iter().enumerate() {
            let (a, b) = face(t);
            segment(&mut colliders, a.into(), b.into(), 0.08, Kind::Standup(i));
        }

        // The ramp: measured along, and walled at its low end, where it's still too low to roll
        // under (its mouth is open; a ball going in is on it).
        let mut points = Vec::new();
        let mut along = 0.0;
        for (i, p) in layout.ramp.path.iter().enumerate() {
            let p = Vec3::from(*p);
            if i > 0 {
                along += (p - points.last().map_or(p, |(q, _): &(Vec3, f32)| *q)).length();
            }
            points.push((p, along));
        }
        let ramp = RampPath {
            points,
            half_width: layout.ramp.half_width,
        };
        let low: Vec<(Vec3, Vec3)> = ramp
            .points
            .iter()
            .take_while(|(p, _)| p.z < 2.0 * BALL_RADIUS)
            .map(|(p, s)| (*p, ramp.at(*s).1))
            .collect();
        for side in [-1.0, 1.0] {
            for pair in low.windows(2) {
                let edge = |(p, d): (Vec3, Vec3)| {
                    let d = d.truncate().normalize_or_zero();
                    (p.truncate() + Vec2::new(-d.y, d.x) * side * ramp.half_width).to_array()
                };
                segment(
                    &mut colliders,
                    edge(pair[0]),
                    edge(pair[1]),
                    0.06,
                    Kind::Wall,
                );
            }
        }
        if let Some(&(p, d)) = low.last() {
            let d = d.truncate().normalize_or_zero();
            let across = Vec2::new(-d.y, d.x) * ramp.half_width;
            let p = p.truncate();
            segment(
                &mut colliders,
                (p - across).into(),
                (p + across).into(),
                0.06,
                Kind::Wall,
            );
        }

        let flipper = |place: &layout::FlipperPlace| {
            let (rest, up) = match place.side {
                Side::Left => (layout.flipper.rest, layout.flipper.up),
                Side::Right => (180.0 - layout.flipper.rest, 180.0 - layout.flipper.up),
            };
            Flipper {
                side: place.side,
                pivot: Vec2::from(place.pivot),
                angle: rest.to_radians(),
                angular_velocity: 0.0,
                rest: rest.to_radians(),
                up: up.to_radians(),
                pressed: false,
            }
        };
        let flippers = [flipper(&layout.flippers[0]), flipper(&layout.flippers[1])];
        let captive = Ball::new(Vec2::new(layout.captive.x, layout.captive.y));
        Self {
            settings,
            colliders,
            ramp,
            balls: Vec::new(),
            captive,
            flippers,
            drops_down: vec![false; layout.drop_targets.len()],
            doors_angle: 0.0,
            doors_speed: 0.0,
            plunger: 0.0,
            plunger_held: false,
            bumper_flash: vec![f32::INFINITY; layout.bumpers.len()],
            sling_flash: [f32::INFINITY; 2],
            bull_flash: f32::INFINITY,
            standup_flash: vec![f32::INFINITY; layout.standups.len()],
            layout,
            carry: 0.0,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// A new ball, resting on the plunger.
    pub fn serve(&mut self) {
        let p = self.layout.plunger;
        self.balls
            .push(Ball::new(Vec2::new(p.x, p.y + self.plunger - BALL_RADIUS)));
    }

    pub fn flip(&mut self, side: Side, pressed: bool) {
        for flipper in &mut self.flippers {
            if flipper.side == side {
                flipper.pressed = pressed;
            }
        }
    }

    /// Holding the plunger pulls it back; letting go launches whatever ball rests on it.
    pub fn hold_plunger(&mut self, held: bool) {
        self.plunger_held = held;
    }

    /// Stands every drop target up again.
    pub fn reset_drop_targets(&mut self) {
        self.drops_down.fill(false);
    }

    /// The ball's height above the playfield, for drawing: on the ramp, up it; in the scoop,
    /// sunk into it; in the mine, out of sight.
    pub fn height(&self, ball: &Ball) -> f32 {
        match ball.place {
            Place::Playfield => BALL_RADIUS,
            Place::Ramp { along, .. } => self.ramp.at(along).0.z + BALL_RADIUS,
            Place::Scoop { .. } => BALL_RADIUS * 0.3,
            Place::Mine { .. } => -10.0,
        }
    }

    /// Where the ball is on the playfield (on the ramp: under the ramp's middle there).
    pub fn ball_at(&self, ball: &Ball) -> Vec3 {
        let flat = match ball.place {
            Place::Ramp { along, .. } => self.ramp.at(along).0.truncate(),
            _ => ball.position,
        };
        flat.extend(self.height(ball))
    }

    /// Moves everything on by `seconds`, in whole steps, and says what happened.
    pub fn advance(&mut self, seconds: f32) -> Vec<Event> {
        let mut events = Vec::new();
        self.carry = (self.carry + seconds).min(0.1);
        while self.carry >= STEP {
            self.carry -= STEP;
            self.step(&mut events);
        }
        let dt = seconds;
        for t in self
            .bumper_flash
            .iter_mut()
            .chain(self.sling_flash.iter_mut())
            .chain(std::iter::once(&mut self.bull_flash))
            .chain(self.standup_flash.iter_mut())
        {
            *t += dt;
        }
        events
    }

    fn step(&mut self, events: &mut Vec<Event>) {
        let dt = STEP;
        self.move_flippers(dt);
        self.move_plunger(dt, events);
        self.swing_doors(dt);
        let mut balls = std::mem::take(&mut self.balls);
        for ball in &mut balls {
            self.step_ball(ball, dt, events);
        }
        // Balls knock each other, and the captive ball.
        let mut captive = self.captive;
        for i in 0..balls.len() {
            if balls[i].place != Place::Playfield {
                continue;
            }
            for j in i + 1..balls.len() {
                if balls[j].place == Place::Playfield {
                    let (left, right) = balls.split_at_mut(j);
                    knock(&mut left[i], &mut right[0]);
                }
            }
            knock(&mut balls[i], &mut captive);
        }
        self.captive = captive;
        self.step_captive(dt, events);
        let drain = self.layout.drain.y;
        let shooter = self.layout.gate.x0;
        balls.retain(|ball| {
            let gone = ball.place == Place::Playfield
                && ball.position.y > drain
                && ball.position.x < shooter;
            if gone {
                events.push(Event::Drain);
            }
            !gone
        });
        self.balls = balls;
    }

    fn move_flippers(&mut self, dt: f32) {
        let Settings {
            flipper_speed,
            flipper_return,
            ..
        } = self.settings;
        for flipper in &mut self.flippers {
            let (target, speed) = if flipper.pressed {
                (flipper.up, flipper_speed)
            } else {
                (flipper.rest, flipper_return)
            };
            let gap = target - flipper.angle;
            let step = speed * dt;
            if gap.abs() <= step {
                flipper.angle = target;
                flipper.angular_velocity = 0.0;
            } else {
                let way = gap.signum();
                flipper.angle += way * step;
                flipper.angular_velocity = way * speed;
            }
        }
    }

    fn move_plunger(&mut self, dt: f32, events: &mut Vec<Event>) {
        let p = self.layout.plunger;
        if self.plunger_held {
            self.plunger = (self.plunger + PLUNGER_PULL * dt).min(p.max_pull);
            return;
        }
        if self.plunger <= 0.0 {
            return;
        }
        // Let go: whatever rests on the tip flies, as fast as it was pulled back.
        let tip = p.y + self.plunger;
        let speed = self.settings.plunger_speed * (self.plunger / p.max_pull);
        for ball in &mut self.balls {
            let on_tip = ball.place == Place::Playfield
                && (ball.position.x - p.x).abs() < 0.5
                && (tip - BALL_RADIUS - ball.position.y).abs() < 0.12;
            if on_tip {
                ball.velocity = Vec2::new(0.0, -speed);
                events.push(Event::Launch);
            }
        }
        self.plunger = 0.0;
    }

    /// The doors swing back to hanging straight, the way a ball pushed them.
    fn swing_doors(&mut self, dt: f32) {
        let spring = 260.0;
        let damping = 9.0;
        self.doors_speed += (-spring * self.doors_angle - damping * self.doors_speed) * dt;
        self.doors_angle = (self.doors_angle + self.doors_speed * dt).clamp(-60.0, 60.0);
    }

    fn step_ball(&mut self, ball: &mut Ball, dt: f32, events: &mut Vec<Event>) {
        match ball.place {
            Place::Playfield => self.roll(ball, dt, events),
            Place::Ramp { along, speed } => self.ride_ramp(ball, along, speed, dt, events),
            Place::Scoop { held } => {
                let held = held + dt;
                ball.place = Place::Scoop { held };
                if held >= SCOOP_HOLD {
                    let s = self.layout.scoop;
                    ball.place = Place::Playfield;
                    ball.position =
                        s.center() + SCOOP_KICK.normalize() * (s.r + BALL_RADIUS + 0.05);
                    ball.velocity = SCOOP_KICK;
                    events.push(Event::ScoopKick);
                }
            }
            Place::Mine { held } => {
                let held = held + dt;
                ball.place = Place::Mine { held };
                if held >= MINE_HOLD {
                    ball.place = Place::Playfield;
                    ball.position = Vec2::from(self.layout.mine_exit);
                    ball.velocity = MINE_DROP;
                    events.push(Event::MineKick);
                }
            }
        }
    }

    /// The ball on the playfield: rolls, bounces off what it meets, and goes onto the ramp or
    /// into the scoop.
    fn roll(&mut self, ball: &mut Ball, dt: f32, events: &mut Vec<Event>) {
        let before = ball.position;
        ball.velocity.y += self.settings.slope * dt;
        ball.velocity = ball.velocity.clamp_length_max(self.settings.max_speed);
        ball.position += ball.velocity * dt;
        self.collide_still(ball, events);
        self.collide_flippers(ball);
        self.collide_gate(ball, before);
        self.collide_plunger(ball);
        self.cross_doors(ball, before, events);
        self.cross_rollovers(ball, events);
        // Into the ramp's mouth, going up it.
        let (mouth, way) = self.ramp.mouth();
        let crossed = (before - mouth).dot(way) <= 0.0 && (ball.position - mouth).dot(way) > 0.0;
        let across = (ball.position - mouth).dot(Vec2::new(-way.y, way.x)).abs();
        let speed = ball.velocity.dot(way);
        if crossed && across < self.ramp.half_width - 0.15 && speed > 0.0 {
            ball.place = Place::Ramp { along: 0.0, speed };
        }
        // Into the scoop, slowly enough to drop in.
        let s = self.layout.scoop;
        if (ball.position - s.center()).length() < s.r && ball.velocity.length() < SCOOP_CATCH {
            ball.place = Place::Scoop { held: 0.0 };
            ball.position = s.center();
            ball.velocity = Vec2::ZERO;
            events.push(Event::Scoop);
        }
    }

    /// Up the ramp, slowing so that only a ball that went in at `ramp_speed` makes the top; back
    /// down it, speeding up.
    fn ride_ramp(
        &mut self,
        ball: &mut Ball,
        along: f32,
        speed: f32,
        dt: f32,
        events: &mut Vec<Event>,
    ) {
        let slowing = if speed > 0.0 {
            self.settings.ramp_speed.powi(2) / (2.0 * self.ramp.length())
        } else {
            RAMP_BACK
        };
        let speed = speed - slowing * dt;
        let along = along + speed * dt;
        if along >= self.ramp.length() {
            ball.place = Place::Mine { held: 0.0 };
            ball.velocity = Vec2::ZERO;
            events.push(Event::RampMade);
        } else if along <= 0.0 && speed < 0.0 {
            // Rolled back out of the mouth.
            let (mouth, way) = self.ramp.mouth();
            ball.place = Place::Playfield;
            ball.position = mouth - way * 0.05;
            ball.velocity = way * speed;
            events.push(Event::RampBack);
        } else {
            ball.place = Place::Ramp { along, speed };
            ball.position = self.ramp.at(along).0.truncate();
        }
    }

    fn collide_still(&mut self, ball: &mut Ball, events: &mut Vec<Event>) {
        let Settings {
            wall_bounce,
            rubber_bounce,
            bumper_kick,
            sling_kick,
            ..
        } = self.settings;
        for collider in &self.colliders {
            if let Kind::DropTarget(i) = collider.kind
                && self.drops_down[i]
            {
                continue;
            }
            let (near, thick) = collider.shape.nearest(ball.position);
            let apart = ball.position - near;
            let distance = apart.length();
            let overlap = thick + BALL_RADIUS - distance;
            if overlap <= 0.0 || distance < 1e-6 {
                continue;
            }
            let normal = apart / distance;
            ball.position += normal * overlap;
            let closing = ball.velocity.dot(normal);
            if closing >= 0.0 {
                continue;
            }
            let bounce = match collider.kind {
                Kind::Wall | Kind::DropTarget(_) | Kind::Standup(_) => wall_bounce,
                _ => rubber_bounce,
            };
            let bounce = if -closing < RESTING { 0.0 } else { bounce };
            ball.velocity -= (1.0 + bounce) * closing * normal;
            let hard = -closing > KICK_THRESHOLD;
            match collider.kind {
                Kind::Bumper(i) => {
                    ball.velocity += normal * bumper_kick;
                    self.bumper_flash[i] = 0.0;
                    events.push(Event::Bumper(i));
                }
                Kind::Sling(i) if hard => {
                    ball.velocity += normal * sling_kick;
                    self.sling_flash[i] = 0.0;
                    events.push(Event::Sling(i));
                }
                Kind::DropTarget(i) if hard => {
                    self.drops_down[i] = true;
                    events.push(Event::DropTarget(i));
                }
                Kind::Standup(i) if hard => {
                    self.standup_flash[i] = 0.0;
                    events.push(Event::Standup(i));
                }
                _ => {}
            }
        }
    }

    /// The flippers: a ball they meet bounces off them as they move, carried with them.
    fn collide_flippers(&self, ball: &mut Ball) {
        let params = self.layout.flipper;
        for flipper in &self.flippers {
            let a = flipper.pivot;
            let b = a + Vec2::from_angle(flipper.angle) * params.length;
            let ab = b - a;
            let t = ((ball.position - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            let near = a + ab * t;
            let thick = params.pivot_r + (params.tip_r - params.pivot_r) * t;
            let apart = ball.position - near;
            let distance = apart.length();
            let overlap = thick + BALL_RADIUS - distance;
            if overlap <= 0.0 || distance < 1e-6 {
                continue;
            }
            let normal = apart / distance;
            ball.position += normal * overlap;
            // The flipper's surface where the ball touches it moves with its swing.
            let contact = near + normal * thick - a;
            let surface = flipper.angular_velocity * Vec2::new(-contact.y, contact.x);
            let closing = (ball.velocity - surface).dot(normal);
            if closing >= 0.0 {
                continue;
            }
            let bounce = if -closing < RESTING {
                0.0
            } else {
                self.settings.flipper_bounce
            };
            ball.velocity -= (1.0 + bounce) * closing * normal;
        }
    }

    /// The gate at the top of the shooter lane lets the ball out, up the table, but not back.
    fn collide_gate(&self, ball: &mut Ball, before: Vec2) {
        let g = self.layout.gate;
        let in_lane = ball.position.x > g.x0 - BALL_RADIUS * 0.5 && ball.position.x < g.x1 + 0.2;
        if in_lane && before.y + BALL_RADIUS <= g.y && ball.position.y + BALL_RADIUS > g.y {
            ball.position.y = g.y - BALL_RADIUS;
            if ball.velocity.y > 0.0 {
                ball.velocity.y *= -self.settings.wall_bounce;
            }
        }
    }

    /// The plunger's tip is the floor of the shooter lane.
    fn collide_plunger(&self, ball: &mut Ball) {
        let p = self.layout.plunger;
        let tip = p.y + self.plunger;
        if (ball.position.x - p.x).abs() < 0.6
            && ball.position.y + BALL_RADIUS > tip
            && ball.position.y < tip + 1.0
        {
            ball.position.y = tip - BALL_RADIUS;
            if ball.velocity.y > 0.0 {
                ball.velocity.y = -ball.velocity.y * 0.2;
                if ball.velocity.y.abs() < RESTING {
                    ball.velocity.y = 0.0;
                }
            }
        }
    }

    /// Through the swinging doors: they swing the way the ball went, and slow it a little.
    fn cross_doors(&mut self, ball: &mut Ball, before: Vec2, events: &mut Vec<Event>) {
        let d = self.layout.doors;
        if ball.position.x < d.x0 || ball.position.x > d.x1 {
            return;
        }
        let inward = before.y > d.y && ball.position.y <= d.y;
        let outward = before.y < d.y && ball.position.y >= d.y;
        if inward || outward {
            ball.velocity *= 0.85;
            let push = (ball.velocity.y.abs() * 0.9).min(600.0);
            self.doors_speed += if inward { push } else { -push };
            events.push(Event::Doors { inward });
        }
    }

    fn cross_rollovers(&self, ball: &mut Ball, events: &mut Vec<Event>) {
        let over = self
            .layout
            .rollovers
            .iter()
            .position(|r| (ball.position - Vec2::new(r.x, r.y)).length() < ROLLOVER_REACH);
        if over != ball.over
            && let Some(i) = over
        {
            events.push(Event::Rollover(i));
        }
        ball.over = over;
    }

    /// The captive ball: stays in its lane; hit hard enough, it knocks the bull at the top.
    fn step_captive(&mut self, dt: f32, events: &mut Vec<Event>) {
        let mut ball = self.captive;
        ball.velocity.y += self.settings.slope * dt;
        ball.position += ball.velocity * dt;
        let mut scratch = Vec::new();
        self.collide_still(&mut ball, &mut scratch);
        let lane = self.layout.captive.lane;
        let top = lane.y0 + BALL_RADIUS;
        let bottom = lane.y1 + 0.3;
        if ball.position.y < top {
            ball.position.y = top;
            if ball.velocity.y < 0.0 {
                if -ball.velocity.y > KICK_THRESHOLD {
                    self.bull_flash = 0.0;
                    events.push(Event::Bull);
                }
                ball.velocity.y *= -0.4;
            }
        }
        if ball.position.y > bottom {
            ball.position.y = bottom;
            if ball.velocity.y > 0.0 {
                ball.velocity.y *= -0.3;
            }
        }
        ball.position.x = ball
            .position
            .x
            .clamp(lane.x0 + BALL_RADIUS, lane.x1 - BALL_RADIUS);
        ball.velocity.x *= 0.98;
        self.captive = ball;
    }
}

/// A line `r` thick each side, with round ends.
fn segment(colliders: &mut Vec<Collider>, a: [f32; 2], b: [f32; 2], r: f32, kind: Kind) {
    colliders.push(Collider {
        shape: Shape::Segment {
            a: Vec2::from(a),
            b: Vec2::from(b),
            r,
        },
        kind,
    });
}

/// Two balls of the same weight that touch: pushed apart, bouncing if they're closing.
fn knock(a: &mut Ball, b: &mut Ball) {
    let apart = b.position - a.position;
    let distance = apart.length();
    if distance >= 2.0 * BALL_RADIUS || distance < 1e-6 {
        return;
    }
    let normal = apart / distance;
    let overlap = 2.0 * BALL_RADIUS - distance;
    a.position -= normal * overlap / 2.0;
    b.position += normal * overlap / 2.0;
    let closing = (b.velocity - a.velocity).dot(normal);
    if closing < 0.0 {
        let impulse = normal * (-0.95 * closing);
        a.velocity -= impulse;
        b.velocity += impulse;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALOON: &str = include_str!("../../assets/pinball/saloon/layout.json");
    const SPRITES: &str = include_str!("../../assets/pinball/saloon/sprites.json");

    fn saloon() -> Table {
        Table::new(Layout::from_json(SALOON).unwrap(), Settings::default())
    }

    /// Runs the table for `seconds`, a frame at a time, collecting what happened.
    fn run(table: &mut Table, seconds: f32) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..(seconds * 60.0) as usize {
            events.extend(table.advance(1.0 / 60.0));
        }
        events
    }

    fn ball_at(table: &mut Table, x: f32, y: f32, vx: f32, vy: f32) {
        let mut ball = Ball::new(Vec2::new(x, y));
        ball.velocity = Vec2::new(vx, vy);
        table.balls = vec![ball];
    }

    #[test]
    fn the_camera_is_the_arts() {
        let table = saloon();
        let camera = Camera::new(table.layout().camera, table.layout().size);
        let sprites: serde_json::Value = serde_json::from_str(SPRITES).unwrap();
        let at: Vec<f32> = serde_json::from_value(sprites["ball"]["at"].clone()).unwrap();
        let scale = sprites["ball"]["scale"].as_f64().unwrap() as f32;
        let ours = camera.pixels_per_inch(Vec3::from_slice(&at));
        assert!((ours - scale).abs() < 0.01, "{ours} {scale}");
    }

    #[test]
    fn a_ball_left_alone_rolls_down_the_middle_into_the_drain() {
        let mut table = saloon();
        ball_at(&mut table, 9.5, 28.0, 0.0, 0.0);
        let events = run(&mut table, 4.0);
        assert!(events.contains(&Event::Drain), "{events:?}");
        assert!(table.balls.is_empty());
    }

    #[test]
    fn the_plunger_sends_the_ball_round_the_top() {
        let mut table = saloon();
        table.serve();
        run(&mut table, 0.5);
        let resting = table.balls[0].position;
        table.hold_plunger(true);
        run(&mut table, 1.0);
        table.hold_plunger(false);
        let mut highest = resting.y;
        let mut left_lane = false;
        for _ in 0..180 {
            table.advance(1.0 / 60.0);
            if let Some(ball) = table.balls.first() {
                highest = highest.min(ball.position.y);
                left_lane |= ball.position.x < 17.5;
            }
        }
        assert!(highest < 5.0, "only up to {highest}");
        assert!(left_lane, "never came out of the shooter lane");
    }

    #[test]
    fn a_held_flipper_cradles_the_ball() {
        let mut table = saloon();
        table.flip(Side::Left, true);
        run(&mut table, 0.2);
        // Dropped onto the raised flipper, two thirds along.
        let f = table.flippers[0];
        let along = Vec2::from_angle(f.angle);
        let above = Vec2::new(along.y, -along.x);
        let at = f.pivot + along * 1.8 + above * (0.3 + BALL_RADIUS + 0.05);
        ball_at(&mut table, at.x, at.y, 0.0, 0.0);
        let events = run(&mut table, 3.0);
        assert!(!events.contains(&Event::Drain), "{events:?}");
        let ball = table.balls[0];
        assert!(
            ball.velocity.length() < 5.0,
            "still moving at {}",
            ball.velocity
        );
        assert!(
            (ball.position - f.pivot).length() < 3.0,
            "at {}",
            ball.position
        );
    }

    #[test]
    fn flipping_sends_a_resting_ball_up_the_table() {
        let mut table = saloon();
        let f = table.flippers[0];
        let along = Vec2::from_angle(f.angle);
        let above = Vec2::new(along.y, -along.x);
        let at = f.pivot + along * 1.8 + above * (0.3 + BALL_RADIUS + 0.05);
        ball_at(&mut table, at.x, at.y, 0.0, 0.0);
        table.flip(Side::Left, true);
        let mut highest = at.y;
        for _ in 0..40 {
            table.advance(1.0 / 60.0);
            if let Some(ball) = table.balls.first() {
                highest = highest.min(ball.position.y);
            }
        }
        assert!(highest < 20.0, "only up to {highest}");
    }

    #[test]
    fn pop_bumpers_kick() {
        let mut table = saloon();
        let b = table.layout().bumpers[0];
        ball_at(&mut table, b.x - 2.5, b.y, 120.0, 0.0);
        let mut kicked = None;
        for _ in 0..12 {
            if table.advance(1.0 / 60.0).contains(&Event::Bumper(0)) {
                kicked = Some(table.balls[0].velocity);
                break;
            }
        }
        let kicked = kicked.expect("hit the bumper");
        assert!(
            kicked.length() > 150.0 && kicked.x < 0.0,
            "kicked away: {kicked}"
        );
    }

    #[test]
    fn a_bottle_hit_falls() {
        let mut table = saloon();
        let t = table.layout().drop_targets[2];
        let facing = Vec2::from(t.facing);
        let from = Vec2::new(t.x, t.y) + facing * 2.5;
        let v = -facing * 200.0;
        ball_at(&mut table, from.x, from.y, v.x, v.y);
        let events = run(&mut table, 0.2);
        assert!(events.contains(&Event::DropTarget(2)), "{events:?}");
        assert_eq!(table.drops_down, [false, false, true, false, false]);
    }

    #[test]
    fn the_ramp_takes_a_fast_ball_to_the_mine_and_rolls_a_slow_one_back() {
        let mut table = saloon();
        let (mouth, way) = table.ramp.mouth();
        let from = mouth - way * 0.8;
        let fast = way * 260.0;
        ball_at(&mut table, from.x, from.y, fast.x, fast.y);
        let events = run(&mut table, 2.5);
        assert!(events.contains(&Event::RampMade), "{events:?}");
        assert!(events.contains(&Event::MineKick), "{events:?}");

        let slow = way * 130.0;
        ball_at(&mut table, from.x, from.y, slow.x, slow.y);
        let events = run(&mut table, 2.0);
        assert!(events.contains(&Event::RampBack), "{events:?}");
        assert!(!events.contains(&Event::RampMade));
    }

    #[test]
    fn the_scoop_holds_a_slow_ball_then_kicks_it_out() {
        let mut table = saloon();
        let s = table.layout().scoop;
        ball_at(&mut table, s.x - 1.5, s.y + 0.4, 80.0, -20.0);
        let events = run(&mut table, 0.4);
        assert!(events.contains(&Event::Scoop), "{events:?}");
        let mut kicked = None;
        for _ in 0..80 {
            if table.advance(1.0 / 60.0).contains(&Event::ScoopKick) {
                kicked = Some(table.balls[0].velocity);
                break;
            }
        }
        let kicked = kicked.expect("kicked out");
        assert!(
            kicked.y > 0.0 && kicked.x < 0.0,
            "kicked toward the flippers: {kicked}"
        );
    }

    #[test]
    fn knocking_the_captive_ball_bucks_the_bull() {
        let mut table = saloon();
        run(&mut table, 0.5);
        let lane = table.layout().captive.lane;
        let x = table.captive.position.x;
        assert!(x > lane.x0 && x < lane.x1);
        ball_at(&mut table, x, 27.0, 0.0, -380.0);
        let events = run(&mut table, 0.6);
        assert!(events.contains(&Event::Bull), "{events:?}");
    }

    #[test]
    fn the_shooter_gate_lets_balls_out_but_not_back() {
        let mut table = saloon();
        let g = table.layout().gate;
        let x = (g.x0 + g.x1) / 2.0;
        ball_at(&mut table, x, g.y - 2.0, 0.0, 150.0);
        run(&mut table, 0.3);
        assert!(
            table.balls[0].position.y < g.y,
            "got back in at {}",
            table.balls[0].position
        );
    }
}
