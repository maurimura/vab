# Flycast in the browser (Sega NAOMI: Virtua Tennis)

[Flycast](https://github.com/flyinghead/flycast) (Dreamcast, NAOMI, NAOMI 2, Atomiswave) built to
WebAssembly behind the libretro API, so the bar's emulator worker runs it as it runs the FBNeo
cores, Supermodel and MAME. The game it is for is **Virtua Tennis / Power Smash** (`vtennisg`, the
NAOMI GD-ROM release, GDS-0011). A NAOMI is Dreamcast hardware: an SH4 at 200 MHz, the PowerVR2
GPU, the AICA sound chip with its own ARM7.

Upstream Flycast has no WebAssembly port. This module is built on
[retrom-project/flycast-wasm](https://github.com/retrom-project/flycast-wasm) (branch `retrom/1.0`,
a fork of [nasomers/flycast-wasm](https://github.com/nasomers/flycast-wasm) with NAOMI fixes):
its patch set turns flyinghead/flycast (2c48c01) into an Emscripten libretro core with an SH4 ->
WebAssembly JIT (`core/rec-wasm`: SH4 decoded to Flycast's SHIL, each block compiled to a small
wasm module sharing the core's memory and function table, chained into multi-block modules in
the background). Their published core is an EmulatorJS/RetroArch build (ASYNCIFY, its own pacer,
AudioWorklet and HUD); we take only the core, add our patches and shim, and link our own module
as `mame/build.sh` does. Flycast draws with OpenGL ES 3 (WebGL2 here); our frontend has no
`RETRO_ENVIRONMENT_SET_HW_RENDER`, so the shim does that part inside the module.

## Files

- `build.sh`: the pinned emsdk; retrom-project/flycast-wasm at a pinned commit, the files we take
  from it checked by sha256; flyinghead/flycast at the commit the fork pins, with its submodules;
  the fork's patches, the JIT's sources, then ours (applied once: the set applied is recorded in
  `.cache/applied` and taken off again in reverse when it changes); Flycast's own CMake build as
  a libretro core (`LIBRETRO=ON USE_GLES=ON`, `-fwasm-exceptions`); then our shim and the link of
  `dist/flycast.mjs` + `dist/flycast.wasm` (`createFlycast`, ES module, web/worker/node).
  `./flycast/build.sh link` only recompiles the shim and relinks; `DEBUG=1` links with
  assertions and function names. `EMSDK_DIR`, `JOBS` as in the other modules.
- `exports.json`: the libretro API, `_flycast_set`, `_flycast_stats`, `_flycast_jit_mode`, and
  the JIT's helpers (`_wasm_mem_read8`... `_wasm_div1`), which the compiled blocks import.
- `patches/`: ours, applied after the fork's (each starts with why).
- `shim/vab_flycast.cpp`: the hardware-render side of the frontend (context, read-back), the core
  options, `flycast_set`/`flycast_stats`. `shim/gl_null.mjs` writes the no-op OpenGL ES 3.0 used
  when there is no canvas.
- `inputs.mjs`: the scripted inputs and the RAM hash shared by the checks and the harness.
- `bench.mjs`: frame times in Node, from boot or from a state; state size and save/load times.
- `check.mjs`: determinism (below). Makes its own start state on first run.
- `lab.mjs`: the cabinet in the bar end to end, in headless Chromes of its own against `make dev
  BUCKET=local`: a player alone from the start-up state into a match, a second in lockstep, a
  watcher ([Virtua Tennis](../README.md#virtua-tennis) in the root README).
- `harness/`: `serve.mjs` (port 8791, ROMs from `flycast/.cache/roms`), `index.html` +
  `core-worker.mjs` (the core in a worker on an OffscreenCanvas as in the bar: boot, attract,
  coins and menus, a match; timings, screenshots, sound levels), `run.mjs` (drives it in a headless
  Chrome of its own, profile `flycast-chrome-*`, screenshots to `.cache/checks/`), `play.html`
  (play it with the keyboard), `det-node.mjs` (the `?det=` run in Node, to compare with Chrome).

Builds from scratch in ~3.5 minutes on this M-series Mac (checkouts ~1.5 min, CMake configure
~1 min, the build ~1 min; `.cache/flycast` is 670 MB with submodules, `.cache/build` 160 MB);
after a change to a patch ~30 s, to the shim ~10 s. Two builds of the same inputs are
byte-identical. `flycast.wasm` is 5.8 MB, `flycast.mjs` 173 KB.

## The contract

```js
const core = await Core.create(createFlycast, callbacks, { canvas }); // an OffscreenCanvas, as worker.js does
core.addFile("vtennisg/gds-0011.chd", disc);   // the GD-ROM, in the folder named after the set
core.loadGame("vtennisg.zip", zip);            // 640x480 @ 59.7938 Hz, 44101.43 Hz stereo
```

- **Files** (MEMFS): `/roms/vtennisg.zip` (the game is loaded from that path) and
  `/roms/vtennisg/gds-0011.chd` (Flycast opens `<dir of the zip>/<zip name>/<gdrom name>.chd`).
  The zip is MAME's `vtennisg` merged with the `naomigd` BIOS: the BIOS EPROMs are found by CRC
  inside it (`patches/0004`), so no `/system/dc/naomi.zip` is needed. The shim creates
  `/system/dc` and `/save/reicast`; Flycast writes the NAOMI's EEPROM/NVRAM files there when the
  game is unloaded or reset, and nothing reads them back unless they survive in MEMFS.
- **Drawing**: with `Module.canvas` (an OffscreenCanvas of any size) the shim sizes it to 853x480,
  makes a WebGL2 context on it (`specialHTMLTargets["!canvas"]`), answers Flycast's
  `SET_HW_RENDER` (framebuffer 0, `emscripten_webgl_get_proc_address`, a plain `glGetString`
  version), calls `context_reset` once the game is loaded, and hands each presented frame to the
  video callback as RGBA bytes (pixel format 100; XRGB8888 if the frontend refuses it), read back
  through a pixel buffer two `retro_run`s later (`vab_readback_latency`, below); duplicates are
  NULL (`GET_CAN_DUPE`). It skips the read-back on frames the frontend won't show
  (`GET_AUDIO_VIDEO_ENABLE` bit 0 clear: rollback replays). Without a canvas (Node) Flycast gets
  the no-op GL: the same machine, and a black frame for each one it presents (so the Node checks
  see frames as the page does).
- **One frame per `retro_run`**: exactly one vblank of the machine (3344830 SH4 cycles, `patches/0002`),
  whatever the game draws; no pacing, skipping or owed frames. The rates reported are the
  machine's: 59.7938 Hz (200 MHz / 3344830; not the NTSC 59.94 Flycast reports) and 44101.43 Hz
  (200 MHz / 4535, the AICA's sample clock), so 737.56 samples per frame: the worker's pacing and
  resampling then keep sound and picture in step. `retro_get_system_av_info`: base 640x480,
  aspect 4:3, max 853x853.
- **Sound**: `retro_audio_sample_batch`, int16 stereo, ~737.6 frames per `retro_run`.
- **Inputs**: a RetroPad per player on ports 0 and 1 (Flycast's NAOMI mapping, shot12 inputs):
  stick = D-pad, **B = SHOT1**, **A = SHOT2**, **Start**, **Select = coin**, R3 = service (always on
  a NAOMI), L3 = test (only with `reicast_allow_service_buttons`). Descriptors: `SHOT1`, `SHOT2`,
  `Start`, `Coin`, `Test`, D-pad. No bitmask polling (the frontend doesn't offer it).
- **Memory**: `retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM)` is the NAOMI's 32 MB of main RAM
  (`mem_b`; what lockstep checkpoints hash). VRAM (16 MB) and sound RAM (8 MB) are in states only.
- **States**: ~57 MB of data in a 73 MB buffer (`retro_serialize_size` is fixed for the game at
  its first call: Flycast's worst case for the TA contexts present then, plus 8 MB; the unused
  tail is zeroed, so states deflate well and a machine always makes the same bytes). States are
  complete: loading one into a fresh instance (after `retro_reset` or not) replays exactly.
  `retro_reset` reloads the game (~40 ms). Neither a reset nor a state load discards the JIT's
  compiled code (`patches/0007`): every block re-verifies its hash at its next dispatch instead.
  A machine whose JIT is cold (a joiner's) still compiles the game's code as it runs, ~2600
  blocks over 4-5 s with frames of 100-300 ms, so the bar's worker plays 300 frames blind after
  loading a handed-over state and goes back to it (`vab_warmup` in assets/games.ron).
- **Exports besides libretro**: `_flycast_set(key, value)` (C strings; `Core.setOption`) sets a
  core option for the next load; `_flycast_stats()` returns JSON counters since the last call
  (`runs`, `presented`, `delivered`, read-back timings, guest cycles per run);
  `_flycast_jit_mode(n)` (tests only).

### Core options

Flycast's libretro option keys are still named `reicast_*`. The shim answers them in this order:
forced value, `flycast_set`, the frontend's `GET_VARIABLE` (its `OPTIONS` map), the shim's default.

Forced (the frame loop and determinism; `flycast_set` refuses them):
`reicast_threaded_rendering` disabled, `reicast_auto_skip_frame` disabled,
`reicast_frame_skipping` disabled, `reicast_detect_vsync_swap_interval` disabled,
`reicast_enable_rttb` disabled and `reicast_emulate_framebuffer` disabled (nothing the GPU draws
goes back into VRAM: the no-op GL and every GPU must give the same machine), `reicast_sh4clock`
200, `reicast_hle_bios` disabled, `reicast_emulate_bba`/`reicast_upnp`/`reicast_dcnet`/
`reicast_network_output` disabled, custom/dumped textures disabled, `reicast_gdrom_fast_loading`
disabled.

Defaults (the frontend may set them; none changes how the machine runs across players as long
as everyone has the same): `reicast_force_freeplay` enabled, `reicast_region` USA (Virtua Tennis;
Japan is Power Smash), `reicast_language` English, `reicast_broadcast` NTSC, `reicast_cable_type`
VGA, `reicast_internal_resolution` 640x480, `reicast_alpha_sorting` per-triangle (normal),
`reicast_enable_dsp` enabled, `reicast_delay_frame_swapping` enabled, `reicast_mipmapping` enabled,
`reicast_anisotropic_filtering` off, `reicast_texture_filtering` 0, `reicast_texupscale` 1,
`reicast_widescreen_hack`/`_cheats` disabled, `reicast_allow_service_buttons` disabled, fog and
modifier volumes enabled. Not a Flycast option: `vab_readback_latency` 2 (or 1).

Region, free play and the DSP change the game itself, so all players must have the same.

## Patches

On top of retrom-project/flycast-wasm's `wasm-jit-phase1-modified.patch`, `flycast-webgl.patch`
and `flycast-rom-crc.patch` (not its `flycast-range.patch`: HTTP Range streaming through ASYNCIFY):

- `0001-libretro-shell-for-the-bar`: retro_run without the fork's EmulatorJS pacer and HUD (it ran
  0-2 frames per call by the wall clock and used `window`); sound to `audio_batch_cb`, not the
  fork's AudioWorklet; the real frame and sample rates; states saved/loaded without stopping and
  restarting the emulator, at one padded size; the shim's hooks.
- `0002-one-vblank-per-retro-run`: the SH4 stops at every vblank, never when a frame is presented
  or after a render timeout.
- `0003-fixed-clock`: the AICA real-time clock starts at 2001-01-01 00:00:00 on every machine (and
  the Dreamcast-only clock reads follow).
- `0004-naomi-bios-from-the-rom-set`: the BIOS looked up in the ROM set by its full path (it was
  opened by bare file name, so only a separate `naomi.zip` worked).
- `0005-deterministic-jit`: new blocks always compile on the spot (the fork deferred them past a
  24 ms wall-clock budget and ran them through a SHIL bridge that charges other cycles); the
  JIT's import "thunk resolution" no longer calls its helpers (it flushed the store queue into
  RAM, wrote address 0 and changed SR on the first compile, which corrupted a state loaded
  before it); NaN results of FPU arithmetic stored as the SH4's 0x7FBFFFFF in the JIT and the
  interpreter (x86 and ARM hosts make different NaNs).
- `0006-scif-timers-from-the-state`: loading a state no longer reschedules the SH4 serial port's
  timer from the pre-load clock.
- `0007-keep-the-jit-across-state-loads`: a cache reset (a state load, `retro_reset`) keeps the
  compiled modules and marks every RAM page as written, so each block re-verifies its hash at its
  next dispatch (the fork's own self-modifying-code check) and only changed code is recompiled.
  The compiled code bakes in the addresses of the RAM, VRAM, sound RAM and the SH4 context block,
  which a game reload freed and allocated again (elsewhere, once the JIT had allocated in
  between: every kept block then read freed memory and the machine spun), so those now stay put
  across a reload, and a region that did move still costs the full reset. Online, the host and
  every machine that resyncs would otherwise recompile ~2600 blocks over 4-5 s after each join,
  each hitch a wait for the others in lockstep.

## Measured

2026-10-08, M-series Mac (10 threads) shared with other builds (load average 4-6), Chrome 154
headless (WebGL2 through ANGLE/Metal) with the core in a worker as in the bar, frames paced at
59.79 Hz; Node 24 with the no-op GL. "Attract" is the attract mode after boot, "match" a rally in
a match (scripted stick and shots). Budget 16.72 ms.

| ms per `retro_run` | avg | p50 | p95 |
|---|---|---|---|
| Chrome, attract | 10.3 | 10.1 | 12.8 |
| Chrome, match | 10.7 | 10.3 | 14.2 |
| Chrome, match, read-back latency 1 | 13.4-15.7 | 14.0 | 18-22 |
| Node, match from a state (JIT cold) | 9.7 | 9.1 | 12.4 |
| Node, match, JIT warm | 9.3 | 8.9 | 11.3 |
| Node, attract | 8.4 | 8.5 | 10.3 |
| boot (BIOS + GD-ROM load, ~10200 frames, unpaced) | 2.4 | 1.3 | 10.3 |

Where a match frame goes in Chrome: ~9 ms emulation (SH4 JIT code and its dispatch loop ~60%,
the AICA/ARM7 ~10%, the DSP ~1 ms, the PowerVR TA and renderer the rest), ~0.9 ms taking the
frame out of the pixel buffer (`glGetBufferSubData`; 3-5 ms with latency 1, when the GPU process
hasn't finished the frame yet), 0.2 ms the frontend's copy. Without the DSP
(`reicast_enable_dsp` disabled) frames are ~1 ms cheaper; with `per-strip` alpha sorting the GPU
side shrinks too (fewer draw calls), at some cost in translucency order. The worst frames are the
JIT compiling new code: ~90 ms at scene changes during boot, ~300 ms right after a state load
(the cache is cleared), a few tens of ms otherwise.

States: 73.0 MiB buffer; serialize 21 ms / unserialize 17 ms through JavaScript in Chrome (11 / 7
in Node), in-wasm slot save 1.5 ms / load 4.6-6.7 ms (Node). The game boots to its attract mode in
~10200 frames (2.8 minutes of game time; the NAOMI BIOS loads the GD-ROM into the DIMM board): 25 s
unpaced in Chrome or Node.

Siblings: Tekken 3 on MAME 7.7-9.1 ms, Virtua Striker 2 on Supermodel 12 ms. Virtua Tennis sits
between, over the ~8 ms that leaves room for rollback re-simulation, and its 57 MB states (an
in-wasm save is 1.5 ms, a load 5-7 ms, and a load clears the JIT) make rollback impractical:
**lockstep**.

## Determinism

`node flycast/check.mjs` (all pass, 2026-10-08), from a state in a match, both players' random
inputs, the 32 MB RAM hashed every 60 frames:

- two processes, one compiling in a tight loop (no chain modules ever land), one yielding to the
  event loop each frame (background chain compiles land): identical over 1800 frames;
- a process whose JIT already ran the inputs, reloading the state: identical;
- save after 600 frames, 600 more, back to the save, the same 600: same RAM and the same whole
  state byte for byte; another process, `retro_reset` then that save: same RAM;
- a rollback of 4 frames every 7 for 600 frames, a state saved every frame: same RAM as straight
  through.

And the harness's `?det=1200` in Chrome (WebGL2 in a worker) gives the same 20 RAM hashes as
`harness/det-node.mjs` in Node (no-op GL): the renderer never touches the machine. Runs from boot
are reproducible too (the same RAM samples run after run).

What was needed (patches 0003, 0005, 0006): the AICA clock; JIT blocks never deferred to the
SHIL bridge (`flycast_jit_mode(1)`, upstream's wall-clock deferral, diverges within a second);
the JIT's first-compile side effects; the serial timer's restore. The NaN canonicalization is a
precaution: no NaN reached RAM in our runs (the same RAM with and without it), and it costs
nothing measurable. All players must run the same core build and the same options.

## Known gaps

- The NAOMI BIOS's boot screens (GD-ROM check, loading progress) don't show: ~2.8 minutes of
  black then the logo. Flycast notices direct framebuffer writes through page protection, which
  WebAssembly lacks. A start-up state (emulator/snapshot.mjs) skips the boot anyway.
- A JIT that hasn't run the game yet compiles it as it goes: ~2600 blocks over 4-5 s, hitches up
  to ~300 ms. The compiled code now survives resets and state loads (`patches/0007`), and a
  joiner warms up before its session (`vab_warmup`), but a watcher still starts cold.
- States are big (73 MB buffer) and a frame costs ~10.5 ms: lockstep, not rollback.
- Display latency: frames reach the frontend two `retro_run`s late (one with
  `vab_readback_latency` 1, costing 3-5 ms more a frame in a match). A worker that handed the page
  `OffscreenCanvas.transferToImageBitmap()` frames instead of RGBA bytes would skip the read-back.
- Double-precision FPU ops run in the interpreter (as in the fork); single precision is compiled.
- Only `vtennisg` has been run. Other NAOMI games should load the same way (the BIOS from a merged
  set, a GD-ROM disc in the set's folder, the 59.79 Hz rate); NAOMI 2/Atomiswave untested.
- The fork's JIT is young (2026) and large (9000 lines); this core has been run for minutes, not
  hours.
