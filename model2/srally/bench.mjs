// Times the Sega Rally core in Node (the headless build: no picture): how long a frame takes in
// WebAssembly, where the time goes (the game's code, the geometry decode, the sound board), and
// how big and slow a save state is. Ends with the hash of the i960's RAM (as worker.js hashes
// it), to compare runs and builds.
//
//   node model2/srally/bench.mjs [model2/srally/dist/headless/srally.mjs] [~/Downloads/srallycb.zip]
//
// FRAMES (default 600) frames timed after WARMUP (default 1200: the boot and into the attract
// mode's 3D). RACE=1 instead inserts a coin, presses START and drives (holds the accelerator)
// through the menus into a race before timing (RACE_FRAMES, default 3600, to get there).
// OPTIONS="key=value,..." passes srally_set options. NOVIDEO=1 runs without the picture (as a
// rollback re-run: the core still emulates everything, the renderer draws nothing).
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { Core } from "../../web/emulator/libretro.js";
import { RACE_SCRIPT, hash, modes, raceMask } from "./script.mjs";

const HERE = import.meta.dirname;
const [corePath = join(HERE, "dist/headless/srally.mjs"), romPath = join(process.env.ROMS ?? join(homedir(), "Downloads"), "srallycb.zip")] =
  process.argv.slice(2);
const FRAMES = Number(process.env.FRAMES ?? 600);
const WARMUP = Number(process.env.WARMUP ?? 1200);
const RACE = process.env.RACE === "1";
const RACE_FRAMES = Number(process.env.RACE_FRAMES ?? RACE_SCRIPT.until);

const { default: createSrally } = await import(resolve(corePath));
const module = await createSrally();
const set = (key, value) => {
  const alloc = (text) => {
    const size = module.lengthBytesUTF8(text) + 1;
    const ptr = module._malloc(size);
    module.stringToUTF8(text, ptr, size);
    return ptr;
  };
  const k = alloc(key), v = alloc(value);
  module._srally_set(k, v);
  module._free(k);
  module._free(v);
};
for (const item of (process.env.OPTIONS ?? "").split(",").filter(Boolean)) set(...item.split("="));

let audioFrames = 0, loudest = 0, ran = 0;
const core = new Core(module, {
  onFrame() {},
  onAudio(samples) { audioFrames += samples.length / 2; for (const s of samples) if (Math.abs(s) > loudest) loudest = Math.abs(s); },
  onLog(level, text) { if (level >= 1 || process.env.VERBOSE) console.error(text); },
});
if (process.env.NOVIDEO === "1") core.present = false;
let started = performance.now();
const av = core.loadGame(basename(romPath), readFileSync(romPath));
console.log(`loaded in ${(performance.now() - started).toFixed(0)} ms: ${av.width}x${av.height} @ ${av.fps.toFixed(3)} Hz, ${av.sampleRate} Hz audio`);

const run = (mask) => { core.inputs[0] = mask; core.run(); ran++; };
started = performance.now();
if (RACE) {
  for (let f = 0; f < RACE_FRAMES; f++) run(raceMask(f));
  console.log(`race script: ${RACE_FRAMES} frames in ${((performance.now() - started) / 1000).toFixed(1)} s; ${JSON.stringify(modes(core.systemRam()))}`);
} else {
  for (let f = 0; f < WARMUP; f++) run(0);
  console.log(`warm-up: ${WARMUP} frames in ${((performance.now() - started) / 1000).toFixed(1)} s`);
}
module.UTF8ToString(module._srally_timings()); // drop the warm-up's
const times = [];
for (let i = 0; i < FRAMES; i++) {
  const start = performance.now();
  run(RACE ? raceMask(RACE_FRAMES + i) : 0);
  times.push(performance.now() - start);
}
const sorted = [...times].sort((a, b) => a - b);
const avg = times.reduce((a, b) => a + b, 0) / times.length;
const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2);
console.log(`wasm: ${FRAMES} frames${RACE ? " in a race" : " of the attract mode"}, ${avg.toFixed(2)} ms/frame average, ${at(0.5)} median, ${at(0.95)} p95, ` +
  `${sorted.at(-1).toFixed(2)} worst (budget ${(1000 / av.fps).toFixed(2)}); ${(audioFrames / ran).toFixed(2)} audio frames per frame`);
const t = JSON.parse(module.UTF8ToString(module._srally_timings()));
const ms = (us) => (us / t.frames / 1000).toFixed(2);
console.log(`  game code ${ms(t.logic)} ms, geometry decode ${ms(t.geometry)} ms, renderer ${ms(t.raster)} ms, sound board ${ms(t.sound)} ms, ` +
  `audio hand-over ${ms(t.audio)} ms; all of retro_run ${ms(t.total)} ms`);
console.log(`  the game's stack: ${module._srally_stack_used()} bytes used; heap arena ${module._srally_heap_top()} bytes (peak ${module._srally_heap_peak()})`);

try {
  let start = performance.now();
  const state = core.serialize();
  const saveMs = performance.now() - start;
  start = performance.now();
  core.unserialize(state);
  console.log(`state: ${state.length} bytes, save ${saveMs.toFixed(1)} ms, load ${(performance.now() - start).toFixed(1)} ms`);
} catch (error) {
  console.log(`state: ${error.message}`);
}
const ram = core.systemRam();
console.log(`i960 RAM: ${ram.length} bytes, hash ${hash(ram)}; sound: loudest sample ${loudest} of 32767`);
const resetStart = performance.now();
core.reset();
console.log(`reset (power-on): ${(performance.now() - resetStart).toFixed(1)} ms; wasm memory ${(module.HEAPU8.length / 1048576).toFixed(0)} MB`);
