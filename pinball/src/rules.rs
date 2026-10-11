//! The Saloon's rules: what each shot scores, and what it lights, starts and wins, as the cards
//! on the apron say:
//! - The piano starts the mode lit on the star (the slingshots change which), a task against the
//!   clock: knock down the bottles, hit the wanted posters, ride the bull, make the mine ramp,
//!   loop the orbit, bust through the doors. Each won is a step up in rank; all six light the
//!   showdown.
//! - The mine ramp loads a cart; three carts start runaway multiball, where the ramp is a
//!   jackpot and then the bull a super jackpot.
//! - The three wanted posters light a bounty at the doors; the bottles relight the kickback.
//! - The lit top lane, plunged into, is a skill shot; all three top lanes raise the bonus.
//!
//! And like Space Cadet, it says what to go for: whatever matters now has its arrow blinking
//! (fast when it's what the mode wants), and the display spells it out (`Rules::screen`).

use std::collections::VecDeque;

use glam::Vec2;

use crate::{Event, Table};

/// Balls in a game.
pub const BALLS: u32 = 3;
/// Seconds: a ball drained this soon after its launch comes back; a mode lasts this long.
const BALL_SAVE: f32 = 8.0;
const MULTIBALL_SAVE: f32 = 12.0;
const MODE_TIME: f32 = 45.0;
const SHOWDOWN_TIME: f32 = 60.0;
/// A skill shot is the first switch after the launch, this soon after it.
const SKILL_SHOT_FOR: f32 = 6.0;
const BOTTLES_BACK_AFTER: f32 = 1.5;
/// How long a lamp lights for a hit, and how long each thing the display says stays up.
const FLASH: f32 = 0.6;
const CALLOUT_FOR: f32 = 2.2;
const NOTE_FOR: f32 = 1.6;
const GOAL_FOR: f32 = 3.0;
/// The bonus count shows this long before the next ball.
const BONUS_FOR: f32 = 2.5;
/// The kickback sends a ball in the left outlane straight back up the side.
const KICKBACK: Vec2 = Vec2::new(0.0, -180.0);
/// Where the left outlane's switch is (the kickback's), and the bounties, in dollars.
const LEFT_OUTLANE: usize = 0;
const ORBIT: usize = 4;
const TOP_LANES: usize = 5;
const ORBIT_GATE: usize = 1;
const BOUNTIES: [u64; 5] = [500, 1000, 2500, 5000, 10_000];
/// The display's rank names, and the rank for each number of modes won.
pub const RANKS: [&str; 5] = ["GREENHORN", "COWHAND", "DEPUTY", "SHERIFF", "MARSHAL"];
const RANK_FOR_MODES: [usize; 7] = [0, 1, 2, 2, 3, 3, 4];

/// The shots a mode can ask for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shot {
    Bottle,
    Poster(usize),
    Bull,
    Ramp,
    Orbit,
    Doors,
}

/// The six modes on the star, in its order (its lamps are `mode-0` to `mode-5`).
struct ModeKind {
    name: &'static str,
    /// What it asks for, how many, and the lamp pointing at it.
    wants: fn(Shot) -> bool,
    needs: u32,
    arrow: &'static str,
    /// What to do, with how many are still wanted.
    task: fn(u32) -> String,
}

const MODES: [ModeKind; 6] = [
    ModeKind {
        name: "BRAWL",
        wants: |shot| shot == Shot::Bottle,
        needs: 5,
        arrow: "bottles-arrow",
        task: |left| format!("KNOCK DOWN {left} {}", plural(left, "BOTTLE", "BOTTLES")),
    },
    ModeKind {
        name: "DUEL",
        wants: |shot| matches!(shot, Shot::Poster(_)),
        needs: 3,
        arrow: "poster",
        task: |left| format!("SHOOT {left} WANTED {}", plural(left, "POSTER", "POSTERS")),
    },
    ModeKind {
        name: "BULL RIDE",
        wants: |shot| shot == Shot::Bull,
        needs: 3,
        arrow: "bull-arrow",
        task: |left| format!("HIT THE BULL {left} MORE {}", plural(left, "TIME", "TIMES")),
    },
    ModeKind {
        name: "GOLD RUSH",
        wants: |shot| shot == Shot::Ramp,
        needs: 2,
        arrow: "ramp-arrow",
        task: |left| {
            format!(
                "MAKE THE MINE RAMP {left} MORE {}",
                plural(left, "TIME", "TIMES")
            )
        },
    },
    ModeKind {
        name: "STAGECOACH",
        wants: |shot| shot == Shot::Orbit,
        needs: 2,
        arrow: "orbit-arrow",
        task: |left| {
            format!(
                "LOOP THE LEFT ORBIT {left} MORE {}",
                plural(left, "TIME", "TIMES")
            )
        },
    },
    ModeKind {
        name: "JAIL BREAK",
        wants: |shot| shot == Shot::Doors,
        needs: 3,
        arrow: "doors-arrow",
        task: |left| {
            format!(
                "BUST THROUGH THE DOORS {left} MORE {}",
                plural(left, "TIME", "TIMES")
            )
        },
    },
];

fn plural(n: u32, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

/// A mode being played: which, how long it has left, and how far along it is (for the duel,
/// which posters it has had).
#[derive(Clone, Copy, Debug)]
struct Running {
    mode: usize,
    left: f32,
    done: u32,
    posters: [bool; 3],
}

/// Something the display says for a while, over the goals: big words and a line under them, or
/// just a line.
#[derive(Clone, Debug, PartialEq)]
struct Callout {
    big: Option<String>,
    line: String,
    left: f32,
}

/// What the display shows (the client draws it in dots).
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    /// Between games: the last score.
    Over { score: String },
    /// Playing: the top line (the ball, or the mode and its clock, and the score), and under it
    /// big words with a line, or a few lines of what to go for, the first maybe a title.
    Playing {
        left: String,
        score: String,
        body: Body,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Callout { big: String, line: String },
    Text { title: Option<String>, text: String },
}

/// Buttons pressed this frame that the rules care about: the flippers change the lit lanes.
#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub score: u64,
    /// The ball being played (1 to 3), and balls earned to play again.
    pub ball: u32,
    extra_balls: u32,
    pub over: bool,
    time: f32,
    /// Seconds until the next ball comes (the bonus counting meanwhile).
    serve_in: Option<f32>,
    /// This ball's bonus, and what it's multiplied by.
    bonus: u64,
    multiplier: u64,
    /// Seconds of ball save left; and whether this ball has had its save.
    save: f32,
    saved_this_ball: bool,
    /// The skill shot's lane, while it can still be made, and the time left to make it.
    skill: Option<(usize, f32)>,
    lanes: [bool; 3],
    posters: [bool; 3],
    bounty_lit: bool,
    bounties: usize,
    carts: u32,
    multiball: bool,
    jackpots: u32,
    super_lit: bool,
    /// Balls still to come out of the mine (seconds until each).
    ejects: Vec<f32>,
    modes_won: [bool; 6],
    selected: usize,
    running: Option<Running>,
    showdown: Option<f32>,
    showdowns: u32,
    kickback: bool,
    extra_ball_lit: bool,
    extra_ball_given: bool,
    bottles_back_in: Option<f32>,
    /// Lamps lit for a moment by a hit.
    flashes: Vec<(&'static str, f32)>,
    callouts: VecDeque<Callout>,
    /// Which goal the display is on, and for how much longer.
    goal: usize,
    goal_left: f32,
}

impl Default for Rules {
    fn default() -> Self {
        Self::new()
    }
}

impl Rules {
    /// Waiting for a game.
    pub fn new() -> Self {
        Self {
            score: 0,
            ball: 1,
            extra_balls: 0,
            over: true,
            time: 0.0,
            serve_in: None,
            bonus: 0,
            multiplier: 1,
            save: 0.0,
            saved_this_ball: false,
            skill: None,
            lanes: [false; 3],
            posters: [false; 3],
            bounty_lit: false,
            bounties: 0,
            carts: 0,
            multiball: false,
            jackpots: 0,
            super_lit: false,
            ejects: Vec::new(),
            modes_won: [false; 6],
            selected: 0,
            running: None,
            showdown: None,
            showdowns: 0,
            kickback: false,
            extra_ball_lit: false,
            extra_ball_given: false,
            bottles_back_in: None,
            flashes: Vec::new(),
            callouts: VecDeque::new(),
            goal: 0,
            goal_left: GOAL_FOR,
        }
    }

    /// A new game: everything back to the start, and the first ball on the plunger.
    pub fn start(&mut self, table: &mut Table) {
        *self = Self {
            over: false,
            kickback: true,
            time: self.time,
            ..Self::new()
        };
        table.balls.clear();
        table.reset_drop_targets();
        self.serve(table);
    }

    fn serve(&mut self, table: &mut Table) {
        table.serve();
        self.skill = Some(((self.ball as usize + 1) % 3, SKILL_SHOT_FOR));
        self.saved_this_ball = false;
    }

    /// A ball waits on the plunger.
    pub fn on_plunger(&self, table: &Table) -> bool {
        table
            .balls
            .iter()
            .any(|ball| table.in_shooter_lane(ball) && ball.velocity.length() < 20.0)
    }

    /// The rank won so far (an index into `RANKS`).
    pub fn rank(&self) -> usize {
        RANK_FOR_MODES[self.modes_won.iter().filter(|&&won| won).count()]
    }

    /// The game moves on by `dt` seconds, after what happened on the table.
    pub fn update(&mut self, table: &mut Table, events: &[Event], input: Input, dt: f32) {
        self.time += dt;
        if self.over {
            return;
        }
        self.tick(table, dt);
        // The flippers move the lit top lanes along (on the plunger, the skill shot's lane).
        let shift = |lanes: [bool; 3], right: bool| {
            if right {
                [lanes[2], lanes[0], lanes[1]]
            } else {
                [lanes[1], lanes[2], lanes[0]]
            }
        };
        for (pressed, right) in [(input.left, false), (input.right, true)] {
            if !pressed {
                continue;
            }
            if self.on_plunger(table)
                && let Some((lane, _)) = &mut self.skill
            {
                *lane = if right {
                    (*lane + 1) % 3
                } else {
                    (*lane + 2) % 3
                };
            } else {
                self.lanes = shift(self.lanes, right);
            }
        }
        let mut drains = 0;
        for &event in events {
            if event == Event::Drain {
                drains += 1;
            } else {
                self.event(event, table);
            }
        }
        for _ in 0..drains {
            self.drained(table);
        }
        // All the bottles down: the kickback lit again, and they stand up.
        if self.bottles_back_in.is_none() && table.drops_down.iter().all(|&down| down) {
            self.bottles_back_in = Some(BOTTLES_BACK_AFTER);
            self.add(10_000, 2_000);
            if self.kickback {
                self.note("LAST ROUND  10,000");
            } else {
                self.kickback = true;
                self.callout("KICKBACK", "LIT FOR THE LEFT OUTLANE");
            }
        }
        // The last ball in play gone: its bonus, then the next.
        if table.balls.is_empty() && self.ejects.is_empty() && self.serve_in.is_none() {
            self.end_ball();
        }
    }

    fn tick(&mut self, table: &mut Table, dt: f32) {
        self.save = (self.save - dt).max(0.0);
        // Before the launch, the skill shot waits; after it, it can be made for a while.
        let waiting = self.on_plunger(table);
        if let Some((_, left)) = &mut self.skill {
            *left = if waiting { SKILL_SHOT_FOR } else { *left - dt };
            if *left <= 0.0 {
                self.skill = None;
            }
        }
        self.flashes.retain_mut(|(_, left)| {
            *left -= dt;
            *left > 0.0
        });
        if let Some(callout) = self.callouts.front_mut() {
            callout.left -= dt;
            if callout.left <= 0.0 {
                self.callouts.pop_front();
            }
        }
        self.goal_left -= dt;
        if self.goal_left <= 0.0 {
            self.goal_left = GOAL_FOR;
            self.goal += 1;
        }
        if let Some(left) = &mut self.bottles_back_in {
            *left -= dt;
            if *left <= 0.0 {
                self.bottles_back_in = None;
                table.reset_drop_targets();
            }
        }
        let mut released = 0;
        self.ejects.retain_mut(|left| {
            *left -= dt;
            let now = *left <= 0.0;
            released += usize::from(now);
            !now
        });
        for _ in 0..released {
            table.release_from_mine();
        }
        if let Some(running) = &mut self.running {
            running.left -= dt;
            if running.left <= 0.0 {
                let name = MODES[running.mode].name;
                self.running = None;
                self.callout(name, "TIME'S UP");
            }
        }
        if let Some(left) = &mut self.showdown {
            *left -= dt;
            if *left <= 0.0 {
                self.showdown = None;
                self.showdowns += 1;
                self.modes_won = [false; 6];
                self.selected = 0;
                self.callout("HIGH NOON", "THE STAR STARTS OVER");
            }
        }
        if let Some(left) = &mut self.serve_in {
            *left -= dt;
            if *left <= 0.0 {
                self.serve_in = None;
                self.serve(table);
            }
        }
    }

    /// Points to the score, and to this ball's bonus.
    fn add(&mut self, points: u64, bonus: u64) {
        self.score += points;
        self.bonus += bonus;
    }

    fn flash(&mut self, lamp: &'static str) {
        self.flashes.retain(|(name, _)| *name != lamp);
        self.flashes.push((lamp, FLASH));
    }

    /// Big words on the display, and a line under them.
    fn callout(&mut self, big: &str, line: &str) {
        self.push(Callout {
            big: Some(big.to_string()),
            line: line.to_string(),
            left: CALLOUT_FOR,
        });
    }

    /// A line on the display.
    fn note(&mut self, line: &str) {
        self.push(Callout {
            big: None,
            line: line.to_string(),
            left: NOTE_FOR,
        });
    }

    fn push(&mut self, callout: Callout) {
        // A few at most: the newest matter, and notes give way.
        if self.callouts.len() >= 3 {
            if let Some(i) = self.callouts.iter().skip(1).position(|c| c.big.is_none()) {
                self.callouts.remove(i + 1);
            } else {
                self.callouts.pop_back();
            }
        }
        self.callouts.push_back(callout);
    }

    fn event(&mut self, event: Event, table: &mut Table) {
        // The first switch after a launch: the skill shot if it's the lit lane, gone otherwise.
        if !matches!(event, Event::Launch | Event::Gate(0))
            && let Some((lane, _)) = self.skill.take()
            && event == Event::Rollover(TOP_LANES + lane)
        {
            self.add(20_000, 0);
            self.callout("SKILL SHOT", "20,000");
        }
        match event {
            Event::Bumper(i) => {
                self.add(100, 10);
                self.flash(if i % 2 == 0 {
                    "bumper-light-0"
                } else {
                    "bumper-light-1"
                });
            }
            Event::Sling(_) => {
                self.add(10, 0);
                // The slingshots change the mode the piano starts.
                if self.running.is_none() && self.showdown.is_none() {
                    self.select_next();
                }
            }
            Event::DropTarget(_) => {
                self.add(750, 250);
                self.flash("bottles-arrow");
                self.shot(Shot::Bottle, table);
            }
            Event::Standup(i) => {
                self.add(500, 250);
                let fresh = !self.posters[i];
                self.posters[i] = true;
                if self.posters.iter().all(|&hit| hit) && !self.bounty_lit {
                    self.bounty_lit = true;
                    let value = BOUNTIES[self.bounties.min(4)];
                    self.callout(
                        "BOUNTY LIT",
                        &format!("${} AT THE DOORS", with_commas(value)),
                    );
                } else if fresh && !self.bounty_lit {
                    let left = self.posters.iter().filter(|&&hit| !hit).count();
                    self.note(&format!(
                        "{left} MORE {}",
                        plural(left as u32, "POSTER", "POSTERS")
                    ));
                }
                self.shot(Shot::Poster(i), table);
            }
            Event::Rollover(LEFT_OUTLANE) => {
                self.add(250, 100);
                if self.kickback {
                    self.kickback = false;
                    let layout = table.layout().rollovers[LEFT_OUTLANE];
                    table.kick(Vec2::new(layout.x, layout.y), KICKBACK);
                    self.callout("KICKBACK", "");
                } else {
                    self.flash("kickback");
                }
            }
            Event::Rollover(i) if i == ORBIT => {
                self.add(250, 100);
                self.flash("orbit-light");
            }
            Event::Rollover(i) if (TOP_LANES..TOP_LANES + 3).contains(&i) => {
                self.add(500, 200);
                self.lanes[i - TOP_LANES] = true;
                if self.lanes.iter().all(|&lit| lit) {
                    self.lanes = [false; 3];
                    if self.multiplier < 5 {
                        self.multiplier += 1;
                        self.callout(&format!("BONUS {}X", self.multiplier), "");
                    } else {
                        self.add(25_000, 0);
                        self.note("TOP LANES  25,000");
                    }
                }
            }
            Event::Rollover(i) => {
                self.add(250, 100);
                self.flash(match i {
                    1 => "inlane-left",
                    2 => "inlane-right",
                    _ => "special",
                });
            }
            Event::Gate(ORBIT_GATE) => {
                self.add(3_000, 1_000);
                self.flash("orbit-arrow");
                if !self
                    .running
                    .is_some_and(|r| (MODES[r.mode].wants)(Shot::Orbit))
                {
                    self.note("ORBIT  3,000");
                }
                self.shot(Shot::Orbit, table);
            }
            Event::Doors { inward: true } => {
                self.add(1_000, 300);
                self.flash("doors-arrow");
                if self.bounty_lit {
                    let value = BOUNTIES[self.bounties.min(4)];
                    self.add(value * 10, 0);
                    self.bounties = (self.bounties + 1).min(BOUNTIES.len());
                    self.bounty_lit = false;
                    self.posters = [false; 3];
                    self.callout("BOUNTY", &format!("${} COLLECTED", with_commas(value)));
                }
                if self.extra_ball_lit {
                    self.extra_ball_lit = false;
                    self.extra_balls += 1;
                    self.callout("EXTRA BALL", "SHOOT AGAIN");
                }
                self.shot(Shot::Doors, table);
            }
            Event::Scoop => {
                self.add(2_000, 500);
                self.flash("scoop-arrow");
                self.start_mode(table);
            }
            Event::RampMade => {
                self.add(5_000, 1_000);
                self.flash("ramp-arrow");
                if self.multiball {
                    let jackpot = 50_000 + 10_000 * u64::from(self.jackpots);
                    self.jackpots += 1;
                    self.add(jackpot, 0);
                    self.super_lit = true;
                    self.callout("JACKPOT", &with_commas(jackpot));
                } else {
                    self.carts += 1;
                    if self.carts >= 3 {
                        self.start_multiball();
                    } else {
                        self.note(&format!("CART {} OF 3 LOADED", self.carts));
                    }
                }
                self.shot(Shot::Ramp, table);
            }
            Event::Bull => {
                self.add(2_500, 500);
                self.flash("bull-arrow");
                if self.multiball && self.super_lit {
                    let jackpot = 3 * (50_000 + 10_000 * u64::from(self.jackpots));
                    self.super_lit = false;
                    self.add(jackpot, 0);
                    self.callout("SUPER", &format!("JACKPOT {}", with_commas(jackpot)));
                }
                self.shot(Shot::Bull, table);
            }
            Event::Launch => {
                if !self.saved_this_ball && self.save <= 0.0 {
                    self.save = BALL_SAVE;
                }
            }
            Event::Gate(_)
            | Event::Doors { inward: false }
            | Event::RampBack
            | Event::ScoopKick
            | Event::MineKick
            | Event::Drain => {}
        }
    }

    /// A shot the running mode, or the showdown, may want.
    fn shot(&mut self, shot: Shot, table: &mut Table) {
        if self.showdown.is_some() {
            self.add(25_000, 0);
            self.note("OUTLAW HIT  25,000");
            return;
        }
        let Some(mut running) = self.running else {
            return;
        };
        let kind = &MODES[running.mode];
        if !(kind.wants)(shot) {
            return;
        }
        if let Shot::Poster(i) = shot {
            // The duel: each poster once.
            if running.posters[i] {
                return;
            }
            running.posters[i] = true;
        }
        running.done += 1;
        self.add(5_000, 1_000);
        if running.done < kind.needs {
            self.running = Some(running);
            self.note(&(kind.task)(kind.needs - running.done));
            return;
        }
        // Won.
        self.running = None;
        let rank = self.rank();
        self.modes_won[running.mode] = true;
        let won = self.modes_won.iter().filter(|&&won| won).count();
        let points = 25_000 * won as u64;
        self.add(points, 5_000);
        self.callout(kind.name, &format!("WON  {}", with_commas(points)));
        if self.rank() > rank {
            self.callout(RANKS[self.rank()], "YOU'VE BEEN PROMOTED");
        }
        if won == 2 && !self.extra_ball_given {
            self.extra_ball_given = true;
            self.extra_ball_lit = true;
            self.note("EXTRA BALL LIT AT THE DOORS");
        }
        if won == MODES.len() {
            self.callout("SHOWDOWN", "IS LIT AT THE PIANO");
        } else {
            self.select_next();
        }
        let _ = table;
    }

    /// The next mode not yet won, after the selected one.
    fn select_next(&mut self) {
        for step in 1..=MODES.len() {
            let mode = (self.selected + step) % MODES.len();
            if !self.modes_won[mode] {
                self.selected = mode;
                return;
            }
        }
    }

    /// The piano: starts the lit mode, or the showdown once they're all won.
    fn start_mode(&mut self, table: &mut Table) {
        if self.running.is_some() || self.showdown.is_some() {
            return;
        }
        if self.modes_won.iter().all(|&won| won) {
            self.showdown = Some(SHOWDOWN_TIME);
            self.callout("SHOWDOWN", "EVERY SHOT 25,000");
            return;
        }
        if self.modes_won[self.selected] {
            self.select_next();
        }
        let mode = self.selected;
        if mode == 0 {
            // The brawl wants every bottle standing.
            table.reset_drop_targets();
            self.bottles_back_in = None;
        }
        self.running = Some(Running {
            mode,
            left: MODE_TIME,
            done: 0,
            posters: [false; 3],
        });
        let kind = &MODES[mode];
        self.callout(kind.name, &(kind.task)(kind.needs));
        self.goal = 0;
        self.goal_left = GOAL_FOR;
    }

    fn start_multiball(&mut self) {
        self.carts = 0;
        self.multiball = true;
        self.super_lit = false;
        self.ejects = vec![1.0, 2.2];
        self.save = self.save.max(MULTIBALL_SAVE);
        self.callout("MULTIBALL", "RUNAWAY CARTS");
    }

    /// A ball went down the drain (`Table::balls` no longer has it).
    fn drained(&mut self, table: &mut Table) {
        let left = table.balls.len() + self.ejects.len();
        if self.save > 0.0 {
            // Saved: in multiball, the mine lets out another; alone, it's back on the plunger.
            self.saved_this_ball = true;
            if self.multiball {
                self.ejects.push(0.8);
            } else if left == 0 {
                table.serve();
                self.save = 0.0;
            }
            self.callout("BALL SAVED", "");
            return;
        }
        if self.multiball && left <= 1 {
            self.multiball = false;
            self.super_lit = false;
            self.note("MULTIBALL OVER");
        }
    }

    /// The ball's over: its bonus, and the next ball, or the same one again, or the game's over.
    fn end_ball(&mut self) {
        let bonus = self.bonus * self.multiplier;
        self.score += bonus;
        self.callouts.clear();
        self.push(Callout {
            big: Some(format!("BONUS {}X", self.multiplier)),
            line: with_commas(bonus),
            left: BONUS_FOR,
        });
        self.bonus = 0;
        self.multiplier = 1;
        self.save = 0.0;
        self.skill = None;
        self.running = None;
        self.showdown = None;
        self.multiball = false;
        self.super_lit = false;
        if self.extra_balls > 0 {
            self.extra_balls -= 1;
            self.note("SHOOT AGAIN");
        } else if self.ball < BALLS {
            self.ball += 1;
        } else {
            self.over = true;
            return;
        }
        self.serve_in = Some(BONUS_FOR);
    }

    /// Whether a lamp (by its name in the art) is lit now: lit for good, blinking, or flashing
    /// for a hit.
    pub fn lit(&self, lamp: &str, table: &Table) -> bool {
        let slow = (self.time * 2.0).fract() < 0.5;
        let fast = (self.time * 6.0).fract() < 0.5;
        if self.flashes.iter().any(|(name, _)| *name == lamp) {
            return true;
        }
        let number = |prefix: &str| {
            lamp.strip_prefix(prefix)
                .and_then(|i| i.parse::<usize>().ok())
        };
        if let Some(i) = number("chamber-") {
            // The six-shooter: two chambers for each ball left.
            let left = (BALLS + 1).saturating_sub(self.ball) + self.extra_balls;
            return (i as u32) < left * 2;
        }
        if let Some(i) = number("mode-") {
            return match self.running {
                Some(running) if running.mode == i => fast,
                _ if self.modes_won[i] => true,
                None if self.showdown.is_none() && self.selected == i => slow,
                _ => false,
            };
        }
        if let Some(i) = number("rank-").or_else(|| number("backbox-rank-")) {
            return i <= self.rank();
        }
        if let Some(i) = number("bounty-") {
            return i < self.bounties || (i == self.bounties && self.bounty_lit && slow);
        }
        if let Some(i) = number("lane-") {
            return match self.skill {
                Some((lane, _)) if lane == i => fast,
                _ => self.lanes[i],
            };
        }
        if let Some(i) = number("poster-light-") {
            return match self.running {
                Some(running) if running.mode == 1 => running.posters[i] || fast,
                _ => self.posters[i],
            };
        }
        // An arrow: fast for what the mode (or the showdown, or a jackpot) wants now, slow for
        // what's worth a shot.
        let mode_wants = self
            .running
            .is_some_and(|running| MODES[running.mode].arrow == lamp);
        let urgent = mode_wants || (self.showdown.is_some() && lamp.ends_with("-arrow"));
        if urgent {
            return fast;
        }
        let ready = self.running.is_none() && self.showdown.is_none();
        match lamp {
            "scoop-arrow" => ready && slow,
            "ramp-arrow" => {
                if self.multiball {
                    fast
                } else {
                    slow
                }
            }
            "lock" => !self.multiball && (self.carts == 2 && slow || self.carts == 1),
            "bull-arrow" => self.super_lit && fast,
            "jackpot" => self.super_lit && slow || self.multiball && !self.super_lit,
            "doors-arrow" => (self.bounty_lit || self.extra_ball_lit) && slow,
            "extra-ball" => self.extra_ball_lit && slow,
            "showdown" => {
                self.showdown.is_some() && fast
                    || ready && self.modes_won.iter().all(|&won| won) && slow
            }
            "kickback" => self.kickback,
            "shoot-again" => self.extra_balls > 0 || (self.save > 0.0 && fast),
            _ => {
                let _ = table;
                false
            }
        }
    }

    /// What to go for now, most pressing first.
    fn goals(&self, table: &Table, touch: bool) -> Vec<(Option<String>, String)> {
        let mut goals = Vec::new();
        if self.on_plunger(table) {
            if self.skill.is_some() {
                goals.push((
                    Some("SKILL SHOT".to_string()),
                    "PLUNGE INTO THE BLINKING TOP LANE".to_string(),
                ));
            }
            goals.push((
                None,
                if touch {
                    "HOLD A FINGER DOWN TO PULL THE PLUNGER"
                } else {
                    "HOLD SPACE TO PULL THE PLUNGER, LET GO TO LAUNCH"
                }
                .to_string(),
            ));
            return goals;
        }
        if let Some(running) = self.running {
            let kind = &MODES[running.mode];
            goals.push((
                Some(format!("{} {}", kind.name, clock(running.left))),
                (kind.task)(kind.needs - running.done),
            ));
        }
        if let Some(left) = self.showdown {
            goals.push((
                Some(format!("SHOWDOWN {}", clock(left))),
                "EVERY BLINKING SHOT SCORES 25,000".to_string(),
            ));
        }
        if self.multiball {
            goals.push((
                Some("MULTIBALL".to_string()),
                if self.super_lit {
                    "RIDE THE BULL FOR THE SUPER JACKPOT"
                } else {
                    "SHOOT THE MINE RAMP FOR A JACKPOT"
                }
                .to_string(),
            ));
        }
        if self.running.is_none() && self.showdown.is_none() {
            if self.modes_won.iter().all(|&won| won) {
                goals.push((None, "SHOOT THE PIANO TO FACE THE OUTLAW".to_string()));
            } else {
                goals.push((
                    None,
                    format!("SHOOT THE PIANO TO START {}", MODES[self.selected].name),
                ));
                goals.push((None, "SLINGSHOTS CHANGE THE MODE".to_string()));
            }
        }
        if self.extra_ball_lit {
            goals.push((None, "SHOOT THE DOORS FOR AN EXTRA BALL".to_string()));
        }
        if self.bounty_lit {
            let value = BOUNTIES[self.bounties.min(4)];
            goals.push((
                None,
                format!("SHOOT THE DOORS FOR A ${} BOUNTY", with_commas(value)),
            ));
        } else {
            let left = self.posters.iter().filter(|&&hit| !hit).count() as u32;
            goals.push((
                None,
                format!(
                    "HIT {left} WANTED {} TO LIGHT A BOUNTY",
                    plural(left, "POSTER", "POSTERS")
                ),
            ));
        }
        if !self.multiball {
            goals.push((
                None,
                format!("SHOOT THE MINE RAMP TO LOAD CART {} OF 3", self.carts + 1),
            ));
        }
        if self.multiplier < 5 {
            goals.push((
                None,
                format!("LIGHT THE TOP LANES FOR {}X BONUS", self.multiplier + 1),
            ));
        }
        goals
    }

    /// What the display shows now.
    pub fn screen(&self, table: &Table, touch: bool) -> Screen {
        if self.over {
            return Screen::Over {
                score: with_commas(self.score),
            };
        }
        let left = format!("BALL {}", self.ball);
        let score = with_commas(self.score);
        if let Some(callout) = self.callouts.front() {
            let body = match &callout.big {
                Some(big) => Body::Callout {
                    big: big.clone(),
                    line: callout.line.clone(),
                },
                None => Body::Text {
                    title: None,
                    text: callout.line.clone(),
                },
            };
            return Screen::Playing { left, score, body };
        }
        let goals = self.goals(table, touch);
        // A mode's own goal shows every other time.
        let pick = if self.running.is_some() && self.goal.is_multiple_of(2) {
            0
        } else {
            self.goal % goals.len().max(1)
        };
        let (title, text) = goals.get(pick).cloned().unwrap_or_default();
        Screen::Playing {
            left,
            score,
            body: Body::Text { title, text },
        }
    }
}

/// Seconds as a clock: 0:45.
fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).ceil() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn with_commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layout, Settings};

    fn saloon() -> (Table, Rules) {
        let layout =
            Layout::from_json(include_str!("../../assets/pinball/saloon/layout.json")).unwrap();
        let mut table = Table::new(layout, Settings::default());
        let mut rules = Rules::new();
        rules.start(&mut table);
        (table, rules)
    }

    /// The rules told `events` happened, a frame each.
    fn happen(table: &mut Table, rules: &mut Rules, events: &[Event]) {
        for &event in events {
            rules.update(table, &[event], Input::default(), 1.0 / 60.0);
        }
    }

    fn says(rules: &Rules, table: &Table, words: &str) -> bool {
        // Everything it would say: its callouts, and every goal.
        let goals = rules.goals(table, false);
        rules
            .callouts
            .iter()
            .map(|c| format!("{:?} {}", c.big, c.line))
            .chain(
                goals
                    .iter()
                    .map(|(title, text)| format!("{title:?} {text}")),
            )
            .any(|said| said.contains(words))
    }

    #[test]
    fn the_piano_starts_the_lit_mode_and_its_shots_win_it() {
        let (mut table, mut rules) = saloon();
        assert!(says(&rules, &table, "PLUNGE"));
        // Off the plunger, the display says what the piano starts, and its arrow blinks.
        table.balls[0].position = glam::Vec2::new(9.5, 20.0);
        assert!(says(&rules, &table, "SHOOT THE PIANO TO START BRAWL"));
        happen(&mut table, &mut rules, &[Event::Sling(0)]);
        assert!(says(&rules, &table, "START DUEL"), "the slings change it");
        happen(&mut table, &mut rules, &[Event::Scoop]);
        assert!(says(&rules, &table, "SHOOT 3 WANTED POSTERS"));
        let blinks = (0..60).any(|i| {
            rules.time = i as f32 / 60.0;
            rules.lit("poster-light-2", &table)
        }) && (0..60).any(|i| {
            rules.time = i as f32 / 60.0;
            !rules.lit("poster-light-2", &table)
        });
        assert!(blinks, "the posters blink");
        let score = rules.score;
        happen(
            &mut table,
            &mut rules,
            &[Event::Standup(0), Event::Standup(0), Event::Standup(1)],
        );
        assert!(rules.running.is_some(), "the same poster twice counts once");
        happen(&mut table, &mut rules, &[Event::Standup(2)]);
        assert!(rules.running.is_none() && rules.modes_won[1]);
        assert_eq!(rules.rank(), 1);
        assert!(rules.score > score + 25_000);
        assert!(says(&rules, &table, "COWHAND"));
    }

    #[test]
    fn a_mode_runs_out_of_time() {
        let (mut table, mut rules) = saloon();
        happen(&mut table, &mut rules, &[Event::Scoop]);
        assert!(rules.running.is_some());
        rules.update(&mut table, &[], Input::default(), MODE_TIME + 0.1);
        assert!(rules.running.is_none() && !rules.modes_won[0]);
    }

    #[test]
    fn three_carts_start_multiball_and_the_mine_lets_out_two_more_balls() {
        let (mut table, mut rules) = saloon();
        table.balls[0].position = glam::Vec2::new(9.5, 20.0);
        happen(&mut table, &mut rules, &[Event::RampMade, Event::RampMade]);
        assert!(!rules.multiball && says(&rules, &table, "CART 3 OF 3"));
        happen(&mut table, &mut rules, &[Event::RampMade]);
        assert!(rules.multiball);
        let mut kicked = 0;
        for _ in 0..240 {
            let events = table.advance(1.0 / 60.0);
            kicked += events.iter().filter(|&&e| e == Event::MineKick).count();
            rules.update(&mut table, &events, Input::default(), 1.0 / 60.0);
        }
        // Three in play, wherever they are (a drained one comes back while the ball save lasts).
        assert!(kicked >= 2);
        assert_eq!(
            table.balls.len() + rules.ejects.len(),
            3,
            "{:?}",
            table.balls
        );
        happen(&mut table, &mut rules, &[Event::RampMade]);
        assert!(rules.super_lit && says(&rules, &table, "RIDE THE BULL"));
        let score = rules.score;
        happen(&mut table, &mut rules, &[Event::Bull]);
        assert!(rules.score >= score + 150_000);
    }

    #[test]
    fn the_wanted_posters_light_a_bounty_at_the_doors() {
        let (mut table, mut rules) = saloon();
        table.balls[0].position = glam::Vec2::new(9.5, 20.0);
        happen(
            &mut table,
            &mut rules,
            &[Event::Standup(0), Event::Standup(1), Event::Standup(2)],
        );
        assert!(rules.bounty_lit && says(&rules, &table, "$500 BOUNTY"));
        let score = rules.score;
        happen(&mut table, &mut rules, &[Event::Doors { inward: true }]);
        assert!(!rules.bounty_lit && rules.score >= score + 5_000);
        assert!(rules.lit("bounty-0", &table));
        assert!(says(&rules, &table, "HIT 3 WANTED POSTERS"));
    }

    #[test]
    fn the_kickback_sends_a_ball_in_the_left_outlane_back_up() {
        let (mut table, mut rules) = saloon();
        let lane = table.layout().rollovers[LEFT_OUTLANE];
        table.balls[0].position = glam::Vec2::new(lane.x, lane.y - 2.0);
        table.balls[0].velocity = glam::Vec2::new(0.0, 60.0);
        let mut highest = f32::MAX;
        for _ in 0..90 {
            let events = table.advance(1.0 / 60.0);
            rules.update(&mut table, &events, Input::default(), 1.0 / 60.0);
            if let Some(ball) = table.balls.first() {
                highest = highest.min(ball.position.y);
            }
        }
        assert!(!rules.kickback, "used");
        assert!(highest < 26.0, "only back up to {highest}");
        assert_eq!(rules.ball, 1, "not drained");
    }

    #[test]
    fn a_ball_drained_right_after_its_launch_is_saved() {
        let (mut table, mut rules) = saloon();
        happen(&mut table, &mut rules, &[Event::Launch]);
        table.balls.clear();
        happen(&mut table, &mut rules, &[Event::Drain]);
        assert_eq!(rules.ball, 1);
        assert_eq!(table.balls.len(), 1, "back on the plunger");
        // Not twice.
        happen(&mut table, &mut rules, &[Event::Launch]);
        table.balls.clear();
        happen(&mut table, &mut rules, &[Event::Drain]);
        assert!(rules.serve_in.is_some(), "the bonus, then the next ball");
        rules.update(&mut table, &[], Input::default(), BONUS_FOR + 0.1);
        assert_eq!(rules.ball, 2);
    }

    #[test]
    fn the_skill_shot_is_the_lit_lane_first() {
        let (mut table, mut rules) = saloon();
        let (lane, _) = rules.skill.unwrap();
        // On the plunger, the flippers move it.
        rules.update(
            &mut table,
            &[],
            Input {
                right: true,
                left: false,
            },
            0.01,
        );
        let (moved, _) = rules.skill.unwrap();
        assert_eq!(moved, (lane + 1) % 3);
        let score = rules.score;
        happen(
            &mut table,
            &mut rules,
            &[
                Event::Launch,
                Event::Gate(0),
                Event::Rollover(TOP_LANES + moved),
            ],
        );
        assert!(rules.score >= score + 20_000);
    }

    #[test]
    fn the_game_ends_after_the_third_ball() {
        let (mut table, mut rules) = saloon();
        for ball in 1..=BALLS {
            assert_eq!(rules.ball, ball);
            table.balls.clear();
            happen(&mut table, &mut rules, &[Event::Drain]);
            rules.update(&mut table, &[], Input::default(), BONUS_FOR + 0.1);
        }
        assert!(rules.over);
        assert!(matches!(rules.screen(&table, false), Screen::Over { .. }));
    }
}
