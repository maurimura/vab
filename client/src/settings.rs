//! Tuning: `/settings` in the chat opens a panel of the numbers that shape how the games play,
//! to try changes on the spot. Up and Down pick one, Left and Right change it (with Shift, ten
//! steps at a time), and the mouse or a finger works the - and + buttons. N racks the pool
//! table again (as New rack does), New game starts that game over, R
//! puts everything back and Esc closes the panel. The numbers last until the page reloads.

use std::ops::Range;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;

use crate::Mode;
use crate::chat::{Chat, type_in_chat};
use crate::help::close_panel;

/// A value that can be tuned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knob {
    TopSpeed,
    SoftestShot,
    Friction,
    BallBounce,
    CushionBounce,
    PocketMouth,
    FullPull,
    PullBack,
    PuckSpeed,
    PuckFriction,
    RailBounce,
    PaddleBounce,
    BotSpeed,
    Sand,
    SandGrip,
    PuckKnock,
    HardestThrow,
    Flick,
    Sway,
    Steady,
    SteadyFor,
    Shake,
    Scatter,
    FlickSpeed,
    FlickSpread,
    TableSlope,
    FlipperSwing,
    FlipperDrop,
    FlipperBounce,
    WallBounce,
    RubberBounce,
    BumperKick,
    SlingKick,
    Plunger,
    RampNeeds,
    BallTopSpeed,
    RollingDrag,
    GameSpeed,
}

struct Spec {
    knob: Knob,
    label: &'static str,
    unit: &'static str,
    min: f32,
    max: f32,
    step: f32,
    decimals: usize,
}

/// Every knob in the order the panel lists them, under the heading of the game they're for.
const SPECS: [Spec; 38] = [
    Spec {
        knob: Knob::TopSpeed,
        label: "Hardest shot",
        unit: "px/s",
        min: 100.0,
        max: 2000.0,
        step: 10.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::SoftestShot,
        label: "Softest shot",
        unit: "px/s",
        min: 0.0,
        max: 300.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Friction,
        label: "Friction",
        unit: "px/s²",
        min: 0.0,
        max: 500.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::BallBounce,
        label: "Ball bounce",
        unit: "",
        min: 0.3,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::CushionBounce,
        label: "Cushion bounce",
        unit: "",
        min: 0.1,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::PocketMouth,
        label: "Pocket mouth",
        unit: "px",
        min: 9.0,
        max: 30.0,
        step: 0.5,
        decimals: 1,
    },
    Spec {
        knob: Knob::FullPull,
        label: "Time to full power",
        unit: "s",
        min: 0.2,
        max: 4.0,
        step: 0.05,
        decimals: 2,
    },
    Spec {
        knob: Knob::PullBack,
        label: "Cue pull-back",
        unit: "px",
        min: 4.0,
        max: 80.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::PuckSpeed,
        label: "Puck top speed",
        unit: "px/s",
        min: 100.0,
        max: 1500.0,
        step: 10.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::PuckFriction,
        label: "Puck friction",
        unit: "px/s²",
        min: 0.0,
        max: 300.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::RailBounce,
        label: "Rail bounce",
        unit: "",
        min: 0.1,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::PaddleBounce,
        label: "Paddle bounce",
        unit: "",
        min: 0.0,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::BotSpeed,
        label: "Bot speed",
        unit: "px/s",
        min: 0.0,
        max: 400.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Sand,
        label: "Sand",
        unit: "px/s²",
        min: 1.0,
        max: 300.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::SandGrip,
        label: "Sand grip",
        unit: "/s",
        min: 0.0,
        max: 3.0,
        step: 0.05,
        decimals: 2,
    },
    Spec {
        knob: Knob::PuckKnock,
        label: "Puck bounce",
        unit: "",
        min: 0.0,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::HardestThrow,
        label: "Hardest throw",
        unit: "px/s",
        min: 100.0,
        max: 1500.0,
        step: 10.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Flick,
        label: "Flick strength",
        unit: "x",
        min: 0.2,
        max: 5.0,
        step: 0.1,
        decimals: 1,
    },
    Spec {
        knob: Knob::Sway,
        label: "Hand sway",
        unit: "mm",
        min: 0.0,
        max: 100.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Steady,
        label: "Sway held",
        unit: "x",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        decimals: 2,
    },
    Spec {
        knob: Knob::SteadyFor,
        label: "Steady for",
        unit: "s",
        min: 0.2,
        max: 10.0,
        step: 0.1,
        decimals: 1,
    },
    Spec {
        knob: Knob::Shake,
        label: "Shake after",
        unit: "mm/s",
        min: 0.0,
        max: 200.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Scatter,
        label: "Scatter",
        unit: "mm",
        min: 0.0,
        max: 40.0,
        step: 0.5,
        decimals: 1,
    },
    Spec {
        knob: Knob::FlickSpeed,
        label: "Right flick",
        unit: "mm/s",
        min: 100.0,
        max: 5000.0,
        step: 50.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::FlickSpread,
        label: "Flick error",
        unit: "mm",
        min: 0.0,
        max: 300.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::TableSlope,
        label: "Table slope",
        unit: "°",
        min: 2.0,
        max: 12.0,
        step: 0.5,
        decimals: 1,
    },
    Spec {
        knob: Knob::FlipperSwing,
        label: "Flipper swing",
        unit: "rad/s",
        min: 5.0,
        max: 100.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::FlipperDrop,
        label: "Flipper drop",
        unit: "rad/s",
        min: 5.0,
        max: 100.0,
        step: 1.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::FlipperBounce,
        label: "Flipper bounce",
        unit: "",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        decimals: 2,
    },
    Spec {
        knob: Knob::WallBounce,
        label: "Wall bounce",
        unit: "",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        decimals: 2,
    },
    Spec {
        knob: Knob::RubberBounce,
        label: "Rubber bounce",
        unit: "",
        min: 0.0,
        max: 1.0,
        step: 0.02,
        decimals: 2,
    },
    Spec {
        knob: Knob::BumperKick,
        label: "Bumper kick",
        unit: "in/s",
        min: 0.0,
        max: 600.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::SlingKick,
        label: "Sling kick",
        unit: "in/s",
        min: 0.0,
        max: 600.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::Plunger,
        label: "Full plunge",
        unit: "in/s",
        min: 60.0,
        max: 400.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::RampNeeds,
        label: "Ramp needs",
        unit: "in/s",
        min: 20.0,
        max: 300.0,
        step: 5.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::BallTopSpeed,
        label: "Ball top speed",
        unit: "in/s",
        min: 100.0,
        max: 1000.0,
        step: 10.0,
        decimals: 0,
    },
    Spec {
        knob: Knob::RollingDrag,
        label: "Rolling drag",
        unit: "/s",
        min: 0.0,
        max: 1.0,
        step: 0.01,
        decimals: 2,
    },
    Spec {
        knob: Knob::GameSpeed,
        label: "Game speed",
        unit: "x",
        min: 0.25,
        max: 1.5,
        step: 0.05,
        decimals: 2,
    },
];

/// The panel shows the games side by side, so it fits on the screen: the knob each column
/// starts with.
const FIRST_IN_COLUMN: [Knob; 3] = [Knob::TopSpeed, Knob::Sand, Knob::TableSlope];

/// Shows the panel (`/settings` in the chat).
#[derive(Message)]
pub struct ShowSettings;

/// Puts the pool table's balls back in the rack (the panel's New rack button, or N).
#[derive(Message)]
pub struct NewRack;

/// Starts a new game of shuffleboard (the panel's New game button).
#[derive(Message)]
pub struct NewShuffleboardGame;

/// Starts a new game of darts (the panel's New game button, under Darts).
#[derive(Message)]
pub struct NewDartsGame;

/// Starts a new game of pinball (the panel's New game button, under Pinball).
#[derive(Message)]
pub struct NewPinballGame;

#[derive(Resource)]
pub struct Settings {
    open: bool,
    selected: usize,
    values: [f32; SPECS.len()],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open: false,
            selected: 0,
            values: defaults(),
        }
    }
}

impl Settings {
    /// The panel is up, and the keys are for it.
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn get(&self, knob: Knob) -> f32 {
        self.values[index(knob)]
    }

    /// How the pool table plays.
    pub fn pool(&self) -> billiards::Settings {
        billiards::Settings {
            min_speed: self.get(Knob::SoftestShot),
            max_speed: self.get(Knob::TopSpeed),
            friction: self.get(Knob::Friction),
            ball_restitution: self.get(Knob::BallBounce),
            cushion_restitution: self.get(Knob::CushionBounce),
            pocket_mouth: self.get(Knob::PocketMouth),
        }
    }

    /// How the air hockey table plays.
    pub fn hockey(&self) -> hockey::Settings {
        hockey::Settings {
            max_speed: self.get(Knob::PuckSpeed),
            friction: self.get(Knob::PuckFriction),
            rail_restitution: self.get(Knob::RailBounce),
            paddle_restitution: self.get(Knob::PaddleBounce),
            bot_speed: self.get(Knob::BotSpeed),
        }
    }

    /// How the hand throwing darts moves, and throws.
    pub fn darts(&self) -> darts::Hand {
        darts::Hand {
            drift: self.get(Knob::Sway),
            steady: self.get(Knob::Steady),
            steady_for: self.get(Knob::SteadyFor),
            shake: self.get(Knob::Shake),
            scatter: self.get(Knob::Scatter),
            flick_speed: self.get(Knob::FlickSpeed),
            flick_spread: self.get(Knob::FlickSpread),
        }
    }

    /// How the shuffleboard table plays.
    pub fn shuffleboard(&self) -> shuffleboard::Settings {
        shuffleboard::Settings {
            friction: self.get(Knob::Sand),
            grip: self.get(Knob::SandGrip),
            bounce: self.get(Knob::PuckKnock),
            max_speed: self.get(Knob::HardestThrow),
        }
    }

    /// How the pinball machine plays.
    pub fn pinball(&self) -> pinball::Settings {
        pinball::Settings {
            slope: pinball::GRAVITY * self.get(Knob::TableSlope).to_radians().sin(),
            flipper_speed: self.get(Knob::FlipperSwing),
            flipper_return: self.get(Knob::FlipperDrop),
            flipper_bounce: self.get(Knob::FlipperBounce),
            wall_bounce: self.get(Knob::WallBounce),
            rubber_bounce: self.get(Knob::RubberBounce),
            bumper_kick: self.get(Knob::BumperKick),
            sling_kick: self.get(Knob::SlingKick),
            plunger_speed: self.get(Knob::Plunger),
            ramp_speed: self.get(Knob::RampNeeds),
            max_speed: self.get(Knob::BallTopSpeed),
            rolling: self.get(Knob::RollingDrag),
        }
    }

    /// How fast the pinball machine runs: 1 is real time.
    pub fn pinball_speed(&self) -> f32 {
        self.get(Knob::GameSpeed)
    }

    /// Moves the value of the picked knob by `steps` of its step, within its range.
    fn nudge(&mut self, row: usize, steps: f32) {
        let spec = &SPECS[row];
        let value = self.values[row] + spec.step * steps;
        // On whole steps, so repeated nudges don't drift.
        let value = (value / spec.step).round() * spec.step;
        self.values[row] = value.clamp(spec.min, spec.max);
    }
}

fn index(knob: Knob) -> usize {
    SPECS
        .iter()
        .position(|spec| spec.knob == knob)
        .expect("every knob is listed")
}

fn defaults() -> [f32; SPECS.len()] {
    let pool = billiards::Settings::default();
    let hockey = hockey::Settings::default();
    let shuffleboard = shuffleboard::Settings::default();
    let hand = darts::Hand::default();
    let pinball = pinball::Settings::default();
    SPECS.map(|spec| match spec.knob {
        Knob::TopSpeed => pool.max_speed,
        Knob::SoftestShot => pool.min_speed,
        Knob::Friction => pool.friction,
        Knob::BallBounce => pool.ball_restitution,
        Knob::CushionBounce => pool.cushion_restitution,
        Knob::PocketMouth => pool.pocket_mouth,
        Knob::FullPull => 2.0,
        Knob::PullBack => 32.0,
        Knob::PuckSpeed => hockey.max_speed,
        Knob::PuckFriction => hockey.friction,
        Knob::RailBounce => hockey.rail_restitution,
        Knob::PaddleBounce => hockey.paddle_restitution,
        Knob::BotSpeed => hockey.bot_speed,
        Knob::Sand => shuffleboard.friction,
        Knob::SandGrip => shuffleboard.grip,
        Knob::PuckKnock => shuffleboard.bounce,
        Knob::HardestThrow => shuffleboard.max_speed,
        // How much faster the puck goes than the hand that let it go: the canvas is small.
        Knob::Flick => 3.0,
        Knob::Sway => hand.drift,
        Knob::Steady => hand.steady,
        Knob::SteadyFor => hand.steady_for,
        Knob::Shake => hand.shake,
        Knob::Scatter => hand.scatter,
        Knob::FlickSpeed => hand.flick_speed,
        Knob::FlickSpread => hand.flick_spread,
        Knob::TableSlope => pinball::SLOPE_DEGREES,
        Knob::FlipperSwing => pinball.flipper_speed,
        Knob::FlipperDrop => pinball.flipper_return,
        Knob::FlipperBounce => pinball.flipper_bounce,
        Knob::WallBounce => pinball.wall_bounce,
        Knob::RubberBounce => pinball.rubber_bounce,
        Knob::BumperKick => pinball.bumper_kick,
        Knob::SlingKick => pinball.sling_kick,
        Knob::Plunger => pinball.plunger_speed,
        Knob::RampNeeds => pinball.ramp_speed,
        Knob::BallTopSpeed => pinball.max_speed,
        Knob::RollingDrag => pinball.rolling,
        Knob::GameSpeed => 1.0,
    })
}

/// Run condition: the panel isn't up.
pub fn settings_closed(settings: Res<Settings>) -> bool {
    !settings.open
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .add_message::<ShowSettings>()
            .add_message::<NewRack>()
            .add_message::<NewShuffleboardGame>()
            .add_message::<NewDartsGame>()
            .add_message::<NewPinballGame>()
            .add_systems(Startup, spawn_panel)
            // After the chat and the controls panel have had the keys, so a key that closes
            // either doesn't also tune something, and before anything else reads them.
            .add_systems(
                PreUpdate,
                tune_with_keys
                    .after(InputSystems)
                    .after(type_in_chat)
                    .after(close_panel),
            )
            .add_systems(Update, (open, tune_with_pointer, show_panel).chain());
    }
}

#[derive(Component)]
struct Panel;

/// A knob's line in the panel, and the text with its value.
#[derive(Component)]
struct Row(usize);

#[derive(Component)]
struct Value(usize);

/// The panel's buttons: - and + by row and which way they go, New rack, New game (shuffleboard,
/// darts and pinball), and Close for a screen without Esc.
#[derive(Component, Clone, Copy)]
enum PanelButton {
    Step(usize, f32),
    NewRack,
    NewShuffleboardGame,
    NewDartsGame,
    NewPinballGame,
    Close,
}

const SELECTED: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);

fn text(text: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn heading(heading: &str, top: f32) -> impl Bundle {
    (
        text(heading, 13.0, Color::srgb(0.6, 0.85, 0.6)),
        Node {
            margin: UiRect::top(Val::Px(top)),
            ..default()
        },
    )
}

/// A game's heading, and a button for it on the right.
fn heading_with(heading: &str, button: PanelButton, label: &str, top: f32) -> impl Bundle {
    (
        Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            margin: UiRect::top(Val::Px(top)),
            ..default()
        },
        children![
            text(heading, 13.0, Color::srgb(0.6, 0.85, 0.6)),
            (
                button,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                children![text(label, 12.0, Color::WHITE)],
            ),
        ],
    )
}

fn step_button(row: usize, way: f32) -> impl Bundle {
    (
        PanelButton::Step(row, way),
        Node {
            width: Val::Px(26.0),
            height: Val::Px(22.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
        children![text(if way < 0.0 { "-" } else { "+" }, 14.0, Color::WHITE)],
    )
}

/// A knob's line: its name, - and +, and its value between them.
fn knob_row(row: usize) -> impl Bundle {
    (
        Row(row),
        Node {
            height: Val::Px(28.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        children![
            (
                text(SPECS[row].label, 14.0, Color::WHITE),
                TextLayout::new(Justify::Left, LineBreak::NoWrap),
                Node {
                    width: Val::Px(150.0),
                    ..default()
                },
            ),
            step_button(row, -1.0),
            (
                Value(row),
                text("", 14.0, Color::srgb(1.0, 0.85, 0.3)),
                Node {
                    width: Val::Px(96.0),
                    ..default()
                },
                TextLayout {
                    justify: Justify::Center,
                    linebreak: LineBreak::NoWrap,
                },
            ),
            step_button(row, 1.0),
        ],
    )
}

/// A column of the panel: these knobs, under their games' headings.
fn spawn_column(column: &mut ChildSpawnerCommands, rows: Range<usize>) {
    for row in rows {
        match SPECS[row].knob {
            Knob::TopSpeed => {
                column.spawn(heading_with(
                    "Pool",
                    PanelButton::NewRack,
                    "New rack (N)",
                    0.0,
                ));
            }
            Knob::PuckSpeed => {
                column.spawn(heading("Air hockey", 6.0));
            }
            Knob::Sand => {
                column.spawn(heading_with(
                    "Shuffleboard",
                    PanelButton::NewShuffleboardGame,
                    "New game",
                    0.0,
                ));
            }
            Knob::Sway => {
                column.spawn(heading_with(
                    "Darts",
                    PanelButton::NewDartsGame,
                    "New game",
                    6.0,
                ));
            }
            Knob::TableSlope => {
                column.spawn(heading_with(
                    "Pinball",
                    PanelButton::NewPinballGame,
                    "New game",
                    0.0,
                ));
            }
            _ => {}
        }
        column.spawn(knob_row(row));
    }
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // Over the games' screens.
            GlobalZIndex(2),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(16.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.88)),
                ))
                .with_children(|list| {
                    list.spawn(text("Settings", 18.0, Color::WHITE));
                    list.spawn(Node {
                        column_gap: Val::Px(16.0),
                        ..default()
                    })
                    .with_children(|columns| {
                        for (i, first) in FIRST_IN_COLUMN.iter().enumerate() {
                            let end = FIRST_IN_COLUMN
                                .get(i + 1)
                                .map_or(SPECS.len(), |&next| index(next));
                            columns
                                .spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(4.0),
                                    ..default()
                                })
                                .with_children(|column| spawn_column(column, index(*first)..end));
                        }
                    });
                    list.spawn((
                        text(
                            "Up/Down pick  Left/Right change (Shift: x10)\nR resets all  Esc closes",
                            12.0,
                            Color::srgb(0.7, 0.7, 0.7),
                        ),
                        Node {
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        },
                    ));
                    list.spawn((
                        PanelButton::Close,
                        Node {
                            align_self: AlignSelf::End,
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                        children![text("Close", 13.0, Color::WHITE)],
                    ));
                });
        });
}

fn open(
    mut asked: MessageReader<ShowSettings>,
    mode: Res<State<Mode>>,
    mut settings: ResMut<Settings>,
) {
    if asked.read().count() == 0 {
        return;
    }
    settings.open = true;
    // At a game, on its knobs.
    let first = match mode.get() {
        Mode::Pool => Some(Knob::TopSpeed),
        Mode::Hockey => Some(Knob::PuckSpeed),
        Mode::Shuffleboard => Some(Knob::Sand),
        Mode::Darts => Some(Knob::Sway),
        Mode::Pinball => Some(Knob::TableSlope),
        _ => None,
    };
    if let Some(first) = first {
        settings.selected = index(first);
    }
}

fn tune_with_keys(
    mut keyboard: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    chat: Res<Chat>,
    mut settings: ResMut<Settings>,
    mut new_rack: MessageWriter<NewRack>,
) {
    if !settings.open || chat.is_open() {
        keyboard.clear();
        return;
    }
    let ten = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let steps = if ten { 10.0 } else { 1.0 };
    // Held keys repeat, so a value can be run up or down.
    for key in keyboard.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        let row = settings.selected;
        match key.key_code {
            KeyCode::ArrowUp | KeyCode::KeyW => {
                settings.selected = (row + SPECS.len() - 1) % SPECS.len();
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                settings.selected = (row + 1) % SPECS.len();
            }
            KeyCode::ArrowLeft | KeyCode::KeyA => settings.nudge(row, -steps),
            KeyCode::ArrowRight | KeyCode::KeyD => settings.nudge(row, steps),
            KeyCode::KeyR => settings.values = defaults(),
            KeyCode::KeyN if !key.repeat => {
                new_rack.write(NewRack);
            }
            KeyCode::Escape => settings.open = false,
            _ => {}
        }
    }
    // The keys were for the panel: Esc doesn't also leave a game, nor E sit at one.
    let pressed: Vec<KeyCode> = keys.get_just_pressed().copied().collect();
    for key in pressed {
        keys.clear_just_pressed(key);
    }
}

/// A click or tap on - or + changes that knob, on a line picks it, and on New rack, New game
/// or Close does that.
#[allow(clippy::too_many_arguments)]
fn tune_with_pointer(
    window: Single<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut settings: ResMut<Settings>,
    rows: Query<(&Row, &ComputedNode, &UiGlobalTransform)>,
    panel_buttons: Query<(&PanelButton, &ComputedNode, &UiGlobalTransform)>,
    mut new_rack: MessageWriter<NewRack>,
    mut new_shuffleboard_game: MessageWriter<NewShuffleboardGame>,
    mut new_darts_game: MessageWriter<NewDartsGame>,
    mut new_pinball_game: MessageWriter<NewPinballGame>,
) {
    if !settings.open {
        return;
    }
    let clicks = mouse
        .just_pressed(MouseButton::Left)
        .then(|| window.cursor_position())
        .flatten();
    let taps = touches.iter_just_pressed().map(|finger| finger.position());
    for at in clicks.into_iter().chain(taps) {
        let point = at * window.scale_factor();
        if let Some((row, ..)) = rows
            .iter()
            .find(|(_, node, transform)| node.contains_point(**transform, point))
        {
            settings.selected = row.0;
        }
        let pressed = panel_buttons
            .iter()
            .find(|(_, node, transform)| node.contains_point(**transform, point));
        match pressed.map(|(button, ..)| *button) {
            Some(PanelButton::Step(row, way)) => settings.nudge(row, way),
            Some(PanelButton::NewRack) => {
                new_rack.write(NewRack);
            }
            Some(PanelButton::NewShuffleboardGame) => {
                new_shuffleboard_game.write(NewShuffleboardGame);
            }
            Some(PanelButton::NewDartsGame) => {
                new_darts_game.write(NewDartsGame);
            }
            Some(PanelButton::NewPinballGame) => {
                new_pinball_game.write(NewPinballGame);
            }
            Some(PanelButton::Close) => settings.open = false,
            None => {}
        }
    }
}

fn show_panel(
    settings: Res<Settings>,
    mut panel: Single<&mut Visibility, With<Panel>>,
    mut rows: Query<(&Row, &mut BackgroundColor)>,
    mut values: Query<(&Value, &mut Text)>,
) {
    panel.set_if_neq(if settings.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !settings.is_changed() {
        return;
    }
    for (row, mut background) in &mut rows {
        let color = if row.0 == settings.selected {
            SELECTED
        } else {
            Color::NONE
        };
        background.set_if_neq(BackgroundColor(color));
    }
    for (value, mut text) in &mut values {
        let spec = &SPECS[value.0];
        let number = format!("{:.*}", spec.decimals, settings.values[value.0]);
        text.0 = if spec.unit.is_empty() {
            number
        } else {
            format!("{number} {}", spec.unit)
        };
    }
}
