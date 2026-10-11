// Static server for the harness: the repo at /, ROM sets at /roms/ from ROMS (default
// ~/Downloads). node model2/srally/harness/serve.mjs [port] [--play <rom>]  (--play opens the
// play page for that ROM set in the default browser)
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";
import { homedir } from "node:os";

const ROOT = resolve(import.meta.dirname, "../../..");
const ROMS = process.env.ROMS ?? join(homedir(), "Downloads");
const args = process.argv.slice(2);
const play = args.includes("--play") ? args[args.indexOf("--play") + 1] ?? "srallycb" : null;
const PORT = Number(args.find((a) => /^\d+$/.test(a)) ?? 8792);
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".zip": "application/zip", ".json": "application/json" };

createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
  const file = path.startsWith("/roms/") ? join(ROMS, path.slice(6)) : join(ROOT, path === "/" ? "model2/srally/harness/index.html" : path);
  if (!file.startsWith(path.startsWith("/roms/") ? ROMS : ROOT)) return res.writeHead(403).end("no");
  try {
    const size = statSync(file).size;
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream", "content-length": size, "cache-control": "no-store" });
    createReadStream(file).pipe(res);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(PORT, () => {
  console.log(`play:  http://localhost:${PORT}/model2/srally/harness/play.html?rom=srallycb`);
  console.log(`bench: http://localhost:${PORT}/model2/srally/harness/index.html?rom=srallycb`);
  if (play) spawn("open", [`http://localhost:${PORT}/model2/srally/harness/play.html?rom=${play}`], { stdio: "ignore" });
}).on("error", (e) => { console.error(e.code === "EADDRINUSE" ? `port ${PORT} is taken (another serve.mjs running?)` : e.message); process.exit(1); });
