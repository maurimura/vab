// Makes the arcade mode's seat states: for each seat k of 8, a whole-machine state of a
// `cabinets=1`, `link_topology=star`, `seat=k` machine whose one cabinet is cabinet k of a linked
// 8-cabinet ring in its attract mode (link up, id k + 1 of 8, car k + 1, "LINK SYSTEM / UP TO 8
// RACERS WANTED"), so a player sitting at seat k loads it with the worker's plain reset() +
// unserialize() and is on the link at once: nothing ever boots over the network (ring-notes.md).
//
//   ROMS=<dir> node daytona/make-states.mjs [--nvram=DIR] [--at=5000] [--out=daytona/dist/states]
//       [--shots=DIR] [--core=daytona/dist/headless/daytona.mjs] [--rom=$ROMS/daytona.zip]
//
// 1. The ring presets 8/0-8/7 (master car 1, slaves cars 2-8): from --nvram=DIR (DIR/8/<k>/), or
//    made into a temporary directory by make-nvram.mjs.
// 2. An 8-cabinet ring (the shim's default topology) from power-on to frame --at; each cabinet
//    saved alone there (daytona_cabinet_save), all at the same frame.
// 3. For each seat: a star machine of one cabinet, reset, cabinet k's state loaded into it
//    (daytona_cabinet_load), checked (link up, id k + 1 of 8), and the whole machine serialized
//    (retro_serialize: the bridge's table of blocks is in it, all seats empty) and packed as
//    worker.js's pack() does ("vabz" + deflate-raw) into <out>/daytona.seat<k>.state.
// 4. Each file loaded into a fresh core as the worker would (reset, unpack, unserialize): link up,
//    id k + 1 of 8, still up after 120 frames alone; a PNG of each seat's screen then (--shots,
//    default <out>/shots/), and of each ring cabinet at the capture.
//
// The states are derived from the ROM set: dist/ is git-ignored. `make upload-daytona-states`
// puts them in R2 at roms/daytona.seat<k>.state.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Core } from "../web/emulator/libretro.js";
import { Seat, pack, png, unpack } from "./harness/seat.mjs";

const HERE = import.meta.dirname;
const opt = Object.fromEntries(process.argv.slice(2).map((a) => { const m = /^--([^=]+)(?:=(.*))?$/s.exec(a); return m ? [m[1], m[2] ?? "1"] : [a, "1"]; }));
const CORE = resolve(opt.core ?? join(HERE, "dist/headless/daytona.mjs"));
const ROM = resolve(opt.rom ?? join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip"));
const OUT = resolve(opt.out ?? join(HERE, "dist/states"));
const SHOTS = resolve(opt.shots ?? join(OUT, "shots"));
const AT = Number(opt.at ?? 5000);
const N = 8;
mkdirSync(OUT, { recursive: true });
mkdirSync(SHOTS, { recursive: true });
for (const [what, file] of [["core", CORE], ["ROM set", ROM]]) {
  if (!existsSync(file)) { console.error(`no ${what} at ${file}`); process.exit(1); }
}

// 1. The ring presets.
let nvram = opt.nvram ? resolve(opt.nvram) : process.env.RING_NVRAM;
if (!nvram || !existsSync(join(nvram, `${N}/${N - 1}`))) {
  nvram = mkdtempSync(join(tmpdir(), "daytona-ring-nvram-"));
  console.log(`making the ring presets 8/0-8/7 in ${nvram} (make-nvram.mjs)...`);
  const made = spawnSync(process.execPath, [join(HERE, "make-nvram.mjs"), "make", "--only", `${N}/`, "--out", nvram, "--core", CORE, "--rom", ROM], { stdio: "inherit" });
  if (made.status !== 0) process.exit(1);
}

// 2. The ring, to frame AT; every cabinet saved there.
const { default: createDaytona } = await import(CORE);
const ringModule = await createDaytona();
const ring = new Core(ringModule, { onFrame: (rgba, width, height) => { ring.picture = { rgba, width, height }; }, onAudio() {}, onLog: (level, text) => { if (level >= 2) console.error(`  ring: ${text}`); } });
ring.inputs = new Uint16Array(N);
ring.present = false;
ring.setOption("cabinets", String(N));
for (let k = 0; k < N; k++) {
  ringModule.FS.mkdirTree(`/ring-nvram/${k}`);
  for (const file of ["ioboard_eeprom.bin", "backup_ram.bin"]) ringModule.FS.writeFile(`/ring-nvram/${k}/${file}`, readFileSync(join(nvram, `${N}/${k}`, file)));
}
ring.setOption("nvram_dir", "/ring-nvram");
ring.loadGame("daytona.zip", readFileSync(ROM));
const ringStatus = () => JSON.parse(ringModule.UTF8ToString(ringModule._daytona_link_status())).link;
const started = performance.now();
let allUp;
for (let f = 1; f <= AT; f++) {
  ring.run();
  if (allUp === undefined && f % 10 === 0 && ringStatus().every((l, k) => l.state === "up" && l.id === k + 1 && l.count === N)) allUp = f;
}
const links = ringStatus();
console.log(`ring of ${N}: all up by frame ${allUp}, at frame ${AT}: ${links.map((l) => `${l.state} ${l.id}/${l.count}`).join(", ")} (${((performance.now() - started) / AT).toFixed(1)} ms a frame)`);
if (!links.every((l, k) => l.state === "up" && l.id === k + 1 && l.count === N)) { console.error("the ring is not up: no states"); process.exit(1); }
const cabinets = [];
for (let k = 0; k < N; k++) {
  const size = ringModule._malloc(4);
  const ptr = ringModule._daytona_cabinet_save(k, size);
  const n = ringModule.HEAPU32[size >> 2];
  ringModule._free(size);
  if (!ptr || !n) { console.error(`cabinet ${k}: no state`); process.exit(1); }
  cabinets.push(ringModule.HEAPU8.slice(ptr, ptr + n));
}
// Proof: each ring cabinet's screen right after the capture (frames AT+1..AT+8, one each).
for (let k = 0; k < N; k++) {
  ring.setOption("view", String(k));
  ring.present = true;
  ring.picture = undefined;
  ring.run();
  if (ring.picture) writeFileSync(join(SHOTS, `ring-c${k}-f${AT + 1 + k}.png`), png(ring.picture.rgba, ring.picture.width, ring.picture.height));
}
console.log(`cabinet states at frame ${AT}: ${cabinets.map((c) => c.length).join(", ")} bytes`);

// 3. Each seat's whole-machine state.
const linked = (status, k) => status.link[0]?.state === "up" && status.link[0].id === k + 1 && status.link[0].count === N;
const maker = await Seat.create({ core: CORE, rom: ROM, seat: 0, onLog: (level, text) => { if (level >= 2) console.error(`  seat: ${text}`); } });
let failures = 0;
for (let k = 0; k < N; k++) {
  maker.core.reset(); // then the seat: a power-on has no link up to check it against
  maker.set("seat", k);
  const ptr = maker.m._malloc(cabinets[k].length);
  maker.m.HEAPU8.set(cabinets[k], ptr);
  const loaded = maker.m._daytona_cabinet_load(0, ptr, cabinets[k].length);
  maker.m._free(ptr);
  const status = maker.status();
  if (!loaded || !linked(status, k)) {
    console.error(`FAIL seat ${k}: cabinet state ${loaded ? "loaded" : "did not load"}, link ${JSON.stringify(status.link)}`);
    failures++;
    continue;
  }
  const state = maker.core.serialize();
  const packed = await pack(state);
  const file = join(OUT, `daytona.seat${k}.state`);
  writeFileSync(file, packed);
  console.log(`seat ${k}: ${file}: ${packed.length} bytes packed (${state.length} as retro_serialize gives it)`);
}

// 4. Each file in a fresh core, as the worker loads it.
for (let k = 0; k < N; k++) {
  const file = join(OUT, `daytona.seat${k}.state`);
  if (!existsSync(file)) continue;
  const seat = await Seat.create({ core: CORE, rom: ROM, seat: k, onLog: (level, text) => { if (level >= 2) console.error(`  fresh seat ${k}: ${text}`); } });
  const bytes = readFileSync(file);
  await seat.load(bytes);
  const before = seat.status();
  for (let f = 0; f < 119; f++) seat.run();
  seat.run(0, true);
  const after = seat.status();
  if (seat.picture) writeFileSync(join(SHOTS, `seat${k}-loaded-f120.png`), seat.pngBytes());
  const ok = linked(before, k) && linked(after, k) && before.star?.seat === k && before.star.seats.every((s, c) => (c === k ? s.state === "self" : s.state === "empty"));
  console.log(`${ok ? "ok  " : "FAIL"} seat ${k} in a fresh core: link ${before.link[0].state}, id ${before.link[0].id} of ${before.link[0].count}; after 120 frames alone ${after.link[0].state} ${after.link[0].id} of ${after.link[0].count} (${(await unpack(bytes)).length} bytes unpacked)`);
  if (!ok) failures++;
}
console.log(failures ? `${failures} seat(s) failed` : `all ${N} seat states made and checked in ${OUT}; screens in ${SHOTS}`);
console.log(`(listing: ${readdirSync(OUT).filter((f) => f.endsWith(".state")).join(" ")})`);
process.exit(failures ? 1 : 0);
