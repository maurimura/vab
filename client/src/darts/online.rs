//! Two players at a dartboard, through the room: the board is a place with two seats, as a
//! cabinet is ("darts:x,y"), and the players at it send each other what they do as messages
//! (seats.rs). Someone arriving gets the game as it is from the player who was already there,
//! so a reload or a lost connection doesn't end it; whoever has the first seat throws red and
//! starts each new game. The player whose turn it is sends where their hand has the dart while
//! they aim, a few times a second, and where each dart landed: the thrower's hand decides that,
//! and both boards score it alike.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What one player at the board tells the other.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A new game, `first` throwing first.
    Start { first: usize },
    /// Where the thrower's hand has the dart (millimetres on the board), and whether they're
    /// holding the throw.
    Aim { at: Vec2, held: bool },
    /// A dart thrown, landing at `at`.
    Throw { at: Vec2 },
    /// The whole game, from a player who was at the board to one arriving (after a reload, say),
    /// between darts: where it's at, and the darts in the board.
    Sync {
        rules: darts::Game,
        stuck: Vec<Vec2>,
    },
}

/// How the room names a dartboard: "darts:x,y", its cell.
pub fn table_id(cell: IVec2) -> String {
    format!("darts:{},{}", cell.x, cell.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darts_go_through_json_unchanged() {
        let throw = Message::Throw {
            at: Vec2::new(-12.345_678, 102.987_65),
        };
        let json = serde_json::to_string(&throw).unwrap();
        // Bit for bit, so both boards score the same dart.
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), throw);
    }
}
