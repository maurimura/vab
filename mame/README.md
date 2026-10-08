# MAME in the browser

[MAME](https://www.mamedev.org/)'s Namco System 12 and System 23 drivers (Tekken 3, Time Crisis
II, and the other machines in those two driver files) built to WebAssembly behind the libretro
API, so the bar's emulator worker runs them the way it runs the FBNeo cores and Supermodel. It
is the libretro fork of MAME ([libretro/mame](https://github.com/libretro/mame), MAME 0.289): its
libretro OSD steps the machine one frame per `retro_run()` and returns (no threads, no
coroutines), and its save states are MAME's own, written straight into the caller's buffer.
Time Crisis II also links two boards for its two-cabinet co-op, each board in its own browser
(below, "Linked cabinets").

- `build.sh` fetches the pinned fork, applies `patches/`, runs MAME's own GENie build for
  Emscripten (`TARGETOS=asmjs`, `OSD=retro`, `SUBTARGET=vab`, `SOURCES=` the two drivers; makedep
  finds their devices) with the pinned emsdk, then links `dist/mame.mjs` + `dist/mame.wasm`
  itself, exporting `exports.json` (the libretro API, as FBNeo's, and the link API). `EMSDK_DIR`
  overrides where the emsdk is (default `emulator/.cache/emsdk`). `./mame/build.sh link` only
  relinks.
- `bench.mjs` times frames, save states and checks pictures, sound and determinism in Node,
  through `web/emulator/libretro.js`: `node mame/bench.mjs ~/Downloads/tekken3je1.zip`, or
  `GUN=1 node mame/bench.mjs ~/Downloads/timecrs2.zip` (`FRAMES`, `WARMUP`, `SHOT=frame.png`;
  `GUN=1` answers the core's lightgun, aimed at the middle; `COIN=1200` inserts a credit then
  and pulls the trigger now and then, so the timed frames are a game; `DRC=1`/`DRC=0` sets the
  core option `mame_drc`).
- `link-check.mjs` runs two Time Crisis II boards linked back to back (below).
- `core-options.mjs` lets those scripts set core options libretro.js doesn't answer.
- `make mame` builds and puts the core in local R2 (served at /mame/), `make mame-remote` uploads
  it to production.

A full build from scratch took about 6 minutes on an M1 Max (the build tree is ~360 MB); after
a change to a driver, seconds plus the link (~1.5 min).

From the page or worker, exactly as an FBNeo core:

```js
const core = await Core.create(createMAME, callbacks);
core.loadGame("timecrs2.zip", bytes); // 640x480 at 59.904 Hz, 48000 Hz stereo
```

## ROM sets

MAME picks the system from the zip's name and finds files by name or, failing that, by CRC
(in that zip and in its parent's, e.g. a `tekken3.zip` next to a clone's).

- **Time Crisis II**: `timecrs2` is "Time Crisis II (US, TSS3 Ver. B)" (System 23). Besides its
  own ROMs it needs one device ROM, the TSS-I/O gun board's program `tssioprog.ic3` (0x40000
  bytes, CRC edad4538, MAME device `namco_tssio`): inside `timecrs2.zip` or as `namco_tssio.zip`
  next to it. No BIOS. The `timecrs2.zip` we have has it inside (22 files, all CRCs matching), so
  it is all that's needed. Clones: `timecrs2v2b` (World TSS2 Ver.B), `timecrs2v1b` (Japan TSS1
  Ver.B), `timecrs2v4a` / `timecrs2v5a` (Super System 23: no link device).
- **Tekken 3**: MAME 0.289's `tekken3` is "Tekken 3 (World, TET2/VER.E1)" (program ROMs
  `tet2vere1.2e/.2j`). The `tekken3.zip` we have is the Japanese TET1/VER.E1 board with older
  file names (`tet1vere.2e/.2j`, CRCs 8b01113b / df4c96fb): MAME's **`tekken3je1`**. Loaded as
  `tekken3.zip` MAME stops with "tet2vere1.2e NOT FOUND"; loaded as `tekken3je1.zip` (the same
  bytes) it runs. So the file the worker hands the core must be named `tekken3je1.zip`.

## Measured

Measured on 2026-10-07 on an M1 Max shared with other builds (load average in brackets), with
the real `timecrs2.zip` and the `tekken3.zip` above (as `tekken3je1.zip`). "Attract" is frames
1200-1800 after power-on, "game" frames 2400-3000 with a credit inserted at frame 1200 and the
trigger pulled now and then (stage 1), "boot" the average over the first 1200 frames (the power-on
test, the 300-count partner search and the NAMCO splash).

| ms per frame (p50 unless said) | Time Crisis II, interpreter (`mame_drc` off, the default) | Time Crisis II, recompiler (on) | Tekken 3 |
|---|---|---|---|
| Chrome 154 headless, page's main thread: attract | **13.7-14.8** (2.5-5) | 14.4-15.4 (2.7-3.3) | 6.6 (3.5) |
| Chrome: game | **17.3** (3.8) | 18.7 (6.5) | 7.7 (a fight) |
| Chrome: boot (average) | 19.1-20.4 | 22.0-22.2 | 4.3 |
| Node 24 (`bench.mjs`, CPU time): attract | 17.1 (1.5) | 16.8 (1.8) | 10.0 (5) |
| Node: game | 25.4 (5-9) | 23.7 (10) | 10.5 (9.5) |
| Node: boot (average) | 37.0 (1.5) | 25.7 (1.8) | 7.6 |

Tekken 3's "game" is a fight: frames 2400-3000 after a coin at 1200, Start at 1300 and random
play from 1400, measured with `patches/0006` (Node on 2026-10-08; Chrome on 2026-10-07 with a
System 12-only build of the same code: 8.1 ms p95, 7.2 ms without drawing). The frame budget is
16.69 ms (59.904 Hz). In Chrome, the bar's browser, MAME's MIPS III
interpreter is faster than its recompiler (whose only WebAssembly backend is the C one, which
interprets MAME's intermediate code), so the interpreter is the default; Node's older V8 prefers
the recompiler. Both are deterministic (below); both boards of a link must use the same.
Time Crisis II is at the budget in attract mode and ~4% over it in a game on this machine.

Where a game frame goes (V8 profile, interpreter, Node): the 3D renderer ~32%
(`namcos23_renderer::render_scanline` alone 25%), the R4650 ~24% (only ~175,000 instructions a
frame now, see `patches/0005`), the two H8 MCUs (System 23's sound/IO H8/3002 and the TSS-I/O
board's H8/3337) ~15%, MAME's 640x480 compose and text layer ~6%, the C352 ~1%. Without
`patches/0005` the R4650 spent ~99% of its time in the game's wait-for-vblank loop and a frame
took ~60 ms (recompiler) / ~95 ms (interpreter) in Node.

| | Time Crisis II | Tekken 3 |
|---|---|---|
| frame, refresh (after load) | 640x480 at 59.904 Hz | 256x240 at 59.826 Hz (the game switches to 512x480) |
| save state | 33,253,766 bytes (31.7 MiB: the R4650's 16 MB RAM, texture RAM, ...) | 8,332,353 bytes (7.9 MiB) |
| serialize / unserialize (through JS), Chrome | 3.8 / 1.7 ms | 1.1 / 1.0 ms |
| `saveSlot` / `loadSlot` (in wasm), Chrome | 0.9 / 0.9 ms | 0.36 / 0.79 ms |
| system RAM (`retro_get_memory_data`) | 16 MiB, the R4650's main RAM at 0 | 4 MiB, the R3000's main RAM |
| audio | 801 sample frames per frame at 48 kHz | 801 |
| load (inflate + checksum 120 / 43 MB of ROM) | 1.4-1.6 s | 0.6 s |

Sizes: `mame.wasm` 26,611,631 bytes (25.4 MiB; 4.74 MB gzipped), `mame.mjs` 104,589 bytes
(28 KB gzipped). The wasm is larger than the 25 MiB a static asset may be, so it is served from
R2 like the other cores.

Checked with the real sets:

- Time Crisis II boots, runs its attract mode with sound and plays (a credit, a solo game,
  stage 1); with the link, the two-cabinet game (below).
- Determinism (`bench.mjs`, interpreter and recompiler): the same instance replaying 120 frames
  from a state, and a fresh instance (a cold recompiler cache) loading it, end with the same RAM
  and the same whole state; rechecked with `patches/0006` in a game (`GUN=1 COIN=1`), both
  ways. `link-check.mjs` checks the same for linked boards over 300 frames.
- Link off is exactly the game as before the link patch: against a core built without
  `patches/0004`, 3000 frames from power-on with a credit, the trigger, the pedal and a moving
  aim had the same RAM, picture and sound at every 30-frame checkpoint.
- Tekken 3 (`tekken3je1`) boots and plays in the System 12 + 23 build (8.3 MB state, frames
  above), its first power-on screen reads "Thu. 1 Jan. 1998", and its states are complete
  (`patches/0006`, below): `bench.mjs`'s replay and fresh instance end with the same RAM and
  whole state, `emulator/rollback-check.mjs` stays in sync from power-on over 3000 frames, and
  across instances (2026-10-08, 3900 frames of coin, Start and random play): a fresh machine that
  loads another's state from frame 600 or 2400 twice in a row matches it at every 60-frame
  checkpoint, and two machines booted apart, one with junk left in its heap, stay identical,
  whole states included.

## Inputs

The fork reads every port 0-7, every frame, whatever `retro_set_controller_port_device` said: the
RetroPad one button id at a time, the analog sticks and L2/R2 (unused here; 0 is fine), and with
`mame_lightgun_mode` = `lightgun` (our default) `RETRO_DEVICE_LIGHTGUN` too.

**Tekken 3**: MAME's P1/P2 fields by RetroPad port 0/1, by the fork's per-game table
(`mame_buttons_profiles`, on by default):

| RetroPad id | MAME field | Tekken 3 | the bar's key |
|---|---|---|---|
| 1 Y | Button 1 | Left Punch | A |
| 9 X | Button 2 | Right Punch | S |
| 0 B | Button 3 | Left Kick | Z |
| 8 A | Button 4 | Right Kick | X |
| 4-7 | Up Down Left Right (8-way) | | arrows |
| 3 Start | Start 1 / Start 2 (port 1) | | 1 |
| 2 Select | Coin 1 / Coin 2 (port 1) | | 5 |

With the lightgun answered, MAME's buttons also listen to the gun's: keep the gun silent
(`Core.gun = false`) for Tekken 3.

**Time Crisis II**: `INPUT_PORTS_START(timecrs2)` includes System 23's `s23` ports and changes:

| port, mask | MAME type, name | default |
|---|---|---|
| JVS_PLAYER1 0x00000001 | IPT_BUTTON1 "Gun Trigger" | |
| JVS_PLAYER1 0x00008000 | IPT_BUTTON2 "Foot Pedal" | |
| JVS_PLAYER1 0x00000002 | IPT_BUTTON3 "User Enter" (the service menu's) | |
| JVS_PLAYER1 0x00000010 / 0x00000020 | IPT_JOYSTICK_UP "User Service Up" / IPT_JOYSTICK_DOWN "User Service Down" | |
| JVS_PLAYER1 0x00004000 | PORT_CONFNAME "Link ID": 0x0000 "Left/Red", 0x4000 "Right/Blue" | Left/Red |
| JVS_PLAYER1 0x00e03f00 | IPT_UNUSED (Buttons 5-13) | |
| JVS_PLAYER1 0x00000040 (from s23) | IPT_SERVICE1 | |
| JVS_SCREEN_POSITION_INPUT_X1 0xfff | IPT_LIGHTGUN_X, 91..824 (default 457), crosshair, sensitivity 50, key delta 20 | |
| JVS_SCREEN_POSITION_INPUT_Y1 0xfff | IPT_LIGHTGUN_Y, 38..285 (default 161), crosshair, sensitivity 50, key delta 10 | |
| JVS_COIN1 0x01 (s23) | IPT_COIN1 | |
| JVS_SYSTEM 0x80 (s23) | service switch | |
| DSW 0x08 (DIP:5) | "Link Play Enabled" (`patches/0004`; "Unknown" before): 0x08 Off, 0x00 On | Off |
| DSW 0x01/0x02/0x04 (s23) | "Service Mode DIP" (DIP:8), "Skip POST" (DIP:7), "Freeze?" (DIP:6); DIP:4-1 Unknown | Off |
| P1 (s23) | the board's "Dev Service" buttons P1-A..H, Up, Down, P1-Sel, "Dev Service Start" (IPT_START2), all player 2 | |

The input descriptors the core sends (`SET_INPUT_DESCRIPTORS`), exactly:

| port | device | id | name |
|---|---|---|---|
| 0 | RetroPad | 4, 5 | "Up", "Down" (the User Service ones) |
| 0 | RetroPad | 0 (B) | "Gun Trigger" |
| 0 | RetroPad | 8 (A) | "Foot Pedal" |
| 0 | RetroPad | 1 (Y) | "User Enter" |
| 0 | RetroPad | 2 (Select) | "Coin" |
| 0 | lightgun | 13, 14 | "Aim X", "Aim Y" |
| 0 | lightgun | 2 (TRIGGER), 3 (AUX_A), 7 (SELECT) | "Gun Trigger", "Foot Pedal", "Coin" |
| 1 | RetroPad | 4, 5, 0, 8, 1, 9, 10, 11, 3 | "Up", "Down", "Dev Service P1-A", "Dev Service P1-Sel", "Dev Service P1-B", "Dev Service P1-C", "Dev Service P1-D", "Dev Service P1-E", "Start" (MAME's "Dev Service Start") |
| 1 | lightgun | 2, 3, 6 | "Dev Service P1-A", "Dev Service P1-Sel", "Start" |

So the player is port 0: the gun's position (LIGHTGUN SCREEN_X / SCREEN_Y, -0x8000..0x7fff across
the picture), its trigger (or RetroPad B), the pedal (AUX_A or RetroPad A), a coin (SELECT or
RetroPad Select; 4 coins a credit at the default settings, then the trigger starts). There is no
Start for player 1, and nothing should press port 1 (the board's development buttons). The
Link ID and DIP:5 are set by `mame_link_set`, not by input.

## Core options

Unanswered `GET_VARIABLE`s take the defaults of the fork's option table (`patches/0002`), which
are the bar's, so the frontend needs to set none of them:

| key | default | was |
|---|---|---|
| `mame_drc` | disabled (MAME's `-nodrc`: the MIPS III interpreter, faster in Chrome) | (new) |
| `mame_thread_mode` | disabled (and Emscripten forces 1 processor, no work-queue threads) | enabled |
| `mame_lightgun_mode` | lightgun | none |
| `mame_mouse_enable` | disabled | enabled |
| `mame_buttons_profiles` | enabled (Tekken 3's layout above) | disabled |
| `mame_throttle`, `mame_boot_to_osd`, `mame_boot_to_bios`, `mame_read_config`, `mame_write_config`, `mame_auto_save`, `mame_cheats_enable`, `mame_alternate_renderer` | disabled | (same) |
| `mame_softlists_enable` / `mame_media_type` | enabled / rom | (same) |

Changing options needs `GET_VARIABLE_UPDATE` or a reload.

## Video, sound, states

- Pixels: the core asks for format 100 (the bar's RGBA8888) first and draws opaque R G B A bytes;
  XRGB8888 if refused. The frame is MAME's own size for the game (it can change with the game's
  video mode: the frontend takes each frame's).
- `retro_get_system_av_info` after load reports the refresh after the first frame (patches/0002
  runs it inside `retro_load_game`). 48000 Hz audio, about one frame's worth per `retro_run`.
- `GET_AUDIO_VIDEO_ENABLE` without the video bit: MAME skips drawing the frame and sends none
  (the machine, including System 23's 3D renderer, runs the same).
- `retro_reset` is a soft reset at the start of the next `retro_run`; a state loaded before then
  replaces it.
- No MAME UI: no info or warning screens, no RetroPad combination opens the menu or quits. The
  light-gun crosshair is MAME's own, drawn into the frame, shown while the gun moves.

## Determinism

`patches/0003` and parts of `0002` make two machines identical and make a machine that loads a
state go on exactly like the one that saved it: a fixed RTC base time (1998-01-01 UTC, the same
in every time zone; System 23 has an RTC4543 too), the RTC's clock saved, PSX root counters and
SIO keeping their saved timers, a fixed sound-update rate, one frame per `retro_run` from the
first, the boot frame run inside `retro_load_game`, reset at a frame boundary. System 23 reads
nothing else from the host (its NVRAM starts zeroed: the in-memory file system has none), its
JVS devices use only zero-delay synchronize timers, and the R4650's recompiler checks its code
blocks against RAM, so a cold cache after a state load runs the same (measured above).

`patches/0006` makes the states complete (found with Tekken 3, whose replay from a state went
its own way): a machine that loads a state, once or several times in a row, goes on exactly as
the one that saved it, and its state right after the load is the saver's, byte for byte.

- A CPU's pending input-line events (the queue `set_input_line` fills and a zero-delay
  synchronize timer empties) are saved, and that timer armed again after a load, which drops
  temporary timers. Before, two loads with no frame between them (`bench.mjs`'s replay does that)
  left an event no timer would apply, and every later change of that line queued behind it:
  Tekken 3's R3000 diverged within a frame. This is MAME's core: every CPU's state grew by
  8,844 bytes (67 lines), Time Crisis II's three too.
- The PSX interrupt controller no longer sets the CPU's line again after a load (the line is in
  the state); that queued the event above on every load. The R3000's COM_DELAY register is saved.
- PSX CPU, GTE, DMA, MDEC and GPU registers no reset sets, and the H8 SCI's clock step, start at
  zero, not whatever the heap held where the device was built: two Node runs whose only
  difference was the script's name (the environment's size, so the heap's layout) used to drift
  apart by frame ~1200.
- The MIPS III interpreter's FPU condition flags are saved (the recompiler keeps them in FCR31,
  which was): Time Crisis II in a game wrote a different condition bit into FCR31 within 120
  frames of a load into a fresh instance.
- Inactive timers are saved with index 0 rather than their number from the last save they were
  active in, and Lua's idle timer stays off after a load: both only made a loader's state bytes
  differ from the saver's.

Tekken 3's clock: System 12's BIOS stops the H8's clocked serial receive in the middle of a byte
after each RTC4543 read, which left the clock line low, so the next read began with a rising edge
and every other read came one bit late: the first power-on screen said "2 Feb. 2030". The SCI's
clock now idles high once it stops (`patches/0006`), and the screen says "FIRST POWER ON Thu. 1
Jan. 1998 0:00:04".

Floating point: System 23's geometry (the CPU's FPU, the driver's matrices and its polygon
renderer) is single precision. WebAssembly float arithmetic is IEEE and the same everywhere,
except the bits of a NaN it produces, which may differ between an x86 and an ARM machine (wasm
leaves NaN payloads nondeterministic). If the game ever computes a NaN into its RAM, two players
on different CPUs could drift; not seen. Within one machine everything repeats exactly.

## Linked cabinets (Time Crisis II co-op)

Time Crisis II's co-op is two linked boards, each a full cabinet, joined by an RS-422 line through
the C422 serial controller (C139-compatible; 16 KB shared RAM at 0x06200000, registers at
0x06400000, IRQ 5). `patches/0004` ports pocketjazzy's emulation of it (MAME pull request
[#15777](https://github.com/mamedev/mame/pull/15777), head 313ead5c16; fork
[MAME-TC2-Public](https://github.com/pocketjazzy/MAME-TC2-Public), which links two MAME instances
over TCP) with the line replaced by an in-memory transport the frontend drives: each browser runs
one board, and the boards swap what they transmitted once per frame over a reliable, ordered
channel.

The PR targets `timecrs2`, US TSS3 Ver.B (its README: "Only the US TSS3 Ver. B set is validated").
Besides the C139 model it changes four instructions of the game's code in RAM, each only after
reading back the expected original (verify-before-poke; on another revision the poke is refused
and the code left alone), and two data words, all only while linked:

| address | original | becomes | what for (the PR's words) |
|---|---|---|---|
| 0x8000BC78 | `slti v0,v0,0x100` (0x28420100) | `slti v0,v0,0x401` | TX pump burst quantum: frames up to 0x400 halfwords go as one burst |
| 0x800B2504 | `sw a3,0x390(v0)` (0xAC470390) | `nop` | op6F play-clock adoption store |
| 0x800B2508 | `sw a2,0x394(v0)` (0xAC460394) | `nop` | op6F segment-clock adoption store |
| 0x800B71A0 | `sltiu v0,v0,0x11` (0x2C420011) | `sltiu v0,v0,0x1F` | remote-placeholder reaper patience, 17 -> 31 ticks |
| 0x802F3FD8 (data) | link keepalive word | raised to 2 when below 2 while 0x802F3FD0 == 2 | keepalive floor |
| 0x802D2030 (data) | 4-slot wave-anchor ring | dead slots' ids blanked to 0xFFFFFFFF | wave-anchor resurrect |

All four were found and poked on both boards with our set (`mame_link_status` word 10 = 0x55).

### API

Exported from the module (`exports.json`), C signatures:

- `void mame_link_set(int enabled, int side)`: plugs the cable in (enabled 1) or out (0) and sets
  this board's side, 0 Left/Red or 1 Right/Blue (the two boards must differ). Linked, the board's
  DIP:5 "Link Play Enabled" is on and its "Link ID" is the side, as the PR's launcher set them;
  unlinked both are as before (off, Left/Red) and the C422 is the plain register stub of before
  (single-player behaviour unchanged, checked above). Call it before `loadGame` (the cable in
  from power-on, as the game's partner search expects), or between two frames; it lasts for the
  instance (not in save states: call it again on a new instance before loading a state).
- `int mame_link_outgoing(uint8_t *dst, int max)`: what this board transmitted since the last
  call, as frames of `[u16 big-endian byte count][bytes]` (the PR's TCP framing). Returns the byte
  count; copies and forgets the bytes only if they fit in `max` (with `dst` NULL or `max` too small
  it just returns the count). Opaque to the frontend: hand them to the other board as they are.
- `void mame_link_incoming(const uint8_t *src, int length)`: the other board's
  `mame_link_outgoing` bytes, delivered into this board's receive area at once (with the receive
  IRQ), between two frames. Malformed sizes end the parse; zero bytes is fine.
- `int mame_link_status(uint32_t *out, int count)`: up to 12 words for a status line: [0] linked,
  [1] side, [2] frames sent, [3] frames received, [4] bytes sent, [5] bytes received, [6] the
  game's link staging mode word 0x802F3FD0 (2 = linked gameplay staged), [7] its keepalive word
  0x802F3FD8 (16 in a linked game), [8] its link state-machine call counter (0x802CEDD8), [9] bytes
  waiting to be taken, [10] the code patches above (bits 0/2/4/6 poked, 1/3/5/7 refused), [11]
  the DIP switches as the game reads them (bit 3 clear = link play on). Returns how many it filled
  (0 with no Time Crisis II loaded).

### The per-frame protocol

Both boards count link ticks k = 0, 1, 2, ..., one `retro_run` per tick (board B may power on S
ticks after A: its first frame is then tick S; S = 0 works). D >= 1 is a delay in ticks, a
constant of the session, the same on both boards. **Before running tick k a board is fed, with
`mame_link_incoming`, exactly the bytes the other board's `mame_link_outgoing` returned right
after it ran tick k - D, once, and nothing else** (nothing while k - D is before the other's
first tick). So in each browser:

```
mame_link_set(1, side); loadGame(...)
for k = 0, 1, 2, ...:
    wait until the other board's message for tick k - D has arrived (if k - D >= its first tick)
    mame_link_incoming(those bytes)
    retro_run()
    send { tick: k, bytes: mame_link_outgoing(...) }   // every tick, empty or not
```

A board's frame is then a function of its own player's inputs and those bytes only, so each
board is deterministic and the two browsers run their frames concurrently: a board may be up to
D ticks ahead of the other and waits only when the other falls further behind. Every tick's
message must be sent, even empty, so the other side knows that tick is over. The PR kept its two
instances within 2 frames of each other (a frame token per vblank, a wall-clock stall); D plays
that part here, and the game itself tolerates the latency: its link timeout fires after 17
frames of drift between its own state counter and the partner's (the PR's analysis).

Tested with D = 2, 4 and 8, both boards from power-on: all link up and play the linked game.
With larger D the game's link exchanges are slower (its state-machine counter at tick 2100: 1145
with D = 2, 444 with D = 8), so pick the smallest D that covers the one-way latency between the
two workers plus a frame: about 40-45 ms worker to worker is D = 4. Traffic each way: ~3 frames
and ~1.4 KB per tick while the boards are linked in attract mode (~85 KB/s), ~1 frame and
330-400 bytes per tick in a game (~20-24 KB/s).

Save states: a board's state holds everything of the link but the transport: the bytes it
transmitted and not yet taken are dropped by a state load (take them before saving), and the
`mame_link_set` setting is the instance's. A pair of boards restarted from states saved at tick
K needs, for ticks K..K+D-1, the bytes the other board sent at ticks K-D..K-1: keep the last D
messages with the states. `link-check.mjs` does exactly that and gets the same bytes and RAM.

What a healthy link looks like (measured, D = 2): the first link frame at tick ~610 (the end of
the power-on test), the 300-count partner search (ticks ~620-920), then at the NAMCO splash the
boards link up (frames of 0x6c halfwords, then ~3 frames a tick), the GASHIN logo appears on both
on the same tick, and the attract modes run in step. A credit and the trigger give "SELECT GAME
MODE" with "LINK PLAY - BLUE PLAYER IS YOUR TEAMMATE" on the red board and "... RED PLAYER IS
YOUR TEAMMATE" on the blue one; the trigger again: "2P TEAMMATE PLAY!", stage 1 on both, mode word
2 on both, keepalive 16.

One deviation from the PR was needed for the link to come up in lockstep at all: outside the game
(before the debounced "in game" state) a non-zero TXSIZE written while the chip is armed is
transmitted at once, the real chip's trigger as the PR itself describes it, which it applies only
in a game. With the PR's stop-and-wait both boards staged their link-up frame at the NAMCO splash
and polled it forever (the pump writes TX Control before TXOFFSET/TXSIZE, so the START edge always
finds nothing staged, and only a delivered peer frame clears the stage). Otherwise the PR's
semantics are kept: what is read out of the shared RAM and when, chunked messages, the keepalive
replay of the last frame after 33 ms of silence with the current counter stamped in, the
announce latch, the in-game TX-complete release, the vblank service and the code patches, all in
emulated time. The PR's receive path drained its queue at register accesses; here frames are
delivered at the frame boundary.

### Checking it

```sh
node mame/link-check.mjs ~/Downloads/timecrs2.zip                            # power-on, link-up, attract, 2400 ticks
COIN=1200 FRAMES=2100 node mame/link-check.mjs ~/Downloads/timecrs2.zip      # ... and a linked game
COIN=1200 FRAMES=6000 DELAY=4 SHOT=/tmp/tc2 SHOTS=1300,2400 node mame/link-check.mjs ~/Downloads/timecrs2.zip
```

It runs two boards (Left/Red and Right/Blue) in one process exactly as above, prints a status line
per side every `EVERY` ticks (frames and bytes each way, the mode word, keepalive, counter), checks
that every frame one board sent reached the other once, D ticks later, then loads both boards'
states from the middle of the run (`CHECK_AT`) into fresh instances linked to each other and
checks they send the same bytes and keep the same RAM for `CHECK` ticks, and ends with "LINKED
GAMEPLAY on both boards (mode word 2)" when they got that far. `STAGGER`, `DRC`, `SHOT`/`SHOTS`
(PNGs of both boards) as in its header. Both boards run in one process, so a tick costs two
frames.

## Known gaps

- Time Crisis II is at the frame budget in attract mode and a little over it in a game on an M1
  Max in Chrome (above); slower machines will run it slow. The 3D rasterizer is the biggest cost.
- Booting Time Crisis II (power-on test, partner search) takes ~20 ms a frame for ~15 s; a
  start-up state would skip it for solo play, but a link needs both boards to boot linked.
- Tekken 3's refresh changes after load (above); the worker paces at the first one reported.
- `tekken3`'s driver isn't flagged MACHINE_SUPPORTS_SAVE (its states are checked above with
  `tekken3je1` only; other System 12 games and boards, e.g. CD-XA or tektagt's DMA, are not).
- Anonymous MAME timers (`timer_set`) aren't saved; the compiled devices use only zero-delay ones.
- 25 MiB of wasm (Lua, SQLite and the UI's menus come along with MAME's core).
