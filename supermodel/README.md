# Supermodel in the browser

[Supermodel](https://github.com/trzy/Supermodel), the Sega Model 3 emulator, built to WebAssembly
behind the libretro API, so the bar's emulator worker can run Model 3 games (Virtua Striker 2,
Virtua Fighter 3, Scud Race, ...) the way it runs the FBNeo cores. A spike for now: it boots,
draws and plays, but a frame costs more than the 16.7 ms a 60 Hz game allows (numbers below).

- `build.sh` fetches a pinned Supermodel, applies `patches/`, compiles with the pinned emsdk and
  links three outputs: `dist/supermodel.mjs` (WebGL2, for the page), `dist/headless/` (no-op GL,
  for Node) and `.cache/native/bench` (the same code natively, as a baseline).
- `shim/` is what replaces Supermodel's SDL front end: `libretro.cpp` (the API, the config, save
  states), `GL/glew.h` + `gl_shim.cpp` (OpenGL ES 3.0 in place of desktop GL, GLSL ES rewrite),
  `RetroInputSystem.cpp` (RetroPad buttons as Supermodel inputs), `osd/` (audio, single-threaded
  threads, file paths) and `Network/` (socket stubs).
- `bench.mjs` times frames in Node; `harness/` runs the WebGL2 build in a headless Chrome of our
  own (`node harness/serve.mjs`, then `node harness/run.mjs "?rom=vs298&warmup=1800&frames=300"`).
- To play: `node supermodel/harness/serve.mjs` and open
  http://localhost:8790/supermodel/harness/play.html?rom=vs298 in Chrome (keyboard and sound;
  `&ppc=0` for the real clock, 50 MHz otherwise).

From the page or worker, exactly as an FBNeo core, plus a canvas for the module to draw on:

```js
const core = await Core.create(createSupermodel, callbacks, { canvas: new OffscreenCanvas(496, 384) });
core.loadGame("vs298.zip", bytes); // 496x384 at 60 Hz, 44100 Hz stereo
```

Measured on 2026-10-04 with Virtua Striker 2 '98, 30 s into the attract mode, on an M-series Mac
(ms per frame; the PowerPC runs at the Model 3's real 166 MHz unless said):

| build | frame | PowerPC | 3D walk + GL | sound |
|---|---|---|---|---|
| native (clang, no GL) | 19.9 | 18.1 | 0.6 | 1.2 |
| wasm, Node (no GL) | 35.0 | 32.6 | 0.5 | 1.7 |
| wasm, Chrome, WebGL2 | 36.8 | 28.0 | 6.3 | 1.7 |
| wasm, Chrome, PowerPC at 50 MHz | 16.2 | 8.4 | 5.4 | 1.7 |

A save state is 31.9 MB and takes about 20 ms to write or read (through a file in the in-memory
file system). The PowerPC interpreter is the cost; a PowerPC-to-WebAssembly recompiler is what
would bring the real clock under budget.

In the bar: a game with `core: "supermodel"` in assets/games.ron runs on this module (the page
gives the worker's core an OffscreenCanvas), served from R2 at /supermodel/ (`make supermodel`,
`make supermodel-remote`). Online such games are `lockstep: true`: GGRS runs frames only once
everyone's input is in, with 4 frames of input delay and no per-frame saves, since a save state
is 32 MB and 20 ms. patches/0002 pins the Model 3's clock chip so the players' machines stay
identical. Joining a game in progress and spectating still hand over a whole 32 MB state.

Not done yet: `retro_get_memory_data` for desync checks, a memory-based save state, an input
delay that follows the measured ping, and the Model 3's 57.5 Hz (the core reports 60 Hz so its
735 samples a frame stay in step).
