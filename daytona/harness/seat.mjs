// One seat of Daytona USA's arcade mode in Node: a machine of one cabinet on a star
// (cabinets=1, link_topology=star, seat=k: the shim's class Bridge), driven frame by frame, with
// the link exports (daytona_link_out / _in / _absent) and the worker's state packing. Shared by
// make-states.mjs (the seat states) and harness/arcade-check.mjs (seats in separate processes).
import { readFileSync } from "node:fs";
import { crc32, deflateSync } from "node:zlib";
import { Core } from "../../web/emulator/libretro.js";

/** A cabinet's own link block, bytes (the shim's kSlot; daytona_link_block_size()). */
export const BLOCK = 0x1c0;
/** RetroPad bits by name, as the shim reads them (README "Inputs"). */
export const BUTTONS = { b: 0, vr1: 1, coin: 2, start: 3, up: 4, down: 5, left: 6, right: 7, a: 8, vr2: 9, vr3: 10, vr4: 11 };

/** Marks a deflated state: "vabz", then the deflate-raw bytes (web/emulator/worker.js). */
const PACKED = Uint8Array.of(0x76, 0x61, 0x62, 0x7a);

/** worker.js's pack(), as it is: "vabz" + deflate-raw through CompressionStream. */
export async function pack(state) {
  const stream = new Blob([state]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const packed = new Uint8Array(await new Response(stream).arrayBuffer());
  const out = new Uint8Array(PACKED.length + packed.length);
  out.set(PACKED);
  out.set(packed, PACKED.length);
  return out;
}

/** worker.js's unpack(): bytes not marked as packed are taken as they are. */
export async function unpack(bytes) {
  if (bytes.length < PACKED.length || PACKED.some((b, i) => bytes[i] !== b)) return bytes;
  const stream = new Blob([bytes.subarray(PACKED.length)]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** FNV-1a over 32-bit words, as worker.js's RAM hash and check.mjs. */
export function hash(bytes) {
  let h = 0x811c9dc5;
  const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.length >> 2);
  for (let i = 0; i < words.length; i++) h = Math.imul(h ^ words[i], 0x01000193);
  return (h >>> 0).toString(16).padStart(8, "0");
}

/** An RGBA picture as a PNG file's bytes. */
export function png(rgba, width, height) {
  const chunk = (type, data) => {
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    out.write(type, 4, "latin1");
    data.copy(out, 8);
    out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  const rows = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(rows, y * (width * 4 + 1) + 1);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}

/** One cabinet of the star, in this process. */
export class Seat {
  /** core: the headless daytona.mjs; rom: daytona.zip; options: more daytona_set before loading. */
  static async create({ core, rom, seat, options = {}, onLog }) {
    const { default: createDaytona } = await import(core);
    return new Seat(await createDaytona(), { rom, seat, options, onLog });
  }

  constructor(module, { rom, seat, options, onLog = () => {} }) {
    this.m = module;
    this.seat = seat;
    this.frame = 0; // retro_runs since the state was loaded
    this.logs = [];
    this.core = new Core(module, {
      onFrame: (rgba, width, height) => { this.picture = { rgba, width, height }; },
      onAudio() {},
      onLog: (level, text) => { this.logs.push({ level, text }); onLog(level, text); },
    });
    this.core.present = false; // nothing rasterized but the frames asked for
    this.set("cabinets", 1);
    this.set("link_topology", "star");
    this.set("seat", seat);
    for (const [key, value] of Object.entries(options)) this.set(key, value);
    this.core.loadGame("daytona.zip", readFileSync(rom));
    this.buffer = module._malloc(BLOCK);
  }

  set(key, value) { this.core.setOption(key, String(value)); }
  status() { return JSON.parse(this.m.UTF8ToString(this.m._daytona_link_status())); }
  /** "up 1 of 8" for the cabinet. */
  line() { const l = this.status().link[0]; return `${l.state} ${l.id ?? "-"} of ${l.count ?? "-"}`; }

  /** The worker's way in: reset, then the (packed or plain) whole-machine state. */
  async load(bytes) {
    this.core.reset();
    this.core.unserialize(await unpack(bytes));
    this.frame = 0;
  }
  async save() { return pack(this.core.serialize()); }

  /** daytona_link_out: { changed, block } (the block is a copy). */
  out() {
    const changed = this.m._daytona_link_out(this.buffer) !== 0;
    return { changed, block: this.m.HEAPU8.slice(this.buffer, this.buffer + BLOCK) };
  }
  /** daytona_link_in: `seat`'s latest block. */
  in(seat, block) {
    this.m.HEAPU8.set(block, this.buffer);
    this.m._daytona_link_in(seat, this.buffer, block.length);
  }
  absent(seat) { this.m._daytona_link_absent(seat); }

  /** One frame with this RetroPad mask on port 0; `draw`: rasterize it (this.picture). */
  run(pad = 0, draw = false) {
    this.core.inputs[0] = pad;
    this.core.present = draw;
    this.picture = undefined;
    this.core.run();
    this.frame++;
  }
  ram() { return hash(this.core.systemRam()); }
  screen() { return this.picture ? hash(this.picture.rgba) : ""; }
  pngBytes() { return this.picture ? png(this.picture.rgba, this.picture.width, this.picture.height) : undefined; }
}
