// Online lab: drives one or two headless Chromes of our own (never the user's) through the real
// bar page over CDP, sits them at the Virtua Striker cabinet, starts a match, and measures where
// time goes: in the emulator worker (frames run and posted, the core's frame cost, GGRS waits,
// the input delay and the others' input in hand, second by second), on the page (event-loop lag,
// rAF cadence) and on the wire. A one-way delay +- jitter can be injected on every WebRTC
// data-channel send, to play at a chosen ping on one machine. Results print and go to
// <out>/<label>.json with a screenshot of each browser.
//
//   node supermodel/harness/online-lab.mjs --mode=solo|online [--delay=60 --jitter=10]
//        [--seconds=25 --settle=35] [--label=rtt120] [--site=http://localhost:8787] [--out=.]
//
// Needs the site running (make dev; --site for another, e.g. the preview) with the Supermodel
// core and vs298.zip in its bucket. The worker probe wraps the core's wasm exports, whose names
// are minified per build: it reads them off the served glue.
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
const MODE = argv.mode ?? "solo";
const DELAY = Number(argv.delay ?? 0);
const JITTER = Number(argv.jitter ?? 0);
const SECONDS = Number(argv.seconds ?? 30);
const LABEL = argv.label ?? `${MODE}-${DELAY}-${JITTER}`;
const SITE = argv.site ?? "http://localhost:8787";
const ROOM = argv.room ?? `lab-${Date.now().toString(36)}`;
import { tmpdir } from "node:os";
const OUT = argv.out ?? ".";
const SCRATCH = mkdtempSync(join(tmpdir(), "online-lab-"));
const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SETTLE = Number(argv.settle ?? 40);
/** With --poke=N, browser B flips a word of the game's RAM after its Nth frame, a drift the machines must notice. */
const POKE = Number(argv.poke ?? 0);
/** With --rttA=N, browser A's worker is told the round trip was N ms when it goes online: a bigger input delay on one side only. */
const RTT_A = Number(argv.rttA ?? 0);
// The core's wasm exports are minified; the served glue maps them: _retro_run=Module["_retro_run"]=wasmExports["hb"].
const glue = await (await fetch(`${SITE}/supermodel/supermodel.mjs`)).text();
const exportName = (fn) => glue.match(new RegExp(`_${fn}=Module\\["_${fn}"\\]=wasmExports\\["(\\w+)"\\]`))?.[1];
const EXPORTS = { run: exportName("retro_run"), save: exportName("retro_serialize"), load: exportName("retro_unserialize"),
  ram: exportName("retro_get_memory_data"), memory: glue.match(/wasmMemory=wasmExports\["(\w+)"\]/)?.[1] };
console.error("wasm export names:", EXPORTS);

// ---------------------------------------------------------------- probes injected in the page
const PAGE_PROBE = `(() => {
  document.cookie = "welcomed=1; path=/"; // the first visit's help card would eat the first key
  const L = window.__lab = { delay: ${DELAY}, jitter: ${JITTER}, sends: [], recvs: [], rafGaps: [], lagGaps: [], longTasks: 0, longTaskMs: 0, wsSends: 0, wsRecvs: 0, dcSends: 0, dcRecvs: 0, workers: [], echoes: [] };
  const OWorker = window.Worker;
  window.Worker = function (url, opts) { const w = new OWorker(url, opts); L.workers.push(w); w.addEventListener("message", (e) => { if (e.data && e.data.type === "lab-echo") L.echoes.push(performance.now() - e.data.t); }); return w; };
  window.Worker.prototype = OWorker.prototype;
  L.echo = () => { const w = L.workers.at(-1); if (w) w.postMessage({ type: "lab-echo", t: performance.now() }); };
  const abs = () => performance.timeOrigin + performance.now();
  const isPing = (d) => { if (!(d instanceof ArrayBuffer) || d.byteLength !== 6) return false; const b = new Uint8Array(d); return b[0] === 0x76 && b[1] === 0x61 && b[2] === 0x62 && b[3] === 0x70; };
  // Outgoing game packets: direct (data channel) or relayed (WebSocket, kind byte 0 at offset 4).
  const dcSend = RTCDataChannel.prototype.send;
  let due = 0;
  const later = (fn) => { const d = L.delay + (Math.random() * 2 - 1) * L.jitter; due = Math.max(due, performance.now() + Math.max(0, d)); setTimeout(fn, due - performance.now()); };
  RTCDataChannel.prototype.send = function (data) {
    const ping = isPing(data);
    if (!ping && data instanceof ArrayBuffer) { L.sends.push(abs()); L.dcSends++; }
    if (!L.delay) return dcSend.call(this, data);
    later(() => { try { dcSend.call(this, data); } catch {} });
  };
  const desc = Object.getOwnPropertyDescriptor(RTCDataChannel.prototype, "onmessage");
  Object.defineProperty(RTCDataChannel.prototype, "onmessage", { configurable: true, get() { return desc.get.call(this); },
    set(fn) { desc.set.call(this, (ev) => { if (ev.data instanceof ArrayBuffer && !isPing(ev.data)) { L.recvs.push(abs()); L.dcRecvs++; } return fn(ev); }); } });
  const wsSend = WebSocket.prototype.send;
  WebSocket.prototype.send = function (data) {
    if (data instanceof Uint8Array && data.length > 4 && data[4] === 0 && !isPing(data.buffer.slice(5))) { L.sends.push(abs()); L.wsSends++; }
    return wsSend.call(this, data);
  };
  const wdesc = Object.getOwnPropertyDescriptor(WebSocket.prototype, "onmessage");
  Object.defineProperty(WebSocket.prototype, "onmessage", { configurable: true, get() { return wdesc.get.call(this); },
    set(fn) { wdesc.set.call(this, (ev) => { if (ev.data instanceof ArrayBuffer && ev.data.byteLength > 4 && new Uint8Array(ev.data)[4] === 0 && !isPing(ev.data.slice(5))) { L.recvs.push(abs()); L.wsRecvs++; } return fn(ev); }); } });
  let lastRaf = 0;
  const raf = (t) => { if (lastRaf) L.rafGaps.push(t - lastRaf); lastRaf = t; requestAnimationFrame(raf); };
  requestAnimationFrame(raf);
  let lastT = performance.now();
  const tick = () => { const now = performance.now(); L.lagGaps.push(now - lastT - 8); lastT = now; setTimeout(tick, 8); };
  setTimeout(tick, 8);
  try { new PerformanceObserver((list) => { for (const e of list.getEntries()) { L.longTasks++; L.longTaskMs += e.duration; } }).observe({ type: "longtask", buffered: true }); } catch {}
})();`;

// ------------------------------------------------------- probes injected in the emulator worker
const WORKER_PROBE = `(() => {
  globalThis.addEventListener("message", (e) => { if (e.data && e.data.type === "lab-echo") postMessage({ type: "lab-echo", t: e.data.t }); });
  // The worker's onmessage, wrapped: an "online" message can be told a different round trip (--rttA).
  const od = Object.getOwnPropertyDescriptor(globalThis, "onmessage") || Object.getOwnPropertyDescriptor(Object.getPrototypeOf(globalThis), "onmessage");
  if (od && od.set) Object.defineProperty(globalThis, "onmessage", { configurable: true, get() { return od.get.call(globalThis); },
    set(fn) { od.set.call(globalThis, (ev) => { if (ev.data && ev.data.type === "online" && W.forceRtt) { ev.data.roundTrip = W.forceRtt; W.events.push("rtt forced " + W.forceRtt); } return fn(ev); }); } });
  const W = globalThis.__lab = { events: [], t0: performance.timeOrigin + performance.now(), poke: 0, ticks: 0, tickRuns: [0, 0, 0, 0, 0, 0], tickDurs: [], timerLags: [], runs: [], runsAt: [], framePosts: [], frameDups: 0, outs: [], ins: [], pollWaits: [], stats: null, statsLog: [], saves: [], loads: [], stalls: 0 };
  const abs = () => performance.timeOrigin + performance.now();
  let cur = null;
  let lastHash = -1;
  const pendingIns = [];
  const oST = globalThis.setTimeout;
  globalThis.setTimeout = function (fn, ms = 0, ...a) {
    const at = performance.now();
    return oST(() => {
      const t0 = performance.now();
      W.timerLags.push(t0 - at - ms);
      cur = { runs: 0 };
      while (pendingIns.length) W.pollWaits.push(t0 - pendingIns.shift());
      try { return fn(...a); } finally {
        W.ticks++; W.tickDurs.push(performance.now() - t0); W.tickRuns[Math.min(cur.runs, 5)]++; cur = null;
      }
    }, ms);
  };
  const oPM = globalThis.postMessage;
  globalThis.postMessage = function (m, tr) {
    if (m && m.type === "frame") {
      const r = m.rgba; let h = 0; for (let i = 0; i < r.length; i += 1021) h = (Math.imul(h, 31) + r[i]) | 0;
      if (h === lastHash) W.frameDups++; lastHash = h;
      W.framePosts.push(abs());
    } else if (m && m.type === "netplay" && m.event !== "stats") { W.events.push(((abs() - W.t0) / 1000).toFixed(1) + "s:" + m.event + (m.frame !== undefined ? "@" + m.frame : ""));
    } else if (m && m.type === "netplay" && m.event === "stats") { W.stats = m; W.statsLog.push({ t: abs(), ping: m.ping, delay: m.delay, rollback: m.rollback, fps: m.fps, stalls: m.stalls, stallMs: m.stallMs, look: m.look, prefills: m.prefills, framesAhead: m.framesAhead }); }
    return oPM.call(this, m, tr);
  };
  const MP = MessagePort.prototype;
  const oMPpm = MP.postMessage;
  MP.postMessage = function (m, tr) { if (Array.isArray(m) && m.length === 2 && typeof m[0] === "number") W.outs.push(abs()); else if (m instanceof Int16Array) W.audioPosts = (W.audioPosts || 0) + 1; return oMPpm.call(this, m, tr); };
  const d = Object.getOwnPropertyDescriptor(MP, "onmessage");
  Object.defineProperty(MP, "onmessage", { configurable: true, get() { return d.get.call(this); },
    set(fn) { d.set.call(this, (ev) => { const m = ev.data; if (Array.isArray(m) && m.length === 2 && typeof m[0] === "number") { W.ins.push(abs()); pendingIns.push(performance.now()); } return fn(ev); }); } });
  W.adv = []; // [t, ok, ms] per session_advance
  W.recvAt = []; W.outgoingAt = []; W.outgoingCounts = [];
  const wrapped = new WeakMap();
  const wrapExports = (ex) => {
    if (ex && typeof ex.session_advance === "function") {
      if (wrapped.has(ex)) return wrapped.get(ex);
      const out = {}; for (const k of Object.keys(ex)) out[k] = ex[k];
      out.session_advance = function (...a) { const t0 = performance.now(); const r = ex.session_advance.apply(this, a); W.adv.push([abs(), r[0] !== 0 ? 1 : 0, performance.now() - t0]); return r; };
      out.session_receive = function (...a) { W.recvAt.push(abs()); return ex.session_receive.apply(this, a); };
      out.session_outgoing = function (...a) { const r = ex.session_outgoing.apply(this, a); W.outgoingAt.push(abs()); W.outgoingCounts.push(r.length); return r; };
      wrapped.set(ex, out);
      return out;
    }
    if (!ex || typeof ex["${EXPORTS.run}"] !== "function") return ex;
    if (wrapped.has(ex)) return wrapped.get(ex);
    W.wrappedExports = (W.wrappedExports || 0) + 1;
    const out = {}; for (const k of Object.keys(ex)) out[k] = ex[k];
    const time = (name, arr, after) => { const f = ex[name]; out[name] = function (...a) { const t0 = performance.now(); try { return f.apply(this, a); } finally { arr.push(performance.now() - t0); after && after(); } }; };
    time("${EXPORTS.run}", W.runs, () => {
      W.runsAt.push(abs()); if (cur) cur.runs++;
      if (W.poke && W.runs.length === W.poke) { const ptr = ex["${EXPORTS.ram}"](2); new Uint8Array(ex["${EXPORTS.memory}"].buffer)[ptr + 0x200000] ^= 0xff; W.poked = W.runs.length; }
    });
    time("${EXPORTS.save}", W.saves); time("${EXPORTS.load}", W.loads);
    wrapped.set(ex, out);
    return out;
  };
  const proxy = (inst) => new Proxy(inst, { get: (t, p) => (p === "exports" ? wrapExports(t.exports) : Reflect.get(t, p)) });
  const oInst = WebAssembly.instantiate;
  WebAssembly.instantiate = async function (src, imports) { const r = await oInst.call(this, src, imports); return r instanceof WebAssembly.Instance ? proxy(r) : { module: r.module, instance: proxy(r.instance) }; };
  const OInstance = WebAssembly.Instance;
  WebAssembly.Instance = function (module, imports) { return proxy(new OInstance(module, imports)); };
  WebAssembly.Instance.prototype = OInstance.prototype;
  const oInstS = WebAssembly.instantiateStreaming;
  if (oInstS) WebAssembly.instantiateStreaming = async function (src, imports) { const r = await oInstS.call(this, src, imports); return { module: r.module, instance: proxy(r.instance) }; };
})();`;

// ------------------------------------------------------------------------------ a tiny CDP client
class CDP {
  #ws; #next = 1; #pending = new Map(); handlers = [];
  static async connect(url) { const c = new CDP(); c.#ws = new WebSocket(url); await new Promise((r, j) => { c.#ws.onopen = r; c.#ws.onerror = j; });
    c.#ws.onmessage = (m) => { const msg = JSON.parse(m.data); if (msg.id) { const p = c.#pending.get(msg.id); c.#pending.delete(msg.id); msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result); } else for (const h of c.handlers) h(msg); }; return c; }
  send(method, params = {}, sessionId) { return new Promise((resolve, reject) => { const id = this.#next++; this.#pending.set(id, { resolve, reject }); this.#ws.send(JSON.stringify({ id, method, params, sessionId })); }); }
  close() { this.#ws.close(); }
}

class Browser {
  chrome; cdp; page; workers = []; name;
  async launch(name) {
    this.name = name;
    const port = 9400 + Math.floor(Math.random() * 400);
    const profile = mkdtempSync(join(SCRATCH, `chrome-${name}-`));
    this.chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--window-size=1280,800",
      "--autoplay-policy=no-user-gesture-required", "--enable-unsafe-webgpu", "--ignore-gpu-blocklist", "about:blank"], { stdio: "ignore" });
    let version;
    for (let i = 0; i < 50 && !version; i++) { try { version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json(); } catch { await sleep(200); } }
    if (!version) throw new Error(`Chrome ${name} did not come up`);
    this.cdp = await CDP.connect(version.webSocketDebuggerUrl);
    const { targetId } = await this.cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await this.cdp.send("Target.attachToTarget", { targetId, flatten: true });
    this.page = sessionId;
    this.cdp.handlers.push((msg) => this.#event(msg));
    await this.cdp.send("Page.enable", {}, sessionId);
    await this.cdp.send("Runtime.enable", {}, sessionId);
    await this.cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE_PROBE }, sessionId);
    await this.cdp.send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, sessionId);
  }
  async #event(msg) {
    if (msg.method === "Target.attachedToTarget") {
      const { sessionId, targetInfo, waitingForDebugger } = msg.params;
      if (targetInfo.type === "worker" && targetInfo.url.includes("emulator/worker.js")) {
        try {
          await this.cdp.send("Runtime.enable", {}, sessionId);
          // Paused at start, the worker's globals (setTimeout, postMessage) aren't installed yet:
          // let it run and probe right away, before its module script has downloaded.
          if (waitingForDebugger) await this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId);
          // The globals appear a moment after the worker starts; wrapping them before that breaks the worker.
          for (let i = 0; i < 50; i++) {
            const { result } = await this.cdp.send("Runtime.evaluate", { expression: "typeof setTimeout === 'function' && typeof postMessage === 'function'", returnByValue: true }, sessionId);
            if (result?.value) break;
            await sleep(2);
          }
          const probe = await this.cdp.send("Runtime.evaluate", { expression: WORKER_PROBE + " [typeof setTimeout, typeof postMessage, typeof WebAssembly.instantiateStreaming].join()", returnByValue: true }, sessionId);
          if (probe.exceptionDetails) console.error(`[${this.name}] WORKER PROBE THREW:`, probe.exceptionDetails.exception?.description ?? probe.exceptionDetails.text);
          this.workers.push(sessionId);
          console.error(`[${this.name}] worker attached and probed (${probe.result?.value})`);
        } catch (e) { console.error(`[${this.name}] worker probe failed: ${e.message}`); }
        return;
      }
      if (waitingForDebugger) this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId).catch(() => {});
    }
    if (msg.method === "Runtime.exceptionThrown" && this.workers.includes(msg.sessionId)) console.error(`[${this.name}] WORKER error:`, (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text ?? "").split("\n").slice(0, 4).join(" | "));
    if (msg.method === "Runtime.consoleAPICalled" && this.workers.includes(msg.sessionId)) console.error(`[${this.name}] WORKER ${msg.params.type}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 300));
    if (msg.method === "Runtime.exceptionThrown" && msg.sessionId === this.page) console.error(`[${this.name}] page error:`, msg.params.exceptionDetails.exception?.description?.split("\n")[0] ?? msg.params.exceptionDetails.text);
    if (msg.method === "Runtime.consoleAPICalled" && msg.sessionId === this.page && msg.params.type !== "log") console.error(`[${this.name}] page ${msg.params.type}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 200));
  }
  async eval(expression, sessionId = this.page) {
    const { result, exceptionDetails } = await this.cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  }
  async open(url) { await this.cdp.send("Page.navigate", { url }, this.page); }
  async key(code, key, vk, hold = 60) {
    await this.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode: vk, text: key.length === 1 ? key : undefined }, this.page);
    await sleep(hold);
    await this.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk }, this.page);
  }
  async click(x, y) {
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }, this.page);
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }, this.page);
  }
  async screenshot(file) { const { data } = await this.cdp.send("Page.captureScreenshot", { format: "png" }, this.page); writeFileSync(file, Buffer.from(data, "base64")); }
  close() { try { this.cdp.close(); } catch {} try { this.chrome.kill(); } catch {} }
}

// ------------------------------------------------------------------------------------ the run
const q = (arr, p) => { if (!arr.length) return NaN; const s = [...arr].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(s.length * p))]; };
const f1 = (x) => (Number.isFinite(x) ? x.toFixed(1) : "-");
const f2 = (x) => (Number.isFinite(x) ? x.toFixed(2) : "-");
const dist = (arr) => `p50 ${f2(q(arr, 0.5))} p95 ${f2(q(arr, 0.95))} p99 ${f2(q(arr, 0.99))} max ${f2(Math.max(...arr))}`;
/** Greedy monotone match: for each a[i], the first b[j] >= a[i] not yet used; returns b-a. */
const hops = (a, b) => { const out = []; let j = 0; for (const t of a) { while (j < b.length && b[j] < t) j++; if (j >= b.length) break; out.push(b[j] - t); j++; } return out; };
const sliceAfter = (arr, t) => arr.filter((x) => x >= t);

const browsers = [];
process.on("exit", () => browsers.forEach((b) => b.close()));
process.on("SIGINT", () => process.exit(1));

async function sitDown(b) {
  await b.open(`${SITE}/?room=${ROOM}`);
  for (let i = 0; i < 300; i++) { if (await b.eval("typeof window.emulatorPlay === 'function' && window.__lab && window.__lab.rafGaps.length > 30").catch(() => false)) break; await sleep(200); }
  await sleep(1500);
  await b.click(640, 400);
  await sleep(300);
  // Bevy may still be starting (a cold server serves its 15 MB slowly): press E until a worker appears.
  for (let tries = 0; tries < 8 && !b.workers.length; tries++) {
    await b.key("KeyE", "e", 69);
    for (let i = 0; i < 30 && !b.workers.length; i++) await sleep(100);
  }
  if (!b.workers.length) throw new Error(`[${b.name}] no emulator worker appeared after pressing E`);
  let i = 0;
  for (; i < 150; i++) { if (await b.eval("globalThis.__lab && globalThis.__lab.runs.length > 10", b.workers[0]).catch((e) => (console.error("eval:", e.message), false))) break; await sleep(200); }
  console.error(`[${b.name}] game running after ${i * 0.2}s; lab keys:`, await b.eval("JSON.stringify(Object.fromEntries(Object.entries(globalThis.__lab || {}).map(([k, v]) => [k, Array.isArray(v) ? v.length : v])))", b.workers[0]).catch((e) => e.message));
}

/** A coin, Start, and a few button presses to get through team select into a match. */
async function startMatch(b) {
  // Held long enough to register even if the lockstep session is stalling (a short press is lost).
  await b.key("Digit5", "5", 53, 400); await sleep(800);
  await b.key("Digit1", "1", 49, 400); await sleep(3000);
  for (let i = 0; i < 6; i++) { await b.key("KeyX", "x", 88, 400); await sleep(1500); }
}

const A = new Browser(); browsers.push(A);
await A.launch("A");
await sitDown(A);
if (RTT_A) await A.eval(`globalThis.__lab.forceRtt = ${RTT_A}`, A.workers[0]);
let B;
if (MODE === "online") {
  B = new Browser(); browsers.push(B);
  await B.launch("B");
  await sitDown(B);
  // Both in a session: each worker reports GGRS stats once synchronized.
  for (let i = 0; i < 300; i++) {
    const ok = await Promise.all([A, B].map((b) => b.eval("!!(globalThis.__lab.stats && globalThis.__lab.stats.ping !== undefined)", b.workers[0]).catch(() => false)));
    if (ok.every(Boolean)) break;
    await sleep(200);
  }
  console.error("online:", await A.eval("JSON.stringify(globalThis.__lab.stats)", A.workers[0]), await B.eval("JSON.stringify(globalThis.__lab.stats)", B.workers[0]));
}
if (POKE && B) { await B.eval(`globalThis.__lab.poke = globalThis.__lab.runs.length + ${POKE}`, B.workers[0]); console.error(`B will flip a word of RAM after ${POKE} more frames`); }
await startMatch(A);
console.error(`settling ${SETTLE}s ...`);
await sleep(SETTLE * 1000);

const t0 = Date.now();
console.error(`measuring ${SECONDS}s ...`);
await A.eval(`window.__echoTimer = setInterval(() => window.__lab.echo(), 50)`);
await sleep(SECONDS * 1000);
await A.eval(`clearInterval(window.__echoTimer)`);
const t1 = Date.now();

const report = { label: LABEL, mode: MODE, delay: DELAY, jitter: JITTER, seconds: (t1 - t0) / 1000, browsers: {} };
for (const b of browsers) {
  const W = await b.eval("JSON.stringify(globalThis.__lab)", b.workers[0]).then(JSON.parse);
  const P = await b.eval("JSON.stringify(window.__lab)").then(JSON.parse);
  const wOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()", b.workers[0]);
  const pOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()");
  console.error(`[${b.name}] clock offsets vs Date.now(): worker ${wOff.toFixed(1)} ms, page ${pOff.toFixed(1)} ms`);
  for (const k of ["outs", "ins", "runsAt", "framePosts"]) W[k] = W[k].map((t) => t - wOff);
  for (const k of ["sends", "recvs"]) P[k] = P[k].map((t) => t - pOff);
  const gl = await b.eval("(() => { const g = new OffscreenCanvas(1, 1).getContext('webgl2'); const e = g.getExtension('WEBGL_debug_renderer_info'); return g.getParameter(e ? e.UNMASKED_RENDERER_WEBGL : g.RENDERER); })()", b.workers[0]).catch((e) => e.message);
  console.error(`[${b.name}] worker WebGL2 renderer: ${gl}; audio posts ${W.audioPosts}`);
  console.error(`[${b.name}] raw: ticks ${W.ticks} runs ${W.runs.length} runsAt ${W.runsAt.length} posts ${W.framePosts.length} outs ${W.outs.length} ins ${W.ins.length} stats ${W.statsLog.length}; first runsAt ${W.runsAt[0]} t0 ${t0} t1 ${t1}; page sends ${P.sends.length} recvs ${P.recvs.length}`);
  const win = (arr) => arr.filter((t) => t >= t0 && t <= t1);
  const runsAt = win(W.runsAt), posts = win(W.framePosts);
  // Per-tick arrays aren't timestamped; take the tail proportional to the window.
  const frac = W.runsAt.length ? Math.min(1, SECONDS * 1000 / (Date.now() - W.runsAt[0])) : 1;
  const tail = (arr) => arr.slice(Math.floor(arr.length * (1 - frac)));
  const runs = tail(W.runs), tickDurs = tail(W.tickDurs), timerLags = tail(W.timerLags), pollWaits = tail(W.pollWaits);
  const outs = win(W.outs), ins = win(W.ins), sends = win(P.sends), recvs = win(P.recvs);
  const hopOut = hops(outs, sends), hopIn = hops(recvs, ins);
  const gaps = (arr) => arr.slice(1).map((t, i) => t - arr[i]);
  const clump = (arr) => { const g = gaps(arr); return `n ${g.length} p50 ${f2(q(g, 0.5))} p95 ${f2(q(g, 0.95))} max ${f2(Math.max(...g))} under1ms ${g.filter((x) => x < 1).length}`; };
  console.error(`[${b.name}] gaps (one clock each): worker outs ${clump(outs)} | page sends ${clump(sends)} | page recvs ${clump(recvs)} | worker ins ${clump(ins)}`);
  if (P.echoes.length) console.error(`[${b.name}] page->worker->page echo ms: p50 ${f2(q(P.echoes, 0.5))} p95 ${f2(q(P.echoes, 0.95))} max ${f2(Math.max(...P.echoes))} (${P.echoes.length} echoes)`);
  // Frame post intervals: how evenly the picture is updated.
  const postGaps = posts.slice(1).map((t, i) => t - posts[i]);
  const stats = W.statsLog.filter((s) => s.t >= t0 && s.t <= t1);
  const adv = W.adv.filter(([t]) => t >= t0 && t <= t1);
  const stalls = []; let stallStart = null, okCount = 0, failCount = 0;
  for (const [t, ok] of adv) { if (ok) { okCount++; if (stallStart !== null) { stalls.push(t - stallStart); stallStart = null; } } else { failCount++; if (stallStart === null) stallStart = t; } }
  const advMs = adv.filter(([, ok]) => ok).map(([, , ms]) => ms);
  if (b.name === "A") console.error(`[A] hop debug (ms rel. t0) page recvs: ${recvs.slice(100, 108).map((t) => (t - t0).toFixed(1)).join(" ")} | worker ins: ${ins.slice(100, 108).map((t) => (t - t0).toFixed(1)).join(" ")}`);
  const r = report.browsers[b.name] = {
    emulatedFps: runsAt.length / report.seconds, shownFps: posts.length / report.seconds,
    run: { p50: q(runs, 0.5), p95: q(runs, 0.95), p99: q(runs, 0.99), max: Math.max(...runs) },
    tick: { perSec: tickDurs.length / report.seconds, runsPerTick: W.tickRuns, dur: { p50: q(tickDurs, 0.5), p95: q(tickDurs, 0.95), max: Math.max(...tickDurs) } },
    timerLag: { p50: q(timerLags, 0.5), p95: q(timerLags, 0.95), max: Math.max(...timerLags) },
    pollWait: { n: pollWaits.length, p50: q(pollWaits, 0.5), p95: q(pollWaits, 0.95), max: Math.max(...pollWaits) },
    postGap: { p50: q(postGaps, 0.5), p95: q(postGaps, 0.95), p99: q(postGaps, 0.99), max: Math.max(...postGaps), over25: postGaps.filter((g) => g > 25).length, over40: postGaps.filter((g) => g > 40).length },
    frameDups: W.frameDups, saves: W.saves.length, saveMs: q(W.saves, 0.5), loads: W.loads.length, loadMs: q(W.loads, 0.5),
    packets: { out: outs.length, in: ins.length, pageSends: sends.length, pageRecvs: recvs.length, dc: [P.dcSends, P.dcRecvs], ws: [P.wsSends, P.wsRecvs] },
    hopOut: { p50: q(hopOut, 0.5), p95: q(hopOut, 0.95), max: Math.max(...hopOut) },
    hopIn: { p50: q(hopIn, 0.5), p95: q(hopIn, 0.95), max: Math.max(...hopIn) },
    page: { raf: { p50: q(P.rafGaps, 0.5), p95: q(P.rafGaps, 0.95), max: Math.max(...P.rafGaps) }, lag: { p50: q(P.lagGaps, 0.5), p95: q(P.lagGaps, 0.95), max: Math.max(...P.lagGaps) }, longTasks: P.longTasks, longTaskMs: P.longTaskMs },
    ggrs: stats.length ? { ping: { min: Math.min(...stats.map((s) => s.ping)), max: Math.max(...stats.map((s) => s.ping)) }, delay: stats.at(-1).delay, rollback: stats.at(-1).rollback, fpsReported: stats.map((s) => s.fps),
      trajectory: W.statsLog.map((s) => `${s.delay}${s.stalls ? `/${s.stalls}w${s.stallMs}` : ""}${s.look?.length ? `[${s.look.join(",")}]` : ""}${s.prefills ? `p${s.prefills}` : ""}${s.framesAhead ? `a${s.framesAhead}` : ""}`).join(" ") } : null,
    events: W.events.join(" "), poked: W.poked,
    ggrsAdvance: { ok: okCount, stalled: failCount, stallEpisodes: stalls.length, stallMs: { p50: q(stalls, 0.5), p95: q(stalls, 0.95), max: Math.max(...stalls) }, advanceMs: { p50: q(advMs, 0.5), p95: q(advMs, 0.95), max: Math.max(...advMs) }, outgoingCalls: W.outgoingAt.filter((t) => t >= t0 && t <= t1).length },
    raw: { outs, ins, sends, recvs, runsAt, posts },
  };
  if (outs.length) {
    const from = outs[Math.floor(outs.length / 2)];
    const ev = [...runsAt.map((t) => [t, "run"]), ...outs.map((t) => [t, "out"]), ...ins.map((t) => [t, "in"]), ...posts.map((t) => [t, "POST"])]
      .filter(([t]) => t >= from && t < from + 400).sort((x, y) => x[0] - y[0]);
    r.timeline = ev.map(([t, k]) => `${(t - from).toFixed(1)}:${k}`).join(" ");
  }
  await b.screenshot(join(OUT, `${LABEL}-${b.name}.png`));
}
// Wire: A's page sends -> B's page receives (and back), same machine clock.
if (B) {
  const a = report.browsers.A.raw, bb = report.browsers.B.raw;
  const ab = hops(a.sends, bb.recvs), ba = hops(bb.sends, a.recvs);
  report.wire = { aToB: { p50: q(ab, 0.5), p95: q(ab, 0.95), max: Math.max(...ab) }, bToA: { p50: q(ba, 0.5), p95: q(ba, 0.95), max: Math.max(...ba) } };
}

// ------------------------------------------------------------------------------- print
console.log(`\n== ${LABEL}: ${MODE}${DELAY ? `, injected one-way ${DELAY}±${JITTER} ms` : ""}, ${f1(report.seconds)} s ==`);
for (const [name, r] of Object.entries(report.browsers)) {
  console.log(`-- ${name} --`);
  console.log(`  emulated ${f1(r.emulatedFps)} fps, shown ${f1(r.shownFps)} fps, duplicate frames ${r.frameDups}` + (r.ggrs ? `, GGRS ping ${r.ggrs.ping.min}-${r.ggrs.ping.max} ms, input delay ${r.ggrs.delay} frames, rollback ${r.ggrs.rollback}, fps reported ${r.ggrs.fpsReported.join(" ")}` : ""));
  if (r.ggrs) console.log(`  per second since the session began, delay[/waits+longest ms]: ${r.ggrs.trajectory}`);
  if (r.events) console.log(`  netplay events (s since the worker started): ${r.events}${r.poked ? `; RAM flipped after frame ${r.poked}` : ""}`);
  console.log(`  retro_run ms: p50 ${f2(r.run.p50)} p95 ${f2(r.run.p95)} p99 ${f2(r.run.p99)} max ${f2(r.run.max)}`);
  console.log(`  ticks/s ${f1(r.tick.perSec)}; runs per tick [0,1,2,3,4,5+] = ${r.tick.runsPerTick.join(" ")}; tick ms p50 ${f2(r.tick.dur.p50)} p95 ${f2(r.tick.dur.p95)} max ${f2(r.tick.dur.max)}`);
  console.log(`  setTimeout overshoot ms: p50 ${f2(r.timerLag.p50)} p95 ${f2(r.timerLag.p95)} max ${f2(r.timerLag.max)}`);
  console.log(`  frame post gap ms: p50 ${f2(r.postGap.p50)} p95 ${f2(r.postGap.p95)} p99 ${f2(r.postGap.p99)} max ${f2(r.postGap.max)}; gaps >25 ms: ${r.postGap.over25}, >40 ms: ${r.postGap.over40}`);
  console.log(`  packets: worker out ${r.packets.out} in ${r.packets.in}; page sends ${r.packets.pageSends} recvs ${r.packets.pageRecvs} (dc ${r.packets.dc}, ws ${r.packets.ws})`);
  console.log(`  worker->page hop ms: p50 ${f2(r.hopOut.p50)} p95 ${f2(r.hopOut.p95)} max ${f2(r.hopOut.max)}; page->worker hop ms: p50 ${f2(r.hopIn.p50)} p95 ${f2(r.hopIn.p95)} max ${f2(r.hopIn.max)}; arrival->tick wait: p50 ${f2(r.pollWait.p50)} p95 ${f2(r.pollWait.p95)} max ${f2(r.pollWait.max)}`);
  console.log(`  page: rAF gap ms p50 ${f2(r.page.raf.p50)} p95 ${f2(r.page.raf.p95)} max ${f2(r.page.raf.max)}; event-loop lag ms p50 ${f2(r.page.lag.p50)} p95 ${f2(r.page.lag.p95)} max ${f2(r.page.lag.max)}; long tasks ${r.page.longTasks} (${f1(r.page.longTaskMs)} ms)`);
  if (r.ggrsAdvance) console.log(`  GGRS advance: ${r.ggrsAdvance.ok} frames, ${r.ggrsAdvance.stalled} stalled calls in ${r.ggrsAdvance.stallEpisodes} episodes; stall ms p50 ${f1(r.ggrsAdvance.stallMs.p50)} p95 ${f1(r.ggrsAdvance.stallMs.p95)} max ${f1(r.ggrsAdvance.stallMs.max)}; advance() incl. frame ms p50 ${f2(r.ggrsAdvance.advanceMs.p50)} max ${f2(r.ggrsAdvance.advanceMs.max)}; outgoing() drains ${r.ggrsAdvance.outgoingCalls}`);
  if (r.timeline) console.log(`  400 ms timeline (ms:event): ${r.timeline}`);
  if (r.saves) console.log(`  saves ${r.saves} (${f2(r.saveMs)} ms), loads ${r.loads} (${f2(r.loadMs)} ms)`);
}
if (report.wire) console.log(`-- wire (page to page, same clock) -- A->B ms: p50 ${f2(report.wire.aToB.p50)} p95 ${f2(report.wire.aToB.p95)} max ${f2(report.wire.aToB.max)}; B->A: p50 ${f2(report.wire.bToA.p50)} p95 ${f2(report.wire.bToA.p95)} max ${f2(report.wire.bToA.max)}`);
for (const r of Object.values(report.browsers)) delete r.raw;
writeFileSync(join(OUT, `${LABEL}.json`), JSON.stringify(report, null, 1));
browsers.forEach((b) => b.close());
process.exit(0);
