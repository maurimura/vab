// Virtua Tennis at its cabinet in real browsers: headless Chromes of our own (never the user's),
// over CDP, in the bar from `make dev BUCKET=local`. A sits down (E) and plays alone, from the
// start-up state through Start and the menus; B sits at the same cabinet in the same room, so
// both play in lockstep (A hands B the machine, 76 MB deflated to ~20 MB); C watches (F). Prints
// each worker's stats (fps, a frame's cost, waits, input delay, ping), fails on a desync or a
// machine that stops showing frames, and saves a screenshot of each browser.
//
//   node flycast/lab.mjs [--site=http://localhost:8787] [--seconds=20] [--out=flycast/.cache/lab]
//        [--delay=40 --jitter=15]
//
// With --delay, every game packet a page sends over its direct WebRTC link leaves that many
// ms later (± --jitter), so the players play at a chosen ping on one machine, as
// supermodel/harness/online-lab.mjs does; the stats then print every 2 s for --seconds after
// each change of players, to see how the game settles.
//
// Needs the site running with the Flycast core, vtennisg.zip, vtennisg/gds-0011.chd and
// vtennisg.state in its bucket (make flycast, make upload-rom ..., emulator/snapshot.mjs), a
// cabinet whose game is "vtennisg" on its map, and a local build of the client (it uses the
// client's test hooks, client/src/testing.rs).
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
const SITE = argv.site ?? "http://localhost:8787";
const SECONDS = Number(argv.seconds ?? 20);
const DELAY = Number(argv.delay ?? 0);
const JITTER = Number(argv.jitter ?? 0);
const OUT = argv.out ?? new URL(".cache/lab", import.meta.url).pathname;
const ROOM = `vtennis-${Date.now().toString(36)}`;
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PROFILES = mkdtempSync(join(tmpdir(), "vtennis-lab-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

// In each emulator worker: its netplay events and frames, kept for us to read.
const WORKER_PROBE = `(() => {
  const L = globalThis.__lab = { stats: [], events: [], frames: 0 };
  const post = globalThis.postMessage;
  globalThis.postMessage = function (m, transfer) {
    if (m && m.type === "netplay" && m.event === "stats") L.stats.push({ ...m, at: performance.timeOrigin + performance.now() });
    else if (m && m.type === "netplay") L.events.push(m.event);
    else if (m && m.type === "frame") L.frames++;
    return post.call(this, m, transfer);
  };
})();`;

class CDP {
  #ws; #next = 1; #pending = new Map(); handlers = [];
  static async connect(url) {
    const c = new CDP();
    c.#ws = new WebSocket(url);
    await new Promise((resolve, reject) => { c.#ws.onopen = resolve; c.#ws.onerror = reject; });
    c.#ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id) {
        const p = c.#pending.get(msg.id);
        c.#pending.delete(msg.id);
        msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result);
      } else for (const h of c.handlers) h(msg);
    };
    return c;
  }
  send(method, params = {}, sessionId) {
    return new Promise((resolve, reject) => {
      const id = this.#next++;
      this.#pending.set(id, { resolve, reject });
      this.#ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  }
  close() { this.#ws.close(); }
}

class Browser {
  workers = [];
  errors = [];
  constructor(name) { this.name = name; }
  async launch() {
    const port = 9800 + Math.floor(Math.random() * 150);
    const profile = mkdtempSync(join(PROFILES, `${this.name}-`));
    this.chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run",
      "--window-size=1280,800", "--autoplay-policy=no-user-gesture-required", "--use-angle=metal", "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream", "about:blank"], { stdio: "ignore" });
    let version;
    for (let i = 0; i < 50 && !version; i++) {
      try { version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json(); } catch { await sleep(200); }
    }
    if (!version) throw new Error(`Chrome ${this.name} did not come up`);
    this.cdp = await CDP.connect(version.webSocketDebuggerUrl);
    const { targetId } = await this.cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await this.cdp.send("Target.attachToTarget", { targetId, flatten: true });
    this.page = sessionId;
    this.cdp.handlers.push((msg) => this.#event(msg));
    await this.cdp.send("Page.enable", {}, sessionId);
    await this.cdp.send("Runtime.enable", {}, sessionId);
    // The welcome card would eat the first key; and the injected ping: each packet sent over
    // the direct link (not the 6-byte "vabp" round-trip pings) leaves DELAY ± JITTER ms later,
    // in order.
    await this.cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `document.cookie = "welcomed=1; path=/";
      (() => {
        const delay = ${DELAY}, jitter = ${JITTER};
        if (!delay) return;
        const isPing = (d) => { if (!(d instanceof ArrayBuffer) || d.byteLength !== 6) return false; const b = new Uint8Array(d); return b[0] === 0x76 && b[1] === 0x61 && b[2] === 0x62 && b[3] === 0x70; };
        const send = RTCDataChannel.prototype.send;
        let due = 0;
        RTCDataChannel.prototype.send = function (data) {
          if (isPing(data)) return send.call(this, data);
          due = Math.max(due, performance.now() + Math.max(0, delay + (Math.random() * 2 - 1) * jitter));
          setTimeout(() => { try { send.call(this, data); } catch {} }, due - performance.now());
        };
      })();` }, sessionId);
    await this.cdp.send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, sessionId);
  }
  async #event(msg) {
    if (msg.method === "Target.attachedToTarget") {
      const { sessionId, targetInfo, waitingForDebugger } = msg.params;
      if (targetInfo.type === "worker" && targetInfo.url.includes("emulator/worker.js")) {
        await this.cdp.send("Runtime.enable", {}, sessionId);
        if (waitingForDebugger) await this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId);
        for (let i = 0; i < 100; i++) {
          const { result } = await this.cdp.send("Runtime.evaluate", { expression: "typeof postMessage === 'function'", returnByValue: true }, sessionId);
          if (result?.value) break;
          await sleep(2);
        }
        await this.cdp.send("Runtime.evaluate", { expression: WORKER_PROBE }, sessionId);
        this.workers.push(sessionId);
        return;
      }
      if (waitingForDebugger) this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId).catch(() => {});
    }
    if (msg.method === "Runtime.exceptionThrown") {
      const where = this.workers.includes(msg.sessionId) ? "worker" : "page";
      const text = (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text ?? "").split("\n")[0];
      this.errors.push(`${where}: ${text}`);
      console.error(`[${this.name}] ${where} error:`, text);
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "log" && argv.verbose) {
      console.log(`[${this.name}] ${this.workers.includes(msg.sessionId) ? "worker" : "page"}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 200));
    }
    if (msg.method === "Runtime.consoleAPICalled" && ["warning", "error"].includes(msg.params.type)) {
      const where = this.workers.includes(msg.sessionId) ? "worker" : "page";
      const text = msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 200);
      if (msg.params.type === "error") this.errors.push(`${where}: ${text}`);
      console.error(`[${this.name}] ${where} ${msg.params.type}:`, text);
    }
  }
  async eval(expression, sessionId = this.page) {
    const { result, exceptionDetails } = await this.cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  }
  state() { return this.eval("window.vab ? JSON.stringify(window.vab.state()) : null").then((s) => (s ? JSON.parse(s) : null)); }
  lab() { return this.eval("JSON.stringify(globalThis.__lab)", this.workers.at(-1)).then(JSON.parse); }
  async key(code, key, vk, hold = 120) {
    await this.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode: vk }, this.page);
    await sleep(hold);
    await this.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk }, this.page);
  }
  async click(x, y) {
    for (const type of ["mousePressed", "mouseReleased"]) await this.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 }, this.page);
  }
  async screenshot(file) {
    const { data } = await this.cdp.send("Page.captureScreenshot", { format: "png" }, this.page);
    writeFileSync(file, Buffer.from(data, "base64"));
  }
  close() { try { this.cdp.close(); } catch {} try { this.chrome.kill(); } catch {} }
}

const browsers = [];
process.on("exit", () => browsers.forEach((b) => b.close()));
process.on("SIGINT", () => process.exit(1));

async function until(what, test, timeout = 60000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const value = await test().catch(() => undefined);
    if (value) return value;
    await sleep(250);
  }
  throw new Error(`gave up waiting for ${what}`);
}

/** A Chrome in the bar, at the Virtua Tennis cabinet: E sits, F watches. */
async function arrive(name, key, room = ROOM) {
  const b = new Browser(name);
  browsers.push(b);
  await b.launch();
  await b.cdp.send("Page.navigate", { url: `${SITE}/?room=${room}` }, b.page);
  await until(`${name}: the bar`, async () => (await b.state())?.mode === "Walking", 120000);
  await b.click(640, 400);
  await b.key("ShiftLeft", "Shift", 16);
  await b.eval(`window.vab.goTo("vtennisg")`);
  await until(`${name}: the cabinet`, async () => (await b.state())?.go_to?.cell);
  await sleep(400);
  await b.key(key === "E" ? "KeyE" : "KeyF", key.toLowerCase(), key.charCodeAt(0));
  await until(`${name}: the game`, async () => (await b.state())?.mode === "Playing", 60000);
  await until(`${name}: its worker`, async () => b.workers.length > 0);
  // The core, ROM, disc and state download (75 MB), then the machine shows frames. Joining a
  // game: the handover, the warm-up, the session; meanwhile, what everyone's machine is doing.
  const started = Date.now();
  let reported = 0;
  await until(`${name}: frames`, async () => {
    const frames = (await b.lab()).frames;
    if (Date.now() - started > reported + 5000) {
      reported = Date.now() - started;
      const others = await Promise.all(browsers.filter((o) => o !== b).map(async (o) => `${o.name} ${(await o.lab().catch(() => ({ frames: "?" }))).frames}`));
      console.log(`${at()} waiting for ${name}'s frames: ${name} ${frames}, ${others.join(", ")} | "${await status(b)}"`);
    }
    return frames > 10;
  }, 120000);
  return b;
}

const status = async (b) => (await b.state())?.status ?? "";
const t0 = Date.now();
const at = () => `${((Date.now() - t0) / 1000).toFixed(0).padStart(4)} s`;
/** Frames shown a second by `b`'s worker over `ms`, from the worker's own count. */
async function framesPerSecond(b, ms) {
  const before = (await b.lab()).frames;
  await sleep(ms);
  return (((await b.lab()).frames - before) * 1000) / ms;
}
/** The stats of `b`'s worker since `since` (Date.now()), summed up. */
async function phase(b, since) {
  const stats = (await b.lab()).stats.filter((s) => s.at >= since);
  if (!stats.length) return "no stats";
  const sorted = (key) => stats.map((s) => s[key]).sort((x, y) => x - y);
  const fps = sorted("fps");
  const runs = sorted("runMs");
  return `${stats.length} s: fps ${fps[0]}-${fps.at(-1)} (median ${fps[fps.length >> 1]}), a frame ${runs[0]}-${runs.at(-1)} ms ` +
    `(median ${runs[runs.length >> 1]}), waits ${stats.reduce((n, s) => n + s.stalls, 0)} (longest ${Math.max(...stats.map((s) => s.stallMs))} ms), ` +
    `delay ${stats.at(-1).delay}, ping ${stats.at(-1).ping} ms, frame ${stats.at(-1).frame}`;
}
/** Every 2 s for `seconds`: each player's latest second of stats, to see the game settle. */
async function timeline(players, seconds) {
  const seen = new Map();
  for (let t = 0; t < seconds; t += 2) {
    await sleep(2000);
    const lines = [];
    for (const b of players) {
      const st = (await b.lab().catch(() => undefined))?.stats.at(-1);
      if (!st || seen.get(b) === st.at) continue;
      seen.set(b, st.at);
      lines.push(`${b.name} fps ${String(st.fps).padStart(2)} frame ${String(st.runMs).padStart(4)} ms waits ${st.stalls} (${st.stallMs} ms) delay ${st.delay} ping ${st.ping}`);
    }
    if (lines.length) console.log(`${at()} ${lines.join(" | ")}`);
  }
}
// The page's keys (client/src/emulator.rs): 1 = Start, Z = SHOT1, X = SHOT2, the arrows.
const press = (b, which) => ({
  start: () => b.key("Digit1", "1", 49, 150),
  shot1: () => b.key("KeyZ", "z", 90, 150),
  shot2: () => b.key("KeyX", "x", 88, 150),
  left: () => b.key("ArrowLeft", "ArrowLeft", 37, 300),
  right: () => b.key("ArrowRight", "ArrowRight", 39, 300),
})[which]().catch(() => {});

let failed = false;
const fail = (why) => { failed = true; console.error(`${at()} FAIL: ${why}`); };

// A alone: the start-up state's screen, then Start and the menus into a match.
const A = await arrive("A", "E");
console.log(`${at()} A sat down: "${await status(A)}"`);
await sleep(2000);
await A.screenshot(join(OUT, "A-start.png"));
console.log(`${at()} A alone at the start-up state: ${(await framesPerSecond(A, 5000)).toFixed(1)} frames a second`);
await press(A, "start");
await sleep(2500);
for (let i = 0; i < 8; i++) { await press(A, "start"); await sleep(600); await press(A, "shot1"); await sleep(1400); }
const rally = setInterval(() => { press(A, "shot1"); setTimeout(() => press(A, Math.random() < 0.5 ? "left" : "right"), 400); }, 1200);
await sleep(Math.max(0, SECONDS - 10) * 1000);
// Alone there is no session, so no stats: the frames shown say how it runs.
console.log(`${at()} A alone, in the game: ${(await framesPerSecond(A, 10000)).toFixed(1)} frames a second | "${await status(A)}"`);
await A.screenshot(join(OUT, "A-alone.png"));

// B sits down: A hands over the machine, both play in lockstep.
const B = await arrive("B", "E");
console.log(`${at()} B sat down: "${await status(B)}"`);
await until("both in a session", async () => {
  const labs = await Promise.all([A, B].map((b) => b.lab()));
  return labs.every((lab) => lab.stats.at(-1)?.ping !== undefined);
}, 90000);
let since = Date.now() + 1000;
const rallyB = setInterval(() => { press(B, "shot1"); setTimeout(() => press(B, Math.random() < 0.5 ? "left" : "right"), 600); }, 1300);
if (DELAY) await timeline([A, B], SECONDS);
else await sleep(SECONDS * 1000);
for (const b of [A, B]) console.log(`${at()} ${b.name}, two players in lockstep: ${await phase(b, since)} | "${await status(b)}"`);
const [framesA, framesB] = await Promise.all([A, B].map((b) => framesPerSecond(b, 5000)));
console.log(`${at()} frames a second: A ${framesA.toFixed(1)}, B ${framesB.toFixed(1)}`);
if (framesA < 30 || framesB < 30) fail("a player's machine shows fewer than 30 frames a second online");
for (const b of [A, B]) {
  const { events } = await b.lab();
  if (events.includes("desync")) fail(`${b.name}'s machine desynced`);
}

// C watches.
const C = await arrive("C", "F");
console.log(`${at()} C watching: "${await status(C)}"`);
await sleep(5000);
console.log(`${at()} C watching: ${(await framesPerSecond(C, 5000)).toFixed(1)} frames a second | "${await status(C)}"`);
if (DELAY) await timeline([A, B], Math.max(0, SECONDS - 10));
else await sleep(Math.max(0, SECONDS - 10) * 1000);
for (const b of [A, B]) console.log(`${at()} ${b.name}, with a watcher: ${await phase(b, since)}`);
clearInterval(rally);
clearInterval(rallyB);
for (const b of [A, B, C]) await b.screenshot(join(OUT, `${b.name}.png`));
for (const b of [A, B, C]) {
  const { events, frames } = await b.lab();
  if (events.includes("desync")) fail(`${b.name}'s machine desynced`);
  if (b.errors.length) fail(`${b.name} had errors: ${b.errors.slice(0, 3).join(" | ")}`);
  console.log(`${at()} ${b.name}: ${frames} frames shown, events ${JSON.stringify(events)}`);
}
console.log(`${at()} screenshots in ${OUT}`);
browsers.forEach((b) => b.close());
process.exit(failed ? 1 : 0);
