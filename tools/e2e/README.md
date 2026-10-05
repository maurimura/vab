# Browser tests

Players in headless Chrome, against the site from `make dev`, for what only shows in a browser:
the bar, the tables, and two players in one room.

```sh
make dev     # in one terminal: the site at localhost:8787
make e2e     # in another: every test here
node tools/e2e/shuffleboard.mjs   # or just one
```

- `tables.mjs`: one player visits the pool, air hockey and shuffleboard tables.
- `nearest.mjs`: next to both a cabinet and a table, E uses the one the player stands nearer.
- `shuffleboard.mjs`: two players at the shuffleboard table: seats, throws alike on both
  tables, the held puck, a reload mid-game, a whole round, someone leaving.
- `voice.mjs`: two players at a cabinet hear each other (Chrome's fake microphone beeps), mute
  each other with Shift and a number, unmute with `/unmute`, and turn their microphones off with
  M and with a click. `GAME` picks the cabinet (Snow Bros. unless said).

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

`lib.mjs` has the helpers: `player(name, room)`, `use(player, object, mode)`,
`waitFor(player, test)`, `check(ok, message)`.

On a Mac, Chrome draws with the GPU, 60 frames a second; elsewhere, or with `RENDER=software`, in
software, at 5 to 15. `CHROME` says where Chrome is, and `BASE_URL` where the site is.
