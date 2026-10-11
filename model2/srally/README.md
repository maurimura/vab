# Sega Rally Championship in the browser

Sega Rally Championship (Sega Model 2A, 1995; MAME's set `srallyc`, run with its Revision B
program EPROMs as `srallycb.zip` has them) behind the libretro API, so the bar's emulator worker
(`web/emulator/libretro.js`, `worker.js`) runs it like the other cores. It is not an emulator: it
is [segarally95-recomp](https://github.com/xandoxan65/segarally95-recomp), the game's i960
program lifted to C by hand and tool, on that project's Model 2 runtime (TGP and geometrizer HLE,
the 68000 + SCSP sound board, System 24 tile layers). The 3D is drawn with WebGL2 by the
renderer's port (`shim/video.c`, another piece of work; see "Video" below).

- `build.sh` fetches the pinned recomp (`a80d71a`, 2026-10-08) into `.cache/src`, applies
  `patches/`, and `core.mk` compiles three outputs: `dist/srally.mjs` + `.wasm` (web and
  worker, WebGL2), `dist/headless/srally.mjs` (Node, no picture) and `.cache/native/bench` (the
  same core natively, the baseline). `build.sh viewer` still builds the recomp's own SDL viewer
  (`.cache/native/segamod2`, its CMake build) with the patches in.
- `shim/libretro.c`: the libretro surface; `coro.c`: the game's coroutine; `arena.c`: the
  recomp's heap in two fixed arenas; `machine.h`, `machine.c`, `state_begin.c`, `state_end.c`:
  what a save state is; `zip.c`: reading the ROM set (zlib); `video_stub.c`: the tile layers on
  the CPU while `video.c`'s patch is not in; `main_native.c`: the native bench.
- `check.mjs` (Node: smoke test, boot, a race, determinism, save states), `bench.mjs` (Node
  timings), `script.mjs` (the inputs they drive), `harness/` (a page to play with keyboard and
  sound, and a bench page run in a headless Chrome of our own), `layout.awk` (the link-map check
  of what a save state holds).

## Licence and what is never committed

segarally95-recomp has **no licence file** at the pinned commit: the user is asking its author.
Until there is one, nothing of it is committed here: `build.sh` fetches it, and `patches/` hold
only our changes. Its lifted C is the game's own program in another language, so `dist/` (the
compiled core) is derived from Sega's code: it lives in `dist/` (git-ignored) and should be
served the way the ROM sets are (R2), never published elsewhere. The ROM set itself is read at
run time from the zip the frontend hands the core; nothing is generated from it at build time.

## Building

```sh
./model2/srally/build.sh [web] [headless] [native] [viewer]   # web headless native when none given
```

- No ROM set is needed to build. Pinned emsdk 6.0.10 from `$EMSDK_DIR` (default
  `emulator/.cache/emsdk`; in a worktree a symlink `emulator/.cache -> <main
  checkout>/emulator/.cache` reuses the main checkout's). Native: clang and zlib.
- Knobs: `JOBS`, `OPT` (default `-O3`), `PERF_FLAGS`, `DEBUG=1` (Emscripten assertions),
  `ASYNCIFY_FLAGS` (e.g. an `ASYNCIFY_REMOVE` list to try). Always `-fno-fast-math
  -ffp-contract=off` for the lifted code (`i960_fp.h`'s rounding).
- A clean build of all three: 20 s on an M1 Max (10 cores); wasm 1.0 MB, `.mjs` 73 KB.

## The contract (what the worker and the page rely on)

```js
import createSrally from "/model2/srally/dist/srally.mjs";   // -sMODULARIZE -sEXPORT_ES6
const core = await Core.create(createSrally, callbacks, { canvas: new OffscreenCanvas(496, 384) });
core.setOption("region", "international");                   // before loading
core.loadGame("srallyc.zip", bytes); // 496x384, 4:3, 57.524 Hz, 44100 Hz stereo
```

- **Exports**: the libretro set and `_malloc`/`_free` (`exports.json`), `_srally_set(key,
  value)`, `_srally_timings()`, and for the checks `_srally_stack_used()`,
  `_srally_heap_top()`, `_srally_heap_peak()`. Memory as daytona/: 256 MB initial, growth to
  2 GB, 4 MB stack, table growth, forced file system; WebGL2 only (`-sMIN_WEBGL_VERSION=2`).
- **Loading**: the zip from `info->data` when given, else from `info->path` (libretro.js writes
  it to `/roms/` and passes the path). The files the recomp reads are inflated into the module's
  file system (`/srally/roms`), checked (the recomp's CRC list), turned into memory images and
  removed again. A set that is not the right one fails the load with the files named in the log.
- **AV**: 496x384, aspect 4:3, **fps = 57.524160** (`16 MHz / (656 x 424)`), **sample rate
  44100**. Pixel format **100** (RGBA bytes, top row first, pitch 496 x 4) when the frontend
  takes it (libretro.js does), else XRGB8888 converted in the core.
- **Sound**: each `retro_run` runs the sound board (68000 at 256 cycles a sample, SCSP) for
  exactly that frame's samples, 44100 x 278144 / 16000000 = 766.6344 a frame (766 or 767, the
  fraction carried in the machine), stereo int16 to `audio_batch_cb`. Silent until the game's
  first sound command, as the recomp's board.
- **Inputs** (RetroPad port 0, ids as libretro.js answers them; the bar's keys in brackets):
  LEFT/RIGHT steer [arrows] (a deterministic ramp toward 0x20/0xe0 from 0x80, daytona/'s:
  `steer_slow` ADC units a frame for the first `steer_slow_frames` of a press away from centre,
  then `steer_step`, also back to centre; defaults 3, 16, 12), UP accelerator, DOWN brake (full
  or released: 0xe0 / 0x00, as the recomp's viewer), B shift down [Z], A shift up [X] (the
  H-shifter's gears 1-4 in sequence, starting in 1; an automatic car ignores it), Y VIEW CHANGE
  [A], START [1], SELECT coin [5]. The factory settings want **two coins a credit**. The menus
  say "select with wheel & confirm with pedal": the wheel's position picks (hold LEFT or RIGHT;
  the slow stage makes the middle choices reachable: the course select reads 0x20 / 0x80 / 0xe0
  as courses 0 / 1 / 3), and a *press* of the accelerator confirms (held from before the screen
  it does not; each screen times out after ~21 s). Descriptors: "Steer left", "Steer right",
  "Accelerate", "Brake", "Shift down", "Shift up", "View change", "Start", "Coin".
- **Options** (`srally_set`, strings): `steer_step`, `steer_slow`, `steer_slow_frames` (any
  time); `region` = `international` (default, the SDL viewer's) | `japan` | `us`, read at load
  (Japan shows the copyright notice at boot); `nvram_dir` (default `/nvram`), read at load: the
  game's settings and backup RAM from `<nvram_dir>/srally.yaml` (the recomp's YAML) when there is
  one, else the factory's; `log` = `status` | `lift` | `none` (the recomp's own stderr lines),
  before loading. Unknown keys are logged.
- **Save states**: a copy of the machine's memory (below, "How it works"): `"SRST"`, version 1,
  a hash of this build's memory layout, then the machine's static data and its heap arena,
  zero-padded to `retro_serialize_size()` = 38,069,049 bytes, fixed. A state loads into any
  instance of the **same build** (the layout hash; the web and headless builds differ), freshly
  reset or running; another build's is refused with a logged reason. Save ~7 ms, load ~3 ms.
- **Main RAM** (`RETRO_MEMORY_SYSTEM_RAM`): the i960's RAM, copied out on each call (1.25 MB):
  the CRX RAM (0x200000, 256 KB: the game's variables; its main mode word at offset 0x2098, 0 boot,
  2 attract, 3 game, 4 test; the race clock at 0x14120; the course chosen at 0xa8c4) then the
  work RAM (0x500000, 1 MB) at 0x40000. No host address ends up in it (`patches/0006`), so its
  hash is the same in every build and instance for the same inputs (natively too, until a race's
  floating point meets the C library's maths, which is not the same as wasm's: a few ULPs, then
  the races part; wasm is the same everywhere).
- **Reset**: power-on, by restoring the machine as it was right after loading (under 1 ms).
- **Errors**: a load that fails returns false with the reason logged; if the game's main loop
  ever ends (the recomp's dispatch halt) the core logs it and runs nothing until a reset or a
  state load.
- **Timings**: `srally_timings()` returns, summed since the last call, in microseconds:
  `{"frames","logic","geometry","raster","sound","audio","total"}` (logic: the game's code and
  the board; geometry: the frame's display list decoded into triangles; raster: the renderer;
  sound: the sound board; audio: handing samples over; total: all of `retro_run`).
- **Online**: deterministic (same build, same inputs: the same RAM and sound on every machine;
  checked) and a state is 38 MB, so like Daytona it should be `lockstep: true`.

## Video

The renderer's interface (`shim/video_api.h`, `video.h` from its port):
`srally_video_init(w, h)` once at load (Emscripten: after `srally_video_create_context()` made a
WebGL2 context on `Module.canvas`), `srally_video_render()` at the end of a frame the frontend
wants to see (`GET_AUDIO_VIDEO_ENABLE`), `srally_video_pixels()` (RGBA8 496x384, the fenced
read-back one frame late), `srally_video_shutdown()`, and an optional `srally_video_invalidate()`
the shim calls after a state load or reset. The shim, not the renderer, decodes each frame's
geometry (`model2_geo_decode`, every frame, shown or not, so the machine is the same either way;
with the SDL viewer's rule: not on 2D-only screens, the mesh cleared on entering one); the decode
has no worker thread, so a renderer's `model2_geo_kick()` does nothing. The renderer's files
(`model2_geo_gl.c`, `model2_gl.c`, `sys24_tile.c`, `sys24_gfx.c`, `sys24_viewer*.c`, `video.c`)
are linked outside the machine and allocate from the C library: drawing never changes the game.
Until the renderer's patch to `model2_geo_gl.c` is in `patches/`, `core.mk` links
`video_stub.c` instead: the tile layers on the CPU (menus, HUD), no 3D (~4.8 ms a frame in Chrome).

## How it works

- **One frame a `retro_run`**: the game owns its main loop (`post_reset_dispatch`), so it runs on
  a coroutine (`coro.c`): Emscripten fibers (Asyncify), natively `ucontext`. `patches/0001` adds
  a `frame_end` host op to the vblank wait every `geo_vsync_wait` goes through (once a game frame,
  as the SDL viewer presents once a wait); the shim's runs the boot screen's tile sync as the
  viewer does, then yields. A swap must not happen inside an export that returns a value (the
  unwound export's result is lost), so the game starts on the first `retro_run`. Each Emscripten
  instance learns once, from a probe fiber, the id Asyncify gives the bottom of a fiber's stack,
  and puts it in the game's fiber before each resume: a state from another instance carries that
  instance's.
- **No threads**: `patches/0002` runs the sound board inline (`model2_snd_run`: the queued MIDI
  bytes, then the 68000 and SCSP for n samples); `patches/0003` starts no geometry decode worker
  (`model2_geo_render_set_sync`); the vblank is stepped, not timed. `patches/0004` returns errors
  where the recomp's host exits; `patches/0005` gives `libc_printf`'s call of its dispatch the
  real prototype (in wasm a call through the wrong one traps); `patches/0006` fixes two places
  where lifted code used a host address as an i960 one (a word the boot stores, and the link
  block `game_dispatch_main` copies each frame, which read whatever the host pointer's low 32
  bits addressed: different per build and, natively, per run).
- **The machine is memory**: the recomp keeps its whole state in statics (i960 registers, RAM
  windows, the board) and lifted code keeps host pointers in registers, so a structured state is
  not practical. Instead the machine's objects are linked between two markers (`state_begin.c`,
  `state_end.c`; `layout.awk` checks the link map at every wasm link, and the core checks a few
  addresses at load), the recomp's `malloc` goes to two static arenas (`arena.c`: ROM images,
  never saved; the heap, saved up to its top), and the game's fiber stack and Asyncify buffer are
  in the machine. Everything is at the same address in every instance of a build, so a state is
  those bytes, less the maincpu ROM image and the geometry decode's scratch (`patches/0003`
  names it). The native bench saves and loads in-process (ASLR moves addresses between runs).

## Checks, bench, harness

```sh
node model2/srally/check.mjs                       # --rom ~/Downloads/srallycb.zip by default
node model2/srally/bench.mjs                       # FRAMES, WARMUP, RACE=1, OPTIONS="k=v,..."
model2/srally/.cache/native/bench ~/Downloads/srallycb.zip --frames 2400 \
  --script "900-905:SELECT,930-935:SELECT,990-995:START,1150-:UP" --state 1900:300 --shots 2000
node model2/srally/harness/serve.mjs               # then http://localhost:8792/model2/srally/harness/play.html?rom=srallycb
node model2/srally/harness/run.mjs "?rom=srallycb&race&warmup=2000&frames=600&shots=300,2300" out/srally
```

`check.mjs` (verified 2026-10-08, all PASS in ~40 s): system and AV info; a non-zip and a zip of
the wrong files fail cleanly; boot to the attract mode (frame 289); two coins (900, 930), START
(990), the accelerator from 1150: car select, transmission, and the race clock running from frame
1745; sound (766.63 samples a frame, peaks near full scale); a second load in the same module runs
as a fresh one; two instances (one with a different allocation history) give the same RAM and
sound hashes every 60 frames for 2400 frames; a state saved at frame 600 (attract) and 1900
(racing), 300 frames run, loaded back in the same instance and in a fresh one (reset, then the
state): the same RAM and sound hashes every 60 frames.

The bench (`main_native.c`): `--script` RetroPad events by frame, `--watch` RAM words,
`--state F:M` an in-process save / run / load / run, `--shots` the tile layers as PNGs.

## Measured (2026-10-08, M1 Max)

Milliseconds a frame (budget 17.38): the attract mode timed over frames 1200-1800, a race over
600 frames after check.mjs's script (2400 frames from power-on: coins, START, the accelerator; the
race clock runs from frame 1745). "Core" = without a picture asked for (`--no-video`, `NOVIDEO=1`,
`&novideo`: everything is emulated, nothing drawn).

| where | scene | frame avg / p95 / worst | game code | geometry decode | renderer | sound board |
|---|---|---|---|---|---|---|
| native (clang -O3, bench), core | attract | 1.14 / 1.31 / 1.41 | 0.05 | 0.72 | - | 0.37 |
| native, core | race | 0.92 / 1.04 / 1.14 | 0.04 | 0.47 | - | 0.40 |
| wasm, Node 24 (bench.mjs), core | attract | 1.62 / 1.86 / 2.95 | 0.07 | 1.08 | - | 0.46 |
| wasm, Node, core | race | 1.28 / 1.44 / 1.57 | 0.07 | 0.74 | - | 0.48 |
| wasm, Chrome 154 headless (web build), core | attract | 1.59 / 1.80 / 2.70 | 0.12 | 0.99 | - | 0.48 |
| wasm, Chrome, core | race | 1.31 / 1.50 / 1.90 | 0.13 | 0.68 | - | 0.49 |
| wasm, Chrome, tile stand-in drawing | race | 6.46 / 6.90 / 7.20 | 0.12 | 0.65 | 4.84 | 0.47 |

- Asyncify: the game's coroutine costs nothing measurable at run time (an `ASYNCIFY_REMOVE` of
  the sound board and the decode, which never yield, ran the same 1.26-1.31 ms); it nearly
  doubles the code (wasm 546 KB without it, 1.0 MB with). So no JSPI.
- Loading (inflate, CRC, images): ~320 ms in wasm, ~250 ms native. ROM images 36.3 MB (arena
  peak 40.3 of 48 MB); the heap arena's top stays under 5 MB in a race, peak 10.1 MB over a
  whole attract cycle (19,282 frames), of 16 MB. The game's own C stack is under 2 KB deep (of
  the 512 KB its fiber has). Wasm memory: 256 MB, no growth.
- State: 38,069,049 bytes (16 MB of it the heap arena's room); save 7 ms, load 3 ms (Node).

## Status

Done and checked: the core in Node and in Chrome (determinism, states, a race on the keys'
inputs), the native bench, the SDL viewer still building with the patches. Left: the 3D picture
(the renderer's port and its `model2_geo_gl.c` patch), the bar's integration (assets/games.ron,
the Makefile's upload, the cabinet: other agents), free play or other settings through
`nvram_dir` if the bar wants them.
