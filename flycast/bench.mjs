// Times the Flycast core (flycast/build.sh) in Node, through the bar's own frontend (libretro.js):
// how long a frame takes from boot and from a state, how big and slow a save state is, how many
// frames the game presents and how much it sounds. Node has no canvas, so the core draws with its
// no-op GL (shim/gl_null.mjs): these are the machine's costs (SH4 JIT, AICA and its ARM7, the
// PowerVR's TA, the renderer's scene walk) without the GPU and the read-back; the harness
// (harness/run.mjs) times the whole frame in Chrome.
//
//   node flycast/bench.mjs                         boot (BOOT frames), then FRAMES timed frames
//   node flycast/bench.mjs flycast/.cache/states/match-<build>.state    from a state (check.mjs makes one)
//
// Env: FRAMES (timed frames, default 1800), BOOT (frames run from power-on before timing when no
// state is given, default 10800: the NAOMI BIOS, the GD-ROM load, the attract mode), PLAY=1
// (random inputs for both players while timed: a rally when the state is in a match), JIT=n
// (flycast_jit_mode: 0 the bar's synchronous compiles, 1 upstream's wall-clock deferral),
// ROMS (default flycast/.cache/roms), CORE (default flycast/dist/flycast.mjs).
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { loadavg } from "node:os";
import { join, resolve } from "node:path";
import { Core } from "../web/emulator/libretro.js";
import { hash, inputs } from "./inputs.mjs";

const HERE = import.meta.dirname;
const statePath = process.argv[2];
const FRAMES = Number(process.env.FRAMES ?? 1800);
const BOOT = Number(process.env.BOOT ?? 10800);
const ROMS = resolve(process.env.ROMS ?? join(HERE, ".cache/roms"));
const CORE = resolve(process.env.CORE ?? join(HERE, "dist/flycast.mjs"));

let started = performance.now();
const { default: createFlycast } = await import(CORE);
const module = await createFlycast();
let framesDrawn = 0, audioFrames = 0, loudest = 0, sumSquares = 0;
const errors = [];
const core = new Core(module, {
  onFrame() { framesDrawn++; },
  onAudio(samples) {
    audioFrames += samples.length / 2;
    for (const s of samples) {
      if (Math.abs(s) > loudest) loudest = Math.abs(s);
      sumSquares += s * s;
    }
  },
  onLog(level, text) { if (level >= 2) errors.push(text); },
});
core.netplay = true;
if (process.env.JIT) module._flycast_jit_mode(Number(process.env.JIT));
const createMs = performance.now() - started;
core.addFile("vtennisg/gds-0011.chd", readFileSync(join(ROMS, "vtennisg/gds-0011.chd")));
started = performance.now();
const av = core.loadGame("vtennisg.zip", readFileSync(join(ROMS, "vtennisg.zip")));
const loadMs = performance.now() - started;
console.log(`core ${(createMs / 1000).toFixed(2)} s, vtennisg loaded in ${(loadMs / 1000).toFixed(2)} s: ` +
  `${av.width}x${av.height} @ ${av.fps.toFixed(4)} Hz, ${av.sampleRate.toFixed(1)} Hz audio, aspect ${av.aspectRatio.toFixed(3)}`);
console.log(`buttons: ${[...core.buttons].map(([id, name]) => `${id}=${name}`).join(", ")}`);
console.log(`system RAM: ${core.systemRam().length} bytes`);
const stats = () => JSON.parse(module.UTF8ToString(module._flycast_stats()));

const timed = (label, n, script) => {
  const times = [], cpu = [];
  stats();
  audioFrames = 0; loudest = 0; sumSquares = 0;
  for (let i = 0; i < n; i++) {
    if (script) { core.inputs[0] = script[i][0]; core.inputs[1] = script[i][1]; }
    const c = process.cpuUsage();
    const t = performance.now();
    core.run();
    times.push(performance.now() - t);
    const used = process.cpuUsage(c);
    cpu.push((used.user + used.system) / 1000);
  }
  const s = stats();
  const summary = (list) => {
    const sorted = [...list].sort((a, b) => a - b);
    const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2);
    return `${(list.reduce((a, b) => a + b, 0) / list.length).toFixed(2)} avg, ${at(0.5)} p50, ${at(0.95)} p95, ${sorted.at(-1).toFixed(2)} max ms`;
  };
  console.log(`${label}: ${n} frames, wall ${summary(times)}`);
  console.log(`${" ".repeat(label.length)}  CPU ${summary(cpu)}; load average ${loadavg().map((l) => l.toFixed(1)).join(" ")}`);
  console.log(`${" ".repeat(label.length)}  the game presented ${s.presented} frames; ${(s.cycles / Math.max(1, s.timedRuns)).toFixed(0)} SH4 cycles a frame; ` +
    `sound ${(audioFrames / n).toFixed(2)} samples a frame, loudest ${loudest}, rms ${Math.sqrt(sumSquares / Math.max(1, audioFrames * 2)).toFixed(0)}`);
};

if (statePath) {
  core.unserialize(readFileSync(statePath));
  console.log(`state ${statePath}`);
} else {
  started = performance.now();
  for (let i = 0; i < BOOT; i++) core.run();
  console.log(`boot: ${BOOT} frames in ${((performance.now() - started) / 1000).toFixed(1)} s ` +
    `(${((performance.now() - started) / BOOT).toFixed(2)} ms/frame; the BIOS and the GD-ROM load first)`);
}
const script = process.env.PLAY === "1" ? inputs(FRAMES, 5) : undefined;
timed(statePath ? "from the state (JIT cold)" : "after boot", FRAMES, script);
if (statePath) {
  core.unserialize(readFileSync(statePath));
  timed("again (JIT warm)", FRAMES, script);
}

// Save states: through JavaScript (serialize) and within wasm memory (slots, as rollback does).
const time = (fn, n = 10) => {
  const t = performance.now();
  for (let i = 0; i < n; i++) fn();
  return (performance.now() - t) / n;
};
const state = core.serialize();
const saveMs = time(() => core.serialize());
const loadStateMs = time(() => core.unserialize(state));
core.allocSlots(2);
const slotSaveMs = time(() => core.saveSlot(0));
const slotLoadMs = time(() => core.loadSlot(0));
console.log(`state: ${state.length} bytes (${(state.length / 1048576).toFixed(1)} MiB); serialize ${saveMs.toFixed(2)} ms, ` +
  `unserialize ${loadStateMs.toFixed(2)} ms; in-wasm slot save ${slotSaveMs.toFixed(2)} ms, load ${slotLoadMs.toFixed(2)} ms`);
console.log(`RAM ${hash(core.systemRam())}, state ${createHash("sha1").update(state).digest("hex").slice(0, 12)}`);
if (errors.length) console.log(`core said: ${[...new Set(errors)].join(" | ").slice(0, 400)}`);
