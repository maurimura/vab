// Times the Supermodel core in Node: how long a Model 3 frame takes in WebAssembly, and how big
// and slow a save state is. The headless build draws into a no-op OpenGL, so this is the CPU side
// of a frame: PowerPC, sound boards, tile generator and the Real3D scene walk.
//
//   node supermodel/bench.mjs supermodel/dist/headless/supermodel.mjs ~/Downloads/vs298.zip
//
// FRAMES (default 600), WARMUP (default 120) and PPC (PowerPC MHz; default the real clock) env
// vars change the run.
import { readFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { Core } from "../web/emulator/libretro.js";

const [corePath, romPath] = process.argv.slice(2);
const FRAMES = Number(process.env.FRAMES ?? 600);
const WARMUP = Number(process.env.WARMUP ?? 120);
const PPC = process.env.PPC;

const { default: createSupermodel } = await import(resolve(corePath));
const module = await createSupermodel();

function set(key, value) {
  const alloc = (text) => {
    const size = module.lengthBytesUTF8(text) + 1;
    const ptr = module._malloc(size);
    module.stringToUTF8(text, ptr, size);
    return ptr;
  };
  const k = alloc(key);
  const v = alloc(value);
  module._supermodel_set(k, v);
  module._free(k);
  module._free(v);
}
if (PPC) set("PowerPCFrequency", PPC);

let audioFrames = 0;
let loudest = 0;
const core = new Core(module, {
  onFrame() {},
  onAudio(samples) { audioFrames += samples.length / 2; for (const s of samples) if (Math.abs(s) > loudest) loudest = Math.abs(s); },
  onLog(level, text) { if (level >= 2 || !/^(\s|'|Opened |vs\d|Optional ROM)/.test(text)) console.error(text); }, // skip the ROM listing
});
const started = performance.now();
const av = core.loadGame(basename(romPath), readFileSync(romPath));
console.log(`loaded in ${((performance.now() - started) / 1000).toFixed(1)} s: ${av.width}x${av.height} @ ${av.fps} Hz, ${av.sampleRate} Hz audio`);

for (let i = 0; i < WARMUP; i++) core.run();
module.UTF8ToString(module._supermodel_timings()); // drop the warm-up's
const times = [];
for (let i = 0; i < FRAMES; i++) {
  const start = performance.now();
  core.run();
  times.push(performance.now() - start);
}
const sorted = [...times].sort((a, b) => a - b);
const avg = times.reduce((a, b) => a + b, 0) / times.length;
const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2);
console.log(`wasm${PPC ? ` (PowerPC ${PPC} MHz)` : ""}: ${FRAMES} frames, ${avg.toFixed(2)} ms/frame average, ${at(0.5)} median, ${at(0.95)} p95, ${sorted.at(-1).toFixed(2)} worst; ${(audioFrames / (WARMUP + FRAMES)).toFixed(1)} audio frames per frame`);

const t = JSON.parse(module.UTF8ToString(module._supermodel_timings()));
const ms = (us) => (us / t.frames / 1000).toFixed(2);
console.log(`  of which PowerPC board ${ms(t.ppc)} ms, Real3D scene walk ${ms(t.render)} ms, sound boards ${ms(t.sound)} ms, drive board ${ms(t.drive)} ms`);

let start = performance.now();
const state = core.serialize();
const saveMs = performance.now() - start;
start = performance.now();
core.unserialize(state);
const loadMs = performance.now() - start;
console.log(`state: ${state.length} bytes, save ${saveMs.toFixed(2)} ms, load ${loadMs.toFixed(2)} ms`);
console.log(`sound: loudest sample ${loudest} of 32767`);
