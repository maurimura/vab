//! Two players at a pool table, through the room: the table is a place with two seats, as a
//! cabinet is ("pool:x,y"), and the players at it send each other what they do as messages
//! (web/index.html passes them on). Whoever sat first is player 1 and starts each game; the
//! player whose turn it is sends where their cue points, where they put the cue ball, their
//! shot, and where everything ended up once it stopped. Both tables play each shot out the
//! same way, and the other one snaps to where the shooter's ended, so they never drift apart.

use std::cell::RefCell;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

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

/// A message as it goes through the room: for this table, from one player to the other.
#[derive(Serialize, Deserialize)]
struct Envelope {
    pool: String,
    #[serde(flatten)]
    message: Message,
}

/// Who sits at the table this player is at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seats {
    /// The table, "pool:x,y".
    pub table: String,
    /// This player's id in the room.
    pub me: u32,
    /// Who sits in each seat, and their names.
    pub players: [Option<(u32, String)>; 2],
}

impl Seats {
    /// This player's seat, 0 or 1.
    pub fn mine(&self) -> Option<usize> {
        self.players
            .iter()
            .position(|player| player.as_ref().is_some_and(|(id, _)| *id == self.me))
    }

    /// The other player at the table, with their seat, if there is one.
    pub fn opponent(&self) -> Option<(usize, u32)> {
        let mine = self.mine()?;
        let other = 1 - mine;
        self.players[other].as_ref().map(|(id, _)| (other, *id))
    }

    pub fn name(&self, seat: usize) -> Option<&str> {
        self.players[seat].as_ref().map(|(_, name)| name.as_str())
    }
}

thread_local! {
    /// The latest seats at each pool table, from the room.
    static SEATS: RefCell<Vec<Seats>> = const { RefCell::new(Vec::new()) };
    /// Messages from the other player, oldest first.
    static INBOX: RefCell<Vec<(u32, Message)>> = const { RefCell::new(Vec::new()) };
}

/// Called by index.html when who sits at a pool table changes: `ids` per seat (0 when it's
/// empty: no player has that id, server/src/lib.rs) and their `names`, and this player's own
/// id. Ids are anything up to 2^32, so unsigned.
#[wasm_bindgen]
pub fn pool_seats(table: String, me: u32, ids: Vec<u32>, names: Vec<String>) {
    let players = [0, 1].map(|seat| {
        let id = *ids.get(seat)?;
        (id != 0).then(|| (id, names.get(seat).cloned().unwrap_or_default()))
    });
    let seats = Seats { table, me, players };
    SEATS.with_borrow_mut(|all| {
        all.retain(|other| other.table != seats.table);
        all.push(seats);
    });
}

/// Called by index.html with a message from another player at a pool table.
#[wasm_bindgen]
pub fn pool_message(from: u32, json: String) {
    if let Ok(envelope) = serde_json::from_str::<Envelope>(&json) {
        INBOX.with_borrow_mut(|inbox| inbox.push((from, envelope.message)));
    }
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Takes a seat at the pool table `table`.
    #[wasm_bindgen(js_name = poolSit)]
    fn pool_sit(table: &str);
    #[wasm_bindgen(js_name = poolStand)]
    fn pool_stand();
    /// Sends `json` (an envelope) to the player `to`.
    #[wasm_bindgen(js_name = poolSend)]
    fn pool_send(to: u32, json: &str);
}

/// How the room names a pool table: "pool:x,y", its first cell.
pub fn table_id(cell: IVec2) -> String {
    format!("pool:{},{}", cell.x, cell.y)
}

pub fn sit(table: &str) {
    INBOX.with_borrow_mut(Vec::clear);
    pool_sit(table);
}

pub fn stand() {
    pool_stand();
}

/// Sends `message` to the player `to`, at `table`.
pub fn send(table: &str, to: u32, message: Message) {
    let envelope = Envelope {
        pool: table.to_string(),
        message,
    };
    if let Ok(json) = serde_json::to_string(&envelope) {
        pool_send(to, &json);
    }
}

/// The seats at `table`, as the room last said.
pub fn seats_at(table: &str) -> Option<Seats> {
    SEATS.with_borrow(|all| all.iter().find(|seats| seats.table == table).cloned())
}

/// How many sit at `table`.
pub fn seated(table: &str) -> usize {
    seats_at(table).map_or(0, |seats| seats.players.iter().flatten().count())
}

/// The messages that came in since the last call, oldest first.
pub fn take_messages() -> Vec<(u32, Message)> {
    INBOX.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_go_through_json_unchanged() {
        let shot = Message::Shot {
            cue: [64.123_456, 63.987_654],
            aim: [0.707_106_77, -0.707_106_77],
            power: 0.333_333_34,
            settings: [30.0, 800.0, 100.0, 0.95, 0.75, 15.5],
        };
        let json = serde_json::to_string(&Envelope {
            pool: "pool:-5,-1".into(),
            message: shot.clone(),
        })
        .unwrap();
        let back: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pool, "pool:-5,-1");
        // Bit for bit, so both tables play the same shot.
        assert_eq!(back.message, shot);
    }

    #[test]
    fn seats_know_whose_is_whose() {
        // Ids past 2^31, as the room gives them.
        let seats = Seats {
            table: "pool:0,0".into(),
            me: 3_771_786_126,
            players: [
                Some((498_995_628, "Mauri".into())),
                Some((3_771_786_126, "Ivo".into())),
            ],
        };
        assert_eq!(seats.mine(), Some(1));
        assert_eq!(seats.opponent(), Some((0, 498_995_628)));
        assert_eq!(seats.name(0), Some("Mauri"));
    }
}
