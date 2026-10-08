// Cruis'n USA, a one-player game, in real browsers: two headless Chromes of our own (never the
// user's), over CDP. A sits at the Cruis'n USA cabinet (E): the game starts from its start-up
// state with credits; A presses Start, picks the race, the transmission and the car with taps of
// Up, and drives (the gas held, the wheel now and then). B presses E at the same cabinet, which
// is full, so B watches. Prints how evenly each worker posts its frames (the game draws every
// other frame in a race: 34.5 ms apart at its 57.9 Hz), compares B's machine with A's at the
// same frames (worker "probe"s: A numbers its frames as its stream to the watchers does) and
// saves screenshots of both browsers. Each Chrome is killed by its own PID at the end.
//
//   node mame/cruisn-lab.mjs [--site=http://localhost:8787] [--out=mame/.cache/lab]
//
// Needs the site running with the MAME core, crusnusa41.zip and crusnusa41.state in its bucket
// (`make dev BUCKET=local`, or `npx wrangler dev --local` in server/), and the bar's map with the
// cabinet set to "crusnusa41" at (2, -3); local builds only (the client's test hooks,
// client/src/testing.rs).
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
const SITE = argv.site ?? "http://localhost:8787";
const OUT = argv.out ?? new URL(".cache/lab", import.meta.url).pathname;
const ROOM = `cruisn-${Date.now().toString(36)}`;
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PROFILES = mkdtempSync(join(tmpdir(), "cruisn-lab-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(OUT, { recursive: true });

const WORKER_PROBE = `(() => {
  const L = globalThis.__lab = { probes: {}, frames: 0, times: [], buttons: null };
  const post = globalThis.postMessage;
  globalThis.postMessage = function (m, transfer) {
    if (m && m.type === "probe") L.probes[m.frame] = m;
    else if (m && m.type === "frame") { L.frames++; L.times.push(performance.now()); if (L.times.length > 4000) L.times.shift(); }
    else if (m && m.type === "buttons") L.buttons = m.buttons;
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
    const port = 9450 + Math.floor(Math.random() * 300);
    const profile = mkdtempSync(join(PROFILES, `${this.name}-`));
    this.chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run",
      "--window-size=1280,800", "--autoplay-policy=no-user-gesture-required", "--use-angle=metal", "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream", "about:blank"], { stdio: "ignore" });
    console.log(`${this.name}: Chrome pid ${this.chrome.pid}`);
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
  down(code, key, vk) { return this.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode: vk }, this.page); }
  up(code, key, vk) { return this.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk }, this.page); }
  async key(code, key, vk, hold = 120) { await this.down(code, key, vk); await sleep(hold); await this.up(code, key, vk); }
  async click(x, y) {
    for (const type of ["mousePressed", "mouseReleased"]) await this.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 }, this.page);
  }
  async screenshot(file) {
    const { data } = await this.cdp.send("Page.captureScreenshot", { format: "png" }, this.page);
    writeFileSync(file, Buffer.from(data, "base64"));
  }
  close() { try { this.cdp.close(); } catch {} try { process.kill(this.chrome.pid); } catch {} }
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
const t0 = Date.now();
const at = () => `${((Date.now() - t0) / 1000).toFixed(0).padStart(4)} s`;
const status = async (b) => (await b.state())?.status ?? "";

async function arrive(name) {
  const b = new Browser(name);
  browsers.push(b);
  await b.launch();
  await b.cdp.send("Page.navigate", { url: `${SITE}/?room=${ROOM}` }, b.page);
  await until(`${name}: the bar`, async () => (await b.state())?.mode === "Walking", 180000);
  await b.click(640, 400);
  await b.key("ShiftLeft", "Shift", 16);
  // In front of the cabinet at (2, -3): (3, -2) is the cell nearest it (goTo's pick, (1, -2), is
  // nearer Tekken 3's).
  await b.eval(`window.vab.standAt(3, -2, 2, -3)`);
  await sleep(600);
  await b.key("KeyE", "e", 69);
  await until(`${name}: the game`, async () => (await b.state())?.mode === "Playing");
  await until(`${name}: its worker`, async () => b.workers.length > 0);
  return b;
}
/** Intervals between frames the worker posted over the last `ms`. */
async function pacing(b, ms) {
  const before = (await b.lab()).times.length;
  await sleep(ms);
  const times = (await b.lab()).times;
  const fresh = times.slice(-Math.min(times.length, times.length - before + 1));
  const gaps = fresh.slice(1).map((t, i) => t - fresh[i]).sort((x, y) => x - y);
  const q = (p) => gaps[Math.min(gaps.length - 1, Math.floor(gaps.length * p))]?.toFixed(1);
  return `${((gaps.length * 1000) / ms).toFixed(1)} frames posted a second, ${q(0.5)} ms apart (p50), p95 ${q(0.95)}, p99 ${q(0.99)}, worst ${gaps.at(-1)?.toFixed(1)}`;
}

const A = await arrive("A");
console.log(`${at()} A sat down: "${await status(A)}"`);
await until("A's frames", async () => (await A.lab()).frames > 30, 60000);
await sleep(3000);
const labA = await A.lab();
console.log(`${at()} A's buttons: ${JSON.stringify(labA.buttons)}`);
await A.screenshot(join(OUT, "cruisn-A-1-attract.png"));
console.log(`${at()} A in the attract mode: ${await pacing(A, 5000)}`);
// Start, then taps of Up: the race, the transmission, the car.
await A.key("Digit1", "1", 49, 150);
for (let i = 0; i < 9; i++) {
  await sleep(1300);
  await A.key("ArrowUp", "ArrowUp", 38, 300);
  if (i === 4) await A.screenshot(join(OUT, "cruisn-A-2-menus.png"));
}
// The gas held, the wheel now and then.
await A.down("ArrowUp", "ArrowUp", 38);
const steering = setInterval(() => {
  const right = Math.random() < 0.5;
  A.key(right ? "ArrowRight" : "ArrowLeft", right ? "ArrowRight" : "ArrowLeft", right ? 39 : 37, 250).catch(() => {});
}, 2000);
await sleep(8000);
await A.screenshot(join(OUT, "cruisn-A-3-race.png"));
console.log(`${at()} A racing: ${await pacing(A, 8000)} | "${await status(A)}"`);

const B = await arrive("B");
console.log(`${at()} B pressed E: "${await status(B)}"`);
await until("B's frames", async () => (await B.lab()).frames > 30, 60000);
await sleep(4000);
console.log(`${at()} B watching: "${await status(B)}"; A "${await status(A)}"`);
// Same frames on both machines: B numbers its frames from the stream's start, and A answers a
// probe for a frame by that number (worker.js). B's next frame says where the stream is.
const asked = [];
for (let round = 0; round < 4; round++) {
  const seen = new Set(Object.keys((await B.lab()).probes).map(Number));
  await B.eval(`onmessage({ data: { type: "probe" } })`, B.workers.at(-1));
  const now = await until("B's frame", async () => {
    const frames = Object.keys((await B.lab()).probes).map(Number).filter((f) => !seen.has(f) && !asked.includes(f));
    return frames.length ? Math.max(...frames) : undefined;
  });
  const frame = now + 240;
  asked.push(frame);
  for (const b of [A, B]) await b.eval(`onmessage({ data: { type: "probe", at: ${frame} } })`, b.workers.at(-1));
  await sleep(6000);
}
console.log(`${at()} A racing, watched: ${await pacing(A, 6000)}`);
console.log(`${at()} B watching: ${await pacing(B, 6000)}`);
await sleep(8000);
const [a, b] = await Promise.all([A.lab(), B.lab()]);
const both = asked.filter((f) => a.probes[f] && b.probes[f]);
const same = both.filter((f) => a.probes[f].hash === b.probes[f].hash);
console.log(`${at()} B's machine against A's: RAM the same at ${same.length} of ${both.length} probed frames (${asked.join(", ")})` +
  `${both.length - same.length ? `; DIFFERENT at ${both.filter((f) => !same.includes(f)).join(", ")}` : ""}`);
clearInterval(steering);
await A.up("ArrowUp", "ArrowUp", 38);
await A.screenshot(join(OUT, "cruisn-A-4-race-watched.png"));
await B.screenshot(join(OUT, "cruisn-B-watching.png"));
console.log(`screenshots in ${OUT}`);
browsers.forEach((x) => x.close());
process.exit(0);
