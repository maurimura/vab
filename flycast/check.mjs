// Checks that the Flycast core (flycast/build.sh) keeps players' machines identical, in Node,
// through the bar's frontend (web/emulator/libretro.js) as its worker drives it. Every test
// starts from the same state (in a Virtua Tennis match) and feeds the same scripted inputs to
// both players, and compares the NAOMI's 32 MB of RAM (what the bar's lockstep checkpoints hash)
// every 60 frames:
//
//   processes  two separate Node processes: one runs frame after frame in a tight loop (the JIT
//              compiles each new block on the spot, its multi-block chain modules never land),
//              the other gives the event loop a turn between frames as the worker does (chain
//              modules compiled in the background land and run). Same RAM, every check.
//   warm       a process that has already played the inputs (JIT cache and chains warm) loads
//              the state again and replays them: the same RAM as a fresh process.
//   savestate  600 frames, a save, 600 more; back to the save, the same 600 again: the same RAM
//              and the same whole state, byte for byte. A fresh process, reset as the worker
//              does before loading another machine's state, loads the save and plays the 600:
//              the same RAM.
//   rollback   a rollback every 7 frames (back 4, replayed with the same inputs), saving a state
//              every frame as rollback netplay does: the same RAM as straight through.
//
//   node flycast/check.mjs [--frames 1800] [--state file] [--core flycast/dist/flycast.mjs]
//
// Without --state it makes one: boots the game (about 10200 frames: the NAOMI BIOS and the
// GD-ROM load), inserts coins, starts a match (800 frames), and keeps it in
// flycast/.cache/states/ under the core build's hash for the next run. ROMs from ROMS (default
// flycast/.cache/roms): vtennisg.zip and vtennisg/gds-0011.chd. Exit status 0 when all pass.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { Core } from "../web/emulator/libretro.js";
import { hash, inputs } from "./inputs.mjs";

export { hash, inputs };

const HERE = import.meta.dirname;
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const CORE = resolve(option("--core", join(HERE, "dist/flycast.mjs")));
const ROMS = resolve(process.env.ROMS ?? join(HERE, ".cache/roms"));
const FRAMES = Number(option("--frames", 1800));
const EVERY = 60;

/** A booted Virtua Tennis on a fresh core instance (a module of its own: another machine). */
export async function boot(corePath = CORE, { log = false } = {}) {
  const { default: createFlycast } = await import(corePath);
  const module = await createFlycast();
  const core = new Core(module, {
    onFrame() {},
    onAudio() {},
    onLog(level, text) { if (log || level >= 3) console.error(`core: ${text}`); },
  });
  core.netplay = true;
  core.addFile("vtennisg/gds-0011.chd", readFileSync(join(ROMS, "vtennisg/gds-0011.chd")));
  core.loadGame("vtennisg.zip", readFileSync(join(ROMS, "vtennisg.zip")));
  return { core, module };
}

const turn = () => new Promise((r) => setImmediate(r));

/** Plays `script` from the current state; the RAM hash every EVERY frames. */
async function play(core, script, { yieldEach = false, from = 0 } = {}) {
  const hashes = [];
  for (let f = 0; f < script.length; f++) {
    core.inputs[0] = script[f][0];
    core.inputs[1] = script[f][1];
    if (yieldEach) await turn();
    core.run();
    if ((from + f + 1) % EVERY === 0) hashes.push(hash(core.systemRam()));
  }
  return hashes;
}

/** The state the tests start from: in a match. */
async function matchState(statePath) {
  if (statePath) return readFileSync(statePath);
  const build = createHash("sha1").update(readFileSync(CORE.replace(/\.mjs$/, ".wasm"))).digest("hex").slice(0, 12);
  const dir = join(HERE, ".cache/states");
  const path = join(dir, `match-${build}.state`);
  if (existsSync(path)) return readFileSync(path);
  console.log(`making a state in a match (${path}): booting the game, about a minute...`);
  const { core } = await boot();
  for (let i = 0; i < 10200; i++) core.run();
  // Coins, Start, then Start and SHOT1 through the menus (player, court) into a match.
  const bit = (id) => 1 << id;
  for (let i = 0; i < 800; i++) {
    let mask = 0;
    if (i < 6 || (i >= 20 && i < 26)) mask |= bit(2);
    if (i >= 60 && i % 90 < 6) mask |= bit(3);
    if (i >= 150 && i % 45 > 38) mask |= bit(0);
    if (i >= 300 && i % 200 < 20) mask |= bit(7);
    core.inputs[0] = mask;
    core.run();
  }
  core.inputs[0] = 0;
  const state = core.serialize();
  mkdirSync(dir, { recursive: true });
  writeFileSync(path, state);
  return state;
}

/** Runs one test half in a child process (a separate machine); its JSON result. */
function child(task) {
  return new Promise((resolveChild, reject) => {
    const proc = spawn(process.execPath, [import.meta.filename, "--child", JSON.stringify(task)], { stdio: ["ignore", "pipe", "inherit"] });
    let out = "";
    proc.stdout.on("data", (d) => (out += d));
    proc.on("exit", (code) => (code === 0 ? resolveChild(JSON.parse(out)) : reject(new Error(`child ${task.kind} exited with ${code}`))));
  });
}

async function runChild(task) {
  const { core } = await boot(task.core);
  const state = readFileSync(task.state);
  const script = inputs(task.frames);
  core.unserialize(state);
  if (task.kind === "tight" || task.kind === "yield") {
    return { hashes: await play(core, script, { yieldEach: task.kind === "yield" }) };
  }
  if (task.kind === "warm") {
    await play(core, script, { yieldEach: true });
    core.unserialize(state);
    return { hashes: await play(core, script, { yieldEach: true }) };
  }
  if (task.kind === "resume") {
    // Another machine: reset, then the state the first one saved, then the same frames.
    core.reset();
    core.unserialize(readFileSync(task.resume));
    return { hashes: await play(core, script.slice(600, 1200), { from: 600 }) };
  }
  throw new Error(task.kind);
}

const isMain = process.argv[1] && resolve(process.argv[1]) === import.meta.filename;
if (isMain && args[0] === "--child") {
  const result = await runChild(JSON.parse(args[1]));
  process.stdout.write(JSON.stringify(result));
  process.exit(0);
}
if (isMain) await main();

async function main() {

const same = (a, b) => a.length === b.length && a.every((h, i) => h === b[i]);
const firstDiff = (a, b) => a.findIndex((h, i) => h !== b[i]);
let failed = 0;
const report = (name, ok, detail) => {
  if (!ok) failed++;
  console.log(`${ok ? "PASS" : "FAIL"} ${name}: ${detail}`);
};

const started = performance.now();
const state = await matchState(option("--state"));
const statePath = join(HERE, ".cache/states/check-start.state");
writeFileSync(statePath, state);
console.log(`core ${CORE}; state ${(state.length / 1048576).toFixed(1)} MiB; ${FRAMES} frames, RAM compared every ${EVERY}`);

// processes + warm, in parallel (three separate machines)
{
  const base = { core: CORE, state: statePath, frames: FRAMES };
  const [tight, yielding, warm] = await Promise.all([
    child({ ...base, kind: "tight" }), child({ ...base, kind: "yield" }), child({ ...base, kind: "warm" }),
  ]);
  const at = (i) => (i + 1) * EVERY;
  report("processes", same(tight.hashes, yielding.hashes),
    same(tight.hashes, yielding.hashes) ? `${tight.hashes.length} checks identical (last ${tight.hashes.at(-1)})`
      : `first difference at frame ${at(firstDiff(tight.hashes, yielding.hashes))}`);
  report("warm", same(tight.hashes, warm.hashes),
    same(tight.hashes, warm.hashes) ? `${warm.hashes.length} checks identical` : `first difference at frame ${at(firstDiff(tight.hashes, warm.hashes))}`);
}

// savestate: replay-exact in place, then another machine resumes from the save
{
  const { core } = await boot();
  const script = inputs(1200);
  core.unserialize(state);
  await play(core, script.slice(0, 600));
  const save = core.serialize();
  const savePath = join(HERE, ".cache/states/check-save.state");
  writeFileSync(savePath, save);
  const first = await play(core, script.slice(600), { from: 600 });
  const firstState = createHash("sha1").update(core.serialize()).digest("hex");
  core.unserialize(save);
  const again = await play(core, script.slice(600), { from: 600 });
  const againState = createHash("sha1").update(core.serialize()).digest("hex");
  report("savestate", same(first, again) && firstState === againState,
    `RAM ${same(first, again) ? "same" : "DIFFERENT"} over 600 frames, whole state ${firstState === againState ? "same" : "DIFFERENT"}`);
  const other = await child({ core: CORE, state: statePath, frames: 1200, kind: "resume", resume: savePath });
  report("savestate (another machine)", same(first, other.hashes),
    same(first, other.hashes) ? "reset + the save + 600 frames: same RAM" : `first difference at frame ${600 + (firstDiff(first, other.hashes) + 1) * EVERY}`);
}

// rollback
{
  const ROLLBACK = 4, PERIOD = 7, N = Math.min(FRAMES, 600);
  const script = inputs(N, 3);
  const straight = await boot();
  straight.core.unserialize(state);
  const want = await play(straight.core, script);
  const { core } = await boot();
  core.unserialize(state);
  core.allocSlots(ROLLBACK + 1);
  const hashes = [];
  let saveMs = 0, loadMs = 0, saves = 0, loads = 0;
  const step = (f) => {
    core.inputs[0] = script[f][0];
    core.inputs[1] = script[f][1];
    core.run();
  };
  for (let f = 0; f < N; f++) {
    let t = performance.now();
    core.saveSlot(f % (ROLLBACK + 1));
    saveMs += performance.now() - t;
    saves++;
    step(f);
    if (f % PERIOD === PERIOD - 1 && f >= ROLLBACK) {
      // Back to the state saved before frame f - ROLLBACK + 1, then those frames again.
      const back = f - ROLLBACK + 1;
      t = performance.now();
      core.loadSlot(back % (ROLLBACK + 1));
      loadMs += performance.now() - t;
      loads++;
      for (let g = back; g <= f; g++) {
        if (g > back) core.saveSlot(g % (ROLLBACK + 1));
        step(g);
      }
    }
    if ((f + 1) % EVERY === 0) hashes.push(hash(core.systemRam()));
  }
  report("rollback", same(want, hashes),
    `${loads} rollbacks of ${ROLLBACK} frames in ${N}: RAM ${same(want, hashes) ? "same as straight through" : `DIFFERENT from frame ${(firstDiff(want, hashes) + 1) * EVERY}`}; ` +
    `slot save ${(saveMs / saves).toFixed(1)} ms, load ${(loadMs / loads).toFixed(1)} ms`);
}

console.log(`${failed ? `${failed} FAILED` : "all passed"} in ${((performance.now() - started) / 1000).toFixed(0)} s`);
process.exit(failed ? 1 : 0);
}
