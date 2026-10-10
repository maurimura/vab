// Exports a table for the game, in parts: node export.mjs saloon
//
// Into assets/pinball/<table>/:
// - background.png: the room, the cabinet behind the table, and the floor with every lamp off.
// - items/: everything standing on the table that doesn't move, each with its depth (how far
//   down the table it reaches), so the game can draw the ball in front of or behind it.
// - states/: the moving parts (flippers, bumpers, bottles, doors, the bull, the plunger), one
//   image per state.
// - lamps/: each lamp, lit, to draw over the floor when it's on.
// - front.png (the rails, lockdown bar and glass, over everything), backbox.png (with its
//   display dark) and its rank lamps, and the ball.
// - sprites.json: where each image goes (in 1920x1080 screen pixels), its depth and states.
// - layout.json: what the ball collides with, in table inches, and the camera (the pinball crate).
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { open } from "./chrome.mjs";

const table = process.argv[2] ?? "saloon";
const SCALE = Number(process.env.SCALE ?? 2);
const out = path.resolve("../../assets/pinball", table);
fs.rmSync(out, { recursive: true, force: true });
for (const dir of ["items", "states", "lamps", "ranks"]) fs.mkdirSync(path.join(out, dir), { recursive: true });

const { browser, page } = await open("file://" + path.resolve(table, "export.html"), { scale: SCALE });
const call = (fn, ...args) => page.evaluate((fn, args) => window.SCENE[fn](...args), fn, args);

/** Shoots the page as drawn, trims it to what's drawn, and saves it: where it goes on screen. */
async function save(file, { opaque = false } = {}) {
  const shot = PNG.sync.read(Buffer.from(await page.screenshot({ omitBackground: !opaque, encoding: "base64" }), "base64"));
  const { width, height, data } = shot;
  let x0 = width, y0 = height, x1 = -1, y1 = -1;
  if (opaque) [x0, y0, x1, y1] = [0, 0, width - 1, height - 1];
  else for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    if (data[(y * width + x) * 4 + 3] > 2) {
      if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
    }
  }
  if (x1 < 0) return null;
  const w = x1 - x0 + 1, h = y1 - y0 + 1;
  const cut = new PNG({ width: w, height: h });
  PNG.bitblt(shot, cut, x0, y0, w, h, 0, 0);
  fs.writeFileSync(path.join(out, file), PNG.sync.write(cut, { colorType: opaque ? 2 : 6 }));
  return { file, x: x0 / SCALE, y: y0 / SCALE, w: w / SCALE, h: h / SCALE };
}

const sprites = { size: [1920, 1080], scale: SCALE, items: [], states: {}, lamps: {}, ranks: [] };

await call("background");
sprites.background = await save("background.png", { opaque: true });
console.log("background");

const scene = await page.evaluate(() => ({ items: window.SCENE.items, dynamic: window.SCENE.dynamic }));
for (const it of scene.items.filter((it) => !it.flat)) {
  await call("item", it.i);
  const s = await save(`items/${String(sprites.items.length).padStart(3, "0")}-${it.name}.png`);
  if (s) sprites.items.push({ ...s, name: it.name, depth: it.key });
}
console.log(sprites.items.length, "items");

for (const [name, { key, states }] of Object.entries(scene.dynamic)) {
  const frames = [];
  for (let i = 0; i < states.length; i++) {
    await call("state", name, i);
    frames.push(await save(`states/${name}-${i}.png`));
  }
  sprites.states[name] = { depth: key, states, frames };
}
console.log(Object.keys(sprites.states).length, "moving parts");

const lamps = await call("lamps");
for (const name of lamps) {
  await call("lamp", name);
  sprites.lamps[name] = await save(`lamps/${name}.png`);
}
console.log(lamps.length, "lamps");

await call("front");
sprites.front = await save("front.png");
const { dmd } = await call("backbox", { dark: true });
sprites.backbox = { ...(await save("backbox.png")), dmd };
for (let i = 0; i < 5; i++) {
  await call("backbox", { onlyRank: i });
  sprites.ranks.push(await save(`ranks/rank-${i}.png`));
}
const ball = await call("ball");
sprites.ball = { ...(await save("ball.png")), ...ball };
console.log("front, backbox, ball");

fs.writeFileSync(path.join(out, "sprites.json"), JSON.stringify(sprites, null, 1));
fs.writeFileSync(path.join(out, "layout.json"), JSON.stringify(await call("physics"), null, 1));
await browser.close();
console.log("exported to", out);
