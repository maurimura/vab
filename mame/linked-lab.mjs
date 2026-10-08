// Time Crisis II's twin cabinet in real browsers: two headless Chromes of our own (never the
// user's), over CDP, sit at the Time Crisis II cabinet of the bar from `make dev`, one after the
// other: the first plays alone, the second's arrival starts both boards over linked. Both shoot
// their way through LINK PLAY into a linked game, and a third Chrome watches (F). Prints, every
// few seconds, each worker's stats (fps, waits, delay, the game's link mode word) and each
// page's status line; at the end compares the watcher's machine with the first player's at
// the same frames (worker "probe"s) and saves a screenshot of each browser.
//
//   node mame/linked-lab.mjs [--site=http://localhost:8787] [--seconds=40] [--out=mame/.cache/lab]
//   node mame/linked-lab.mjs --mode=solo-pair   (for comparison: two Chromes, each alone in its own room)
//
// Phases: A alone (frames a second), A and B linked at the mode select (no one shoots yet),
// A and B in the linked game (half of --seconds), then with C watching (the other half).
//
// Needs the site running with the MAME core, timecrs2.zip and its three states in its bucket
// (`make dev BUCKET=local`, mame/link-states.mjs), and a cabinet whose game is "timecrs2" on
// its map; local builds only (it uses the client's test hooks, client/src/testing.rs).
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
const SITE = argv.site ?? "http://localhost:8787";
const SECONDS = Number(argv.seconds ?? 40);
const OUT = argv.out ?? new URL(".cache/lab", import.meta.url).pathname;
const ROOM = `linked-${Date.now().toString(36)}`;
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PROFILES = mkdtempSync(join(tmpdir(), "linked-lab-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

// In each emulator worker: its netplay events and probe answers, kept for us to read.
const WORKER_PROBE = `(() => {
  const L = globalThis.__lab = { stats: [], events: [], probes: {}, frames: 0 };
  const post = globalThis.postMessage;
  globalThis.postMessage = function (m, transfer) {
    if (m && m.type === "netplay" && m.event === "stats") L.stats.push({ ...m, at: performance.timeOrigin + performance.now() });
    else if (m && m.type === "netplay") L.events.push(m.event);
    else if (m && m.type === "probe") L.probes[m.frame] = m;
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
    await this.cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `document.cookie = "welcomed=1; path=/";` }, sessionId);
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
      console.error(`[${this.name}] ${where} error:`, (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text ?? "").split("\n")[0]);
    }
    if (msg.method === "Runtime.consoleAPICalled" && ["warning", "error"].includes(msg.params.type)) {
      const where = this.workers.includes(msg.sessionId) ? "worker" : "page";
      console.error(`[${this.name}] ${where} ${msg.params.type}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 200));
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

/** A Chrome in the bar, at the Time Crisis II cabinet: E sits, F watches. */
async function arrive(name, key, room = ROOM) {
  const b = new Browser(name);
  browsers.push(b);
  await b.launch();
  await b.cdp.send("Page.navigate", { url: `${SITE}/?room=${room}` }, b.page);
  await until(`${name}: the bar`, async () => (await b.state())?.mode === "Walking", 120000);
  await b.click(640, 400);
  await b.key("ShiftLeft", "Shift", 16);
  await b.eval(`window.vab.goTo("timecrs2")`);
  await until(`${name}: the cabinet`, async () => (await b.state())?.go_to?.cell);
  await sleep(400);
  await b.key(key === "E" ? "KeyE" : "KeyF", key.toLowerCase(), key.charCodeAt(0));
  await until(`${name}: the game`, async () => (await b.state())?.mode === "Playing");
  await until(`${name}: its worker`, async () => b.workers.length > 0);
  return b;
}

const status = async (b) => (await b.state())?.status ?? "";
const t0 = Date.now();
const at = () => `${((Date.now() - t0) / 1000).toFixed(0).padStart(4)} s`;

async function report(players) {
  for (const b of players) {
    const lab = await b.lab().catch(() => undefined);
    const st = lab?.stats.at(-1);
    console.log(`${at()} ${b.name}: ${st ? `frame ${st.frame} fps ${st.fps} run ${st.runMs} ms delay ${st.delay} waits ${st.stalls} (${st.stallMs} ms) ahead ${st.framesAhead} ` +
      `look ${st.look?.join("/")} link mode ${st.link?.mode} keepalive ${st.link?.keepalive} rx ${st.link?.rxFrames}${st.lost ? " LOST" : ""}` : `frames ${lab?.frames}`}` +
      ` | "${await status(b)}"`);
  }
}

/** The stats of `b`'s worker since `since` (Date.now()), summed up. */
async function phase(b, since) {
  const lab = await b.lab();
  const stats = lab.stats.filter((s) => s.at >= since);
  if (!stats.length) return "no stats";
  const sorted = (key) => stats.map((s) => s[key]).sort((x, y) => x - y);
  const fps = sorted("fps");
  const runs = sorted("runMs");
  return `${stats.length} s: fps ${fps[0]}-${fps.at(-1)} (median ${fps[fps.length >> 1]}), a frame ${runs[0]}-${runs.at(-1)} ms ` +
    `(median ${runs[runs.length >> 1]}), waits ${stats.reduce((n, s) => n + s.stalls, 0)} (longest ${Math.max(...stats.map((s) => s.stallMs))} ms), ` +
    `delay ${stats.at(-1).delay}, ping ${stats.at(-1).ping} ms, modes ${[...new Set(stats.map((s) => s.link?.mode))].join(",")}, ` +
    `lost ${stats.some((s) => s.lost)}`;
}
/** Frames shown a second by `b`'s worker over `ms`, from the worker's own count. */
async function framesPerSecond(b, ms) {
  const before = (await b.lab()).frames;
  await sleep(ms);
  return (((await b.lab()).frames - before) * 1000) / ms;
}

if (argv.mode === "solo-pair") {
  // For comparison: two Chromes each playing the game alone (two rooms), in a solo game.
  const A = await arrive("A", "E", `${ROOM}-a`);
  const B = await arrive("B", "E", `${ROOM}-b`);
  await sleep(3000);
  console.log(`${at()} both alone at the mode select: A ${(await framesPerSecond(A, 5000)).toFixed(1)}, B ${(await framesPerSecond(B, 5000)).toFixed(1)} frames a second`);
  // SOLO ONLY is the lower panel of the mode select: aim there with the mouse and click.
  for (let i = 0; i < 8; i++) {
    for (const b of [A, B]) {
      await b.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 640, y: 560 }, b.page);
      await b.click(640, 560);
    }
    await sleep(1200);
  }
  const shooting = setInterval(() => [A, B].forEach((b) => b.key("KeyZ", "z", 90, 150).catch(() => {})), 1500);
  await sleep(5000);
  for (let i = 0; i < 4; i++) {
    const [a, b] = await Promise.all([framesPerSecond(A, 5000), framesPerSecond(B, 5000)]);
    console.log(`${at()} both alone, in a solo game: A ${a.toFixed(1)}, B ${b.toFixed(1)} frames a second`);
  }
  clearInterval(shooting);
  for (const b of [A, B]) await b.screenshot(join(OUT, `solo-pair-${b.name}.png`));
  browsers.forEach((b) => b.close());
  process.exit(0);
}

const A = await arrive("A", "E");
console.log(`${at()} A sat down: "${await status(A)}"`);
await sleep(4000);
console.log(`${at()} A alone: ${(await framesPerSecond(A, 6000)).toFixed(1)} frames a second | "${await status(A)}"`);
const B = await arrive("B", "E");
console.log(`${at()} B sat down: "${await status(B)}"`);
// Both boards start over linked; wait until both run linked (stats with the link).
await until("both linked", async () => {
  const labs = await Promise.all([A, B].map((b) => b.lab()));
  return labs.every((lab) => lab.stats.at(-1)?.link?.heard);
}, 90000);
console.log(`${at()} both boards linked`);
await report([A, B]);
// At the mode select (the start-up states' screen), a few seconds before anyone shoots.
let since = Date.now() + 1000;
await sleep(7000);
for (const b of [A, B]) console.log(`${at()} ${b.name}, linked, at the mode select: ${await phase(b, since)}`);
// Shoot: LINK PLAY at the mode select (the gun aims at the middle), then on into the game.
const shooting = setInterval(() => {
  A.key("KeyZ", "z", 90, 150).catch(() => {});
  setTimeout(() => B.key("KeyZ", "z", 90, 150).catch(() => {}), 400);
}, 1500);
await until("linked gameplay on both", async () => {
  const labs = await Promise.all([A, B].map((b) => b.lab()));
  return labs.every((lab) => lab.stats.at(-1)?.link?.mode === 2);
}, 60000).then(() => console.log(`${at()} LINKED GAMEPLAY on both`), (e) => console.log(`${at()} ${e.message}`));
since = Date.now() + 1000;
await sleep(SECONDS * 500);
await report([A, B]);
for (const b of [A, B]) console.log(`${at()} ${b.name}, linked game: ${await phase(b, since)} | "${await status(b)}"`);
const C = await arrive("C", "F");
console.log(`${at()} C watches: "${await status(C)}"`);
since = Date.now() + 3000;
const probes = [];
const measureFrom = Date.now();
while (Date.now() - measureFrom < SECONDS * 500) {
  await sleep(5000);
  await report([A, B, C]);
  const st = (await A.lab()).stats.at(-1);
  if (st) {
    const frame = st.frame + 120;
    probes.push(frame);
    for (const [b, s] of [[A, A.workers.at(-1)], [C, C.workers.at(-1)]]) await b.eval(`onmessage({ data: { type: "probe", at: ${frame} } })`, s);
  }
}
const watcherFps = await framesPerSecond(C, 5000);
clearInterval(shooting);
await sleep(3000);
for (const b of [A, B]) console.log(`${b.name}, linked game, watched: ${await phase(b, since)} | "${await status(b)}"`);
{
  const [a, c] = await Promise.all([A.lab(), C.lab()]);
  const both = probes.filter((f) => a.probes[f] && c.probes[f]);
  const same = both.filter((f) => a.probes[f].hash === c.probes[f].hash);
  console.log(`C (watching): ${watcherFps.toFixed(1)} frames a second; RAM the same as A's at ${same.length} of ${both.length} probed frames (${probes.length} asked)` +
    `${both.length - same.length ? `; DIFFERENT at ${both.filter((f) => !same.includes(f)).join(", ")}` : ""}; status "${await status(C)}"`);
}
for (const b of [A, B, C]) await b.screenshot(join(OUT, `linked-lab-${b.name}.png`));
console.log(`screenshots: ${OUT}/linked-lab-{A,B,C}.png`);
browsers.forEach((b) => b.close());
process.exit(0);
