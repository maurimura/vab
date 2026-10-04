// Runs the harness page in a headless Chrome of our own (never the user's) over CDP and prints
// its results, saving a screenshot next to them.
//   node supermodel/harness/run.mjs [?rom=vs298&frames=600&warmup=600&ppc=50] [screenshot.png]
// Needs serve.mjs running on 8790. Node 22+ (built-in WebSocket).
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const query = process.argv[2] ?? "?rom=vs298";
const shot = process.argv[3] ?? "supermodel-harness.png";
const TIMEOUT_MS = Number(process.env.TIMEOUT ?? 300) * 1000;
const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9333 + Math.floor(Math.random() * 100);
const profile = mkdtempSync(join(tmpdir(), "supermodel-chrome-"));
const chrome = spawn(CHROME, [`--headless=new`, `--remote-debugging-port=${PORT}`, `--user-data-dir=${profile}`, "--no-first-run",
  "--window-size=1024,900", "--enable-unsafe-webgpu", "--ignore-gpu-blocklist", "about:blank"], { stdio: "ignore" });
const quit = () => { try { chrome.kill(); } catch {} };
process.on("exit", quit);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let version;
for (let i = 0; i < 50 && !version; i++) {
  try { version = await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json(); } catch { await sleep(200); }
}
if (!version) { console.error("Chrome did not come up"); process.exit(1); }

const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let nextId = 1;
const pending = new Map();
const events = [];
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id) { const { resolve, reject } = pending.get(msg.id); pending.delete(msg.id); msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result); return; }
  events.push(msg);
  // The page's console and errors as they happen, so a hang shows its cause.
  if (msg.method === "Runtime.consoleAPICalled") console.error(`page ${msg.params.type}:`, ...msg.params.args.map((a) => a.value ?? a.description ?? ""));
  if (msg.method === "Runtime.exceptionThrown") console.error("page error:", msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
};
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = nextId++;
  pending.set(id, { resolve, reject });
  ws.send(JSON.stringify({ id, method, params, sessionId }));
});

const { targetId } = await send("Target.createTarget", { url: "about:blank" });
const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
await send("Page.enable", {}, sessionId);
await send("Runtime.enable", {}, sessionId);
await send("Page.navigate", { url: `http://localhost:8790/supermodel/harness/index.html${query}` }, sessionId);
const { result, exceptionDetails } = await send("Runtime.evaluate", {
  expression: `new Promise((r) => { setTimeout(() => r({ error: "timed out; the page shows: " + document.getElementById("out")?.textContent }), ${TIMEOUT_MS});
    const wait = () => window.bench ? window.bench.then(r, (e) => r({ error: String(e && e.stack || e) })) : setTimeout(wait, 100); wait(); })`,
  awaitPromise: true, returnByValue: true, timeout: TIMEOUT_MS + 10000,
}, sessionId);
if (exceptionDetails) console.error(exceptionDetails.text, exceptionDetails.exception?.description);
const value = result?.value ?? {};
for (const line of value.lines ?? []) console.log(line);
if (value.error) console.error("harness error:", value.error.split("\n").slice(0, 3).join("\n"));
const { data } = await send("Page.captureScreenshot", { format: "png" }, sessionId);
writeFileSync(shot, Buffer.from(data, "base64"));
console.log(`screenshot: ${shot}`);
ws.close();
quit();
