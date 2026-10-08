// The Flycast harness's emulator, in a worker as in the bar (web/emulator/worker.js): the core
// through the bar's libretro.js, drawing with WebGL2 on an OffscreenCanvas made here, one frame per
// task (WebGL fences only signal between tasks, so the read-back works as it does in the bar).
// The page (index.html) sends phases to run; this posts back timings, frames and sound levels.
import { Core } from "/web/emulator/libretro.js";
import createFlycast from "/flycast/dist/flycast.mjs";
import { hash, inputs } from "/flycast/inputs.mjs";

const B = 0, Y = 1, SELECT = 2, START = 3, UP = 4, DOWN = 5, LEFT = 6, RIGHT = 7, A = 8;
const bit = (id) => 1 << id;
const pulse = (i, at, length = 6) => i >= at && i < at + length;

/** Inputs for player 1 by frame, per script. B is SHOT1, A SHOT2, Select a coin. */
const SCRIPTS = {
  idle: () => 0,
  // Two coins, Start, then Start and SHOT1 now and then through the menus (court, player).
  "coin-start": (i) => {
    let mask = 0;
    if (pulse(i, 0) || pulse(i, 20)) mask |= bit(SELECT);
    if (i >= 60 && i % 90 < 6) mask |= bit(START);
    if (i >= 150 && i % 45 > 38) mask |= bit(B);
    if (i >= 300 && i % 200 < 20) mask |= bit(RIGHT);
    return mask;
  },
  // A rally: run left and right, up and down now and then, swing with SHOT1, lob with SHOT2.
  play: (i) => {
    let mask = (i >> 5) & 1 ? bit(LEFT) : bit(RIGHT);
    if ((i >> 7) % 3 === 1) mask |= bit(UP);
    if ((i >> 7) % 3 === 2) mask |= bit(DOWN);
    if (i % 20 < 4) mask |= bit(B);
    if (i % 150 > 145) mask |= bit(A);
    if (i % 400 < 4) mask |= bit(START);
    return mask;
  },
};

let core, module;
let lastFrame;
let framesDelivered = 0;
const sound = { frames: 0, peak: 0, sumSquares: 0, samples: 0 };
const post = (msg, transfer) => postMessage(msg, transfer ?? []);
const download = async (url) => {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
};
const channel = new MessageChannel();
const yieldTask = () => new Promise((r) => { channel.port1.onmessage = r; channel.port2.postMessage(0); });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function load({ rom, files, options, state, reset }) {
  let t = performance.now();
  const canvas = new OffscreenCanvas(1, 1);
  module = await createFlycast({ canvas });
  core = new Core(module, {
    onFrame(rgba, width, height) {
      lastFrame = { rgba, width, height };
      framesDelivered++;
    },
    onAudio(samples) {
      sound.frames += samples.length / 2;
      for (let i = 0; i < samples.length; i++) {
        const s = samples[i];
        if (Math.abs(s) > sound.peak) sound.peak = Math.abs(s);
        sound.sumSquares += s * s;
      }
      sound.samples += samples.length;
    },
    onLog(level, text) { if (level >= 2) post({ type: "log", text }); },
  });
  core.netplay = true;
  const createMs = performance.now() - t;
  t = performance.now();
  const [romBytes, ...extra] = await Promise.all([download(rom), ...files.map(download)]);
  const downloadMs = performance.now() - t;
  files.forEach((url, i) => core.addFile(url.slice(url.indexOf("/roms/") + 6), extra[i]));
  for (const [key, value] of Object.entries(options ?? {})) core.setOption(key, value);
  t = performance.now();
  const av = core.loadGame(rom.split("/").pop(), romBytes);
  const loadMs = performance.now() - t;
  let stateMs;
  if (state) {
    const bytes = await download(state);
    t = performance.now();
    core.unserialize(bytes);
    stateMs = performance.now() - t;
    // As the worker does before another machine's state: power-on, then the state.
    if (reset) {
      for (let i = 0; i < 60; i++) core.run();
      core.reset();
      core.unserialize(bytes);
    }
  }
  post({ type: "loaded", av, buttons: [...core.buttons], createMs, downloadMs, loadMs, stateMs, ramBytes: core.systemRam().length });
}

function stats(list) {
  const sorted = [...list].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
  return { avg: list.reduce((a, b) => a + b, 0) / list.length, p50: at(0.5), p95: at(0.95), max: sorted.at(-1) };
}

async function phase({ name, frames, script = "idle", pace = true, shots = [], fps }) {
  // "random": both players' inputs from inputs.mjs (check.mjs's), and the RAM hash every 60
  // frames, to compare with the same run in Node.
  const random = script === "random" ? inputs(frames, 1) : undefined;
  const hashes = [];
  const input = random ? (i) => random[i][0] : SCRIPTS[script];
  const period = 1000 / (fps ?? 59.79);
  const times = [];
  const start = performance.now();
  let delivered = framesDelivered;
  sound.frames = 0; sound.peak = 0; sound.sumSquares = 0; sound.samples = 0;
  module._flycast_stats(); // reset
  for (let i = 0; i < frames; i++) {
    if (pace) {
      const wait = start + i * period - performance.now();
      if (wait > 2) await sleep(wait - 1);
      else await yieldTask();
    } else {
      await yieldTask();
    }
    core.inputs[0] = input(i);
    if (random) core.inputs[1] = random[i][1];
    const t = performance.now();
    core.run();
    times.push(performance.now() - t);
    if (random && i % 60 === 59) hashes.push(hash(core.systemRam()));
    if (shots.includes(i) && lastFrame) {
      const copy = lastFrame.rgba.slice();
      post({ type: "shot", name: `${name}-${String(i).padStart(5, "0")}`, rgba: copy, width: lastFrame.width, height: lastFrame.height }, [copy.buffer]);
    }
  }
  const core_ = JSON.parse(module.UTF8ToString(module._flycast_stats()));
  const wallMs = performance.now() - start;
  const ram = core.systemRam();
  let sample = 2166136261;
  for (let i = 0; i < ram.length; i += 4096) sample = Math.imul(sample ^ ram[i], 16777619) >>> 0;
  post({
    type: "phase", name, frames, wallMs, run: stats(times), core: core_,
    delivered: framesDelivered - delivered,
    sound: { perFrame: sound.frames / frames, peak: sound.peak, rms: Math.sqrt(sound.sumSquares / Math.max(1, sound.samples)) },
    ramSample: sample.toString(16), hashes,
  });
}

onmessage = async ({ data }) => {
  try {
    if (data.type === "load") await load(data);
    else if (data.type === "phase") await phase(data);
    else if (data.type === "state") {
      const t0 = performance.now();
      const state = core.serialize();
      const t1 = performance.now();
      core.unserialize(state);
      const t2 = performance.now();
      post({ type: "state", bytes: state.length, serializeMs: t1 - t0, unserializeMs: t2 - t1 });
    }
  } catch (error) {
    post({ type: "error", error: String(error?.stack ?? error) });
  }
};
