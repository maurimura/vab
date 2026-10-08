// Plays a game online between emulator workers (web/emulator/worker.js, each in its own thread
// like separate browsers) over a simulated network, with random buttons for everyone. Player 1
// starts alone, the others drop in one by one, then player 2 leaves; each time player 1 hands
// the game as it is to everyone, as the page does. GGRS compares the machines every 60 frames:
// any "desync" means they drifted apart.
//
// Player 1 also streams the game to a watcher (another worker, its stream arriving in order
// like through the room), and a machine here plays the same stream: every 1.5 s player 1 sends
// a state as for a new watcher, which must match what that machine got to by playing the
// stream up to there. With PLAYERS=1 (a one-player game, Out Run) player 1 only plays alone,
// streaming, for twice as long.
//
//   node emulator/netplay-check.mjs <core.mjs> <rom.zip> [state] [bios.zip ...]
//   PLAYERS=4 node emulator/netplay-check.mjs emulator/dist/konami/fbneo.mjs ~/Downloads/ssriders.zip emulator/dist/ssriders.state
//   PLAYERS=1 WHEEL=1 node emulator/netplay-check.mjs emulator/dist/outrun/fbneo.mjs ~/Downloads/outrun.zip emulator/dist/outrun.state
//
// Env: PLAYERS (2), SECONDS per stage (10), LATENCY one way in ms (40), JITTER ms (10), LOSS
// fraction (0.02), TURNS=1 for turn-based games, GUN=1 for lightgun games, WHEEL=1 for a driving
// game (its wheel as assets/games.ron has it for the ROM set: the arrows turn it), BREAK=1 hands
// the last player the start-up state instead of the game (must desync). Needs the netplay
// module built: make netplay.
import { readFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { MessageChannel, Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (!isMainThread) {
  // Enough of a browser worker for worker.js: postMessage/onmessage, and fetch for file URLs.
  globalThis.postMessage = (message, transfer) => parentPort.postMessage(message, transfer);
  // The worker's frame clock (Alarm) wakes itself with messages to itself. A browser runs each
  // as its own task, between the page's messages; Node's MessagePort runs up to a thousand of
  // them in one go, holding everything else back while frames take their time. One per turn.
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
  const { Core } = await import("../web/emulator/libretro.js");
  const [corePath, romPath, statePath, ...biosPaths] = process.argv.slice(2);
  const PLAYERS = Number(process.env.PLAYERS ?? 2);
  const SECONDS = Number(process.env.SECONDS ?? 10);
  const LATENCY = Number(process.env.LATENCY ?? 40);
  const JITTER = Number(process.env.JITTER ?? 10);
  const LOSS = Number(process.env.LOSS ?? 0.02);
  const url = (path) => pathToFileURL(resolve(path)).href;
  const wait = (seconds) => new Promise((done) => setTimeout(done, seconds * 1000));
  /** A game's `wheel` in assets/games.ron, as the page passes it on: { lock, back, curve, span }. */
  async function gameWheel(rom) {
    const games = await readFile(new URL("../assets/games.ron", import.meta.url), "utf8");
    const line = games.split("\n").find((line) => line.includes(`rom: "${rom}"`)) ?? "";
    const found = line.match(/wheel: Some\(\(lock: ([\d.]+), back: ([\d.]+), curve: ([\d.]+)(?:, span: \((\d+), (\d+)\))?\)\)/);
    if (!found) throw new Error(`${rom}: no wheel in assets/games.ron`);
    const [lock, back, curve, from = 0, to = 32767] = found.slice(1).map((n) => (n === undefined ? n : Number(n)));
    return { lock, back, curve, span: [from, to] };
  }
  // A driving game's wheel: the arrows turn it in the workers, and the machine here reads it
  // from the stream's inputs as theirs do.
  const wheel = process.env.WHEEL === "1" ? await gameWheel(basename(romPath, ".zip")) : undefined;

  const players = [];
  const seated = () => players.filter((player) => !player.left);
  const emulatorWorker = () =>
    new Worker(new URL(import.meta.url), {
      workerData: { worker: new URL("../web/emulator/worker.js", import.meta.url).href },
    });

  // The watcher, and the machine here that checks player 1's stream.
  const WATCHER = 77;
  const watcher = { worker: emulatorWorker(), frames: 0, errors: [], arrives: 0 };
  watcher.worker.on("message", (message) => {
    if (message.type === "frame") watcher.frames++;
    if (message.type === "ready") players[0]?.worker.postMessage({ type: "snapshot", to: WATCHER });
  });
  watcher.worker.on("error", (error) => watcher.errors.push(error.message));
  watcher.worker.postMessage({
    type: "start",
    core: url(corePath),
    rom: url(romPath),
    files: biosPaths.map(url),
    turns: process.env.TURNS === '1',
    gun: process.env.GUN === '1',
    wheel,
    hold: true,
  });
  // In order, each LATENCY ± JITTER ms after the one before at the earliest. One timer at a
  // time: Node doesn't keep timers of different lengths that come due together in order.
  const queue = [];
  const deliver = () => {
    while (queue.length && queue[0].at <= performance.now()) {
      const { message, transfer } = queue.shift();
      watcher.worker.postMessage(message, transfer);
    }
    if (queue.length) setTimeout(deliver, queue[0].at - performance.now());
  };
  const toWatcher = (message, transfer) => {
    const delay = Math.max(0, LATENCY + (Math.random() * 2 - 1) * JITTER);
    watcher.arrives = Math.max(watcher.arrives, performance.now() + delay);
    queue.push({ message, transfer, at: watcher.arrives });
    if (queue.length === 1) setTimeout(deliver, watcher.arrives - performance.now());
  };
  const { default: createFBNeo } = await import(url(corePath));
  const boot = async () => {
    const core = await Core.create(createFBNeo, { onFrame() {}, onAudio() {} });
    core.netplay = true;
    core.turns = process.env.TURNS === '1';
    core.gun = process.env.GUN === '1';
    core.wheel = wheel;
    for (const path of biosPaths) core.addFile(basename(path), await readFile(path));
    core.loadGame(basename(romPath), await readFile(romPath));
    // Drawn, as a watcher's worker draws every frame it plays: Out Run's sprite chip writes
    // back to its RAM as it draws.
    core.present = true;
    return core;
  };
  const check = { core: await boot(), scratch: await boot(), stream: -1, frame: 0, matched: 0, mismatched: 0, states: 0 };
  const sameRam = (a, b) => Buffer.from(a.systemRam()).equals(Buffer.from(b.systemRam()));
  // States leave the worker deflated ("vabz", worker.js pack); the watcher's worker inflates
  // its own copy, this check inflates the one it loads.
  const PACKED = Uint8Array.of(0x76, 0x61, 0x62, 0x7a);
  async function unpack(bytes) {
    if (bytes.length < PACKED.length || PACKED.some((b, i) => bytes[i] !== b)) return bytes;
    const stream = new Blob([bytes.subarray(PACKED.length)]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }
  // Messages are handled one after another (a state inflates asynchronously).
  let streaming = Promise.resolve();
  async function streamed(message) {
    if (message.type === "watch-state") {
      const frame = new DataView(message.bytes.buffer, message.bytes.byteOffset).getUint32(0, true);
      check.states++;
      if (message.stream !== check.stream) {
        // A new stream (a new session): start over from it.
        check.core.unserialize(await unpack(message.bytes.subarray(4)));
        Object.assign(check, { stream: message.stream, frame });
      } else if (frame === check.frame) {
        check.scratch.unserialize(await unpack(message.bytes.subarray(4)));
        if (sameRam(check.core, check.scratch)) check.matched++;
        else check.mismatched++;
      } else {
        check.mismatched++;
        console.log(`state for frame ${frame}, but the stream got to ${check.frame}`);
      }
      // Named as the page names it, so the watcher passes over a second state of its stream.
      if (message.to === undefined || message.to === WATCHER) {
        toWatcher({ type: "watch-state", bytes: message.bytes, stream: `0/${message.stream}` }, [message.bytes.buffer]);
      }
    }
    if (message.type === "watch-inputs") {
      if (message.stream === check.stream && message.frame === check.frame) {
        const ports = check.core.inputs;
        for (let i = 0; i < message.inputs.length; i += 4) {
          ports.set(message.inputs.subarray(i, i + 4));
          check.core.run();
        }
        check.frame += message.inputs.length / 4;
      }
      toWatcher({ type: "watch-inputs", frame: message.frame, inputs: message.inputs }, [message.inputs.buffer]);
    }
  }
  const snapshots = setInterval(() => players[0]?.worker.postMessage({ type: "snapshot", to: 1234 }), 1500);

  // One worker per seat, as the page starts it.
  function sit(seat, hold) {
    const worker = emulatorWorker();
    const { port1: inside, port2: outside } = new MessageChannel();
    const player = { seat, worker, outside, frames: 0, events: {}, captures: new Map(), errors: [] };
    worker.on("message", (message) => {
      if (message.type === "frame") player.frames++;
      if (message.type === "captured") player.captures.set(message.epoch, message.state);
      if (message.type === "netplay" && message.event === "stats") player.stats = message;
      else if (message.type === "netplay") player.events[message.event] = (player.events[message.event] ?? 0) + 1;
      if (message.type === "watch-state" || message.type === "watch-inputs") streaming = streaming.then(() => streamed(message));
    });
    worker.on("error", (error) => player.errors.push(error.message));
    // The network: each packet arrives LATENCY ± JITTER ms later, or not at all.
    outside.on("message", ([to, packet]) => {
      const other = players[to];
      if (!other || other.left || Math.random() < LOSS) return;
      const delay = Math.max(0, LATENCY + (Math.random() * 2 - 1) * JITTER);
      setTimeout(() => other.left || other.outside.postMessage([seat, packet], [packet]), delay);
    });
    worker.postMessage(
      {
        type: "start",
        core: url(corePath),
        rom: url(romPath),
        files: biosPaths.map(url),
        state: statePath && url(statePath),
        seat,
        turns: process.env.TURNS === '1',
        gun: process.env.GUN === '1',
        wheel,
        port: inside,
        hold,
      },
      [inside],
    );
    players[seat] = player;
    return player;
  }

  // What the page does when the players change: the lowest seat playing captures its machine
  // and everyone starts a new session from that capture.
  let epoch = 0;
  async function handOver(breakLast) {
    const [coordinator, ...others] = seated();
    if (!others.length) return coordinator.worker.postMessage({ type: "solo" });
    epoch++;
    coordinator.worker.postMessage({ type: "capture", epoch });
    while (!coordinator.captures.has(epoch)) await wait(0.05);
    const seats = seated().map((player) => player.seat);
    const state = coordinator.captures.get(epoch);
    coordinator.worker.postMessage({ type: "online", epoch, seats });
    for (const player of others) {
      const given = breakLast && player === others.at(-1) ? await readFile(statePath) : state;
      player.worker.postMessage({ type: "online", epoch, seats, state: given });
    }
  }

  // Button mashing: a random mask held for 2-20 frames, per player, and somewhere to aim (the
  // high 16 bits, which only a lightgun game reads; the rest carry it all the same).
  const USABLE = 0b0000_1111_1111_1011; // B Y START UP DOWN LEFT RIGHT A X L R
  const mashing = setInterval(() => {
    for (const player of seated()) {
      if (--player.hold > 0) continue;
      player.hold = 2 + Math.floor(Math.random() * 18);
      const buttons = Math.floor(Math.random() * 0x10000) & USABLE;
      const aim = Math.floor(Math.random() * 0x10000) << 16;
      player.worker.postMessage({ type: "input", input: (buttons | aim) >>> 0 });
    }
  }, 1000 / 60);

  const report = (stage) => {
    for (const player of players) {
      const { ping, delay, rollback } = player.stats ?? {};
      console.log(stage, JSON.stringify({ seat: player.seat, left: player.left, frames: player.frames, ping, delay, rollback, ...player.events, errors: player.errors }));
    }
    const { matched, mismatched, states } = check;
    console.log(stage, JSON.stringify({ watcher: watcher.frames, states, matched, mismatched, errors: watcher.errors }));
  };
  sit(0, false).worker.postMessage({ type: "stream", on: true });
  await wait(SECONDS / 2);
  for (let seat = 1; seat < PLAYERS; seat++) {
    sit(seat, true);
    await handOver(process.env.BREAK && seat === PLAYERS - 1);
    await wait(SECONDS);
    report(`${seat + 1} players:`);
  }
  if (PLAYERS > 1) {
    const leaving = players[1];
    leaving.left = true;
    await leaving.worker.terminate();
    await handOver();
  }
  const before = seated().map((player) => player.frames);
  await wait(PLAYERS > 1 ? SECONDS : SECONDS * 2);
  report(PLAYERS > 1 ? "player 2 left:" : "1 player:");
  const stuck = seated().some((player, i) => player.frames - before[i] < SECONDS * 30);
  if (stuck) players[0].errors.push(PLAYERS > 1 ? "stopped after player 2 left" : "stopped");
  clearInterval(mashing);
  clearInterval(snapshots);
  for (const player of seated()) await player.worker.terminate();
  await watcher.worker.terminate();
  if (check.mismatched || !check.matched || watcher.errors.length || !watcher.frames) {
    players[0].errors.push("the stream doesn't match the game");
  }
  const desynced = players.some((player) => player.events.desync);
  const broken = players.some((player) => player.errors.length || !player.frames);
  process.exitCode = !broken && desynced === Boolean(process.env.BREAK) ? 0 : 1;
  console.log(process.exitCode ? "FAIL" : "ok");
}
