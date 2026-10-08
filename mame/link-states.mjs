// Makes Time Crisis II's start-up states for the bar (web/emulator/worker.js, linked.js) and
// checks them:
//
// - timecrs2.state: one board, link unplugged, booted and past its power-on test, with COINS
//   coins in (a credit is 4 coins: the game goes to its mode select at once). A player alone
//   starts from it, as at any other cabinet.
// - timecrs2-link0.state, timecrs2-link1.state: the two boards of the twin cabinet, Left/Red
//   and Right/Blue, booted linked to each other from power-on (mame/README.md "Linked
//   cabinets", with a delay of DELAY ticks), linked up at the NAMCO screen, with COINS coins
//   in each, both saved after the same tick. Each file is linked.js's container: the board's
//   state and the link bytes it still had to receive (what the other board transmitted in the
//   last DELAY ticks), which the worker hands it at the session's first ticks.
//
// All three are deflated as the worker packs states ("vabz"). Then the check: fresh boards,
// loaded unlinked and run a little as the bar's worker has them, plugged in, reset and given
// the states (worker.js #linkUp) go on linked through LinkedSession (linked.js), with the same
// delay as the states were made with (and must then play on exactly as the boards that saved
// them: the same link bytes and RAM, tick by tick) and with each delay in DELAYS, where both
// players shoot to pick LINK PLAY and must get to linked gameplay (the game's mode word 2).
//
//   node mame/link-states.mjs ~/Downloads/timecrs2.zip [out dir, default mame/.cache/roms]
//   make upload-rom ROM=mame/.cache/roms/timecrs2-link0.state   (and the other two; local R2)
//
// Env: CORE (default mame/dist/mame.mjs: the states only load into the core build that made
// them), COIN_AT (tick the coins go in, default 1300: linked boards link up at ~950 and show
// the GASHIN logo by 1300), COINS (default 16: 4 credits), SAVE_AT (default 40 ticks after the
// last coin), DELAY (the link's delay the states are made with, default 2: a session's delay
// is never less, linked.js), SAME (ticks of the exact replay check, default 300), DELAYS
// (default 2,4,8,12; empty: no game check), GAME (ticks each game check runs, default 900),
// SHOT (path prefix: PNGs of the boards at the save and at the end of each game check).
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { deflateRawSync, deflateSync, inflateRawSync } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";
import { LinkedSession, makeLinkState, parseLinkState } from "../web/emulator/linked.js";

const [romPath, outDir = new URL(".cache/roms", import.meta.url).pathname] = process.argv.slice(2);
if (!romPath) {
  console.error("usage: node mame/link-states.mjs <timecrs2.zip> [out dir]");
  process.exit(2);
}
const CORE = process.env.CORE ?? new URL("dist/mame.mjs", import.meta.url).pathname;
const COIN_AT = Number(process.env.COIN_AT ?? 1300);
const COINS = Number(process.env.COINS ?? 16);
const SAVE_AT = Number(process.env.SAVE_AT ?? COIN_AT + COINS * 10 + 40);
const DELAY = Number(process.env.DELAY ?? 2);
const SAME = Number(process.env.SAME ?? 300);
const DELAYS = (process.env.DELAYS ?? "2,4,8,12").split(",").filter(Boolean).map(Number);
const GAME = Number(process.env.GAME ?? 900);
const SHOT = process.env.SHOT;

const { default: createMAME } = await import(resolve(CORE));
const rom = readFileSync(romPath);
const sha = (bytes) => createHash("sha1").update(bytes).digest("hex").slice(0, 12);
/** The gun aimed at the middle of the screen, and these buttons (RetroPad bits). */
const MIDDLE = (128 << 16) | (128 << 24);
const SELECT = 1 << 2; // a coin
const TRIGGER = 1 << 0; // B

/** A board: the core loaded, linked on `side` from power-on, or unplugged (side undefined). */
async function board(name, side) {
  const b = { name, frame: undefined };
  b.core = new Core(await createMAME(), {
    onFrame(rgba, width, height) { b.frame = { rgba, width, height }; },
    onAudio() {},
  });
  b.core.gun = true;
  b.core.netplay = true;
  if (side !== undefined) b.core.linkSet(true, side);
  b.core.loadGame(basename(romPath), rom);
  return b;
}

/** A board's machine for LinkedSession: its input on port 0, its packets straight to `to`. */
function machine(b, to) {
  b.out = [];
  return {
    run([input]) {
      b.core.inputs.fill(0);
      b.core.inputs[0] = input;
      b.core.run();
    },
    send(packets) {
      for (const [, packet] of packets) {
        // Each tick's bytes, in order (the session's hello, its "tick" 0xffffffff, aside).
        if (new DataView(packet.buffer, packet.byteOffset).getUint32(0, true) !== 0xffffffff) b.out.push(packet.slice(4));
        to().session.receive(b.session.local, packet);
      }
    },
  };
}

/** Links two boards with LinkedSessions of `delay`, each handed `carried` bytes first. */
function link(a, b, delay, carried = [[], []]) {
  for (const [x, other, local] of [[a, b, 0], [b, a, 1]]) {
    x.session = new LinkedSession({
      link: { incoming: (bytes) => x.core.linkIncoming(bytes), outgoing: () => x.core.linkOutgoing() },
      local,
      delay,
      carried: carried[local],
    });
    x.machine = machine(x, () => other);
  }
}

/** One tick on both boards, each with its input. */
function tick(boards, inputs) {
  boards.forEach((x, i) => {
    if (!x.session.advance(inputs[i], x.machine)) throw new Error(`${x.name} had to wait at tick ${x.session.currentFrame()}`);
  });
}

/** Coins for the tick `t`: `COINS` of them from COIN_AT, 5 ticks down and 5 up each. */
const coins = (t) => (t >= COIN_AT && t < COIN_AT + COINS * 10 && (t - COIN_AT) % 10 < 5 ? SELECT : 0);

const pack = (state) => {
  const packed = deflateRawSync(state, { level: 9 });
  const out = new Uint8Array(4 + packed.length);
  out.set([0x76, 0x61, 0x62, 0x7a]); // "vabz", as worker.js packs
  out.set(packed, 4);
  return out;
};

mkdirSync(outDir, { recursive: true });
let started = performance.now();

// The two boards, from power-on to the save.
const red = await board("red", 0);
const blue = await board("blue", 1);
link(red, blue, DELAY);
for (let t = 0; t < SAVE_AT; t++) {
  tick([red, blue], [MIDDLE | coins(t), MIDDLE | coins(t)]);
  if ((t + 1) % 300 === 0) console.log(`linked boards: tick ${t + 1}, ${status(red)} | ${status(blue)}`);
}
const saved = [red, blue].map((x) => x.core.serialize());
const carried = [blue.out.slice(-DELAY), red.out.slice(-DELAY)]; // what each still had to receive
shot(red, "save-red");
shot(blue, "save-blue");
for (const [side, x] of [[0, red], [1, blue]]) {
  const file = join(outDir, `timecrs2-link${side}.state`);
  const bytes = makeLinkState({ side, carried: carried[side], state: pack(saved[side]) });
  writeFileSync(file, bytes);
  console.log(`${file}: ${(bytes.length / 1e6).toFixed(1)} MB (state ${(saved[side].length / 1e6).toFixed(1)} MB, ` +
    `${carried[side].map((c) => c.length).join("+")} bytes in flight), saved after tick ${SAVE_AT - 1}: ${status(x)}`);
}
console.log(`made in ${((performance.now() - started) / 1000).toFixed(0)} s`);

// The board alone.
started = performance.now();
const solo = await board("solo");
for (let t = 0; t < SAVE_AT; t++) {
  solo.core.inputs[0] = MIDDLE | coins(t);
  solo.core.run();
}
shot(solo, "save-solo");
{
  const file = join(outDir, "timecrs2.state");
  const bytes = pack(solo.core.serialize());
  writeFileSync(file, bytes);
  console.log(`${file}: ${(bytes.length / 1e6).toFixed(1)} MB, saved after tick ${SAVE_AT - 1} (${((performance.now() - started) / 1000).toFixed(0)} s)`);
}

let ok = true;

/** Fresh boards from the files, as the bar's worker starts them (worker.js #linkUp). */
async function fromFiles(delay) {
  const boards = [];
  for (const side of [0, 1]) {
    const file = parseLinkState(readFileSync(join(outDir, `timecrs2-link${side}.state`)));
    const x = await board(side ? "blue'" : "red'");
    // Unplugged at first, as a player alone: a few frames of the solo game.
    x.core.unserialize(inflate(readFileSync(join(outDir, "timecrs2.state"))));
    for (let i = 0; i < 30; i++) x.core.run();
    x.core.linkSet(true, side);
    x.core.reset();
    x.core.unserialize(inflate(file.state));
    x.carried = file.carried;
    boards.push(x);
  }
  link(boards[0], boards[1], delay, [boards[0].carried, boards[1].carried]);
  return boards;
}

// The same delay as the states were made with: exactly the boards that saved them.
if (SAME) {
  started = performance.now();
  const [red2, blue2] = await fromFiles(DELAY);
  let firstDiff;
  for (let t = 0; t < SAME && !firstDiff; t++) {
    const input = MIDDLE | (t % 120 < 6 ? TRIGGER : 0);
    tick([red, blue], [input, input]);
    tick([red2, blue2], [input, input]);
    for (const [x, y] of [[red, red2], [blue, blue2]]) {
      const sameOut = Buffer.compare(Buffer.from(x.out.at(-1)), Buffer.from(y.out.at(-1))) === 0;
      const sameRam = sha(x.core.systemRam()) === sha(y.core.systemRam());
      if (!sameOut || !sameRam) firstDiff = `${x.name} at tick ${t}: ${sameOut ? "" : "link bytes differ "}${sameRam ? "" : "RAM differs"}`;
    }
  }
  ok &&= !firstDiff;
  console.log(`from the files, delay ${DELAY}, ${SAME} ticks: ` +
    (firstDiff ? `DIFFERENT from the boards that saved them (${firstDiff})` : "the same link bytes and RAM as the boards that saved them, tick by tick") +
    ` (${((performance.now() - started) / 1000).toFixed(0)} s)`);
}

// Each delay a session may have: both players shoot (LINK PLAY, then on), and the boards get
// to a linked game. The red player shoots a little before the blue one.
for (const delay of DELAYS) {
  started = performance.now();
  const boards = await fromFiles(delay);
  let gameAt;
  for (let t = 0; t < GAME; t++) {
    const shoot = (from) => (t >= from && (t - from) % 60 < 6 && t < from + 400 ? TRIGGER : 0);
    tick(boards, [MIDDLE | shoot(60), MIDDLE | shoot(75)]);
    const [a, b] = boards.map((x) => x.core.linkStatus());
    if (gameAt === undefined && a.mode === 2 && b.mode === 2) gameAt = t;
  }
  const [a, b] = boards.map((x) => x.core.linkStatus());
  const good = a.mode === 2 && b.mode === 2 && a.keepalive >= 2 && b.keepalive >= 2;
  ok &&= good;
  boards.forEach((x) => shot(x, `delay${delay}-${x.name.replace("'", "")}`));
  console.log(`from the files, delay ${delay}: ${good ? `LINKED GAMEPLAY on both boards from tick ${gameAt}` : "NOT in a linked game"} ` +
    `after ${GAME} ticks; ${status(boards[0])} | ${status(boards[1])} (${((performance.now() - started) / 1000).toFixed(0)} s)`);
}

console.log(ok ? "states made and checked" : "states made, check FAILED");
process.exit(ok ? 0 : 1);

/** A state as `pack` left it, inflated. */
function inflate(bytes) {
  const marked = bytes[0] === 0x76 && bytes[1] === 0x61 && bytes[2] === 0x62 && bytes[3] === 0x7a;
  return marked ? new Uint8Array(inflateRawSync(bytes.subarray(4))) : bytes;
}

function status(x) {
  const s = x.core.linkStatus();
  return `${x.name}: linked ${s.linked} mode ${s.mode} keepalive ${s.keepalive} counter ${s.counter} sent ${s.txFrames}/${s.txBytes}B got ${s.rxFrames}/${s.rxBytes}B`;
}

function shot(x, name) {
  if (!SHOT || !x.frame) return;
  writeFileSync(`${SHOT}-${name}.png`, png(x.frame));
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
