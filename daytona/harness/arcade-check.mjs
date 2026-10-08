// The arcade mode end to end in Node, without a browser: each seat a separate Node process
// running one cabinet (cabinets=1, link_topology=star, seat=k, loaded from its seat state, as
// the worker would), the blocks carried between them by a relay in this process (IPC), with a
// delay and jitter. What the worker must do each frame is what a child does here (child()).
//
//   ROMS=<dir> node daytona/harness/arcade-check.mjs [--delay=6] [--jitter=3] [--lead=3]
//       [--seed=1] [--absent=zero|freeze] [--only=contract,pair,replay,trio] [--out=DIR]
//       [--states=daytona/dist/states] [--core=...] [--rom=...]
//
// contract  in this process, one seat: the exports as the worker uses them (block size, what
//         daytona_link_out says when, daytona_link_in's rejections, the status JSON, the frame
//         the board is handed made of the blocks handed over before the frame, the table in a
//         whole-machine state, link_absent zero and freeze, the seat check).
// pair    seats 0 and 1 in two processes, both in the attract mode: seat 0 presses START, seat 1
//         START 2 s (115 frames) later, then both choose (accelerator) and hold the accelerator:
//         (a) both race, linked, POSITION n/2 on both screens (PNGs); seat 0 records its
//         per-frame log (pad, blocks handed over) and a state at frame 1200.
// replay  (d) seat 0 again in a fresh process from the same seat state with only that log: the
//         same RAM hash and screen every 120 frames; and from the frame-1200 state (a spectator
//         joining then) with the log from there: the same.
// trio    seats 0, 1 and 2 in three processes: (a) again; (b) seat 2 presses START after the
//         race began: its own session (a race alone); (c) seat 1 leaves mid-race (its process
//         ends, the relay says so): seat 0's race goes on.
//
// The relay: every frame a child sends its frame number, with its block when it changed
// (daytona_link_out); the relay passes it to the others stamped with the frame it is due at, the
// sender's frame + 1 + delay + up to `jitter` more (seeded per sender and receiver, in order).
// A child hands over (daytona_link_in) every block due by its next frame, then runs it: one
// block a seat at most a frame, whatever arrived. The processes run as fast as they can; so that
// "6 frames" means the same in each, none runs more than `lead` frames ahead of another (a
// stand-in for every browser's 57.5 Hz clock; the bridge itself never waits).
//
// Game facts these checks read (measured on MAME's daytona set, 2026-10-07, see ring-notes.md
// "The bridge"): main RAM 0x501080 is the number of cars on the track (10 outside a race, 16 in a
// linked race of two, 40 in a race alone), 0x540027 the entrants in this cabinet's session
// (2 for a linked pair, 1 for one alone, 0 idle).
import { fork } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { BUTTONS, Seat, hash, pack } from "./seat.mjs";

const HERE = import.meta.dirname;
const opt = Object.fromEntries(process.argv.slice(2).map((a) => { const m = /^--([^=]+)(?:=(.*))?$/s.exec(a); return m ? [m[1], m[2] ?? "1"] : [a, "1"]; }));
const CORE = resolve(opt.core ?? join(HERE, "../dist/headless/daytona.mjs"));
const ROM = resolve(opt.rom ?? join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip"));
const STATES = resolve(opt.states ?? join(HERE, "../dist/states"));
const OUT = resolve(opt.out ?? join(tmpdir(), "daytona-arcade"));
const DELAY = Number(opt.delay ?? 6), JITTER = Number(opt.jitter ?? 3), LEAD = Number(opt.lead ?? 3), SEED = Number(opt.seed ?? 1);
const ABSENT = opt.absent ?? "zero";
const EVERY = 120;
const CARS = 0x1080, ENTRANTS = 0x40027; // main RAM offsets (from 0x500000)

const stateFile = (k) => join(STATES, `daytona.seat${k}.state`);

// The players: presses by seat as [button, frame, frames held] in each seat's own frames since its
// state was loaded. Seat 1's START is 115 frames (2 s) after seat 0's; each then chooses the
// highlighted circuit (accelerator), START again, and holds the accelerator from 1500 (Mission
// Select's choice, then the race: GO! at about frame 1900). Seat 2 presses START at 2100.
const PLAYERS = {
  0: [["start", 300, 10], ["up", 600, 10], ["start", 1200, 10], ["up", 1500, 1e9]],
  1: [["start", 415, 10], ["up", 605, 10], ["start", 1200, 10], ["up", 1500, 1e9]],
  2: [["start", 2100, 10], ["up", 2400, 10], ["start", 3000, 10], ["up", 3300, 1e9]],
};
const padAt = (seat, f) => (PLAYERS[seat] ?? []).reduce((pad, [b, at, n]) => (f >= at && f < at + n ? pad | (1 << BUTTONS[b]) : pad), 0);

if (opt.child) await child();
else if (opt.replay) await replay();
else await main();

/** One seat: load, then per frame hand over what is due, run, send the block. */
async function child() {
  const k = Number(opt.seat), frames = Number(opt.frames), leave = opt.leave ? Number(opt.leave) : -1;
  const shots = (opt.shots ?? "").split(",").filter(Boolean).map(Number);
  const probes = (opt.probes ?? "").split(",").filter(Boolean).map(Number);
  const saveAt = opt.saveAt ? Number(opt.saveAt) : -1;
  const tag = opt.tag;
  const seat = await Seat.create({ core: CORE, rom: ROM, seat: k, options: { link_absent: ABSENT }, onLog: (level, text) => { if (level >= 2) console.error(`  [seat ${k}] ${text}`); } });
  await seat.load(readFileSync(stateFile(k)));
  const others = new Set((opt.seats ?? "").split(",").map(Number).filter((s) => s !== k));
  const heard = new Map(); // seat -> the last frame it ran
  const queue = new Map([...others].map((s) => [s, []])); // seat -> [{ due, block } | { due, absent }]
  let wake;
  process.on("message", (msg) => {
    if (msg.t === "f") {
      heard.set(msg.seat, msg.frame);
      if (msg.block) queue.get(msg.seat).push({ due: msg.due, block: msg.block });
    } else if (msg.t === "absent") {
      others.delete(msg.seat); // nothing more to wait for from it
      queue.get(msg.seat).push({ due: msg.due, absent: true });
    } else if (msg.t === "go") heard.set("go", 0);
    wake?.();
  });
  const until = async (ready) => { while (!ready()) await new Promise((r) => (wake = r)); };
  process.send({ t: "ready", seat: k, link: seat.status() });
  await until(() => heard.has("go"));

  const log = []; // per frame: [pad, ops]; an op is [seat, block (base64)] or [seat] (absent)
  const hashes = {}, probed = [];
  const started = performance.now();
  for (let f = 0; f < frames; f++) {
    if (f === leave) { process.send({ t: "leave", seat: k, frame: f }); break; }
    await until(() => [...others].every((s) => (heard.get(s) ?? -1) >= f - LEAD));
    // Everything due by this frame, in order: the latest block of each seat wins.
    const ops = [];
    for (const [s, q] of queue) {
      while (q.length && q[0].due <= f) {
        const e = q.shift();
        if (e.absent) { seat.absent(s); ops.push([s]); }
        else { seat.in(s, e.block); ops.push([s, Buffer.from(e.block).toString("base64")]); }
      }
    }
    const pad = padAt(k, f);
    log.push([pad, ops]);
    const n = f + 1; // frames run once this one has
    seat.run(pad, n % EVERY === 0 || shots.includes(n));
    if (n % EVERY === 0) hashes[n] = { ram: seat.ram(), screen: seat.screen() };
    if (shots.includes(n) && seat.picture) writeFileSync(join(OUT, `${tag}-seat${k}-f${n}.png`), seat.pngBytes());
    if (probes.includes(n)) {
      const ram = seat.core.systemRam();
      probed.push({ frame: n, cars: ram[CARS], entrants: ram[ENTRANTS], ram: hash(ram), screen: seat.screen(), link: seat.status() });
    }
    if (n === saveAt) writeFileSync(join(OUT, `${tag}-seat${k}-f${n}.state`), await seat.save());
    const out = seat.out();
    process.send({ t: "f", seat: k, frame: f, block: out.changed ? out.block : undefined });
    await new Promise(setImmediate); // let the relay's messages in
  }
  const ms = (performance.now() - started) / Math.max(1, log.length);
  if (opt.record) writeFileSync(join(OUT, `${tag}-seat${k}.log.json`), JSON.stringify({ seat: k, state: stateFile(k), saveAt, frames: log.length, hashes, log }));
  process.send({ t: "done", seat: k, frames: log.length, hashes, probed, ms, link: seat.status(), warnings: seat.logs.filter((l) => l.level >= 2).map((l) => l.text) },
    () => process.exit(0));
}

/** The relay and the scenario: forks a child per seat, carries their blocks, returns their reports. */
function session(tag, seats, { frames, leave = {}, shots = [], probes = [], record = -1, saveAt = -1 }) {
  return new Promise((resolveSession, reject) => {
    // A xorshift per sender and receiver, so a run's delays do not depend on the order the
    // children's messages reach the relay: the same seed, the same game.
    const rngs = new Map();
    const random = (key, from, to) => {
      let x = rngs.get(key) ?? ((Math.imul(SEED, 0x9e3779b1) ^ Math.imul(from + 1, 0x85ebca6b) ^ Math.imul(to + 1, 0xc2b2ae35)) >>> 0 || 1);
      x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0;
      rngs.set(key, x);
      return x;
    };
    const lastDue = new Map(); // "from>to" -> due
    const children = new Map(), reports = new Map(), ready = new Set();
    const due = (from, to, frame) => {
      const key = `${from}>${to}`;
      const d = Math.max(lastDue.get(key) ?? 0, frame + 1 + DELAY + (JITTER ? random(key, from, to) % (JITTER + 1) : 0));
      lastDue.set(key, d);
      return d;
    };
    const others = (k) => [...children].filter(([s, c]) => s !== k && c.connected);
    for (const k of seats) {
      const args = [`--child`, `--seat=${k}`, `--seats=${seats.join(",")}`, `--frames=${frames}`, `--tag=${tag}`, `--out=${OUT}`, `--core=${CORE}`, `--rom=${ROM}`,
        `--states=${STATES}`, `--delay=${DELAY}`, `--lead=${LEAD}`, `--absent=${ABSENT}`, `--shots=${shots.join(",")}`, `--probes=${probes.join(",")}`];
      if (leave[k] !== undefined) args.push(`--leave=${leave[k]}`);
      if (record === k) args.push("--record", `--saveAt=${saveAt}`);
      const c = fork(import.meta.filename, args, { serialization: "advanced", stdio: ["ignore", "inherit", "inherit", "ipc"] });
      children.set(k, c);
      c.on("message", (msg) => {
        if (msg.t === "ready") {
          ready.add(k);
          console.log(`  ${tag}: seat ${k} loaded ${stateFile(k).split("/").at(-1)}: link ${msg.link.link[0].state}, id ${msg.link.link[0].id} of ${msg.link.link[0].count}`);
          if (ready.size === seats.length) for (const child of children.values()) child.send({ t: "go" });
        } else if (msg.t === "f") {
          for (const [s, c2] of others(k)) c2.send(msg.block ? { ...msg, due: due(k, s, msg.frame) } : msg);
        } else if (msg.t === "leave") {
          console.log(`  ${tag}: seat ${k} leaves at its frame ${msg.frame}`);
          for (const [s, c2] of others(k)) c2.send({ t: "absent", seat: k, due: due(k, s, msg.frame - 1) });
        } else if (msg.t === "done") reports.set(k, msg);
      });
      c.on("close", (code) => {
        if (!reports.has(k)) { reject(new Error(`${tag}: seat ${k}'s process ended (${code}) without a report`)); return; }
        if (reports.size === seats.length && [...children.values()].every((x) => x.exitCode !== null)) resolveSession(reports);
      });
    }
  });
}

/** The bridge's exports, one seat in this process (seat 0's state; seat 1's for the seat check). */
async function contract(check) {
  const seat = await Seat.create({ core: CORE, rom: ROM, seat: 0 });
  const m = seat.m, said = () => seat.logs.splice(0).map((l) => l.text).join(" | ");
  check(m._daytona_link_block_size() === 448, "contract: daytona_link_block_size() is 448", String(m._daytona_link_block_size()));
  await seat.load(readFileSync(stateFile(0)));
  said();
  let st = seat.status();
  check(st.link[0].state === "up" && st.link[0].id === 1 && st.link[0].count === 8 && st.star?.seat === 0 && st.star.count === 8 && st.star.absent === ABSENT &&
    st.star.seats.length === 8 && st.star.seats[0].state === "self" && st.star.seats.slice(1).every((x) => x.state === "empty"),
    "contract: seat 0's state loads (reset + unserialize): up 1 of 8; status star: self + 7 empty", JSON.stringify(st.star));
  const first = seat.out(), second = seat.out();
  check(first.changed && !second.changed, "contract: daytona_link_out says changed on its first call after a load, then not without a frame between");
  seat.run();
  const third = seat.out();
  check(third.changed && third.block.some((b) => b !== 0), "contract: after a frame it gives the block the board sent (changed, not zeros)", `${third.block.filter((b) => b !== 0).length} of 448 bytes non-zero`);
  // Rejections: a block of the wrong size, the own seat, no such seat; each said and ignored.
  seat.in(1, new Uint8Array(447).fill(7));
  const short = said();
  seat.in(0, new Uint8Array(448).fill(7));
  const own = said();
  seat.in(9, new Uint8Array(448).fill(7));
  const none = said();
  st = seat.status();
  check(/447 bytes.*rejected/.test(short) && /own: ignored/.test(own) && /no seat 9/.test(none) && st.star.seats[1].state === "empty",
    "contract: daytona_link_in rejects a 447-byte block, the own seat and seat 9, with a log each", `${short} | ${own} | ${none}`);
  // The frame the board is handed: seat 0 (id 1) gets the one from id 2, block p from seat 1 + p.
  const mark = (k) => Uint8Array.from({ length: 448 }, (_, i) => (i * 7 + k * 31) & 0xff || 1);
  seat.in(1, mark(1));
  seat.in(1, mark(1).map((b) => b ^ 0x55)); // latest wins
  seat.in(5, mark(5));
  st = seat.status();
  check(st.star.seats[1].state === "present" && st.star.seats[1].age === 0 && st.star.seats[5].state === "present", "contract: a block handed over makes its seat present (age 0 before the frame)",
    JSON.stringify(st.star.seats.slice(1, 6)));
  seat.run();
  const frame = m.HEAPU8.slice(m._daytona_link_frame(0, 0), m._daytona_link_frame(0, 0) + 0xe01);
  const slot = (p) => frame.subarray(1 + p * 448, 1 + (p + 1) * 448);
  const same = (x, y) => x.length === y.length && x.every((v, i) => v === y[i]);
  check(frame[0] === 2 && same(slot(0), mark(1).map((b) => b ^ 0x55)) && same(slot(4), mark(5)) && slot(1).every((b) => b === 0) && same(slot(7), third.block),
    "contract: the next frame's data frame is id 2's, made of the latest blocks (seat 1 at slot 0, seat 5 at slot 4, empty seats zeros, its own block last)",
    `type ${frame[0]}`);
  check(seat.status().star.seats[1].age === 1, "contract: the status's age counts frames since the hand-over", JSON.stringify(seat.status().star.seats[1]));
  // The table is machine state: a state carries it.
  const state = seat.core.serialize();
  seat.core.reset();
  seat.core.unserialize(state);
  st = seat.status();
  check(st.star.seats[1].state === "present" && st.star.seats[5].state === "present" && st.star.seats[2].state === "empty", "contract: a whole-machine state carries the blocks (present seats again after reset + load)");
  // Leaving: zeros (default) or frozen; a later block brings the seat back.
  seat.set("link_absent", "freeze");
  seat.absent(5);
  seat.set("link_absent", "zero");
  seat.absent(1);
  st = seat.status();
  check(st.star.seats[1].state === "absent" && st.star.seats[1].frozen === false && st.star.seats[5].state === "absent" && st.star.seats[5].frozen === true,
    "contract: daytona_link_absent: link_absent=zero zeroes the seat's block, freeze keeps it", JSON.stringify([st.star.seats[1], st.star.seats[5]]));
  seat.run();
  const after = m.HEAPU8.slice(m._daytona_link_frame(0, 0), m._daytona_link_frame(0, 0) + 0xe01);
  check(after.subarray(1, 449).every((b) => b === 0) && same(after.subarray(1 + 4 * 448, 1 + 5 * 448), mark(5)), "contract: and the board is handed that (seat 1 zeros, seat 5 as it was)");
  seat.set("link_absent", ABSENT);
  seat.in(1, mark(1));
  check(seat.status().star.seats[1].state === "present", "contract: a block after leaving makes the seat present again");
  // The seat check: seat 1's state in a machine set to seat 0.
  said();
  await seat.load(readFileSync(stateFile(1)));
  check(/link id 2 of 8 .*seat=0/.test(said()), "contract: loading seat 1's state as seat 0 is warned about", `${seat.status().link[0].id} of ${seat.status().link[0].count}`);
  if (seat.core.linkBlockSize !== undefined) check(seat.core.linkBlockSize === 448, "contract: libretro.js finds the link (Core.linkBlockSize 448)", String(seat.core.linkBlockSize));
}

/** (d) seat 0 alone from a state and the recorded log (from `from` on): its hashes. */
async function replay() {
  const rec = JSON.parse(readFileSync(opt.replay, "utf8"));
  const from = Number(opt.from ?? 0);
  const seat = await Seat.create({ core: CORE, rom: ROM, seat: rec.seat, options: { link_absent: ABSENT }, onLog: (level, text) => { if (level >= 2) console.error(`  [replay] ${text}`); } });
  await seat.load(readFileSync(opt.state));
  const hashes = {};
  for (let f = from; f < rec.frames; f++) {
    const [pad, ops] = rec.log[f];
    for (const [s, block] of ops) block ? seat.in(s, Buffer.from(block, "base64")) : seat.absent(s);
    const n = f + 1;
    seat.run(pad, n % EVERY === 0);
    if (n % EVERY === 0) hashes[n] = { ram: seat.ram(), screen: seat.screen() };
  }
  console.log(JSON.stringify({ hashes }));
}

function replayIn(log, state, from) {
  return new Promise((resolveReplay, reject) => {
    const c = fork(import.meta.filename, [`--replay=${log}`, `--state=${state}`, `--from=${from}`, `--core=${CORE}`, `--rom=${ROM}`, `--absent=${ABSENT}`],
      { stdio: ["ignore", "pipe", "inherit", "ipc"] });
    let text = "";
    c.stdout.on("data", (d) => (text += d));
    c.on("exit", (code) => (code === 0 ? resolveReplay(JSON.parse(text.trim().split("\n").at(-1))) : reject(new Error(`replay ended ${code}`))));
  });
}

async function main() {
  mkdirSync(OUT, { recursive: true });
  for (const [what, file] of [["core", CORE], ["ROM set", ROM], ["seat 0 state", stateFile(0)], ["seat 2 state", stateFile(2)]])
    if (!existsSync(file)) { console.error(`no ${what} at ${file} (states: node daytona/make-states.mjs)`); process.exit(1); }
  const only = new Set((opt.only ?? "contract,pair,replay,trio").split(","));
  let failures = 0;
  const check = (ok, what, detail = "") => { console.log(`${ok ? "PASS" : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`); if (!ok) failures++; };
  const at = (report, frame) => report.probed.find((p) => p.frame === frame);
  const show = (p) => (p ? `cars ${p.cars}, entrants ${p.entrants}` : "no probe");
  console.log(`relay: delay ${DELAY} frames + jitter 0-${JITTER} (in order), lead ${LEAD}, seed ${SEED}; link_absent=${ABSENT}; out ${OUT}`);
  if (only.has("contract")) await contract(check);

  if (only.has("pair") || only.has("replay")) {
    const started = performance.now();
    const r = await session("pair", [0, 1], { frames: 3000, shots: [1080, 2400, 3000], probes: [1080, 2400, 3000], record: 0, saveAt: 1200 });
    console.log(`  pair: ${((performance.now() - started) / 1000).toFixed(1)} s; ${[0, 1].map((k) => `seat ${k} ${r.get(k).frames} frames, ${r.get(k).ms.toFixed(1)} ms a frame`).join("; ")}`);
    for (const k of [0, 1]) for (const w of r.get(k).warnings) console.log(`  seat ${k} said: ${w}`);
    const [a, b] = [r.get(0), r.get(1)];
    check([a, b].every((x, k) => x.link.link[0].state === "up" && x.link.link[0].id === k + 1 && x.link.link[0].count === 8), "pair: both links up all along (1 and 2 of 8)",
      `${a.link.link[0].state} ${a.link.link[0].id}/8, ${b.link.link[0].state} ${b.link.link[0].id}/8`);
    check(at(a, 1080)?.entrants === 2 && at(b, 1080)?.entrants === 2 && at(a, 1080)?.cars === 10, "(a) pair: both entered one session (circuit select, 2 entrants on both)",
      `seat 0 ${show(at(a, 1080))}; seat 1 ${show(at(b, 1080))}; pair-seat0/1-f1080.png`);
    const racing = (p) => p?.cars === 16 && p.entrants === 2;
    check(racing(at(a, 2400)) && racing(at(b, 2400)) && racing(at(a, 3000)) && racing(at(b, 3000)),
      "(a) pair: both race, linked (16 cars on the track, 2 entrants: POSITION n/2)",
      `f2400 seat 0 ${show(at(a, 2400))}, seat 1 ${show(at(b, 2400))}; f3000 seat 0 ${show(at(a, 3000))}, seat 1 ${show(at(b, 3000))}; screens pair-seat0-f2400.png, pair-seat1-f2400.png, ...f3000`);

    if (only.has("replay")) {
      const log = join(OUT, "pair-seat0.log.json");
      const recorded = JSON.parse(readFileSync(log, "utf8")).hashes;
      for (const [label, state, from] of [["from the seat state", stateFile(0), 0], ["from its frame-1200 state (a spectator joining)", join(OUT, "pair-seat0-f1200.state"), 1200]]) {
        const { hashes } = await replayIn(log, state, from);
        const frames = Object.keys(recorded).filter((f) => Number(f) > from);
        const same = frames.filter((f) => hashes[f]?.ram === recorded[f].ram && hashes[f]?.screen === recorded[f].screen);
        const differ = frames.filter((f) => !same.includes(f)).slice(0, 3).map((f) => `${f}: ${recorded[f].ram}/${recorded[f].screen} vs ${hashes[f]?.ram}/${hashes[f]?.screen}`);
        check(same.length === frames.length && frames.length > 0, `(d) replay of seat 0's log ${label}: RAM and screen the same every ${EVERY} frames`,
          `${same.length}/${frames.length} checkpoints (${frames[0]}-${frames.at(-1)})${differ.length ? `; differ ${differ.join(", ")}` : `, last ${recorded[frames.at(-1)].ram}/${recorded[frames.at(-1)].screen}`}`);
      }
    }
  }

  if (only.has("trio")) {
    const started = performance.now();
    const r = await session("trio", [0, 1, 2], { frames: 4800, leave: { 1: 2700 }, shots: [1080, 2400, 2640, 3000, 3600, 4200, 4800], probes: [1080, 2400, 2640, 3000, 3600, 4200, 4800] });
    console.log(`  trio: ${((performance.now() - started) / 1000).toFixed(1)} s; ${[0, 1, 2].map((k) => `seat ${k} ${r.get(k).frames} frames, ${r.get(k).ms.toFixed(1)} ms a frame`).join("; ")}`);
    for (const k of [0, 1, 2]) for (const w of r.get(k).warnings) console.log(`  seat ${k} said: ${w}`);
    const [a, b, c] = [r.get(0), r.get(1), r.get(2)];
    const racing = (p) => p?.cars === 16 && p.entrants === 2;
    check(racing(at(a, 2400)) && racing(at(b, 2400)) && racing(at(a, 2640)) && racing(at(b, 2640)), "(a) trio: seats 0 and 1 race, linked (16 cars, 2 entrants)",
      `f2640 seat 0 ${show(at(a, 2640))}, seat 1 ${show(at(b, 2640))}`);
    check(at(c, 2400)?.entrants === 1 && at(c, 2400)?.cars === 10 && at(c, 4200)?.cars === 40 && at(c, 4200)?.entrants === 1,
      "(b) trio: seat 2's START after the race began opens its own session (alone: 1 entrant, then a race of 40 cars)",
      `f2400 ${show(at(c, 2400))}; f4200 ${show(at(c, 4200))}; trio-seat2-f2400.png, trio-seat2-f4200.png`);
    const later = [3000, 3600, 4200, 4800].map((f) => at(a, f));
    const moving = later.every((p, i) => p && (i === 0 || (p.ram !== later[i - 1].ram && p.screen !== later[i - 1].screen)));
    const absentSeen = a.link.star.seats[1];
    check(b.frames === 2700 && racing(at(a, 2640)) && later.every((p) => p?.cars === 16) && moving && a.link.link[0].state === "up",
      "(c) trio: seat 1 left at frame 2700, seat 0's race goes on (still a linked race; its RAM and screen moving, not frozen)",
      `seat 0 at 3000-4800: cars ${later.map((p) => p?.cars).join(" ")}, RAM ${later.map((p) => p?.ram).join(" ")}; seat 1 in seat 0's status: ${JSON.stringify(absentSeen)}; trio-seat0-f3000..f4800.png`);
    check(racing(at(c, 4200)) === false && at(a, 4200)?.entrants === 2, "(b) trio: the two sessions stay apart (seat 2 alone, seat 0's session still its pair)",
      `seat 0 ${show(at(a, 4200))}, seat 2 ${show(at(c, 4200))}`);
  }
  console.log(failures ? `${failures} check(s) FAILED` : "all checks PASSED");
  process.exit(failures ? 1 : 0);
}
