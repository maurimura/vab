//! Which thing in the bar the player can use: of the cabinets, tables and dartboards in the 8
//! cells around their feet, the nearest. Each kind has its own hint and E (cabinets.rs, pool,
//! hockey, shuffleboard, darts), but only the nearest thing offers itself, so where a cabinet
//! and a table are both next to the player, E uses the one they stand closest to. What they all
//! say and do is here too: the hint's words (`hint_text`), what E and F do (`chosen`: E plays,
//! or watches once every seat is taken; F watches), and on a touch screen the Play and Watch
//! buttons, one pair for everything, shown by whatever is nearest (`offer`).

use bevy::prelude::*;
use world::{Placed, cell_to_world, world_to_cell};

use crate::Mode;
use crate::player::Player;
use crate::touch::{self, Touch, TouchButton};

/// What a usable thing is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Cabinet,
    Pool,
    Hockey,
    Shuffleboard,
    Darts,
}

/// Everything in the bar the player can use: what it is, the cell it's placed on, and every
/// cell it covers.
#[derive(Resource, Default)]
pub struct Usables(Vec<(Kind, IVec2, Vec<IVec2>)>);

impl Usables {
    pub fn add(&mut self, kind: Kind, placed: &Placed) {
        self.0.push((kind, placed.cell(), placed.cells().collect()));
    }

    /// A one-cell thing (a cabinet).
    pub fn add_cell(&mut self, kind: Kind, cell: IVec2) {
        self.0.push((kind, cell, vec![cell]));
    }
}

/// The usable thing nearest the player, if one is next to them: what it is, and the cell it's
/// placed on.
#[derive(Resource, Default)]
pub struct Nearby(Option<(Kind, IVec2)>);

impl Nearby {
    /// What the nearest thing is, and the cell it's placed on.
    pub fn get(&self) -> Option<(Kind, IVec2)> {
        self.0
    }

    /// The cell of the nearest thing, if it's of `kind`.
    pub fn cell(&self, kind: Kind) -> Option<IVec2> {
        self.get()
            .filter(|(nearest, _)| *nearest == kind)
            .map(|(_, cell)| cell)
    }

    /// Of `placed`, things of `kind`, the one nearest the player, if it's nearer than anything
    /// else.
    pub fn of<'a>(&self, kind: Kind, placed: &'a [Placed]) -> Option<&'a Placed> {
        let cell = self.cell(kind)?;
        placed.iter().find(|placed| placed.cell() == cell)
    }
}

/// What E or F does with the thing nearby.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Use {
    /// Sits down to play.
    Sit,
    /// Watches the game being played there.
    Watch,
}

/// What a press does at a place with `seated` of its `seats` taken: E (`sit`) sits down, or
/// watches once every seat is taken; F (`watch`) watches whoever plays. Where nobody plays,
/// there is nothing to watch.
pub fn chosen(sit: bool, watch: bool, seated: u32, seats: u32) -> Option<Use> {
    if !sit && !watch {
        return None;
    }
    let watching = !sit || seated >= seats;
    match (watching, seated) {
        (true, 0) => None,
        (true, _) => Some(Use::Watch),
        (false, _) => Some(Use::Sit),
    }
}

/// The hint over a place that runs `title`, with `seated` of its `seats` taken and `watching`
/// people watching: what E and F do there, or on a touch screen (where the buttons say) who's
/// there.
pub fn hint_text(title: &str, seated: u32, seats: u32, watching: u32, touch: bool) -> String {
    let full = seated >= seats;
    match (seated, watching) {
        (0, 0) if touch => title.to_string(),
        (0, w) if touch => format!("{title} - {w} watching"),
        (n, 0) if touch => format!("{title} - {n} of {seats} playing"),
        (n, w) if touch => format!("{title} - {n} of {seats} playing, {w} watching"),
        (0, 0) => format!("E  {title}"),
        (0, w) => format!("E  {title} - {w} watching"),
        (n, 0) if full => format!("E  Watch {title} - {n} playing"),
        (n, w) if full => format!("E  Watch {title} - {n} playing, {w} watching"),
        (n, 0) => format!("E  {title} - {n} of {seats} playing, join in   F  Watch"),
        (n, w) => format!("E  {title} - {n} of {seats} playing, join in   F  Watch ({w} watching)"),
    }
}

/// A label shown above something the player can use, hidden until they're next to it.
pub fn hint_label() -> impl Bundle {
    (
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
        Visibility::Hidden,
    )
}

/// Shows a hint label with `label`, centered just above `above` (in world pixels).
pub fn place_hint(
    label: &str,
    above: Vec2,
    camera: (&Camera, &GlobalTransform),
    (text, node, visibility, computed): (&mut Text, &mut Node, &mut Visibility, &ComputedNode),
) {
    let (camera, camera_transform) = camera;
    let Ok(on_screen) = camera.world_to_viewport(camera_transform, above.extend(0.0)) else {
        return;
    };
    if text.0 != label {
        text.0 = label.to_string();
    }
    let size = computed.size() * computed.inverse_scale_factor();
    node.left = Val::Px((on_screen.x - size.x / 2.0).round());
    node.top = Val::Px((on_screen.y - size.y).round());
    *visibility = Visibility::Visible;
}

/// On a touch screen, shows the Play and Watch buttons that apply to the thing nearby and hides
/// the other: whatever is nearest calls this from its hint, so the two never disagree.
pub fn offer<'a>(
    buttons: impl IntoIterator<Item = (&'a TouchButton, Mut<'a, Visibility>)>,
    play: bool,
    watch: bool,
) {
    for (button, mut shown) in buttons {
        let offered = match button {
            TouchButton::Play => play,
            TouchButton::Watch => watch,
            _ => continue,
        };
        shown.set_if_neq(if offered {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

pub struct NearbyPlugin;

impl Plugin for NearbyPlugin {
    fn build(&self, app: &mut App) {
        // Before the hints and E look at it.
        app.init_resource::<Nearby>()
            .add_systems(Startup, spawn_buttons)
            .add_systems(PreUpdate, find.run_if(in_state(Mode::Walking)))
            .add_systems(
                Update,
                hide_buttons_away_from_things.run_if(in_state(Mode::Walking)),
            )
            .add_systems(OnExit(Mode::Walking), hide_buttons);
    }
}

/// The Play and Watch buttons for fingers, in the corner: one pair, for whatever is nearest.
fn spawn_buttons(mut commands: Commands, touch: Res<Touch>) {
    if !touch.is_on() {
        return;
    }
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            bottom: Val::Px(24.0),
            column_gap: Val::Px(10.0),
            ..default()
        },
        children![
            (
                touch::button(TouchButton::Watch, "Watch", 96.0, 48.0),
                Visibility::Hidden,
            ),
            (
                touch::button(TouchButton::Play, "Play", 96.0, 48.0),
                Visibility::Hidden,
            ),
        ],
    ));
}

/// With nothing nearby, nothing offers the buttons: they go.
fn hide_buttons_away_from_things(
    nearby: Res<Nearby>,
    buttons: Query<(&TouchButton, &mut Visibility)>,
) {
    if nearby.get().is_none() {
        hide_buttons(buttons);
    }
}

fn hide_buttons(mut buttons: Query<(&TouchButton, &mut Visibility)>) {
    offer(&mut buttons, false, false);
}

fn find(usables: Option<Res<Usables>>, player: Query<&Player>, mut nearby: ResMut<Nearby>) {
    let found = match (usables, player.single()) {
        (Some(usables), Ok(player)) => nearest(&usables.0, player.feet),
        _ => None,
    };
    if nearby.0 != found {
        nearby.0 = found;
    }
}

/// Of `usables`, those with a cell in the 8 around `feet`'s, the one with the cell nearest
/// `feet`.
fn nearest(usables: &[(Kind, IVec2, Vec<IVec2>)], feet: Vec2) -> Option<(Kind, IVec2)> {
    let standing = world_to_cell(feet);
    usables
        .iter()
        .filter_map(|(kind, first, covered)| {
            let distance = covered
                .iter()
                .filter(|cell| (**cell - standing).abs().max_element() == 1)
                .map(|cell| cell_to_world(cell.x, cell.y).distance(feet))
                .min_by(f32::total_cmp)?;
            Some((distance, *kind, *first))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, kind, first)| (kind, first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_of_a_cabinet_and_a_table_both_next_to_the_player() {
        // A table four cells long on row 4, a cabinet on row 2 above its second cell: the cells
        // on row 3 between them are next to both.
        let table: Vec<IVec2> = (-5..-1).map(|x| IVec2::new(x, 4)).collect();
        let usables = vec![
            (Kind::Shuffleboard, IVec2::new(-5, 4), table),
            (Kind::Cabinet, IVec2::new(-4, 2), vec![IVec2::new(-4, 2)]),
        ];
        let middle = cell_to_world(-4, 3);
        let toward = |cell: IVec2| (cell_to_world(cell.x, cell.y) - middle).normalize() * 5.0;
        // Standing to the table's side of the cell, the table; to the cabinet's, the cabinet.
        let by_table = middle + toward(IVec2::new(-4, 4));
        let by_cabinet = middle + toward(IVec2::new(-4, 2));
        assert_eq!(world_to_cell(by_table), IVec2::new(-4, 3));
        assert_eq!(world_to_cell(by_cabinet), IVec2::new(-4, 3));
        assert_eq!(
            nearest(&usables, by_table),
            Some((Kind::Shuffleboard, IVec2::new(-5, 4)))
        );
        assert_eq!(
            nearest(&usables, by_cabinet),
            Some((Kind::Cabinet, IVec2::new(-4, 2)))
        );
        // By the table's far end, only the table is near.
        let far_end = cell_to_world(-2, 3);
        assert_eq!(
            nearest(&usables, far_end),
            Some((Kind::Shuffleboard, IVec2::new(-5, 4)))
        );
        // Two cells away from both: nothing.
        assert_eq!(nearest(&usables, cell_to_world(-4, 0)), None);
    }

    #[test]
    fn e_sits_or_watches_a_full_place_and_f_watches() {
        assert_eq!(chosen(true, false, 0, 2), Some(Use::Sit));
        assert_eq!(chosen(true, false, 1, 2), Some(Use::Sit));
        assert_eq!(chosen(true, false, 2, 2), Some(Use::Watch));
        assert_eq!(chosen(false, true, 1, 2), Some(Use::Watch));
        // Nobody to watch, and nothing pressed.
        assert_eq!(chosen(false, true, 0, 2), None);
        assert_eq!(chosen(false, false, 1, 2), None);
    }

    #[test]
    fn the_hint_says_what_e_and_f_do() {
        assert_eq!(hint_text("Pool", 0, 2, 0, false), "E  Pool");
        assert_eq!(
            hint_text("Pool", 1, 2, 0, false),
            "E  Pool - 1 of 2 playing, join in   F  Watch"
        );
        assert_eq!(
            hint_text("Pool", 1, 2, 3, false),
            "E  Pool - 1 of 2 playing, join in   F  Watch (3 watching)"
        );
        assert_eq!(
            hint_text("Pool", 2, 2, 1, false),
            "E  Watch Pool - 2 playing, 1 watching"
        );
        // On a touch screen the buttons say what to press.
        assert_eq!(hint_text("Pool", 2, 2, 0, true), "Pool - 2 of 2 playing");
        assert_eq!(hint_text("Pool", 0, 2, 1, true), "Pool - 1 watching");
    }
}
