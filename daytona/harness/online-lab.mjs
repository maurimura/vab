// Online lab for Daytona USA: drives one, two or three headless Chromes of our own (never the
// user's) through the real bar page over CDP, sits them at the Daytona cabinet (the client's
// test hook vab.goTo("daytona"), then E; a watcher presses F), starts a linked race (free play:
// Start five times, then the accelerator held, steering now and then), and measures where time
// goes: in the emulator worker (frames run and posted, the core's frame cost, GGRS waits, the
// input delay and the others' input in hand, second by second), on the page (event-loop lag,
// rAF cadence, frames received) and on the wire; the handover when the second player joins and
// the start of a watcher (the state's size on the wire and each step's time); checkpoints
// (sent, received, mismatches); memory (wasm heap, page JS heap, the browser's processes).
// A one-way delay +- jitter can be injected on every WebRTC data-channel send, on both
// machines (so the round trip is twice --delay). Results print and go to <out>/<label>.json,
// with a screenshot of each page and the last game frame each worker posted
// (<label>-<browser>[-<when>].png, 496x384, as the worker has it).
//
//   node daytona/harness/online-lab.mjs --mode=solo|online [--join=attract|race] [--watch]
//        [--delay=60 --jitter=20] [--poke=600] [--rttA=300] [--seconds=60 --settle=20]
//        [--label=rtt120] [--site=http://localhost:8787] [--room=lab-x] [--out=.] [--scratch=<dir>]
//        [--replay=<input log .json>]
//
// --scratch: where the Chromes' profiles go (default: the system's temp folder). The JSON keeps
//   each worker's warnings and its input log ([frame, RetroPad mask]), enough to replay a run on
//   the headless core from power-on (the machines are deterministic).
// --join=race: player 2 sits once player 1's race is on (the handover carries a race), else
//   while player 1's machine is in the attract mode. Both then start a race on their cabinet.
// --watch: a third browser watches (F at the cabinet) once the race is on; Right then switches
//   it to player 2's screen and back with Left (frames saved before and after).
// --poke=N: browser B flips a byte of cabinet 0's work RAM (offset 0x80000, which the game
//   keeps: the checkpoint after it differs) N frames after the accelerator goes down.
// --rttA=N: browser A's worker is told the round trip was N ms when it goes online.
// --joinAt=N, --bStartAt=N (with --join=race): player 2 sits, and starts a race, once A's
//   machine has run N frames.
// --replay=<file>: browser A plays a recorded input log ([[frame, mask], ...], a report's
//   inputLog) from power-on, frame-exact, instead of keys (online too, through the session):
//   e.g. harness/overflow-start.json, a start timing that overflows the geometrizer's polygon
//   list at frame 2001 (patches/0004).
//
// Needs the site running (make dev BUCKET=local, with `make daytona` / `make upload-daytona
// R2_TARGET=--local` and `make upload-rom ROM=$HOME/Downloads/daytona.zip` done; --site for
// another) and a client built with the test hooks (any PROFILE but wasm-release). The worker
// probe wraps the core's wasm exports, whose names are minified per build: it reads them off the
// served glue.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deflateSync, crc32 } from "node:zlib";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => { const [k, ...v] = a.replace(/^--/, "").split("="); return [k, v.length ? v.join("=") : "1"]; }));
const MODE = argv.mode ?? "solo";
const JOIN = argv.join ?? "attract";
const WATCH = argv.watch === "1";
const DELAY = Number(argv.delay ?? 0);
const JITTER = Number(argv.jitter ?? 0);
const SECONDS = Number(argv.seconds ?? 60);
const SETTLE = Number(argv.settle ?? 20);
const LABEL = argv.label ?? `${MODE}${MODE === "online" ? `-${JOIN}` : ""}-${DELAY}-${JITTER}${WATCH ? "-watch" : ""}${argv.poke ? "-poke" : ""}`;
const SITE = argv.site ?? "http://localhost:8787";
const ROOM = argv.room ?? `lab-${Date.now().toString(36)}`;
const OUT = argv.out ?? ".";
const SCRATCH = mkdtempSync(join(argv.scratch ?? tmpdir(), "daytona-lab-"));
const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
/** With --poke=N, browser B flips a byte of the game's RAM N frames after the race starts. */
const POKE = Number(argv.poke ?? 0);
/** Where in cabinet 0's 1 MB work RAM: a word the stride-4 checkpoint hash reads and the game keeps. */
const POKE_AT = 0x80000;
/** With --rttA=N, browser A's worker is told the round trip was N ms when it goes online. */
const RTT_A = Number(argv.rttA ?? 0);
/**
 * With --replay=<file>, browser A's controls come from a file instead of keys: [[frame,
 * RetroPad mask], ...] from power-on, as a report's inputLog has them, put in the worker's own
 * input path after that many frames, so a run's start timing is exact (and repeatable).
 */
const REPLAY = argv.replay ? JSON.parse(readFileSync(argv.replay, "utf8")) : [];
/** With --join=race: player 2 sits once A's machine has run this many frames (default: 8 s into A's race). */
const JOIN_AT = Number(argv.joinAt ?? 0);
/** With --join=race: player 2 starts a race once A's machine has run this many frames (default: at once). */
const B_START_AT = Number(argv.bStartAt ?? 0);

// The core's wasm exports are minified; the served glue maps them: _retro_run=Module["_retro_run"]=wasmExports["N"].
const glue = await (await fetch(`${SITE}/daytona/daytona.mjs`)).text();
const exportName = (fn) => glue.match(new RegExp(`_${fn}=Module\\["_${fn}"\\]=wasmExports\\["([\\w$]+)"\\]`))?.[1];
const EXPORTS = {
  run: exportName("retro_run"), save: exportName("retro_serialize"), load: exportName("retro_unserialize"), reset: exportName("retro_reset"),
  size: exportName("retro_serialize_size"), ram: exportName("retro_get_memory_data"), set: exportName("daytona_set"), link: exportName("daytona_link_status"),
  memory: glue.match(/wasmMemory=wasmExports\["([\w$]+)"\]/)?.[1],
};
console.error("wasm export names:", EXPORTS);
if (Object.values(EXPORTS).some((name) => !name)) throw new Error(`could not map every export from ${SITE}/daytona/daytona.mjs`);

// ---------------------------------------------------------------- probes injected in the page
const PAGE_PROBE = `(() => {
  document.cookie = "welcomed=1; path=/"; // the first visit's help card would eat the first key
  const L = window.__lab = { delay: ${DELAY}, jitter: ${JITTER}, sends: [], recvs: [], rafGaps: [], lagGaps: [], longTasks: 0, longTaskMs: 0, wsSends: 0, wsRecvs: 0, dcSends: 0, dcRecvs: 0, workers: [], echoes: [],
    framesIn: [], ws: { sent: {}, recv: {} } };
  const abs = () => performance.timeOrigin + performance.now();
  const OWorker = window.Worker;
  window.Worker = function (url, opts) {
    const w = new OWorker(url, opts); L.workers.push(w);
    w.addEventListener("message", (e) => {
      if (e.data && e.data.type === "lab-echo") L.echoes.push(performance.now() - e.data.t);
      if (e.data && e.data.type === "frame") L.framesIn.push(abs());
    });
    return w;
  };
  window.Worker.prototype = OWorker.prototype;
  L.echo = () => { const w = L.workers.at(-1); if (w) w.postMessage({ type: "lab-echo", t: performance.now() }); };
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
  // The room's binary messages, by kind (web/room.js: 0 packet, 1 handover, 2 watch state, 3
  // watch inputs) and, for the pieces of a handover or a watch state, by "kind/id" too (the id,
  // an epoch or a stream, is the u32 after the kind, both ways).
  const tally = (side, bytes, n) => {
    const kind = bytes[4];
    const keys = [kind];
    if ((kind === 1 || kind === 2) && bytes.length >= 9) keys.push(kind + "/" + (bytes[5] | (bytes[6] << 8) | (bytes[7] << 16) | (bytes[8] << 24)));
    for (const key of keys) { const k = side[key] ??= { n: 0, bytes: 0, first: 0, last: 0 }; k.n++; k.bytes += n; k.first ||= abs(); k.last = abs(); }
  };
  const wsSend = WebSocket.prototype.send;
  WebSocket.prototype.send = function (data) {
    if (data instanceof Uint8Array && data.length > 4) {
      tally(L.ws.sent, data, data.length);
      if (data[4] === 0 && !isPing(data.buffer.slice(5))) { L.sends.push(abs()); L.wsSends++; }
    }
    return wsSend.call(this, data);
  };
  const wdesc = Object.getOwnPropertyDescriptor(WebSocket.prototype, "onmessage");
  Object.defineProperty(WebSocket.prototype, "onmessage", { configurable: true, get() { return wdesc.get.call(this); },
    set(fn) { wdesc.set.call(this, (ev) => {
      if (ev.data instanceof ArrayBuffer && ev.data.byteLength > 4) {
        const bytes = new Uint8Array(ev.data); const kind = bytes[4]; tally(L.ws.recv, bytes, ev.data.byteLength);
        if (kind === 0 && !isPing(ev.data.slice(5))) { L.recvs.push(abs()); L.wsRecvs++; }
      }
      return fn(ev); }); } });
  let lastRaf = 0;
  const raf = (t) => { if (lastRaf) L.rafGaps.push(t - lastRaf); lastRaf = t; if (L.rafGaps.length > 20000) L.rafGaps.splice(0, 10000); requestAnimationFrame(raf); };
  requestAnimationFrame(raf);
  let lastT = performance.now();
  const tick = () => { const now = performance.now(); L.lagGaps.push(now - lastT - 8); if (L.lagGaps.length > 40000) L.lagGaps.splice(0, 20000); lastT = now; setTimeout(tick, 8); };
  setTimeout(tick, 8);
  try { new PerformanceObserver((list) => { for (const e of list.getEntries()) { L.longTasks++; L.longTaskMs += e.duration; } }).observe({ type: "longtask", buffered: true }); } catch {}
})();`;

// ------------------------------------------------------- probes injected in the emulator worker
const WORKER_PROBE = `(() => {
  const W = globalThis.__lab = { events: [], t0: performance.timeOrigin + performance.now(), poke: 0, ticks: 0, tickRuns: [0, 0, 0, 0, 0, 0], tickDurs: [], timerLags: [], runs: [], runsAt: [], framePosts: [], frameDups: 0,
    outs: [], ins: [], pollWaits: [], stats: null, statsLog: [], saves: [], loads: [], resets: [], loadsAt: [], savesAt: [], msgs: [], posted: [], sets: [], checkOut: 0, checkIn: 0, stalls: 0, grab: false, grabbed: null, inputLog: [], warns: [], replay: null, replayAt: 0, handler: null };
  const oWarn = console.warn;
  console.warn = function (...a) { W.warns.push([W.runs.length, a.map(String).join(" ")]); return oWarn.apply(this, a); };
  const abs = () => performance.timeOrigin + performance.now();
  globalThis.addEventListener("message", (e) => { if (e.data && e.data.type === "lab-echo") postMessage({ type: "lab-echo", t: e.data.t }); });
  // The worker's onmessage, wrapped: what comes in (sizes and times), and an "online" message
  // can be told a different round trip (--rttA).
  const od = Object.getOwnPropertyDescriptor(globalThis, "onmessage") || Object.getOwnPropertyDescriptor(Object.getPrototypeOf(globalThis), "onmessage");
  const size = (x) => (x && (x.byteLength ?? x.length)) || 0;
  if (od && od.set) Object.defineProperty(globalThis, "onmessage", { configurable: true, get() { return od.get.call(globalThis); },
    set(fn) { od.set.call(globalThis, W.handler = (ev) => {
      const m = ev.data;
      if (m && m.type === "input") W.inputLog.push([W.runs.length, m.mask]);
      if (m && m.type && m.type !== "input" && m.type !== "lab-echo" && m.type !== "watch-inputs") W.msgs.push({ t: abs(), type: m.type, bytes: size(m.state) || size(m.bytes), to: m.to, epoch: m.epoch, view: m.view, roundTrip: m.roundTrip });
      if (m && m.type === "online" && W.forceRtt) { m.roundTrip = W.forceRtt; W.events.push("rtt forced " + W.forceRtt); }
      return fn(ev); }); } });
  let cur = null;
  let lastHash = -1;
  const pendingIns = [];
  // A wake-up of the worker's loop: a timer, or a message to itself (worker.js's Alarm).
  const wake = (fn, lag) => function (...a) {
    const t0 = performance.now();
    if (lag !== undefined) W.timerLags.push(t0 - lag);
    cur = { runs: 0 };
    while (pendingIns.length) W.pollWaits.push(t0 - pendingIns.shift());
    try { return fn.apply(this, a); } finally {
      if (cur.runs) { W.ticks++; W.tickDurs.push(performance.now() - t0); }
      W.tickRuns[Math.min(cur.runs, 5)]++; cur = null;
    }
  };
  const oST = globalThis.setTimeout;
  globalThis.setTimeout = function (fn, ms = 0, ...a) { const at = performance.now() + ms; return oST(wake(fn, at), ms, ...a); };
  const oPM = globalThis.postMessage;
  globalThis.postMessage = function (m, tr) {
    if (m && m.type === "frame") {
      const r = m.rgba; let h = 0; for (let i = 0; i < r.length; i += 1021) h = (Math.imul(h, 31) + r[i]) | 0;
      if (h === lastHash) W.frameDups++; lastHash = h;
      W.framePosts.push(abs());
      if (W.grab) { W.grab = false; W.grabbed = { width: m.width, height: m.height, rgba: btoaBytes(r), t: abs() }; }
    } else if (m && m.type === "netplay" && m.event !== "stats") { W.events.push(((abs() - W.t0) / 1000).toFixed(1) + "s:" + m.event + (m.frame !== undefined ? "@" + m.frame : "") + (m.seat !== undefined ? "/seat" + m.seat : ""));
    } else if (m && m.type === "netplay" && m.event === "stats") { W.stats = m; W.statsLog.push({ t: abs(), ping: m.ping, delay: m.delay, rollback: m.rollback, fps: m.fps, stalls: m.stalls, stallMs: m.stallMs, look: m.look, prefills: m.prefills, framesAhead: m.framesAhead });
    } else if (m && m.type) W.posted.push({ t: abs(), type: m.type, bytes: size(m.state) || size(m.bytes), to: m.to, epoch: m.epoch, stream: m.stream });
    return oPM.call(this, m, tr);
  };
  const btoaBytes = (u8) => { let s = ""; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000)); return btoa(s); };
  const isControl = (buf) => buf instanceof ArrayBuffer && buf.byteLength >= 13 && new Uint8Array(buf)[0] === 0xff && new Uint8Array(buf)[1] === 0xff;
  const MP = MessagePort.prototype;
  const oMPpm = MP.postMessage;
  MP.postMessage = function (m, tr) {
    if (Array.isArray(m) && m.length === 2 && typeof m[0] === "number") { if (isControl(m[1])) W.checkOut++; else W.outs.push(abs()); }
    else if (m instanceof Int16Array) W.audioPosts = (W.audioPosts || 0) + 1;
    return oMPpm.call(this, m, tr);
  };
  const d = Object.getOwnPropertyDescriptor(MP, "onmessage");
  Object.defineProperty(MP, "onmessage", { configurable: true, get() { return d.get.call(this); },
    set(fn) {
      const woken = wake(fn);
      d.set.call(this, (ev) => {
        const m = ev.data;
        if (Array.isArray(m) && m.length === 2 && typeof m[0] === "number") { if (isControl(m[1])) W.checkIn++; else { W.ins.push(abs()); pendingIns.push(performance.now()); } return fn(ev); }
        return m === 0 ? woken(ev) : fn(ev); // 0: the Alarm's message to itself
      });
    } });
  W.adv = []; // [t, ok, ms] per session_advance
  W.recvAt = []; W.outgoingAt = []; W.outgoingCounts = [];
  const wrapped = new WeakMap();
  const cString = (mem, ptr) => { const b = new Uint8Array(mem.buffer); let e = ptr; while (b[e]) e++; return new TextDecoder().decode(b.slice(ptr, e)); };
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
    if (!ex || typeof ex["${EXPORTS.run}"] !== "function" || typeof ex["${EXPORTS.link}"] !== "function") return ex;
    if (wrapped.has(ex)) return wrapped.get(ex);
    W.wrappedExports = (W.wrappedExports || 0) + 1;
    const mem = ex["${EXPORTS.memory}"];
    const out = {}; for (const k of Object.keys(ex)) out[k] = ex[k];
    const time = (name, arr, after) => { const f = ex[name]; out[name] = function (...a) { const t0 = performance.now(); try { return f.apply(this, a); } finally { arr.push(performance.now() - t0); after && after(t0); } }; };
    time("${EXPORTS.run}", W.runs, () => {
      W.runsAt.push(abs()); if (cur) cur.runs++;
      if (W.replay && W.handler) for (const n = W.runs.length; W.replayAt < W.replay.length && W.replay[W.replayAt][0] <= n; W.replayAt++) W.handler({ data: { type: "input", mask: W.replay[W.replayAt][1] } });
      if (W.poke && W.runs.length === W.poke) { const ptr = ex["${EXPORTS.ram}"](2); new Uint8Array(mem.buffer)[ptr + ${POKE_AT}] ^= 0xff; W.poked = W.runs.length; W.pokedAt = abs(); }
    });
    time("${EXPORTS.save}", W.saves, () => W.savesAt.push(abs()));
    time("${EXPORTS.load}", W.loads, () => W.loadsAt.push([abs(), W.runs.length]));
    time("${EXPORTS.reset}", W.resets);
    const set = ex["${EXPORTS.set}"];
    out["${EXPORTS.set}"] = function (k, v) { W.sets.push([abs(), cString(mem, k), cString(mem, v)]); return set.call(this, k, v); };
    W.linkStatus = () => { try { return JSON.parse(cString(mem, ex["${EXPORTS.link}"]())); } catch (e) { return String(e); } };
    W.stateSize = () => ex["${EXPORTS.size}"]();
    W.heapBytes = () => mem.buffer.byteLength;
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

const KEYS = { ArrowUp: 38, ArrowDown: 40, ArrowLeft: 37, ArrowRight: 39, Digit1: 49, Digit5: 53, KeyE: 69, KeyF: 70, KeyZ: 90, KeyX: 88, Escape: 27 };
const keyOf = (code) => ({ Digit1: "1", Digit5: "5", KeyE: "e", KeyF: "f", KeyZ: "z", KeyX: "x", Escape: "Escape" })[code] ?? code;

class Browser {
  chrome; cdp; page; workers = []; name;
  async launch(name) {
    this.name = name;
    const port = 9400 + Math.floor(Math.random() * 400);
    const profile = mkdtempSync(join(SCRATCH, `chrome-${name}-`));
    this.chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--window-size=1280,800",
      "--autoplay-policy=no-user-gesture-required", "--use-angle=metal", "--ignore-gpu-blocklist", "about:blank"], { stdio: "ignore" });
    console.error(`[${name}] Chrome pid ${this.chrome.pid}, port ${port}`);
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
          for (let i = 0; i < 50; i++) {
            const { result } = await this.cdp.send("Runtime.evaluate", { expression: "typeof setTimeout === 'function' && typeof postMessage === 'function'", returnByValue: true }, sessionId);
            if (result?.value) break;
            await sleep(2);
          }
          const probe = await this.cdp.send("Runtime.evaluate", { expression: WORKER_PROBE + " [typeof setTimeout, typeof postMessage, typeof WebAssembly.instantiateStreaming].join()", returnByValue: true }, sessionId);
          if (probe.exceptionDetails) console.error(`[${this.name}] WORKER PROBE THREW:`, probe.exceptionDetails.exception?.description ?? probe.exceptionDetails.text);
          if (REPLAY.length && this.name === "A") await this.cdp.send("Runtime.evaluate", { expression: `globalThis.__lab.replay = ${JSON.stringify(REPLAY)}` }, sessionId);
          this.workers.push(sessionId);
          console.error(`[${this.name}] worker attached and probed (${probe.result?.value})${REPLAY.length && this.name === "A" ? `; replaying ${REPLAY.length} input changes` : ""}`);
        } catch (e) { console.error(`[${this.name}] worker probe failed: ${e.message}`); }
        return;
      }
      if (waitingForDebugger) this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId).catch(() => {});
    }
    if (msg.method === "Runtime.exceptionThrown" && this.workers.includes(msg.sessionId)) console.error(`[${this.name}] WORKER error:`, (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text ?? "").split("\n").slice(0, 4).join(" | "));
    if (msg.method === "Runtime.consoleAPICalled" && this.workers.includes(msg.sessionId)) console.error(`[${this.name}] WORKER ${msg.params.type}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 400));
    if (msg.method === "Runtime.exceptionThrown" && msg.sessionId === this.page) console.error(`[${this.name}] page error:`, msg.params.exceptionDetails.exception?.description?.split("\n")[0] ?? msg.params.exceptionDetails.text);
    if (msg.method === "Runtime.consoleAPICalled" && msg.sessionId === this.page && msg.params.type !== "log" && msg.params.type !== "debug") console.error(`[${this.name}] page ${msg.params.type}:`, msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 300));
  }
  get worker() { return this.workers.at(-1); }
  async eval(expression, sessionId = this.page) {
    const { result, exceptionDetails } = await this.cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  }
  async open(url) { await this.cdp.send("Page.navigate", { url }, this.page); }
  async down(code) { await this.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: keyOf(code), code, windowsVirtualKeyCode: KEYS[code], text: keyOf(code).length === 1 ? keyOf(code) : undefined }, this.page); }
  async up(code) { await this.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: keyOf(code), code, windowsVirtualKeyCode: KEYS[code] }, this.page); }
  async key(code, hold = 60) { await this.down(code); await sleep(hold); await this.up(code); }
  async click(x, y) {
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }, this.page);
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }, this.page);
  }
  async screenshot(file) { const { data } = await this.cdp.send("Page.captureScreenshot", { format: "png" }, this.page); writeFileSync(file, Buffer.from(data, "base64")); }
  /** The next frame the worker posts, as a PNG file. */
  async frame(file) {
    await this.eval("globalThis.__lab.grab = true, globalThis.__lab.grabbed = null", this.worker);
    for (let i = 0; i < 50; i++) { if (await this.eval("!!globalThis.__lab.grabbed", this.worker)) break; await sleep(50); }
    const g = await this.eval("globalThis.__lab.grabbed", this.worker);
    if (!g) return console.error(`[${this.name}] no frame to save for ${file}`);
    writeFileSync(file, png(g.width, g.height, Buffer.from(g.rgba, "base64")));
  }
  /** Resident memory of this Chrome and all its processes, MB. */
  rssMB() {
    try {
      const rows = execFileSync("ps", ["-A", "-o", "pid=,ppid=,rss="]).toString().trim().split("\n").map((l) => l.trim().split(/\s+/).map(Number));
      const kids = new Map(); for (const [pid, ppid] of rows) kids.set(ppid, [...(kids.get(ppid) ?? []), pid]);
      const rss = new Map(rows.map(([pid, , kb]) => [pid, kb]));
      let total = 0; const stack = [this.chrome.pid];
      while (stack.length) { const p = stack.pop(); total += rss.get(p) ?? 0; stack.push(...(kids.get(p) ?? [])); }
      return total / 1024;
    } catch { return NaN; }
  }
  close() { try { this.cdp.close(); } catch {} try { this.chrome.kill(); } catch {} }
}

/** An RGBA picture as a PNG file (no dependencies). */
function png(width, height, rgba) {
  const chunk = (type, data) => { const b = Buffer.alloc(12 + data.length); b.writeUInt32BE(data.length, 0); b.write(type, 4, "ascii"); data.copy(b, 8); b.writeUInt32BE(crc32(b.subarray(4, 8 + data.length)) >>> 0, 8 + data.length); return b; };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 6;
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) { raw[y * (width * 4 + 1)] = 0; rgba.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4); for (let x = 0; x < width; x++) raw[y * (width * 4 + 1) + 1 + x * 4 + 3] = 255; }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

// ------------------------------------------------------------------------------------ the run
const q = (arr, p) => { if (!arr.length) return NaN; const s = [...arr].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(s.length * p))]; };
const f1 = (x) => (Number.isFinite(x) ? x.toFixed(1) : "-");
const f2 = (x) => (Number.isFinite(x) ? x.toFixed(2) : "-");
const max = (arr) => (arr.length ? Math.max(...arr) : NaN);
/** Greedy monotone match: for each a[i], the first b[j] >= a[i] not yet used; returns b-a. */
const hops = (a, b) => { const out = []; let j = 0; for (const t of a) { while (j < b.length && b[j] < t) j++; if (j >= b.length) break; out.push(b[j] - t); j++; } return out; };
const MB = (n) => (n / 1048576).toFixed(2);

const browsers = [];
process.on("exit", () => browsers.forEach((b) => b.close()));
process.on("SIGINT", () => process.exit(1));
const T0 = Date.now();
const at = () => ((Date.now() - T0) / 1000).toFixed(1);

/**
 * Opens the bar, goes to the Daytona cabinet and presses `key` there (E sits, F watches): with
 * the client's test hook (vab.goTo, any build but wasm-release), or on the deployed site by
 * walking from the spawn (-3,-2) one cell to (-2,-2) with Right+Down (64 px/s, a cell is 16 px
 * across), where the Daytona cabinet at (-2,-3) is the nearest, and checking which core the
 * worker loaded (Esc and a little more walking otherwise).
 */
async function goToCabinet(b, key = "KeyE") {
  await b.open(`${SITE}/?room=${ROOM}`);
  for (let i = 0; i < 300; i++) { if (await b.eval("typeof window.emulatorPlay === 'function' && window.__lab && window.__lab.rafGaps.length > 30").catch(() => false)) break; await sleep(200); }
  await sleep(1500);
  await b.click(640, 400);
  await sleep(200);
  const hook = await b.eval("!!(window.vab && window.vab.goTo)").catch(() => false);
  let pressedAt = Date.now();
  if (hook) {
    await b.eval(`window.vab.goTo("daytona")`);
    for (let i = 0; i < 100; i++) { const s = await b.eval("window.vab.state()").catch(() => null); if (s?.go_to?.name === "daytona" && s.go_to.cell) break; await sleep(100); }
    await sleep(300);
    pressedAt = Date.now();
    for (let tries = 0; tries < 6 && !b.workers.length; tries++) {
      await b.key(key);
      for (let i = 0; i < 30 && !b.workers.length; i++) await sleep(100);
    }
  } else {
    const walks = [[["ArrowRight", "ArrowDown"], 350], [["ArrowRight", "ArrowDown"], 120], [["ArrowRight", "ArrowDown"], 120], [["ArrowLeft", "ArrowUp"], 180], [["ArrowRight"], 150]];
    let seated = false;
    for (const [keys, ms] of walks) {
      for (const k of keys) await b.down(k);
      await sleep(ms);
      for (const k of keys) await b.up(k);
      await sleep(400);
      const before = b.workers.length;
      pressedAt = Date.now();
      for (let tries = 0; tries < 3 && b.workers.length === before; tries++) {
        await b.key(key);
        for (let i = 0; i < 30 && b.workers.length === before; i++) await sleep(100);
      }
      if (b.workers.length === before) continue;
      let core = "";
      for (let i = 0; i < 100 && !core; i++) { core = await b.eval("(performance.getEntriesByType('resource').map((e) => e.name).find((n) => /\\/(daytona|supermodel|fbneo)\\//.test(n)) || '')", b.worker).catch(() => ""); if (!core) await sleep(100); }
      console.error(`[${b.name}] t=${at()}s walked ${keys.join("+")} ${ms} ms, pressed ${keyOf(key)}: worker loads ${core || "(unknown yet)"}`);
      if (core.includes("/daytona/")) { seated = true; break; }
      await b.key("Escape");
      await sleep(1500);
    }
    if (!seated) throw new Error(`[${b.name}] could not reach the Daytona cabinet by walking (no test hook on this build)`);
  }
  if (!b.workers.length) throw new Error(`[${b.name}] no emulator worker appeared after pressing ${key}`);
  let i = 0;
  for (; i < 300; i++) { if (await b.eval("globalThis.__lab && globalThis.__lab.runs.length > 10", b.worker).catch(() => false)) break; await sleep(200); }
  const firstRun = await b.eval("globalThis.__lab.runsAt[0] - (performance.timeOrigin + performance.now() - Date.now())", b.worker).catch(() => NaN);
  console.error(`[${b.name}] t=${at()}s ${key === "KeyF" ? "watching" : "seated"}; first frame ${((firstRun - pressedAt) / 1000).toFixed(2)}s after the key`);
  return pressedAt;
}

/** Until browser b's machine has run `frames` frames. */
async function framesOf(b, frames) {
  for (let i = 0; i < 1500; i++) { if (await b.eval(`globalThis.__lab.runs.length >= ${frames}`, b.worker).catch(() => false)) return; await sleep(100); }
}

/** Start five times (course, transmission...), as daytona/bench.mjs's PLAY, then the accelerator held. */
async function startRace(b) {
  // Past the boot's NETWORK CHECKING screens first (a player sits, then presses Start).
  for (let i = 0; i < 100; i++) { if (await b.eval("globalThis.__lab.runs.length >= 300", b.worker).catch(() => true)) break; await sleep(100); }
  for (let i = 0; i < 5; i++) { await b.key("Digit1", 300); await sleep(4900); }
  await b.down("ArrowUp");
}

/** Steers now and then while driving: a press of Left or Right every two seconds or so. */
let driving = true;
async function drive(b) {
  let side = 0;
  while (driving) {
    await sleep(1600 + Math.random() * 800);
    if (!driving) break;
    await b.key(side++ % 2 ? "ArrowLeft" : "ArrowRight", 200 + Math.random() * 300).catch(() => {});
    await b.down("ArrowUp").catch(() => {}); // still held
  }
}

const statsOf = (b) => b.eval("JSON.stringify(globalThis.__lab.stats)", b.worker);
async function waitOnline(...bs) {
  for (let i = 0; i < 400; i++) {
    const ok = await Promise.all(bs.map((b) => b.eval("!!(globalThis.__lab.stats && globalThis.__lab.stats.ping !== undefined)", b.worker).catch(() => false)));
    if (ok.every(Boolean)) return true;
    await sleep(200);
  }
  return false;
}

const A = new Browser(); browsers.push(A);
await A.launch("A");
await goToCabinet(A);
if (RTT_A) await A.eval(`globalThis.__lab.forceRtt = ${RTT_A}`, A.worker);
let B, C, bSatAt, raceAt;
if (MODE === "online") {
  if (JOIN === "race") {
    console.error(`[A] t=${at()}s starting a race alone`);
    if (REPLAY.length && JOIN_AT) await framesOf(A, JOIN_AT);
    else {
      await (REPLAY.length ? framesOf(A, REPLAY.find(([, mask]) => mask & 16)?.[0] ?? 0) : startRace(A));
      raceAt = Date.now();
      if (JOIN_AT) await framesOf(A, JOIN_AT);
      else await sleep(8000);
    }
  }
  B = new Browser(); browsers.push(B);
  await B.launch("B");
  bSatAt = await goToCabinet(B);
  const ok = await waitOnline(A, B);
  console.error(`t=${at()}s online ${ok}:`, await statsOf(A), await statsOf(B));
  if (JOIN === "race") {
    // Player 1 keeps racing; player 2 starts their own on their cabinet.
    if (B_START_AT) await framesOf(A, B_START_AT);
    await startRace(B);
    raceAt ??= Date.now();
  } else {
    console.error(`t=${at()}s both start a race`);
    await Promise.all([startRace(A), startRace(B)]);
    raceAt = Date.now();
  }
} else if (REPLAY.length) {
  // The replay drives: on once it has put the accelerator down.
  await framesOf(A, REPLAY.find(([, mask]) => mask & 16)?.[0] ?? 0);
  raceAt = Date.now();
} else {
  await startRace(A);
  raceAt = Date.now();
}
console.error(`t=${at()}s racing; driving`);
const drivers = [REPLAY.length ? null : A, B].filter(Boolean).map(drive);
if (POKE && B) { await B.eval(`globalThis.__lab.poke = globalThis.__lab.runs.length + ${POKE}`, B.worker); console.error(`B will flip RAM byte +0x${POKE_AT.toString(16)} after ${POKE} more frames`); }

let watchInfo;
if (WATCH && B) {
  await sleep(5000);
  C = new Browser(); browsers.push(C);
  await C.launch("C");
  const fAt = await goToCabinet(C, "KeyF");
  for (let i = 0; i < 200; i++) { if (await C.eval("globalThis.__lab.loads.length > 0 && globalThis.__lab.runs.length > 30", C.worker).catch(() => false)) break; await sleep(200); }
  await sleep(3000);
  await C.frame(join(OUT, `${LABEL}-C-start.png`));
  await A.frame(join(OUT, `${LABEL}-A-at-watch-start.png`));
  await B.frame(join(OUT, `${LABEL}-B-at-watch-start.png`));
  await C.click(640, 400);
  await C.key("ArrowRight", 200);
  await sleep(3000);
  await C.frame(join(OUT, `${LABEL}-C-after-right.png`));
  watchInfo = { fAt };
}

console.error(`t=${at()}s settling ${SETTLE}s ...`);
await sleep(SETTLE * 1000);

const t0 = Date.now();
console.error(`t=${at()}s measuring ${SECONDS}s ...`);
await A.eval(`window.__echoTimer = setInterval(() => window.__lab.echo(), 50)`);
const rssSamples = new Map(browsers.map((b) => [b.name, []]));
for (let s = 0; s < SECONDS; s += 5) { await sleep(Math.min(5, SECONDS - s) * 1000); for (const b of browsers) rssSamples.get(b.name).push(b.rssMB()); }
await A.eval(`clearInterval(window.__echoTimer)`);
const t1 = Date.now();
if (C) { await C.key("ArrowLeft", 200); await sleep(2500); await C.frame(join(OUT, `${LABEL}-C-after-left.png`)); }
driving = false;

const report = { label: LABEL, mode: MODE, join: MODE === "online" ? JOIN : undefined, delay: DELAY, jitter: JITTER, seconds: (t1 - t0) / 1000, room: ROOM, browsers: {} };
const clocks = {};
for (const b of browsers) {
  const W = await b.eval("JSON.stringify(globalThis.__lab)", b.worker).then(JSON.parse);
  const P = await b.eval("JSON.stringify(window.__lab)").then(JSON.parse);
  const wOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()", b.worker);
  const pOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()");
  clocks[b.name] = { wOff, pOff };
  for (const k of ["outs", "ins", "runsAt", "framePosts", "savesAt"]) W[k] = W[k].map((t) => t - wOff);
  W.loadsAt = W.loadsAt.map(([t, n]) => [t - wOff, n]);
  for (const k of ["msgs", "posted"]) W[k] = W[k].map((m) => ({ ...m, t: m.t - wOff }));
  W.sets = W.sets.map(([t, k, v]) => [t - wOff, k, v]);
  for (const k of ["sends", "recvs", "framesIn"]) P[k] = P[k].map((t) => t - pOff);
  for (const side of ["sent", "recv"]) for (const k of Object.values(P.ws[side])) { k.first -= pOff; k.last -= pOff; }
  const link = await b.eval("JSON.stringify(globalThis.__lab.linkStatus())", b.worker).then(JSON.parse).catch((e) => e.message);
  const heap = await b.eval("globalThis.__lab.heapBytes()", b.worker).catch(() => NaN);
  const stateSize = await b.eval("globalThis.__lab.stateSize()", b.worker).catch(() => NaN);
  const jsHeap = await b.eval("performance.memory ? performance.memory.usedJSHeapSize : NaN").catch(() => NaN);
  const win = (arr) => arr.filter((t) => t >= t0 && t <= t1);
  const runsAt = win(W.runsAt), posts = win(W.framePosts), framesIn = win(P.framesIn);
  const frac = W.runsAt.length ? Math.min(1, SECONDS * 1000 / (Date.now() - W.runsAt[0])) : 1;
  const tail = (arr) => arr.slice(Math.floor(arr.length * (1 - frac)));
  const runs = tail(W.runs), tickDurs = tail(W.tickDurs), timerLags = tail(W.timerLags), pollWaits = tail(W.pollWaits);
  const outs = win(W.outs), ins = win(W.ins), sends = win(P.sends), recvs = win(P.recvs);
  const hopOut = hops(outs, sends), hopIn = hops(recvs, ins);
  if (P.echoes.length) console.error(`[${b.name}] page->worker->page echo ms: p50 ${f2(q(P.echoes, 0.5))} p95 ${f2(q(P.echoes, 0.95))} max ${f2(max(P.echoes))} (${P.echoes.length} echoes)`);
  const postGaps = posts.slice(1).map((t, i) => t - posts[i]);
  const perSecond = (arr) => { const out = []; for (let s = t0; s + 1000 <= t1; s += 1000) out.push(arr.filter((t) => t >= s && t < s + 1000).length); return out; };
  const stats = W.statsLog.filter((s) => s.t >= t0 && s.t <= t1);
  const adv = W.adv.filter(([t]) => t >= t0 && t <= t1);
  const stalls = []; let stallStart = null, okCount = 0, failCount = 0;
  for (const [t, ok] of adv) { if (ok) { okCount++; if (stallStart !== null) { stalls.push(t - stallStart); stallStart = null; } } else { failCount++; if (stallStart === null) stallStart = t; } }
  const advMs = adv.filter(([, ok]) => ok).map(([, , ms]) => ms);
  const r = report.browsers[b.name] = {
    emulatedFps: runsAt.length / report.seconds, shownFps: posts.length / report.seconds, pageFramesFps: framesIn.length / report.seconds,
    perSecond: { emulated: perSecond(runsAt), shown: perSecond(posts) },
    run: { p50: q(runs, 0.5), p95: q(runs, 0.95), p99: q(runs, 0.99), max: max(runs) },
    tick: { perSec: tickDurs.length / report.seconds, runsPerWake: W.tickRuns, dur: { p50: q(tickDurs, 0.5), p95: q(tickDurs, 0.95), max: max(tickDurs) } },
    timerLag: { p50: q(timerLags, 0.5), p95: q(timerLags, 0.95), max: max(timerLags) },
    pollWait: { n: pollWaits.length, p50: q(pollWaits, 0.5), p95: q(pollWaits, 0.95), max: max(pollWaits) },
    postGap: { p50: q(postGaps, 0.5), p95: q(postGaps, 0.95), p99: q(postGaps, 0.99), max: max(postGaps), over25: postGaps.filter((g) => g > 25).length, over40: postGaps.filter((g) => g > 40).length },
    frameDups: W.frameDups,
    saves: W.saves.length, saveMs: W.saves, loads: W.loads.length, loadMs: W.loads, resets: W.resets.length, resetMs: W.resets,
    checkpoints: { sent: W.checkOut, received: W.checkIn },
    packets: { out: outs.length, in: ins.length, pageSends: sends.length, pageRecvs: recvs.length, dc: [P.dcSends, P.dcRecvs], ws: [P.wsSends, P.wsRecvs] },
    room: P.ws,
    hopOut: { p50: q(hopOut, 0.5), p95: q(hopOut, 0.95), max: max(hopOut) },
    hopIn: { p50: q(hopIn, 0.5), p95: q(hopIn, 0.95), max: max(hopIn) },
    page: { raf: { p50: q(P.rafGaps, 0.5), p95: q(P.rafGaps, 0.95), max: max(P.rafGaps) }, lag: { p50: q(P.lagGaps, 0.5), p95: q(P.lagGaps, 0.95), max: max(P.lagGaps) }, longTasks: P.longTasks, longTaskMs: P.longTaskMs },
    ggrs: stats.length ? { ping: { min: Math.min(...stats.map((s) => s.ping)), max: Math.max(...stats.map((s) => s.ping)) }, delay: stats.at(-1).delay, delays: [...new Set(stats.map((s) => s.delay))], rollback: stats.at(-1).rollback, fpsReported: stats.map((s) => s.fps),
      waits: stats.reduce((n, s) => n + (s.stalls || 0), 0), longestWait: Math.max(0, ...stats.map((s) => s.stallMs || 0)),
      trajectory: W.statsLog.map((s) => `${s.delay}${s.stalls ? `/${s.stalls}w${s.stallMs}` : ""}${s.look?.length ? `[${s.look.join(",")}]` : ""}${s.prefills ? `p${s.prefills}` : ""}`).join(" ") } : null,
    events: W.events.join(" "), poked: W.poked, pokedAt: W.pokedAt && W.pokedAt - wOff,
    desyncs: W.events.filter((e) => e.includes("desync")).length,
    ggrsAdvance: { ok: okCount, stalled: failCount, stallEpisodes: stalls.length, stallMs: { p50: q(stalls, 0.5), p95: q(stalls, 0.95), max: max(stalls) }, advanceMs: { p50: q(advMs, 0.5), p95: q(advMs, 0.95), max: max(advMs) } },
    memory: { wasmHeapMB: heap / 1048576, stateSize, pageJsHeapMB: jsHeap / 1048576, chromeRssMB: rssSamples.get(b.name) },
    link, views: W.sets.filter(([, k]) => k === "view").map(([t, , v]) => `${((t - T0) / 1000).toFixed(1)}s:${v}`).join(" "),
    options: W.sets.filter(([, k]) => k !== "view").map(([, k, v]) => `${k}=${v}`).join(" "),
    warns: W.warns, inputLog: W.inputLog, msgs: W.msgs, posted: W.posted, loadsAt: W.loadsAt, savesAt: W.savesAt, 
    raw: { outs, ins, sends, recvs, runsAt: W.runsAt },
  };
  await b.screenshot(join(OUT, `${LABEL}-${b.name}-page.png`));
  await b.frame(join(OUT, `${LABEL}-${b.name}.png`));
}

// The handover: player 1's machine captures, deflates and hands its game to player 2 through the
// room; both reset and load it, then play on together. Times on the one machine clock.
const rel = (t) => (Number.isFinite(t) ? `${((t - T0) / 1000).toFixed(2)}s` : "-");
function handovers() {
  const a = report.browsers.A, b = report.browsers.B;
  if (!b) return [];
  const out = [];
  for (const cap of a.posted.filter((m) => m.type === "captured")) {
    const capIn = a.msgs.filter((m) => m.type === "capture" && m.epoch === cap.epoch).at(-1);
    const bOnline = b.msgs.find((m) => m.type === "online" && m.epoch === cap.epoch);
    const aOnline = a.msgs.find((m) => m.type === "online" && m.epoch === cap.epoch);
    const loadB = b.loadsAt.find(([t]) => bOnline && t >= bOnline.t);
    const loadA = a.loadsAt.find(([t]) => aOnline && t >= aOnline.t);
    const runB = b.raw.runsAt.find((t) => loadB && t > loadB[0]);
    const runA = a.raw.runsAt.find((t) => loadA && t > loadA[0]);
    const saveA = a.savesAt.filter((t) => capIn && t >= capIn.t && t <= cap.t).at(0);
    out.push({ epoch: cap.epoch, deflated: cap.bytes, stateSize: a.memory.stateSize,
      capture: capIn?.t, saved: saveA, captured: cap.t, onlineB: bOnline?.t, onlineBBytes: bOnline?.bytes, roundTripB: bOnline?.roundTrip, loadB: loadB?.[0], firstRunB: runB, onlineA: aOnline?.t, loadA: loadA?.[0], firstRunA: runA,
      roomSent: a.room.sent[`1/${cap.epoch}`], roomRecv: b.room.recv[`1/${cap.epoch}`] });
  }
  return out;
}
report.handovers = handovers();
if (C) {
  const c = report.browsers.C, a = report.browsers.A;
  const snap = a.msgs.filter((m) => m.type === "snapshot");
  const ws = a.posted.filter((m) => m.type === "watch-state");
  const cIn = c.msgs.filter((m) => m.type === "watch-state");
  const cLoad = c.loadsAt.at(-1);
  report.watch = { fPressed: watchInfo.fAt, cReady: c.posted.find((m) => m.type === "ready")?.t, snapshotAsked: snap.map((m) => m.t), watchStatesSent: ws.map((m) => ({ t: m.t, bytes: m.bytes, to: m.to })),
    watchStatesIn: cIn.map((m) => ({ t: m.t, bytes: m.bytes })), cLoad: cLoad?.[0], cFirstRun: c.raw.runsAt.find((t) => cLoad && t > cLoad[0]), roomRecv: c.room.recv[2], roomSent: a.room.sent[2], views: c.views };
}
if (B) {
  const a = report.browsers.A.raw, bb = report.browsers.B.raw;
  const ab = hops(a.sends, bb.recvs), ba = hops(bb.sends, a.recvs);
  report.wire = { aToB: { p50: q(ab, 0.5), p95: q(ab, 0.95), max: max(ab) }, bToA: { p50: q(ba, 0.5), p95: q(ba, 0.95), max: max(ba) } };
}

// ------------------------------------------------------------------------------- print
console.log(`\n== ${LABEL}: ${MODE}${MODE === "online" ? `, player 2 joins in the ${JOIN === "race" ? "race" : "attract mode"}` : ""}${DELAY ? `, injected one-way ${DELAY}±${JITTER} ms on each side` : ""}${WATCH ? ", a watcher" : ""}${POKE ? `, poke after ${POKE} frames` : ""}; measured ${f1(report.seconds)} s after settling ${SETTLE} s ==`);
for (const [name, r] of Object.entries(report.browsers)) {
  console.log(`-- ${name} --`);
  console.log(`  emulated ${f1(r.emulatedFps)} fps, posted ${f1(r.shownFps)} fps, page got ${f1(r.pageFramesFps)} fps, duplicate frames ${r.frameDups}` + (r.ggrs ? `, GGRS ping ${r.ggrs.ping.min}-${r.ggrs.ping.max} ms, input delay ${r.ggrs.delay} frames (seen ${r.ggrs.delays.join("/")}), rollback ${r.ggrs.rollback}, waits ${r.ggrs.waits} (longest ${r.ggrs.longestWait} ms)` : ""));
  console.log(`  per second (emulated): min ${Math.min(...r.perSecond.emulated)} max ${Math.max(...r.perSecond.emulated)}; (posted): min ${Math.min(...r.perSecond.shown)} max ${Math.max(...r.perSecond.shown)}`);
  if (r.ggrs) console.log(`  fps reported each second: ${r.ggrs.fpsReported.join(" ")}`);
  if (r.ggrs) console.log(`  each second since the session began, delay[/waits w longest ms][look least,median,most]: ${r.ggrs.trajectory}`);
  if (r.events) console.log(`  netplay events (s since the worker started): ${r.events}${r.poked ? `; RAM flipped after frame ${r.poked}` : ""}`);
  console.log(`  checkpoints sent ${r.checkpoints.sent}, received ${r.checkpoints.received}; desync reports ${r.desyncs}`);
  console.log(`  retro_run ms: p50 ${f2(r.run.p50)} p95 ${f2(r.run.p95)} p99 ${f2(r.run.p99)} max ${f2(r.run.max)}`);
  console.log(`  wake-ups with a frame/s ${f1(r.tick.perSec)}; frames per wake-up [0,1,2,3,4,5+] = ${r.tick.runsPerWake.join(" ")}; ms p50 ${f2(r.tick.dur.p50)} p95 ${f2(r.tick.dur.p95)} max ${f2(r.tick.dur.max)}`);
  console.log(`  frame post gap ms: p50 ${f2(r.postGap.p50)} p95 ${f2(r.postGap.p95)} p99 ${f2(r.postGap.p99)} max ${f2(r.postGap.max)}; gaps >25 ms: ${r.postGap.over25}, >40 ms: ${r.postGap.over40}`);
  console.log(`  packets: worker out ${r.packets.out} in ${r.packets.in}; page sends ${r.packets.pageSends} recvs ${r.packets.pageRecvs} (dc ${r.packets.dc}, ws ${r.packets.ws}); worker->page hop p50 ${f2(r.hopOut.p50)} p95 ${f2(r.hopOut.p95)}; page->worker p50 ${f2(r.hopIn.p50)} p95 ${f2(r.hopIn.p95)}; arrival->wake p50 ${f2(r.pollWait.p50)} p95 ${f2(r.pollWait.p95)}`);
  console.log(`  page: rAF gap ms p50 ${f2(r.page.raf.p50)} p95 ${f2(r.page.raf.p95)} max ${f2(r.page.raf.max)}; event-loop lag ms p50 ${f2(r.page.lag.p50)} p95 ${f2(r.page.lag.p95)} max ${f2(r.page.lag.max)}; long tasks ${r.page.longTasks} (${f1(r.page.longTaskMs)} ms)`);
  if (r.ggrsAdvance.ok || r.ggrsAdvance.stalled) console.log(`  GGRS advance: ${r.ggrsAdvance.ok} frames, ${r.ggrsAdvance.stalled} stalled calls in ${r.ggrsAdvance.stallEpisodes} episodes; stall ms p50 ${f1(r.ggrsAdvance.stallMs.p50)} p95 ${f1(r.ggrsAdvance.stallMs.p95)} max ${f1(r.ggrsAdvance.stallMs.max)}`);
  console.log(`  memory: wasm heap ${f1(r.memory.wasmHeapMB)} MB, page JS heap ${f1(r.memory.pageJsHeapMB)} MB, Chrome processes RSS ${r.memory.chromeRssMB.map((x) => x.toFixed(0)).join("/")} MB; state ${r.memory.stateSize} bytes`);
  console.log(`  saves ${r.saves} (ms ${r.saveMs.map(f1).join(" ")}), resets ${r.resets} (ms ${r.resetMs.map(f1).join(" ")}), loads ${r.loads} (ms ${r.loadMs.map(f1).join(" ")})`);
  console.log(`  core options: ${r.options}; view set at: ${r.views}; link: ${JSON.stringify(r.link)}`);
  if (r.warns.length) console.log(`  worker warnings (frame: text): ${r.warns.map(([n, t]) => `${n}: ${t}`).join(" | ").slice(0, 1500)}`);
}
for (const h of report.handovers) {
  const d = (x, y) => (Number.isFinite(x) && Number.isFinite(y) ? `${(y - x).toFixed(0)} ms` : "-");
  console.log(`-- handover, epoch ${h.epoch}: state ${h.stateSize} bytes, deflated ${h.deflated} bytes (${MB(h.deflated)} MiB); room: A sent ${h.roomSent?.n} pieces ${h.roomSent?.bytes} bytes, B got ${h.roomRecv?.n} pieces ${h.roomRecv?.bytes} bytes --`);
  console.log(`  A capture asked ${rel(h.capture)}; serialize+deflate -> captured ${d(h.capture, h.captured)}; room first->last piece at B ${d(h.roomRecv?.first, h.roomRecv?.last)}; captured -> B's online msg ${d(h.captured, h.onlineB)} (round trip ${f1(h.roundTripB)} ms); B online -> loaded ${d(h.onlineB, h.loadB)} -> first frame ${d(h.loadB, h.firstRunB)}; A online ${d(h.captured, h.onlineA)} -> loaded ${d(h.onlineA, h.loadA)} -> first frame ${d(h.loadA, h.firstRunA)}`);
  console.log(`  capture asked -> both running: ${d(h.capture, Math.max(h.firstRunA ?? NaN, h.firstRunB ?? NaN))}`);
}
if (report.watch) {
  const w = report.watch, d = (x, y) => (Number.isFinite(x) && Number.isFinite(y) ? `${(y - x).toFixed(0)} ms` : "-");
  console.log(`-- watcher: F -> worker ready ${d(w.fPressed, w.cReady)}; snapshot asked ${w.snapshotAsked.map(rel).join(",")}; watch-states sent ${w.watchStatesSent.map((s) => `${rel(s.t)} ${s.bytes} bytes${s.to ? " to " + s.to : ""}`).join(", ")}; room to C: ${w.roomRecv?.n} pieces ${w.roomRecv?.bytes} bytes in ${d(w.roomRecv?.first, w.roomRecv?.last)}; C got ${w.watchStatesIn.map((s) => `${rel(s.t)} ${s.bytes} bytes`).join(", ")}; loaded ${rel(w.cLoad)}, first frame ${d(w.cLoad, w.cFirstRun)} later; F -> first frame ${d(w.fPressed, w.cFirstRun)} --`);
  console.log(`  watcher's view set at: ${w.views}`);
}
if (report.wire) console.log(`-- wire (page to page, same clock; a greedy match of sends to arrivals, so an offset of whole frames, ~17 or ~35 ms, is the matcher skipping a packet, not the wire) -- A->B ms: p50 ${f2(report.wire.aToB.p50)} p95 ${f2(report.wire.aToB.p95)} max ${f2(report.wire.aToB.max)}; B->A: p50 ${f2(report.wire.bToA.p50)} p95 ${f2(report.wire.bToA.p95)} max ${f2(report.wire.bToA.max)}`);
for (const r of Object.values(report.browsers)) delete r.raw;
writeFileSync(join(OUT, `${LABEL}.json`), JSON.stringify(report, null, 1));
browsers.forEach((b) => b.close());
process.exit(0);
