// Times the MAME core (mame/build.sh) in Node, through the bar's own frontend (libretro.js): how
// long a frame takes, how big and slow a save state is, and whether the game draws and sounds.
// MAME renders in software, so this is the whole frame the worker would run.
//
//   node mame/bench.mjs ~/Downloads/tekken3je1.zip
//   GUN=1 node mame/bench.mjs ~/Downloads/timecrs2.zip   (more files after it: BIOS / device ROMs)
//   STATE=mame/dist/crusnusa41.state COIN=0 WARMUP=1500 node mame/bench.mjs ~/crusnusa41.zip
//
// Env: FRAMES (timed frames, default 600), WARMUP (frames run first, default 1200: System 12
// boots slowly), CORE (default mame/dist/mame.mjs), SHOT (a .png path for the last frame), GUN=1
// for a lightgun game (the frontend then answers the core's lightgun: here, aimed at the middle),
// DRC=1 / DRC=0 the R4650's recompiler on / off (core option mame_drc; unset: the core's default),
// COIN=frame: 4 coins at that frame of the warm-up, then the trigger every few frames (Time
// Crisis II: a game starts, so the timed frames are gameplay; with GUN=1). A driving game (the
// core calls RetroPad Up "Accelerate": Cruis'n USA) is driven instead: Start 60 frames after the
// coins, taps of the gas to pick the race, the transmission and the car, then the gas held from
// 700 frames after the coins with a touch of the wheel now and then. STATE=path: a start-up state
// loaded after power-on, as the worker does (Cruis'n USA's: a fresh machine stops at its controls
// calibration, mame/crusnusa-state.mjs).
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { loadavg } from "node:os";
import { basename, resolve } from "node:path";
import { deflateSync, inflateRawSync } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";
import { optionsFromEnv, withOptions } from "./core-options.mjs";

const [romPath, ...extraPaths] = process.argv.slice(2);
if (!romPath) {
  console.error("usage: node mame/bench.mjs <rom.zip> [bios-or-device.zip ...]");
  process.exit(2);
}
const FRAMES = Number(process.env.FRAMES ?? 600);
const WARMUP = Number(process.env.WARMUP ?? 1200);
const CORE = process.env.CORE ?? new URL("dist/mame.mjs", import.meta.url).pathname;

const { default: createMAME } = await import(resolve(CORE));
const options = optionsFromEnv();
let started = performance.now();
const module = withOptions(await createMAME(), options);

let frame; // the last frame drawn: { rgba, width, height }
let framesDrawn = 0;
let audioFrames = 0;
let loudest = 0;
const errors = [];
const core = new Core(module, {
  onFrame(rgba, width, height) {
    frame = { rgba, width, height };
    framesDrawn++;
  },
  onAudio(samples) {
    audioFrames += samples.length / 2;
    for (const s of samples) if (Math.abs(s) > loudest) loudest = Math.abs(s);
  },
  onLog(level, text) {
    if (level >= 2) errors.push(text);
  },
});
const createMs = performance.now() - started;
core.gun = process.env.GUN === "1";
if (core.gun) core.inputs[0] = (128 << 16) | (128 << 24);
for (const path of extraPaths) core.addFile(basename(path), readFileSync(path));
started = performance.now();
let av;
try {
  av = core.loadGame(basename(romPath), readFileSync(romPath));
} catch (error) {
  console.error(`${error.message}\n${errors.join("\n")}`);
  process.exit(1);
}
const loadMs = performance.now() - started;
if (process.env.STATE) {
  // Start-up states may be deflated, as the worker packs them ("vabz").
  const bytes = readFileSync(process.env.STATE);
  core.reset();
  core.unserialize(bytes.subarray(0, 4).toString() === "vabz" ? inflateRawSync(bytes.subarray(4)) : bytes);
  console.log(`from ${process.env.STATE}`);
}
console.log(`options: ${JSON.stringify(options)} (mame_drc unset: the core's default)`);
console.log(`core ${(createMs / 1000).toFixed(2)} s, ${basename(romPath)} loaded in ${(loadMs / 1000).toFixed(2)} s: ` +
  `${av.width}x${av.height} @ ${av.fps.toFixed(3)} Hz, ${av.sampleRate} Hz audio, aspect ${av.aspectRatio.toFixed(3)}`);
if (errors.length) console.log(`core said: ${errors.join(" | ").slice(0, 600)}`);
console.log(`buttons: ${[...core.buttons].map(([id, name]) => `${id}=${name}`).join(", ") || "(none)"}`);

const ramHash = () => createHash("sha1").update(core.systemRam()).digest("hex").slice(0, 12);
const ramBytes = core.systemRam().length; // (a view into wasm memory: re-read it each time)
console.log(`system RAM: ${ramBytes} bytes${ramBytes ? `, hash ${ramHash()}` : " (none!)"}`);

const nonBlack = (f) => {
  if (!f) return false;
  const pixels = new Uint32Array(f.rgba.buffer, f.rgba.byteOffset, f.width * f.height);
  for (let i = 0; i < pixels.length; i++) if (pixels[i] & 0xffffff) return true;
  return false;
};

const COIN = process.env.COIN === undefined ? undefined : Number(process.env.COIN);
const baseInput = core.inputs[0];
const driving = core.buttons.get(4) === "Accelerate";
/** Frame f's input: the coins and the trigger, or a drive (COIN), otherwise as set above. */
const play = (f) => {
  if (COIN === undefined) return;
  const at = f - COIN;
  let mask = 0;
  if (at >= 0 && at < 40 && at % 10 < 5) mask |= 1 << 2; // SELECT: a coin, 4 times
  if (!driving) {
    if (at >= 90 && ((at >> 3) & 3) === 0) mask |= 1 << 0; // B: the trigger now and then
  } else if (at >= 60 && at < 70) {
    mask |= 1 << 3; // Start
  } else if (at >= 120 && at < 700) {
    if (at % 60 < 20) mask |= 1 << 4; // taps of the gas: the race, the transmission, the car
  } else if (at >= 700) {
    mask |= 1 << 4; // the gas held
    if (at % 128 < 12) mask |= 1 << 6; // and a touch of the wheel, left
    else if (at % 128 >= 64 && at % 128 < 76) mask |= 1 << 7; // and right
  }
  core.inputs[0] = (baseInput | mask) >>> 0;
};
started = performance.now();
for (let i = 0; i < WARMUP; i++) {
  play(i);
  core.run();
}
const warmupMs = performance.now() - started;
const ramAfterWarmup = ramBytes ? ramHash() : "";

const times = [];
const cpuTimes = []; // this process's CPU time per frame: steadier than wall time on a busy machine
let shown = 0;
let lit = 0;
const sizes = new Set();
audioFrames = 0;
loudest = 0;
for (let i = 0; i < FRAMES; i++) {
  const before = framesDrawn;
  play(WARMUP + i);
  const cpu = process.cpuUsage();
  const start = performance.now();
  core.run();
  times.push(performance.now() - start);
  const used = process.cpuUsage(cpu);
  cpuTimes.push((used.user + used.system) / 1000);
  if (framesDrawn > before) {
    shown++;
    sizes.add(`${frame.width}x${frame.height}`);
    if (nonBlack(frame)) lit++;
  }
}
const stats = (list) => {
  const sorted = [...list].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2);
  const avg = (list.reduce((a, b) => a + b, 0) / list.length).toFixed(2);
  return `${avg} ms average, ${at(0.5)} p50, ${at(0.95)} p95, ${sorted.at(-1).toFixed(2)} worst`;
};
console.log(`warm-up: ${WARMUP} frames in ${(warmupMs / 1000).toFixed(1)} s (${(warmupMs / Math.max(1, WARMUP)).toFixed(2)} ms/frame)`);
console.log(`frames: ${FRAMES}, wall ${stats(times)} (budget ${(1000 / av.fps).toFixed(2)} ms)`);
console.log(`        CPU ${stats(cpuTimes)}; load average ${loadavg().map((l) => l.toFixed(1)).join(" ")}`);
console.log(`video: ${shown} of ${FRAMES} frames drawn (${[...sizes].join(", ") || "none"}), ${lit} not black`);
console.log(`audio: ${(audioFrames / FRAMES).toFixed(1)} sample frames per frame, loudest ${loudest} of 32767`);
if (ramBytes) console.log(`system RAM changed during the timed frames: ${ramHash() !== ramAfterWarmup}`);

// Save states: through JavaScript (serialize) and within wasm memory (slots, as rollback does).
const state = core.serialize();
const time = (fn, n = 10) => {
  const t = performance.now();
  for (let i = 0; i < n; i++) fn();
  return (performance.now() - t) / n;
};
const saveMs = time(() => core.serialize());
const loadStateMs = time(() => core.unserialize(state));
core.allocSlots(2);
const slotSaveMs = time(() => core.saveSlot(0));
const slotLoadMs = time(() => core.loadSlot(0));
console.log(`state: ${state.length} bytes (${(state.length / 1048576).toFixed(1)} MiB); serialize ${saveMs.toFixed(2)} ms, ` +
  `unserialize ${loadStateMs.toFixed(2)} ms; in-wasm slot save ${slotSaveMs.toFixed(2)} ms, load ${slotLoadMs.toFixed(2)} ms`);

// Same instance, same input: a state, 120 frames, back to the state, 120 frames again. Then a
// fresh instance (another player's machine, e.g. one joining a game in progress) loads the state
// and runs the same 120 frames: a recompiler's code cache, warm in the first, starts cold there.
if (ramBytes) {
  const sha = (bytes) => createHash("sha1").update(bytes).digest("hex");
  core.unserialize(state);
  for (let i = 0; i < 120; i++) core.run();
  const first = ramHash();
  const firstState = sha(core.serialize());
  core.unserialize(state);
  for (let i = 0; i < 120; i++) core.run();
  const again = ramHash();
  const againState = sha(core.serialize());
  console.log(`replay from a state: RAM ${first === again ? "same" : "DIFFERENT"}, whole state ${firstState === againState ? "same" : "different"}`);
  if (process.env.FRESH !== "0") {
    const other = new Core(withOptions(await createMAME(), options), { onFrame() {}, onAudio() {} });
    other.gun = core.gun;
    other.inputs.set(core.inputs);
    for (const path of extraPaths) other.addFile(basename(path), readFileSync(path));
    other.loadGame(basename(romPath), readFileSync(romPath));
    other.unserialize(state);
    for (let i = 0; i < 120; i++) other.run();
    const otherRam = createHash("sha1").update(other.systemRam()).digest("hex").slice(0, 12);
    console.log(`a fresh instance from the state: RAM ${otherRam === first ? "same" : "DIFFERENT"}, ` +
      `whole state ${sha(other.serialize()) === firstState ? "same" : "different"}`);
  }
}

if (process.env.SHOT && frame) {
  writeFileSync(process.env.SHOT, png(frame));
  console.log(`last frame: ${process.env.SHOT}`);
}

/** A frame as a PNG (RGBA, no filtering). */
function png({ rgba, width, height }) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (bytes) => {
    let c = 0xffffffff;
    for (const b of bytes) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    out.write(type, 4, "ascii");
    data.copy(out, 8);
    out.writeUInt32BE(crc(out.subarray(4, 8 + data.length)), 8 + data.length);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
  }
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}
