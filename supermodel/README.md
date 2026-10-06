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
  own (`node harness/serve.mjs`, then `node harness/run.mjs "?rom=vs298&warmup=1800&frames=300"`),
  and `harness/online-lab.mjs` plays the cabinet online in two headless Chromes through the site
  (`make dev`) and measures the netcode second by second, at a ping of your choice.
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
| the same, frame read back a frame late | 12.7 | 8.4 | 2.1 | 1.7 |

Reading the finished frame back from WebGL synchronously cost 4.7 ms of waiting for the GPU; the
core now reads each frame into a pixel buffer behind a fence and takes it out a frame later, and
hands the page RGBA bytes as they are (libretro.js's format 100), so the picture runs one frame
behind the machine. A save state is 31.9 MB and takes about 20 ms to write or read (through a
file in the in-memory file system). The PowerPC interpreter is the cost; a PowerPC-to-WebAssembly
recompiler is what would bring the real clock under budget. Codegen flags (`-flto`, `-msimd128`,
no exceptions) made no measurable difference.

In the bar: a game with `core: "supermodel"` in assets/games.ron runs on this module (the page
gives the worker's core an OffscreenCanvas), served from R2 at /supermodel/ (`make supermodel`,
`make supermodel-remote`). Online such games are `lockstep: true`: GGRS runs frames only once
everyone's input is in and never saves per frame, since a save state is 32 MB. The worker
(web/emulator/worker.js) runs one frame per 60 Hz slot on a precise clock, sends each input the
moment GGRS makes it, and picks the input delay from the measured round trip, then follows the
other machines' lateness: a frame more when their input keeps arriving late, a frame less after a
quiet stretch (the status line shows the delay and any waits). Every 120 frames the machines
compare a hash of the game's RAM; if they have drifted apart, the lowest seat hands its machine
to everyone again and play goes on in step half a second later (the lab's `--poke` flips a word
of one machine's RAM to try it). patches/0002 pins the Model 3's clock chip so the players'
machines stay identical; patches/0006 puts the PowerPC's timebase and decrementer anchors in
the save state (Supermodel left them out, so a machine joining a game read a different
timebase than the host and took the decrementer interrupt at a different cycle, and the two
drifted apart: a goal on one machine only); patches/0005 makes every NaN the PowerPC FPU
produces the same one (WebAssembly leaves a NaN's bits to the host, and x86 and ARM differ).
Joining a game in progress and spectating hand over a 32 MB state, deflated to about 5 MB.

Not done yet: the Model 3's 57.5 Hz (the core reports 60 Hz so its 735 samples a frame stay in
step), and presenting frames from the worker itself instead of the one-frame-late read-back.
