// Experiments on Daytona USA's own link play as a ring of up to eight cabinets in one machine
// (the shim's link_* options and daytona_cabinet_* exports), headless in Node: the findings are
// in daytona/ring-notes.md. Needs the ROM set, the headless core built from it, and for more than
// two cabinets the ring presets (make-nvram.mjs make --only 8/ --out DIR).
//
//   node daytona/harness/ring-lab.mjs <command> [--out=DIR] [--nvram=DIR] [--cabinets=8] ...
//
//   form      power on, log the link until every cabinet is up, shoot every cabinet, save the
//             whole machine as the base state (--save=F, default 2400: <out>/base-<N>.state).
//   bench     from the base state: ms a frame with one cabinet viewed (--frames=600) and the
//             core's timings per cabinet.
//   run       a scenario: --base[=FILE] (from the base state) or from power-on, --set=k=v,...
//             (daytona_set before loading, e.g. link_delay=6), --events="..." (below), --frames=N,
//             --every=P with --shots=k,k,... (periodic screenshots), --tag=name.
//   sweep     the run scenario from the base state once per --values=a,b,...: {v} in --events is
//             the value, --shotAt=T;T;... the times to shoot --shots (e.g. the join window).
//   capture   power on, run to --at=F, save cabinet --cab=K's state to --file (a "preset state").
//   protocol  from the base state: --frames of traffic (attract, or --events for a race), bytes per
//             frame per cabinet, the shift-register check, how much of each block changes, and
//             how old each received block is (frames since its cabinet sent it).
//
// Events (comma separated; t in frames from the start of the scenario):
//   K:button@t[+n]   hold a button on cabinet K for n frames (default 6): start, up (accelerator),
//                    down (brake), left, right, coin, a/b (shift up/down), vr1-vr4
//   cut=K;K@t, pause=K@t, ghost=K@t, cut=@t   set link_cut / link_pause / link_ghost (empty: none)
//   reset:K@t        power on cabinet K alone        save:K@t   keep cabinet K's state (and a file)
//   load:K@t[=file]  load cabinet K from the kept state (or a file)
//   shots=K;K@t      screenshots of those cabinets (one frame each, in a row)   status@t   print the link
//   base@t=file      save the whole machine (a base state for later --base=file runs)
//   set:key=value@t  any daytona_set option (a list value, e.g. link_blank, separates items with ";",
//                    as --set does)
//
// --out (default <tmpdir>/daytona-ring) gets the PNGs (<tag>-c<K>-f<frame>.png, frame = retro_runs
// since power-on) and states. --core and --rom as check.mjs (ROMS env var for the ROM's directory).
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { crc32, deflateSync } from "node:zlib";
import { Core } from "../../web/emulator/libretro.js";

const HERE = import.meta.dirname;
const [command, ...rest] = process.argv.slice(2);
const opt = Object.fromEntries(rest.map((a) => { const m = /^--([^=]+)(?:=(.*))?$/s.exec(a); return m ? [m[1], m[2] ?? "1"] : [a, "1"]; }));
const CORE = resolve(opt.core ?? join(HERE, "../dist/headless/daytona.mjs"));
const ROM = resolve(opt.rom ?? join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip"));
const OUT = resolve(opt.out ?? join(tmpdir(), "daytona-ring"));
const N = Number(opt.cabinets ?? 8);
const NVRAM = opt.nvram ? resolve(opt.nvram) : process.env.RING_NVRAM;
mkdirSync(OUT, { recursive: true });

const BUTTONS = { b: 0, vr1: 1, coin: 2, start: 3, up: 4, down: 5, left: 6, right: 7, a: 8, vr2: 9, vr3: 10, vr4: 11 };

function png(rgba, width, height) {
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

/** One machine of N linked cabinets, driven frame by frame. */
class Ring {
  static async create(settings = {}) {
    const { default: createDaytona } = await import(CORE);
    return new Ring(await createDaytona(), settings);
  }

  constructor(module, settings) {
    this.m = module;
    this.frame = 0; // retro_runs since power-on (or as the base state says)
    this.holds = [];
    this.kept = new Map(); // cabinet -> its saved state (save:K)
    this.core = new Core(module, {
      onFrame: (rgba, width, height) => { this.picture = { rgba, width, height }; },
      onAudio() {},
      onLog: (level, text) => { if (level >= 2) console.error(`  [${this.frame}] ${text.trim()}`); },
    });
    this.core.inputs = new Uint16Array(8);
    this.core.present = false; // nothing rasterized but the screenshots
    this.set("cabinets", N);
    const presets = NVRAM && join(NVRAM, String(N));
    if (N > 2 && !(presets && existsSync(presets))) throw new Error(`no ring presets: --nvram=DIR with DIR/${N}/<k>/ (make-nvram.mjs make --only ${N}/ --out DIR)`);
    if (presets && existsSync(presets)) {
      for (const k of readdirSync(presets)) {
        this.m.FS.mkdirTree(`/ring-nvram/${k}`);
        for (const file of ["ioboard_eeprom.bin", "backup_ram.bin"]) this.m.FS.writeFile(`/ring-nvram/${k}/${file}`, readFileSync(join(presets, k, file)));
      }
      this.set("nvram_dir", "/ring-nvram");
    }
    for (const [key, value] of Object.entries(settings)) this.set(key, value);
    this.core.loadGame("daytona.zip", readFileSync(ROM));
  }

  set(key, value) { this.core.setOption(key, String(value)); }
  json(name) { return JSON.parse(this.m.UTF8ToString(this.m[name]())); }
  status() { return this.json("_daytona_link_status"); }
  stats() { return this.json("_daytona_link_stats"); }
  timings() { return this.json("_daytona_timings"); }
  /** "up 1/8" per cabinet, compact: U1/8 W0/0 L1/8 off, plus a mode when not run. */
  line() {
    return this.status().link.map((l) => `${l.state[0].toUpperCase()}${l.id ?? ""}/${l.count ?? ""}${l.mode ? `(${l.mode})` : ""}`).join(" ");
  }

  hold(k, mask, frames) { this.holds.push({ k, mask, until: this.frame + frames }); }
  step(n = 1) {
    for (let i = 0; i < n; i++) {
      this.core.inputs.fill(0);
      for (const h of this.holds) this.core.inputs[h.k] |= h.mask;
      this.core.run();
      this.frame++;
      this.holds = this.holds.filter((h) => h.until > this.frame);
    }
  }
  /** Runs one frame with cabinet k shown and writes its screen; returns the file. */
  shoot(k, tag) {
    this.set("view", k);
    this.core.present = true;
    this.picture = undefined;
    this.step(1);
    this.core.present = false;
    const file = join(OUT, `${tag}-c${k}-f${this.frame}.png`);
    if (this.picture) writeFileSync(file, png(this.picture.rgba, this.picture.width, this.picture.height));
    return file;
  }

  saveCabinet(k) {
    const m = this.m, size = m._malloc(4);
    const ptr = m._daytona_cabinet_save(k, size);
    const n = m.HEAPU32[size >> 2];
    m._free(size);
    if (!ptr || !n) throw new Error(`cabinet ${k}: no state`);
    return m.HEAPU8.slice(ptr, ptr + n);
  }
  loadCabinet(k, bytes) {
    const m = this.m, ptr = m._malloc(bytes.length);
    m.HEAPU8.set(bytes, ptr);
    const ok = m._daytona_cabinet_load(k, ptr, bytes.length);
    m._free(ptr);
    return !!ok;
  }
  resetCabinet(k) { return !!this.m._daytona_cabinet_reset(k); }
  /** The last data frame cabinet k received (sent = 0) or sent (1), 0xe01 bytes, or null. */
  linkFrame(k, sent) {
    const ptr = this.m._daytona_link_frame(k, sent);
    return ptr ? this.m.HEAPU8.slice(ptr, ptr + 0xe01) : null;
  }

  saveBase(file) {
    writeFileSync(file, this.core.serialize());
    writeFileSync(`${file}.json`, JSON.stringify({ frame: this.frame, cabinets: N }));
  }
  loadBase(file) {
    this.core.reset();
    this.core.unserialize(readFileSync(file));
    this.frame = JSON.parse(readFileSync(`${file}.json`, "utf8")).frame;
  }
}

const baseFile = join(OUT, `base-${N}.state`);

/** Runs the events for `frames` frames, with screenshots every `every` frames; logs the link. */
function scenario(ring, { tag, events, frames, every = 0, shots = "", quiet = false }) {
  const list = parseEvents(events), cabs = shots.split(",").filter(Boolean).map(Number), start = ring.frame;
  console.log(`${tag}: from frame ${start}, link ${ring.line()}`);
  let last = ring.line();
  while (ring.frame - start < frames) {
    const t = ring.frame - start;
    while (list.length && list[0].t <= t) {
      const said = apply(ring, list.shift(), tag);
      if (!quiet || !said.startsWith("cabinet")) console.log(`  t=${t} (frame ${ring.frame}): ${said}`);
    }
    if (every && cabs.length && t % every === 0) for (const k of cabs) ring.shoot(k, tag);
    else ring.step(1);
    if (ring.frame % 10 === 0) {
      const line = ring.line();
      if (line !== last) console.log(`  t=${ring.frame - start} (frame ${ring.frame}): link ${(last = line)}`);
    }
  }
  const s = ring.stats();
  console.log(`${tag}: end at frame ${ring.frame}, link ${ring.line()}; cables queued ${s.cabinets.map((c) => c.queued).join(" ")}`);
}

/** Parses the --events list into [{t, act, ...}] sorted by time. */
function parseEvents(text = "") {
  const events = [];
  for (const item of text.split(",").map((s) => s.trim()).filter(Boolean)) {
    const m = /^(.*)@(\d+)(?:\+(\d+))?(?:=(.*))?$/.exec(item);
    if (!m) throw new Error(`event? ${item}`);
    const [, what, t, dur, arg] = m;
    let e;
    if (/^\d+:[a-z0-9]+$/.test(what) && what.split(":")[1] in BUTTONS) {
      const [k, b] = what.split(":");
      e = { act: "hold", k: Number(k), mask: 1 << BUTTONS[b], n: Number(dur ?? 6), label: what };
    } else if (/^(cut|pause|ghost)=/.test(what)) {
      const [mode, list] = what.split("=");
      e = { act: "mode", mode, list: list.replaceAll(";", ",") };
    } else if (/^(reset|save|load):\d+$/.test(what)) e = { act: what.split(":")[0], k: Number(what.split(":")[1]), file: arg };
    else if (/^shots=/.test(what)) e = { act: "shots", ks: what.slice(6).split(";").map(Number) };
    else if (what === "status") e = { act: "status" };
    else if (what === "base") e = { act: "base", file: arg };
    else if (/^set:/.test(what)) { const [key, value] = what.slice(4).split("="); e = { act: "set", key, value }; }
    else throw new Error(`event? ${item}`);
    events.push({ t: Number(t), ...e });
  }
  return events.sort((a, b) => a.t - b.t);
}

/** Applies one event; returns a line for the log. */
function apply(ring, e, tag) {
  switch (e.act) {
    case "hold": ring.hold(e.k, e.mask, e.n); return `cabinet ${e.k} ${e.label.split(":")[1]} for ${e.n}`;
    case "mode": ring.set(`link_${e.mode}`, e.list); return `link_${e.mode}=${e.list}`;
    case "reset": return `cabinet ${e.k} power-on: ${ring.resetCabinet(e.k)}`;
    case "save": {
      const started = performance.now();
      const bytes = ring.saveCabinet(e.k);
      const ms = (performance.now() - started).toFixed(1);
      ring.kept.set(e.k, bytes);
      const file = join(OUT, `${tag}-cab${e.k}-f${ring.frame}.cabstate`);
      writeFileSync(file, bytes);
      return `cabinet ${e.k} saved in ${ms} ms: ${bytes.length} bytes (${deflateSync(bytes).length} deflated) -> ${basename(file)}`;
    }
    case "load": {
      const bytes = e.file ? readFileSync(e.file) : ring.kept.get(e.k);
      const started = performance.now();
      const ok = ring.loadCabinet(e.k, bytes);
      return `cabinet ${e.k} loaded ${e.file ? basename(e.file) : "(kept state)"} in ${(performance.now() - started).toFixed(1)} ms: ${ok}`;
    }
    case "shots": return `shots ${e.ks.map((k) => basename(ring.shoot(k, tag))).join(" ")}`;
    case "status": return `link ${ring.line()}`;
    case "base": ring.saveBase(resolve(e.file)); return `whole machine saved at frame ${ring.frame} -> ${e.file}`;
    case "set": ring.set(e.key, e.value.replaceAll(";", ",")); return `${e.key}=${e.value}`;
  }
}

async function makeRing(settings) {
  const ring = await Ring.create(settings);
  if (opt.base) {
    const file = opt.base === "1" ? baseFile : resolve(opt.base);
    if (!existsSync(file)) throw new Error(`no base state ${file}: run "form" first`);
    ring.loadBase(file);
  }
  return ring;
}

function settingsFrom(text = "") {
  // key=value,key=value; a list value (link_cut) separates its items with ";".
  return Object.fromEntries(text.split(",").filter(Boolean).map((s) => s.split("=")).map(([k, v]) => [k, (v ?? "").replaceAll(";", ",")]));
}

if (command === "form") {
  const ring = await Ring.create(settingsFrom(opt.set));
  const save = Number(opt.save ?? 2400);
  let last = "", allUp;
  const started = performance.now();
  while (ring.frame < save) {
    ring.step(1);
    if (ring.frame % 10 === 0) {
      const line = ring.line();
      if (line !== last) console.log(`frame ${ring.frame}: ${(last = line)}`);
      if (allUp === undefined && ring.status().link.every((l) => l.state === "up" && l.count === N)) allUp = ring.frame;
    }
  }
  console.log(`all ${N} up by frame ${allUp}; ${((performance.now() - started) / ring.frame).toFixed(1)} ms a frame (nothing drawn)`);
  for (const k of Array.from({ length: N }, (_, k) => k)) ring.shoot(k, "form");
  ring.saveBase(baseFile);
  console.log(`base state at frame ${ring.frame}: ${baseFile}; link ${ring.line()}`);
} else if (command === "bench") {
  const ring = await makeRing({});
  const frames = Number(opt.frames ?? 600);
  ring.set("view", opt.view ?? 0);
  ring.core.present = true;
  ring.step(60);
  ring.timings();
  const times = [];
  for (let i = 0; i < frames; i++) {
    const t = performance.now();
    ring.step(1);
    times.push(performance.now() - t);
  }
  const sorted = [...times].sort((a, b) => a - b), avg = times.reduce((a, b) => a + b) / frames;
  const t = ring.timings(), ms = (us) => (us / t.frames / 1000).toFixed(2);
  console.log(`${N} cabinets, one viewed: ${avg.toFixed(2)} ms a frame, p50 ${sorted[frames >> 1].toFixed(2)}, p95 ${sorted[Math.floor(frames * 0.95)].toFixed(2)}, worst ${sorted.at(-1).toFixed(2)} (budget 17.38)`);
  console.log(`  game code + board per cabinet: ${t.logic.map(ms).join(" ")} ms; geometrizer ${t.geometry.map(ms).join(" ")}; sound board ${t.sound.map(ms).join(" ")}; rasterizer ${t.raster.map(ms).join(" ")}; all of retro_run ${ms(t.total)}`);
  console.log(`  link ${ring.line()}; wasm memory ${(ring.m.HEAPU8.length / 1048576).toFixed(0)} MB`);
} else if (command === "run") {
  const ring = await makeRing(settingsFrom(opt.set));
  scenario(ring, { tag: opt.tag ?? "run", events: opt.events, frames: Number(opt.frames ?? 1200), every: Number(opt.every ?? 0), shots: opt.shots });
} else if (command === "sweep") {
  // The same scenario once per value (from the base state each time): {v} in --events is the
  // value; --shotAt=T;T... shoots --shots then. E.g. how late a second START still joins.
  const ring = await makeRing(settingsFrom(opt.set));
  const base = opt.base === "1" ? baseFile : resolve(opt.base);
  for (const v of opt.values.split(",")) {
    ring.loadBase(base);
    const at = opt.shotAt.replaceAll("{v}", v).split(";").map((s) => s.split("+").reduce((a, b) => a + Number(b), 0));
    const events = [...opt.events.replaceAll("{v}", v).split(","), ...at.map((t) => `shots=${opt.shots.replaceAll(",", ";")}@${t}`)].join(",");
    scenario(ring, { tag: `${opt.tag ?? "sweep"}-${v}`, events, frames: Math.max(...at) + 2 * opt.shots.split(",").length, quiet: true });
  }
} else if (command === "capture") {
  const ring = await Ring.create(settingsFrom(opt.set));
  const at = Number(opt.at ?? 3000), k = Number(opt.cab ?? 6);
  ring.step(at);
  const bytes = ring.saveCabinet(k);
  const file = resolve(opt.file ?? join(OUT, `preset-cab${k}-f${at}.cabstate`));
  writeFileSync(file, bytes);
  ring.shoot(k, "capture");
  console.log(`cabinet ${k} at frame ${at} (link ${ring.line()}): ${bytes.length} bytes -> ${file}`);
} else if (command === "protocol") {
  const ring = await makeRing(settingsFrom(opt.set));
  const frames = Number(opt.frames ?? 600), events = parseEvents(opt.events);
  const start = ring.frame, before = ring.stats();
  const sent = Array.from({ length: N }, () => []); // per cabinet: its own block per frame
  const ages = Array.from({ length: 8 }, () => new Map()); // block position -> age -> count
  const zero = new Uint8Array(0x1c0);
  while (ring.frame - start < frames) {
    while (events.length && events[0].t <= ring.frame - start) apply(ring, events.shift(), "protocol");
    ring.step(1);
    for (let k = 0; k < N; k++) {
      const tx = ring.linkFrame(k, 1);
      sent[k].push(tx ? tx.slice(1, 1 + 0x1c0) : zero);
    }
    // How old each block of the frame cabinet 0 (the master) just received is: the newest frame
    // whose block of the cabinet it should be from (link id = master's + 1 + position) matches.
    const rx = ring.linkFrame(0, 0);
    if (!rx || ring.frame - start < 32) continue;
    for (let p = 0; p < 8; p++) {
      const block = rx.subarray(1 + p * 0x1c0, 1 + (p + 1) * 0x1c0);
      const from = (1 + p) % N; // cabinet k has link id k + 1; the master's predecessor is cabinet 1
      const history = sent[from];
      let age = -1;
      for (let a = 0; a < 120 && a < history.length; a++)
        if (Buffer.compare(Buffer.from(history[history.length - 1 - a]), Buffer.from(block)) === 0) { age = a; break; }
      ages[p].set(age, (ages[p].get(age) ?? 0) + 1);
    }
  }
  const after = ring.stats(), n = ring.frame - start;
  console.log(`${n} frames from frame ${start}, link ${ring.line()}`);
  for (let k = 0; k < N; k++) {
    const a = after.cabinets[k], b = before.cabinets[k], d = (key) => a[key] - b[key];
    const blocks = sent[k];
    let changedFrames = 0, changedBytes = 0, nonzero = 0, packed = 0, packedDelta = 0;
    for (let i = 0; i < blocks.length; i++) {
      nonzero += blocks[i].reduce((s, v) => s + (v !== 0), 0);
      packed += deflateSync(blocks[i]).length;
      if (i) {
        const delta = blocks[i].map((v, j) => v ^ blocks[i - 1][j]);
        const c = delta.reduce((s, v) => s + (v !== 0), 0);
        changedBytes += c;
        changedFrames += c > 0;
        packedDelta += deflateSync(delta).length;
      }
    }
    console.log(`cabinet ${k}: ${(d("tx") / n).toFixed(0)} bytes/frame out (${(d("data") / n).toFixed(2)} data, ${(d("vsync") / n).toFixed(2)} vsync, ${d("tokens")} tokens), ` +
      `${(d("rx") / n).toFixed(0)} in; shift ${a.shift[0] - b.shift[0]} same / ${a.shift[1] - b.shift[1]} differ; ` +
      `own block: ${(nonzero / blocks.length).toFixed(0)} of 448 bytes non-zero, changed in ${changedFrames} of ${blocks.length - 1} frames, ` +
      `${(changedBytes / (blocks.length - 1)).toFixed(1)} bytes a frame; deflated ${(packed / blocks.length).toFixed(0)} B, delta deflated ${(packedDelta / (blocks.length - 1)).toFixed(0)} B`);
  }
  console.log("age (frames) of each block in the master's received frame, by position (link id = position + 2):");
  for (let p = 0; p < 8; p++) console.log(`  position ${p}: ${[...ages[p]].sort((x, y) => y[1] - x[1]).map(([age, c]) => `${age < 0 ? "no match" : age}: ${c}`).join(", ")}`);
  if (opt.dump) {
    for (let k = 0; k < N; k++) writeFileSync(join(OUT, `blocks-c${k}.bin`), Buffer.concat(sent[k].map((b) => Buffer.from(b))));
    console.log(`blocks per frame written to ${OUT}/blocks-c<k>.bin`);
  }
} else {
  console.error("usage: node daytona/harness/ring-lab.mjs form|bench|run|capture|protocol [--out=DIR] [--nvram=DIR] [--cabinets=8] ... (see the top of the file)");
  process.exit(2);
}
process.exit(0);
