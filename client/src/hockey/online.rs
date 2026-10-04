//! Two players at an air hockey table, through the room (seats.rs): the table has two seats,
//! "hockey:x,y". Both play on the same rink, seat 0's goal at its bottom and seat 1's at its
//! top; each sees it turned so that their own goal is at the bottom.
//!
//! The puck is run by the player in whose half it is: their hits land at once, wherever the
//! other player is. A few times a second each player says where their paddle is and, while the
//! puck is theirs, where it is and how it's going; the other one's rink follows that. When it
//! crosses the centre line the player running it hands it over. A goal is let in, and so
//! counted, by the player it went in on, who serves the puck again on their side and says how
//! many they've let in.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What one player at the table tells the other.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A new game (seat 0 starts them).
    Start,
    /// Where the sender's paddle is; while the puck is theirs, where it is and how it's going
    /// (x, y, then its velocity), and whether they're handing it over; and how many goals they've
    /// let in.
    Tick {
        paddle: [f32; 2],
        puck: Option<[f32; 4]>,
        handover: bool,
        conceded: u32,
    },
}

/// How the room names an air hockey table: "hockey:x,y", its first cell.
pub fn table_id(cell: IVec2) -> String {
    format!("hockey:{},{}", cell.x, cell.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_go_through_json_unchanged() {
        let tick = Message::Tick {
            paddle: [48.123_456, 140.987_65],
            puck: Some([12.5, 80.000_01, -312.333_33, 0.707_106_77]),
            handover: true,
            conceded: 3,
        };
        let json = serde_json::to_string(&tick).unwrap();
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), tick);
    }
}
