//! Playing air hockey: the rink seen from above, upright, the player's goal at the bottom and
//! the other one's at the top, drawn pixel by pixel into a small image that fills the screen at
//! a whole-number zoom, as the pool table is. The player's paddle moves with the mouse, kept in
//! their half: the page locks the pointer (a click on the table does), so it never wanders off,
//! and Esc lets it go, as a second Esc leaves. On a touch screen the paddle sits under the
//! finger. Alone at the table, the player plays a bot that slides its paddle across its goal
//! (the hockey crate), and letting the pointer go pauses; when someone sits at the other seat,
//! they play each other (online.rs). First to 7 wins, and a click starts the next game (player
//! 1's, against someone). Esc goes back to the bar. How it plays can be tuned with `/settings`.

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use bevy::window::CursorOptions;
use hockey::{
    Bot, Event, GOAL_WIDTH, HEIGHT, PADDLE_RADIUS, PUCK_RADIUS, Rink, WIDTH, WINNING_SCORE,
};

use wasm_bindgen::prelude::*;

use super::online::Message;
use crate::Mode;
use crate::chat::{Chat, chat_closed};
use crate::help::Help;
use crate::pixels::Pixels;
use crate::seats;
use crate::settings::Settings;
use crate::touch::{self, Touch, TouchButton};

/// The image the rink is drawn into, and where the rink's top-left corner is in it.
const CANVAS: UVec2 = UVec2::new(320, 180);
const RINK: Vec2 = Vec2::new(112.0, 10.0);
/// The rail around the rink, in pixels.
const RAIL: i32 = 6;
/// The scores, left of the rink: the other side's across from its half and the player's from theirs,
/// and the line right of the rink, from their left edges, how wide, and their tops (canvas
/// pixels).
const THEIR_SCORE: (f32, f32, f32) = (20.0, 80.0, 40.0);
const YOUR_SCORE: (f32, f32, f32) = (20.0, 80.0, 110.0);
const STATUS: (f32, f32, f32) = (222.0, 90.0, 80.0);
/// Text heights, in canvas pixels, and the smallest they get, in logical pixels.
const SCORE_SIZE: f32 = 6.0;
const STATUS_SIZE: f32 = 3.5;
const SMALLEST_TEXT: f32 = 8.0;
/// How long "Goal!" stays up after one.
const GOAL_SHOWN_FOR: f32 = 1.5;
/// An Esc this soon after the pointer was let go is the one that let it go (the browser takes
/// it, and may pass it on too): it doesn't also leave the table.
const ESC_AFTER_UNLOCK: f32 = 0.3;
/// How often each player tells the other where things are, in seconds.
const TICK_EVERY: f32 = 1.0 / 30.0;
/// The puck is handed over once it's this far into the other half.
const HANDOVER_PAST: f32 = 1.0;
/// Handed the puck, a player keeps their own rink's puck if it's this close to where the other
/// player let it go: it's been following it, and is likely a little further along already.
const KEEP_OWN_PUCK: f32 = 24.0;

/// The air hockey table the player sits at, as the room names it (online::table_id, seats.rs).
/// Set before switching to `Mode::Hockey`.
#[derive(Resource)]
pub struct AtTable(pub String);

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Whether a click on the canvas should lock the pointer (and, turned off, lets it go).
    #[wasm_bindgen(js_name = pointerLockWanted)]
    fn pointer_lock_wanted(on: bool);
    #[wasm_bindgen(js_name = pointerLocked)]
    fn pointer_locked() -> bool;
    /// This browser won't lock the pointer.
    #[wasm_bindgen(js_name = pointerLockFailed)]
    fn pointer_lock_failed() -> bool;
    /// Lets the locked pointer go.
    #[wasm_bindgen(js_name = releasePointer)]
    fn release_pointer();
}

/// How the player moves their paddle.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    /// It sits under their finger.
    Finger,
    /// It moves with the mouse, the pointer locked.
    Locked,
    /// The pointer isn't locked (yet, or after an Esc): the game waits for a click.
    Paused,
    /// The browser won't lock the pointer: the paddle follows it, hidden, as best it can.
    Unlocked,
}

impl Control {
    fn now(touch: &Touch) -> Self {
        if touch.is_on() {
            Control::Finger
        } else if pointer_locked() {
            Control::Locked
        } else if pointer_lock_failed() {
            Control::Unlocked
        } else {
            Control::Paused
        }
    }
}

const SURFACE: [u8; 4] = [226, 236, 244, 255];
/// The little holes the air comes out of, every few pixels, each row a half step along from
/// the last.
const AIR_HOLE: [u8; 4] = [186, 196, 208, 255];
const AIR_HOLE_SPACING: i32 = 6;
const MARKING_RED: [u8; 4] = [214, 96, 96, 255];
const MARKING_BLUE: [u8; 4] = [120, 150, 214, 255];
const RAIL_COLOR: [u8; 4] = [196, 40, 40, 255];
const RAIL_EDGE: [u8; 4] = [120, 22, 22, 255];
const RAIL_LIGHT: [u8; 4] = [232, 92, 92, 255];
const GOAL_SLOT: [u8; 4] = [22, 20, 28, 255];
const PUCK: [u8; 4] = [30, 30, 36, 255];
const PUCK_RIM: [u8; 4] = [70, 70, 82, 255];
const SHINE: [u8; 4] = [255, 255, 255, 255];
/// The player's paddle and its knob, then the other side's.
const PADDLES: [([u8; 4], [u8; 4]); 2] = [
    ([214, 48, 48, 255], [150, 28, 28, 255]),
    ([48, 96, 214, 255], [28, 60, 150, 255]),
];

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Mode::Hockey), show_rink)
            .add_systems(
                Update,
                (
                    fit_canvas,
                    sync,
                    play,
                    draw,
                    show_text,
                    place_text,
                    leave.run_if(chat_closed),
                )
                    .chain()
                    .run_if(in_state(Mode::Hockey)),
            )
            .add_systems(OnExit(Mode::Hockey), hide_rink);
    }
}

/// The game, kept between visits to the table.
#[derive(Resource, Default)]
struct Game {
    rink: Rink,
    bot: Bot,
    /// Where the player last pointed their paddle, in rink pixels.
    target: Option<Vec2>,
    /// How far the paddle is from the (hidden) mouse pointer: when the mouse goes past a rail
    /// or the centre line, the paddle stops there and the rest is forgotten, so moving back
    /// moves it back at once. A finger always has the paddle right under it.
    slack: Vec2,
    /// Who scored last, and how long ago.
    goal: Option<(usize, f32)>,
    /// The pointer was down last frame (a fresh press starts the next game).
    was_pressed: bool,
    /// The player's paddle is under their pointer. Until then (on sitting down, and in a new
    /// game) it goes straight there, rather than sweeping across the rink into the puck.
    paddle_placed: bool,
    /// The pointer was locked last frame, and when it was last let go (in seconds of `Time`).
    was_locked: bool,
    unlocked_at: Option<f32>,
    /// The table, as the room names it, while the player sits at it.
    table_id: Option<String>,
    /// The other player at the table, when there is one: then they play each other, not the bot.
    opponent: Option<Opponent>,
    /// This player runs the puck (it's in their half), and the other player's rink follows.
    owner: bool,
    /// Where the other player's paddle is, as they last said.
    their_paddle: Vec2,
    /// Time since this player last told the other where things are.
    since_tick: f32,
}

/// Who the player is playing against.
struct Opponent {
    /// The player's own seat (0 has their goal at the rink's bottom), and the other player's id.
    me: usize,
    id: u32,
    /// Both seats' names.
    names: [String; 2],
}

impl Game {
    /// The player's seat: 0 against the bot.
    fn me(&self) -> usize {
        self.opponent.as_ref().map_or(0, |opponent| opponent.me)
    }

    /// A fresh game, with the same players: the puck served to seat 0, who runs it.
    fn new_game(&mut self) {
        let settings = self.rink.settings;
        self.rink = Rink {
            settings,
            ..Rink::new()
        };
        self.bot = Bot::default();
        self.goal = None;
        self.owner = self.me() == 0;
        self.their_paddle = self.rink.paddles[1 - self.me()];
        self.paddle_placed = false;
        self.slack = Vec2::ZERO;
    }

    /// Against `opponent` now, or against the bot (`None`): a new game either way.
    fn play_against(&mut self, opponent: Option<Opponent>) {
        self.opponent = opponent;
        self.target = None;
        self.new_game();
    }

    /// Tells the other player where this player's paddle is, the puck while it's theirs (and
    /// that they're handing it over), and how many goals they've let in.
    fn tick(&mut self, handover: bool) {
        self.since_tick = 0.0;
        let (Some(table), Some(opponent)) = (&self.table_id, &self.opponent) else {
            return;
        };
        let me = opponent.me;
        let rink = &self.rink;
        let puck = (self.owner || handover).then_some([
            rink.puck.x,
            rink.puck.y,
            rink.velocity.x,
            rink.velocity.y,
        ]);
        let tick = Message::Tick {
            paddle: rink.paddles[me].into(),
            puck,
            handover,
            conceded: rink.score[1 - me],
        };
        seats::send(table, opponent.id, &tick);
    }
}

/// The rink as `me` sees it, their own goal at the bottom, from as seat 0 sees it, and back.
fn turned(me: usize, at: Vec2) -> Vec2 {
    if me == 1 {
        Vec2::new(WIDTH, HEIGHT) - at
    } else {
        at
    }
}

#[derive(Component)]
struct Overlay;

#[derive(Component)]
struct Canvas;

/// Text placed over the canvas: from `left`, `width` across, from `top` down (canvas pixels),
/// `size` tall.
#[derive(Component)]
struct OnCanvas {
    left: f32,
    width: f32,
    top: f32,
    size: f32,
}

/// What a text says.
#[derive(Component, Clone, Copy, PartialEq)]
enum Says {
    TheirScore,
    YourScore,
    Status,
}

fn show_rink(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Option<ResMut<Game>>,
    touch: Res<Touch>,
) {
    match game {
        // Picked up where it was left: the paddle goes to the pointer, wherever that is now.
        Some(mut game) => {
            game.paddle_placed = false;
            game.slack = Vec2::ZERO;
        }
        None => commands.init_resource::<Game>(),
    }
    if !touch.is_on() {
        pointer_lock_wanted(true);
    }
    let image = Image::new_fill(
        Extent3d {
            width: CANVAS.x,
            height: CANVAS.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let text = |says: Says, (left, width, top): (f32, f32, f32), size: f32| {
        (
            says,
            OnCanvas {
                left,
                width,
                top,
                size,
            },
            Text::new(""),
            TextFont::default(),
            TextColor(Color::WHITE),
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        )
    };
    let mut overlay = commands.spawn((
        Overlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // The bar stays in sight, dimmed.
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        GlobalZIndex(1),
        children![
            (Canvas, ImageNode::new(images.add(image)), Node::default()),
            text(Says::TheirScore, THEIR_SCORE, SCORE_SIZE),
            text(Says::YourScore, YOUR_SCORE, SCORE_SIZE),
            text(Says::Status, STATUS, STATUS_SIZE),
        ],
    ));
    if touch.is_on() {
        // In the corner, left of the page's Chat button (web/index.html), as at the pool table.
        overlay.with_child((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(80.0),
                ..default()
            },
            children![touch::button(TouchButton::Leave, "Leave", 64.0, 36.0)],
        ));
    }
}

/// As big as fits, at a whole number of the screen's own pixels per canvas pixel.
fn fit_canvas(window: Single<&Window>, mut canvas: Single<&mut Node, With<Canvas>>) {
    let fits = Vec2::new(
        window.physical_width() as f32,
        window.physical_height() as f32,
    ) / CANVAS.as_vec2();
    let zoom = fits.min_element().floor().max(1.0);
    let size = CANVAS.as_vec2() * zoom / window.scale_factor();
    let (width, height) = (Val::Px(size.x), Val::Px(size.y));
    if canvas.width != width || canvas.height != height {
        canvas.width = width;
        canvas.height = height;
    }
}

/// Sits at the table in the room, starts a match when someone sits at the other seat (and goes
/// back to the bot when they leave), and follows what they say: their paddle, the puck while
/// it's theirs, the puck when they hand it over, and the goals they've let in.
fn sync(at: Option<Res<AtTable>>, mut game: ResMut<Game>) {
    let Some(at) = at else {
        return;
    };
    let game = &mut *game;
    if game.table_id.as_deref() != Some(at.0.as_str()) {
        game.table_id = Some(at.0.clone());
        seats::sit(&at.0);
    }

    let now = seats::seats_at(&at.0).and_then(|seats| {
        let me = seats.mine()?;
        let (_, id) = seats.opponent()?;
        Some(Opponent {
            me,
            id,
            names: seats.names(),
        })
    });
    match (&mut game.opponent, now) {
        (Some(opponent), Some(now)) if opponent.id == now.id && opponent.me == now.me => {
            opponent.names = now.names;
        }
        (_, Some(now)) => game.play_against(Some(now)),
        (Some(_), None) => game.play_against(None),
        (None, None) => {}
    }

    let messages = seats::take_messages::<Message>(&at.0);
    let Some(id) = game.opponent.as_ref().map(|opponent| opponent.id) else {
        return;
    };
    let me = game.me();
    for (_, message) in messages.into_iter().filter(|(from, _)| *from == id) {
        match message {
            Message::Start => game.new_game(),
            Message::Tick {
                paddle,
                puck,
                handover,
                conceded,
            } => {
                game.their_paddle = Vec2::from(paddle);
                if conceded > game.rink.score[me] {
                    game.goal = Some((me, 0.0));
                }
                game.rink.score[me] = conceded;
                let Some([x, y, vx, vy]) = puck else {
                    continue;
                };
                let (theirs, going) = (Vec2::new(x, y), Vec2::new(vx, vy));
                let keep_mine = handover
                    && game.rink.puck.distance(theirs) < KEEP_OWN_PUCK
                    && in_half(me, game.rink.puck);
                if (handover || !game.owner) && !keep_mine {
                    game.rink.puck = theirs;
                    game.rink.velocity = going;
                }
                if handover {
                    game.owner = true;
                }
            }
        }
    }
}

/// Whether `at` (as seat 0 sees the rink) is in `seat`'s half.
fn in_half(seat: usize, at: Vec2) -> bool {
    if seat == 0 {
        at.y > HEIGHT / 2.0
    } else {
        at.y < HEIGHT / 2.0
    }
}

/// The player's paddle moves as they say, the bot's where it says, and time moves on, unless
/// the game waits for a click (to lock the pointer). Once someone has won, a fresh press starts
/// the next game.
#[allow(clippy::too_many_arguments)]
fn play(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    touch: Res<Touch>,
    chat: Res<Chat>,
    help: Res<Help>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut game: ResMut<Game>,
    mut cursor: Query<&mut CursorOptions>,
) {
    let game = &mut *game;
    let seconds = time.delta_secs();
    game.rink.settings = settings.hockey();
    if let Some((_, since)) = &mut game.goal {
        *since += seconds;
    }
    let control = Control::now(&touch);
    let just_locked = control == Control::Locked && !game.was_locked;
    if game.was_locked && control != Control::Locked {
        game.unlocked_at = Some(time.elapsed_secs());
    }
    game.was_locked = control == Control::Locked;
    // Without the lock, the paddle stands in for the pointer.
    let hidden = control == Control::Unlocked;
    for mut options in &mut cursor {
        if options.visible == hidden {
            options.visible = !hidden;
        }
    }

    let busy = chat.is_open() || help.is_open() || settings.is_open();
    let (node, transform) = *canvas;
    // Physical pixels per canvas pixel; until the canvas has been laid out it has no size.
    let zoom = node.size().x / CANVAS.x as f32;
    let me = game.me();
    let online = game.opponent.is_some();
    let (low, high) = Rink::half(me);
    match control {
        _ if busy || zoom <= 0.0 => {}
        Control::Locked => {
            // From where the paddle is when the pointer was just locked, not counting how the
            // mouse moved before that (onto the table, to click it): no jump.
            // The player sees the rink turned their way round: so does the mouse.
            let moved = turned(me, motion.delta / zoom) - turned(me, Vec2::ZERO);
            game.target = Some(match game.target {
                Some(target) if !just_locked => (target + moved).clamp(low, high),
                _ => game.rink.paddles[me],
            });
            game.paddle_placed = true;
        }
        Control::Finger | Control::Unlocked => {
            let pointer = touch.finger().or(window.cursor_position());
            if let Some(at) = pointer {
                let corner = transform.translation - node.size() / 2.0;
                let on_canvas = (at * window.scale_factor() - corner) / zoom;
                let pointed = turned(me, on_canvas - RINK);
                if pointed.is_finite() {
                    // A finger has the paddle right under it. A hidden pointer pushed past a
                    // rail leaves the paddle there, and the rest is forgotten (`slack`).
                    if control == Control::Finger {
                        game.slack = Vec2::ZERO;
                    }
                    let wanted = pointed + game.slack;
                    let target = wanted.clamp(low, high);
                    if control == Control::Unlocked {
                        game.slack += target - wanted;
                    }
                    game.target = Some(target);
                }
            }
        }
        Control::Paused => {}
    }

    let pressed = !busy && (mouse.pressed(MouseButton::Left) || touch.finger().is_some());
    let fresh_press = pressed && !game.was_pressed;
    game.was_pressed = pressed;
    if game.rink.winner().is_some() {
        // Against someone, player 1 starts the next game, for both.
        if fresh_press && me == 0 {
            game.new_game();
            if let (Some(table), Some(opponent)) = (&game.table_id, &game.opponent) {
                seats::send(table, opponent.id, &Message::Start);
            }
        }
        return;
    }
    // Against the bot, waiting for a click, or for the chat or a panel to close, time stands
    // still. Against someone it can't: the paddle just stays where it is.
    if !online && (busy || control == Control::Paused) {
        return;
    }

    if !game.paddle_placed
        && let Some(target) = game.target
    {
        game.rink.paddles[me] = target.clamp(low, high);
        game.paddle_placed = true;
    }
    let mut targets = [Vec2::ZERO; 2];
    targets[me] = game.target.unwrap_or(game.rink.paddles[me]);
    targets[1 - me] = if online {
        game.their_paddle
    } else {
        game.bot.target(&game.rink, seconds)
    };
    // The puck is run by whoever's half it's in: the other rink follows, and doesn't score.
    let events = if !online || game.owner {
        game.rink.advance(targets, seconds)
    } else {
        game.rink.advance_following(targets, seconds)
    };
    for event in events {
        if let Event::Goal(scorer) = event {
            game.goal = Some((scorer, 0.0));
            // Let in here, and served again on this side: say so straight away.
            game.tick(false);
        }
    }
    if !online {
        return;
    }
    // How far the puck is into the other player's half (seat 0's is the bottom one).
    let into_theirs = if me == 0 {
        HEIGHT / 2.0 - game.rink.puck.y
    } else {
        game.rink.puck.y - HEIGHT / 2.0
    };
    if game.owner && into_theirs > HANDOVER_PAST {
        game.tick(true);
        game.owner = false;
    }
    game.since_tick += seconds;
    if game.since_tick >= TICK_EVERY {
        game.tick(false);
    }
}

fn draw(
    game: Res<Game>,
    canvas: Single<&ImageNode, With<Canvas>>,
    mut images: ResMut<Assets<Image>>,
    mut rink_art: Local<Option<Pixels>>,
) {
    let Some(mut image) = images.get_mut(&canvas.image) else {
        return;
    };
    let mut pixels = rink_art.get_or_insert_with(draw_rink).clone();
    let rink = &game.rink;
    // Turned the player's way round: their paddle at the bottom, in their colour.
    let me = game.me();
    for (seat, paddle) in rink.paddles.iter().enumerate() {
        let (body, knob) = PADDLES[usize::from(seat != me)];
        let at = turned(me, *paddle) + RINK;
        pixels.disc(at, PADDLE_RADIUS, body);
        pixels.disc(at, PADDLE_RADIUS * 0.45, knob);
        pixels.set((at.x - 3.0) as i32, (at.y - 4.0) as i32, SHINE);
    }
    let puck = turned(me, rink.puck) + RINK;
    pixels.disc(puck, PUCK_RADIUS, PUCK_RIM);
    pixels.disc(puck, PUCK_RADIUS - 1.0, PUCK);
    pixels.set((puck.x - 2.0) as i32, (puck.y - 2.0) as i32, SHINE);
    image.data = Some(pixels.into_bytes());
}

/// The rink without the puck and paddles: the surface with its air holes and markings, then the rails over
/// their edges, with a goal slot through each end rail.
fn draw_rink() -> Pixels {
    let mut pixels = Pixels::new(CANVAS);
    let from = RINK.as_ivec2();
    let to = from + IVec2::new(WIDTH as i32, HEIGHT as i32);
    pixels.rect(from, to, SURFACE);
    let spacing = AIR_HOLE_SPACING;
    for (row, y) in (spacing / 2..HEIGHT as i32)
        .step_by(spacing as usize)
        .enumerate()
    {
        let shift = if row % 2 == 0 { spacing / 2 } else { spacing };
        for x in (shift..WIDTH as i32 - 1).step_by(spacing as usize) {
            pixels.set(from.x + x, from.y + y, AIR_HOLE);
        }
    }
    // The centre line and circle, and a crease in front of each goal (the rails cut it in half).
    let middle = RINK + Vec2::new(WIDTH, HEIGHT) / 2.0;
    pixels.rect(
        IVec2::new(from.x, middle.y as i32),
        IVec2::new(to.x, middle.y as i32 + 1),
        MARKING_RED,
    );
    pixels.ring(middle, 14.0, MARKING_BLUE);
    for end in [RINK.y, RINK.y + HEIGHT] {
        pixels.ring(
            Vec2::new(middle.x, end),
            GOAL_WIDTH / 2.0 + 2.0,
            MARKING_BLUE,
        );
    }

    // The rails: an edge, then the rail, light catching its top; four bands round the surface.
    let bands = [
        (from - RAIL, IVec2::new(to.x + RAIL, from.y)),
        (IVec2::new(from.x - RAIL, to.y), to + RAIL),
        (IVec2::new(from.x - RAIL, from.y), IVec2::new(from.x, to.y)),
        (IVec2::new(to.x, from.y), IVec2::new(to.x + RAIL, to.y)),
    ];
    for (band_from, band_to) in bands {
        pixels.rect(band_from, band_to, RAIL_EDGE);
    }
    for (band_from, band_to) in bands {
        let inner_from = band_from.max(from - RAIL + 1);
        let inner_to = band_to.min(to + RAIL - 1);
        pixels.rect(inner_from, inner_to, RAIL_COLOR);
    }
    pixels.rect(
        from - RAIL + 1,
        IVec2::new(to.x + RAIL - 1, from.y - RAIL + 2),
        RAIL_LIGHT,
    );
    // The goals: slots through the end rails.
    let goal_from = from.x + ((WIDTH - GOAL_WIDTH) / 2.0) as i32;
    let goal_to = goal_from + GOAL_WIDTH as i32;
    pixels.rect(
        IVec2::new(goal_from, from.y - RAIL),
        IVec2::new(goal_to, from.y),
        GOAL_SLOT,
    );
    pixels.rect(
        IVec2::new(goal_from, to.y),
        IVec2::new(goal_to, to.y + RAIL),
        GOAL_SLOT,
    );
    // The creases reach further than the rails are deep: nothing is drawn beyond the table.
    let canvas = CANVAS.as_ivec2();
    let clear = [0, 0, 0, 0];
    pixels.rect(IVec2::ZERO, IVec2::new(canvas.x, from.y - RAIL), clear);
    pixels.rect(IVec2::new(0, to.y + RAIL), canvas, clear);
    pixels
}

fn show_text(game: Res<Game>, touch: Res<Touch>, mut texts: Query<(&Says, &mut Text)>) {
    let rink = &game.rink;
    let control = Control::now(&touch);
    let me = game.me();
    // The bot, or the other player by name, and player 1, who starts games against someone.
    let (them, first) = match &game.opponent {
        Some(opponent) => (
            opponent.names[1 - me].clone(),
            Some(opponent.names[0].clone()),
        ),
        None => ("Bot".to_string(), None),
    };
    for (says, mut text) in &mut texts {
        let line = match says {
            Says::TheirScore => format!("{them}\n{}", rink.score[1 - me]),
            Says::YourScore => format!("{}\nYou", rink.score[me]),
            Says::Status => match (rink.winner(), game.goal) {
                (Some(winner), _) => {
                    let who = if winner == me {
                        "You win".to_string()
                    } else {
                        format!("{them} wins")
                    };
                    let again = match (&first, touch.is_on()) {
                        (Some(first), _) if me != 0 => {
                            format!("Waiting for {first} to play again.")
                        }
                        (_, true) => "Tap to play again.".to_string(),
                        (_, false) => "Click to play again.".to_string(),
                    };
                    format!("{who}!\n{again}")
                }
                (None, Some((scorer, since))) if since < GOAL_SHOWN_FOR => {
                    if scorer == me {
                        "Goal for you!".to_string()
                    } else {
                        format!("Goal for {them}!")
                    }
                }
                (None, _) if control == Control::Paused && game.opponent.is_none() => {
                    "Click the table to play.\nEsc leaves.".to_string()
                }
                (None, _) if control == Control::Paused => {
                    "Click the table to take your paddle.\nEsc leaves.".to_string()
                }
                (None, _) => {
                    let how = match control {
                        Control::Finger => "Your finger moves the paddle.",
                        Control::Locked => "The mouse moves the paddle. Esc pauses.",
                        _ => "The mouse moves the paddle. Esc leaves.",
                    };
                    format!("First to {WINNING_SCORE}.\n{how}")
                }
            },
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// Puts the texts where they go over the canvas, sized with it.
fn place_text(
    window: Single<&Window>,
    canvas: Single<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
    mut texts: Query<(&OnCanvas, &mut Node, &mut TextFont)>,
) {
    let (node, transform) = *canvas;
    let scale_factor = window.scale_factor();
    // Logical pixels per canvas pixel, and where the canvas's corner is.
    let scale = node.size().x / CANVAS.x as f32 / scale_factor;
    let corner = (transform.translation - node.size() / 2.0) / scale_factor;
    for (on, mut text_node, mut font) in &mut texts {
        let font_size = FontSize::Px((on.size * scale).max(SMALLEST_TEXT).round());
        if font.font_size != font_size {
            font.font_size = font_size;
        }
        let left = Val::Px((corner.x + on.left * scale).round());
        let top = Val::Px((corner.y + on.top * scale).round());
        let width = Val::Px((on.width * scale).round());
        if text_node.left != left || text_node.top != top || text_node.width != width {
            text_node.left = left;
            text_node.top = top;
            text_node.width = width;
        }
    }
}

/// Esc leaves, unless the pointer is locked or was just let go: then it pauses (letting the
/// pointer go, if the browser didn't). Leave on a touch screen leaves.
fn leave(
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<Touch>,
    time: Res<Time>,
    game: Res<Game>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let esc = keys.just_pressed(KeyCode::Escape);
    let locked = pointer_locked();
    if esc && locked {
        release_pointer();
        return;
    }
    let just_let_go = game
        .unlocked_at
        .is_some_and(|at| time.elapsed_secs() - at < ESC_AFTER_UNLOCK);
    if (esc && !just_let_go) || touch.tapped(TouchButton::Leave) {
        mode.set(Mode::Walking);
    }
}

fn hide_rink(
    mut commands: Commands,
    mut game: ResMut<Game>,
    overlays: Query<Entity, With<Overlay>>,
    mut cursor: Query<&mut CursorOptions>,
) {
    pointer_lock_wanted(false);
    for mut options in &mut cursor {
        options.visible = true;
    }
    // Up from the table: whoever is left there plays the bot, and so does this player when they
    // come back alone, in a new game if they were playing someone.
    seats::stand();
    game.table_id = None;
    if game.opponent.is_some() {
        game.play_against(None);
    }
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
}
