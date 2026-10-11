// Runs the Sega Rally harness page in a headless Chrome of our own (never the user's) over CDP
// and prints its results, saving a screenshot of the page and the frames it was asked for.
//   node model2/srally/harness/run.mjs [?rom=srallycb&race&warmup=2000&frames=600&shots=300,1500] [out-prefix]
// The shots land at <out-prefix>-<frame>.png, the page at <out-prefix>-page.png. Starts its own
// serve.mjs on a free port unless SERVER=<origin> is given. Node 22+ (built-in WebSocket).
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const query = process.argv[2] ?? "?rom=srallycb";
const prefix = process.argv[3] ?? "srally";
const TIMEOUT_MS = Number(process.env.TIMEOUT ?? 300) * 1000;
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9433 + Math.floor(Math.random() * 100);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let origin = process.env.SERVER;
let server;
if (!origin) {
  const port = 8800 + Math.floor(Math.random() * 100);
  server = spawn(process.execPath, [join(import.meta.dirname, "serve.mjs"), String(port)], { stdio: "ignore" });
  origin = `http://localhost:${port}`;
  for (let i = 0; i < 50; i++) { try { await fetch(origin); break; } catch { await sleep(100); } }
}
const profile = mkdtempSync(join(tmpdir(), "srally-chrome-"));
const chrome = spawn(CHROME, [`--headless=new`, `--remote-debugging-port=${PORT}`, `--user-data-dir=${profile}`, "--no-first-run",
  "--window-size=1100,1000", "--ignore-gpu-blocklist", "--enable-gpu", "about:blank"], { stdio: "ignore" });
const quit = () => { try { chrome.kill(); } catch {} try { server?.kill(); } catch {} };
process.on("exit", quit);

let version;
for (let i = 0; i < 50 && !version; i++) {
  try { version = await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json(); } catch { await sleep(200); }
}
if (!version) { console.error("Chrome did not come up"); process.exit(1); }
console.log(`${version.Browser}`);

const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let nextId = 1;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id) { const { resolve, reject } = pending.get(msg.id); pending.delete(msg.id); msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result); return; }
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
await send("Page.navigate", { url: `${origin}/model2/srally/harness/index.html${query}` }, sessionId);
const { result, exceptionDetails } = await send("Runtime.evaluate", {
  expression: `new Promise((r) => { setTimeout(() => r({ error: "timed out; the page shows: " + document.getElementById("out")?.textContent }), ${TIMEOUT_MS});
    const wait = () => window.bench ? window.bench.then(r, (e) => r({ error: String(e && e.stack || e) })) : setTimeout(wait, 100); wait(); })`,
  awaitPromise: true, returnByValue: true, timeout: TIMEOUT_MS + 10000,
}, sessionId);
if (exceptionDetails) console.error(exceptionDetails.text, exceptionDetails.exception?.description);
const value = result?.value ?? {};
for (const line of value.lines ?? []) console.log(line);
if (value.error) console.error("harness error:", value.error.split("\n").slice(0, 3).join("\n"));
for (const [frame, url] of Object.entries(value.shots ?? {})) {
  const file = `${prefix}-${String(frame).padStart(5, "0")}.png`;
  writeFileSync(file, Buffer.from(url.split(",")[1], "base64"));
  console.log(`frame ${frame}: ${file}`);
}
const { data } = await send("Page.captureScreenshot", { format: "png" }, sessionId);
writeFileSync(`${prefix}-page.png`, Buffer.from(data, "base64"));
console.log(`page: ${prefix}-page.png`);
ws.close();
quit();
process.exit(value.error ? 1 : 0);
