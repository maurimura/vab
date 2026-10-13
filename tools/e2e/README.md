# Browser tests

Players in headless Chrome, against the site from `make dev`, for what only shows in a browser:
the bar, the tables, and two players in one room.

```sh
make dev     # in one terminal: the site at localhost:8787
make e2e     # in another: every test here
node tools/e2e/shuffleboard.mjs   # or just one
```

- `tables.mjs`: one player visits the pool, air hockey and shuffleboard tables.
- `pick-seat.mjs`: three players at The Simpsons (`GAME=ssriders` for Sunset Riders), whose
  seats are characters: the hint lists them with their number keys, 3 sits one as Bart, the
  next can't take Bart and picks Homer with 2, both see who the other plays, and E sits the
  third as the first free one, Marge. At Mortal Kombat II the hint offers P1 and P2, and 2
  sits the player as player 2.
- `controls.mjs`: the controls panel for every game and for one. The gear in the bar opens it
  (and closes it); Enter on Your name opens the page's name box, where Esc keeps the name and
  a new one and Enter sets it (cookie too); the arrows turn its wheel (and don't walk), + slows every game's, Steer left
  goes on K, and both Mortal Kombat II and Out Run (`GAME=crusnusa41` for Cruis'n USA) have K.
  The gear at Out Run: the arrows still turn the game's wheel, + slows its own more, Steer left
  goes on J (not on Y, the chat's) for Out Run alone, Esc closes the panel only; kept over a
  reload (Tab opens it then); R at the game puts back every game's, R in the bar (`/controls`)
  the usual.
- `onboarding.mjs`: the first screen is for the first visit: back again (a reload), a player
  goes straight in with the same name and look, without the picker; `/look` still opens it.
- `darts.mjs`: one player at the dartboard: scoring, turns, a bust and a win.
- `darts-online.mjs`: two players at the dartboard: seats, the thrower's hand, darts alike on
  both boards, a reload mid-turn, someone leaving.
- `pinball.mjs`: the Saloon pinball machine: the plunger, the flippers, the next ball.
- `nearest.mjs`: next to both a cabinet and a table, E uses the one the player stands nearer.
- `talk.mjs`: talking to the staff: the way to the bartender on a first visit, the bartender
  coming across the counter, a game picked and the way there shown, Spanish picked (the titles
  staying as they are) and kept over a reload, the cleaner stopping to talk, and walking away.
- `staff.mjs`: two players see the same bartender and cleaner at the same stop of their shift;
  the bartender keeps behind the counter and the cleaner out of it; a shift later the next of
  each takes over for both.
- `shuffleboard.mjs`: two players at the shuffleboard table: seats, throws alike on both
  tables, the held puck, a reload mid-game, a whole round, someone leaving.
- `voice.mjs`: two players hear each other (Chrome's fake microphone beeps) at a cabinet, then at
  each table. At the cabinet they mute each other with Shift and a number, unmute with
  `/unmute`, turn their microphones off with M and with a click, and standing up ends the call;
  at the pool table a click on the panel leaves the cue alone. `GAME` picks the cabinet (Snow
  Bros. unless said).
- `watch.mjs`: a second player watches each table (F): the pool rack and where a shot ends, the
  air hockey rink as the player has it, where a shuffleboard throw stops, a dart scored on the
  board.

Local builds of the client have hooks for these (`client/src/testing.rs`, on `window.vab`),
which the site never has:

- `vab.state()`: what the game is doing: the mode, where the player is, each table's game
  (`pool`, `hockey`, `shuffleboard`) and who's in the voice panel (`voice`), so a test checks
  the game itself, not pixels.
- `vab.goTo(name)`: puts the player next to an object (`"shuffleboard"`, `"pool_table"`,
  `"air_hockey"`, or a cabinet's game), for E to use, rather than walking there.
- `vab.standAt(x, y, towardX, towardY)`: puts the player in a cell, leaning toward another;
  `vab.state().nearby` says which thing E would use.
- `vab.throw(vx, vy)`: throws the waiting shuffleboard puck at exactly that speed.
- `vab.throwDart(x, y)`: throws a dart that lands exactly there (millimetres from the board's
  middle, y up).
- `vab.clockAhead(seconds)`: the staff go by a clock that far ahead of the room's, for a later
  shift (`vab.state().staff` says who's on and where; `vab.state().talk`, what they're saying).

`lib.mjs` has the helpers: `player(name, room)`, `use(player, object, mode)`,
`waitFor(player, test)`, `check(ok, message)`.

On a Mac, Chrome draws with the GPU, 60 frames a second; elsewhere, or with `RENDER=software`, in
software, at 5 to 15. `CHROME` says where Chrome is, and `BASE_URL` where the site is.
