// Checks the Sega Rally core in Node (the headless build), through web/emulator/libretro.js as
// the worker drives it:
//   smoke        always: system and AV info, options, and that loading fails cleanly (an error
//                logged, no crash) for a file that is not a zip and for a zip of the wrong files.
//   boot         900 frames from power-on, the game's milestones logged (the recomp's status
//                lines and the mode words: boot, attract), then two coins, START and the
//                accelerator held: into a race (the race clock running), with frame times.
//   determinism  two instances (one with a different allocation history in its module), the
//                same inputs: the same i960 RAM hash every 60 frames, and the same sound.
//   savestate    a save at frame 600 (attract) and at 1900 (racing): run 300 frames, load, run the
//                300 again, in the same instance and in a fresh one (reset, then the state): the
//                same RAM hashes and the same sound each time.
// All but the smoke test need the ROM set (MAME's srallyc with the Revision B program, as
// srallycb.zip has it) and skip with a message otherwise.
//
//   node model2/srally/check.mjs [--core model2/srally/dist/headless/srally.mjs] [--rom ~/Downloads/srallycb.zip]
//
// Exit status 0 when everything that ran passed.
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { crc32, deflateRawSync } from "node:zlib";
import { Core } from "../../web/emulator/libretro.js";
import { RACE_SCRIPT, hash, modes, raceMask } from "./script.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const HERE = import.meta.dirname;
const CORE = resolve(option("--core", join(HERE, "dist/headless/srally.mjs")));
const ROM = resolve(option("--rom", join(process.env.ROMS ?? join(homedir(), "Downloads"), "srallycb.zip")));
const EVERY = 60;
let failures = 0;
const pass = (what) => console.log(`PASS ${what}`);
const fail = (what) => { failures++; console.log(`FAIL ${what}`); };
const check = (ok, what) => (ok ? pass(what) : fail(what));

const { default: createSrally } = await import(CORE);

/** A core through libretro.js; its log and the recomp's stderr lines collected. */
async function makeCore({ junk = 0, options = {} } = {}) {
  const log = [];
  const module = await createSrally({ printErr: (text) => log.push(`stderr: ${text}`), print: (text) => log.push(`stdout: ${text}`) });
  // A different allocation history in the module (strings, buffers): the machine must not care.
  for (let i = 0; i < junk; i++) module._malloc(1000 + 37 * i);
  const set = (key, value) => {
    const alloc = (text) => { const n = module.lengthBytesUTF8(text) + 1; const p = module._malloc(n); module.stringToUTF8(text, p, n); return p; };
    module._srally_set(alloc(key), alloc(String(value)));
  };
  for (const [k, v] of Object.entries(options)) set(k, v);
  const audio = { frames: 0, hash: 0x811c9dc5, loudest: 0 };
  const core = new Core(module, {
    onFrame() {},
    onAudio(samples) {
      audio.frames += samples.length / 2;
      for (let i = 0; i < samples.length; i++) {
        audio.hash = Math.imul(audio.hash ^ (samples[i] & 0xffff), 0x01000193);
        if (Math.abs(samples[i]) > audio.loudest) audio.loudest = Math.abs(samples[i]);
      }
    },
    onLog(level, text) { log.push(`core ${level}: ${text}`); },
  });
  return { module, core, log, audio, set };
}

/** A zip of the given { name: bytes } (stored), for the loading tests. */
function zip(files) {
  const locals = [], centrals = [];
  let offset = 0;
  for (const [name, data] of Object.entries(files)) {
    const n = Buffer.from(name), d = Buffer.from(data), c = crc32(d);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(20, 4); local.writeUInt32LE(c, 14);
    local.writeUInt32LE(d.length, 18); local.writeUInt32LE(d.length, 22); local.writeUInt16LE(n.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(20, 6); central.writeUInt32LE(c, 16);
    central.writeUInt32LE(d.length, 20); central.writeUInt32LE(d.length, 24); central.writeUInt16LE(n.length, 28); central.writeUInt32LE(offset, 42);
    locals.push(local, n, d);
    centrals.push(central, n);
    offset += 30 + n.length + d.length;
  }
  const dir = Buffer.concat(centrals), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(centrals.length / 2, 8); end.writeUInt16LE(centrals.length / 2, 10);
  end.writeUInt32LE(dir.length, 12); end.writeUInt32LE(offset, 16);
  return new Uint8Array(Buffer.concat([...locals, dir, end]));
}

// ---- smoke ----
{
  const { module, core, log } = await makeCore();
  const info = module._malloc(20);
  module._retro_get_system_info(info);
  const name = module.UTF8ToString(module.getValue(info, "i32")), version = module.UTF8ToString(module.getValue(info + 4, "i32"));
  console.log(`system: ${name}, ${version}`);
  check(/Sega Rally/.test(name), "system info names the game");
  const av = module._malloc(40);
  module._retro_get_system_av_info(av);
  const width = module.getValue(av, "i32"), height = module.getValue(av + 4, "i32");
  const fps = module.getValue(av + 24, "double"), rate = module.getValue(av + 32, "double");
  console.log(`av: ${width}x${height}, ${fps.toFixed(6)} Hz, ${rate} Hz`);
  check(width === 496 && height === 384 && Math.abs(fps - 57.52416) < 1e-4 && rate === 44100, "AV info: 496x384, 57.524 Hz, 44100 Hz");
  check(module._retro_serialize_size() === 0, "no state before a game");
  core.run(); // harmless with nothing loaded
  let threw = false;
  try { core.loadGame("srallyc.zip", new Uint8Array([1, 2, 3, 4])); } catch { threw = true; }
  check(threw && log.some((l) => /not a zip/.test(l)), "a file that is not a zip: load fails, logged");
  threw = false;
  try { core.loadGame("srallyc.zip", zip({ "epr-17888b.12": new Uint8Array(16) })); } catch { threw = true; }
  check(threw && log.some((l) => /srallyc set|missing ROM|checksum/.test(l)), "a zip of the wrong files: load fails, logged");
  const before = log.length;
  const { set } = { set: (k, v) => { const a = (t) => { const n = module.lengthBytesUTF8(t) + 1; const p = module._malloc(n); module.stringToUTF8(t, p, n); return p; }; module._srally_set(a(k), a(v)); } };
  set("no_such_option", "1");
  set("region", "mars");
  check(log.slice(before).filter((l) => /no option|region:/.test(l)).length === 2, "unknown options and values are logged");
  const t = JSON.parse(module.UTF8ToString(module._srally_timings()));
  check(["frames", "logic", "geometry", "raster", "sound", "audio", "total"].every((k) => k in t), "srally_timings() JSON");
}

if (!existsSync(ROM)) {
  console.log(`SKIP the rest: no ROM set at ${ROM} (--rom <zip>)`);
  process.exit(failures ? 1 : 0);
}
const romBytes = new Uint8Array(readFileSync(ROM));

// ---- boot and race ----
{
  const { module, core, log, audio } = await makeCore({ options: { log: "status" } });
  let t = performance.now();
  core.loadGame("srallyc.zip", romBytes);
  console.log(`loaded in ${(performance.now() - t).toFixed(0)} ms; state ${module._retro_serialize_size()} bytes`);
  let last = "", raceAt = -1, attractAt = -1;
  const times = [], raceTimes = [];
  for (let f = 0; f < RACE_SCRIPT.until; f++) {
    core.inputs[0] = raceMask(f);
    t = performance.now();
    core.run();
    const dt = performance.now() - t;
    times.push(dt);
    const m = modes(core.systemRam());
    if (m.race) raceTimes.push(dt);
    const now = JSON.stringify(m);
    if (now !== last) { console.log(`  frame ${f + 1}: mode ${m.mode} inner ${m.inner} scene ${m.scene} race ${m.race}`); last = now; }
    if (m.mode === 2 && attractAt < 0) attractAt = f + 1;
    if (m.race && raceAt < 0) raceAt = f + 1;
    if (f + 1 === 900) {
      for (const line of log.filter((l) => /^stderr: (lift|model2_hw)/.test(l))) console.log(`  ${line.slice(8)}`);
      check(attractAt > 0, `boot: the attract mode by frame 900 (frame ${attractAt})`);
    }
  }
  check(raceAt > 0, `coins, START, accelerator: the race clock runs (from frame ${raceAt})`);
  check(audio.loudest > 1000, `sound: ${(audio.frames / RACE_SCRIPT.until).toFixed(2)} samples a frame, loudest ${audio.loudest}`);
  const stats = (list) => {
    const s = [...list].sort((a, b) => a - b);
    return `${(list.reduce((a, b) => a + b, 0) / list.length).toFixed(2)} avg, ${s[Math.floor(s.length * 0.95)].toFixed(2)} p95, ${s.at(-1).toFixed(2)} worst ms`;
  };
  console.log(`  frame times: all ${times.length} frames ${stats(times.slice(1))} (the first, the boot, ${times[0].toFixed(1)} ms); racing ${raceTimes.length} frames ${stats(raceTimes)}`);
}

async function scripted(core, audio, from, to, hashes) {
  for (let f = from; f < to; f++) {
    core.inputs[0] = raceMask(f);
    core.run();
    if ((f + 1) % EVERY === 0) hashes.push(`${f + 1}:${hash(core.systemRam())}:${(audio.hash >>> 0).toString(16)}`);
  }
}
// ---- unload and load again in the same module: as a fresh one ----
{
  const a = await makeCore(), b = await makeCore();
  a.core.loadGame("srallyc.zip", romBytes);
  await scripted(a.core, a.audio, 0, 1200, []);
  a.module._retro_unload_game();
  a.core.loadGame("srallyc.zip", romBytes);
  a.audio.hash = 0x811c9dc5;
  const again = [], fresh = [];
  await scripted(a.core, a.audio, 0, 1200, again);
  b.core.loadGame("srallyc.zip", romBytes);
  await scripted(b.core, b.audio, 0, 1200, fresh);
  check(again.every((h, i) => h === fresh[i]), `a second load in the same module runs as a fresh one (${again.at(-1)})`);
}

// ---- determinism ----
{
  const a = await makeCore(), b = await makeCore({ junk: 300, options: { steer_step: "12" } });
  a.core.loadGame("srallyc.zip", romBytes);
  b.core.loadGame("srallyc.zip", romBytes);
  const ha = [], hb = [];
  for (let f = 0; f < RACE_SCRIPT.until; f += EVERY) {
    await scripted(a.core, a.audio, f, f + EVERY, ha);
    await scripted(b.core, b.audio, f, f + EVERY, hb);
  }
  const first = ha.findIndex((h, i) => h !== hb[i]);
  check(first < 0, first < 0 ? `determinism: two instances, ${RACE_SCRIPT.until} frames, ${ha.length} RAM and sound hashes the same (last ${ha.at(-1)})`
    : `determinism: differ from ${ha[first]} / ${hb[first]}`);
}

// ---- save states ----
for (const at of [600, 1900]) {
  const a = await makeCore();
  a.core.loadGame("srallyc.zip", romBytes);
  await scripted(a.core, a.audio, 0, at, []);
  let t = performance.now();
  const state = a.core.serialize();
  const saveMs = performance.now() - t;
  const audioAt = a.audio.hash;
  const straight = [];
  await scripted(a.core, a.audio, at, at + 300, straight);
  // The same instance, back to the save.
  t = performance.now();
  a.core.unserialize(state);
  const loadMs = performance.now() - t;
  a.audio.hash = audioAt;
  const again = [];
  await scripted(a.core, a.audio, at, at + 300, again);
  // A fresh instance with another history: reset, then the state.
  const b = await makeCore({ junk: 123 });
  b.core.loadGame("srallyc.zip", romBytes);
  await scripted(b.core, b.audio, 0, 37, []);
  b.core.reset();
  b.core.unserialize(state);
  b.audio.hash = audioAt;
  const fresh = [];
  await scripted(b.core, b.audio, at, at + 300, fresh);
  // A module that has not run a frame yet (a watcher joining): the game's coroutine is new to it.
  const c = await makeCore();
  c.core.loadGame("srallyc.zip", romBytes);
  c.core.unserialize(state);
  c.audio.hash = audioAt;
  const cold = [];
  await scripted(c.core, c.audio, at, at + 300, cold);
  const same = (x) => x.length === straight.length && x.every((h, i) => h === straight[i]);
  const say = (x) => (same(x) ? "identical" : "DIFFERENT");
  check(same(again) && same(fresh) && same(cold), `save state at frame ${at} (${state.length} bytes, save ${saveMs.toFixed(1)} ms, load ${loadMs.toFixed(1)} ms): ` +
    `300 frames after a load ${say(again)} in the same instance, ${say(fresh)} in a fresh one, ${say(cold)} in one that never ran (RAM and sound)`);
  if (!same(fresh)) console.log(`  straight ${straight.slice(0, 2)} fresh ${fresh.slice(0, 2)}`);
}

console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
