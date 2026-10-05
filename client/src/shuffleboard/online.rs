//! Two players at a shuffleboard table, through the room: the table is a place with two seats,
//! as a cabinet is ("shuffleboard:x,y"), and the players at it send each other what they do as
//! messages (seats.rs). Someone sitting down gets the game as it is from the player who was
//! already there, so a reload or a lost connection doesn't end it; whoever sits in the first
//! seat throws red and starts each new game. The player whose throw it is sends where they have
//! the puck while they slide it about, the throw itself, and where everything ended up once it
//! stopped. Both tables play each throw out the same way, and the other one snaps to where the
//! thrower's ended, so they never drift apart.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use shuffleboard::{Puck, Settings};

/// What one player at the table tells the other.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A new game, `first` throwing first.
    Start { first: usize },
    /// Where the thrower has the waiting puck.
    Hold { at: Vec2 },
    /// The throw: where the puck left from, how fast, and how the thrower's table plays (their
    /// settings), so both play it out alike.
    Throw {
        from: Vec2,
        velocity: Vec2,
        settings: Settings,
    },
    /// Where every puck ended up once everything stopped: those on the table, and those that
    /// went off it, where they lie.
    Settled { pucks: Vec<Puck>, fallen: Vec<Puck> },
    /// The whole game, from a player who was at the table to one sitting down (after a reload,
    /// say), between throws.
    Sync {
        pucks: Vec<Puck>,
        fallen: Vec<Puck>,
        rules: shuffleboard::Game,
    },
}

/// How the room names a shuffleboard table: "shuffleboard:x,y", its first cell.
pub fn table_id(cell: IVec2) -> String {
    format!("shuffleboard:{},{}", cell.x, cell.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throws_go_through_json_unchanged() {
        let throw = Message::Throw {
            from: Vec2::new(23.456_789, 61.987_654),
            velocity: Vec2::new(-3.141_592_7, 412.333_33),
            settings: Settings {
                friction: 30.5,
                grip: 0.733_333_3,
                bounce: 0.85,
                max_speed: 580.0,
            },
        };
        let json = serde_json::to_string(&throw).unwrap();
        // Bit for bit, so both tables play the same throw.
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), throw);
    }
}
