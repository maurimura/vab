# Sega Model 2 games

Each Model 2 game here is a static recompilation of that game's own programs, not an emulator:
the i960 code (and, for Daytona, the TGP and sound programs) is C generated from the ROM set,
with just enough of the board around it. A recomp runs one game, so each game is its own module
with its own runtime, and what the modules share is ours: the libretro shim pattern, save states,
the build and the harnesses.

- [`srally/`](srally/README.md): Sega Rally Championship (Model 2A), on
  [segarally95-recomp](https://github.com/xandoxan65/segarally95-recomp).
- [`../daytona/`](../daytona/README.md): Daytona USA (Model 2), on
  [daytona-arcade-recomp](https://github.com/alphanu1/daytona-arcade-recomp). Still at the top
  level; it moves to `model2/daytona/` in a change of its own (the paths are in the Makefile, the
  Worker's routes and the labs).
