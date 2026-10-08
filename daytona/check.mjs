// Checks the Daytona core in Node (the headless build), through web/emulator/libretro.js as the
// worker drives it:
//   smoke        always: the libretro surface (system and AV info, options, link status) and that
//                loading fails cleanly without the ROM set (no crash, an error logged).
//   determinism  two separate Node processes, the same scripted RetroPad inputs for 1200 frames:
//                identical main RAM and screen hashes every 120 frames.
//   savestate    600 frames, serialize; a fresh process resets, unserializes and runs 600 more:
//                the same hashes as the straight 1200-frame run.
//   drawing      the hidden cabinet rasterized too (draw_hidden) or not: the same RAM; and cabinet
//                0 shown again after 360 frames hidden: its first frame is the straight run's.
//   link         cabinets=2: after boot both comm boards report the link up, cabinet 1 of 2 and
//                2 of 2 (needs the twin presets in daytona/nvram/2/: make-nvram.mjs).
// All but the smoke test need the ROM set and the core built from it, and skip with a message
// otherwise.
//
//   node daytona/check.mjs [--core daytona/dist/headless/daytona.mjs] [--rom ~/Downloads/daytona.zip]
//
// Exit status 0 when everything that ran passed.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { deflateRawSync, crc32 } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";

const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const HERE = import.meta.dirname;
const CORE = resolve(option("--core", join(HERE, "dist/headless/daytona.mjs")));
const ROM = resolve(option("--rom", join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip")));
const FRAMES = Number(option("--frames", 1200));
const EVERY = 120;

/** FNV-1a over 32-bit words, as web/emulator/worker.js's hashRam. */
export function hash(bytes) {
  let h = 0x811c9dc5;
  const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.length >> 2);
  for (let i = 0; i < words.length; i++) h = Math.imul(h ^ words[i], 0x01000193);
  return (h >>> 0).toString(16).padStart(8, "0");
}

async function createModule() {
  const { default: createDaytona } = await import(CORE);
  return createDaytona({ canvas: { width: 1, height: 1 } }); // a canvas the core must ignore
}

function setOption(module, key, value) {
  const alloc = (text) => {
    const size = module.lengthBytesUTF8(text) + 1;
    const ptr = module._malloc(size);
    module.stringToUTF8(text, ptr, size);
    return ptr;
  };
  const k = alloc(key), v = alloc(value);
  module._daytona_set(k, v);
  module._free(k);
  module._free(v);
}

/** RetroPad masks for frame f, cabinet k: coins, start, throttle, steering and shifts. */
function scripted(f, k) {
  const B = 1 << 0, Y = 1 << 1, SELECT = 1 << 2, START = 1 << 3, UP = 1 << 4, DOWN = 1 << 5, LEFT = 1 << 6, RIGHT = 1 << 7, A = 1 << 8;
  let mask = 0;
  if (f % 300 >= 100 && f % 300 < 106) mask |= SELECT;
  if (f % 300 >= 160 && f % 300 < 166) mask |= START;
  if (f > 400) mask |= UP;
  if (f % 97 < 20) mask |= DOWN;
  if (f % 120 < 40 + 10 * k) mask |= LEFT;
  else if (f % 120 >= 70) mask |= RIGHT;
  if (f % 150 === 10) mask |= A;
  if (f % 210 === 15) mask |= B;
  if (f % 400 === 33) mask |= Y;
  return mask;
}

// A child process: loads the game, optionally a state, runs frames and prints hashes as JSON.
async function child() {
  const from = Number(option("--from", 0)), to = Number(option("--to", FRAMES));
  const module = await createModule();
  setOption(module, "cabinets", option("--cabinets", "2"));
  if (args.includes("--draw-hidden")) setOption(module, "draw_hidden", "1");
  const [hideFrom, hideTo] = (option("--hide", "-1:-1")).split(":").map(Number); // cabinet 1 shown in between
  let screen = "";
  let audio = 0x811c9dc5; // FNV-1a of the sound since the last mark
  const logs = [];
  const core = new Core(module, {
    onFrame(rgba) { screen = hash(rgba); },
    onAudio(samples) { for (const s of samples) audio = Math.imul(audio ^ (s & 0xffff), 0x01000193); },
    onLog(level, text) { logs.push(text); if (level >= 3) console.error(text); },
  });
  core.netplay = true;
  core.loadGame(basename(ROM), readFileSync(ROM));
  const load = option("--load");
  if (load) {
    core.reset(); // as the worker does before loading a state from another machine
    core.unserialize(readFileSync(load));
  }
  const hashes = {};
  for (let f = from; f < to; f++) {
    if (f === hideFrom) setOption(module, "view", "1");
    if (f === hideTo) setOption(module, "view", "0");
    core.inputs[0] = scripted(f, 0);
    core.inputs[1] = scripted(f, 1);
    core.run();
    if ((f + 1) % EVERY === 0) {
      hashes[f + 1] = { ram: hash(core.systemRam()), screen, audio: (audio >>> 0).toString(16) };
      audio = 0x811c9dc5;
    }
  }
  const save = option("--save");
  if (save) writeFileSync(save, core.serialize());
  const status = JSON.parse(module.UTF8ToString(module._daytona_link_status()));
  console.log(JSON.stringify({ hashes, status, errors: logs.filter((t) => /stopped|error/i.test(t)) }));
}

function runChild(extra) {
  const result = spawnSync(process.execPath, [import.meta.filename, "--child", "--core", CORE, "--rom", ROM, ...extra], {
    encoding: "utf8", maxBuffer: 1 << 26,
  });
  if (result.status !== 0) throw new Error(`child ${extra.join(" ")} failed:\n${result.stderr}`);
  return JSON.parse(result.stdout.trim().split("\n").at(-1));
}

let failures = 0;
const check = (ok, what, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
  if (!ok) failures++;
};
const skip = (what, why) => console.log(`skip ${what}: ${why}`);

/** A stored (uncompressed) zip of the given files, for the load-failure checks. */
function zip(files) {
  const locals = [], centrals = [];
  let offset = 0;
  for (const [name, data] of Object.entries(files)) {
    const bytes = Buffer.from(data), nameBytes = Buffer.from(name), crc = crc32(bytes);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(10, 4); local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(bytes.length, 18); local.writeUInt32LE(bytes.length, 22); local.writeUInt16LE(nameBytes.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(20, 4); central.writeUInt16LE(10, 6); central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(bytes.length, 20); central.writeUInt32LE(bytes.length, 24); central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, nameBytes, bytes);
    centrals.push(central, nameBytes);
    offset += 30 + nameBytes.length + bytes.length;
  }
  const dir = Buffer.concat(centrals), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(centrals.length / 2, 8); end.writeUInt16LE(centrals.length / 2, 10);
  end.writeUInt32LE(dir.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, dir, end]);
}

async function smoke() {
  const module = await createModule();
  const logs = [];
  const core = new Core(module, { onFrame() {}, onAudio() {}, onLog(level, text) { logs.push({ level, text }); } });

  // struct retro_system_info { const char *library_name, *library_version, *valid_extensions; bool need_fullpath, block_extract; }
  const info = module._malloc(16);
  module._retro_get_system_info(info);
  const name = module.UTF8ToString(module.getValue(info, "i32")), version = module.UTF8ToString(module.getValue(info + 4, "i32"));
  module._free(info);
  check(/Daytona/.test(name), "system info", `${name} ${version}`);
  const av = module._malloc(40);
  module._retro_get_system_av_info(av);
  const [w, h, maxW, maxH] = [0, 4, 8, 12].map((o) => module.getValue(av + o, "i32"));
  const aspect = module.getValue(av + 16, "float"), fps = module.getValue(av + 24, "double"), rate = module.getValue(av + 32, "double");
  module._free(av);
  check(w === 496 && h === 384 && maxW === 496 && maxH === 384, "AV geometry 496x384", `${w}x${h}, max ${maxW}x${maxH}`);
  check(Math.abs(aspect - 4 / 3) < 1e-6, "aspect 4:3", aspect.toFixed(6));
  check(Math.abs(fps - 16e6 / (656 * 424)) < 1e-9, "fps is the board's 57.524 Hz", fps.toFixed(6));
  check(rate === 48000, "sample rate 48000", String(rate));

  const loadFails = (label, file, bytes) => {
    logs.length = 0;
    let threw = false;
    try {
      if (bytes) core.loadGame(file, bytes);
      else {
        // No file at the path at all: retro_load_game straight, as the worker would on a bad path.
        const infoPtr = module._malloc(16);
        module.HEAPU32.fill(0, infoPtr >> 2, (infoPtr >> 2) + 4);
        const path = `/roms/${file}`, size = module.lengthBytesUTF8(path) + 1, p = module._malloc(size);
        module.stringToUTF8(path, p, size);
        module.setValue(infoPtr, p, "i32");
        threw = !module._retro_load_game(infoPtr);
        module._free(infoPtr);
      }
    } catch (e) {
      threw = /could not load/.test(e.message);
    }
    const error = logs.find((l) => l.level >= 3);
    check(threw && Boolean(error), `load fails cleanly: ${label}`, error ? error.text.split("\n")[0].slice(0, 140) : "no error logged");
  };
  loadFails("no ROM set", "daytona.zip");
  loadFails("not a zip", "daytona.zip", new TextEncoder().encode("not a zip at all"));
  loadFails("a zip of other files", "daytona.zip", zip({ "readme.txt": "hello", "epr-16722a.12": new Uint8Array(1024) }));

  // Nothing loaded: every entry point stays harmless.
  module._retro_run();
  module._retro_reset();
  check(module._retro_serialize_size() === 0 && module._retro_get_memory_data(2) === 0 && module._retro_get_memory_size(2) === 0,
    "no game: no state, no RAM, run and reset do nothing");
  logs.length = 0;
  setOption(module, "cabinets", "1");
  setOption(module, "view", "1");
  setOption(module, "steer_step", "12");
  setOption(module, "no_such_option", "1");
  check(logs.length === 1 && /no option/.test(logs[0].text), "options: known ones taken quietly, an unknown one warned about");
  const status = JSON.parse(module.UTF8ToString(module._daytona_link_status()));
  check(status.link.length === 0, "link status without a game", JSON.stringify(status));
  const timings = JSON.parse(module.UTF8ToString(module._daytona_timings()));
  check(timings.frames === 0, "timings without a game", JSON.stringify(timings));
  if (/stub/.test(version)) await blankMachine();
  return version;
}

// The stub build only: a two-cabinet machine on blank ROM images (test_blank_images), so all of
// the shim's machine runs without a ROM set: power-on, settings, the linked boards, the first
// frame stopping at the game code that is not there (caught, logged, not a crash), reset, states.
async function blankMachine() {
  const module = await createModule();
  const logs = [];
  const core = new Core(module, { onFrame() {}, onAudio() {}, onLog(level, text) { logs.push({ level, text }); } });
  setOption(module, "cabinets", "2");
  setOption(module, "test_blank_images", "1");
  module.FS.mkdirTree("/nvram/1");
  module.FS.writeFile("/nvram/1/ioboard_eeprom.bin", new Uint8Array(128).fill(0x5a));
  const av = core.loadGame("daytona.zip", new Uint8Array(4));
  check(av.width === 496 && Math.abs(av.fps - 57.524) < 0.001, "blank machine: loads (2 cabinets)", logs.map((l) => l.text).join(" | ").slice(0, 300));
  check(logs.some((l) => /Cabinet 2: settings from \/nvram\/1/.test(l.text)), "blank machine: cabinet 2's settings from /nvram/1/");
  const status = JSON.parse(module.UTF8ToString(module._daytona_link_status()));
  check(status.link.length === 2 && status.link.every((l) => l.state === "off"), "blank machine: two comm boards, not started", JSON.stringify(status));
  check(module._retro_get_memory_size(2) === 0x100000 && module._retro_get_memory_data(2) !== 0, "blank machine: 1 MB main RAM for the desync hash");
  const size = module._retro_serialize_size();
  let saved = true;
  try { core.serialize(); } catch { saved = false; }
  check(size > 0 && !saved && logs.some((l) => /no save states/.test(l.text)), `blank machine: no states without the snapshot patch (size ${size}), said so`);
  logs.length = 0;
  core.run();
  const stopped = logs.find((l) => l.level >= 3);
  check(Boolean(stopped && /no recompiled code/.test(stopped.text)), "blank machine: the first frame stops at the missing game code, caught", stopped?.text);
  logs.length = 0;
  core.run();
  check(logs.length === 0, "blank machine: stopped stays stopped");
  core.reset();
  core.run();
  check(logs.some((l) => /no recompiled code/.test(l.text)), "blank machine: reset powers on again");
  const path = "/out/1", p = module._malloc(16);
  module.stringToUTF8(path, p, 16);
  const ok = module._daytona_save_nvram(1, p);
  module._free(p);
  const eeprom = ok ? module.FS.readFile("/out/1/ioboard_eeprom.bin") : new Uint8Array();
  check(ok && eeprom.length === 128 && eeprom.every((b) => b === 0x5a), "blank machine: daytona_save_nvram writes cabinet 2's settings back");
  module._retro_unload_game();
}

if (args.includes("--child")) {
  await child();
} else {
  console.log(`core ${CORE}`);
  if (!existsSync(CORE)) {
    console.error(`no ${CORE}: run ./daytona/build.sh headless first`);
    process.exit(1);
  }
  const version = await smoke();
  const stub = /stub/.test(version);
  const blocked = !existsSync(ROM) ? `no ROM set at ${ROM} (ROMS=<dir> or --rom)`
    : stub ? "this core has the stub game code: rebuild with the ROM set (./daytona/build.sh)" : "";
  if (blocked) {
    for (const what of ["determinism", "savestate", "drawing", "link"]) skip(what, blocked);
  } else {
    const dir = mkdtempSync(join(tmpdir(), "daytona-check-"));
    const guarded = (what, fn) => { try { fn(); } catch (e) { check(false, what, e.message.split("\n").slice(0, 4).join(" ")); } };
    const same = (x, y) => x.ram === y?.ram && x.screen === y?.screen && x.audio === y?.audio;
    const show = (x, y) => `${x.ram}/${x.screen}/${x.audio}${same(x, y) ? "" : ` ≠ ${y?.ram}/${y?.screen}/${y?.audio}`}`;
    // determinism
    const a = runChild(["--to", String(FRAMES)]), b = runChild(["--to", String(FRAMES)]);
    const frames = Object.keys(a.hashes);
    check(frames.every((f) => same(a.hashes[f], b.hashes[f])) && frames.length === FRAMES / EVERY,
      `determinism: two processes, ${FRAMES} frames (RAM/screen/sound)`, frames.map((f) => `${f}:${show(a.hashes[f], b.hashes[f])}`).join(" "));
    if (a.errors.length) check(false, "no errors while running", a.errors.join("; "));
    // savestate
    guarded("savestate", () => {
      const half = FRAMES / 2, state = join(dir, "half.state");
      runChild(["--to", String(half), "--save", state]);
      const c = runChild(["--from", String(half), "--to", String(FRAMES), "--load", state]);
      const later = Object.keys(c.hashes);
      check(later.every((f) => same(c.hashes[f], a.hashes[f])) && later.length === half / EVERY,
        `savestate: ${half} frames, then ${half} more in a fresh process`, later.map((f) => `${f}:${show(c.hashes[f], a.hashes[f])}`).join(" "));
    });
    // drawing
    guarded("drawing", () => {
      const drawn = runChild(["--to", String(FRAMES), "--draw-hidden"]);
      check(frames.every((f) => drawn.hashes[f].ram === a.hashes[f].ram && drawn.hashes[f].screen === a.hashes[f].screen),
        "drawing: rasterizing the hidden cabinet too changes nothing", frames.map((f) => `${f}:${drawn.hashes[f].ram}`).join(" "));
      const hidden = runChild(["--to", String(FRAMES), "--hide", `${FRAMES / 2}:${FRAMES * 0.8 - 1}`]);
      const after = frames.filter((f) => f >= FRAMES * 0.8);
      check(frames.every((f) => hidden.hashes[f].ram === a.hashes[f].ram) && after.every((f) => hidden.hashes[f].screen === a.hashes[f].screen),
        `drawing: cabinet 1 hidden for frames ${FRAMES / 2}-${FRAMES * 0.8 - 1}, then its first frame shown is whole`,
        after.map((f) => `${f}:${hidden.hashes[f].screen === a.hashes[f].screen ? "same" : "differs"}`).join(" "));
    });
    // link
    if (!existsSync(join(HERE, "nvram/2/1"))) skip("link", "no twin presets in daytona/nvram/2/ (node daytona/make-nvram.mjs)");
    else {
      const linked = (status) => status.link.length === 2 && status.link.every((l, k) => l.state === "up" && l.id === k + 1 && l.count === 2);
      // Booting and the master's 4 s numbering wait take a while: a longer run if 1200 frames were not enough.
      guarded("link", () => {
        const status = linked(a.status) ? a.status : runChild(["--to", "3600"]).status;
        check(linked(status), "link: both cabinets up, 1 of 2 and 2 of 2", JSON.stringify(status));
      });
    }
  }
  console.log(failures ? `${failures} check(s) failed` : "all checks that ran passed");
  process.exit(failures ? 1 : 0);
}
