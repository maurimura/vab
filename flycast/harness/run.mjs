// Runs the Flycast bench page (index.html) in a headless Chrome of our own, never the user's, over
// CDP: prints its results and saves its screenshots in flycast/.cache/checks/.
//   node flycast/harness/run.mjs ["?boot=3000&attract=600&coin=1500&match=1800&shots=300"]
// Needs serve.mjs running (PORT, default 8791). Node 22+ (built-in WebSocket). Chrome's profile
// is a temporary flycast-chrome-* directory; Chromes left over from an earlier run (same pattern)
// are killed first, since piled-up instances once made pages render black.
import { execSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const query = process.argv[2] ?? "";
const SERVER = Number(process.env.PORT ?? 8791);
const TIMEOUT_MS = Number(process.env.TIMEOUT ?? 900) * 1000;
const CHECKS = resolve(import.meta.dirname, "../.cache/checks");
const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const DEBUG_PORT = 9400 + Math.floor(Math.random() * 100);

try { execSync(`pkill -f "user-data-dir=.*flycast-chrome-"`); } catch {}
mkdirSync(CHECKS, { recursive: true });
const profile = mkdtempSync(join(tmpdir(), "flycast-chrome-"));
const chrome = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${DEBUG_PORT}`, `--user-data-dir=${profile}`, "--no-first-run",
  "--window-size=1100,1000", "--ignore-gpu-blocklist", "--autoplay-policy=no-user-gesture-required", "about:blank"], { stdio: "ignore" });
const quit = () => { try { chrome.kill(); } catch {} try { rmSync(profile, { recursive: true, force: true }); } catch {} };
process.on("exit", quit);
process.on("SIGINT", () => process.exit(130));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let version;
for (let i = 0; i < 50 && !version; i++) {
  try { version = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`)).json(); } catch { await sleep(200); }
}
if (!version) { console.error("Chrome did not come up"); process.exit(1); }

const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let nextId = 1;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id) { const { resolve, reject } = pending.get(msg.id); pending.delete(msg.id); msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result); return; }
  if (msg.method === "Runtime.exceptionThrown") console.error("page error:", msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
  if (msg.method === "Runtime.consoleAPICalled" && msg.params.type !== "log") console.error(`page ${msg.params.type}:`, ...msg.params.args.map((a) => a.value ?? a.description ?? ""));
  if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "log") console.log(...msg.params.args.map((a) => a.value ?? a.description ?? ""));
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
await send("Page.navigate", { url: `http://localhost:${SERVER}/flycast/harness/index.html${query}` }, sessionId);
await sleep(500);
const { result, exceptionDetails } = await send("Runtime.evaluate", {
  expression: `new Promise((r) => { setTimeout(() => r({ error: "timed out; the page shows: " + document.getElementById("out")?.textContent }), ${TIMEOUT_MS});
    const wait = () => window.bench ? window.bench.then(r, (e) => r({ error: String(e && e.stack || e), lines: [] })) : setTimeout(wait, 100); wait(); })`,
  awaitPromise: true, returnByValue: true, timeout: TIMEOUT_MS + 10000,
}, sessionId);
if (exceptionDetails) console.error(exceptionDetails.text, exceptionDetails.exception?.description);
const value = result?.value ?? {};
if (value.error) console.error("bench error:", value.error.split("\n").slice(0, 4).join("\n"));
for (const shot of value.shots ?? []) {
  writeFileSync(join(CHECKS, `${shot.name}.png`), Buffer.from(shot.png.split(",")[1], "base64"));
}
if (value.shots?.length) console.log(`${value.shots.length} screenshots in ${CHECKS}`);
const { data } = await send("Page.captureScreenshot", { format: "png" }, sessionId);
writeFileSync(join(CHECKS, "page.png"), Buffer.from(data, "base64"));
ws.close();
process.exit(value.error ? 1 : 0);
