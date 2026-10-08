// Time Crisis II's twin cabinet through the bar's own emulator worker (web/emulator/worker.js,
// linked.js): two workers in their own threads, like two browsers, as seats 0 (Left/Red) and 1
// (Right/Blue) of a `linked` game over a simulated network, and a third watching seat 0's
// board, the way the page (web/index.html) drives them:
//
//   1. seat 0 sits down alone and plays its board unlinked, from timecrs2.state, streaming it;
//   2. seat 1 sits down: both workers get "online" (no state), so both boards start over from
//      their side's start-up state (timecrs2-link<side>.state) and run linked, D ticks apart;
//   3. both players shoot: LINK PLAY at the mode select, then the linked game (mode word 2);
//   4. seat 1 stands up: seat 0 gets "solo" and starts over alone, unlinked.
//
// Checks: both boards reach linked gameplay; every tick's message goes out once, in order, and
// what seat 0's board was handed before each frame (its watch stream) is exactly seat 1's
// message from D ticks earlier (or the start-up state's carried bytes); no waits on a quiet
// network once both run; the watcher's machine has the same RAM as seat 0's at the same frames
// (worker "probe"s); seat 0 plays on unlinked after seat 1 leaves.
//
//   node mame/linked-check.mjs ~/Downloads/timecrs2.zip [states dir, default mame/.cache/roms]
//
// Env: CORE (default mame/dist/mame.mjs: must be the build the states were made with),
// LATENCY one way in ms (default 20), JITTER ms (default 5), ALONE seconds seat 0 plays alone
// (default 8), LINKED seconds of linked play (default 60), AFTER seconds alone again (8).
// Needs web/netplay (make netplay): the worker loads it whatever the game.
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { MessageChannel, Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (!isMainThread) {
  // Enough of a browser worker for worker.js: postMessage/onmessage, and fetch for file URLs.
  globalThis.postMessage = (message, transfer) => parentPort.postMessage(message, transfer);
  // The worker's frame clock (Alarm) wakes itself with messages to itself. A browser runs each
  // as its own task, between the page's messages; Node's MessagePort runs up to a thousand of
  // them in one go, which, with a frame each that takes longer than its 16.7 ms here, holds
  // everything else (the page's messages, the network) back for seconds. One per turn instead.
  globalThis.MessageChannel = class {
    constructor() {
      this.port1 = { onmessage: null };
      this.port2 = { postMessage: (data) => setImmediate(() => this.port1.onmessage?.({ data })) };
    }
  };
  globalThis.onmessage = null;
  parentPort.on("message", (data) => globalThis.onmessage?.({ data }));
  globalThis.fetch = async (url) => {
    try {
      const bytes = await readFile(new URL(url));
      const type = String(url).endsWith(".wasm") ? "application/wasm" : "application/octet-stream";
      return new Response(bytes, { headers: { "content-type": type } });
    } catch {
      return new Response(null, { status: 404 });
    }
  };
  await import(workerData.worker);
} else {
  const { decodeRecords, parseLinkState } = await import("../web/emulator/linked.js");
  const [romPath, statesDir = new URL(".cache/roms", import.meta.url).pathname] = process.argv.slice(2);
  if (!romPath) {
    console.error("usage: node mame/linked-check.mjs <timecrs2.zip> [states dir]");
    process.exit(2);
  }
  const CORE = process.env.CORE ?? new URL("dist/mame.mjs", import.meta.url).pathname;
  const LATENCY = Number(process.env.LATENCY ?? 20);
  const JITTER = Number(process.env.JITTER ?? 5);
  const ALONE = Number(process.env.ALONE ?? 8);
  const LINKED = Number(process.env.LINKED ?? 60);
  const AFTER = Number(process.env.AFTER ?? 8);
  const url = (path) => pathToFileURL(resolve(path)).href;
  const wait = (seconds) => new Promise((done) => setTimeout(done, seconds * 1000));
  const emulatorWorker = () =>
    new Worker(new URL(import.meta.url), { workerData: { worker: new URL("../web/emulator/worker.js", import.meta.url).href } });
  const game = {
    core: url(CORE),
    rom: url(romPath),
    state: url(join(statesDir, "timecrs2.state")),
    gun: true,
    linked: true,
  };
  const carried = [0, 1].map(async (side) => parseLinkState(await readFile(join(statesDir, `timecrs2-link${side}.state`))).carried);
  const problems = [];
  const started = performance.now();
  const at = () => `${((performance.now() - started) / 1000).toFixed(1)} s`;

  /**
   * In order, each `LATENCY ± JITTER` ms after it was sent at the earliest (a reliable, ordered
   * channel). One timer at a time: Node doesn't keep timers of different lengths that come due
   * together in order.
   */
  function channel(deliver) {
    let arrives = 0;
    const queue = [];
    const flush = () => {
      while (queue.length && queue[0].at <= performance.now()) deliver(...queue.shift().args);
      if (queue.length) setTimeout(flush, queue[0].at - performance.now());
    };
    return (...args) => {
      const delay = Math.max(0, LATENCY + (Math.random() * 2 - 1) * JITTER);
      arrives = Math.max(arrives, performance.now() + delay);
      queue.push({ args, at: arrives });
      if (queue.length === 1) setTimeout(flush, arrives - performance.now());
    };
  }

  // The watcher: seat 0's stream, through the room (in order, a little late).
  const watcher = { worker: emulatorWorker(), frames: 0, probes: new Map(), errors: [] };
  watcher.worker.on("message", (message) => {
    if (message.type === "frame") watcher.frames++;
    if (message.type === "ready") seats[0]?.worker.postMessage({ type: "snapshot", to: "watcher" });
    if (message.type === "probe") watcher.probes.set(message.frame, message);
  });
  watcher.worker.on("error", (error) => watcher.errors.push(error.message));
  watcher.worker.postMessage({ type: "start", core: game.core, rom: game.rom, gun: true, linked: true, hold: true });
  const toWatcher = channel((message, transfer) => watcher.worker.postMessage(message, transfer));

  const seats = [];
  /** A player sitting at `seat`, their worker started as the page starts it. */
  function sit(seat, hold) {
    const worker = emulatorWorker();
    const { port1: inside, port2: outside } = new MessageChannel();
    const player = { seat, worker, outside, frames: 0, stats: [], events: [], probes: new Map(), errors: [], sent: [], records: [] };
    worker.on("message", (message) => {
      if (message.type === "frame") player.frames++;
      if (message.type === "netplay" && message.event === "stats") player.stats.push({ ...message, at: performance.now() });
      else if (message.type === "netplay") player.events.push(`${at()} ${message.event}`);
      if (message.type === "probe") player.probes.set(message.frame, message);
      if (message.type === "watch-state") {
        player.records.push({ state: message.stream });
        // Named as the page names it, so the watcher passes over a second state of its stream.
        toWatcher({ type: "watch-state", bytes: message.bytes, stream: `0/${message.stream}` }, [message.bytes.buffer]);
      }
      if (message.type === "watch-link") {
        player.records.push(...decodeRecords(message.records).map((record) => ({ ...record, stream: message.stream })));
        toWatcher({ type: "watch-link", records: message.records }, [message.records.buffer]);
      }
      if (message.type === "watch-inputs") toWatcher({ type: "watch-inputs", frame: message.frame, inputs: message.inputs }, [message.inputs.buffer]);
    });
    worker.on("error", (error) => player.errors.push(error.message));
    const send = channel((to, packet) => seats[to] && !seats[to].left && seats[to].outside.postMessage([seat, packet], [packet]));
    // The network: [epoch u16][tick u32][link bytes] for a linked game; kept to check.
    outside.on("message", ([to, packet]) => {
      const bytes = new Uint8Array(packet);
      const view = new DataView(packet);
      player.sent.push({ epoch: view.getUint16(0, true), tick: view.getUint32(2, true), bytes: bytes.slice(6) });
      send(to, packet);
    });
    worker.postMessage({ type: "start", ...game, linkState: url(join(statesDir, `timecrs2-link${seat}.state`)), seat, port: inside, hold }, [inside]);
    worker.postMessage({ type: "input", input: (128 << 16) | (128 << 24) });
    seats[seat] = player;
    return player;
  }

  /**
   * Pulls the trigger now and then: at the LINK PLAY panel of the mode select first (the game
   * picks what was shot; SOLO ONLY is just below it), then about the middle.
   */
  function shoot(player, from, every) {
    let shots = 0;
    return setInterval(() => {
      if (performance.now() < from) return;
      const x = shots < 4 ? 128 : 100 + Math.floor(Math.random() * 56);
      const y = shots++ < 4 ? 90 : 100 + Math.floor(Math.random() * 56);
      const aim = ((x << 16) | (y << 24)) >>> 0;
      player.worker.postMessage({ type: "input", input: aim | 1 });
      setTimeout(() => player.worker.postMessage({ type: "input", input: aim }), 120);
    }, every);
  }

  /** Asks seat 0's machine and the watcher's for their RAM at a frame a little ahead of seat 0. */
  const probed = [];
  function probe() {
    const last = seats[0].stats.at(-1);
    if (!last) return;
    const frame = last.frame + 90;
    seats[0].worker.postMessage({ type: "probe", at: frame });
    watcher.worker.postMessage({ type: "probe", at: frame });
    probed.push(frame);
  }

  // 1. Alone.
  const red = sit(0, false);
  red.worker.postMessage({ type: "stream", on: true });
  await wait(ALONE);
  console.log(`${at()} seat 0 alone: ${red.frames} frames shown, watcher ${watcher.frames}`);

  // 2. Seat 1 sits down: both start over linked (index.html #handOver, linked).
  const blue = sit(1, true);
  const roundTrip = 2 * LATENCY;
  for (const player of [red, blue]) player.worker.postMessage({ type: "online", epoch: 1, seats: [0, 1], roundTrip });
  const linkedAt = performance.now();
  red.framesAtLink = red.frames;
  // 3. Both shoot: LINK PLAY, then the game. The red player first.
  const shooting = [shoot(red, linkedAt + 6000, 900), shoot(blue, linkedAt + 6500, 1100)];
  const probing = setInterval(probe, 5000);
  for (let s = 0; s < LINKED; s += 10) {
    await wait(10);
    for (const player of [red, blue]) {
      const st = player.stats.at(-1);
      console.log(`${at()} seat ${player.seat}: frame ${st?.frame} fps ${st?.fps} delay ${st?.delay} stalls ${st?.stalls} ` +
        `ahead ${st?.framesAhead} link ${JSON.stringify(st?.link)}${st?.lost ? " LOST" : ""}`);
    }
  }
  clearInterval(probing);
  shooting.forEach(clearInterval);
  await wait(4); // the last probes

  // 4. Seat 1 stands up: seat 0 starts over alone.
  blue.left = true;
  await blue.worker.terminate();
  red.worker.postMessage({ type: "solo" });
  const before = red.frames;
  await wait(AFTER);
  red.worker.postMessage({ type: "probe" });
  await wait(4);
  const alone = red.probes.get(-1);
  const afterFrames = red.frames - before;
  await red.worker.terminate();
  await watcher.worker.terminate();

  // The checks.
  const linkedStats = (player) => player.stats.filter((st) => st.link);
  for (const player of [red, blue]) {
    const stats = linkedStats(player);
    const modes = stats.map((st) => st.link.mode);
    const gameAt = stats.find((st) => st.link.mode === 2);
    const delay = stats.at(-1)?.delay;
    // Quiet network: no waits once both run (the first few seconds have the other starting).
    // Node runs a game frame slower than its 16.7 ms, so both boards run flat out, and the one
    // that's a little faster waits for the other now and then: only a long wait is a stall.
    const running = stats.filter((st) => st.at - linkedAt > 8000);
    const waits = running.reduce((n, st) => n + st.stalls, 0);
    const longest = Math.max(0, ...running.map((st) => st.stallMs));
    const fps = running.map((st) => st.fps);
    console.log(`seat ${player.seat}: delay ${delay}; linked gameplay ${gameAt ? `from frame ${gameAt.frame}` : "NEVER"}; ` +
      `modes seen ${[...new Set(modes)].join(",")}; waits after the start ${waits} (longest ${longest} ms); fps ${Math.min(...fps)}-${Math.max(...fps)}; ` +
      `events ${player.events.join(", ")}; errors ${player.errors.join(" | ") || "none"}`);
    if (!gameAt) problems.push(`seat ${player.seat} never got to linked gameplay`);
    if (stats.some((st) => st.lost)) problems.push(`seat ${player.seat} lost the link`);
    if (longest > 250) problems.push(`seat ${player.seat} stalled ${longest} ms on a quiet network`);
    if (player.errors.length) problems.push(`seat ${player.seat}: ${player.errors[0]}`);
    // Every tick's message, once, in order, from tick 0.
    const ticks = player.sent.filter((p) => p.epoch === 1 && p.tick !== 0xffffffff).map((p) => p.tick);
    const hellos = player.sent.filter((p) => p.epoch === 1 && p.tick === 0xffffffff).map((p) => p.bytes[0]);
    if (hellos.length !== 1) problems.push(`seat ${player.seat} said hello ${hellos.length} times`);
    if (ticks.some((tick, i) => tick !== i)) problems.push(`seat ${player.seat}'s messages aren't ticks 0, 1, 2, ... once each`);
    player.ticks = ticks.length;
  }
  // What seat 0's board was handed before each frame: seat 1's message from D ticks before.
  {
    const D = linkedStats(red).at(-1)?.delay;
    const linkedStream = red.records.filter((r) => r.frame !== undefined && r.stream === Math.max(...red.records.filter((x) => x.frame !== undefined).map((x) => x.stream)));
    const theirs = new Map(blue.sent.filter((p) => p.epoch === 1 && p.tick !== 0xffffffff).map((p) => [p.tick, p.bytes]));
    const red0 = (await carried[0]) ?? [];
    let checked = 0;
    let wrong;
    for (const { frame, bytes } of linkedStream) {
      const expected = frame >= D ? theirs.get(frame - D) : frame < red0.length ? red0[frame] : new Uint8Array(0);
      if (!expected) continue; // past what seat 1 sent before it left
      checked++;
      if (Buffer.compare(Buffer.from(bytes), Buffer.from(expected)) !== 0) wrong ??= frame;
    }
    const total = linkedStream.reduce((n, r) => n + r.bytes.length, 0);
    console.log(`seat 0's stream: ${linkedStream.length} linked frames, ${total} link bytes handed in; ${checked} checked against ` +
      `seat 1's messages D = ${D} ticks earlier: ${wrong === undefined ? "all the same" : `DIFFERENT at frame ${wrong}`}`);
    if (wrong !== undefined || checked < 600) problems.push(`seat 0 wasn't handed seat 1's bytes D ticks later (${checked} checked, first wrong ${wrong})`);
  }
  // The watcher's machine against seat 0's, at the same frames.
  {
    const results = probed.map((frame) => [frame, red.probes.get(frame), watcher.probes.get(frame)]).filter(([, a, b]) => a && b);
    const same = results.filter(([, a, b]) => a.hash === b.hash);
    console.log(`watcher: ${watcher.frames} frames shown; RAM at ${results.length} of ${probed.length} probed frames: ` +
      `${same.length} the same as seat 0's${results.length - same.length ? `, DIFFERENT at ${results.filter(([, a, b]) => a.hash !== b.hash).map(([f]) => f).join(", ")}` : ""}` +
      `; modes ${results.map(([, a, b]) => `${a.link?.mode}/${b.link?.mode}`).join(" ")}`);
    if (!results.length || same.length !== results.length) problems.push("the watcher's machine isn't seat 0's");
    if (watcher.errors.length) problems.push(`watcher: ${watcher.errors[0]}`);
  }
  console.log(`after seat 1 left: seat 0 showed ${afterFrames} frames in ${AFTER} s; its link: ${JSON.stringify(alone?.link)}`);
  if (afterFrames < AFTER * 20) problems.push("seat 0 stopped after seat 1 left");
  if (!alone || alone.link?.linked !== 0) problems.push("seat 0 didn't start over unlinked");
  console.log(problems.length ? `FAIL: ${problems.join("; ")}` : "linked check passed");
  process.exit(problems.length ? 1 : 0);
}
