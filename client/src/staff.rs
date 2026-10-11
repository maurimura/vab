//! The bar's staff: a bartender behind the counter and a cleaner mopping the floor, each one of
//! a few people who take turns by shift. Everyone in a room sees the same person doing the same
//! thing in the same place, though the room never says a word about them: their shift is
//! worked out stop by stop from the map and the time on the room's clock (`roomClock` in
//! index.html, which the room's welcome sets right), alike in every browser.
//!
//! Behind the bar is the floor next to a counter that players can't walk to, and the floor
//! joined to it; a map without any has no bartender. The cleaner goes about the floor players
//! walk, a few cells at a time.
//!
//! Players talk to them (talk/): the bartender comes over across the counter, the cleaner stops,
//! and each faces the player until the talk ends, then walks back to where the shift has them by
//! now. That's on the talking player's page only: everyone else sees the shift go on.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use bevy::sprite::Anchor;
use wasm_bindgen::prelude::*;
use world::{Map, cell_to_world};

use crate::nearby::{Kind, Usables};
use crate::player::{self, Walkable, Zoom};
use crate::room::{self, NameTag, NameText};

/// Someone on the staff: the name above their head and their sprite (tools/art/build_staff.py),
/// five frames side by side: standing, two walking and two working.
struct Person {
    name: &'static str,
    sprite: &'static str,
}

const BARTENDERS: &[Person] = &[
    Person {
        name: "Rosa",
        sprite: "characters/bartender_rosa.png",
    },
    Person {
        name: "Lou",
        sprite: "characters/bartender_lou.png",
    },
    Person {
        name: "Kenji",
        sprite: "characters/bartender_kenji.png",
    },
];

const CLEANERS: &[Person] = &[
    Person {
        name: "Doris",
        sprite: "characters/cleaner_doris.png",
    },
    Person {
        name: "Sal",
        sprite: "characters/cleaner_sal.png",
    },
    Person {
        name: "Marta",
        sprite: "characters/cleaner_marta.png",
    },
];

/// How long a shift is, in seconds: then the next person on the list takes over.
const SHIFT: f64 = 30.0 * 60.0;
/// Over this long at either end of a shift, the one leaving fades out and the next fades in.
const FADE: f64 = 1.5;
/// Walking speed in world pixels per second: an easy pace, under a player's 64.
const SPEED: f64 = 40.0;
/// A frame of a sprite, in pixels: a character as players are (16 by 32), with room either side.
const FRAME: UVec2 = UVec2::new(24, 32);
/// Frames a second: steps while walking, strokes of the rag or the mop while working.
const STEPS: f64 = 5.0;
const STROKES: f64 = 2.5;
/// Staff names are a warm yellow, players' white.
const NAME_COLOR: Color = Color::srgb(1.0, 0.85, 0.45);

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Milliseconds since 1970 on the room's clock: this browser's, set right by the welcome.
    #[wasm_bindgen(js_name = roomClock)]
    fn room_clock() -> f64;
}

/// Seconds on the room's clock (ahead of it in a browser test that asked: testing.rs).
fn now() -> f64 {
    #[cfg(feature = "test-hooks")]
    let ahead = crate::testing::clock_ahead();
    #[cfg(not(feature = "test-hooks"))]
    let ahead = 0.0;
    room_clock() / 1000.0 + ahead
}

pub struct StaffPlugin;

impl Plugin for StaffPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OnShift>()
            .init_resource::<Listening>()
            .add_systems(
                Update,
                (hire.run_if(resource_added::<Staff>), go_about).chain(),
            );
    }
}

/// A job on the staff.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Job {
    Bartender,
    Cleaner,
}

impl Job {
    fn people(self) -> &'static [Person] {
        match self {
            Job::Bartender => BARTENDERS,
            Job::Cleaner => CLEANERS,
        }
    }

    /// The cleaner's shifts change halfway through the bartender's, so both never change at once.
    fn shift_start(self, number: i64) -> f64 {
        let offset = match self {
            Job::Bartender => 0.0,
            Job::Cleaner => SHIFT / 2.0,
        };
        number as f64 * SHIFT + offset
    }

    /// Which shift is on at `now`.
    fn shift(self, now: f64) -> i64 {
        ((now - self.shift_start(0)) / SHIFT).floor() as i64
    }

    fn person(self, shift: i64) -> &'static Person {
        &self.people()[self.person_index(shift)]
    }

    /// Which of the people for the job is on `shift`.
    fn person_index(self, shift: i64) -> usize {
        shift.rem_euclid(self.people().len() as i64) as usize
    }

    /// How long they work at a stop (wiping the counter, mopping), at least and at most.
    fn working(self) -> (f64, f64) {
        match self {
            Job::Bartender => (4.0, 10.0),
            Job::Cleaner => (8.0, 20.0),
        }
    }

    /// How long they stand at a stop instead, and how often.
    fn standing(self) -> (f64, f64, f64) {
        match self {
            Job::Bartender => (2.0, 6.0, 0.35),
            Job::Cleaner => (3.0, 6.0, 0.2),
        }
    }
}

/// Where someone on the staff goes about their job.
#[derive(Debug)]
struct Rounds {
    walk: HashSet<IVec2>,
    /// Every cell they walk, and of those where they work, in order, for picking from alike in
    /// every browser.
    cells: Vec<IVec2>,
    work: Vec<IVec2>,
    /// How far the next stop is at most, in cells along x plus along y; none for anywhere.
    reach: Option<i32>,
    /// Where each shift starts, if always the same place.
    start: Option<IVec2>,
    /// Which way they face working at a cell, if it matters (left or not): the bartender, the
    /// counter.
    facing: HashMap<IVec2, bool>,
}

impl Rounds {
    fn new(walk: HashSet<IVec2>, work: impl Fn(IVec2) -> bool, reach: Option<i32>) -> Self {
        let mut cells: Vec<IVec2> = walk.iter().copied().collect();
        cells.sort_by_key(|cell| (cell.x, cell.y));
        let work: Vec<IVec2> = cells.iter().copied().filter(|&cell| work(cell)).collect();
        Self {
            walk,
            cells,
            work,
            reach,
            start: None,
            facing: HashMap::new(),
        }
    }
}

/// Where the staff go about their jobs, from the bar's map: a bartender's and a cleaner's
/// rounds, when the map has room for them.
#[derive(Resource, Debug)]
pub struct Staff {
    bartender: Option<Rounds>,
    cleaner: Option<Rounds>,
    /// The counter the bartender serves at: cells with the bartender's floor on one side and
    /// the players' on another.
    counter: Vec<IVec2>,
}

impl Staff {
    pub fn from_map(map: &Map, walkable: &Walkable) -> Self {
        let counters: HashSet<IVec2> = map
            .objects
            .iter()
            .filter(|placed| placed.tile == "objects/counter")
            .flat_map(|placed| placed.cells())
            .collect();
        let floor: HashSet<IVec2> = walkable.cells().collect();
        let Some(start) = walkable.start() else {
            return Self {
                bartender: None,
                cleaner: None,
                counter: Vec::new(),
            };
        };
        let open = joined(&floor, [start]);
        let counter_side = |cell: IVec2| {
            SIDES
                .into_iter()
                .find(|side| counters.contains(&(cell + *side)))
        };
        let at_counter = |cell: IVec2| counter_side(cell).is_some();
        let closed: HashSet<IVec2> = floor.difference(&open).copied().collect();
        let behind_bar = joined(&closed, closed.iter().copied().filter(|&c| at_counter(c)));
        let bartender = (!behind_bar.is_empty()).then(|| {
            let mut rounds = Rounds::new(behind_bar, at_counter, None);
            // Wiping the counter, facing it.
            for &cell in &rounds.work {
                let side = counter_side(cell).unwrap();
                rounds
                    .facing
                    .insert(cell, cell_to_world(side.x, side.y).x < 0.0);
            }
            // Each shift starts in the middle of the counter.
            let middle =
                rounds.work.iter().map(|c| c.as_vec2()).sum::<Vec2>() / rounds.work.len() as f32;
            rounds.start = rounds.work.iter().copied().min_by(|a, b| {
                let (da, db) = (a.as_vec2().distance(middle), b.as_vec2().distance(middle));
                da.total_cmp(&db)
            });
            rounds
        });
        let mut counter: Vec<IVec2> = match &bartender {
            Some(bar) => counters
                .iter()
                .copied()
                .filter(|&c| {
                    SIDES.iter().any(|side| bar.walk.contains(&(c + *side)))
                        && SIDES.iter().any(|side| open.contains(&(c + *side)))
                })
                .collect(),
            None => Vec::new(),
        };
        counter.sort_by_key(|cell| (cell.x, cell.y));
        Self {
            bartender,
            cleaner: Some(Rounds::new(open, |_| true, Some(5))),
            counter,
        }
    }

    /// The cells of the counter the bartender serves at.
    pub fn counter(&self) -> &[IVec2] {
        &self.counter
    }

    /// Where the bartender comes to talk to a player at `player`, next to the counter's cell
    /// `counter`: straight across it, or the nearest place to it behind the bar.
    pub fn bar_spot(&self, counter: IVec2, player: IVec2) -> Option<IVec2> {
        let bar = self.bartender.as_ref()?;
        let across = counter + (counter - player).clamp(IVec2::NEG_ONE, IVec2::ONE);
        if bar.walk.contains(&across) {
            return Some(across);
        }
        bar.work.iter().copied().min_by_key(|cell| {
            let to = *cell - counter;
            (to.length_squared(), cell.x, cell.y)
        })
    }

    fn rounds(&self, job: Job) -> Option<&Rounds> {
        match job {
            Job::Bartender => self.bartender.as_ref(),
            Job::Cleaner => self.cleaner.as_ref(),
        }
    }
}

const SIDES: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// Of `cells`, those joined to `from` side by side, as players walk from cell to cell.
fn joined(cells: &HashSet<IVec2>, from: impl IntoIterator<Item = IVec2>) -> HashSet<IVec2> {
    let mut found = HashSet::new();
    let mut queue: VecDeque<IVec2> = from.into_iter().filter(|c| cells.contains(c)).collect();
    found.extend(queue.iter().copied());
    while let Some(cell) = queue.pop_front() {
        for side in SIDES {
            let next = cell + side;
            if cells.contains(&next) && found.insert(next) {
                queue.push_back(next);
            }
        }
    }
    found
}

/// The shortest way from one cell to another over `cells`, cell centers in world pixels: side
/// to side, or corner to corner where both cells beside the corner are free too. None if the
/// two aren't joined.
fn route(cells: &HashSet<IVec2>, from: IVec2, to: IVec2) -> Option<Vec<Vec2>> {
    const MOVES: [IVec2; 8] = [
        IVec2::X,
        IVec2::NEG_X,
        IVec2::Y,
        IVec2::NEG_Y,
        IVec2::new(1, 1),
        IVec2::new(-1, -1),
        IVec2::new(1, -1),
        IVec2::new(-1, 1),
    ];
    // Tenths of a world pixel, whole, so every browser breaks ties alike.
    let length = |step: IVec2| (cell_to_world(step.x, step.y).length() * 10.0).round() as u32;
    let mut best = HashMap::from([(from, 0)]);
    let mut came_from = HashMap::new();
    let mut queue = BinaryHeap::from([Reverse((0, from.x, from.y))]);
    while let Some(Reverse((cost, x, y))) = queue.pop() {
        let cell = IVec2::new(x, y);
        if cell == to {
            let mut path = vec![to];
            while let Some(&previous) = came_from.get(path.last().unwrap()) {
                path.push(previous);
            }
            path.reverse();
            return Some(path.iter().map(|c| cell_to_world(c.x, c.y)).collect());
        }
        if cost > best[&cell] {
            continue;
        }
        for step in MOVES {
            let next = cell + step;
            let free = |c: IVec2| cells.contains(&c);
            if !free(next) || !free(cell + step.with_y(0)) || !free(cell + step.with_x(0)) {
                continue;
            }
            let cost = cost + length(step);
            if best.get(&next).is_none_or(|&known| cost < known) {
                best.insert(next, cost);
                came_from.insert(next, cell);
                queue.push(Reverse((cost, next.x, next.y)));
            }
        }
    }
    None
}

/// Dice for a stop on a shift, rolling the same numbers in every browser (SplitMix64).
struct Dice(u64);

impl Dice {
    fn new(job: Job, shift: i64, stop: u32) -> Self {
        let mut dice = Self(((job as u64) << 56) ^ (shift as u64).rotate_left(20) ^ stop as u64);
        dice.roll();
        dice
    }

    fn roll(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// From 0 up to 1.
    fn unit(&mut self) -> f64 {
        (self.roll() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, (least, most): (f64, f64)) -> f64 {
        least + (most - least) * self.unit()
    }

    fn pick(&mut self, cells: &[IVec2]) -> Option<IVec2> {
        (!cells.is_empty()).then(|| cells[(self.roll() % cells.len() as u64) as usize])
    }
}

/// What someone on the staff is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Doing {
    Standing,
    Walking,
    Working,
}

/// A stop on a shift: the walk there from the last one, then working or standing there.
#[derive(Clone, Debug, PartialEq)]
struct Stop {
    /// When they set off, in seconds on the room's clock.
    start: f64,
    /// The way there from the last stop, cell centers in world pixels, ending at this one.
    path: Vec<Vec2>,
    walk: f64,
    work: bool,
    stay: f64,
    /// Which way they face once there, if it matters (left or not).
    face: Option<bool>,
}

impl Stop {
    fn end(&self) -> f64 {
        self.start + self.walk + self.stay
    }

    fn cell(&self) -> IVec2 {
        world::world_to_cell(*self.path.last().unwrap())
    }

    /// Where they are at `now`, what they're doing, and which way they face (left or not), if
    /// the walk went sideways at all or the stop says.
    fn pose(&self, now: f64) -> (Vec2, Doing, Option<bool>) {
        let walked = ((now - self.start).max(0.0) * SPEED) as f32;
        let (at, walking, facing) = along(&self.path, walked);
        let doing = match (walking, self.work) {
            (true, _) => Doing::Walking,
            (false, true) => Doing::Working,
            (false, false) => Doing::Standing,
        };
        (
            at,
            doing,
            if walking {
                facing
            } else {
                self.face.or(facing)
            },
        )
    }
}

/// Where someone is `walked` world pixels along `path`, whether they're still walking it, and
/// which way they face (left or not) if it went sideways at all.
fn along(path: &[Vec2], mut walked: f32) -> (Vec2, bool, Option<bool>) {
    let mut facing = None;
    for leg in path.windows(2) {
        let way = leg[1] - leg[0];
        if way.x != 0.0 {
            facing = Some(way.x < 0.0);
        }
        if walked < way.length() {
            return (leg[0] + way * walked / way.length(), true, facing);
        }
        walked -= way.length();
    }
    (*path.last().unwrap(), false, facing)
}

/// The way from `from`, anywhere, to `to` over `cells`, ending in its middle or at `end` if said.
fn way(cells: &HashSet<IVec2>, from: Vec2, to: IVec2, end: Option<Vec2>) -> Vec<Vec2> {
    let target = end.unwrap_or(cell_to_world(to.x, to.y));
    let mut path = route(cells, world::world_to_cell(from), to).unwrap_or_default();
    if path.is_empty() {
        path = vec![from, target];
    }
    path[0] = from;
    *path.last_mut().unwrap() = target;
    path
}

/// Someone's shift: which one, and which of its stops they're at.
#[derive(Debug)]
struct Shift {
    number: i64,
    index: u32,
    stop: Stop,
}

impl Shift {
    /// The shift going on at `now`, worked out stop by stop from its start.
    fn at(job: Job, rounds: &Rounds, now: f64) -> Option<Self> {
        let number = job.shift(now);
        let mut dice = Dice::new(job, number, u32::MAX);
        let cell = rounds.start.or_else(|| dice.pick(&rounds.cells))?;
        // In place a moment as the shift starts, fading in.
        let first = Stop {
            start: job.shift_start(number),
            path: vec![cell_to_world(cell.x, cell.y)],
            walk: 0.0,
            work: false,
            stay: dice.between((FADE, 2.0 * FADE)),
            face: None,
        };
        let mut shift = Self {
            number,
            index: 0,
            stop: first,
        };
        shift.catch_up(job, rounds, now);
        Some(shift)
    }

    /// On to whichever stop is going on at `now`.
    fn catch_up(&mut self, job: Job, rounds: &Rounds, now: f64) {
        while now >= self.stop.end() {
            self.index += 1;
            self.stop = next_stop(job, rounds, self.number, self.index, &self.stop);
        }
    }
}

/// The stop after `last`: somewhere to work, or now and then just to stand, within reach.
fn next_stop(job: Job, rounds: &Rounds, shift: i64, index: u32, last: &Stop) -> Stop {
    let mut dice = Dice::new(job, shift, index);
    let from = last.cell();
    let (least, most, often) = job.standing();
    let work = dice.unit() >= often || rounds.work.is_empty();
    let pool = if work { &rounds.work } else { &rounds.cells };
    let near: Vec<IVec2> = pool
        .iter()
        .copied()
        .filter(|&c| {
            c != from
                && rounds
                    .reach
                    .is_none_or(|r| (c - from).abs().element_sum() <= r)
        })
        .collect();
    let to = dice.pick(&near).or_else(|| dice.pick(pool)).unwrap_or(from);
    let path =
        route(&rounds.walk, from, to).unwrap_or_else(|| vec![last.path[last.path.len() - 1]]);
    let walk = path
        .windows(2)
        .map(|leg| leg[0].distance(leg[1]) as f64)
        .sum::<f64>()
        / SPEED;
    Stop {
        start: last.end(),
        path,
        walk,
        work,
        stay: dice.between(if work { job.working() } else { (least, most) }),
        face: rounds.facing.get(&to).copied().filter(|_| work),
    }
}

/// Someone on the staff in the bar, the shift they're on, and where they're drawn.
#[derive(Component)]
struct Member {
    job: Job,
    shift: Option<Shift>,
    feet: Vec2,
    aside: Option<Aside>,
}

/// Off their shift a moment for a talk with the player, on this page only: over to the player
/// (or staying put), then back to the shift.
struct Aside {
    path: Vec<Vec2>,
    /// When they set off, on the room's clock, and how fast they go.
    start: f64,
    speed: f64,
    /// Back to the shift, rather than over to the player.
    back: bool,
}

impl Aside {
    fn new(path: Vec<Vec2>, now: f64, back: bool) -> Self {
        let speed = if back { 2.0 * SPEED } else { SPEED };
        Self {
            path,
            start: now,
            speed,
            back,
        }
    }
}

/// Who on the staff the player is talking to, if anyone (talk/).
#[derive(Resource, Default)]
pub struct Listening(pub Option<Listener>);

pub struct Listener {
    pub job: Job,
    /// Where they come to talk: across the counter from the player. None: where they are.
    pub spot: Option<IVec2>,
    /// The player's feet, which they face.
    pub player: Vec2,
}

/// Who's on each job now, for talking to them.
#[derive(Resource, Default)]
pub struct OnShift {
    pub bartender: Option<OnDuty>,
    pub cleaner: Option<OnDuty>,
}

pub struct OnDuty {
    pub name: &'static str,
    /// Which of the people for the job (staff.rs lists them), for what they say.
    pub person: usize,
    /// Where they're drawn.
    pub feet: Vec2,
}

/// Puts the staff in the bar once its map is here.
fn hire(
    mut commands: Commands,
    staff: Res<Staff>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(FRAME, 5, 1, None, None));
    for job in [Job::Bartender, Job::Cleaner] {
        if staff.rounds(job).is_none() {
            continue;
        }
        let tag = NameTag(room::spawn_name(&mut commands, String::new()));
        let atlas = TextureAtlas {
            layout: layout.clone(),
            index: 0,
        };
        commands.spawn((
            Member {
                job,
                shift: None,
                feet: Vec2::ZERO,
                aside: None,
            },
            Sprite::from_atlas_image(Handle::default(), atlas),
            Anchor::BOTTOM_CENTER,
            Transform::default(),
            Visibility::Hidden,
            tag,
        ));
    }
}

/// Each on their shift: where they are and what they're doing now, and who's on.
#[allow(clippy::too_many_arguments)]
fn go_about(
    staff: Option<Res<Staff>>,
    asset_server: Res<AssetServer>,
    zoom: Res<Zoom>,
    listening: Res<Listening>,
    mut on_shift: ResMut<OnShift>,
    mut usables: Option<ResMut<Usables>>,
    mut members: Query<(
        &mut Member,
        &mut Sprite,
        &mut Transform,
        &mut Visibility,
        &NameTag,
    )>,
    mut names: Query<(&mut Text, &mut TextColor, &mut BackgroundColor), With<NameText>>,
) {
    let Some(staff) = staff else {
        return;
    };
    let now = now();
    #[cfg(feature = "test-hooks")]
    let mut report = Vec::new();
    for (mut member, mut sprite, mut transform, mut visibility, tag) in &mut members {
        let job = member.job;
        let Some(rounds) = staff.rounds(job) else {
            continue;
        };
        // A new shift, or the clock went back (the welcome set it right).
        let stale = |shift: &Shift| shift.number != job.shift(now) || now < shift.stop.start;
        if member.shift.as_ref().is_none_or(stale) {
            member.shift = Shift::at(job, rounds, now);
            let Some(shift) = &member.shift else {
                continue;
            };
            let person = job.person(shift.number);
            sprite.image = asset_server.load(person.sprite);
            if let Ok((mut text, _, _)) = names.get_mut(tag.0) {
                text.0 = person.name.to_string();
            }
            *visibility = Visibility::Inherited;
        }
        let Some(shift) = member.shift.as_mut() else {
            continue;
        };
        shift.catch_up(job, rounds, now);
        let scheduled = shift.stop.pose(now);
        let number = shift.number;
        let (feet, doing, facing) = step_aside(&mut member, rounds, &listening, scheduled, now);
        member.feet = feet;
        let frame = match doing {
            Doing::Standing => 0,
            Doing::Walking => 1 + ((now * STEPS) as i64).rem_euclid(2) as usize,
            Doing::Working => 3 + ((now * STROKES) as i64).rem_euclid(2) as usize,
        };
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = frame;
        }
        if let Some(left) = facing {
            sprite.flip_x = left;
        }
        let duty = Some(OnDuty {
            name: job.person(number).name,
            person: job.person_index(number),
            feet,
        });
        match job {
            Job::Bartender => on_shift.bartender = duty,
            Job::Cleaner => {
                on_shift.cleaner = duty;
                // The player can talk to the cleaner from next to them, wherever they are.
                if let Some(usables) = usables.as_mut() {
                    usables.place(Kind::Cleaner, world::world_to_cell(feet));
                }
            }
        }
        // Fading in as a shift starts, out as it ends.
        let start = job.shift_start(number);
        let alpha = ((now - start).min(start + SHIFT - now) / FADE).clamp(0.0, 1.0) as f32;
        sprite.color = Color::WHITE.with_alpha(alpha);
        if let Ok((_, mut color, mut background)) = names.get_mut(tag.0) {
            color.0 = NAME_COLOR.with_alpha(alpha);
            background.0 = background.0.with_alpha(0.5 * alpha);
        }
        transform.translation = player::translation(feet, zoom.0);
        #[cfg(feature = "test-hooks")]
        if let Some(shift) = &member.shift {
            let (cell, to) = (world::world_to_cell(feet), shift.stop.cell());
            report.push(serde_json::json!({
                "job": format!("{job:?}"),
                "name": job.person(number).name,
                "shift": number,
                "stop": shift.index,
                "to": [to.x, to.y],
                "cell": [cell.x, cell.y],
                "doing": format!("{doing:?}"),
                "aside": member.aside.as_ref().map(|aside| if aside.back { "back" } else { "talking" }),
                "secondsLeft": start + SHIFT - now,
            }));
        }
    }
    #[cfg(feature = "test-hooks")]
    crate::testing::report("staff", serde_json::Value::Array(report));
}

/// Where someone is and what they're doing: where the shift has them, unless the player is
/// talking to them, or they're on their way back from that.
fn step_aside(
    member: &mut Member,
    rounds: &Rounds,
    listening: &Listening,
    scheduled: (Vec2, Doing, Option<bool>),
    now: f64,
) -> (Vec2, Doing, Option<bool>) {
    let listener = listening.0.as_ref().filter(|l| l.job == member.job);
    let talking = member.aside.as_ref().is_some_and(|aside| !aside.back);
    match listener {
        // Over to talk, from wherever they are.
        Some(listener) if !talking => {
            let path = match listener.spot {
                Some(spot) => way(&rounds.walk, member.feet, spot, None),
                None => vec![member.feet],
            };
            member.aside = Some(Aside::new(path, now, false));
        }
        // The talk is over: back to where the shift has them by now.
        None if talking => member.aside = Some(back(rounds, member.feet, scheduled.0, now)),
        _ => {}
    }
    let Some(aside) = &member.aside else {
        return scheduled;
    };
    let (at, walking, facing) = along(&aside.path, ((now - aside.start) * aside.speed) as f32);
    if walking {
        return (at, Doing::Walking, facing);
    }
    match listener {
        Some(listener) => (at, Doing::Standing, Some(listener.player.x < at.x)),
        // Back: on with the shift once there, or after it again if it went on meanwhile.
        None if at.distance(scheduled.0) < 1.0 => {
            member.aside = None;
            scheduled
        }
        None => {
            member.aside = Some(back(rounds, at, scheduled.0, now));
            (at, Doing::Walking, facing)
        }
    }
}

fn back(rounds: &Rounds, from: Vec2, to: Vec2, now: f64) -> Aside {
    let path = way(&rounds.walk, from, world::world_to_cell(to), Some(to));
    Aside::new(path, now, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::Placed;

    /// A bar 5 cells wide: a counter along row 0 and down both sides, behind it rows -2 and -1,
    /// the floor in front rows 1 to 4.
    fn bar() -> (Map, Walkable) {
        let mut map = Map::default();
        for x in -3..=3 {
            for y in -2..=4 {
                map.floor.push(Placed {
                    x,
                    y,
                    tile: "floor/wood".into(),
                    game: None,
                });
                let counter = (y == 0 && x.abs() <= 3) || (y < 0 && x.abs() == 3);
                if counter {
                    map.objects.push(Placed {
                        x,
                        y,
                        tile: "objects/counter".into(),
                        game: None,
                    });
                }
            }
        }
        let walkable = Walkable::from_map(&map);
        (map, walkable)
    }

    #[test]
    fn the_bartender_keeps_behind_the_counter_and_the_cleaner_to_the_floor() {
        let (map, walkable) = bar();
        let staff = Staff::from_map(&map, &walkable);
        let bartender = staff.bartender.as_ref().unwrap();
        let cleaner = staff.cleaner.as_ref().unwrap();
        assert!(bartender.cells.iter().all(|c| c.y < 0 && c.x.abs() < 3));
        assert_eq!(bartender.cells.len(), 10);
        // Working by the counter: along it, and by the sides.
        assert!(bartender.work.contains(&IVec2::new(0, -1)));
        assert!(bartender.work.contains(&IVec2::new(-2, -2)));
        assert!(!bartender.work.contains(&IVec2::new(0, -2)));
        assert_eq!(bartender.start, Some(IVec2::new(0, -1)));
        // Facing the counter: in front of them on screen down to the left, or at the side.
        assert!(bartender.facing[&IVec2::new(0, -1)]);
        assert!(bartender.facing[&IVec2::new(-2, -2)]);
        assert!(!bartender.facing[&IVec2::new(2, -2)]);
        assert!(cleaner.cells.iter().all(|c| c.y > 0));
        assert_eq!(cleaner.cells.len(), 28);

        for (job, rounds) in [(Job::Bartender, bartender), (Job::Cleaner, cleaner)] {
            let now = 1_760_000_000.0;
            let shift = Shift::at(job, rounds, now).unwrap();
            let mut stop = Shift::at(job, rounds, job.shift_start(shift.number))
                .unwrap()
                .stop;
            for index in 1..200 {
                let next = next_stop(job, rounds, shift.number, index, &stop);
                // Each stop sets off from the last, when it's done, and walks the rounds.
                assert_eq!(next.start, stop.end());
                assert_eq!(next.path[0], *stop.path.last().unwrap());
                assert!(
                    next.path
                        .iter()
                        .all(|p| rounds.walk.contains(&world::world_to_cell(*p)))
                );
                assert!(!next.work || rounds.work.contains(&next.cell()));
                stop = next;
            }
        }
    }

    #[test]
    fn a_shift_comes_out_the_same_worked_out_from_its_start_or_followed_along() {
        let (map, walkable) = bar();
        let staff = Staff::from_map(&map, &walkable);
        let rounds = staff.cleaner.as_ref().unwrap();
        let job = Job::Cleaner;
        let start = job.shift_start(978_000);
        let mut followed = Shift::at(job, rounds, start).unwrap();
        for second in 0..(SHIFT as i64) {
            let now = start + second as f64 + 0.25;
            followed.catch_up(job, rounds, now);
            if second % 97 == 0 {
                let fresh = Shift::at(job, rounds, now).unwrap();
                assert_eq!((fresh.index, &fresh.stop), (followed.index, &followed.stop));
                assert_eq!(fresh.stop.pose(now).0, followed.stop.pose(now).0);
            }
        }
        // A different shift goes another way.
        let next = Shift::at(job, rounds, start + SHIFT + 60.0).unwrap();
        let again = Shift::at(job, rounds, start + 60.0).unwrap();
        assert_ne!(next.stop.path, again.stop.path);
    }

    #[test]
    fn shifts_take_turns() {
        let now = 1_760_000_000.0;
        let names = |job: Job| -> Vec<&str> {
            (0..4)
                .map(|n| job.person(job.shift(now + n as f64 * SHIFT)).name)
                .collect()
        };
        let bartenders = names(Job::Bartender);
        assert!(bartenders.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(bartenders[0], bartenders[3]);
        // The cleaner changes halfway through the bartender's shift.
        let changes = Job::Cleaner.shift_start(Job::Cleaner.shift(now) + 1);
        assert_eq!(
            Job::Bartender.shift(changes),
            Job::Bartender.shift(changes - 1.0)
        );
        assert_ne!(
            Job::Cleaner.shift(changes),
            Job::Cleaner.shift(changes - 1.0)
        );
    }

    #[test]
    fn routes_go_around_what_is_in_the_way() {
        // A wall down the middle of a 3 by 3 square but for its last cell.
        let cells: HashSet<IVec2> = (0..3)
            .flat_map(|x| (0..3).map(move |y| IVec2::new(x, y)))
            .filter(|c| c.x != 1 || c.y == 2)
            .collect();
        let path = route(&cells, IVec2::new(0, 0), IVec2::new(2, 0)).unwrap();
        let path: Vec<IVec2> = path.iter().map(|p| world::world_to_cell(*p)).collect();
        assert_eq!(path.first(), Some(&IVec2::new(0, 0)));
        assert_eq!(path.last(), Some(&IVec2::new(2, 0)));
        assert!(path.contains(&IVec2::new(1, 2)));
        // Never across a corner beside something in the way.
        for leg in path.windows(2) {
            let step = leg[1] - leg[0];
            assert!(cells.contains(&(leg[0] + step.with_y(0))));
            assert!(cells.contains(&(leg[0] + step.with_x(0))));
        }
        assert_eq!(route(&cells, IVec2::new(0, 0), IVec2::new(5, 5)), None);
    }
}
