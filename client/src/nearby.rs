//! Which thing in the bar the player can use: of the cabinets and tables in the 8 cells around
//! their feet, the nearest. Each kind has its own hint and E (cabinets.rs, pool, hockey,
//! shuffleboard), but only the nearest thing offers itself, so where a cabinet and a table are
//! both next to the player, E uses the one they stand closest to.

use bevy::prelude::*;
use world::{Placed, cell_to_world, world_to_cell};

use crate::Mode;
use crate::player::Player;

/// What a usable thing is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Cabinet,
    Pool,
    Hockey,
    Shuffleboard,
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

pub struct NearbyPlugin;

impl Plugin for NearbyPlugin {
    fn build(&self, app: &mut App) {
        // Before the hints and E look at it.
        app.init_resource::<Nearby>()
            .add_systems(PreUpdate, find.run_if(in_state(Mode::Walking)));
    }
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
}
