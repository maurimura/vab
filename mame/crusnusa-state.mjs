// Makes Cruis'n USA's start-up state for the bar (web/emulator/worker.js loads /roms/<rom>.state
// right after the game) and checks it.
//
// A first power-on asks to calibrate the controls (MAME's NVRAM starts empty): "CALIBRATE
// CONTROLS", wheel centred, all the way left, all the way right, the gas pedal down, each
// confirmed with the cabinet's "Enter" test switch. The bar has no key for that switch (it is
// MAME's F2 key, and the bar sends RetroPad buttons only), so this script does it: the wheel and
// the gas pedal moved with the d-pad as a player moves them (MAME's key ramp, mame/README.md
// "Inputs"), Enter pressed through the core's libretro keyboard callback. Then the game boots
// (textures, the Midway logo), COINS coins go in (3 credits start a game at the default
// coinage, 1 coin each) and the machine is saved in its attract mode: the player presses Start.
// The calibration is in the state (the NVRAM is), so players never see it. The file is deflated
// as the worker packs states ("vabz": 12.1 MiB of state, a 3 MB file), which it unpacks.
//
// Then the check: a fresh machine loads the state, as the worker does (power-on, then the state),
// and both machines run the same input (Start, then the gas, the wheel and the brake) for CHECK
// frames: the same RAM at every 60-frame checkpoint and the same whole state at the end.
//
//   node mame/crusnusa-state.mjs ~/crusnusa41.zip [out, default mame/dist/crusnusa41.state]
//   make upload-rom ROM=mame/dist/crusnusa41.state
//
// Env: CORE (default mame/dist/mame.mjs: a state loads only into the core build that made it,
// so make it again after rebuilding), COINS (default 12: 4 games), CHECK (frames, default 1200),
// SHOT (a .png of the frame the state was saved at).
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { deflateRawSync, deflateSync, inflateRawSync } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";

const [romPath, outPath = new URL("dist/crusnusa41.state", import.meta.url).pathname] = process.argv.slice(2);
if (!romPath) {
  console.error("usage: node mame/crusnusa-state.mjs <crusnusa41.zip> [out.state]");
  process.exit(2);
}
const CORE = resolve(process.env.CORE ?? new URL("dist/mame.mjs", import.meta.url).pathname);
const COINS = Number(process.env.COINS ?? 12);
const CHECK = Number(process.env.CHECK ?? 1200);
const rom = readFileSync(romPath);
const { default: createMAME } = await import(CORE);

// RetroPad bits (libretro.h ids): the bar's arrows, coin (5) and Start (1).
const UP = 1 << 4;
const DOWN = 1 << 5;
const LEFT = 1 << 6;
const RIGHT = 1 << 7;
const COIN = 1 << 2;
const START = 1 << 3;
const RETROK_F2 = 283; // the cabinet's Enter (test) switch, PORT_CODE(KEYCODE_F2)
const SET_KEYBOARD_CALLBACK = 12;

/**
 * The core, and a way to press its keys: the core hands the frontend a keyboard callback
 * (struct retro_keyboard_callback, a wasm function pointer), called here through the module's
 * function table, which this instantiation keeps.
 */
async function machine() {
  let table;
  const wasm = readFileSync(CORE.replace(/\.mjs$/, ".wasm"));
  const module = await createMAME({
    instantiateWasm(imports, done) {
      WebAssembly.instantiate(wasm, imports).then(({ instance, module: compiled }) => {
        table = Object.values(instance.exports).find((e) => e instanceof WebAssembly.Table);
        done(instance, compiled);
      });
      return {};
    },
  });
  let keyboard = 0;
  const addFunction = module.addFunction;
  let first = true;
  module.addFunction = (fn, signature) => {
    if (signature === "iii" && first) {
      first = false; // the environment callback (libretro.js registers it first)
      const environment = fn;
      fn = (cmd, data) => {
        if (cmd !== SET_KEYBOARD_CALLBACK) return environment(cmd, data);
        keyboard = module.getValue(data, "i32");
        return 1;
      };
    }
    return addFunction(fn, signature);
  };
  const m = { frame: undefined, frames: 0 };
  m.core = new Core(module, {
    onFrame(rgba, width, height) { m.frame = { rgba, width, height }; },
    onAudio() {},
  });
  m.core.netplay = true; // as the worker has it
  m.core.loadGame(basename(romPath), rom);
  m.key = (code, down) => table.get(keyboard)(down ? 1 : 0, code, 0, 0);
  m.ram = () => createHash("sha1").update(m.core.systemRam()).digest("hex").slice(0, 12);
  return m;
}

/** Runs `frames` frames holding `pad`; with `enter` the Enter switch is pressed for the last 6. */
function run(m, frames, pad = 0, enter = false) {
  for (let i = 0; i < frames; i++) {
    if (enter && i === frames - 6) m.key(RETROK_F2, true);
    m.core.inputs[0] = pad;
    m.core.run();
    m.frames++;
  }
  if (enter) m.key(RETROK_F2, false);
}

/** The calibration screens: plain grey under their two lines of text. */
function calibrating({ rgba, width, height }) {
  for (let y = 120; y < height - 20; y += 16) {
    for (let x = 8; x < width - 8; x += 16) {
      const i = (y * width + x) * 4;
      const [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]];
      if (r !== g || g !== b || r < 64 || r > 192) return false;
    }
  }
  return true;
}

const a = await machine();
// Boot to the calibration (~16 s: the power-on tests, the DCS sound board's).
while (!(a.frame && calibrating(a.frame))) {
  run(a, 30);
  if (a.frames > 3000) throw new Error("No calibration screen after 3000 frames: is the NVRAM already set?");
}
console.log(`calibration from frame ${a.frames}`);
run(a, 60);
run(a, 60, 0, true); // "set steering wheel to center then hit enter"
run(a, 120, LEFT, true); // "turn wheel all the way to the left and hit enter"
run(a, 60);
run(a, 120, RIGHT, true); // "... to the right ..."
run(a, 60);
run(a, 120, UP, true); // "push gas pedal to max and hit enter"
run(a, 120);
if (calibrating(a.frame)) throw new Error("Still calibrating after the four steps");
// The boot goes on: textures, the Midway logo, the attract mode.
run(a, 1500);
for (let i = 0; i < COINS; i++) {
  run(a, 5, COIN);
  run(a, 10);
}
run(a, 60);
const state = a.core.serialize();
const packed = deflateRawSync(state, { level: 9 });
const file = new Uint8Array(4 + packed.length);
file.set([0x76, 0x61, 0x62, 0x7a]); // "vabz", as worker.js packs
file.set(packed, 4);
writeFileSync(outPath, file);
if (!inflateRawSync(file.subarray(4)).equals(Buffer.from(state))) throw new Error("the packed state doesn't unpack to the state");
console.log(`${outPath}: ${file.length} bytes (a ${state.length}-byte state), saved at frame ${a.frames} with ${COINS} coins in`);
if (process.env.SHOT) {
  writeFileSync(process.env.SHOT, png(a.frame));
  console.log(`the frame it was saved at: ${process.env.SHOT}`);
}

// The check: a fresh machine at power-on loads the state, then both play the same.
const b = await machine();
b.core.reset();
b.core.unserialize(state);
const play = (f) => (f < 10 ? START : f < 300 ? 0 : f < 900 ? UP | ((f >> 6) & 1 ? LEFT : RIGHT) : DOWN);
let same = true;
for (let f = 0; f < CHECK; f++) {
  for (const m of [a, b]) {
    m.core.inputs[0] = play(f);
    m.core.run();
  }
  if (f % 60 === 59 && a.ram() !== b.ram()) {
    console.log(`frame ${f}: RAM differs (${a.ram()} / ${b.ram()})`);
    same = false;
    break;
  }
}
const whole = (m) => createHash("sha1").update(m.core.serialize()).digest("hex");
const wholeSame = whole(a) === whole(b);
console.log(`a fresh machine from the state, ${CHECK} frames of Start, gas, wheel and brake: RAM ${same ? "same" : "DIFFERENT"}, whole state ${wholeSame ? "same" : "different"}`);
if (!same || !wholeSame) process.exit(1);

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
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
  }
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}
