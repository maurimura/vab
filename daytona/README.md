# Daytona USA in the browser

Daytona USA (Sega Model 2, 1994; MAME's set `daytona`, Revision A) behind the libretro API, so the
bar's emulator worker (`web/emulator/libretro.js`, `worker.js`) runs it like the FBNeo and
Supermodel cores. It is not an emulator: it is
[daytona-arcade-recomp](https://github.com/alphanu1/daytona-arcade-recomp), a static
recompilation. The game's i960 program, the TGP (geometry DSP) program and the sound board's
68000 program are recompiled to C++ at build time from your ROM set; the fixed-function hardware
(geometrizer, software rasterizer, tilemaps, YM3438 through ymfm, MultiPCM, I/O board, comm
board) is native C++. No interpreter, no JIT, no GPU: a frame is the game's own code plus a
software rasterizer, and the picture is a CPU framebuffer.

One libretro machine is **one to eight linked cabinets** (two by default): the cabinets'
communication boards are wired to each other in memory, so a two-player race is one
deterministic machine that every player's computer runs (lockstep / rollback netplay), each
showing their own cabinet. Up to eight in a ring, with options to delay, cut, pause or ghost a
cabinet's link and exports to save and load one cabinet alone, are for the "arcade mode"
experiments in [ring-notes.md](ring-notes.md) (`harness/ring-lab.mjs`). The arcade mode itself
is **one cabinet a machine** (`cabinets=1`, `link_topology=star`, `seat=k`): each player's browser
runs their own seat's cabinet, started from that seat's preset state, and the frontend carries
the cabinets' 448-byte link blocks between them (below, "The arcade mode").

- `build.sh` fetches the pinned recomp (`1877da9`, 2026-10-06) and its pinned SoftFloat 3e and
  ymfm into `.cache/recomp`, applies `patches/`, builds the recomp's own tools natively (CMake),
  generates the game code from the ROM set (or links a stub without one), and `core.mk` compiles
  three outputs: `dist/daytona.mjs` + `.wasm` (web and worker), `dist/headless/` (Node) and
  `.cache/native/bench` (the same code natively, the baseline).
- `shim/libretro.cpp`: the libretro surface (cabinets, inputs, sound to 48 kHz, states, link
  wires); `shim/gen_stub.cpp`: the stand-in game code; `shim/snapshot_stub.cpp`: the stand-in
  for the runtime's save states until `patches/0001-snapshot.patch` lands; `shim/main_native.cpp`:
  the native bench.
- `patches/0002-board-draw-switch.patch`: `M2Board::set_draw(bool)`, so the cabinet nobody is
  watching is not rasterized (nothing the game reads changes). `patches/0003-board-release-copied-images.patch`:
  the board drops the ROM images the geometrizer, TGP and sound board already copied (~48 MB a
  cabinet).
- `check.mjs` (Node: smoke test, determinism, save states, drawing, link), `bench.mjs` (Node
  timings), `make-nvram.mjs` + `nvram/` (the cabinets' settings presets), `make-states.mjs` (the
  arcade mode's seat states), `harness/` (a page to play it with keyboard and sound, a bench page
  run in a headless Chrome of our own, the ring lab, and `arcade-check.mjs`: seats in separate
  Node processes over a relay).

## Licence and what is never committed

The recomp is BSD-3-Clause (its runtime carries MAME-derived code under BSD-3 too); SoftFloat 3e
and ymfm are BSD-3-Clause. The game code generated from the ROM set, the imported images and
any core built from them are derived from Sega's ROMs: they live only in `.cache/` and `dist/`
(git-ignored, `daytona/.gitignore`) and are never committed. Note that a `dist/` built with the
ROM set contains the recompiled game: `make daytona` / `make upload-daytona` put it in R2 next
to the ROM set itself, and it should be served the same way (not published anywhere else).

## Building

```sh
./daytona/build.sh [web] [headless] [native]   # all three when none given
```

- **Without the ROM set**: the core links `shim/gen_stub.cpp`. Everything is compiled and
  linked (runtime, SoftFloat, ymfm, shim) and the recomp's tools are built natively, so the whole
  toolchain is proven; the core loads no game (with a ROM set it loads, warns, and its first
  frame stops with "no recompiled code", caught and logged). `DAYTONA_GEN=stub` forces this.
- **With the ROM set**: put MAME's `daytona.zip` (Revision A; a zip that also carries the
  `daytona93` clone's files is fine) at `~/Downloads/daytona.zip`, or say where:
  `ROMS=/path/to/dir ./daytona/build.sh`. The build then runs, as the recomp's
  `scripts/recompile.py --set daytona` does: `m2import daytona.zip .cache/rom_cache/daytona`,
  `m2recomp program.bin .cache/gen/daytona --seeds seeds/daytona.txt --hooks seeds/daytona_hooks.txt`,
  `m2tgprecomp tgp_program.bin .cache/gen/daytona_tgp/tgp_gen.cpp`,
  `m2sndrecomp sound_program.bin .cache/gen/daytona_snd/snd_gen.cpp`, and links the real core
  (regenerating only when the ROM set, the tools or the seeds change). A set that is not
  `daytona` (e.g. `daytona93`, which has no link board) is rejected with the file that is wrong,
  exit status 3.
- Pinned emsdk 6.0.10 from `$EMSDK_DIR` (default `emulator/.cache/emsdk`, installed by
  `emulator/emsdk.sh` if missing). In a worktree, a symlink `emulator/.cache -> <main
  checkout>/emulator/.cache` reuses the main checkout's (git-ignored). Native: clang/clang++
  (`NATIVE_CC`/`NATIVE_CXX`), CMake for the tools; no Ninja needed.
- Knobs (environment): `JOBS`, `OPT` (default `-O3`), `PERF_FLAGS` (e.g. `-flto -msimd128`),
  `DEBUG=1` (Emscripten assertions), `M2RECOMP_CHUNK` (i960 instructions per generated
  function, `m2recomp --chunk`, default 1500 as m2recomp's: see Measured). Always
  `-fno-fast-math -ffp-contract=off`: exact rounding is the recomp's rule (a fused multiply-add
  rounds once where the TGP's code rounds twice).
- Times on an M1 Max (10 cores): all three targets from clean, stub game code, 68 s (wasm
  350 KB); with the ROM set, from import to the last link, 4 min 3 s (wasm 4.4 MB).

## The contract (what the worker and the page rely on)

```js
import createDaytona from "/daytona/dist/daytona.mjs";   // -sMODULARIZE -sEXPORT_ES6
const core = await Core.create(createDaytona, callbacks, { canvas }); // any canvas is ignored
core.setOption("cabinets", "2");                         // before loading (default 2)
core.loadGame("daytona.zip", bytes); // 496x384, 4:3, 57.524 Hz, 48000 Hz stereo
core.setOption("view", "1");                             // any time
```

- **Exports**: supermodel's set (`exports.json`) with `_daytona_set(key, value)` and
  `_daytona_timings()` in place of the `supermodel_` ones, plus `_daytona_link_status()` (JSON)
  and `_daytona_save_nvram(cabinet, dir)`; for the ring experiments `_daytona_link_stats()`
  (JSON), `_daytona_link_frame(cabinet, sent)`, `_daytona_cabinet_save(cabinet, size_t *size)`,
  `_daytona_cabinet_load(cabinet, data, size)` and `_daytona_cabinet_reset(cabinet)` (Options,
  below); for the arcade mode `_daytona_link_block_size()`, `_daytona_link_out(dst)`,
  `_daytona_link_in(seat, src, len)` and `_daytona_link_absent(seat)` ("The arcade mode",
  below). Same memory settings as Supermodel (256 MB initial,
  growth to 2 GB, 4 MB stack, table growth, forced file system). No WebGL, no canvas.
- **AV**: 496x384, aspect 4:3, **fps = 57.524 Hz** (`16 MHz / (656 x 424)`, the board's own
  timing), sample rate 48000. Pixel format XRGB8888: the board's `0xAARRGGBB` screen goes to
  `video_cb` as it is (pitch 496 x 4), no conversion in the core.
- **Sound**: each `retro_run` turns that frame's YM3438 output (55.6 kHz) and MultiPCM output
  (44.6 kHz) into 48 kHz stereo int16 (834 or 835 frames a frame) by linear interpolation at
  exact rational positions (output sample j reads input j x 125/108 and j x 625/672), mixed and
  clamped as the recomp's `m2run --wav` does. Nothing is dropped or doubled across frames, the
  carry is in the save state, and it is the same on every machine. The sound is the viewed
  cabinet's.
- **Cabinets**: `cabinets` = 1 to 8 (default 2), read at `retro_load_game`. Cabinet k is driven
  by RetroPad port k. With two, cabinet 0's board sends to cabinet 1's and back (in-memory
  cables, the recomp's `tests/test_comm_board.cpp` transport); each `retro_run` runs one frame of
  every cabinet in order. With more, they are a ring the other way round, 0 -> N-1 -> ... -> 1
  -> 0, run in that order, so that the master's numbering gives cabinet k link id k + 1 and with
  no delay the master's frame goes all round in one `retro_run` (two cabinets: the same as
  ever). The boards poll their cables themselves (at vblank and when the game
  reads the comm flag), as on TCP in the recomp's app; frame sync is off (the cabinets are in
  step by construction, and a sync wait could never be met in one thread). So cabinet 1 sees
  cabinet 0's data of the same frame, cabinet 0 sees cabinet 1's a frame later: fixed, hence
  deterministic. A cable holding 32 link frames (the next board stopped reading) refuses more
  and the board loses the link, as the app's TCP send limit would.
- **View**: `view` = 0 to cabinets - 1 (default 0; out of range shows 0), any time: whose picture
  and sound the frontend gets. Not machine state (not in save states): every player's machine is
  the same whatever it shows. The other cabinets are not rasterized (`patches/0002`): their game, geometrizer
  and the video caches' inputs run as ever, and the first frame it is shown again is composed
  whole (both checked by `check.mjs` with the game). Frames the frontend runs without video
  (`GET_AUDIO_VIDEO_ENABLE`, rollback re-runs) rasterize nothing. `draw_hidden=1` draws both.
- **Inputs** (per cabinet, RetroPad ids as libretro.js answers them; the bar's keys in brackets):
  LEFT/RIGHT steer [arrows], UP accelerator (0xe0 held, else 0x20) [up], DOWN brake [down],
  B shift down [Z], A shift up [X] (sequential gears 1-4 on presses, starting in 1, as the app's
  `Controls::sample`), Y/X/L/R view buttons VR1-VR4 [A S D C], START start [1], SELECT coin [5].
  Steering is a deterministic integer ramp toward +-0x60 from centre while LEFT/RIGHT is held
  and back to centre otherwise, in two stages: `steer_slow` ADC units a frame (default 3) for
  the first `steer_slow_frames` (default 16) of a press that turns the wheel away from centre,
  then `steer_step` a frame (default 12: the app's 0.12 of full lock), which is also the rate
  when counter-steering and back to centre. So a tap is a small correction and a hold reaches
  full lock in about a third of a second. The slow stage is what makes the game's circuit
  select usable from keys: it follows the wheel's position (measured on the game: +16..+64 from
  centre highlights ADVANCED, +72 and over EXPERT, centre BEGINNER; stepping on the accelerator
  chooses the highlighted one), so a short hold of RIGHT with UP pressed picks ADVANCED and a
  hold of half a second or more EXPERT. (With the old single-stage ramp the ADVANCED band lasted
  4 frames.) The wheel position, the frames the key has been held, gear and last pad (for shift
  edges) are machine state, in the save state (format version 2). Input descriptors for every port: "Steer left", "Steer right", "Accelerate", "Brake",
  "Shift down", "Shift up", "View 1".."View 4", "Start", "Coin".
- **Options** (`daytona_set`, strings): `cabinets`, `view`, `steer_step`, `steer_slow`,
  `steer_slow_frames`, `nvram_dir` (default
  `/nvram`), `presets` (`0`: skip the built-in settings presets), `draw_hidden`, `script0` ..
  `script7` (an input script file in the module's FS driving that cabinet instead of its pad, the
  recomp's `scripts/inputs` format, as `m2run --inputs`), `test_blank_images` (toolchain test: a
  machine on zeroed ROM images, no ROM set needed) and with it `test_program` (an i960 program
  image of our own instead of the game's, no sound board: for timing generated code); `seat` and
  `link_absent` for the arcade mode (below). Unknown keys are logged. `bench.mjs` passes more as `OPTIONS="key=value,..."`, the native bench as
  `DAYTONA_OPTIONS`.
- **Link experiments** (options; none of them machine state or in save states; all off by
  default, so nothing changes unless set; findings in [ring-notes.md](ring-notes.md)):
  - `link_delay` = n: each cable holds what is written to it n `retro_run`s before the next
    cabinet can read it (0: the same `retro_run`); `link_jitter` = j: up to j more at random
    per write (a deterministic xorshift per cable), still in order. Any time; for new bytes.
  - `link_cut` = list (`"5"`, `"3,5"`, `""` none): those cabinets are powered off: not run, their
    cables closed, so the boards either side lose the link. Taking a cabinet off the list
    reconnects it with both its cables emptied (its board as it was; `daytona_cabinet_reset`
    boots it). `link_pause` = list: not run, cables left open (nobody reads or writes them).
    `link_ghost` = list: the game is not run but its comm board is, once a frame (vblank), so the
    cabinet keeps its place in the ring and passes on what it receives with its own block as
    the game last wrote it.
  - `link_full` = `refuse` (default) or `drop`: what a cable holding 32 link frames ready and
    unread does with another write: refuse it (the writing board loses the link, as a TCP send
    limit would) or drop its oldest frames.
  - `link_assist` = 1: a slave whose board waits for its number while the master's link is up
    is handed the master's numbering frame (0xfe, its own id, the count), once a frame: what a
    network layer that knows the seats would do for a cabinet booting into a ring already
    numbered.
  - `link_pace` = 1: a board is handed at most one data frame a frame; the others wait. Unset
    (or `""`) it is on in a star and off in the ring (as before). `0` in the arcade mode's star:
    a frame only in a `retro_run` after a block was handed over.
  - `link_topology` = `ring` (default) or `star`, read at load: in the star there are no cables;
    each cabinet's data frames are taken apart and only its own 0x1c0-byte block goes out (to
    all, `link_delay` later); once a frame each cabinet is handed the data frame it would have
    received from the cabinet before it, made of everyone's latest block; the numbering is
    answered by the transport (the master's 0xff comes back with the count, its 0xfe reaches
    each slave with that slave's id); vsync frames and forwards go nowhere. Whole-machine states
    leave the star's blocks out. With `cabinets=1` it is the arcade mode's bridge instead (below).
  - Exports: `daytona_link_stats()` (JSON: per cabinet bytes and frames written by kind, bytes
    read, the shift-register check, how much its own block changes, bytes queued in its outgoing
    cable, link_assist frames, dropped frames; the frame counter, delay, jitter, topology),
    `daytona_link_frame(k, sent)` (the last data frame, 0xe01 bytes, cabinet k received or
    sent), `daytona_cabinet_save(k, &size)` (cabinet k alone: `"DAYC"`, version 1, `[u32 size]
    [shim state][u32 size][rt::save_state]`, its EEPROM and backup RAM inside, no cables; ~8.7 MB,
    ~1 MB deflated; valid until the next call), `daytona_cabinet_load(k, data, size)` (into any
    machine of this build with a link: the cabinet is untouched when it fails, the rest of the
    machine and the cables stay as they are), `daytona_cabinet_reset(k)` (power-on of cabinet k
    alone, its settings from `<nvram_dir>/<k>/` or the presets).
- **Save states** (`retro_serialize*`): `"DAYT"` magic, version 1, cabinet count; then per
  cabinet `[u32 size][shim state: wheel, gear, last pad, 48 kHz position, the FM/PCM carry]`
  `[u32 size][rt::save_state of that cabinet's GameLoop]`; then (two or more cabinets) each
  cable's `[u32 size][bytes in flight]` (with `link_delay`, all ready at once after a load; a
  cable holding more than 32 link frames is not saved: the save fails, logged); zero padding to
  `retro_serialize_size()`, which is fixed per
  load (header + bounds: `rt::state_size_bound` per cabinet, the shim's, 32 link frames a
  cable). The arcade mode's machine (`cabinets=1`, `link_topology=star`) adds its table of
  blocks after the cabinet: `"DAYS"`, version 1, the seat, flags, its own block as last sent, then
  per seat 0-7 `[u32 seen/absent][448-byte block]` (4,080 bytes; a state without it, from another
  machine, loads with every other seat empty). `retro_unserialize` checks magic, version and cabinet count, and loads onto a freshly
  reset core (the worker resets first) as onto a running one. Until `patches/0001-snapshot.patch`
  is in, saving fails cleanly with a logged reason (`shim/snapshot_stub.cpp`).
- **Main RAM** (`RETRO_MEMORY_SYSTEM_RAM`): cabinet 0's, from `rt::main_ram` (the snapshot
  patch). The stand-in copies the i960 work RAM (0x500000, 1 MB) out through the bus on every
  call, so the worker's desync hash works already.
- **Reset**: power-on of every cabinet: fresh boards from the imported images (kept in memory
  for this: a copy is 4 ms a cabinet, where importing again from the zip would take about a
  second, its ~46 MB of ROMs inflating at ~68 MB/s in WebAssembly), settings presets, empty
  cables.
- **Settings (NVRAM)**: at power-on cabinet k of N takes `ioboard_eeprom.bin` and
  `backup_ram.bin` from `<nvram_dir>/<k>/`, else the presets built in from `daytona/nvram/<N>/<k>/`,
  else the factory's. The presets (`nvram/README.md`, made by `make-nvram.mjs` from the game's
  test menu): one cabinet LINK ID SINGLE; two cabinets MASTER car 1 and SLAVE car 2; free play
  in all. So a fresh core boots straight into the attract mode, linked with two cabinets. Without
  them a single cabinet waits for a second one and two cabinets are both masters. More than two
  cabinets need presets through `nvram_dir` (`make-nvram.mjs make --only 8/ --out DIR` makes the
  eight of a ring: master car 1, slaves cars 2-8; they are not built in).
- **Errors**: the runtime's exceptions (a bad ROM set, "no recompiled code at ...", a state that
  does not load) are caught at the libretro boundary and logged through
  `GET_LOG_INTERFACE`; a machine that stopped runs nothing until reset or a good state.
- **Timings**: `daytona_timings()` returns, summed over the frames since the last call, in
  microseconds: `{"frames","cabinets","logic":[..],"geometry":[..],"raster":[..],"sound":[..],"audio","total"}`
  (logic = the game's code and the board, per cabinet; raster 0 for a cabinet not drawn; audio =
  the shim's resampling). `daytona_link_status()`:
  `{"cabinets":2,"link":[{"state":"up","id":1,"count":2},{"state":"up","id":2,"count":2}]}`
  (state none/off/waiting/up/lost; `"mode":"cut"|"pause"|"ghost"` on a cabinet so set; the
  arcade mode adds `"star"`, below).

## The arcade mode (one cabinet a machine, on a star)

Up to 8 seats at the Daytona cabinet, each player's browser running only their own seat's
cabinet; findings and measurements in [ring-notes.md](ring-notes.md) (the last section, "The
bridge", is this). Nothing ever boots over the network: a player sitting at seat k loads seat k's
preset state, a cabinet whose link is already up in an 8-ring (link id k + 1 of 8, car k + 1),
captured in the linked attract mode.

- **Options**, before loading: `cabinets=1`, `link_topology=star`, `seat=k` (0-7: which seat this
  cabinet is, its block's slot; it must be the loaded state's, link id k + 1, else a warning is
  logged at the load and when `seat` is set). `link_pace` is on (one frame a frame).
  `link_absent` = `zero` (default) or `freeze`: what a seat that left reads as. The cabinet reads
  RetroPad port 0 whatever its seat. The machine is the shim's class `Bridge`.
- **Exports** (all in `exports.json`):
  - `int daytona_link_block_size(void)`: 448.
  - `int daytona_link_out(uint8_t *dst)`: copies this cabinet's own block, as its board last sent
    it (the one the others need now), to `dst`; returns 1 when it differs from what the previous
    call returned (always 1 on the first call after power-on or a state load), else 0. Call it
    right after `retro_run`.
  - `void daytona_link_in(int seat, const uint8_t *src, int len)`: `seat`'s latest block, for the
    next `retro_run` on (latest wins). `len` must be 448 and `seat` another seat 0-7: anything
    else is rejected with a log (the first 5, then every 1000th).
  - `void daytona_link_absent(int seat)`: that seat left. Its block reads as **zeros** (the
    default, `link_absent=zero`: "no cabinet", its car leaves the others' race) or stays **frozen**
    as last seen (`link_absent=freeze`: its car stays where it stopped). A seat never seen reads
    as zeros either way; the next `daytona_link_in` makes it present again. Zero is the default
    because a frozen racer is an obstacle: in the measured race the car behind it crashed into
    it at full speed one second later; with zeros it drove on (ring-notes.md, "The bridge").
  - `daytona_link_status()` adds the star as this seat sees it:
    `"star":{"seat":0,"count":8,"absent":"zero","seats":[{"state":"self"},{"state":"present","age":1},{"state":"absent","frozen":false,"age":300},{"state":"empty"},...]}`
    (`age`: `retro_run`s since the seat's block was handed over, or since it left).
- **What the board sees**: once a local frame the board is handed the data frame it would have
  received from the seat before it in the ring (link id own + 1), assembled from the latest
  blocks of all seats (empty or left seats zeros, or frozen; its own block last, as it last
  sent it); the numbering is answered if the board ever asks (a master's 0xff comes back with
  the count, a waiting slave is handed its 0xfe, once a frame); vsync frames go nowhere; a write
  is never refused. So it depends only on the blocks handed over before the `retro_run` (and the
  cabinet itself): the table of blocks is machine state (in whole-machine states), and a
  spectator replaying a cabinet's pad input and hand-overs from the same state sees the same
  game (`arcade-check.mjs` (d)).
- **Each frame, the worker** (a player's machine; a spectator's takes the two hand-over lines
  from its log instead of the relay, exactly as logged, and sends nothing):

  ```js
  // load: options cabinets=1, link_topology=star, seat=k; core.reset(); core.unserialize(unpack(seat k's state))
  // (a spectator: the player's state, which carries the table), then hand over every present seat's latest block.
  for (const [seat, block] of blocksArrivedSinceLastFrame) core.linkIn(seat, block); // in arrival order; latest wins
  for (const seat of seatsThatLeft) core.linkAbsent(seat);                           // once, when the room says so
  log(frame, pad, those calls);                                                     // for spectators: they replay exactly these
  core.inputs[0] = pad; core.run();                                                 // never waits for anyone
  const block = core.linkOut(); if (block) relay.send(seat, frame, block);          // only when it changed (~every frame)
  ```

  A seat whose blocks stop coming (a frozen tab) needs nothing: its last block stays. A seat
  that comes back (a new player) just sends blocks again. Traffic: 448 bytes raw a frame a
  seat (25.8 KB/s), ~150-240 bytes deflated, ~50-90 as a deflated delta (ring-notes.md M6).
- **Seat states**: `ROMS=<dir> node daytona/make-states.mjs [--nvram=DIR]` forms an 8-cabinet ring
  (the presets `8/0`-`8/7` from `--nvram`, or made by `make-nvram.mjs`), runs it to frame 5000
  (the linked attract: LINK SYSTEM / UP TO 8 RACERS WANTED, all 8 up), saves each cabinet alone,
  loads cabinet k into a fresh `cabinets=1`, `link_topology=star`, `seat=k` machine, and writes
  that machine's whole state packed as worker.js's `pack()` does (`"vabz"` + deflate-raw) to
  `dist/states/daytona.seat<k>.state` (ROM-derived: `dist/` is git-ignored); then checks each in a
  fresh core (reset, unpack, unserialize: up, id k + 1 of 8; still up after 120 frames alone)
  and writes PNGs (`dist/states/shots/`). 1.02-1.07 MB each (12,222,464 bytes unpacked), 100 s
  for all. `make upload-daytona-states` puts them in R2 at `roms/daytona.seat<k>.state`
  (`application/octet-stream`; `R2_TARGET` / `R2_BUCKET` as `upload-rom`).
- **Known limit (the game's)**: a seat that leaves **between START and the race start** (circuit
  or mission select) leaves the other entrants of its session waiting for it for good (measured
  3.5 min), whether its block is zeroed, frozen or set back to an idle one; leaving after GO! or
  while idle costs the others nothing, and a race goes on to its end and back to the attract
  mode. A frontend has to handle that case itself (ring-notes.md, "The bridge").
- **Cost**: as one cabinet: 8.25 ms a frame in Node (`bench.mjs`, below), wasm memory 256 MB.

## Checks, bench, harness

```sh
node daytona/check.mjs                 # smoke test now; the rest once the ROM is here
node daytona/bench.mjs                 # FRAMES, WARMUP, CABINETS, VIEW, DRAW_HIDDEN, PLAY=1
daytona/.cache/native/bench ~/Downloads/daytona.zip 600 600 2   # frames, warm-up, cabinets
node daytona/harness/serve.mjs --play daytona   # http://localhost:8790/daytona/harness/play.html?rom=daytona
node daytona/harness/run.mjs "?rom=daytona&play&cabinets=2"     # the bench page, own headless Chrome
node daytona/harness/ring-lab.mjs form --nvram=DIR --out=DIR    # an 8-cabinet ring (ring-notes.md)
node daytona/make-states.mjs [--nvram=DIR]                       # the arcade mode's seat states
node daytona/harness/arcade-check.mjs [--delay=6 --jitter=3]     # seats in separate processes (below)
CABINETS=1 OPTIONS=link_topology=star,seat=0 STATE=daytona/dist/states/daytona.seat0.state node daytona/bench.mjs
```

`arcade-check.mjs` (needs the seat states; `--only=contract,pair,replay,trio`, `--out=DIR` for
the PNGs, `--absent=freeze` to try the other policy; verified 2026-10-07: all PASS in 40 s, and
the trio with `--absent=freeze` too; the relay's jitter is seeded per pair, so a run is
reproducible, the same hashes every time):
`contract` (one seat in-process: block size, `daytona_link_out`'s changed flag, the rejections,
the status JSON, the data frame the board is handed made of the blocks handed over before the
frame, the table in a whole-machine state, zero and freeze, the seat warning, libretro.js's
`Core.linkBlockSize`); `pair`: seats 0 and 1 in two Node processes from their seat states,
blocks through a relay in the parent (IPC) 6 frames late plus 0-3 of jitter in order, each
running as fast as it can (2.5 ms a frame, nothing drawn but checkpoints) but none more than 3
frames ahead of another: seat 0 START, seat 1 START 2 s later, both race linked (POSITION 1ST/2
and 2ND/2 on the PNGs); `replay`: seat 0's recorded log (pad and hand-overs per frame) replayed in
a fresh process from its seat state and from its frame-1200 state: RAM and screen hashes the same
at every 120 frames (25/25 and 15/15); `trio`: three processes, seat 2's START after the race
began opens its own session (it races alone, /40), seat 1's process ends mid-race and seat 0's
race goes on. It reads two game variables found for it (main RAM 0x501080: cars on the track,
10 / 16 linked pair / 40 alone; 0x540027: entrants in the session).

`check.mjs` with the stub build (no ROM set; verified on 2026-10-06): system and AV info
(496x384, 4:3, 57.524160 Hz, 48000 Hz), loading fails cleanly with a logged error for a missing
file, a non-zip and a zip of the wrong files, every entry point is harmless with nothing loaded,
options and status JSON; then a two-cabinet machine on blank images: it powers on, cabinet 2
takes its settings from `/nvram/1/`, both comm boards are there, the 1 MB main RAM is exposed,
saving fails with the reason, the first frame stops at "no recompiled code at 00000000" (caught,
logged, the module lives on), reset powers on again, `daytona_save_nvram` writes the settings
back. With the ROM set and the real core it runs instead: determinism (two processes, 1200
frames of scripted pad input, RAM, screen and sound hashes every 120 frames), save states (600
frames, a fresh process resets, loads and runs 600 more: the same hashes), drawing (the hidden
cabinet drawn too: same RAM and screens; cabinet 1 hidden for frames 600-959: its first frame
shown is the straight run's), link (both cabinets up, 1 of 2 and 2 of 2, with the twin presets).
On 2026-10-06 all of them passed but save states, which wait for the snapshot patch (Status).

`harness/play.html` (checked in Chrome: 58 fps, the game running): cabinet 1 on the bar's keys (arrows, Z X shift, A S D C views, 1 start,
5 coin), cabinet 2 on I J K L, U O, 7-0, 2, 6; V shows the other cabinet; paced at 57.524 Hz,
48 kHz sound through `web/emulator/audio.js`. `harness/index.html` (`run.mjs`) times frames in
the page (`&blank` runs the blank-image machine: verified in headless Chrome with the stub web
build, which loads, stops cleanly at the first frame and reports).

### Online lab (`harness/online-lab.mjs`)

The bar itself, end to end: one to three headless Chromes of its own (never yours) open the
site, walk to the Daytona cabinet (the client's test hook `vab.goTo("daytona")`), sit with E (a
watcher presses F), start a race (Start five times, then the accelerator held, steering now and
then) and measure, over CDP, with probes in the page and the emulator worker: frames emulated,
posted and received by the page, the core's frame time, GGRS ping, input delay, waits, the
handover and a watcher's start (state size on the wire, each step's time), checkpoints sent,
received and mismatched, every `view` set, link status, memory. It prints a report and writes
`<label>.json` (with each worker's warnings and input log), page screenshots and the last frame
each worker posted (496x384 PNG).

```sh
make upload-daytona R2_TARGET=--local && make upload-rom ROM=$HOME/Downloads/daytona.zip
make dev BUCKET=local          # another terminal; any PROFILE but wasm-release (test hooks)
node daytona/harness/online-lab.mjs --mode=solo|online [--join=attract|race] [--watch] \
  [--delay=60 --jitter=20] [--poke=600] [--rttA=300] [--seconds=60 --settle=20] \
  [--replay=<input log .json>] [--joinAt=N --bStartAt=N] [--label=x] [--site=URL] [--room=x] \
  [--out=dir] [--scratch=dir]
```

`--join`: player 2 sits while player 1's machine is in the attract mode, or once their race is on
(the handover carries a race). `--watch`: a third browser watches; Right then Left switch its
screen. `--delay`/`--jitter`: ms added to every WebRTC send on both machines (in order).
`--poke=N`: browser B flips a byte of cabinet 1's work RAM N frames into the race. `--rttA`:
browser A's worker is told that round trip. `--replay`: browser A's inputs from a recorded log,
frame-exact from power-on (solo or online), e.g. a start timing that hits patches/0004.

Measured on 2026-10-06 (M1 Max, `make dev BUCKET=local`, Chrome 154 headless, 60 s after the
race settled; both players drove; the core before patches/0004, which changes nothing but an
overflowing frame: `check.mjs`'s hashes are the same with it). Every run, every browser: 57.5 frames emulated, posted and
received by the page a second (57-58 each second), `retro_run` p50 11.8-13.6 ms and p95
13.7-15.5 ms, lockstep (rollback 0); seat 1 sees cabinet 1 (red car 1, POSITION 1ST/2) and seat 2
cabinet 2 (blue car 2, 2ND/2), both links up (1 of 2, 2 of 2). Wasm heap 369 MB alone, 443 MB
after a handover (531 MB on the players with a watcher); each browser ~2.7-3.0 GB resident.

| run | GGRS ping | input delay | waits in the 60 s | checkpoints, mismatches | handover (deflated; capture to both running) |
|---|---|---|---|---|---|
| b1: LAN, player 2 joins in the attract mode | 15-52 ms | 3 (4 for a while) | 0 | 53/53, 0 | 1,642,070 bytes (7 pieces); 900 ms |
| b2: LAN, player 2 joins mid-race | 12-35 ms | 3 | 0 (4, up to 156 ms, in the second after the join) | 53/53, 0 | 1,933,769 bytes (8 pieces); 1,145 ms |
| c1: 30±20 ms each way (page round trip 83 ms) | 52-139 ms | A 6, 5, then 4; B 5 | A 0, B 2 (up to 6 ms) | 53/53, 0 | 1,642,087 bytes; 1,309 ms |
| c2: 60±20 ms each way (page round trip 129 ms) | 105-192 ms | 7, then 6 | 0 | 53/53, 0 | 1,642,071 bytes; 1,971 ms |
| d: LAN and a watcher | 15-38 ms | 3 | 0 | 60/60, 0 | watcher: 2,010,779 bytes (8 pieces, 49 ms through the room); F to its first frame 1,843 ms; it plays 57.5 fps, starts on player 1's screen, Right shows player 2's, Left back |
| e: LAN, `--poke=600` | 14-52 ms | 3 | 0 | 55/55, 1: the poke (flipped after frame 2373, both report frame 2400) | resync: 1,972,772 bytes; 438 ms; no mismatch after |

The two-cabinet state: `retro_serialize` 9-13 ms in Chrome, 24,666,112 bytes (~17.6 MB of
content), deflated by the worker in ~135-155 ms (serialize included) to 1.6 MB in the attract
mode, 1.8-2.0 MB in a race; inflate, reset and load on the joiner 30-50 ms. With patches/0004
(2026-10-06): two solo runs replaying start timings that stop the unpatched machine
(`--replay=daytona/harness/overflow-start.json`, a solo run's input log that overflowed at frame
2001, and the same 16 frames later: 2017; logged once each) and one on keys: 57.5 fps, no halt;
online (`--join=race --replay=daytona/harness/overflow-start.json --joinAt=1300 --bStartAt=2300`)
the overflow came in the session, on both machines at the same frame: logged on both, 57/57
checkpoints, 0 mismatches, 57.5 fps.

## Measured (2026-10-06, M1 Max, MAME's `daytona` set)

The recompile: `m2recomp` 38,001 i960 instructions from 704 seeds, 26 functions of 1,500 (127
of 300 with `M2RECOMP_CHUNK=300`), all native; `m2tgprecomp` 823 TGP instructions reachable;
`m2sndrecomp` 1,916 68000 instructions; 9.9 MB of C++. The first build with the ROM set (import,
recompile, runtime and game code for wasm and native) took 4 min 3 s; the game code alone, wasm,
70 s. `daytona.wasm` 4,418,294 bytes (presets embedded), `daytona.mjs` 66 KB; the native bench
5.2 MB. V8 compiles the module with Liftoff in 39 ms (all of it with TurboFan, eagerly: 1.0 s).

Milliseconds a frame, 600 frames after 1500 (attract: no input; race: `PLAY=1` / `&play`, Start
five times then the accelerator, so both players are in the linked race); budget 17.38 ms
(57.524 Hz). Game code = the i960 and TGP programs and the board, per cabinet; the rasterizer runs
for the shown cabinet only; the shim's resampling and mixing is 0.01 ms; libretro.js's
XRGB-to-RGBA copy of the frame is in the totals.

| where | cabinets, scene | frame avg / p95 / worst | game code | geometrizer | rasterizer | sound board |
|---|---|---|---|---|---|---|
| native (clang -O3, `.cache/native/bench`) | 2, linked attract | 7.88 / 9.08 / 10.83 | 0.70, 0.72 | 0.24, 0.24 | 5.46 | 0.26, 0.26 |
| native | 1, attract | 6.61 / 8.04 / 8.21 | 0.63 | 0.24 | 5.47 | 0.27 |
| wasm, Node 24 (`bench.mjs`) | 2, linked attract | 12.10 / 14.04 / 18.29 | 1.68, 1.73 | 0.38, 0.37 | 6.62 | 0.30, 0.29 |
| wasm, Node | 2, linked race | 10.83 / 12.59 / 17.85 | 1.67, 1.73 | 0.29, 0.30 | 5.53 | 0.31, 0.31 |
| wasm, Node | 1, attract | 8.13 / 9.90 / 10.51 | 0.73 | 0.34 | 6.07 | 0.29 |
| wasm, Node | 1, race | 8.17 / 9.93 / 18.48 | 0.94 | 0.29 | 5.87 | 0.32 |
| wasm, Node (2026-10-07) | 1, arcade mode: seat 0's state, star, alone (linked attract) | 8.25 / 8.95 / 9.98 | 1.57 | 0.31 | 5.32 | 0.28 |
| wasm, Chrome 154 headless (`harness/run.mjs`, web build) | 2, linked attract | 13.15 / 14.90 / 16.80 | 2.45, 2.47 | 0.33, 0.32 | 6.30 | 0.24, 0.23 |
| wasm, Chrome | 2, linked race | 12.19 / 14.10 / 17.20 | 2.42, 2.44 | 0.26, 0.28 | 5.46 | 0.27, 0.26 |
| wasm, Chrome | 1, race | 7.81 / 9.40 / 13.70 | 0.93 | 0.26 | 5.58 | 0.25 |

So two linked cabinets fit a 57.5 Hz frame in the browser with ~4 ms to spare; the software
rasterizer is half of it (as natively: it is the recomp's cost too, ~5 ms of its ~5 ms frame).
Also measured:

- Loading the ROM set (CRC check and import): 0.9 s in wasm, 0.78 s native. `retro_reset`
  (power-on of both cabinets from the kept images): 14 ms. Wasm heap: 369 MB with two cabinets
  in the attract mode (443 MB seen after a race in Chrome), 307 MB with one.
- Sound: 834.4 samples a frame at 48 kHz (48000 / 57.524); races reach full scale (32767), as
  `m2run --wav`'s mix does.
- The same machine on two compilers: the native build and the wasm build end with the same main
  RAM hash (FNV-1a over words, as the worker's): 5a2fc89c (two cabinets, presets, 2100 frames),
  b42920c3 (one cabinet), f773060d (two cabinets on factory settings, 1200 frames).
- `M2RECOMP_CHUNK`: with the game, 300 against 1500 in Node: game code 1.75/1.82 against
  1.68/1.73 ms, eager TurboFan 0.92 against 1.02 s; natively 300 halves the game code (0.47
  against 0.70 ms). The browser is what counts, so the default stays m2recomp's 1500. (On a
  synthetic program of the game's size, random branches all over, 300 had won by far in wasm
  too: 0.86 against 2.22 ms and 5.9 against 31 s of TurboFan. The game's code is not like that.)
- `PERF_FLAGS=-msimd128`: no difference beyond run-to-run noise (10.70 and 11.43 ms against
  12.19 and 11.29 in a race, the same RAM hash), so it is not used.
- Without the ROM set (the stub build): a clean build of all three targets 68 s, wasm 350 KB.

## Status

Done and checked with the ROM set (`node daytona/check.mjs`, 2026-10-06): the libretro surface,
the game in both cabinets with sound, determinism across two Node processes (RAM, screen and
sound hashes every 120 frames for 1200 frames), the hidden cabinet's rasterizer off changing
nothing and the first frame shown again being whole, the in-memory link (cabinet 1 of 2 and
2 of 2, the linked attract mode, a linked race with POSITION /2 on both screens, red car 1 and
blue car 2), the settings presets (`nvram/`), the page with keyboard and sound (`play.html`
paced at 58 fps in Chrome).

With `patches/0001-snapshot.patch` (the runtime's save states: `rt::save_state`, `load_state`,
`state_size_bound`, `main_ram`; see `patches/README.md` and `snapshot-notes.md`) the save-state
check passes too (2026-10-06): 600 frames, a fresh process resets, loads and runs 600 more, the
same RAM, screen and sound hashes at every 120 frames. In wasm a two-cabinet state is
`retro_serialize_size()` = 24.7 MB (the bound, zero-padded; ~17.6 MB of content), 20 ms to save
and 9.6 ms to load (`bench.mjs`): fine for a handover or a watcher's start (the worker deflates
it), far too slow for a save every frame, so the game stays `lockstep: true`.

Left:

1. The save is slower than it needs to be: `rt::save_state` builds a vector that the shim then
   copies into libretro's buffer, with a 64-bit hash of the body on the way. Writing straight
   into the caller's buffer would bring the 20 ms down to a few; only worth it if rollback is
   ever wanted (a two-cabinet frame is ~5 ms of game code, so rollback would also need its
   re-run frames under budget).
2. The worker's handover, a watcher and the online lab through the real site (`harness/online-lab.mjs`).
   Done 2026-10-06: see "Online lab" under Checks, bench, harness.
3. The bar's integration is in (`assets/games.ron`: `core: "daytona"`, `options`
   `cabinets`/`view`; the worker sets each player's view from their seat; `make daytona` /
   `make upload-rom`); the cabinet's skin is still to be drawn.
4. Known issue, patched (`patches/0004-polygon-limit-drops-the-frame.patch`): with one player
   alone at the two linked cabinets, cabinet 2's attract mode now and then parses a display
   list the game has not finished (start timings landing on 1 mod 16: about 1 in 20), which
   upstream stops the machine for ("SEGA 3D: Max polygon limit exceeded"; deterministic, so
   online both machines stop alike). With the patch that frame's 3D is dropped, the shim logs it, and the
   game goes on; the same on every machine, `check.mjs` unchanged. The root cause in the
   recomp's vblank / geometrizer timing is not found: worth an upstream report with
   `patches/0004-repro-inputs.txt` (details and repro in `patches/README.md`).
