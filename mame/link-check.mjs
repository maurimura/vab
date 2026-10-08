// Two Time Crisis II boards linked back to back in one process, in lockstep, the way two
// browsers run them (mame/README.md, "Linked cabinets"): at every tick of the link clock each
// board is handed what the other transmitted DELAY ticks earlier (mame_link_incoming), runs one
// frame, and gives up what it transmitted (mame_link_outgoing). Checks the plumbing and the
// boards: bytes out of one go into the other, each byte is delivered exactly once and DELAY
// ticks later, nothing crashes, and each board is deterministic across a save state (fresh
// instances that load both boards' states and go on linked to each other, fed the same bytes,
// transmit the same bytes and keep the same RAM).
//
//   node mame/link-check.mjs ~/Downloads/timecrs2.zip
//
// Env: FRAMES (link clock ticks, default 2400), DELAY (ticks, default 2), STAGGER (board B
// powers on this many ticks after A, default 0), DRC=1/0 (core option mame_drc, both boards),
// COIN (tick at which both players insert 4 coins, a credit at the default coinage, then pull
// the trigger 90 and 150 ticks later: LINK PLAY at the mode select; from 240 ticks on they
// shoot now and then, sweeping the aim, and step on the pedal; default off), SHOT (path prefix: PNGs of both boards' last frames, and with
// SHOTS=640,900 also at those ticks),
// CHECK (ticks the save-state check replays, default 300; 0 off) from CHECK_AT (default the
// middle of the run),
// EVERY (ticks between status lines, default 60), CORE (default mame/dist/mame.mjs).
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { deflateSync } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";
import { optionsFromEnv, withOptions } from "./core-options.mjs";

const [romPath, ...extraPaths] = process.argv.slice(2);
if (!romPath) {
  console.error("usage: node mame/link-check.mjs <timecrs2.zip> [more files]");
  process.exit(2);
}
const FRAMES = Number(process.env.FRAMES ?? 2400);
const DELAY = Number(process.env.DELAY ?? 2);
const STAGGER = Number(process.env.STAGGER ?? 0);
const COIN = process.env.COIN ? Number(process.env.COIN) : undefined;
const CHECK = Number(process.env.CHECK ?? 300);
const EVERY = Number(process.env.EVERY ?? 60);
const SHOTS = new Set((process.env.SHOTS ?? "").split(",").filter(Boolean).map(Number));
const CORE = process.env.CORE ?? new URL("dist/mame.mjs", import.meta.url).pathname;
if (!(DELAY >= 1)) throw new Error("DELAY must be at least 1: a board can't wait for the other's current frame");

const { default: createMAME } = await import(resolve(CORE));
const options = optionsFromEnv();
const rom = readFileSync(romPath);
const extras = extraPaths.map((path) => [basename(path), readFileSync(path)]);
const sha = (bytes) => createHash("sha1").update(bytes).digest("hex").slice(0, 12);
const STATUS = ["linked", "side", "txFrames", "rxFrames", "txBytes", "rxBytes", "mode", "keepalive", "counter", "pending", "patches", "dsw"];

/** One board: a core instance linked on `side` (0 Left/Red, 1 Right/Blue). */
async function board(name, side) {
  const m = withOptions(await createMAME(), options);
  const b = { name, side, m, frame: undefined, errors: [] };
  b.core = new Core(m, {
    onFrame(rgba, width, height) { b.frame = { rgba, width, height }; },
    onAudio() {},
    onLog(level, text) { if (level >= 3) b.errors.push(text); },
  });
  b.core.gun = true;
  b.core.inputs[0] = (128 << 16) | (128 << 24); // aimed at the middle
  m._mame_link_set(1, side); // before the first frame: the cable is in from power-on
  for (const [file, bytes] of extras) b.core.addFile(file, bytes);
  b.core.loadGame(basename(romPath), rom);
  b.buffer = m._malloc(1 << 20);
  b.statusPtr = m._malloc(4 * STATUS.length);
  return b;
}

function feed(b, bytes) {
  if (!bytes.length) return;
  if (bytes.length > 1 << 20) throw new Error("incoming link data too big for the buffer");
  b.m.HEAPU8.set(bytes, b.buffer);
  b.m._mame_link_incoming(b.buffer, bytes.length);
}

function take(b) {
  const n = b.m._mame_link_outgoing(b.buffer, 1 << 20);
  if (n > 1 << 20) throw new Error(`${n} bytes of outgoing link data: more than the buffer`);
  return b.m.HEAPU8.slice(b.buffer, b.buffer + n);
}

function status(b) {
  const n = b.m._mame_link_status(b.statusPtr, STATUS.length);
  const words = new Uint32Array(b.m.HEAPU8.buffer, b.statusPtr, n);
  return Object.fromEntries(STATUS.slice(0, n).map((key, i) => [key, words[i]]));
}

/** Frames ([u16 BE size][bytes] each) in a chunk of link bytes. */
function framesIn(bytes) {
  let count = 0;
  for (let at = 0; at + 2 <= bytes.length; count++) at += 2 + ((bytes[at] << 8) | bytes[at + 1]);
  return count;
}

/** Inputs for a tick: 4 coins, the trigger twice (COIN), then some play. */
function input(t, side) {
  let mask = 0;
  let x = 128;
  let y = 128;
  if (COIN !== undefined) {
    const at = t - COIN;
    if (at >= 0 && at < 40 && at % 10 < 5) mask |= 1 << 2; // SELECT: a coin, 4 times
    if ((at >= 90 && at < 96) || (at >= 150 && at < 156)) mask |= 1 << 0; // B: the gun's trigger
    if (at >= 240) {
      if (((at + 16 * side) >> 3) % 4 === 0) mask |= 1 << 0; // a shot now and then
      if ((at >> 6) % 3 !== 0) mask |= 1 << 8; // A: the pedal, most of the time
      x = 128 + Math.round(90 * Math.sin((at + 50 * side) / 41));
      y = 128 + Math.round(60 * Math.cos((at + 30 * side) / 29));
    }
  }
  return (mask | (x << 16) | (y << 24)) >>> 0;
}

let started = performance.now();
const a = await board("A (Left/Red)", 0);
const b = await board("B (Right/Blue)", 1);
console.log(`two boards loaded in ${((performance.now() - started) / 1000).toFixed(1)} s; DELAY ${DELAY}, STAGGER ${STAGGER}, ` +
  `options ${JSON.stringify(options)}`);
b.start = STAGGER;
a.start = 0;
a.peer = b;
b.peer = a;
for (const x of [a, b]) {
  x.out = []; // out[t]: what the board transmitted at link clock tick t
  x.fedBytes = 0;
  x.fedFrames = 0;
  x.ram = []; // RAM hash after each tick, for the save-state check
  x.ms = 0;
}

// The check's starting point: both boards' states between two ticks.
const checkAt = CHECK ? Number(process.env.CHECK_AT ?? Math.floor(FRAMES / 2)) : -1;
const saved = {};

/** One tick of the link clock: each board gets what the other sent DELAY ticks ago, then runs. */
function tick(boards, t, outOf, record = true) {
  for (const x of boards) {
    if (t < x.start) {
      if (record) x.out[t] = new Uint8Array(0);
      continue;
    }
    const incoming = outOf(x.peer, t - DELAY);
    feed(x, incoming);
    if (record) {
      x.fedBytes += incoming.length;
      x.fedFrames += framesIn(incoming);
    }
    x.core.inputs[0] = input(t - x.start, x.side);
    const t0 = performance.now();
    x.core.run();
    x.ms += performance.now() - t0;
    const out = take(x);
    if (record) x.out[t] = out;
    x.lastOut = out;
  }
}
const recorded = (peer, t) => (t >= 0 && peer.out[t]) || new Uint8Array(0);

let firstTraffic;
for (let t = 0; t < FRAMES; t++) {
  if (t === checkAt) {
    for (const x of [a, b]) saved[x.name] = x.core.serialize();
  }
  tick([a, b], t, recorded);
  for (const x of [a, b]) {
    if (t >= checkAt && checkAt >= 0 && t < checkAt + CHECK) x.ram[t] = sha(x.core.systemRam());
    if (firstTraffic === undefined && x.out[t].length) firstTraffic = t;
  }
  if (process.env.SHOT && SHOTS.has(t + 1)) {
    for (const x of [a, b]) if (x.frame) writeFileSync(`${process.env.SHOT}-${t + 1}-${x.name[0].toLowerCase()}.png`, png(x.frame));
  }
  if ((t + 1) % EVERY === 0 || t === FRAMES - 1) {
    const line = [a, b].map((x) => {
      const s = status(x);
      return `${x.name[0]} sent ${s.txFrames}/${s.txBytes}B got ${s.rxFrames}/${s.rxBytes}B mode ${s.mode} keepalive ${s.keepalive} ` +
        `counter ${s.counter} patches ${s.patches.toString(16)}`;
    });
    console.log(`t ${String(t + 1).padStart(5)}: ${line.join(" | ")}`);
  }
}

// The plumbing: every frame a board sent reached the other once, DELAY ticks later.
let ok = true;
for (const x of [a, b]) {
  const s = status(x);
  const peer = x.peer;
  const sentBytes = peer.out.slice(0, Math.max(0, FRAMES - DELAY)).reduce((n, out) => n + out.length, 0);
  const sentFrames = peer.out.slice(0, Math.max(0, FRAMES - DELAY)).reduce((n, out) => n + framesIn(out), 0);
  const good = s.rxBytes === sentBytes && s.rxFrames === sentFrames && x.fedBytes === sentBytes;
  ok &&= good;
  console.log(`${x.name}: received ${s.rxFrames} frames / ${s.rxBytes} bytes, the other sent ${sentFrames} / ${sentBytes} ` +
    `up to ${DELAY} ticks before the end: ${good ? "all, once" : "MISMATCH"}; ${(x.ms / FRAMES).toFixed(2)} ms per frame; ` +
    `${x.errors.length ? `errors: ${x.errors.slice(0, 3).join(" | ")}` : "no errors"}`);
}
console.log(firstTraffic === undefined ? "no link traffic at all" : `first link traffic at tick ${firstTraffic}`);
{
  const [sa, sb] = [status(a), status(b)];
  const dip = (s) => ((s.dsw & 0x08) ? "off" : "on");
  console.log(`link DIP ${dip(sa)}/${dip(sb)}; the PR's code patches 0x${sa.patches.toString(16)}/0x${sb.patches.toString(16)} ` +
    "(0x55 = all four poked, 0xaa = all refused); " +
    (sa.mode === 2 && sb.mode === 2 ? "LINKED GAMEPLAY on both boards (mode word 2)" : `mode words ${sa.mode}/${sb.mode} (2 = linked gameplay staged)`));
}

// Determinism across a save state: fresh boards load the states saved at checkAt and go on
// linked to each other; for the first DELAY ticks they are fed what the originals sent before
// checkAt (the frontend would replay it), then each other's live output.
if (checkAt >= 0) {
  const a2 = await board("A (Left/Red)", 0);
  const b2 = await board("B (Right/Blue)", 1);
  a2.start = a.start;
  b2.start = b.start;
  a2.peer = b2;
  b2.peer = a2;
  for (const [x, orig] of [[a2, a], [b2, b]]) {
    x.core.unserialize(saved[orig.name]);
    x.out = [];
  }
  let same = true;
  let firstDiff;
  for (let t = checkAt; t < Math.min(FRAMES, checkAt + CHECK); t++) {
    tick([a2, b2], t, (peer, at) => (at < checkAt ? recorded(peer === a2 ? a : b, at) : recorded(peer, at)));
    for (const [x, orig] of [[a2, a], [b2, b]]) {
      const sameOut = Buffer.compare(Buffer.from(x.out[t]), Buffer.from(orig.out[t])) === 0;
      const sameRam = sha(x.core.systemRam()) === orig.ram[t];
      if ((!sameOut || !sameRam) && same) {
        same = false;
        firstDiff = `${x.name} at tick ${t}: ${sameOut ? "" : "link bytes differ "}${sameRam ? "" : "RAM differs"}`;
      }
    }
  }
  ok &&= same;
  console.log(`save state at tick ${checkAt}, fresh boards linked to each other for ${CHECK} ticks: ` +
    (same ? "same link bytes and RAM, tick by tick" : `DIFFERENT (${firstDiff})`));
}

if (process.env.SHOT) {
  for (const x of [a, b]) {
    if (!x.frame) continue;
    const path = `${process.env.SHOT}-${x.name[0].toLowerCase()}.png`;
    writeFileSync(path, png(x.frame));
    console.log(`${x.name}: last frame in ${path}`);
  }
}
console.log(ok ? "link check passed" : "link check FAILED");
process.exit(ok ? 0 : 1);

/** A frame as a PNG (RGBA, no filtering). */
function png({ rgba, width, height }) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (bytes) => {
    let c = 0xffffffff;
    for (const byte of bytes) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
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
  header[8] = 8;
  header[9] = 6;
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
  }
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}
