// Times the Daytona core in Node (the headless build): how long a frame of one or two linked
// cabinets takes in WebAssembly, where the time goes, and how big and slow a save state is.
// Ends with the hash of cabinet 0's main RAM (as worker.js hashes it), to compare runs.
//
//   node daytona/bench.mjs [daytona/dist/headless/daytona.mjs] [~/Downloads/daytona.zip]
//
// FRAMES (default 600), WARMUP (default 600: past the boot), CABINETS (1 or 2, default 2), VIEW
// (0 or 1) and DRAW_HIDDEN (1: rasterize both cabinets) env vars change the run. PLAY=1 (free
// play presets) presses Start on every seat five times after the warm-up, 300 frames apart
// (start, circuit, transmission...), then holds the accelerator for 300 frames before timing: a
// race (with WARMUP=1500 the linked race of daytona/nvram/2 is on by then).
// OPTIONS="key=value,..." passes more daytona_set options (e.g. test_blank_images=1, and
// test_program=<file>, a file of this machine's put into the module's file system). STATE=<file>
// loads a whole-machine state after the game (reset, then the state; packed "vabz" states as
// worker.js makes them are unpacked), e.g. the arcade mode's seat state with CABINETS=1
// OPTIONS=link_topology=star,seat=0 STATE=daytona/dist/states/daytona.seat0.state.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { Core } from "../web/emulator/libretro.js";

const HERE = import.meta.dirname;
const [corePath = join(HERE, "dist/headless/daytona.mjs"), romPath = join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip")] =
  process.argv.slice(2);
const FRAMES = Number(process.env.FRAMES ?? 600);
const WARMUP = Number(process.env.WARMUP ?? 600);
const CABINETS = process.env.CABINETS ?? "2";

const { default: createDaytona } = await import(resolve(corePath));
const module = await createDaytona();

function set(key, value) {
  const alloc = (text) => {
    const size = module.lengthBytesUTF8(text) + 1;
    const ptr = module._malloc(size);
    module.stringToUTF8(text, ptr, size);
    return ptr;
  };
  const k = alloc(key);
  const v = alloc(value);
  module._daytona_set(k, v);
  module._free(k);
  module._free(v);
}
set("cabinets", CABINETS);
if (process.env.VIEW) set("view", process.env.VIEW);
if (process.env.DRAW_HIDDEN) set("draw_hidden", process.env.DRAW_HIDDEN);
for (const item of (process.env.OPTIONS ?? "").split(",").filter(Boolean)) {
  let [key, value] = item.split("=");
  if (key === "test_program") {
    module.FS.writeFile("/test_program.bin", readFileSync(value));
    value = "/test_program.bin";
  }
  set(key, value);
}

let audioFrames = 0;
let ran = 0; // frames run, for the sound per frame
let loudest = 0;
let lit = 0;
const core = new Core(module, {
  onFrame(rgba) { lit = 0; for (let i = 0; i < rgba.length; i += 4) if (rgba[i] | rgba[i + 1] | rgba[i + 2]) lit++; },
  onAudio(samples) { audioFrames += samples.length / 2; for (const s of samples) if (Math.abs(s) > loudest) loudest = Math.abs(s); },
  onLog(level, text) { if (level >= 1) console.error(text); },
});
const started = performance.now();
const av = core.loadGame(basename(romPath), /test_blank_images=1/.test(process.env.OPTIONS ?? "") ? new Uint8Array(4) : readFileSync(romPath));
console.log(`loaded in ${((performance.now() - started) / 1000).toFixed(1)} s: ${av.width}x${av.height} @ ${av.fps.toFixed(3)} Hz, ${av.sampleRate} Hz audio, ${CABINETS} cabinet(s)`);

if (process.env.STATE) {
  let state = new Uint8Array(readFileSync(process.env.STATE));
  if (String.fromCharCode(...state.subarray(0, 4)) === "vabz") {
    const stream = new Blob([state.subarray(4)]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
    state = new Uint8Array(await new Response(stream).arrayBuffer());
  }
  const loadStart = performance.now();
  core.reset();
  core.unserialize(state);
  console.log(`state ${process.env.STATE}: ${state.length} bytes, reset + load ${(performance.now() - loadStart).toFixed(1)} ms; link ${module.UTF8ToString(module._daytona_link_status())}`);
}
for (let i = 0; i < WARMUP; i++, ran++) core.run();
if (process.env.PLAY) {
  const START = 1 << 3, UP = 1 << 4;
  const press = (mask, frames) => { for (let i = 0; i < frames; i++, ran++) { core.inputs.fill(mask); core.run(); } };
  for (let i = 0; i < 5; i++) { press(START, 6); press(0, 294); }
  press(UP, 300);
  core.inputs.fill(UP);
}
module.UTF8ToString(module._daytona_timings()); // drop the warm-up's
const times = [];
for (let i = 0; i < FRAMES; i++) {
  const start = performance.now();
  core.run();
  times.push(performance.now() - start);
  ran++;
}
const sorted = [...times].sort((a, b) => a - b);
const avg = times.reduce((a, b) => a + b, 0) / times.length;
const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2);
console.log(`wasm: ${FRAMES} frames, ${avg.toFixed(2)} ms/frame average, ${at(0.5)} median, ${at(0.95)} p95, ${sorted.at(-1).toFixed(2)} worst ` +
  `(budget ${(1000 / av.fps).toFixed(2)}); ${(audioFrames / ran).toFixed(1)} audio frames per frame`);

const t = JSON.parse(module.UTF8ToString(module._daytona_timings()));
const ms = (us) => (us / t.frames / 1000).toFixed(2);
for (let k = 0; k < t.cabinets; k++) {
  console.log(`  cabinet ${k + 1}: game code + board ${ms(t.logic[k])} ms, geometrizer ${ms(t.geometry[k])} ms, rasterizer ${ms(t.raster[k])} ms, sound board ${ms(t.sound[k])} ms`);
}
console.log(`  resampling and mixing ${ms(t.audio)} ms; all of retro_run ${ms(t.total)} ms`);
console.log(`  link: ${module.UTF8ToString(module._daytona_link_status())}`);

try {
  let start = performance.now();
  const state = core.serialize();
  const saveMs = performance.now() - start;
  start = performance.now();
  core.unserialize(state);
  console.log(`state: ${state.length} bytes, save ${saveMs.toFixed(2)} ms, load ${(performance.now() - start).toFixed(2)} ms`);
} catch (error) {
  console.log(`state: ${error.message}`);
}

const ram = core.systemRam();
const words = new Uint32Array(ram.buffer, ram.byteOffset, ram.length >> 2);
let hash = 0x811c9dc5;
for (let i = 0; i < words.length; i++) hash = Math.imul(hash ^ words[i], 0x01000193);
console.log(`main RAM: ${ram.length} bytes, hash ${(hash >>> 0).toString(16).padStart(8, "0")}`);
console.log(`last frame: ${lit} of ${av.width * av.height} pixels lit; sound: loudest sample ${loudest} of 32767`);
const resetStart = performance.now();
core.reset();
console.log(`reset (power-on of every cabinet): ${(performance.now() - resetStart).toFixed(1)} ms; wasm memory ${(module.HEAPU8.length / 1048576).toFixed(0)} MB`);
