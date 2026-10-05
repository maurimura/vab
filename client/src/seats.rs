//! Tables the players in the room sit at together, the pool, air hockey and shuffleboard
//! tables: each is a place with seats in the room, as a cabinet is ("pool:x,y", "hockey:x,y",
//! "shuffleboard:x,y"), and the players at one send each other messages about their game, which
//! web/index.html passes on. Each game has its own messages (pool/online.rs, hockey/online.rs,
//! shuffleboard/online.rs); here they go out as JSON with the table's name added, and come back
//! in for the table they're for.

use std::cell::RefCell;

use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

/// Who sits at the table this player is at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seats {
    /// The table, as the room names it.
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

    /// Both seats' names, "Player 1" and "Player 2" for anyone without one.
    pub fn names(&self) -> [String; 2] {
        [0, 1].map(|seat| match self.name(seat) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => format!("Player {}", seat + 1),
        })
    }
}

thread_local! {
    /// The latest seats at each table, from the room.
    static SEATS: RefCell<Vec<Seats>> = const { RefCell::new(Vec::new()) };
    /// Messages from other players: the table, who from, and the message as JSON, oldest first.
    static INBOX: RefCell<Vec<(String, u32, String)>> = const { RefCell::new(Vec::new()) };
}

/// Called by index.html when who sits at a table changes: `ids` per seat (0 when it's empty:
/// no player has that id, server/src/lib.rs) and their `names`, and this player's own id. Ids
/// are anything up to 2^32, so unsigned.
#[wasm_bindgen]
pub fn table_seats(table: String, me: u32, ids: Vec<u32>, names: Vec<String>) {
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

/// Called by index.html with a message from another player at a table, as JSON.
#[wasm_bindgen]
pub fn table_message(table: String, from: u32, json: String) {
    INBOX.with_borrow_mut(|inbox| inbox.push((table, from, json)));
}

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Takes a seat at `table`, which has `seats` of them.
    #[wasm_bindgen(js_name = tableSit)]
    fn table_sit(table: &str, seats: u32);
    #[wasm_bindgen(js_name = tableStand)]
    fn table_stand();
    /// Sends `json` (a message with its table) to the player `to`.
    #[wasm_bindgen(js_name = tableSend)]
    fn table_send(to: u32, json: &str);
}

/// Sits at `table`, forgetting anything said there before.
pub fn sit(table: &str) {
    INBOX.with_borrow_mut(|inbox| inbox.retain(|(at, ..)| at != table));
    table_sit(table, 2);
}

pub fn stand() {
    table_stand();
}

/// Sends `message` to the player `to`, at `table`.
pub fn send(table: &str, to: u32, message: &impl Serialize) {
    if let Some(json) = envelope(table, message) {
        table_send(to, &json);
    }
}

/// A message as it goes through the room: its own JSON object, with the table it's for.
fn envelope(table: &str, message: &impl Serialize) -> Option<String> {
    let mut value = serde_json::to_value(message).ok()?;
    value.as_object_mut()?.insert("table".into(), table.into());
    serde_json::to_string(&value).ok()
}

/// The seats at `table`, as the room last said.
pub fn seats_at(table: &str) -> Option<Seats> {
    SEATS.with_borrow(|all| all.iter().find(|seats| seats.table == table).cloned())
}

/// How many sit at `table`.
pub fn seated(table: &str) -> usize {
    seats_at(table).map_or(0, |seats| seats.players.iter().flatten().count())
}

/// The messages for `table` that came in since the last call, oldest first, with who sent them.
/// Ones that aren't messages of this kind are dropped.
pub fn take_messages<M: DeserializeOwned>(table: &str) -> Vec<(u32, M)> {
    INBOX.with_borrow_mut(|inbox| {
        let (here, elsewhere) = std::mem::take(inbox)
            .into_iter()
            .partition(|(at, ..)| at == table);
        *inbox = elsewhere;
        here.into_iter()
            .filter_map(|(_, from, json): (String, u32, String)| {
                Some((from, serde_json::from_str(&json).ok()?))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(tag = "kind", rename_all = "lowercase")]
    enum Said {
        Hello { at: [f32; 2] },
    }

    #[test]
    fn messages_go_out_with_their_table_and_come_back_unchanged() {
        let said = Said::Hello {
            at: [64.123_456, -0.707_106_77],
        };
        let json = envelope("pool:-5,-1", &said).unwrap();
        assert!(json.contains(r#""table":"pool:-5,-1""#));
        table_message("pool:-5,-1".into(), 7, json);
        table_message(
            "hockey:3,-1".into(),
            8,
            r#"{"kind":"hello","at":[1,2]}"#.into(),
        );
        // Bit for bit, so both tables play alike; the other table's message waits for it.
        assert_eq!(take_messages::<Said>("pool:-5,-1"), [(7, said)]);
        assert_eq!(take_messages::<Said>("hockey:3,-1").len(), 1);
    }

    #[test]
    fn seats_know_whose_is_whose() {
        // Ids past 2^31, as the room gives them.
        let seats = Seats {
            table: "pool:0,0".into(),
            me: 3_771_786_126,
            players: [
                Some((498_995_628, "Mauri".into())),
                Some((3_771_786_126, String::new())),
            ],
        };
        assert_eq!(seats.mine(), Some(1));
        assert_eq!(seats.opponent(), Some((0, 498_995_628)));
        assert_eq!(seats.names(), ["Mauri".to_string(), "Player 2".to_string()]);
    }
}
