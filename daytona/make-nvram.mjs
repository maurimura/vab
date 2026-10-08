// Makes the cabinets' settings presets (daytona/nvram/<cabinets>/<cabinet>/ioboard_eeprom.bin and
// backup_ram.bin) by playing the game's own test menu, headless, with the Node build: each preset
// is a cabinet booted from factory settings and driven by an input script (the recomp's
// scripts/inputs format, as m2run --inputs) through TEST > GAME ASSIGNMENTS, then its EEPROM and
// backup RAM saved, as m2run --save-nvram does. Needs the ROM set and the core built from it.
//
//   node daytona/make-nvram.mjs explore <script.txt> [--frames N] [--every N] [--shots DIR] [--from F]
//       runs one cabinet on the script and writes a PNG of the screen every N frames (default
//       30; from frame F on) to DIR (default a temp dir), and all of them at half size on one
//       contact sheet (sheet.png, left to right, top to bottom): to find the frames the
//       script's presses need.
//   node daytona/make-nvram.mjs make [--only 1/0|2/0|2/1|8/3|8/] [--out DIR]
//       runs each preset's script (PRESETS below) and writes its files into DIR/<dir>/ (default
//       daytona/nvram/). --only takes one preset or, ending in "/", a machine's (8/: all eight).
//       The ring presets (8/0-8/7) are not built in (core.mk embeds nvram/1 and nvram/2): make
//       them elsewhere and give them to the core with nvram_dir (harness/ring-lab.mjs does).
//
// --core (default daytona/dist/headless/daytona.mjs) and --rom (default $ROMS/daytona.zip, ROMS
// defaulting to ~/Downloads) as check.mjs. Then rebuild (build.sh embeds daytona/nvram/1 and /2)
// and run check.mjs, whose link check needs the twin presets. See daytona/nvram/README.md.
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { crc32, deflateSync } from "node:zlib";
import { Core } from "../web/emulator/libretro.js";

const HERE = import.meta.dirname;
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const CORE = resolve(option("--core", join(HERE, "dist/headless/daytona.mjs")));
const ROM = resolve(option("--rom", join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip")));

// The presets: which cabinet of how many, and the script that sets it up in the test menu. Both
// cabinets of a twin need the same settings but LINK ID and CAR NUMBER (the game cancels a link
// between cabinets whose settings differ).
const PRESETS = [
  { dir: "1/0", script: "nvram/scripts/single.txt", what: "one cabinet on its own (no link), free play" },
  { dir: "2/0", script: "nvram/scripts/twin-master.txt", what: "twin, LINK ID master, car 1, free play" },
  { dir: "2/1", script: "nvram/scripts/twin-slave.txt", what: "twin, LINK ID slave, car 2, free play" },
  // A ring of up to eight (harness/ring-lab.mjs): the master and car 2 as the twin's, then cars 3-8.
  { dir: "8/0", script: "nvram/scripts/twin-master.txt", what: "ring, LINK ID master, car 1, free play", ring: true },
  { dir: "8/1", script: "nvram/scripts/twin-slave.txt", what: "ring, LINK ID slave, car 2, free play", ring: true },
  ...[3, 4, 5, 6, 7, 8].map((car) => ({
    dir: `8/${car - 1}`, script: `nvram/scripts/ring-car${car}.txt`, what: `ring, LINK ID slave, car ${car}, free play`, ring: true,
  })),
];

/** A PNG of RGBA pixels (no dependencies: zlib for the image data, crc32 for the chunks). */
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
  header[8] = 8; // bits per channel
  header[9] = 6; // RGBA
  const rows = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(rows, y * (width * 4 + 1) + 1);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}

/** Half-size copies of the pictures in a grid of `columns`, as one RGBA image. */
function sheet(pictures, columns = 4) {
  const w = pictures[0].width >> 1, h = pictures[0].height >> 1, rows = Math.ceil(pictures.length / columns);
  const out = new Uint8ClampedArray(columns * w * rows * h * 4);
  pictures.forEach((p, i) => {
    const ox = (i % columns) * w, oy = Math.floor(i / columns) * h;
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const s = ((y * 2) * p.width + x * 2) * 4, d = ((oy + y) * columns * w + ox + x) * 4;
        out[d] = p.rgba[s]; out[d + 1] = p.rgba[s + 1]; out[d + 2] = p.rgba[s + 2]; out[d + 3] = 255;
        if (x === w - 1 || y === h - 1) out[d] = out[d + 1] = out[d + 2] = 96; // a grey line between them
      }
  });
  return { rgba: out, width: columns * w, height: rows * h };
}

/** One cabinet from factory settings on `scriptPath`; returns the core after `frames` frames. */
async function run(scriptPath, { frames, every = 0, shots, from = 0 } = {}) {
  const text = readFileSync(scriptPath, "utf8");
  frames ??= Number(/^frames\s+(\d+)/m.exec(text)?.[1] ?? 4000);
  const { default: createDaytona } = await import(CORE);
  const module = await createDaytona();
  const set = (key, value) => {
    const alloc = (s) => { const n = module.lengthBytesUTF8(s) + 1; const p = module._malloc(n); module.stringToUTF8(s, p, n); return p; };
    const k = alloc(key), v = alloc(value);
    module._daytona_set(k, v);
    module._free(k);
    module._free(v);
  };
  let picture;
  const core = new Core(module, {
    onFrame(rgba, width, height) { picture = { rgba, width, height }; },
    onAudio() {},
    onLog(level, text) { if (level >= 2) console.error(text); },
  });
  module.FS.mkdirTree("/scripts");
  module.FS.writeFile("/scripts/input.txt", text);
  set("cabinets", "1");      // a board on its own: the test menu needs no link
  set("presets", "0");       // from the factory's settings, not built-in presets
  set("nvram_dir", "/none");
  set("script0", "/scripts/input.txt");
  core.loadGame(basename(ROM), readFileSync(ROM));
  const taken = [];
  for (let f = 1; f <= frames; f++) {
    core.run();
    if (every && shots && f >= from && f % every === 0 && picture) {
      writeFileSync(join(shots, `frame_${String(f).padStart(5, "0")}.png`), png(picture.rgba, picture.width, picture.height));
      taken.push({ frame: f, ...picture, rgba: picture.rgba.slice() });
    }
  }
  if (taken.length) {
    const s = sheet(taken);
    writeFileSync(join(shots, "sheet.png"), png(s.rgba, s.width, s.height));
    console.log(`sheet.png: frames ${taken.map((t) => t.frame).join(" ")}`);
  }
  return { module, core };
}

function saveNvram(module, dir) {
  const path = "/out", p = module._malloc(8);
  module.stringToUTF8(path, p, 8);
  const ok = module._daytona_save_nvram(0, p);
  module._free(p);
  if (!ok) throw new Error("daytona_save_nvram failed");
  mkdirSync(dir, { recursive: true });
  for (const file of ["ioboard_eeprom.bin", "backup_ram.bin"]) writeFileSync(join(dir, file), module.FS.readFile(`/out/${file}`));
}

if (!existsSync(ROM) || !existsSync(CORE)) {
  console.error(`needs the ROM set (${ROM}) and the core built from it (${CORE}): see daytona/README.md`);
  process.exit(2);
}
const [mode, scriptArg] = args;
if (mode === "explore" && scriptArg) {
  const shots = option("--shots", mkdtempSync(join(tmpdir(), "daytona-shots-")));
  mkdirSync(shots, { recursive: true });
  const frames = option("--frames") ? Number(option("--frames")) : undefined;
  await run(resolve(scriptArg), { frames, every: Number(option("--every", 30)), shots, from: Number(option("--from", 0)) });
  console.log(`screens in ${shots}`);
} else if (mode === "make") {
  const only = option("--only");
  const out = resolve(option("--out", join(HERE, "nvram")));
  const wanted = (p) => (only ? (only.endsWith("/") ? p.dir.startsWith(only) : p.dir === only) : !p.ring);
  for (const preset of PRESETS.filter(wanted)) {
    const script = join(HERE, preset.script);
    if (/^\s*#\s*TODO/m.test(readFileSync(script, "utf8"))) {
      console.error(`${preset.script}: still a skeleton (its TODO lines): fill it in with explore first`);
      process.exitCode = 1;
      continue;
    }
    const { module } = await run(script);
    const dir = join(out, preset.dir);
    saveNvram(module, dir);
    console.log(`${preset.dir}: ${preset.what} -> ${dir}`);
  }
} else {
  console.error("usage: node daytona/make-nvram.mjs explore <script.txt> [--frames N] [--every N] [--shots DIR] | make [--only 1/0|2/0|2/1|8/k|8/] [--out DIR]");
  process.exit(2);
}
