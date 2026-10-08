// Arcade lab for Daytona USA: the bar's arcade mode end to end, through the real site. Headless
// Chromes of our own (never yours) open the bar, walk to the Daytona cabinet (the client's test
// hook vab.goTo("daytona")) and sit with E, each browser running only its own cabinet from its
// seat's state, the cabinets linked through the browsers (web/emulator/worker.js, web/index.html
// `Arcade`). The script, in the game's own terms (ring-notes.md, M2):
//
//   A, B, C sit (seats 0, 1, 2). A presses Start; B 2 s later: both in A's circuit select,
//   while C, idle, shows WAITING FOR YOUR ENTRY. A and B pick a course, then the accelerator
//   held: they race, POSITION n/2. C presses Start once the race is on: its own session. D sits
//   mid-race (seat 3). W watches (F): seat 0's cabinet, A's race, its machine checked against
//   A's (the game's RAM hashed every 60 frames on both). B stands up mid-race: A goes on.
//
// Measured over CDP with probes in the pages and the emulator workers: frames emulated and shown
// each second, retro_run's cost, every link packet's trip from one page to another (send and
// arrival times, same clock), blocks sent and taken in a second, how long each WebRTC link took
// to connect and over what (host, srflx, relay candidates), the core's link status, memory.
// Prints a report, writes <label>.json and screenshots: pages (<label>-<who>-<when>-page.png)
// and the last frame each worker posted (<label>-<who>-<when>.png, 496x384).
//
//   node daytona/harness/arcade-lab.mjs [--site=http://localhost:8787] [--delay=100 --jitter=0]
//        [--relay] [--seconds=60] [--voice=0] [--label=x] [--room=x] [--out=.] [--scratch=<dir>]
//   node daytona/harness/arcade-lab.mjs --mesh=8 [--seconds=30] ...
//   node daytona/harness/arcade-lab.mjs --strand ...
//
// --mesh=N: instead, N players sit one after another (seats 0 to N-1) and stay in the attract
// mode: every pair's link (N(N-1)/2), how long each took to connect, the frames and blocks.
// --strand: instead, a race left before it began: A, B and C sit, A presses Start, B 2 s later,
// both choose a course, and B stands up 8 s after A's Start, during the count. The game would
// have A wait for B for good; A's worker puts its cabinet back in the attract mode once the race
// should long have begun (37.5 s after its Start: worker.js, STRANDED_FRAMES), and A can start
// again. C, idle, is left alone. --strand=first: A, the first to press Start, leaves instead, and
// B is the one left waiting (its entrant count then reads 0, as in the attract mode).
// --delay/--jitter: ms added to every link packet a page sends (data channel or room), in order.
// --relay: no direct links: every link packet goes through the room (its Durable Object), as when
//   two browsers can't reach each other and there's no TURN.
// --voice=0: no fake microphones (by default each Chrome has one, so voice runs over the links).
//
// Needs the site running (make dev BUCKET=local, any PROFILE but wasm-release: test hooks) with
// the core, the ROM set and the seat states in local R2: make upload-daytona R2_TARGET=--local,
// make upload-rom ROM=$HOME/Downloads/daytona.zip, make upload-daytona-states R2_TARGET=--local.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deflateSync, crc32 } from "node:zlib";

const argv = Object.fromEntries(process.argv.slice(2).map((a) => { const [k, ...v] = a.replace(/^--/, "").split("="); return [k, v.length ? v.join("=") : "1"]; }));
const SITE = argv.site ?? "http://localhost:8787";
const DELAY = Number(argv.delay ?? 0);
const JITTER = Number(argv.jitter ?? 0);
const RELAY = argv.relay === "1";
const VOICE = argv.voice !== "0";
const MESH = Number(argv.mesh ?? 0);
const STRAND = argv.strand === "1" || argv.strand === "first";
const STRAND_FIRST = argv.strand === "first";
const SECONDS = Number(argv.seconds ?? (MESH ? 30 : 60));
const LABEL = argv.label ?? `arcade${MESH ? `-mesh${MESH}` : ""}${STRAND ? `-strand${STRAND_FIRST ? "-first" : ""}` : ""}${RELAY ? "-relay" : ""}${DELAY ? `-d${DELAY}` : ""}${JITTER ? `j${JITTER}` : ""}`;
const ROOM = argv.room ?? `arcade-${Date.now().toString(36)}`;
const OUT = argv.out ?? ".";
const SCRATCH = mkdtempSync(join(argv.scratch ?? tmpdir(), "arcade-lab-"));
const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The core's wasm exports are minified; the served glue maps them: _retro_run=Module["_retro_run"]=wasmExports["N"].
const glue = await (await fetch(`${SITE}/daytona/daytona.mjs`)).text();
const exportName = (fn) => glue.match(new RegExp(`_${fn}=Module\\["_${fn}"\\]=wasmExports\\["([\\w$]+)"\\]`))?.[1];
const EXPORTS = {
  run: exportName("retro_run"), load: exportName("retro_unserialize"), ram: exportName("retro_get_memory_data"), ramSize: exportName("retro_get_memory_size"),
  status: exportName("daytona_link_status"), linkIn: exportName("daytona_link_in"), linkOut: exportName("daytona_link_out"),
  memory: glue.match(/wasmMemory=wasmExports\["([\w$]+)"\]/)?.[1],
};
console.error("wasm export names:", EXPORTS);
if (Object.values(EXPORTS).some((name) => !name)) throw new Error(`could not map every export from ${SITE}/daytona/daytona.mjs (a core without the arcade bridge?)`);

// ---------------------------------------------------------------- probes injected in the page
// Link packets: [0xda][seat][frame u32 LE][block], straight over a data channel, or through the
// room as [to u32][0 (a packet)][packet] or [0xffffffff][n][n ids][0][packet] out, [from u32][0][packet] in.
const PAGE_PROBE = `(() => {
  document.cookie = "welcomed=1; path=/"; // the first visit's help card would eat the first key
  const L = window.__lab = { delay: ${DELAY}, jitter: ${JITTER}, sent: [], recv: [], pcs: [], links: [], rafGaps: [], framesIn: [], relayedOut: 0, directOut: 0 };
  const abs = () => performance.timeOrigin + performance.now();
  const OWorker = window.Worker;
  window.Worker = function (url, opts) {
    const w = new OWorker(url, opts);
    w.addEventListener("message", (e) => { if (e.data && e.data.type === "frame") L.framesIn.push(abs()); });
    return w;
  };
  window.Worker.prototype = OWorker.prototype;
  const tag = (u8, at) => (u8.length >= at + 6 + 448 && u8[at] === 0xda ? [u8[at + 1], (u8[at + 2] | (u8[at + 3] << 8) | (u8[at + 4] << 16) | (u8[at + 5] << 24)) >>> 0] : null);
  const lastSent = new Map();
  const noteSent = (t, path) => { if (!t) return; if (lastSent.get(t[0]) === t[1]) return; lastSent.set(t[0], t[1]); L.sent.push([t[0], t[1], abs(), path]); };
  // Sends held back by --delay, in order per channel or socket.
  const dues = new WeakMap();
  const later = (key, fn) => { const d = L.delay + (Math.random() * 2 - 1) * L.jitter; const due = Math.max(dues.get(key) ?? 0, performance.now() + Math.max(0, d)); dues.set(key, due); setTimeout(fn, due - performance.now()); };
  const dcSend = RTCDataChannel.prototype.send;
  RTCDataChannel.prototype.send = function (data) {
    const t = data instanceof ArrayBuffer ? tag(new Uint8Array(data), 0) : null;
    if (t) { noteSent(t, "dc"); L.directOut++; }
    if (!L.delay && !L.jitter) return dcSend.call(this, data);
    later(this, () => { try { dcSend.call(this, data); } catch {} });
  };
  ${RELAY ? `Object.defineProperty(RTCDataChannel.prototype, "readyState", { configurable: true, get() { return "connecting"; } });` : ""}
  const desc = Object.getOwnPropertyDescriptor(RTCDataChannel.prototype, "onmessage");
  Object.defineProperty(RTCDataChannel.prototype, "onmessage", { configurable: true, get() { return desc.get.call(this); },
    set(fn) { desc.set.call(this, (ev) => { if (ev.data instanceof ArrayBuffer) { const t = tag(new Uint8Array(ev.data), 0); if (t) L.recv.push([t[0], t[1], abs(), "dc"]); } return fn(ev); }); } });
  const wsSend = WebSocket.prototype.send;
  WebSocket.prototype.send = function (data) {
    let t = null;
    if (data instanceof Uint8Array && data.length > 6) {
      const several = data[0] === 0xff && data[1] === 0xff && data[2] === 0xff && data[3] === 0xff;
      const kindAt = several ? 5 + 4 * data[4] : 4;
      if (data[kindAt] === 0) t = tag(data, kindAt + 1);
    }
    if (t) { noteSent(t, "ws"); L.relayedOut++; }
    // Where the player stands: the room's move messages (a wasm-release build has no vab.state()).
    if (typeof data === "string" && data.startsWith('{"type":"move"')) { try { const m = JSON.parse(data); L.pos = { x: m.x, y: m.y, t: performance.now() }; } catch {} }
    if (!t || (!L.delay && !L.jitter)) return wsSend.call(this, data);
    later(this, () => { try { wsSend.call(this, data); } catch {} });
  };
  const wdesc = Object.getOwnPropertyDescriptor(WebSocket.prototype, "onmessage");
  Object.defineProperty(WebSocket.prototype, "onmessage", { configurable: true, get() { return wdesc.get.call(this); },
    set(fn) { wdesc.set.call(this, (ev) => {
      if (ev.data instanceof ArrayBuffer && ev.data.byteLength > 6) { const b = new Uint8Array(ev.data); if (b[4] === 0) { const t = tag(b, 5); if (t) L.recv.push([t[0], t[1], abs(), "ws"]); } }
      return fn(ev); }); } });
  // WebRTC links: when each was made, connected, and its data channel opened.
  const OPC = window.RTCPeerConnection;
  window.RTCPeerConnection = function (...a) {
    const pc = new OPC(...a);
    const rec = { created: abs(), connected: null, state: "new", dcOpen: null };
    L.pcs.push(pc); L.links.push(rec);
    pc.addEventListener("connectionstatechange", () => { rec.state = pc.connectionState; if (pc.connectionState === "connected" && !rec.connected) rec.connected = abs(); });
    const watchChannel = (ch) => ch.addEventListener("open", () => (rec.dcOpen ??= abs()));
    const ocdc = pc.createDataChannel.bind(pc);
    pc.createDataChannel = (...b) => { const ch = ocdc(...b); watchChannel(ch); return ch; };
    pc.addEventListener("datachannel", ({ channel }) => watchChannel(channel));
    return pc;
  };
  window.RTCPeerConnection.prototype = OPC.prototype;
  /** Each live link's chosen candidate pair: [local type, remote type, protocol, round trip ms]. */
  L.pairs = async () => Promise.all(L.pcs.filter((pc) => pc.connectionState !== "closed").map(async (pc) => {
    const stats = await pc.getStats(); let pair = null; const byId = new Map();
    stats.forEach((s) => { byId.set(s.id, s); if (s.type === "candidate-pair" && (s.selected || s.nominated) && s.state === "succeeded") pair = s; });
    if (!pair) return [pc.connectionState];
    const local = byId.get(pair.localCandidateId), remote = byId.get(pair.remoteCandidateId);
    return [local?.candidateType, remote?.candidateType, local?.protocol, pair.currentRoundTripTime !== undefined ? Math.round(pair.currentRoundTripTime * 1000) : null];
  }));
  let lastRaf = 0;
  const raf = (t) => { if (lastRaf) L.rafGaps.push(t - lastRaf); lastRaf = t; if (L.rafGaps.length > 20000) L.rafGaps.splice(0, 10000); requestAnimationFrame(raf); };
  requestAnimationFrame(raf);
})();`;

// ------------------------------------------------------- probes injected in the emulator worker
// Frames run (n: frames since the cabinet started, or for a watcher the stream's frame), the
// game's RAM hashed every 60 frames, frames posted, the worker's "arcade" stats.
const WORKER_PROBE = `(() => {
  const W = globalThis.__lab = { t0: performance.timeOrigin + performance.now(), runs: [], runsAt: [], framePosts: [], frameDups: 0, hashes: {}, base: 0, runsAtLoad: 0, pendingWatch: null, loads: [], stats: [], warns: [], msgs: [], grab: false, grabbed: null, gameLog: [], cancelled: [] };
  const abs = () => performance.timeOrigin + performance.now();
  const oWarn = console.warn;
  console.warn = function (...a) { W.warns.push([W.runs.length, a.map(String).join(" ")]); return oWarn.apply(this, a); };
  const od = Object.getOwnPropertyDescriptor(globalThis, "onmessage") || Object.getOwnPropertyDescriptor(Object.getPrototypeOf(globalThis), "onmessage");
  if (od && od.set) Object.defineProperty(globalThis, "onmessage", { configurable: true, get() { return od.get.call(globalThis); },
    set(fn) { od.set.call(globalThis, (ev) => {
      const m = ev.data;
      if (m && m.type === "watch-state" && m.bytes) W.pendingWatch = new DataView(m.bytes.buffer, m.bytes.byteOffset).getUint32(0, true);
      if (m && m.type && m.type !== "input" && m.type !== "watch-frames") W.msgs.push({ t: abs(), type: m.type, bytes: (m.bytes && m.bytes.length) || 0, seat: m.seat, seats: m.seats });
      return fn(ev); }); } });
  const btoaBytes = (u8) => { let s = ""; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000)); return btoa(s); };
  let lastHash = -1;
  const oPM = globalThis.postMessage;
  globalThis.postMessage = function (m, tr) {
    if (m && m.type === "frame") {
      const r = m.rgba; let h = 0; for (let i = 0; i < r.length; i += 1021) h = (Math.imul(h, 31) + r[i]) | 0;
      if (h === lastHash) W.frameDups++; lastHash = h;
      W.framePosts.push(abs());
      if (W.grab) { W.grab = false; W.grabbed = { width: m.width, height: m.height, rgba: btoaBytes(r), n: W.base + W.runs.length - W.runsAtLoad }; }
    } else if (m && m.type === "arcade") W.stats.push({ t: abs(), fps: m.fps, frame: m.frame, blocksIn: m.blocksIn, blocksOut: m.blocksOut });
    else if (m && m.type === "arcade-cancelled") W.cancelled.push([abs(), W.runs.length]);
    return oPM.call(this, m, tr);
  };
  const cString = (mem, ptr) => { const b = new Uint8Array(mem.buffer); let e = ptr; while (b[e]) e++; return new TextDecoder().decode(b.slice(ptr, e)); };
  const wrapped = new WeakMap();
  const wrapExports = (ex) => {
    if (!ex || typeof ex["${EXPORTS.run}"] !== "function" || typeof ex["${EXPORTS.status}"] !== "function") return ex;
    if (wrapped.has(ex)) return wrapped.get(ex);
    const mem = ex["${EXPORTS.memory}"];
    const out = {}; for (const k of Object.keys(ex)) out[k] = ex[k];
    const run = ex["${EXPORTS.run}"], load = ex["${EXPORTS.load}"];
    out["${EXPORTS.run}"] = function (...a) {
      const t0 = performance.now();
      try { return run.apply(this, a); } finally {
        W.runs.push(performance.now() - t0); W.runsAt.push(abs());
        const n = W.base + W.runs.length - W.runsAtLoad;
        // The game's word on it (see W.game), each change: [frame, cars, entrants, time].
        { const ram = new Uint8Array(mem.buffer, ex["${EXPORTS.ram}"](2)); const last = W.gameLog.at(-1);
          if (!last || last[1] !== ram[0x1080] || last[2] !== ram[0x40027] || last[3] !== ram[0x10a0]) W.gameLog.push([n, ram[0x1080], ram[0x40027], ram[0x10a0], abs()]); }
        if (n % 60 === 0) {
          const ptr = ex["${EXPORTS.ram}"](2), size = ex["${EXPORTS.ramSize}"](2);
          const words = new Uint32Array(mem.buffer, ptr, size >> 2); let h = 0x811c9dc5;
          for (let i = 0; i < words.length; i++) h = Math.imul(h ^ words[i], 0x01000193);
          W.hashes[n] = h >>> 0;
        }
      }
    };
    out["${EXPORTS.load}"] = function (...a) {
      if (W.pendingWatch !== null) { W.base = W.pendingWatch; W.runsAtLoad = W.runs.length; W.pendingWatch = null; W.hashes = {}; }
      W.loads.push([abs(), W.runs.length]);
      return load.apply(this, a);
    };
    W.linkStatus = () => { try { return JSON.parse(cString(mem, ex["${EXPORTS.status}"]())); } catch (e) { return String(e); } };
    // The game's own word (main RAM from 0x500000; arcade-check.mjs): cars on the track (10
    // outside a race, 16 in a linked race of two, 40 alone) and entrants in this cabinet's session.
    W.game = () => { const ram = new Uint8Array(mem.buffer, ex["${EXPORTS.ram}"](2)); return { cars: ram[0x1080], entrants: ram[0x40027] }; };
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
    c.#ws.onmessage = (m) => { const msg = JSON.parse(m.data); if (msg.id) { const p = c.#pending.get(msg.id); if (!p) return; c.#pending.delete(msg.id); msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result); } else for (const h of c.handlers) h(msg); }; return c; }
  send(method, params = {}, sessionId) {
    return new Promise((resolve, reject) => {
      const id = this.#next++;
      const timer = setTimeout(() => { this.#pending.delete(id); reject(new Error(`${method}: no answer in 60 s`)); }, 60000);
      this.#pending.set(id, { resolve: (v) => { clearTimeout(timer); resolve(v); }, reject: (e) => { clearTimeout(timer); reject(e); } });
      this.#ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  }
  close() { this.#ws.close(); }
}

const KEYS = { ArrowUp: 38, ArrowDown: 40, ArrowLeft: 37, ArrowRight: 39, Digit1: 49, Digit5: 53, KeyE: 69, KeyF: 70, Escape: 27 };
const keyOf = (code) => ({ Digit1: "1", Digit5: "5", KeyE: "e", KeyF: "f", Escape: "Escape" })[code] ?? code;

const T0 = Date.now();
const at = () => ((Date.now() - T0) / 1000).toFixed(1);

class Browser {
  chrome; cdp; page; workers = []; name;
  async launch(name) {
    this.name = name;
    const port = 9400 + Math.floor(Math.random() * 400);
    const profile = mkdtempSync(join(SCRATCH, `chrome-${name}-`));
    this.chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--window-size=1280,800",
      "--autoplay-policy=no-user-gesture-required", "--use-angle=metal", "--ignore-gpu-blocklist", "--disk-cache-size=1",
      ...(VOICE ? ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] : []), "about:blank"], { stdio: "ignore" });
    console.error(`[${name}] t=${at()}s Chrome pid ${this.chrome.pid}, port ${port}`);
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
          if (waitingForDebugger) await this.cdp.send("Runtime.runIfWaitingForDebugger", {}, sessionId);
          for (let i = 0; i < 50; i++) {
            const { result } = await this.cdp.send("Runtime.evaluate", { expression: "typeof setTimeout === 'function' && typeof postMessage === 'function'", returnByValue: true }, sessionId);
            if (result?.value) break;
            await sleep(2);
          }
          const probe = await this.cdp.send("Runtime.evaluate", { expression: WORKER_PROBE + " 'ok'", returnByValue: true }, sessionId);
          if (probe.exceptionDetails) console.error(`[${this.name}] WORKER PROBE THREW:`, probe.exceptionDetails.exception?.description ?? probe.exceptionDetails.text);
          this.workers.push(sessionId);
          console.error(`[${this.name}] t=${at()}s worker attached and probed`);
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
  /** The next frame the worker posts, as a PNG file; returns the frame number it was. */
  async frame(file) {
    await this.eval("globalThis.__lab.grab = true, globalThis.__lab.grabbed = null", this.worker);
    for (let i = 0; i < 50; i++) { if (await this.eval("!!globalThis.__lab.grabbed", this.worker)) break; await sleep(50); }
    const g = await this.eval("globalThis.__lab.grabbed", this.worker);
    if (!g) return console.error(`[${this.name}] no frame to save for ${file}`);
    writeFileSync(file, png(g.width, g.height, Buffer.from(g.rgba, "base64")));
    return g.n;
  }
  /** Both pictures, named `<label>-<name>-<when>`. */
  async shots(when) {
    const base = join(OUT, `${LABEL}-${this.name}-${when}`);
    const n = await this.frame(`${base}.png`).catch((e) => console.error(`[${this.name}] frame: ${e.message}`));
    await this.screenshot(`${base}-page.png`).catch((e) => console.error(`[${this.name}] screenshot: ${e.message}`));
    const status = await this.eval("window.vab.state().status ?? ''").catch(() => "");
    const game = await this.eval("globalThis.__lab.game()", this.worker).catch(() => ({}));
    console.error(`[${this.name}] t=${at()}s ${when}: frame ${n}, cars ${game.cars}, entrants ${game.entrants}, status "${status}"`);
    shotLog.push({ who: this.name, when, frame: n, ...game, status, file: `${base}.png` });
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
const shotLog = [];

/** An RGBA picture as a PNG file (no dependencies). */
function png(width, height, rgba) {
  const chunk = (type, data) => { const b = Buffer.alloc(12 + data.length); b.writeUInt32BE(data.length, 0); b.write(type, 4, "ascii"); data.copy(b, 8); b.writeUInt32BE(crc32(b.subarray(4, 8 + data.length)) >>> 0, 8 + data.length); return b; };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 6;
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) { raw[y * (width * 4 + 1)] = 0; rgba.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4); for (let x = 0; x < width; x++) raw[y * (width * 4 + 1) + 1 + x * 4 + 3] = 255; }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

const q = (arr, p) => { if (!arr.length) return NaN; const s = [...arr].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(s.length * p))]; };
const f1 = (x) => (Number.isFinite(x) ? x.toFixed(1) : "-");
const max = (arr) => (arr.length ? Math.max(...arr) : NaN);

const browsers = [];
// The Chromes, and their profiles (a few hundred MB each).
process.on("exit", () => {
  browsers.forEach((b) => b.close());
  try { execFileSync("sleep", ["1"]); rmSync(SCRATCH, { recursive: true, force: true }); } catch {}
});
process.on("SIGINT", () => process.exit(1));
process.on("SIGTERM", () => process.exit(1));

/** Opens the bar, goes to the Daytona cabinet and presses `key` there (E sits, F watches). */
async function goToCabinet(b, key = "KeyE") {
  await b.open(`${SITE}/?room=${ROOM}`);
  for (let i = 0; i < 300; i++) { if (await b.eval("typeof window.emulatorPlay === 'function' && window.__lab.rafGaps.length > 30").catch(() => false)) break; await sleep(200); }
  await sleep(1000);
  await b.click(640, 400);
  await sleep(200);
  if (await b.eval("!!window.vab").catch(() => false)) {
    await b.eval(`window.vab.goTo("daytona")`);
    for (let i = 0; i < 100; i++) { const s = await b.eval("window.vab.state()").catch(() => null); if (s?.go_to?.name === "daytona" && s.go_to.cell) break; await sleep(100); }
  } else {
    await walkTo(b, STAND);
  }
  await sleep(300);
  const pressedAt = Date.now();
  // Up to a minute: a page is slow to get there with several cabinets running on the machine.
  for (let tries = 0; tries < 20 && !b.workers.length; tries++) {
    await b.key(key);
    for (let i = 0; i < 30 && !b.workers.length; i++) await sleep(100);
  }
  if (!b.workers.length) throw new Error(`[${b.name}] no emulator worker appeared after pressing ${key}`);
  for (let i = 0; i < 300; i++) { if (await b.eval("globalThis.__lab && globalThis.__lab.runs.length > 10", b.worker).catch(() => false)) break; await sleep(200); }
  const first = await b.eval("globalThis.__lab.runsAt[0] - (performance.timeOrigin + performance.now() - Date.now())", b.worker).catch(() => NaN);
  console.error(`[${b.name}] t=${at()}s ${key === "KeyF" ? "watching" : "seated"}; first frame ${((first - pressedAt) / 1000).toFixed(2)} s after the key`);
  return pressedAt;
}

/** The cell to stand on for the Daytona cabinet at (-2,-3): next to it and to nothing else usable
 * (the start cell (-3,-2) and (-2,-2) are nearer the Virtua Striker cabinet at (-3,-3)). */
const STAND = [-1, -2];
/** The cell under world position (x, y): world::world_to_cell. */
const cellOf = ({ x, y }) => { const across = x / 16, down = -y / 8; return [Math.round((across + down) / 2), Math.round((down - across) / 2)]; };
/** Walks a wasm-release build's player to cell [tx, ty] with the arrow keys, correcting from where
 * the room hears them stand: 64 px/s along x and half that along y, so a cell diagonal (16, 8 px)
 * takes 0.354 s with two arrows held. */
async function walkTo(b, [tx, ty]) {
  const COMBO = { px: ["ArrowRight", "ArrowDown"], nx: ["ArrowLeft", "ArrowUp"], py: ["ArrowLeft", "ArrowDown"], ny: ["ArrowRight", "ArrowUp"] };
  const hold = async (keys, ms) => { for (const k of keys) await b.down(k); await sleep(ms); for (const k of keys) await b.up(k); };
  const where = () => b.eval("window.__lab.pos ? { x: window.__lab.pos.x, y: window.__lab.pos.y } : null").catch(() => null);
  // The first move is only sent once the room is open (up to half a minute on a cold Durable
  // Object): nudge until the room hears one.
  let pos = null;
  for (let i = 0; i < 60 && !(pos = await where()); i++) { await hold(COMBO.px, 40); await sleep(460); }
  if (!pos) throw new Error(`[${b.name}] the room never heard where the player stands`);
  for (let tries = 0; tries < 12; tries++) {
    pos = await where();
    const [cx, cy] = cellOf(pos);
    const dx = tx - cx, dy = ty - cy;
    if (!dx && !dy) { console.error(`[${b.name}] t=${at()}s standing on (${cx},${cy}) at (${pos.x.toFixed(0)}, ${pos.y.toFixed(0)})`); return; }
    if (dx) await hold(dx > 0 ? COMBO.px : COMBO.nx, Math.abs(dx) * 354 * 0.9);
    else await hold(dy > 0 ? COMBO.py : COMBO.ny, Math.abs(dy) * 354 * 0.9);
    await sleep(250); // the room hears the latest position within 100 ms
  }
  throw new Error(`[${b.name}] could not walk to (${tx},${ty})`);
}

const frames = (b) => b.eval("globalThis.__lab.runs.length", b.worker).catch(() => 0);
/** Steers now and then while driving: a press of Left or Right every two seconds or so. */
const driving = new Set();
async function drive(b) {
  driving.add(b);
  let side = 0;
  while (driving.has(b)) {
    await sleep(1600 + Math.random() * 800);
    if (!driving.has(b)) break;
    await b.key(side++ % 2 ? "ArrowLeft" : "ArrowRight", 150 + Math.random() * 200).catch(() => {});
    await b.down("ArrowUp").catch(() => {}); // still held
  }
}
/** Waits until `ms` after `since` (Date.now() ms). */
const until = (since, ms) => sleep(Math.max(0, since + ms - Date.now()));

/** One hop: each block from the page that sent it (its first send) to each page it reached, within [from, to). */
function hopsOf(data, from, to) {
  const sentAt = new Map();
  for (const [name, { p }] of Object.entries(data)) for (const [seat, frame, t] of p.sent) sentAt.set(`${seat}/${frame}`, { t, from: name });
  const all = [], byPair = {};
  for (const [name, { p }] of Object.entries(data)) {
    for (const [seat, frame, t, path] of p.recv) {
      const s = sentAt.get(`${seat}/${frame}`);
      if (!s || s.from === name || t < from || t >= to) continue;
      all.push(t - s.t);
      (byPair[`${s.from}->${name} ${path}`] ??= []).push(t - s.t);
    }
  }
  return { n: all.length, p50: q(all, 0.5), p95: q(all, 0.95), p99: q(all, 0.99), max: max(all),
    byPair: Object.fromEntries(Object.entries(byPair).map(([k, v]) => [k, { n: v.length, p50: +f1(q(v, 0.5)), p95: +f1(q(v, 0.95)), max: +f1(max(v)) }])) };
}

/** The page's probe, its times on Date.now()'s clock. */
async function pageOf(b) {
  const p = await b.eval("JSON.stringify(window.__lab)").then(JSON.parse);
  const pOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()");
  p.sent = p.sent.map(([s, f, t, path]) => [s, f, t - pOff, path]); p.recv = p.recv.map(([s, f, t, path]) => [s, f, t - pOff, path]);
  p.framesIn = p.framesIn.map((t) => t - pOff);
  p.links = p.links.map((l) => ({ ...l, created: l.created - pOff, connected: l.connected && l.connected - pOff, dcOpen: l.dcOpen && l.dcOpen - pOff }));
  p.pairs = await b.eval("window.__lab.pairs()").catch(() => []);
  return p;
}

// ------------------------------------------------------------------------------------ a mesh
if (MESH) {
  const names = "ABCDEFGH".slice(0, MESH).split("");
  const players = names.map((name) => { const b = new Browser(); b.name = name; browsers.push(b); return b; });
  await Promise.all(players.map((b) => b.launch(b.name)));
  const satAt = {};
  for (const b of players) satAt[b.name] = await goToCabinet(b);
  const allSat = Date.now();
  // Until every cabinet hears every other.
  let linkedAt;
  for (let i = 0; i < 300 && !linkedAt; i++) {
    const counts = await Promise.all(players.map((b) => b.eval("globalThis.__lab.linkStatus().star.seats.filter((s) => s.state === 'self' || s.state === 'present').length", b.worker).catch(() => 0)));
    if (counts.every((n) => n === MESH)) linkedAt = Date.now();
    else await sleep(200);
  }
  console.error(`t=${at()}s all ${MESH} seated; every cabinet hears the ${MESH - 1} others ${linkedAt ? `${((linkedAt - allSat) / 1000).toFixed(1)} s after the last sat` : "never"}`);
  await sleep(5000);
  const m0 = Date.now();
  const rss = new Map(players.map((b) => [b.name, []]));
  for (let s = 0; s < SECONDS; s += 5) { await until(m0, (s + 5) * 1000); for (const b of players) rss.get(b.name).push(b.rssMB()); }
  const m1 = Date.now();
  await Promise.all(players.map((b) => b.shots("mesh")));
  const data = {};
  const report = { label: LABEL, mesh: MESH, relay: RELAY, delay: DELAY, voice: VOICE, seconds: (m1 - m0) / 1000, linkedAfterLastSat: linkedAt && (linkedAt - allSat) / 1000, browsers: {}, shots: shotLog };
  for (const b of players) {
    const w = await workerOf(b), p = await pageOf(b);
    data[b.name] = { w, p };
    const win = (arr) => arr.filter((t) => t >= m0 && t < m1);
    const secs = (m1 - m0) / 1000;
    const perSecond = []; for (let t = m0; t + 1000 <= m1; t += 1000) perSecond.push(w.runsAt.filter((x) => x >= t && x < t + 1000).length);
    report.browsers[b.name] = { seat: w.msgs.find((m) => m.type === "start")?.seat, emulatedFps: win(w.runsAt).length / secs, perSecond, run: { p50: q(w.runs.slice(-500), 0.5), p95: q(w.runs.slice(-500), 0.95) },
      blocks: { sentPerSec: p.sent.filter(([, , t]) => t >= m0 && t < m1).length / secs, receivedPerSec: p.recv.filter(([, , t]) => t >= m0 && t < m1).length / secs, directOut: p.directOut, relayedOut: p.relayedOut },
      links: p.links.map((l) => ({ connectMs: l.connected ? Math.round(l.connected - l.created) : null, channelMs: l.dcOpen ? Math.round(l.dcOpen - l.created) : null, state: l.state })),
      pairs: p.pairs, link: w.link?.star?.seats?.map((x) => x.state[0]).join(""), wasmHeapMB: w.heap / 1048576, chromeRssMB: rss.get(b.name) };
  }
  report.hop = hopsOf(data, m0, m1);
  console.log(`\n== ${LABEL}: ${MESH} players at the cabinet, ${MESH * (MESH - 1) / 2} links; every cabinet heard all the others ${f1(report.linkedAfterLastSat)} s after the last sat; measured ${f1(report.seconds)} s ==`);
  for (const [name, r] of Object.entries(report.browsers)) {
    console.log(`-- ${name} (seat ${r.seat}): ${f1(r.emulatedFps)} fps (each second ${Math.min(...r.perSecond)}-${Math.max(...r.perSecond)}), retro_run p50 ${f1(r.run.p50)} p95 ${f1(r.run.p95)} ms; blocks sent ${f1(r.blocks.sentPerSec)}/s, received ${f1(r.blocks.receivedPerSec)}/s (direct sends ${r.blocks.directOut}, relayed ${r.blocks.relayedOut}); star ${r.link}`);
    console.log(`   links (connected/channel open ms after made): ${r.links.map((l) => `${l.connectMs ?? "-"}/${l.channelMs ?? "-"}`).join(" ")}; pairs ${JSON.stringify(r.pairs)}; heap ${f1(r.wasmHeapMB)} MB, RSS ${r.chromeRssMB.map((x) => x.toFixed(0)).join("/")} MB`);
  }
  console.log(`-- one hop, ${report.hop.n} arrivals: p50 ${f1(report.hop.p50)} p95 ${f1(report.hop.p95)} p99 ${f1(report.hop.p99)} max ${f1(report.hop.max)} ms`);
  writeFileSync(join(OUT, `${LABEL}.json`), JSON.stringify(report, null, 1));
  browsers.forEach((b) => b.close());
  process.exit(0);
}

// ------------------------------------------------------------------------- a race left waiting
if (STRAND) {
  const players = ["A", "B", "C"].map((name) => { const b = new Browser(); b.name = name; browsers.push(b); return b; });
  const [A, B, C] = players;
  // Who stands up during the count, and who is left waiting.
  const [leaver, stayer] = STRAND_FIRST ? [A, B] : [B, A];
  await Promise.all(players.map((b) => b.launch(b.name)));
  for (const b of players) await goToCabinet(b);
  await sleep(3000);
  const t0 = Date.now();
  const startFrame = await frames(A);
  console.error(`t=${at()}s A Start (frame ${startFrame})`);
  await A.key("Digit1", 200);
  await until(t0, 2000);
  await B.key("Digit1", 200);
  await until(t0, 5200);
  await A.key("ArrowUp", 180);
  await until(t0, 5400);
  await B.key("ArrowUp", 180);
  await until(t0, 7500);
  await Promise.all(players.map((b) => b.shots("entered")));
  const leaverWorker = await workerOf(leaver);
  await leaver.key("Escape");
  const leftAt = Date.now();
  console.error(`t=${at()}s ${leaver.name} stands up, ${((leftAt - t0) / 1000).toFixed(1)} s after A's Start`);
  await until(leftAt, 15000);
  await Promise.all([stayer, C].map((b) => b.shots("waiting")));
  // Until the stayer's cabinet goes back (a second load of its seat state), or 70 s.
  let backAt;
  for (let i = 0; i < 350 && !backAt; i++) {
    const loads = await stayer.eval("globalThis.__lab.loads.length", stayer.worker).catch(() => 0);
    if (loads > 1) backAt = Date.now();
    else await sleep(200);
  }
  console.error(`t=${at()}s ${stayer.name} ${backAt ? `back in the attract mode ${((backAt - leftAt) / 1000).toFixed(1)} s after ${leaver.name} left (${((backAt - t0) / 1000).toFixed(1)} s after A's Start)` : "never went back"}`);
  await sleep(2500);
  await Promise.all([stayer, C].map((b) => b.shots("back")));
  // The stayer starts again: a session of its own.
  await stayer.key("Digit1", 200);
  await sleep(3000);
  await Promise.all([stayer, C].map((b) => b.shots("again")));
  const w = { [stayer.name]: await workerOf(stayer), [leaver.name]: leaverWorker, C: await workerOf(C) };
  const report = { label: LABEL, leaver: leaver.name, stayer: stayer.name, startFrame, leftAfter: (leftAt - t0) / 1000, backAfterLeave: backAt && (backAt - leftAt) / 1000, backAfterStart: backAt && (backAt - t0) / 1000,
    loads: Object.fromEntries(Object.entries(w).map(([k, v]) => [k, v.loads.length])), cancelled: Object.fromEntries(Object.entries(w).map(([k, v]) => [k, v.cancelled.length])),
    gameLog: Object.fromEntries(Object.entries(w).map(([k, v]) => [k, v.gameLog.map(([n, cars, entrants, mode]) => `${n}:${cars}/${entrants}/${mode}`).join(" ")])),
    warns: Object.fromEntries(Object.entries(w).map(([k, v]) => [k, v.warns])), shots: shotLog };
  console.log(`\n== ${LABEL}: ${leaver.name} left ${f1(report.leftAfter)} s after A's Start; ${stayer.name} back in the attract mode ${report.backAfterLeave ? `${f1(report.backAfterLeave)} s after (${f1(report.backAfterStart)} s after A's Start)` : "never"} ==`);
  console.log(`  seat states loaded: ${JSON.stringify(report.loads)}; races called off: ${JSON.stringify(report.cancelled)}`);
  for (const [k, v] of Object.entries(report.gameLog)) console.log(`  ${k} frame:cars/entrants/mode ${v}`);
  for (const [k, v] of Object.entries(report.warns)) if (v.length) console.log(`  ${k} warnings: ${v.map(([n, t]) => `${n}: ${t}`).join(" | ")}`);
  console.log(`-- screenshots: ${shotLog.map((s) => `${s.who}-${s.when} (frame ${s.frame}, cars ${s.cars}, entrants ${s.entrants}) "${s.status}"`).join("\n   ")}`);
  writeFileSync(join(OUT, `${LABEL}.json`), JSON.stringify(report, null, 1));
  browsers.forEach((b) => b.close());
  process.exit(0);
}

// ------------------------------------------------------------------------------------ the run
const [A, B, C] = ["A", "B", "C"].map((name) => { const b = new Browser(); b.name = name; browsers.push(b); return b; });
await Promise.all([A, B, C].map((b) => b.launch(b.name)));
const seatedAt = {};
for (const b of [A, B, C]) seatedAt[b.name] = await goToCabinet(b); // one at a time: seats 0, 1, 2
console.error(`t=${at()}s three seated; waiting for the links`);
for (let i = 0; i < 100; i++) {
  const linked = await Promise.all([A, B, C].map((b) => b.eval("JSON.stringify(globalThis.__lab.linkStatus().star.seats.map((s) => s.state))", b.worker).then(JSON.parse).catch(() => [])));
  if (linked.every((seats) => seats.filter((s) => s === "self" || s === "present").length >= 3)) break;
  await sleep(200);
}
await sleep(3000);
await Promise.all([A, B, C].map((b) => b.shots("seated")));

// Start: A, then B 2 s later, into A's circuit select; C idle sees the entry prompt.
const t0 = Date.now();
const startFrame = await frames(A);
console.error(`t=${at()}s A Start (frame ${startFrame})`);
await A.key("Digit1", 200);
await until(t0, 2000);
console.error(`t=${at()}s B Start`);
await B.key("Digit1", 200);
await until(t0, 4500);
await Promise.all([A, B, C].map((b) => b.shots("entry")));
// A course (the accelerator picks the highlighted one, BEGINNER with the wheel centred), then as
// ring-lab's M2: Start at +15.6 s, the accelerator held from +20.9 s.
await until(t0, 5200);
await A.key("ArrowUp", 180);
await until(t0, 5400);
await B.key("ArrowUp", 180);
await until(t0, 15600);
await Promise.all([A.key("Digit1", 180), B.key("Digit1", 180)]);
await until(t0, 20900);
await Promise.all([A.down("ArrowUp"), B.down("ArrowUp")]);
console.error(`t=${at()}s A and B on the accelerator`);
// D gets ready meanwhile (sits mid-race below).
const D = new Browser(); browsers.push(D);
await D.launch("D");
await until(t0, 38000);
await Promise.all([A, B].map((b) => b.shots("race")));
drive(A); drive(B);
// C: Start once the race is on: its own session.
console.error(`t=${at()}s C Start, after the race began`);
// A press can land on an instant the game ignores (ring-notes.md, M2): a player presses again.
for (let tries = 0; tries < 3; tries++) {
  await C.key("Digit1", 200);
  await sleep(1200);
  if ((await C.eval("globalThis.__lab.game().entrants", C.worker).catch(() => 0)) > 0) break;
  console.error(`t=${at()}s C's Start didn't take: again`);
}
await until(t0, 41500);
await C.shots("own-session");
// D sits mid-race; W watches (seat 0's, A's cabinet).
seatedAt.D = await goToCabinet(D);
const W = new Browser(); browsers.push(W);
await W.launch("W");
const watchKey = await goToCabinet(W, "KeyF");
for (let i = 0; i < 200; i++) { if (await W.eval("globalThis.__lab.loads.length > 0 && globalThis.__lab.runs.length > 60", W.worker).catch(() => false)) break; await sleep(200); }
await sleep(2000);
await Promise.all([A, D, W].map((b) => b.shots("watch-start")));

/** What the probe in b's worker gathered, its times on Date.now()'s clock. */
async function workerOf(b) {
  const w = await b.eval("JSON.stringify(globalThis.__lab)", b.worker).then(JSON.parse);
  const wOff = await b.eval("performance.timeOrigin + performance.now() - Date.now()", b.worker);
  w.runsAt = w.runsAt.map((t) => t - wOff); w.framePosts = w.framePosts.map((t) => t - wOff); w.stats = w.stats.map((s) => ({ ...s, t: s.t - wOff }));
  w.link = await b.eval("JSON.stringify(globalThis.__lab.linkStatus())", b.worker).then(JSON.parse).catch((e) => e.message);
  w.heap = await b.eval("globalThis.__lab.heapBytes()", b.worker).catch(() => NaN);
  return w;
}
const workerData = {};

// The measured minute; B stands up 25 s into it.
const m0 = Date.now();
console.error(`t=${at()}s measuring ${SECONDS} s`);
const rss = new Map(browsers.map((b) => [b.name, []]));
let bLeftAt;
for (let s = 0; s < SECONDS; s += 5) {
  await until(m0, (s + 5) * 1000);
  for (const b of browsers) rss.get(b.name).push(b.rssMB());
  if (s + 5 === 25) {
    await Promise.all([A, B].map((b) => b.shots("before-leave")));
    driving.delete(B);
    workerData.B = await workerOf(B); // its worker goes when it stands up
    console.error(`t=${at()}s B stands up`);
    await B.key("Escape");
    bLeftAt = Date.now();
  }
  if (s + 5 === 35) await Promise.all([A, C, D, W].map((b) => b.shots("after-leave")));
}
const m1 = Date.now();
await Promise.all([A, C, D, W].map((b) => b.shots("end")));
driving.clear();

// ----------------------------------------------------------------------------- what happened
const report = { label: LABEL, delay: DELAY, jitter: JITTER, relay: RELAY, voice: VOICE, room: ROOM, seconds: (m1 - m0) / 1000, startFrame, bLeftAt: bLeftAt && (bLeftAt - T0) / 1000, browsers: {}, shots: shotLog };
const data = {};
for (const b of browsers) {
  const w = workerData[b.name] ?? (await workerOf(b));
  const p = await pageOf(b);
  const { link, heap } = w;
  const { pairs } = p;
  data[b.name] = { w, p };
  // The window it was at the cabinet for: the measured minute, B until it left.
  const end = b === B ? bLeftAt : m1;
  const win = (arr) => arr.filter((t) => t >= m0 && t < end);
  const perSecond = (arr) => { const out = []; for (let s = m0; s + 1000 <= end; s += 1000) out.push(arr.filter((t) => t >= s && t < s + 1000).length); return out; };
  const runs = w.runs.slice(-win(w.runsAt).length);
  const secs = (end - m0) / 1000;
  const sentWin = p.sent.filter(([, , t]) => t >= m0 && t < end);
  const recvWin = p.recv.filter(([, , t]) => t >= m0 && t < end);
  report.browsers[b.name] = {
    seat: w.msgs.find((m) => m.type === "start")?.seat, seconds: secs,
    emulatedFps: win(w.runsAt).length / secs, shownFps: win(w.framePosts).length / secs, pageFps: win(p.framesIn).length / secs,
    perSecond: perSecond(w.runsAt), run: { p50: q(runs, 0.5), p95: q(runs, 0.95), max: max(runs) }, frameDups: w.frameDups,
    blocks: { sentPerSec: sentWin.length / secs, receivedPerSec: recvWin.length / secs, directOut: p.directOut, relayedOut: p.relayedOut,
      receivedBy: Object.fromEntries(["dc", "ws"].map((path) => [path, recvWin.filter((r) => r[3] === path).length])) },
    workerStats: w.stats.filter((s) => s.t >= m0 && s.t < end).map((s) => `${s.fps}/${s.blocksIn}in/${s.blocksOut}out`).join(" "),
    links: p.links.map((l) => ({ connectMs: l.connected ? Math.round(l.connected - l.created) : null, channelMs: l.dcOpen ? Math.round(l.dcOpen - l.created) : null, state: l.state })),
    pairs, link, wasmHeapMB: heap / 1048576, chromeRssMB: rss.get(b.name), warns: w.warns.slice(0, 20), loads: w.loads,
    gameLog: w.gameLog.map(([n, cars, entrants, mode]) => `${n}:${cars}/${entrants}/${mode}`).join(" "),
    raf: { p50: q(p.rafGaps, 0.5), p95: q(p.rafGaps, 0.95) },
  };
}

report.hop = hopsOf(data, m0, m1);

// The watcher against seat 0's cabinet: the game's RAM at the same frames.
const ha = data.A.w.hashes, hw = data.W.w.hashes;
const common = Object.keys(hw).filter((n) => n in ha);
report.watch = { checked: common.length, same: common.filter((n) => ha[n] === hw[n]).length, first: common[0], last: common.at(-1),
  differ: common.filter((n) => ha[n] !== hw[n]).slice(0, 10), keyToFirstFrame: (data.W.w.runsAt.find((t) => t > watchKey) - watchKey) / 1000 };

// ------------------------------------------------------------------------------------ print
console.log(`\n== ${LABEL}: ${RELAY ? "every link packet through the room" : "links straight between the browsers (WebRTC)"}${DELAY ? `, ${DELAY}±${JITTER} ms added to each send` : ""}${VOICE ? ", fake microphones on" : ""}; measured ${f1(report.seconds)} s ==`);
for (const [name, r] of Object.entries(report.browsers)) {
  console.log(`-- ${name}${name !== "W" ? ` (seat ${r.seat})` : " (watching seat 0)"}, ${f1(r.seconds)} s --`);
  console.log(`  emulated ${f1(r.emulatedFps)} fps (each second ${Math.min(...r.perSecond)}-${Math.max(...r.perSecond)}), posted ${f1(r.shownFps)}, page got ${f1(r.pageFps)}; retro_run ms p50 ${f1(r.run.p50)} p95 ${f1(r.run.p95)} max ${f1(r.run.max)}; duplicate frames ${r.frameDups}`);
  console.log(`  blocks: sent ${f1(r.blocks.sentPerSec)}/s (direct sends ${r.blocks.directOut}, relayed ${r.blocks.relayedOut}), received ${f1(r.blocks.receivedPerSec)}/s (${JSON.stringify(r.blocks.receivedBy)}); worker each second fps/in/out: ${r.workerStats.slice(0, 200)}`);
  console.log(`  links: ${r.links.map((l) => `${l.connectMs ?? "-"}/${l.channelMs ?? "-"} ms ${l.state}`).join(", ")}; pairs ${JSON.stringify(r.pairs)}`);
  console.log(`  the game, frame:cars/entrants/mode at each change: ${r.gameLog.slice(0, 300)}`);
  console.log(`  core link: ${JSON.stringify(r.link?.star ?? r.link)}; wasm heap ${f1(r.wasmHeapMB)} MB, Chrome RSS ${r.chromeRssMB.map((x) => x.toFixed(0)).join("/")} MB`);
  if (r.warns.length) console.log(`  worker warnings: ${r.warns.map(([n, t]) => `${n}: ${t}`).join(" | ").slice(0, 800)}`);
}
console.log(`-- one hop (a block's send on one page to its arrival on another), ${report.hop.n} arrivals: p50 ${f1(report.hop.p50)} p95 ${f1(report.hop.p95)} p99 ${f1(report.hop.p99)} max ${f1(report.hop.max)} ms`);
for (const [k, v] of Object.entries(report.hop.byPair)) console.log(`   ${k}: n ${v.n}, p50 ${v.p50}, p95 ${v.p95}, max ${v.max}`);
console.log(`-- watcher: F to its first frame ${f1(report.watch.keyToFirstFrame)} s; RAM checked at ${report.watch.checked} frames (${report.watch.first}..${report.watch.last}), the same at ${report.watch.same}${report.watch.differ.length ? `, differ at ${report.watch.differ.join(",")}` : ""}`);
console.log(`-- screenshots: ${shotLog.map((s) => `${s.who}-${s.when} (frame ${s.frame}, cars ${s.cars}, entrants ${s.entrants}) "${s.status}"`).join("\n   ")}`);
writeFileSync(join(OUT, `${LABEL}.json`), JSON.stringify(report, null, 1));
browsers.forEach((b) => b.close());
process.exit(0);
