//! 8-ball between two players, judged shot by shot from what the table says happened.
//!
//! The table is open after the break: the first player to pocket a ball on a legal shot takes
//! that group, solids (1 to 7) or stripes (9 to 15), and the other player the other. A legal
//! shot hits one of the shooter's own balls first (any but the 8 while the table is open, the 8
//! once their group is down). Pocketing the cue ball, hitting the wrong ball first or nothing
//! at all is a foul: the turn passes, and the other player has ball in hand, putting the cue
//! ball wherever they like before shooting. A player keeps shooting while they pocket their own
//! balls. Pocketing the 8 after clearing their group, on a legal shot, wins; pocketing it any
//! other way loses, except on the break, where it goes back on its spot.

use crate::Table;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Group {
    Solids,
    Stripes,
}

impl Group {
    pub fn has(self, number: u8) -> bool {
        match self {
            Group::Solids => (1..=7).contains(&number),
            Group::Stripes => (9..=15).contains(&number),
        }
    }

    fn of(number: u8) -> Option<Group> {
        [Group::Solids, Group::Stripes]
            .into_iter()
            .find(|group| group.has(number))
    }

    fn other(self) -> Group {
        match self {
            Group::Solids => Group::Stripes,
            Group::Stripes => Group::Solids,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Foul {
    /// The cue ball went down.
    Scratch,
    /// The cue ball touched no ball.
    NoHit,
    /// The first ball the cue ball touched wasn't one the shooter may hit.
    WrongBall(u8),
}

/// How a game ended: who won, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Win {
    pub winner: usize,
    pub how: WinBy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WinBy {
    /// The winner pocketed the 8 after their group.
    Eight,
    /// The loser pocketed the 8 too early.
    EarlyEight,
    /// The loser pocketed the 8 on a foul.
    FoulOnEight,
}

/// What the rules made of a shot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Verdict {
    pub foul: Option<Foul>,
    /// The shooter took this group (and the other player the other).
    pub took: Option<Group>,
    /// The 8 dropped on the break and went back on its spot.
    pub eight_respotted: bool,
    /// The shooter shoots again.
    pub again: bool,
    pub win: Option<Win>,
}

/// A game of 8-ball between players 0 and 1.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Game {
    /// Who broke.
    pub breaker: usize,
    /// Whose shot it is.
    pub turn: usize,
    /// Each player's group, once the table isn't open any more.
    pub groups: Option<[Group; 2]>,
    /// The next shot is the break.
    pub breaking: bool,
    /// The shooter may put the cue ball anywhere first: the last shot was a foul.
    pub ball_in_hand: bool,
    pub win: Option<Win>,
    /// What the rules made of the last shot, if there's been one.
    pub last: Option<Verdict>,
    /// Every ball that has gone down this game and stayed down, in order, with who was
    /// shooting at the time.
    pub down: Vec<(u8, usize)>,
}

impl Game {
    /// A new game, `breaker` to break.
    pub fn new(breaker: usize) -> Self {
        Self {
            breaker,
            turn: breaker,
            groups: None,
            breaking: true,
            ball_in_hand: false,
            win: None,
            last: None,
            down: Vec::new(),
        }
    }

    /// The balls of `player`'s group that are down, in the order they went in.
    pub fn down_of(&self, player: usize) -> Vec<u8> {
        let Some(group) = self.group_of(player) else {
            return Vec::new();
        };
        self.down
            .iter()
            .map(|&(number, _)| number)
            .filter(|&number| group.has(number))
            .collect()
    }

    /// Whether the 8 went down on `player`'s shot.
    pub fn sank_eight(&self, player: usize) -> bool {
        self.down.contains(&(8, player))
    }

    pub fn group_of(&self, player: usize) -> Option<Group> {
        self.groups.map(|groups| groups[player])
    }

    /// The balls `player` may hit first on their next shot: their group's still on the table,
    /// or the 8 once those are down; any but the 8 while the table is open.
    pub fn may_hit(&self, player: usize, table: &Table, number: u8) -> bool {
        if self.breaking {
            return number != 0;
        }
        match self.group_of(player) {
            None => number != 0 && number != 8,
            Some(group) if cleared(table, group, &[]) => number == 8,
            Some(group) => group.has(number),
        }
    }

    /// Judges the shot just played on `table` (its shot log), once everything has stopped, and
    /// moves the game on. Puts the 8 back on its spot if it dropped on the break.
    pub fn judge(&mut self, table: &mut Table) -> Verdict {
        let shooter = self.turn;
        let shot = table.shot.clone();
        let mut verdict = Verdict::default();
        if self.win.is_some() {
            return verdict;
        }
        for &number in shot.pocketed.iter().filter(|&&number| number != 0) {
            self.down.push((number, shooter));
        }

        // Who could the shooter hit, before this shot's balls dropped?
        let allowed = |number: u8| {
            if self.breaking {
                return true;
            }
            match self.group_of(shooter) {
                None => number != 8,
                Some(group) if cleared(table, group, &shot.pocketed) => number == 8,
                Some(group) => group.has(number),
            }
        };
        verdict.foul = if shot.pocketed.contains(&0) {
            Some(Foul::Scratch)
        } else {
            match shot.first_hit {
                None => Some(Foul::NoHit),
                Some(number) if !allowed(number) => Some(Foul::WrongBall(number)),
                Some(_) => None,
            }
        };

        if shot.pocketed.contains(&8) {
            let group_was_down = self
                .group_of(shooter)
                .is_some_and(|group| cleared(table, group, &shot.pocketed));
            if self.breaking {
                table.respot(8);
                self.down.retain(|&(number, _)| number != 8);
                verdict.eight_respotted = true;
            } else {
                let how = match (group_was_down, verdict.foul) {
                    (true, None) => WinBy::Eight,
                    (true, Some(_)) => WinBy::FoulOnEight,
                    (false, _) => WinBy::EarlyEight,
                };
                let winner = if how == WinBy::Eight {
                    shooter
                } else {
                    1 - shooter
                };
                verdict.win = Some(Win { winner, how });
                self.win = verdict.win;
                self.last = Some(verdict);
                return verdict;
            }
        }

        // The first ball of a group pocketed on a legal shot after the break decides the groups.
        if self.groups.is_none() && !self.breaking && verdict.foul.is_none() {
            verdict.took = shot.pocketed.iter().find_map(|&number| Group::of(number));
            if let Some(group) = verdict.took {
                let mut groups = [group.other(); 2];
                groups[shooter] = group;
                self.groups = Some(groups);
            }
        }

        verdict.again = verdict.foul.is_none()
            && match (self.breaking, self.group_of(shooter)) {
                // Anything down on the break keeps the breaker at the table.
                (true, _) => shot.pocketed.iter().any(|&number| number != 0),
                (false, Some(group)) => shot.pocketed.iter().any(|&number| group.has(number)),
                (false, None) => false,
            };
        if !verdict.again {
            self.turn = 1 - shooter;
        }
        self.ball_in_hand = verdict.foul.is_some();
        self.breaking = false;
        self.last = Some(verdict);
        verdict
    }
}

/// Every ball of `group` is down, not counting those in `just_now` (dropped on this shot).
fn cleared(table: &Table, group: Group, just_now: &[u8]) -> bool {
    table
        .balls
        .iter()
        .filter(|ball| group.has(ball.number))
        .all(|ball| ball.pocketed && !just_now.contains(&ball.number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shot;

    /// A racked table where `down` are already pocketed and the last shot did `shot`.
    fn after(down: &[u8], first_hit: Option<u8>, pocketed: &[u8]) -> Table {
        let mut table = Table::racked(1);
        for ball in &mut table.balls {
            if down.contains(&ball.number) || pocketed.contains(&ball.number) {
                ball.pocketed = true;
            }
        }
        table.shot = Shot {
            first_hit,
            pocketed: pocketed.to_vec(),
        };
        table
    }

    /// A game past the break, player 0 to shoot, solids for player 0 if `groups`.
    fn playing(groups: bool) -> Game {
        Game {
            breaker: 1,
            turn: 0,
            groups: groups.then_some([Group::Solids, Group::Stripes]),
            breaking: false,
            ball_in_hand: false,
            win: None,
            last: None,
            down: Vec::new(),
        }
    }

    #[test]
    fn each_player_sees_their_groups_balls_in_the_order_they_went_in() {
        let mut game = Game::new(0);
        game.judge(&mut after(&[], Some(1), &[12, 3]));
        game.judge(&mut after(&[12, 3], Some(5), &[5, 9]));
        assert_eq!(game.groups, Some([Group::Solids, Group::Stripes]));
        assert_eq!(game.down_of(0), [3, 5]);
        assert_eq!(game.down_of(1), [12, 9]);
        assert!(!game.sank_eight(0));
    }

    #[test]
    fn something_down_on_the_break_keeps_the_breaker_at_the_table() {
        let mut game = Game::new(0);
        let verdict = game.judge(&mut after(&[], Some(1), &[3]));
        assert!(verdict.again && verdict.took.is_none());
        assert_eq!((game.turn, game.groups, game.breaking), (0, None, false));

        let mut game = Game::new(0);
        assert!(!game.judge(&mut after(&[], Some(1), &[])).again);
        assert_eq!(game.turn, 1);
    }

    #[test]
    fn the_first_ball_down_after_the_break_decides_the_groups() {
        let mut game = playing(false);
        let verdict = game.judge(&mut after(&[], Some(12), &[12, 3]));
        assert_eq!(verdict.took, Some(Group::Stripes));
        assert_eq!(game.groups, Some([Group::Stripes, Group::Solids]));
        assert!(verdict.again);
        assert_eq!(game.turn, 0);
    }

    #[test]
    fn hitting_the_other_group_first_is_a_foul_and_the_turn_passes() {
        let mut game = playing(true);
        let verdict = game.judge(&mut after(&[], Some(11), &[2]));
        assert_eq!(verdict.foul, Some(Foul::WrongBall(11)));
        assert!(!verdict.again);
        assert_eq!(game.turn, 1);
        assert!(game.ball_in_hand);
        // The next shot, a fair one, puts the ball back in play.
        game.judge(&mut after(&[], Some(12), &[]));
        assert!(!game.ball_in_hand);
    }

    #[test]
    fn scratching_or_hitting_nothing_is_a_foul() {
        let mut game = playing(true);
        assert_eq!(
            game.judge(&mut after(&[], Some(2), &[2, 0])).foul,
            Some(Foul::Scratch)
        );
        let mut game = playing(true);
        assert_eq!(
            game.judge(&mut after(&[], None, &[])).foul,
            Some(Foul::NoHit)
        );
    }

    #[test]
    fn pocketing_only_the_other_groups_balls_passes_the_turn() {
        let mut game = playing(true);
        let verdict = game.judge(&mut after(&[], Some(2), &[10]));
        assert!(verdict.foul.is_none() && !verdict.again);
        assert_eq!(game.turn, 1);
    }

    #[test]
    fn the_eight_wins_after_the_group_and_loses_before_it_or_on_a_foul() {
        let solids = [1, 2, 3, 4, 5, 6, 7];
        let mut game = playing(true);
        let verdict = game.judge(&mut after(&solids, Some(8), &[8]));
        assert_eq!(
            verdict.win,
            Some(Win {
                winner: 0,
                how: WinBy::Eight
            })
        );

        let mut game = playing(true);
        let verdict = game.judge(&mut after(&[1, 2], Some(3), &[8]));
        assert_eq!(
            verdict.win.map(|win| (win.winner, win.how)),
            Some((1, WinBy::EarlyEight))
        );

        let mut game = playing(true);
        let verdict = game.judge(&mut after(&solids, Some(8), &[8, 0]));
        assert_eq!(
            verdict.win.map(|win| (win.winner, win.how)),
            Some((1, WinBy::FoulOnEight))
        );

        // The last solid and the 8 on the same shot: the 8 came too early.
        let mut game = playing(true);
        let verdict = game.judge(&mut after(&[1, 2, 3, 4, 5, 6], Some(7), &[7, 8]));
        assert_eq!(verdict.win.map(|win| win.winner), Some(1));
    }

    #[test]
    fn once_the_group_is_down_only_the_eight_may_be_hit_first() {
        let solids = [1, 2, 3, 4, 5, 6, 7];
        let game = playing(true);
        let table = after(&solids, None, &[]);
        assert!(game.may_hit(0, &table, 8));
        assert!(!game.may_hit(0, &table, 9));
        let mut game = playing(true);
        let verdict = game.judge(&mut after(&solids, Some(12), &[]));
        assert_eq!(verdict.foul, Some(Foul::WrongBall(12)));
    }

    #[test]
    fn the_eight_on_the_break_goes_back_on_its_spot() {
        let mut game = Game::new(0);
        let mut table = after(&[], Some(1), &[8]);
        let verdict = game.judge(&mut table);
        assert!(verdict.eight_respotted && verdict.win.is_none());
        let eight = table.balls.iter().find(|ball| ball.number == 8).unwrap();
        assert!(!eight.pocketed);
        assert!(verdict.again);
    }
}
