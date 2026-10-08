// Static server for the Flycast harness: the repo at /, the ROM sets at /roms/ from ROMS (default
// flycast/.cache/roms, where vtennisg.zip and vtennisg/gds-0011.chd live).
//   node flycast/harness/serve.mjs [port]      (8791 by default)
import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "../..");
const ROMS = process.env.ROMS ?? join(ROOT, "flycast/.cache/roms");
const PORT = Number(process.argv.slice(2).find((a) => /^\d+$/.test(a)) ?? 8791);
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".zip": "application/zip", ".json": "application/json" };

createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
  if (path.includes("..")) return res.writeHead(403).end();
  const file = path.startsWith("/roms/") ? join(ROMS, path.slice(6)) : join(ROOT, path === "/" ? "flycast/harness/index.html" : path);
  try {
    const size = statSync(file).size;
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream", "content-length": size, "cache-control": "no-store" });
    createReadStream(file).pipe(res);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(PORT, () => {
  console.log(`bench: http://localhost:${PORT}/flycast/harness/index.html`);
  console.log(`play:  http://localhost:${PORT}/flycast/harness/play.html`);
}).on("error", (e) => { console.error(e.code === "EADDRINUSE" ? `port ${PORT} is taken (another serve.mjs running?)` : e.message); process.exit(1); });
