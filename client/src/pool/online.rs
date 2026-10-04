//! Two players at a pool table, through the room: the table is a place with two seats, as a
//! cabinet is ("pool:x,y"), and the players at it send each other what they do as messages
//! (seats.rs). Whoever sat first is player 1 and starts each game; the
//! player whose turn it is sends where their cue points, where they put the cue ball, their
//! shot, and where everything ended up once it stopped. Both tables play each shot out the
//! same way, and the other one snaps to where the shooter's ended, so they never drift apart.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What one player at the table tells the other.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A new game: the rack's seed, and who breaks.
    Start { seed: u32, breaker: usize },
    /// Where the shooter's cue points, and how far it's pulled back (0 to 1).
    Aim { aim: [f32; 2], pull: f32 },
    /// Where the shooter has the cue ball, with ball in hand.
    Place { at: [f32; 2] },
    /// The shot: where the cue ball was, which way and how hard it went, and how the shooter's
    /// table plays (their settings), so both play it out alike.
    Shot {
        cue: [f32; 2],
        aim: [f32; 2],
        power: f32,
        settings: [f32; 6],
    },
    /// Where every ball ended up, in the table's order (x, y, and 1 if down), and what the shot
    /// did, for the rules.
    Settled {
        balls: Vec<[f32; 3]>,
        first_hit: Option<u8>,
        pocketed: Vec<u8>,
    },
}

/// How the room names a pool table: "pool:x,y", its first cell.
pub fn table_id(cell: IVec2) -> String {
    format!("pool:{},{}", cell.x, cell.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_go_through_json_unchanged() {
        let shot = Message::Shot {
            cue: [64.123_456, 63.987_654],
            aim: [0.707_106_77, -0.707_106_77],
            power: 0.333_333_34,
            settings: [30.0, 800.0, 100.0, 0.95, 0.75, 15.5],
        };
        let json = serde_json::to_string(&shot).unwrap();
        // Bit for bit, so both tables play the same shot.
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), shot);
    }
}
