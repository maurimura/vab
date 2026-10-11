//! Talking to the staff (staff.rs). At the counter, or next to the cleaner, E (Talk on a touch
//! screen) starts a conversation in a box at the bottom right, in the page's language or the
//! one picked in it (lines.rs): they say something, and the player answers with 1 to 9, a click
//! or a tap. Esc, E again or walking away ends it.
//!
//! The bartender shows players around: what there is to play, by kind of game, and who's playing
//! now (picking one shows the way there: guide.rs), how to play with friends, and the controls.
//! The cleaner has tips, and points the way to the bartender. On a first visit the way to the
//! bartender shows once the welcome panel closes. Game titles and names stay as they are in
//! every language.

mod guide;
mod lines;

use std::cell::RefCell;

use bevy::ecs::system::SystemParam;
use bevy::input::InputSystems;
use bevy::prelude::*;
use wasm_bindgen::prelude::*;
use world::{Genre, Placed, cell_to_world, world_to_cell};

use crate::Mode;
use crate::cabinets::{self, Cabinets};
use crate::chat::{chat_closed, type_in_chat};
use crate::darts::{self, Dartboards};
use crate::help::{self, ShowHelp, close_panel, help_closed};
use crate::hockey::{self, HockeyTables};
use crate::nearby::{self, Kind, Nearby, hint_label, place_hint};
use crate::pinball::{self, PinballMachines};
use crate::player::Player;
use crate::pool::{self, PoolTables};
use crate::settings::settings_closed;
use crate::shuffleboard::{self, ShuffleboardTables};
use crate::staff::{Job, Listener, Listening, OnShift, Staff};
use crate::touch::{Touch, TouchButton, read_touches};
use guide::{Guide, Place};
use lines::{LANGUAGES, Lines};

const GOLD: Color = Color::srgb(1.0, 0.85, 0.45);
const DIM: Color = Color::srgba(1.0, 1.0, 1.0, 0.6);
const HOVERED: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Where the hint to talk to someone sits, in world pixels from their feet: over their name.
const OVER_HEAD: f32 = 50.0;
/// Where the marker showing the way to the counter sits, from the middle of its cell.
const OVER_COUNTER: f32 = 22.0;
/// The most choices at once: 1 to 9.
const MOST: usize = 9;
const DIGITS: [KeyCode; MOST] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];
const NUMPAD: [KeyCode; MOST] = [
    KeyCode::Numpad1,
    KeyCode::Numpad2,
    KeyCode::Numpad3,
    KeyCode::Numpad4,
    KeyCode::Numpad5,
    KeyCode::Numpad6,
    KeyCode::Numpad7,
    KeyCode::Numpad8,
    KeyCode::Numpad9,
];

thread_local! {
    /// The page's language, or the one the player picked before, until the conversation takes it.
    static LANGUAGE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Called by index.html as it starts, with the language the player picked before (a cookie
/// keeps it) or the browser's, e.g. "es-AR".
#[wasm_bindgen]
pub fn talk_language(code: String) {
    LANGUAGE.set(Some(code));
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Remembers the language the player picked, for their next visit.
    #[wasm_bindgen(js_name = rememberLanguage)]
    fn remember_language(code: &str);
}

pub struct TalkPlugin;

impl Plugin for TalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Talk>()
            .add_plugins(guide::plugin)
            .add_systems(Startup, spawn)
            // E and the numbers are the conversation's while it's up, and nothing else's: after
            // the chat and the welcome panel have had them, and before anything else does.
            .add_systems(
                PreUpdate,
                keys.after(InputSystems)
                    .after(type_in_chat)
                    .after(close_panel)
                    .after(read_touches)
                    .after(nearby::find)
                    .run_if(in_state(Mode::Walking))
                    .run_if(chat_closed)
                    .run_if(help_closed)
                    .run_if(settings_closed),
            )
            .add_systems(
                Update,
                (
                    language,
                    click,
                    follow,
                    show,
                    light_answers,
                    hint.run_if(in_state(Mode::Walking)),
                    first_visit.run_if(in_state(Mode::Walking).and_then(help_closed)),
                )
                    .chain(),
            )
            .add_systems(OnExit(Mode::Walking), |mut talk: ResMut<Talk>| {
                talk.open = None
            });
        #[cfg(feature = "test-hooks")]
        app.add_systems(Last, |talk: Res<Talk>, guide: Res<Guide>| {
            crate::testing::report("talk", report(&talk, &guide));
        });
    }
}

/// The conversation with someone on the staff, if there is one, and the language it's in.
#[derive(Resource)]
pub struct Talk {
    lines: &'static Lines,
    open: Option<Conversation>,
    /// Which of the cleaner's tips comes next.
    tip: usize,
}

impl Default for Talk {
    fn default() -> Self {
        Self {
            lines: LANGUAGES[0],
            open: None,
            tip: 0,
        }
    }
}

struct Conversation {
    job: Job,
    person: usize,
    name: &'static str,
    /// Where the bartender comes to talk, across the counter.
    spot: Option<IVec2>,
    topic: Topic,
    /// What they're saying, once worked out for the topic.
    said: Option<Said>,
}

/// What the conversation is on.
#[derive(Clone, Debug, PartialEq)]
enum Topic {
    Hello,
    /// What there is to play, by kind.
    Play,
    Shelf(Shelf),
    /// Where they're showing the way to.
    There(String),
    Busy,
    Friends,
    Invite,
    Controls,
    Languages,
    Tip,
    /// The cleaner, on who can show the player around.
    Bartender,
}

/// A kind of thing to play: cabinets' games of a genre (or none), or the tables.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shelf {
    Games(Option<Genre>),
    Tables,
}

/// The genres, in the order the bartender lists them, and their place in `Lines::genres`.
const GENRES: [(Option<Genre>, usize); 6] = [
    (Some(Genre::Fighting), 0),
    (Some(Genre::Action), 1),
    (Some(Genre::Racing), 2),
    (Some(Genre::Sports), 3),
    (Some(Genre::Classic), 4),
    (None, 5),
];

/// What someone on the staff says, and what the player can answer.
#[derive(Debug, PartialEq)]
struct Said {
    line: String,
    choices: Vec<(String, Then)>,
}

/// What an answer leads to.
#[derive(Clone, Debug, PartialEq)]
enum Then {
    Topic(Topic),
    /// Shows the way somewhere, then goes on to the topic.
    Guide(Place, Topic),
    /// The controls panel (help.rs), ending the conversation.
    Help,
    Language(usize),
    Bye,
}

/// Something in the bar to talk about: where it is, what kind it is, and how many play there.
struct Spot {
    place: Place,
    genre: Option<Genre>,
    playing: u32,
}

/// What there is in the bar, for what the staff say about it.
struct Bar {
    /// Each game on a cabinet, at its cabinet nearest the player (or nearest with someone
    /// playing, when anyone is), with how many play it in all.
    games: Vec<Spot>,
    cabinets: usize,
    /// Each kind of table there is (its place in `Lines::tables`), the one nearest the player.
    tables: Vec<(usize, Place)>,
    /// The bartender's name, and the way to the counter.
    bartender: Option<(&'static str, Place)>,
    touch: bool,
}

/// What the staff say about the bar, gathered when they say it.
#[derive(SystemParam)]
struct Things<'w, 's> {
    cabinets: Option<Res<'w, Cabinets>>,
    pool: Option<Res<'w, PoolTables>>,
    hockey: Option<Res<'w, HockeyTables>>,
    shuffleboard: Option<Res<'w, ShuffleboardTables>>,
    darts: Option<Res<'w, Dartboards>>,
    pinball: Option<Res<'w, PinballMachines>>,
    staff: Option<Res<'w, Staff>>,
    on_shift: Res<'w, OnShift>,
    touch: Res<'w, Touch>,
    player: Query<'w, 's, &'static Player>,
}

impl Things<'_, '_> {
    fn bar(&self, lines: &Lines) -> Bar {
        let feet = self
            .player
            .single()
            .map_or(Vec2::ZERO, |player| player.feet);
        let away = |cell: IVec2| cell_to_world(cell.x, cell.y).distance_squared(feet);
        let mut games: Vec<Spot> = Vec::new();
        let all = self
            .cabinets
            .as_ref()
            .map_or(&[][..], |cabinets| cabinets.all());
        for (cell, game) in all {
            let playing = cabinets::people_at(*cell).0;
            let place = Place {
                label: game.title.clone(),
                above: cell_to_world(cell.x, cell.y) + Vec2::Y * cabinets::HINT_HEIGHT,
                cells: vec![*cell],
            };
            let Some(spot) = games.iter_mut().find(|spot| spot.place.label == game.title) else {
                games.push(Spot {
                    place,
                    genre: game.genre,
                    playing,
                });
                continue;
            };
            // Show the way to the nearest cabinet, or the nearest one someone plays at.
            let busy = |playing: u32| playing > 0;
            let nearer = away(*cell) < away(spot.place.cells[0]);
            if (busy(playing) && !busy(spot.playing))
                || (busy(playing) == busy(spot.playing) && nearer)
            {
                spot.place = place;
            }
            spot.playing += playing;
        }
        games.sort_by(|a, b| a.place.label.cmp(&b.place.label));
        let mut tables = Vec::new();
        let kinds: [(usize, Option<&[Placed]>, f32); 5] = [
            (0, self.pool.as_ref().map(|t| t.placed()), pool::HINT_HEIGHT),
            (
                1,
                self.hockey.as_ref().map(|t| t.placed()),
                hockey::HINT_HEIGHT,
            ),
            (
                2,
                self.shuffleboard.as_ref().map(|t| t.placed()),
                shuffleboard::HINT_HEIGHT,
            ),
            (
                3,
                self.darts.as_ref().map(|t| t.placed()),
                darts::HINT_HEIGHT,
            ),
            (
                4,
                self.pinball.as_ref().map(|t| t.placed()),
                pinball::HINT_HEIGHT,
            ),
        ];
        for (index, placed, height) in kinds {
            let nearest = placed.unwrap_or_default().iter().min_by(|a, b| {
                let (da, db) = (a.center().distance(feet), b.center().distance(feet));
                da.total_cmp(&db)
            });
            if let Some(table) = nearest {
                let place = Place {
                    label: lines.tables[index].to_string(),
                    above: table.center() + Vec2::Y * height,
                    cells: table.cells().collect(),
                };
                tables.push((index, place));
            }
        }
        Bar {
            games,
            cabinets: all.len(),
            tables,
            bartender: self.bartender(lines),
            touch: self.touch.is_on(),
        }
    }

    /// The bartender on shift, and the way to the counter nearest them.
    fn bartender(&self, lines: &Lines) -> Option<(&'static str, Place)> {
        let duty = self.on_shift.bartender.as_ref()?;
        let counter = self.staff.as_ref()?.counter();
        let nearest = counter.iter().copied().min_by(|a, b| {
            let (da, db) = (
                cell_to_world(a.x, a.y).distance(duty.feet),
                cell_to_world(b.x, b.y).distance(duty.feet),
            );
            da.total_cmp(&db)
        })?;
        let place = Place {
            label: lines.talk_to.replace("{name}", duty.name),
            above: cell_to_world(nearest.x, nearest.y) + Vec2::Y * OVER_COUNTER,
            cells: counter.to_vec(),
        };
        Some((duty.name, place))
    }
}

/// What someone says on a topic, and what the player can answer.
fn say(conversation: &Conversation, lines: &Lines, bar: &Bar, tip: usize) -> Said {
    let back = |to: Topic| (lines.back.to_string(), Then::Topic(to));
    let ask = |text: &str, to: Topic| (text.to_string(), Then::Topic(to));
    let (line, mut choices) = match &conversation.topic {
        Topic::Hello if conversation.job == Job::Bartender => (
            lines.bartender_hello[conversation.person].to_string(),
            vec![
                ask(lines.ask_play, Topic::Play),
                ask(lines.ask_busy, Topic::Busy),
                ask(lines.ask_friends, Topic::Friends),
                ask(lines.ask_controls, Topic::Controls),
                ask(lines.ask_language, Topic::Languages),
            ],
        ),
        Topic::Hello => {
            let mut choices = vec![ask(lines.ask_tip, Topic::Tip)];
            if let Some((_, counter)) = &bar.bartender {
                let then = Then::Guide(counter.clone(), Topic::Bartender);
                choices.push((lines.ask_bartender.to_string(), then));
            }
            choices.push(ask(lines.ask_language, Topic::Languages));
            (
                lines.cleaner_hello[conversation.person].to_string(),
                choices,
            )
        }
        Topic::Play => {
            let mut choices: Vec<_> = GENRES
                .into_iter()
                .filter(|(genre, _)| bar.games.iter().any(|spot| spot.genre == *genre))
                .map(|(genre, index)| ask(lines.genres[index], Topic::Shelf(Shelf::Games(genre))))
                .collect();
            if !bar.tables.is_empty() {
                choices.push(ask(lines.genres[6], Topic::Shelf(Shelf::Tables)));
            }
            choices.push(back(Topic::Hello));
            let count = bar.cabinets.to_string();
            (lines.genres_line.replace("{count}", &count), choices)
        }
        Topic::Shelf(shelf) => {
            let places: Vec<&Place> = match shelf {
                Shelf::Games(genre) => bar
                    .games
                    .iter()
                    .filter(|spot| spot.genre == *genre)
                    .map(|spot| &spot.place)
                    .collect(),
                Shelf::Tables => bar.tables.iter().map(|(_, place)| place).collect(),
            };
            let mut choices: Vec<_> = places
                .into_iter()
                .take(MOST - 1)
                .map(|place| {
                    let then = Then::Guide(place.clone(), Topic::There(place.label.clone()));
                    (place.label.clone(), then)
                })
                .collect();
            choices.push(back(Topic::Play));
            (lines.genre_line.to_string(), choices)
        }
        Topic::There(what) => (
            lines.there.replace("{game}", what),
            vec![(lines.thanks.to_string(), Then::Bye), back(Topic::Play)],
        ),
        Topic::Busy => {
            let mut busy: Vec<&Spot> = bar.games.iter().filter(|spot| spot.playing > 0).collect();
            busy.sort_by_key(|spot| std::cmp::Reverse(spot.playing));
            if busy.is_empty() {
                let choices = vec![ask(lines.ask_play, Topic::Play), back(Topic::Hello)];
                (lines.busy_none.to_string(), choices)
            } else {
                let mut choices: Vec<_> = busy
                    .into_iter()
                    .take(MOST - 1)
                    .map(|spot| {
                        let entry = lines
                            .busy_entry
                            .replace("{game}", &spot.place.label)
                            .replace("{count}", &spot.playing.to_string());
                        let then =
                            Then::Guide(spot.place.clone(), Topic::There(spot.place.label.clone()));
                        (entry, then)
                    })
                    .collect();
                choices.push(back(Topic::Hello));
                (lines.busy_line.to_string(), choices)
            }
        }
        Topic::Friends => (
            lines.friends.to_string(),
            vec![ask(lines.ask_invite, Topic::Invite), back(Topic::Hello)],
        ),
        Topic::Invite => (lines.invite.to_string(), vec![back(Topic::Hello)]),
        Topic::Controls => {
            let line = if bar.touch {
                lines.controls_touch
            } else {
                lines.controls
            };
            let mut choices = vec![back(Topic::Hello)];
            // The controls panel lists the keys; a touch screen's buttons say what they do.
            if !bar.touch {
                choices.insert(0, (lines.ask_all_controls.to_string(), Then::Help));
            }
            (line.to_string(), choices)
        }
        Topic::Languages => {
            let mut choices: Vec<_> = LANGUAGES
                .iter()
                .enumerate()
                .map(|(index, other)| (other.language.to_string(), Then::Language(index)))
                .collect();
            choices.push(back(Topic::Hello));
            (lines.language_line.to_string(), choices)
        }
        Topic::Tip => (
            lines.tips[tip % lines.tips.len()].to_string(),
            vec![ask(lines.another_tip, Topic::Tip), back(Topic::Hello)],
        ),
        Topic::Bartender => {
            let name = bar.bartender.as_ref().map_or("", |(name, _)| name);
            (
                lines.bartender_is.replace("{name}", name),
                vec![(lines.thanks.to_string(), Then::Bye), back(Topic::Hello)],
            )
        }
    };
    choices.truncate(MOST);
    Said { line, choices }
}

/// The conversation's box, at the bottom right.
#[derive(Component)]
struct TalkBox;

/// One of the answers in it, by its place in the list (on a touch screen, a TouchButton).
#[derive(Component)]
struct Answer(usize);

#[derive(Component)]
struct Hint;

fn spawn(mut commands: Commands) {
    commands.spawn((
        TalkBox,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            bottom: Val::Px(16.0),
            width: Val::Px(420.0),
            max_width: Val::Percent(92.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(14.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.92)),
        BorderColor::all(GOLD),
        // Over the chat.
        GlobalZIndex(2),
        Visibility::Hidden,
    ));
    commands.spawn((Hint, hint_label()));
}

fn text(content: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(content),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// Takes the language the page says, once it says it.
fn language(
    mut talk: ResMut<Talk>,
    buttons: Query<(&TouchButton, &Children)>,
    mut labels: Query<&mut Text>,
) {
    let Some(code) = LANGUAGE.take() else {
        return;
    };
    set_language(&mut talk, lines::for_language(&code));
    label_talk_button(talk.lines, &buttons, &mut labels);
}

fn set_language(talk: &mut Talk, lines: &'static Lines) {
    talk.lines = lines;
    if let Some(conversation) = &mut talk.open {
        conversation.said = None;
    }
}

/// The touch screen's Talk button, in the conversation's language.
fn label_talk_button(
    lines: &Lines,
    buttons: &Query<(&TouchButton, &Children)>,
    labels: &mut Query<&mut Text>,
) {
    for (button, children) in buttons {
        if *button != TouchButton::Talk {
            continue;
        }
        for child in children {
            if let Ok(mut label) = labels.get_mut(*child) {
                label.0 = lines.talk.to_string();
            }
        }
    }
}

/// E (or Talk) next to someone on the staff starts a conversation; while one is up, the numbers
/// (or a tap on an answer) answer, and Esc or E ends it. Keys it takes are nobody else's.
#[allow(clippy::too_many_arguments)]
fn keys(
    mut talk: ResMut<Talk>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    nearby: Res<Nearby>,
    on_shift: Res<OnShift>,
    staff: Option<Res<Staff>>,
    player: Single<&Player>,
    mut guide: ResMut<Guide>,
    mut show_help: MessageWriter<ShowHelp>,
    buttons: Query<(&TouchButton, &Children)>,
    mut labels: Query<&mut Text>,
) {
    let e = keys.just_pressed(KeyCode::KeyE);
    let Some(conversation) = &talk.open else {
        let job = match nearby.get() {
            Some((Kind::Bartender, _)) => Job::Bartender,
            Some((Kind::Cleaner, _)) => Job::Cleaner,
            _ => return,
        };
        if !e && !touch.tapped(TouchButton::Talk) {
            return;
        }
        keys.clear_just_pressed(KeyCode::KeyE);
        let duty = match job {
            Job::Bartender => on_shift.bartender.as_ref(),
            Job::Cleaner => on_shift.cleaner.as_ref(),
        };
        let Some(duty) = duty else {
            return;
        };
        let spot = match (job, nearby.get(), &staff) {
            (Job::Bartender, Some((_, counter)), Some(staff)) => {
                staff.bar_spot(counter, world_to_cell(player.feet))
            }
            _ => None,
        };
        talk.open = Some(Conversation {
            job,
            person: duty.person,
            name: duty.name,
            spot,
            topic: Topic::Hello,
            said: None,
        });
        return;
    };
    let mut chosen = None;
    let count = conversation
        .said
        .as_ref()
        .map_or(0, |said| said.choices.len());
    for index in 0..count {
        if keys.any_just_pressed([DIGITS[index], NUMPAD[index]])
            || touch.tapped(TouchButton::Choice(index as u8))
        {
            chosen = conversation
                .said
                .as_ref()
                .map(|said| said.choices[index].1.clone());
        }
    }
    // A touch screen's last answer is Bye.
    if e || keys.just_pressed(KeyCode::Escape) || touch.tapped(TouchButton::Choice(count as u8)) {
        chosen = Some(Then::Bye);
    }
    keys.clear_just_pressed(KeyCode::KeyE);
    keys.clear_just_pressed(KeyCode::Escape);
    for key in DIGITS.into_iter().chain(NUMPAD) {
        keys.clear_just_pressed(key);
    }
    if let Some(then) = chosen {
        answer(&mut talk, then, &mut guide, &mut show_help);
        label_talk_button(talk.lines, &buttons, &mut labels);
    }
}

/// A click on an answer, with a mouse (a touch screen's are TouchButtons).
fn click(
    mut talk: ResMut<Talk>,
    answers: Query<(&Interaction, &Answer), Changed<Interaction>>,
    mut guide: ResMut<Guide>,
    mut show_help: MessageWriter<ShowHelp>,
    buttons: Query<(&TouchButton, &Children)>,
    mut labels: Query<&mut Text>,
) {
    for (interaction, answer_) in &answers {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(said) = talk.open.as_ref().and_then(|c| c.said.as_ref()) else {
            return;
        };
        let then = said
            .choices
            .get(answer_.0)
            .map_or(Then::Bye, |(_, then)| then.clone());
        answer(&mut talk, then, &mut guide, &mut show_help);
        label_talk_button(talk.lines, &buttons, &mut labels);
        return;
    }
}

/// Goes where an answer leads.
fn answer(talk: &mut Talk, then: Then, guide: &mut Guide, show_help: &mut MessageWriter<ShowHelp>) {
    let topic = match then {
        Then::Topic(topic) => topic,
        Then::Guide(place, topic) => {
            guide.0 = Some(place);
            topic
        }
        Then::Help => {
            show_help.write(ShowHelp);
            talk.open = None;
            return;
        }
        Then::Language(index) => {
            set_language(talk, LANGUAGES[index]);
            remember_language(talk.lines.code);
            Topic::Hello
        }
        Then::Bye => {
            talk.open = None;
            return;
        }
    };
    if topic == Topic::Tip
        && talk
            .open
            .as_ref()
            .is_some_and(|conversation| conversation.topic == Topic::Tip)
    {
        talk.tip += 1;
    }
    if let Some(conversation) = &mut talk.open {
        conversation.topic = topic;
        conversation.said = None;
    }
}

/// Ends the conversation when the player walks away, keeps the one they talk to facing them,
/// and works out what's said when the topic changes.
fn follow(
    mut talk: ResMut<Talk>,
    nearby: Res<Nearby>,
    things: Things,
    mut listening: ResMut<Listening>,
) {
    let kind = |job: Job| match job {
        Job::Bartender => Kind::Bartender,
        Job::Cleaner => Kind::Cleaner,
    };
    if let Some(conversation) = &talk.open
        && nearby.get().map(|(near, _)| near) != Some(kind(conversation.job))
    {
        talk.open = None;
    }
    let feet = things
        .player
        .single()
        .map_or(Vec2::ZERO, |player| player.feet);
    listening.0 = talk.open.as_ref().map(|conversation| Listener {
        job: conversation.job,
        spot: conversation.spot,
        player: feet,
    });
    let (lines, tip) = (talk.lines, talk.tip);
    let Some(conversation) = &mut talk.open else {
        return;
    };
    if conversation.said.is_none() {
        let said = say(conversation, lines, &things.bar(lines), tip);
        conversation.said = Some(said);
    }
}

/// The box with what's said and the answers, rebuilt when that changes.
fn show(
    mut commands: Commands,
    talk: Res<Talk>,
    touch: Res<Touch>,
    talk_box: Single<(Entity, &mut Visibility), With<TalkBox>>,
    mut shown: Local<Option<(String, Vec<String>)>>,
) {
    let (entity, mut visibility) = talk_box.into_inner();
    let Some((conversation, said)) = talk
        .open
        .as_ref()
        .and_then(|c| c.said.as_ref().map(|said| (c, said)))
    else {
        *visibility = Visibility::Hidden;
        *shown = None;
        return;
    };
    *visibility = Visibility::Inherited;
    let now = (
        said.line.clone(),
        said.choices.iter().map(|(text, _)| text.clone()).collect(),
    );
    if shown.as_ref() == Some(&now) {
        return;
    }
    *shown = Some(now);
    let lines = talk.lines;
    let touch = touch.is_on();
    commands
        .entity(entity)
        .despawn_children()
        .with_children(|panel| {
            panel.spawn(text(conversation.name, 15.0, GOLD));
            panel.spawn(text(&said.line, 14.0, Color::WHITE));
            let mut answers: Vec<String> = said.choices.iter().map(|(t, _)| t.clone()).collect();
            if touch {
                answers.push(lines.bye.to_string());
            }
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(if touch { 6.0 } else { 2.0 }),
                    ..default()
                })
                .with_children(|list| {
                    for (index, answer_) in answers.iter().enumerate() {
                        if touch {
                            list.spawn((
                                TouchButton::Choice(index as u8),
                                Answer(index),
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(9.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    border_radius: BorderRadius::all(Val::Px(8.0)),
                                    ..default()
                                },
                                BorderColor::all(DIM),
                                BackgroundColor(Color::NONE),
                                children![text(answer_, 14.0, Color::WHITE)],
                            ));
                        } else {
                            list.spawn((
                                Answer(index),
                                Interaction::default(),
                                Node {
                                    padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|row| {
                                row.spawn(text(&format!("{}  ", index + 1), 14.0, GOLD));
                                row.spawn(text(answer_, 14.0, Color::WHITE));
                            });
                        }
                    }
                    if !touch {
                        list.spawn(Node {
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn(text("Esc  ", 14.0, DIM));
                            row.spawn(text(lines.bye, 14.0, DIM));
                        });
                    }
                });
        });
}

/// Lights the answer under the mouse.
fn light_answers(
    mut answers: Query<(&Interaction, &mut BackgroundColor, &Answer), Changed<Interaction>>,
) {
    for (interaction, mut background, _) in &mut answers {
        background.0 = match interaction {
            Interaction::None => Color::NONE,
            _ => HOVERED,
        };
    }
}

/// At the counter or next to the cleaner, the hint to talk to them over their head; on a touch
/// screen, the Talk button.
fn hint(
    talk: Res<Talk>,
    nearby: Res<Nearby>,
    on_shift: Res<OnShift>,
    touch: Res<Touch>,
    camera: Single<(&Camera, &GlobalTransform)>,
    hint: Single<(&mut Text, &mut Node, &mut Visibility, &ComputedNode), With<Hint>>,
    mut buttons: Query<(&TouchButton, &mut Visibility), Without<Hint>>,
) {
    let (mut text, mut node, mut visibility, computed) = hint.into_inner();
    let duty = match nearby.get() {
        Some((Kind::Bartender, _)) => on_shift.bartender.as_ref(),
        Some((Kind::Cleaner, _)) => on_shift.cleaner.as_ref(),
        _ => None,
    };
    let near = duty.map(|duty| (duty.name, duty.feet + Vec2::Y * OVER_HEAD));
    let Some((name, above)) = near.filter(|_| talk.open.is_none()) else {
        *visibility = Visibility::Hidden;
        if talk.open.is_some() {
            nearby::offer(&mut buttons, false, false);
        }
        return;
    };
    let label = if touch.is_on() {
        nearby::offer_talk(&mut buttons);
        name.to_string()
    } else {
        format!("E  {}", talk.lines.talk_to.replace("{name}", name))
    };
    place_hint(
        &label,
        above,
        *camera,
        (&mut text, &mut node, &mut visibility, computed),
    );
}

/// On a first visit, once the welcome panel has gone, the way to the bartender, who shows
/// players around.
fn first_visit(talk: Res<Talk>, things: Things, mut guide: ResMut<Guide>, mut done: Local<bool>) {
    if *done || !help::first_visit() {
        return;
    }
    let Some((_, counter)) = things.bartender(talk.lines) else {
        return;
    };
    *done = true;
    guide.0 = Some(counter);
}

#[cfg(feature = "test-hooks")]
fn report(talk: &Talk, guide: &Guide) -> serde_json::Value {
    let conversation = talk.open.as_ref().map(|conversation| {
        let said = conversation.said.as_ref();
        serde_json::json!({
            "job": format!("{:?}", conversation.job),
            "name": conversation.name,
            "line": said.map(|said| said.line.clone()),
            "choices": said.map(|said| said.choices.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()),
        })
    });
    let guide = guide.0.as_ref().map(|place| {
        serde_json::json!({ "label": place.label, "cell": [place.cells[0].x, place.cells[0].y] })
    });
    serde_json::json!({ "language": talk.lines.code, "open": conversation, "guide": guide })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(label: &str, cell: IVec2) -> Place {
        Place {
            label: label.into(),
            above: cell_to_world(cell.x, cell.y),
            cells: vec![cell],
        }
    }

    fn bar() -> Bar {
        let spot = |title: &str, genre, x, playing| Spot {
            place: place(title, IVec2::new(x, 0)),
            genre,
            playing,
        };
        Bar {
            games: vec![
                spot("Daytona USA", Some(Genre::Racing), 1, 3),
                spot("Mortal Kombat II", Some(Genre::Fighting), 2, 0),
                spot("Out Run", Some(Genre::Racing), 3, 1),
            ],
            cabinets: 10,
            tables: vec![(0, place("Pool", IVec2::new(5, 5)))],
            bartender: Some(("Kenji", place("Talk to Kenji", IVec2::new(0, -5)))),
            touch: false,
        }
    }

    fn conversation(job: Job, topic: Topic) -> Conversation {
        Conversation {
            job,
            person: 2,
            name: "Kenji",
            spot: None,
            topic,
            said: None,
        }
    }

    fn texts(said: &Said) -> Vec<&str> {
        said.choices.iter().map(|(text, _)| text.as_str()).collect()
    }

    #[test]
    fn the_bartender_shows_what_there_is_to_play_and_the_way_there() {
        let bar = bar();
        let english = lines::for_language("en");
        let said = say(
            &conversation(Job::Bartender, Topic::Hello),
            english,
            &bar,
            0,
        );
        assert_eq!(said.line, "Hey! Kenji here. First time at the bar?");
        assert_eq!(said.choices[0].1, Then::Topic(Topic::Play));
        // Only the kinds of game there are, then the tables.
        let said = say(&conversation(Job::Bartender, Topic::Play), english, &bar, 0);
        assert!(said.line.starts_with("10 cabinets"));
        assert_eq!(
            texts(&said),
            ["Fighting games", "Racing", "The tables", "Back"]
        );
        let racing = Topic::Shelf(Shelf::Games(Some(Genre::Racing)));
        let said = say(&conversation(Job::Bartender, racing), english, &bar, 0);
        assert_eq!(texts(&said), ["Daytona USA", "Out Run", "Back"]);
        let Then::Guide(place, then) = &said.choices[0].1 else {
            panic!("picking a game shows the way: {:?}", said.choices[0]);
        };
        assert_eq!(place.cells, [IVec2::new(1, 0)]);
        assert_eq!(*then, Topic::There("Daytona USA".into()));
        // Busiest first.
        let said = say(&conversation(Job::Bartender, Topic::Busy), english, &bar, 0);
        assert_eq!(
            texts(&said),
            ["Daytona USA: 3 playing", "Out Run: 1 playing", "Back"]
        );
    }

    #[test]
    fn titles_and_names_stay_as_they_are_in_another_language() {
        let bar = bar();
        let spanish = lines::for_language("es");
        let racing = Topic::Shelf(Shelf::Games(Some(Genre::Racing)));
        let said = say(&conversation(Job::Bartender, racing), spanish, &bar, 0);
        assert_eq!(texts(&said), ["Daytona USA", "Out Run", "Volver"]);
        let there = Topic::There("Daytona USA".into());
        let said = say(&conversation(Job::Bartender, there), spanish, &bar, 0);
        assert_eq!(said.line, "¿Daytona USA? Por aquí: sigue la flecha.");
        let said = say(
            &conversation(Job::Cleaner, Topic::Bartender),
            spanish,
            &bar,
            0,
        );
        assert!(said.line.starts_with("Kenji, detrás de la barra"));
        // Every language is offered in its own name.
        let said = say(
            &conversation(Job::Cleaner, Topic::Languages),
            spanish,
            &bar,
            0,
        );
        assert_eq!(
            texts(&said),
            [
                "English",
                "Español",
                "Português",
                "Français",
                "Deutsch",
                "Volver"
            ]
        );
    }

    #[test]
    fn the_cleaner_has_tips_and_points_to_the_bartender() {
        let mut bar = bar();
        let english = lines::for_language("en");
        let said = say(&conversation(Job::Cleaner, Topic::Hello), english, &bar, 0);
        assert_eq!(said.line, "Oh, hello! Don't slip. Can I help?");
        assert_eq!(
            texts(&said),
            ["Any tips?", "Who can show me around?", "Language"]
        );
        let first = say(&conversation(Job::Cleaner, Topic::Tip), english, &bar, 0);
        let second = say(&conversation(Job::Cleaner, Topic::Tip), english, &bar, 1);
        assert_ne!(first.line, second.line);
        // No bartender, nobody to point to.
        bar.bartender = None;
        let said = say(&conversation(Job::Cleaner, Topic::Hello), english, &bar, 0);
        assert_eq!(texts(&said), ["Any tips?", "Language"]);
    }
}
