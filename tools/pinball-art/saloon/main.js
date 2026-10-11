import { el, txt, id, poly, smooth, arcPts, lerp, lerp2, band, offsetLine, ringHull, makeCamera, projPath, halftone, linGrad, radGrad, rng, f2 } from "./lib.js";
import { C, cel, star6, sheriff, outlaw, mechBull, cactus } from "./chars.js";
import { drawFloor, drawFloorLight, INSERTS } from "./floor.js";
import { drawBackbox } from "./backbox.js";
import { drawBackground } from "./bg.js";

// ---------------------------------------------------------------- setup
const W = 20.25, L = 42, K = 48;
const cam = makeCamera({ W, L, tilt: 39, dist: 84, fitTop: 112, fitBottom: 932, centerX: 520 });
const { proj, frame } = cam;
const $ = (s) => document.getElementById(s);
const bg = $("bg"), ov = $("ov"), topL = $("top"), head = $("head");
const floorDiv = $("floor");
floorDiv.style.width = K * W + "px";
floorDiv.style.height = K * L + "px";
floorDiv.style.transform = cam.floorMatrix(K);
const fs = el("svg", { width: K * W, height: K * L, viewBox: `0 0 ${W} ${L}` });
floorDiv.appendChild(fs);
const fdefs = el("defs", {}, fs);
const odefs = el("defs", {}, ov);
const INK = C.ink;

function rgb(h) { h = h.replace("#", ""); return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)); }
function hex(c) { return "#" + c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join(""); }
export function mix(a, b, t) { const A = rgb(a), B = rgb(b); return hex(A.map((v, i) => v + (B[i] - v) * t)); }
export function shade(c, k) { return k < 1 ? mix(c, "#2a1208", 1 - k) : mix(c, "#fff4d8", Math.min(1, k - 1)); }

// ---------------------------------------------------------------- geometry
const ARCH = { cx: 10.2, cy: 9.8, r: 9.8 };
const archPts = arcPts(ARCH.cx, ARCH.cy, ARCH.r, 180, 360, 64);
const orbitArc = arcPts(ARCH.cx, ARCH.cy, 8.15, 186, 243, 24);
const dirB = (() => { const v = [0.948, 0.319]; return v; })();
const nB = [-0.319, 0.948];
export const G = {
  W, L, ARCH, CX: 9.5,
  orbitWall: [[2.1, 25.6], [2.1, 10.6]].concat(orbitArc),
  guides: [9.7, 11.65, 13.6, 15.55],
  bumpers: [[10.75, 9.5], [14.55, 9.15], [12.6, 12.25]],
  ramp: [[6.4, 21.2, 0.0], [6.45, 19.7, 0.3], [6.05, 16.4, 1.25], [5.15, 13.0, 2.2], [5.25, 10.2, 2.85], [5.6, 8.9, 3.05]],
  posters: [[2.85, 20.15], [3.68, 19.67], [4.51, 19.19]],
  lane: { x0: 8.25, x1: 10.95, y0: 18.0, y1: 21.2 },
  bottles: [0, 1, 2, 3, 4].map((i) => [11.2 + i * 0.82 * dirB[0], 15.15 + i * 0.82 * dirB[1]]),
  piano: { x0: 16.95, x1: 18.55, y0: 12.3, y1: 15.9, h: 3.0 },
  scoop: [16.0, 13.75],
  bullLane: { x0: 16.55, y0: 18.6, y1: 22.3 },
  captive: [17.6, 21.0],
  // Round the flippers (the right side mirrors the left): each inlane's guide runs into the
  // flipper along the line of its top at rest, so a ball rolls from one onto the other, and the
  // slingshot sits far enough above that line for a ball to pass under it. Outlanes are 1.4
  // inches wide, inlanes 1.35: a ball is 1.06.
  slingL: [[3.74, 30.6], [3.74, 33.83], [6.14, 35.22]],
  slingR: [[15.26, 30.6], [15.26, 33.83], [12.86, 35.22]],
  flipL: [6.15, 37.6], flipR: [12.85, 37.6],
  sepL: [[1.91, 30.6], [1.91, 34.3], [2.35, 35.14], [5.96, 37.225]],
  sepR: [[17.09, 30.6], [17.09, 34.3], [16.65, 35.14], [13.04, 37.225]],
  star: [9.5, 27.4],
  nB,
};

drawBackground(bg, { C, mix, shade });
const FL = drawFloor(fs, fdefs, { C, G, mix, shade, archPts, W, L });

// ---------------------------------------------------------------- primitives
// In export mode (export.html), nothing is drawn on load: tools/pinball-art/export.mjs asks for
// each part on its own, so the game can stack them (static parts by depth, moving parts in each
// of their states), and for the physics layout.
const EXPORT = !!window.EXPORT;
const items = [];
// A static part, drawn in depth order by `key` (how far down the table it reaches). `flat` parts
// lie on the floor, under the ball whatever its depth: they go into the background.
const add = (key, fn, { name, flat = false } = {}) => items.push({ key, fn, name: name ?? `item${items.length}`, flat });
// Moving parts: drawn in each of their states, placed by the game.
const DYN = {};
const dyn = (name, key, states, fn) => { DYN[name] = { key, states, fn }; };
// What the ball collides with, in table inches: for the physics (pinball crate).
const PHYS = { walls: [], posts: [], rubbers: [], blocks: [] };
const SH = { circles: [], loops: [], blobs: [] };
const LIGHT = [-0.6, 0.8];
const SW = 2.3;

function loopArea(loop) { let a = 0; for (let i = 0; i < loop.length; i++) { const p = loop[i], q = loop[(i + 1) % loop.length]; a += p[0] * q[1] - q[0] * p[1]; } return a; }
function ellipseAt(x, y, z, r, N = 40) { const pts = []; for (let i = 0; i < N; i++) { const a = (i / N) * Math.PI * 2; pts.push(proj(x + r * Math.cos(a), y + r * Math.sin(a), z)); } return pts; }

// cel-shaded extrusion: faces get stepped tones, ink outline on silhouette edges and top
function extrude(g, loop, z0, z1, { side, top, sw = SW, planks = 0, topLine = true } = {}) {
  const A = loopArea(loop);
  const faces = [];
  for (let i = 0; i < loop.length; i++) {
    const p = loop[i], q = loop[(i + 1) % loop.length];
    const dx = q[0] - p[0], dy = q[1] - p[1], l = Math.hypot(dx, dy) || 1;
    const n = A > 0 ? [dy / l, -dx / l] : [-dy / l, dx / l];
    const m = [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
    if (n[0] * (cam.C[0] - m[0]) + n[1] * (cam.C[1] - m[1]) <= 0) continue;
    faces.push({ p, q, n, key: m[1] });
  }
  faces.sort((a, b) => a.key - b.key);
  for (const f of faces) {
    const d = f.n[0] * LIGHT[0] + f.n[1] * LIGHT[1];
    const fill = d > 0.55 ? shade(side, 1.06) : d > -0.1 ? side : shade(side, 0.72);
    const quad = [proj(f.p[0], f.p[1], z0), proj(f.q[0], f.q[1], z0), proj(f.q[0], f.q[1], z1), proj(f.p[0], f.p[1], z1)];
    el("path", { d: poly(quad), fill, stroke: fill, "stroke-width": 0.8, "stroke-linejoin": "round" }, g);
    if (planks) for (let k = 1; k < planks; k++) {
      const z = z0 + ((z1 - z0) * k) / planks;
      el("path", { d: poly([proj(f.p[0], f.p[1], z), proj(f.q[0], f.q[1], z)], false), stroke: shade(fill, 0.7), "stroke-width": 1, fill: "none" }, g);
    }
  }
  for (const f of faces) el("path", { d: poly([proj(f.p[0], f.p[1], z0), proj(f.q[0], f.q[1], z0)], false), stroke: INK, "stroke-width": sw, fill: "none", "stroke-linecap": "round" }, g);
  if (top) el("path", { d: projPath(cam, loop, z1), fill: top, stroke: INK, "stroke-width": sw, "stroke-linejoin": "round" }, g);
  // vertical silhouette edges
  return faces;
}

// cel cylinder: base, hard shadow band on the right, thin highlight on the left
function celCyl(g, x, y, r, z0, z1, { side, top, sw = SW, topFill } = {}) {
  const N = 40, bottom = [], upper = [];
  for (let i = 0; i <= N / 2; i++) { const a = (i / (N / 2)) * Math.PI; bottom.push(proj(x + r * Math.cos(a), y + r * Math.sin(a), z0)); }
  for (let i = N / 2; i >= 0; i--) { const a = (i / (N / 2)) * Math.PI; upper.push(proj(x + r * Math.cos(a), y + r * Math.sin(a), z1)); }
  const d = poly(bottom.concat(upper));
  const xl = proj(x - r, y, z0)[0], xr = proj(x + r, y, z0)[0];
  const ys = bottom.concat(upper).map((p) => p[1]);
  const y0 = Math.min(...ys) - 2, y1 = Math.max(...ys) + 2;
  el("path", { d, fill: side }, g);
  const cid = id("cy");
  el("path", { d }, el("clipPath", { id: cid }, odefs));
  const cg = el("g", { "clip-path": `url(#${cid})` }, g);
  el("rect", { x: lerp(xl, xr, 0.64), y: y0, width: (xr - xl) * 0.4, height: y1 - y0, fill: shade(side, 0.68) }, cg);
  el("rect", { x: lerp(xl, xr, 0.16), y: y0, width: (xr - xl) * 0.1, height: y1 - y0, fill: shade(side, 1.35), opacity: 0.9 }, cg);
  el("path", { d, fill: "none", stroke: INK, "stroke-width": sw, "stroke-linejoin": "round" }, g);
  if (top !== null) el("path", { d: poly(ellipseAt(x, y, z1, r)), fill: topFill || top || shade(side, 1.18), stroke: INK, "stroke-width": sw }, g);
}

const POSTS = new Map();
function post(g, x, y, { r = 0.21, h = 1.0, color = C.red } = {}) {
  if (!EXPORT || !PHYS.posts.some((p) => p.x === x && p.y === y)) PHYS.posts.push({ x, y, r });
  SH.circles.push([x, y, r + 0.1, h]);
  POSTS.set(`${x},${y}`, { r, h, color });
  celCyl(g, x, y, r, 0, h, { side: color, sw: 1.6 });
  celCyl(g, x, y, r * 0.55, h, h + 0.1, { side: C.gold, top: C.sunL, sw: 1.2 });
}
function postTop(g, x, y, { r, h, color }) {
  celCyl(g, x, y, r, 0.68, h, { side: color, sw: 1.6 });
  celCyl(g, x, y, r * 0.55, h, h + 0.1, { side: C.gold, top: C.sunL, sw: 1.2 });
}
function rubber(g, circles, { z = 0.5, t = 0.14, color = "#fbf1dc", dark = "#c8b58f", tops = true } = {}) {
  const hull = ringHull(circles.map(([x, y, r]) => [x, y, r + t / 2]));
  const key = circles.map(([x, y, r]) => `${x},${y},${r}`).join(";");
  if (!PHYS.rubbers.some((r) => r.key === key)) PHYS.rubbers.push({ key, circles: circles.map(([x, y, r]) => ({ x, y, r: r + t })) });
  SH.loops.push([ringHull(circles.map(([x, y, r]) => [x, y, r + t])), z + 0.1]);
  const s = cam.scale(circles[0][0], circles[0][1]);
  const lower = projPath(cam, hull, z - 0.14), upper = projPath(cam, hull, z + 0.1);
  for (const d of [lower, upper]) el("path", { d, fill: "none", stroke: INK, "stroke-width": t * s + 4, "stroke-linejoin": "round" }, g);
  el("path", { d: lower, fill: "none", stroke: dark, "stroke-width": t * s, "stroke-linejoin": "round" }, g);
  el("path", { d: projPath(cam, hull, z - 0.03), fill: "none", stroke: dark, "stroke-width": t * s, "stroke-linejoin": "round" }, g);
  el("path", { d: upper, fill: "none", stroke: color, "stroke-width": t * s * 0.95, "stroke-linejoin": "round" }, g);
  if (tops) for (const [x, y] of circles) if (POSTS.has(`${x},${y}`)) postTop(g, x, y, POSTS.get(`${x},${y}`));
}
function rubberPost(key, x, y, color = C.red) { add(key ?? y, (g) => { post(g, x, y, { color }); rubber(g, [[x, y, 0.21]]); }, { name: "post" }); }

// billboard toy drawn from a character function, standing at (x, y)
function toy(g, x, y, draw, { height, unit, flip = false, lean = 0, z = 0 }) {
  const base = proj(x, y, z);
  const sx = cam.scale(x, y, 0) * (height / unit);
  const zpx = base[1] - proj(x, y, 1)[1];
  const sy = Math.max(zpx * (height / unit) * 1.18, sx * 0.9);
  const gg = el("g", { transform: `translate(${f2(base[0])},${f2(base[1])}) scale(${f2(flip ? -sx : sx)},${f2(sy)}) skewX(${lean})` }, g);
  draw(gg);
  return gg;
}

// ---------------------------------------------------------------- walls
const WOOD = { side: C.brown, top: "#d99a5b", planks: 2 };
function wallItem(pts, w = 0.25, h = 1.1, style = WOOD) {
  PHYS.walls.push({ points: pts, width: w });
  for (let i = 0; i < pts.length - 1; i++) {
    const seg = [pts[i], pts[i + 1]];
    SH.loops.push([band(seg, w), h]);
    // Pieces at most 1.5 inches long, each at its own depth.
    const n = Math.max(1, Math.ceil(Math.hypot(seg[1][0] - seg[0][0], seg[1][1] - seg[0][1]) / 1.5));
    for (let k = 0; k < n; k++) {
      const a = lerp2(seg[0], seg[1], k / n), b = lerp2(seg[0], seg[1], (k + 1) / n);
      add(Math.max(a[1], b[1]) - 0.4, (g) => extrude(g, band([a, b], w), 0, h, style), { name: `wall` });
    }
    add(Math.max(seg[0][1], seg[1][1]) - 0.39, (g) => celCyl(g, pts[i + 1][0], pts[i + 1][1], w / 2, 0, h, { side: style.side, top: style.top, sw: 0 }), { name: "wall-end" });
  }
}
wallItem([[0.2, 9.8], [0.2, 44.2]], 0.4, 1.15);
wallItem([[20.1, 9.8], [20.1, 44.2]], 0.3, 1.15);
// The shooter lane: right of its wall, below the wall's top (where its gate is).
const SHOOTER = { x: 18.72, top: 10.1 };
PHYS.shooter = SHOOTER;
wallItem([[SHOOTER.x, SHOOTER.top], [SHOOTER.x, 44.2]], 0.25, 1.1);
wallItem(G.orbitWall, 0.25, 1.1);
wallItem([[G.lane.x0, G.lane.y0], [G.lane.x0, G.lane.y1]], 0.26, 1.1);
// From the ramp's side to the top of the doors' lane: a ball coming down there rolls on into
// the lane, not to a stop between the two.
wallItem([[7.15, 17.3], [G.lane.x0, G.lane.y0]], 0.24, 1.1);
wallItem([[G.lane.x1, G.lane.y0], [G.lane.x1, G.lane.y1]], 0.26, 1.1);
wallItem([[G.bullLane.x0, G.bullLane.y0], [G.bullLane.x0, G.bullLane.y1]], 0.25, 1.1);
add(22.3, (g) => {
  const x = G.bullLane.x0, xs = [x, 18.72];
  // (named below)
  for (const xx of xs) for (const yy of [18.7, 20.45, 22.2]) celCyl(g, xx, yy, 0.13, 1.0, 2.15, { side: C.brownL, top: "#d99a5b", sw: 1.6 });
  for (const xx of xs) for (const z of [1.45, 2.0]) {
    const a = proj(xx, 18.6, z), b = proj(xx, 22.3, z);
    el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 6.5, "stroke-linecap": "round" }, g);
    el("path", { d: poly([a, b], false), stroke: "#c98a4a", "stroke-width": 3.5, "stroke-linecap": "round" }, g);
  }
}, { name: "bull-pen" });
wallItem(G.sepL, 0.22, 1.0);
wallItem(G.sepR, 0.22, 1.0);
// Over each outlane, a short guide off the side wall: it turns some of the balls falling down
// the side (out of the orbit, off the bull) into the inlane, to the flipper, and lets the rest
// by. (Longer, and hardly a ball went down an outlane; none, and most did.)
wallItem([[0.3, 27.0], [1.05, 28.5]], 0.24, 1.1);
wallItem([[18.7, 27.0], [17.95, 28.5]], 0.24, 1.1);
wallItem([[5.96, 37.225], [6.6, 40.1]], 0.22, 0.9);
wallItem([[13.04, 37.225], [12.4, 40.1]], 0.22, 0.9);
// arch: steel band
add(-5, (g) => {
  const outer = arcPts(ARCH.cx, ARCH.cy, ARCH.r + 0.1, 180, 360, 64);
  extrude(g, outer.concat(archPts.slice().reverse()), 0, 1.45, { side: "#8d857b", top: "#cfc8bb" });
}, { name: "arch" });
PHYS.walls.push({ points: arcPts(ARCH.cx, ARCH.cy, ARCH.r + 0.1, 180, 360, 64), width: 0.2 });

// ---------------------------------------------------------------- rubber posts
for (const gx of G.guides) {
  add(5.6, (g) => { post(g, gx, 3.45, { color: C.red }); post(g, gx, 5.5, { color: C.red }); rubber(g, [[gx, 3.45, 0.21], [gx, 5.5, 0.21]]); }, { name: "lane-guide" });
}
rubberPost(25.7, 2.1, 25.6);
rubberPost(10.8, 2.1, 10.6, C.orange);
rubberPost(null, G.lane.x0, G.lane.y1 + 0.05);
rubberPost(null, G.lane.x1, G.lane.y1 + 0.05);
rubberPost(null, 16.55, 22.45 + 0.0);
rubberPost(null, 18.3, 22.6);
rubberPost(null, 16.4, 11.1, C.orange);

// ---------------------------------------------------------------- cactus slingshots
PHYS.slings = [];
function sling(pts, mirror) {
  const key = Math.max(...pts.map((p) => p[1]));
  PHYS.slings.push({ top: pts[0], bottom: pts[2], posts: pts, r: 0.21 + 0.16 });
  add(key, (g) => { for (const p of pts) post(g, p[0], p[1], { color: C.orange }); rubber(g, pts.map((p) => [p[0], p[1], 0.21]), { color: "#e9442f", dark: "#9c1f17", t: 0.16 }); }, { name: mirror ? "sling-right" : "sling-left" });
  // cactus plastic: triangle shield + saguaro arm sticking out
  const z = 1.45;
  const shield = ringHull(pts.map(([x, y]) => [x, y, 0.16]));
  add(key + 0.3, (g) => {
    extrude(g, shield, z - 0.12, z, { side: C.greenD, top: null, sw: 1.8 });
    // (the cactus plastic, named below)
    el("path", { d: projPath(cam, shield, z), fill: C.green, stroke: INK, "stroke-width": SW }, g);
    // painted ribs + spines on the plastic
    const c = pts.reduce((a, p) => [a[0] + p[0] / 3, a[1] + p[1] / 3], [0, 0]);
    const gg = el("g", { transform: frame([c[0], c[1], z]) }, g);
    const cid = id("sh");
    el("path", { d: poly(shield.map(([x, y]) => [x - c[0], y - c[1]])) }, el("clipPath", { id: cid }, odefs));
    const inner = el("g", { "clip-path": `url(#${cid})` }, gg);
    el("rect", { x: mirror ? -3 : -0.1, y: -4, width: 3.1, height: 8, fill: C.greenD, opacity: 0.55 }, inner);
    for (let i = -4; i < 5; i++) el("path", { d: `M${i * 0.42},-4 L${i * 0.42 + (mirror ? -1.2 : 1.2)},4`, stroke: C.greenD, "stroke-width": 0.06, fill: "none" }, inner);
    for (let i = 0; i < 18; i++) { const x = ((i * 37) % 23) / 10 - 1.1, y = ((i * 53) % 41) / 10 - 2.0; el("path", { d: `M${x},${y} l0.12,-0.1 M${x},${y} l-0.1,-0.12`, stroke: C.cream, "stroke-width": 0.035 }, inner); }
    el("path", { d: poly(shield.map(([x, y]) => [x - c[0], y - c[1]])), fill: "none", stroke: C.greenL, "stroke-width": 0.08, transform: "translate(-0.06,-0.06) scale(0.93)" }, inner);
  }, { name: mirror ? "cactus-plastic-right" : "cactus-plastic-left" });
  // saguaro toy standing behind the sling top post
  const top = pts[0];
  SH.blobs.push([top[0] + (mirror ? 0.35 : -0.35), top[1] - 0.2, 0.7, 0.35]);
  add(top[1] - 0.25, (g) => toy(g, top[0] + (mirror ? 0.35 : -0.35), top[1] - 0.25, (k) => cactus(k, { h: 3, sw: 0.11 }), { height: 3.1, unit: 3.1, flip: mirror }), { name: mirror ? "cactus-right" : "cactus-left" });
}
sling(G.slingL, false);
sling(G.slingR, true);

// ---------------------------------------------------------------- flippers
function flipper(g, px, py, ang, len = 2.75) {
  const a = (ang * Math.PI) / 180;
  const tx = px + len * Math.cos(a), ty = py + len * Math.sin(a);
  const loop = ringHull([[px, py, 0.34], [tx, ty, 0.2]]);
  SH.loops.push([loop, 0.85]);
  extrude(g, loop, 0.06, 0.85, { side: "#e9442f", top: null, sw: 2 });
  const inner = ringHull([[px, py, 0.27], [tx, ty, 0.13]]);
  el("path", { d: projPath(cam, loop, 0.85), fill: "#e9442f", stroke: INK, "stroke-width": SW }, g);
  el("path", { d: projPath(cam, inner, 0.87), fill: C.cream, stroke: INK, "stroke-width": 1.3 }, g);
  const hl = ringHull([[px, py, 0.12], [tx, ty, 0.05]]).map(([x, y]) => [x - 0.04, y - 0.06]);
  el("path", { d: projPath(cam, hl, 0.88), fill: "#fffaf0", opacity: 0.9 }, g);
  celCyl(g, px, py, 0.12, 0.87, 0.96, { side: C.gold, top: C.sunL, sw: 1.2 });
}
// Rest and up angles (degrees, y down the table): the left flipper rests pointing down and right.
export const FLIPPER = { length: 2.75, pivotR: 0.34, tipR: 0.2, rest: 30, up: -27, frames: 16 };
const flipAngles = (rest, up) => Array.from({ length: FLIPPER.frames }, (_, i) => rest + ((up - rest) * i) / (FLIPPER.frames - 1));
function flipperShadow(g, px, py, ang) {
  const a = (ang * Math.PI) / 180, len = FLIPPER.length;
  const loop = ringHull([[px, py, 0.34], [px + len * Math.cos(a), py + len * Math.sin(a), 0.2]]);
  const h = 0.85, dx = 0.12 + h * 0.12, dy = 0.1 + h * 0.15;
  el("path", { d: projPath(cam, loop.map(([x, y]) => [x + dx, y + dy]), 0), fill: "#3a1206", opacity: 0.38 }, g);
}
if (!EXPORT) {
  add(38.0, (g) => flipper(g, ...G.flipL, 30));
  add(38.0, (g) => flipper(g, ...G.flipR, 150));
}
dyn("flipper-left", 38.0, flipAngles(FLIPPER.rest, FLIPPER.up), (g, ang) => { flipperShadow(g, ...G.flipL, ang); flipper(g, ...G.flipL, ang); });
dyn("flipper-right", 38.0, flipAngles(180 - FLIPPER.rest, 180 - FLIPPER.up), (g, ang) => { flipperShadow(g, ...G.flipR, ang); flipper(g, ...G.flipR, ang); });

// ---------------------------------------------------------------- pop bumpers: whiskey barrels
function barrel(g, x, y, { lit = false } = {}) {
  SH.circles.push([x, y, 1.05, 1.7]);
  el("path", { d: poly(ellipseAt(x, y, 0.0, 1.4, 56)), fill: C.redD, stroke: INK, "stroke-width": SW }, g);
  el("path", { d: poly(ellipseAt(x, y, 0.14, 1.38, 56)), fill: C.red, stroke: INK, "stroke-width": SW }, g);
  // barrel body with staves and hoops
  const r0 = 0.8, z0 = 0.14, z1 = 1.55;
  const N = 40, prof = (z) => r0 + 0.17 * Math.sin(((z - z0) / (z1 - z0)) * Math.PI);
  const zs = [z0, z0 + 0.2, z0 + 0.45, (z0 + z1) / 2, z1 - 0.45, z1 - 0.2, z1];
  const right = zs.map((z) => proj(x + prof(z), y, z)), left = zs.map((z) => proj(x - prof(z), y, z)).reverse();
  const front = []; for (let i = 0; i <= N / 2; i++) { const a = (i / (N / 2)) * Math.PI; front.push(proj(x + r0 * Math.cos(a), y + r0 * Math.sin(a), z0)); }
  const topArc = []; for (let i = N / 2; i >= 0; i--) { const a = (i / (N / 2)) * Math.PI; topArc.push(proj(x + r0 * Math.cos(a), y + r0 * Math.sin(a), z1)); }
  const body = smooth(front.concat(right.slice(1, -1)).concat(topArc).concat(left.slice(1, -1)), true, 0.2);
  const bodyD = poly(front.concat(right.slice(1, -1).reverse().reverse()).concat(topArc).concat(left.slice(1, -1)));
  el("path", { d: bodyD, fill: lit ? "#d9894a" : C.brownL }, g);
  const cid = id("br");
  el("path", { d: bodyD }, el("clipPath", { id: cid }, odefs));
  const cg = el("g", { "clip-path": `url(#${cid})` }, g);
  const xl = proj(x - 1, y, z0)[0], xr = proj(x + 1, y, z0)[0];
  for (let i = -4; i <= 4; i++) { const xx = proj(x + i * 0.2, y + 0.8, z0)[0]; el("path", { d: `M${xx},${proj(x, y, z1)[1] - 40}V${proj(x, y + 1, 0)[1] + 10}`, stroke: shade(C.brownL, 0.7), "stroke-width": 1.2 }, cg); }
  el("rect", { x: lerp(xl, xr, 0.66), y: proj(x, y, z1 + 1)[1], width: (xr - xl) * 0.4, height: 200, fill: C.brownD, opacity: 0.55 }, cg);
  for (const zh of [z0 + 0.28, z1 - 0.3]) {
    const hp = []; for (let i = 0; i <= N / 2; i++) { const a = (i / (N / 2)) * Math.PI; hp.push(proj(x + prof(zh) * Math.cos(a), y + prof(zh) * Math.sin(a), zh)); }
    el("path", { d: poly(hp, false), stroke: INK, "stroke-width": 5.5, fill: "none" }, cg);
    el("path", { d: poly(hp, false), stroke: "#6d655c", "stroke-width": 3, fill: "none" }, cg);
  }
  el("path", { d: bodyD, fill: "none", stroke: INK, "stroke-width": SW, "stroke-linejoin": "round" }, g);
  // steel ring floating near the base
  el("path", { d: poly(ellipseAt(x, y, 0.55, 1.08)) + poly(ellipseAt(x, y, 0.55, 0.86).reverse()), fill: C.steel, "fill-rule": "evenodd", stroke: INK, "stroke-width": 1.4 }, g);
  // lid / cap
  celCyl(g, x, y, 0.92, z1, z1 + 0.16, { side: lit ? C.orange : C.orangeD, top: null });
  el("path", { d: poly(ellipseAt(x, y, z1 + 0.16, 0.92, 56)), fill: lit ? C.sun : C.orange, stroke: INK, "stroke-width": SW }, g);
  const gg = el("g", { transform: frame([x, y, z1 + 0.2]) }, g);
  el("circle", { cx: 0, cy: 0, r: 0.74, fill: lit ? "#e7b06a" : "#c98a4a", stroke: INK, "stroke-width": 0.06 }, gg);
  for (let i = -3; i <= 3; i++) el("path", { d: `M${i * 0.2},${-Math.sqrt(0.5476 - (i * 0.2) ** 2)}V${Math.sqrt(0.5476 - (i * 0.2) ** 2)}`, stroke: "#8f5228", "stroke-width": 0.035 }, gg);
  txt("XXX", { x: 0, y: 0.14, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.38, "text-anchor": "middle", fill: C.red, stroke: INK, "stroke-width": 0.02 }, gg);
  el("path", { d: "M-0.62,-0.3 A0.7,0.7 0 0,1 0.2,-0.68", stroke: "#fff", "stroke-width": 0.1, fill: "none", "stroke-linecap": "round", opacity: 0.85 }, gg);
  if (lit) el("path", { d: poly(ellipseAt(x, y, 0.8, 1.2)), fill: "#ffc861", opacity: 0.25, style: "mix-blend-mode:screen" }, g);
}
if (!EXPORT) G.bumpers.forEach(([x, y], i) => add(y, (g) => barrel(g, x, y, { lit: i !== 2 })));
G.bumpers.forEach(([x, y], i) => dyn(`bumper-${i}`, y, [false, true], (g, lit) => barrel(g, x, y, { lit })));
PHYS.bumpers = G.bumpers.map(([x, y]) => ({ x, y, r: 1.0 }));

// ---------------------------------------------------------------- the mine-cart ramp up into the gold mine
function rampPath(n = 48) {
  const pts = G.ramp;
  const out = [];
  for (let i = 0; i <= n; i++) {
    const t = (i / n) * (pts.length - 1);
    const k = Math.min(pts.length - 2, Math.floor(t)), u = t - k;
    const p0 = pts[Math.max(0, k - 1)], p1 = pts[k], p2 = pts[k + 1], p3 = pts[Math.min(pts.length - 1, k + 2)];
    const cr = (a, b, c, d) => 0.5 * (2 * b + (-a + c) * u + (2 * a - 5 * b + 4 * c - d) * u * u + (-a + 3 * b - 3 * c + d) * u * u * u);
    out.push([cr(p0[0], p1[0], p2[0], p3[0]), cr(p0[1], p1[1], p2[1], p3[1]), Math.max(0, cr(p0[2], p1[2], p2[2], p3[2]))]);
  }
  return out;
}
const RP = rampPath();
const RW = 0.95; // half width
function rampEdges(off) {
  return RP.map((p, i) => {
    const a = RP[Math.max(0, i - 1)], b = RP[Math.min(RP.length - 1, i + 1)];
    const dx = b[0] - a[0], dy = b[1] - a[1], l = Math.hypot(dx, dy);
    return [p[0] + (-dy / l) * off, p[1] + (dx / l) * off, p[2]];
  });
}
const RL = rampEdges(-RW), RR = rampEdges(RW);
// shadow of the ramp on the floor
SH.blobs.push(["ramp"]);
add(21.5, (g) => {
  // trestle supports
  for (const i of [12, 21, 30, 39, 46]) {
    const l = RL[i], r = RR[i], zz = RP[i][2] - 0.12;
    for (const p of [l, r]) {
      const a = proj(p[0], p[1], 0), b = proj(p[0], p[1], zz), s = cam.scale(p[0], p[1]);
      el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 0.32 * s + 3.5 }, g);
      el("path", { d: poly([a, b], false), stroke: C.brownL, "stroke-width": 0.32 * s }, g);
    }
    const x1 = proj(l[0], l[1], 0.15), x2 = proj(r[0], r[1], zz - 0.1), x3 = proj(r[0], r[1], 0.15), x4 = proj(l[0], l[1], zz - 0.1);
    for (const [a, b] of [[x1, x2], [x3, x4]]) { el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 5 }, g); el("path", { d: poly([a, b], false), stroke: C.brown, "stroke-width": 2.5 }, g); }
  }
  // left side board (inner face visible above the bed)
  const lw = RL.map((p) => proj(p[0], p[1], p[2] + 0.75)).concat(RL.slice().reverse().map((p) => proj(p[0], p[1], p[2] - 0.2)));
  el("path", { d: poly(lw), fill: C.brownD, stroke: INK, "stroke-width": SW }, g);
  // bed: dark ballast + sleepers
  const bed = RL.map((p) => proj(p[0], p[1], p[2])).concat(RR.slice().reverse().map((p) => proj(p[0], p[1], p[2])));
  el("path", { d: poly(bed), fill: "#4a2a17", stroke: INK, "stroke-width": SW }, g);
  for (let i = 1; i < RP.length - 1; i += 2) {
    const a = RL[i], b = RR[i], a2 = RL[i + 1], b2 = RR[i + 1];
    const q = [proj(a[0], a[1], a[2] + 0.05), proj(b[0], b[1], b[2] + 0.05), proj(lerp(b[0], b2[0], 0.6), lerp(b[1], b2[1], 0.6), lerp(b[2], b2[2], 0.6) + 0.05), proj(lerp(a[0], a2[0], 0.6), lerp(a[1], a2[1], 0.6), lerp(a[2], a2[2], 0.6) + 0.05)];
    el("path", { d: poly(q), fill: i % 4 === 1 ? "#a8693a" : "#9a5e32", stroke: INK, "stroke-width": 1.1 }, g);
  }
  // steel rails
  for (const off of [-0.55, 0.55]) {
    const rail = rampEdges(off).map((p) => proj(p[0], p[1], p[2] + 0.16));
    el("path", { d: poly(rail, false), fill: "none", stroke: INK, "stroke-width": 5.2, "stroke-linecap": "round" }, g);
    el("path", { d: poly(rail, false), fill: "none", stroke: C.steel, "stroke-width": 2.6, "stroke-linecap": "round" }, g);
    el("path", { d: poly(rail.map(([x, y]) => [x - 0.6, y - 0.6]), false), fill: "none", stroke: "#fffaf0", "stroke-width": 0.9 }, g);
  }
  // right side board: outer face visible
  const rw = RR.map((p) => proj(p[0], p[1], p[2] + 0.75)).concat(RR.slice().reverse().map((p) => proj(p[0], p[1], p[2] - 0.25)));
  el("path", { d: poly(rw), fill: C.brown, stroke: INK, "stroke-width": SW }, g);
  const mid = RR.map((p) => proj(p[0], p[1], p[2] + 0.25));
  el("path", { d: poly(mid, false), fill: "none", stroke: C.brownD, "stroke-width": 1.4 }, g);
  for (let i = 4; i < RR.length; i += 8) { const p = RR[i]; for (const z of [p[2] + 0.5, p[2] + 0.0]) { const c = proj(p[0], p[1], z); el("circle", { cx: c[0], cy: c[1], r: 1.6, fill: C.steelD, stroke: INK, "stroke-width": 0.6 }, g); } }
  const rtop = RR.map((p) => proj(p[0], p[1], p[2] + 0.75));
  el("path", { d: poly(rtop, false), fill: "none", stroke: "#d99a5b", "stroke-width": 3.2 }, g);
  el("path", { d: poly(rtop, false), fill: "none", stroke: INK, "stroke-width": 1 }, g);
  // entrance flap
  const fl = [proj(RL[0][0], RL[0][1] + 0.25, 0.02), proj(RR[0][0], RR[0][1] + 0.25, 0.02), proj(RR[2][0], RR[2][1], RR[2][2] + 0.03), proj(RL[2][0], RL[2][1], RL[2][2] + 0.03)];
  el("path", { d: poly(fl), fill: C.steel, stroke: INK, "stroke-width": 1.6 }, g);
}, { name: "ramp" });
PHYS.ramp = { path: RP, halfWidth: RW };
// the mine portal at the top of the ramp, with a cart full of gold
add(9.6, (g) => {
  const top = RP[RP.length - 1], z = top[2];
  const lx = RL[RL.length - 1][0] - 0.35, rx = RR[RR.length - 1][0] + 0.35, y = top[1] - 0.2;
  // rock mass around the portal: a heap of cel-shaded boulders
  const boulder = (bx, byy, r, zc, col = "#a86238") => {
    const c = proj(bx, byy, zc), sc = cam.scale(bx, byy, zc) * r;
    const pts = []; for (let i = 0; i < 9; i++) { const a = (i / 9) * Math.PI * 2; const rr = sc * (0.85 + 0.15 * Math.sin(i * 2.3 + bx)); pts.push([c[0] + rr * Math.cos(a), c[1] + rr * 0.8 * Math.sin(a)]); }
    const d = smooth(pts, true, 0.6);
    el("path", { d, fill: col }, g);
    const cid = id("bd"); el("path", { d }, el("clipPath", { id: cid }, odefs));
    const cg = el("g", { "clip-path": `url(#${cid})` }, g);
    el("ellipse", { cx: c[0] + sc * 0.55, cy: c[1] + sc * 0.45, rx: sc * 0.95, ry: sc * 0.8, fill: "#6e3b22" }, cg);
    el("ellipse", { cx: c[0] - sc * 0.35, cy: c[1] - sc * 0.4, rx: sc * 0.35, ry: sc * 0.2, fill: "#d08a54" }, cg);
    el("path", { d, fill: "none", stroke: INK, "stroke-width": SW }, g);
  };
  // (Their footprint is in PHYS.blocks, below.)
  boulder(lx - 0.7, y - 0.9, 1.15, z + 0.4);
  boulder(rx + 0.8, y - 1.1, 1.25, z + 0.5);
  boulder(lx + 0.5, y - 1.6, 1.2, z + 1.6, "#b06a3c");
  boulder(rx - 0.4, y - 1.8, 1.3, z + 1.9, "#a86238");
  boulder((lx + rx) / 2, y - 2.3, 1.35, z + 2.6, "#b87244");
  // tunnel mouth
  const mouth = [proj(lx + 0.2, y, z - 0.25), proj(rx - 0.2, y, z - 0.25), proj(rx - 0.2, y, z + 1.55), proj(lx + 0.2, y, z + 1.55)];
  el("path", { d: poly(mouth), fill: "#120905", stroke: INK, "stroke-width": SW }, g);
  // lantern glow inside
  const lc = proj((lx + rx) / 2, y - 0.4, z + 1.1);
  el("circle", { cx: lc[0], cy: lc[1], r: 16, fill: "#ffb84a", opacity: 0.35, style: "mix-blend-mode:screen" }, g);
  el("circle", { cx: lc[0], cy: lc[1], r: 3.5, fill: C.sunL, stroke: INK, "stroke-width": 1 }, g);
  // timber frame
  for (const x of [lx, rx]) {
    const q = [proj(x - 0.22, y, z - 0.3), proj(x + 0.22, y, z - 0.3), proj(x + 0.22, y, z + 1.9), proj(x - 0.22, y, z + 1.9)];
    el("path", { d: poly(q), fill: C.brownL, stroke: INK, "stroke-width": SW }, g);
    el("path", { d: poly([q[1], q[2]], false), stroke: C.brownD, "stroke-width": 3 }, g);
  }
  const beam = [proj(lx - 0.5, y + 0.05, z + 1.75), proj(rx + 0.5, y + 0.05, z + 1.75), proj(rx + 0.5, y + 0.05, z + 2.25), proj(lx - 0.5, y + 0.05, z + 2.25)];
  el("path", { d: poly(beam), fill: C.brownL, stroke: INK, "stroke-width": SW }, g);
  // sign
  const sg = el("g", { transform: frame([(lx + rx) / 2 - 1.2, y + 0.1, z + 2.95], [1, 0, 0], [0, 0, -1]) }, g);
  el("path", { d: "M0,0 L2.4,0.05 L2.38,0.62 L0.03,0.6 Z", fill: C.cream, stroke: INK, "stroke-width": 0.06 }, sg);
  txt("GOLD MINE", { x: 1.2, y: 0.44, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.36, "text-anchor": "middle", fill: C.redD }, sg);
  for (const x of [0.25, 2.15]) el("path", { d: `M${x},0.6 L${x},0.75`, stroke: INK, "stroke-width": 0.05 }, sg);
  // mine cart on the rails at the mouth
  const cx = top[0], cy = top[1] + 0.6;
  const box = [[cx - 0.75, cy - 0.45], [cx + 0.75, cy - 0.45], [cx + 0.75, cy + 0.45], [cx - 0.75, cy + 0.45]];
  for (const [wx, wy] of [[cx - 0.5, cy + 0.48], [cx + 0.5, cy + 0.48]]) { const p = proj(wx, wy, z + 0.3), s = cam.scale(wx, wy, z); el("ellipse", { cx: p[0], cy: p[1], rx: 0.22 * s, ry: 0.24 * s, fill: "#3a3330", stroke: INK, "stroke-width": 1.5 }, g); }
  extrude(g, box, z + 0.3, z + 1.25, { side: "#b5502a", top: null });
  const rim = box.map(([x, yy]) => [x + (x > cx ? 0.12 : -0.12), yy + (yy > cy ? 0.1 : -0.1)]);
  el("path", { d: projPath(cam, rim, z + 1.25), fill: "#7a2f18", stroke: INK, "stroke-width": SW }, g);
  // gold nuggets heap
  const R = rng(9);
  for (let i = 0; i < 14; i++) {
    const p = proj(cx - 0.6 + R() * 1.2, cy - 0.35 + R() * 0.7, z + 1.3 + R() * 0.35), s = cam.scale(cx, cy, z) * (0.13 + R() * 0.08);
    el("ellipse", { cx: p[0], cy: p[1], rx: s, ry: s * 0.8, fill: R() < 0.5 ? C.gold : C.sun, stroke: INK, "stroke-width": 1.1 }, g);
  }
  const sp = proj(cx + 0.1, cy - 0.1, z + 1.75);
  el("path", { d: `M${sp[0]},${sp[1] - 8} l2,6 l6,1 l-6,2 l-2,6 l-2,-6 l-6,-2 l6,-1z`, fill: "#fffbe0", stroke: INK, "stroke-width": 0.8 }, g);
}, { name: "mine" });
{
  const top = RP[RP.length - 1];
  const lx = RL[RL.length - 1][0] - 0.35, rx = RR[RR.length - 1][0] + 0.35, y = top[1] - 0.2;
  const rocks = [[lx - 0.7, y - 0.9, 1.15], [rx + 0.8, y - 1.1, 1.25], [lx + 0.5, y - 1.6, 1.2], [rx - 0.4, y - 1.8, 1.3], [(lx + rx) / 2, y - 2.3, 1.35]];
  // Round, for the physics: a ball can't come to rest on one.
  PHYS.rocks = rocks.map(([x, yy, r]) => ({ x, y: yy, r: r * 0.8 }));
  // The pile's upper boulders, up against the orbit's wall: they slope down to the right, so a
  // ball that lands on them rolls off out of the corner.
  const up = (deg, r = 7.95) => [ARCH.cx + r * Math.cos((deg * Math.PI) / 180), ARCH.cy + r * Math.sin((deg * Math.PI) / 180)];
  PHYS.blocks.push({ name: "mine", points: [up(203), up(219), up(235), [6.2, 4.5], [4.4, 7.2]] });
  // The portal itself, over the ramp's top.
  PHYS.blocks.push({ name: "mine", points: [[lx, y - 0.6], [rx, y - 0.6], [rx, y + 0.25], [lx, y + 0.25]] });
  PHYS.mineExit = [rx + 1.7, y + 1.1];
}

// ---------------------------------------------------------------- saloon doors under a sign arch (two-way gate)
add(18.1, (g) => {
  const { x0, x1, y0 } = G.lane;
  const y = y0 - 0.05;
  // door frame posts and header with the SALOON sign
  for (const x of [x0, x1]) celCyl(g, x, y, 0.2, 0, 3.0, { side: C.brownL, top: "#d99a5b" });
  const hdr = [[x0 - 0.5, y - 0.22], [x1 + 0.5, y - 0.22], [x1 + 0.5, y + 0.22], [x0 - 0.5, y + 0.22]];
  extrude(g, hdr, 2.8, 3.15, { side: C.brownD, top: C.brown });
  const sg = el("g", { transform: frame([x0 - 0.65, y + 0.23, 4.55], [1, 0, 0], [0, 0, -1]) }, g);
  const sw_ = x1 - x0 + 1.3;
  el("path", { d: `M0,0.15 Q${sw_ / 2},-0.25 ${sw_},0.15 L${sw_},1.35 L0,1.35 Z`, fill: C.cream, stroke: INK, "stroke-width": 0.07 }, sg);
  el("path", { d: `M0.12,0.3 Q${sw_ / 2},-0.08 ${sw_ - 0.12},0.3 L${sw_ - 0.12},1.23 L0.12,1.23 Z`, fill: "none", stroke: C.red, "stroke-width": 0.05 }, sg);
  txt("SALOON", { x: sw_ / 2 + 0.04, y: 1.08, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.78, "text-anchor": "middle", fill: INK, textLength: sw_ - 0.5, lengthAdjust: "spacingAndGlyphs" }, sg);
  txt("SALOON", { x: sw_ / 2, y: 1.03, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.78, "text-anchor": "middle", fill: C.red, textLength: sw_ - 0.5, lengthAdjust: "spacingAndGlyphs" }, sg);
  for (const x of [0.35, sw_ - 0.35]) el("path", { d: `M${x},1.35 L${x},1.45`, stroke: INK, "stroke-width": 0.06 }, sg);
  if (!EXPORT) doors(g, 22);
}, { name: "doors-frame" });
// Swung by `angle` degrees: up the table for a positive angle (a ball going in), toward the
// player for a negative one (a ball coming out).
export const DOORS = { angles: [-60, -45, -30, -18, -8, 0, 8, 18, 30, 45, 60] };
dyn("doors", 18.1, DOORS.angles, (g, angle) => doors(g, angle));
PHYS.doors = { x0: G.lane.x0, x1: G.lane.x1, y: G.lane.y0 - 0.05 };
function doors(g, angle) {
  const { x0, x1, y0 } = G.lane;
  const y = y0 - 0.05;
  // batwing doors
  for (const [hx, dir] of [[x0 + 0.15, 1], [x1 - 0.15, -1]]) {
    const ang = (angle * Math.PI) / 180, w = (x1 - x0) / 2 - 0.2;
    const ex = hx + dir * w * Math.cos(ang), ey = y - w * Math.sin(ang);
    const zb = 0.45, ztH = 2.05, ztE = 1.55;
    const pts = [proj(hx, y, zb), proj(ex, ey, zb)];
    const topC = []; for (let i = 0; i <= 10; i++) { const t = i / 10; topC.push(proj(lerp(ex, hx, t), lerp(ey, y, t), lerp(ztE, ztH, t) + Math.sin(t * Math.PI) * -0.15)); }
    const d = poly(pts.concat(topC));
    el("path", { d, fill: "#c97a3a", stroke: INK, "stroke-width": SW }, g);
    const cid = id("dr");
    el("path", { d }, el("clipPath", { id: cid }, odefs));
    const cg = el("g", { "clip-path": `url(#${cid})` }, g);
    for (let k = 1; k < 8; k++) { const z = zb + k * 0.2; el("path", { d: poly([proj(hx, y, z), proj(ex, ey, z)], false), stroke: "#8a4a20", "stroke-width": 1.6 }, cg); }
    el("path", { d: poly([proj(hx, y, zb), proj(hx + dir * 0.18 * Math.cos(ang), y - 0.18 * Math.sin(ang), zb), proj(hx + dir * 0.18 * Math.cos(ang), y - 0.18 * Math.sin(ang), 2.2), proj(hx, y, 2.2)]), fill: "#a05a28" }, cg);
    el("path", { d: poly([proj(hx, y, 0.6), proj(hx, y, 1.95)], false), stroke: C.gold, "stroke-width": 3 }, g);
  }
}

// ---------------------------------------------------------------- bar counter with bottle drop targets
{
  const pts = G.bottles;
  const back = pts.map(([x, y]) => [x - nB[0] * 0.55, y - nB[1] * 0.55]);
  const ext = [[back[0][0] - dirB[0] * 0.75, back[0][1] - dirB[1] * 0.75], [back[4][0] + dirB[0] * 0.75, back[4][1] + dirB[1] * 0.75]];
  const counter = band(ext, 0.85);
  SH.loops.push([counter, 1.3]);
  PHYS.blocks.push({ name: "bar", points: counter });
  add(15.0, (g) => {
    extrude(g, counter, 0, 1.25, { side: "#7a3d1e", top: null, planks: 0 });
    const topLoop = band([[ext[0][0] - dirB[0] * 0.1, ext[0][1] - dirB[1] * 0.1], [ext[1][0] + dirB[0] * 0.1, ext[1][1] + dirB[1] * 0.1]], 1.05);
    extrude(g, topLoop, 1.25, 1.42, { side: "#a0522d", top: "#c46a37" });
    // polished highlight on the bar top
    const hl = band([[ext[0][0] + 0.2, ext[0][1]], [ext[1][0] - 0.4, ext[1][1] - 0.1]], 0.12).map(([x, y]) => [x - nB[0] * 0.18, y - nB[1] * 0.18]);
    el("path", { d: projPath(cam, hl, 1.43), fill: "#f4b27a", opacity: 0.9 }, g);
    // a mug of beer and a whiskey glass on the bar
    const m = [ext[1][0] - 0.25, ext[1][1] - 0.35];
    celCyl(g, m[0], m[1], 0.22, 1.42, 1.95, { side: "#f2c14e", top: "#fff6dc" });
  }, { name: "bar" });
  // Bottles: drop targets, standing and knocked down.
  PHYS.dropTargets = pts.map(([x, y]) => ({ x, y, along: dirB, facing: nB, halfWidth: 0.38 }));
  const bottle = (g, x, y, i, down) => {
    const t = dirB;
    const zb = 0.25;
    const glass = [["#c97a1e", "#8e4f10"], ["#5f8f3a", "#3d6623"], ["#c97a1e", "#8e4f10"], ["#5f8f3a", "#3d6623"], ["#b23a2a", "#7a2018"]][i];
    if (down) {
      const q = [proj(x - t[0] * 0.38, y - t[1] * 0.38, 0.06), proj(x + t[0] * 0.38, y + t[1] * 0.38, 0.06), proj(x + t[0] * 0.38 - nB[0] * 0.18, y + t[1] * 0.38 - nB[1] * 0.18, 0.06), proj(x - t[0] * 0.38 - nB[0] * 0.18, y - t[1] * 0.38 - nB[1] * 0.18, 0.06)];
      el("path", { d: poly(q), fill: glass[1], stroke: INK, "stroke-width": 1.5 }, g);
      return;
    }
    const gg = el("g", { transform: frame([x, y, zb], [t[0], t[1], 0], [0, 0, -1]) }, g);
    const d = "M-0.36,0 L0.36,0 L0.36,-0.92 C0.36,-1.1 0.14,-1.12 0.12,-1.3 L0.12,-1.62 L0.15,-1.66 L0.15,-1.76 L-0.15,-1.76 L-0.15,-1.66 L-0.12,-1.62 L-0.12,-1.3 C-0.14,-1.12 -0.36,-1.1 -0.36,-0.92 Z";
    cel(gg, d, glass[0], "M0.12,-2 L0.5,-2 L0.5,0 L0.12,0Z", glass[1], { sw: 0.07, hi: "M-0.26,-0.15 L-0.17,-0.15 L-0.17,-0.95 L-0.26,-0.92Z", hiOp: 0.75 });
    el("rect", { x: -0.28, y: -0.78, width: 0.56, height: 0.42, fill: C.cream, stroke: INK, "stroke-width": 0.04 }, gg);
    txt("XXX", { x: 0, y: -0.49, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.2, "text-anchor": "middle", fill: C.red }, gg);
    el("rect", { x: -0.15, y: -1.78, width: 0.3, height: 0.1, fill: C.red, stroke: INK, "stroke-width": 0.03 }, gg);
  };
  if (!EXPORT) pts.forEach(([x, y], i) => add(y + 0.05, (g) => bottle(g, x, y, i, i === 3)));
  pts.forEach(([x, y], i) => dyn(`bottle-${i}`, y + 0.05, [false, true], (g, down) => bottle(g, x, y, i, down)));
}

// ---------------------------------------------------------------- wanted poster standups
const nP = (() => { const v = [0.5, 0.866]; return v; })();
PHYS.standups = G.posters.map(([x, y]) => ({ x, y, along: [nP[1], -nP[0]], facing: nP, halfWidth: 0.56 }));
G.posters.forEach(([x, y], i) => add(y, (g) => {
  const t = [-nP[1], nP[0]].map((v) => -v); // tangent pointing right-up
  const w = 0.56, z0 = 0.22, z1 = 1.62;
  const c = (s, k, z) => proj(x + t[0] * s - nP[0] * k, y + t[1] * s - nP[1] * k, z);
  el("path", { d: poly([c(-0.1, 0.06, 0), c(0.1, 0.06, 0), c(0.1, 0.06, z0), c(-0.1, 0.06, z0)]), fill: C.steelD, stroke: INK, "stroke-width": 1.2 }, g);
  el("path", { d: poly([c(-w, 0, z1), c(w, 0, z1), c(w, 0.16, z1), c(-w, 0.16, z1)]), fill: "#d6b47c", stroke: INK, "stroke-width": 1.4 }, g);
  const gg = el("g", { transform: frame([x - t[0] * w, y - t[1] * w, z1], [t[0], t[1], 0], [0, 0, -1]) }, g);
  const pw = 2 * w, ph = z1 - z0;
  el("path", { d: `M0,0 L${pw},0 L${pw},${ph} L0.05,${ph} Z`, fill: "#f3dcae", stroke: INK, "stroke-width": 0.07 }, gg);
  el("path", { d: `M${pw - 0.25},${ph} L${pw},${ph - 0.22} L${pw},${ph} Z`, fill: "#d6b47c", stroke: INK, "stroke-width": 0.04 }, gg);
  txt("WANTED", { x: pw / 2, y: 0.26, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.24, "text-anchor": "middle", fill: INK, textLength: pw - 0.16, lengthAdjust: "spacingAndGlyphs" }, gg);
  // mugshot
  const face = ["#efb27c", "#e3a06c", "#d89464"][i];
  el("rect", { x: 0.18, y: 0.34, width: pw - 0.36, height: 0.68, fill: "#e8cb93", stroke: INK, "stroke-width": 0.035 }, gg);
  el("ellipse", { cx: pw / 2, cy: 0.74, rx: 0.2, ry: 0.24, fill: face, stroke: INK, "stroke-width": 0.035 }, gg);
  el("path", { d: `M${pw / 2 - 0.36},0.6 L${pw / 2 + 0.36},0.6 L${pw / 2 + 0.2},0.48 L${pw / 2 + 0.16},0.36 L${pw / 2 - 0.16},0.36 L${pw / 2 - 0.2},0.48 Z`, fill: [C.darkD, C.brownD, C.plum][i], stroke: INK, "stroke-width": 0.03 }, gg);
  if (i === 0) el("path", { d: `M${pw / 2 - 0.2},0.8 L${pw / 2 + 0.2},0.8 L${pw / 2},1.0 Z`, fill: C.red, stroke: INK, "stroke-width": 0.03 }, gg);
  if (i === 1) el("path", { d: `M${pw / 2 - 0.14},0.86 q0.14,0.08 0.28,0`, stroke: INK, "stroke-width": 0.04, fill: "none" }, gg);
  if (i === 2) el("path", { d: `M${pw / 2 - 0.16},0.82 q0.16,-0.06 0.32,0`, stroke: INK, "stroke-width": 0.06, fill: "none" }, gg);
  txt(["$500", "$300", "$1000"][i], { x: pw / 2, y: 1.28, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.22, "text-anchor": "middle", fill: C.redD }, gg);
  el("circle", { cx: pw / 2, cy: 0.08, r: 0.035, fill: C.steelD }, gg);
}, { name: `poster-${i}` }));
// back block behind the poster bank
{
  const back = G.posters.map(([x, y]) => [x - nP[0] * 0.5, y - nP[1] * 0.5]);
  const ext = [[back[0][0] - (back[1][0] - back[0][0]) * 0.7, back[0][1] - (back[1][1] - back[0][1]) * 0.7], [back[2][0] + (back[2][0] - back[1][0]) * 0.7, back[2][1] + (back[2][1] - back[1][1]) * 0.7]];
  PHYS.walls.push({ points: ext, width: 0.24 });
  add(19.0, (g) => extrude(g, band(ext, 0.24), 0, 1.15, WOOD), { name: "posters-back" });
}

// ---------------------------------------------------------------- piano with the scoop in front of the keys
// Its back slopes a little (under its lid), so a ball that lands behind it rolls off to the
// left, by the scoop, rather than resting against it.
PHYS.blocks.push({ name: "piano", points: [[G.piano.x0, G.piano.y0 + 0.35], [G.piano.x1, G.piano.y0 - 0.2], [G.piano.x1, G.piano.y1], [G.piano.x0, G.piano.y1]] });
add(15.9, (g) => {
  const { x0, x1, y0, y1, h } = G.piano;
  const box = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
  SH.loops.push([box, h]);
  extrude(g, box, 0, h, { side: "#6b2f1c", top: "#8f4428" });
  // carved front panel on the left face (faces the playfield centre)
  const fp = el("g", { transform: frame([x0 - 0.01, y0, h], [0, 1, 0], [0, 0, -1]) }, g);
  const len = y1 - y0;
  el("rect", { x: 0.2, y: 0.2, width: len - 0.4, height: 1.05, rx: 0.1, fill: "#57240f", stroke: INK, "stroke-width": 0.05 }, fp);
  el("rect", { x: len / 2 - 0.55, y: 0.32, width: 1.1, height: 0.75, fill: C.cream, stroke: INK, "stroke-width": 0.04 }, fp);
  for (let i = 0; i < 4; i++) el("path", { d: `M${len / 2 - 0.45},${0.45 + i * 0.16} H${len / 2 + 0.45}`, stroke: INK, "stroke-width": 0.02 }, fp);
  for (let i = 0; i < 3; i++) el("circle", { cx: len / 2 - 0.3 + i * 0.28, cy: 0.5 + (i % 2) * 0.16, r: 0.04, fill: INK }, fp);
  el("rect", { x: 0.25, y: 1.95, width: len - 0.5, height: 0.85, rx: 0.1, fill: "#57240f", stroke: INK, "stroke-width": 0.05 }, fp);
  // key bed sticking out toward the playfield
  const kb = [[x0 - 0.55, y0 + 0.35], [x0, y0 + 0.35], [x0, y1 - 0.35], [x0 - 0.55, y1 - 0.35]];
  extrude(g, kb, 1.32, 1.5, { side: "#57240f", top: "#fbf1dc" });
  const kg = el("g", { transform: frame([x0 - 0.55, y0 + 0.35, 1.51]) }, g);
  const kl = y1 - y0 - 0.7;
  for (let i = 0; i < 18; i++) el("path", { d: `M0,${(i * kl) / 18} H0.5`, stroke: INK, "stroke-width": 0.02 }, kg);
  for (let i = 0; i < 18; i++) if ([1, 2, 4, 5, 6].includes(i % 7)) el("rect", { x: 0.2, y: (i * kl) / 18 - 0.05, width: 0.3, height: 0.09, fill: INK }, kg);
  // candle on top
  const cp = [x0 + 0.5, y0 + 0.6];
  celCyl(g, cp[0], cp[1], 0.12, h, h + 0.55, { side: C.cream, top: C.cream, sw: 1.4 });
  const fl = proj(cp[0], cp[1], h + 0.75);
  el("circle", { cx: fl[0], cy: fl[1], r: 14, fill: "#ffc861", opacity: 0.35, style: "mix-blend-mode:screen" }, g);
  el("path", { d: `M${fl[0]},${fl[1] - 7} q4,6 0,9 q-4,-3 0,-9z`, fill: C.sun, stroke: INK, "stroke-width": 1.1 }, g);
}, { name: "piano" });
// scoop rim
PHYS.scoop = { x: G.scoop[0], y: G.scoop[1], r: 0.47 };
add(13.7, (g) => {
  const [x, y] = G.scoop;
  el("path", { d: poly(ellipseAt(x, y, 0.0, 0.62)) + poly(ellipseAt(x, y, 0.0, 0.47).reverse()), fill: C.steel, "fill-rule": "evenodd", stroke: INK, "stroke-width": 1.6 }, g);
}, { name: "scoop-rim", flat: true });

// ---------------------------------------------------------------- bull: captive ball + mechanical bull toy
// Bucking: how far the bull rears (degrees) and lifts (inches).
export const BULL = { frames: [[0, 0], [-6, 0.15], [-13, 0.32], [-20, 0.45]] };
dyn("bull", 18.7, BULL.frames, (g, [lean, lift]) => {
  const gg = toy(g, 17.95, 18.35, (k) => mechBull(k, { sw: 0.12 }), { height: 3.7, unit: 4.6, z: 0.3 + lift });
  const base = proj(17.95, 18.35, 0.3 + lift);
  gg.setAttribute("transform", `rotate(${lean} ${f2(base[0])} ${f2(base[1])}) ` + gg.getAttribute("transform"));
});
if (!EXPORT) add(18.7, (g) => toy(g, 17.95, 18.35, (k) => mechBull(k, { sw: 0.12 }), { height: 3.7, unit: 4.6, z: 0.3 }));
PHYS.captive = { x: G.captive[0], y: G.captive[1], lane: { x0: G.bullLane.x0, x1: 18.72, y0: 18.75, y1: G.bullLane.y1 } };
add(18.75, (g) => {
  // chute gate at the top of the lane, the bull waiting behind it
  for (const z of [0.55, 1.15, 1.75]) {
    const a = proj(16.6, 18.75, z), b = proj(18.7, 18.75, z);
    el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 7, "stroke-linecap": "round" }, g);
    el("path", { d: poly([a, b], false), stroke: "#b8703f", "stroke-width": 4, "stroke-linecap": "round" }, g);
  }
}, { name: "bull-chute" });
SH.blobs.push([17.7, 18.3, 1.5, 0.6]);
// The bull and its chute close the pen: a ball coming down behind them rolls off their backs to
// the left, out past the pen.
PHYS.blocks.push({ name: "bull", points: [[16.6, 18.75], [18.65, 18.75], [18.65, 16.9], [17.6, 17.3], [16.6, 18.2]] });
if (!EXPORT) add(G.captive[1], (g) => ball(g, ...G.captive));

// ---------------------------------------------------------------- the sheriff and the outlaw
SH.blobs.push([3.35, 15.75, 0.9, 0.4]);
PHYS.blocks.push({ name: "crate", points: [[2.75, 15.3], [3.95, 15.3], [3.95, 16.2], [2.75, 16.2]] });
add(15.8, (g) => {
  const box = [[2.75, 15.3], [3.95, 15.3], [3.95, 16.2], [2.75, 16.2]];
  extrude(g, box, 0, 0.55, { side: "#a0602e", top: "#c98a4a", planks: 1 });
  toy(g, 3.35, 15.75, (k) => sheriff(k), { height: 6.2, unit: 11.6, z: 0.55 });
}, { name: "sheriff" });
// (Left and down of where the mockup had them, so a ball fits between the rock and the arch.)
const ROCK = [[16.2, 6.9], [16.5, 6.0], [17.4, 5.7], [18.2, 6.1], [18.3, 7.0], [17.3, 7.4]].map(([x, y]) => [x - 1.1, y + 0.6]);
SH.blobs.push([16.05, 7.2, 1.0, 0.5]);
PHYS.blocks.push({ name: "rock", points: ROCK });
add(7.3, (g) => {
  const rock = ROCK;
  extrude(g, rock, 0, 0.7, { side: "#9b5a34", top: "#b8703f" });
  toy(g, 16.15, 7.15, (k) => outlaw(k), { height: 6.0, unit: 11.4, z: 0.7 });
}, { name: "outlaw" });

// ---------------------------------------------------------------- inlane/outlane rollovers, gate, plunger
const ROLLOVERS = [[1.1, 33.2], [2.7, 33.0], [16.3, 33.0], [17.9, 33.2], [1.25, 24.0]];
PHYS.rollovers = ROLLOVERS.map(([x, y]) => ({ x, y }));
for (const [x, y] of ROLLOVERS) add(y, (g) => {
  el("path", { d: projPath(cam, [[x - 0.12, y - 0.5], [x - 0.12, y + 0.5], [x + 0.12, y + 0.5], [x + 0.12, y - 0.5]], 0.07, false), fill: "none", stroke: INK, "stroke-width": 3.6 }, g);
  el("path", { d: projPath(cam, [[x - 0.12, y - 0.5], [x - 0.12, y + 0.5], [x + 0.12, y + 0.5], [x + 0.12, y - 0.5]], 0.07, false), fill: "none", stroke: C.steel, "stroke-width": 1.6 }, g);
}, { name: "rollover", flat: true });
// One-way gates: a wire flap hanging from a bar across a lane, that swings only `way`. At the
// top of the shooter lane it lets the ball out but not back, and slopes so that a ball falling
// back rolls off it onto the playfield. At the top of the left orbit it lets a ball shot up the
// orbit go round the top, but stops one coming round the other way (a plunged ball), which drops
// into the top lanes.
PHYS.gates = [];
function gateItem(a, b, way) {
  PHYS.gates.push({ a, b, way });
  add(Math.max(a[1], b[1]) + 0.05, (g) => {
    const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const along = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    const at = (p, k, off, z) => proj(p[0] + along[0] * k + way[0] * off, p[1] + along[1] * k + way[1] * off, z);
    el("path", { d: poly([proj(a[0], a[1], 1.3), proj(b[0], b[1], 1.3)], false), stroke: INK, "stroke-width": 4 }, g);
    el("path", { d: poly([proj(a[0], a[1], 1.3), proj(b[0], b[1], 1.3)], false), stroke: C.steel, "stroke-width": 2 }, g);
    const wire = [at(a, 0.15, 0.1, 1.28), at(a, 0.15, 0.3, 0.25), at(b, -0.15, 0.3, 0.25), at(b, -0.15, 0.1, 1.28)];
    el("path", { d: poly(wire, false), stroke: INK, "stroke-width": 3.4, fill: "none" }, g);
    el("path", { d: poly(wire, false), stroke: C.steel, "stroke-width": 1.4, fill: "none" }, g);
  }, { name: "gate" });
}
// (Its low end just over the top of the lane's wall, so a ball rolls off it, not into it.)
gateItem([SHOOTER.x - 0.12, SHOOTER.top - 0.1], [19.95, 9.5], [0, -1]);
{
  // Across the orbit near its top, from its inner wall to the arch; up the orbit is round the
  // arch, angles increasing.
  // Just past the end of its inner wall: a ball it stops slides off it, not into a corner.
  const deg = 245, a = (deg * Math.PI) / 180;
  const at = (r) => [ARCH.cx + r * Math.cos(a), ARCH.cy + r * Math.sin(a)];
  gateItem(at(8.15), at(ARCH.r + 0.05), [-Math.sin(a), Math.cos(a)]);
}
// The plunger, pulled back by `pull` inches.
export const PLUNGER = { pulls: [0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1.05, 1.2] };
PHYS.plunger = { x: 19.42, y: 40.6, maxPull: 1.2 };
dyn("plunger", 41.6, PLUNGER.pulls, (g, pull) => plunger(g, pull));
if (!EXPORT) add(41.6, (g) => plunger(g, 0));
function plunger(g, pull) {
  const x = 19.42, z = 0.5;
  const a = proj(x, 41.2 + pull, z), b = proj(x, 43.6, z), s = cam.scale(x, 41.5, z);
  el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 0.26 * s + 3, "stroke-linecap": "round" }, g);
  el("path", { d: poly([a, b], false), stroke: C.steel, "stroke-width": 0.26 * s, "stroke-linecap": "round" }, g);
  const t0 = proj(x, 40.6 + pull, z), t1 = proj(x, 41.25 + pull, z);
  el("path", { d: poly([t0, t1], false), stroke: INK, "stroke-width": 0.46 * s + 3, "stroke-linecap": "round" }, g);
  el("path", { d: poly([t0, t1], false), stroke: "#3a2a24", "stroke-width": 0.46 * s, "stroke-linecap": "round" }, g);
  el("path", { d: poly([t0.map((v, i) => v - (i ? 0.12 * s : 0.15 * s)), t1.map((v, i) => v - (i ? 0.12 * s : 0.15 * s))], false), stroke: "#6d5a4e", "stroke-width": 0.16 * s, "stroke-linecap": "round" }, g);
}

// ---------------------------------------------------------------- the ball (in play, heading for the doors)
function ball(g, x, y) {
  const r = 0.53, c = proj(x, y, r), s = cam.scale(x, y, r) * r;
  el("ellipse", { cx: c[0], cy: c[1], rx: s, ry: s, fill: "#9a948a", stroke: INK, "stroke-width": 2 }, g);
  const cid = id("bl");
  el("ellipse", { cx: c[0], cy: c[1], rx: s, ry: s }, el("clipPath", { id: cid }, odefs));
  const cg = el("g", { "clip-path": `url(#${cid})` }, g);
  el("rect", { x: c[0] - s, y: c[1] + s * 0.2, width: 2 * s, height: s, fill: "#d9955a", opacity: 0.8 }, cg);
  el("rect", { x: c[0] - s, y: c[1] + s * 0.62, width: 2 * s, height: s, fill: "#8f5228", opacity: 0.85 }, cg);
  el("ellipse", { cx: c[0] - s * 0.1, cy: c[1] - s * 0.15, rx: s * 0.95, ry: s * 0.55, fill: "#e8e4dc" }, cg);
  el("ellipse", { cx: c[0] + s * 0.5, cy: c[1] - s * 0.1, rx: s * 0.5, ry: s * 0.75, fill: "#6d675f", opacity: 0.8 }, cg);
  el("ellipse", { cx: c[0] - s * 0.35, cy: c[1] - s * 0.4, rx: s * 0.28, ry: s * 0.18, fill: "#fff", transform: `rotate(-30 ${c[0] - s * 0.35} ${c[1] - s * 0.4})` }, g);
  el("ellipse", { cx: c[0], cy: c[1], rx: s, ry: s, fill: "none", stroke: INK, "stroke-width": 2.2 }, g);
}
if (!EXPORT) add(23.6, (g) => {
  ball(g, 9.75, 23.5);
  // speed streaks behind the ball (comic motion)
  for (const dx of [-0.32, 0, 0.32]) { const a = proj(9.75 + dx * 0.9, 24.3, 0.53), b = proj(9.75 + dx * 1.1, 25.8, 0.4); el("path", { d: poly([a, b], false), stroke: INK, "stroke-width": 2.4, "stroke-linecap": "round", opacity: 0.85 }, g); }
});
if (!EXPORT) SH.circles.push([9.75, 23.5, 0.5, 0.5]);

// ---------------------------------------------------------------- apron
add(41.0, (g) => {
  const top = [[0.4, 40.1], [7.0, 40.1]].concat(arcPts(9.5, 40.15, 2.5, 180, 0, 20, -0.55)).concat([[12.0, 40.1], [18.6, 40.1], [18.6, 44.3], [0.4, 44.3]]);
  extrude(g, top, 0, 0.32, { side: C.brownD, top: "#6e3b22" });
  const gg = el("g", { transform: frame([0.4, 40.1, 0.32]) }, g);
  const cid = id("ap");
  el("path", { d: poly(top.map(([x, y]) => [x - 0.4, y - 40.1])) }, el("clipPath", { id: cid }, odefs));
  const a = el("g", { "clip-path": `url(#${cid})` }, gg);
  for (let i = 0; i < 18; i++) el("path", { d: `M${i * 1.05},0 V4`, stroke: "#5a2f1b", "stroke-width": 0.05 }, a);
  for (const [cx, title, lines] of [[3.2, "HOW TO PLAY", ["SHOOT THE MINE TO LOAD THE CART", "3 CARTS START RUNAWAY MULTIBALL", "WANTED POSTERS START A BOUNTY", "RIDE THE BULL FOR A SUPER JACKPOT"]], [15.0, "SHOWDOWN", ["COMPLETE 6 MODES ON THE STAR", "TO FACE THE OUTLAW AT HIGH NOON", "RANKS: GREENHORN - DEPUTY", "SHERIFF - MARSHAL - LEGEND"]]]) {
    el("path", { d: `M${cx - 2.5},0.5 L${cx + 2.5},0.45 L${cx + 2.45},2.25 L${cx - 2.48},2.3 Z`, fill: "#f3dcae", stroke: INK, "stroke-width": 0.06 }, a);
    txt(title, { x: cx, y: 0.86, "font-family": "Superclarendon", "font-weight": 900, "font-size": 0.27, "text-anchor": "middle", fill: C.redD }, a);
    lines.forEach((t, i) => txt(t, { x: cx, y: 1.2 + i * 0.27, "font-family": "American Typewriter", "font-weight": 700, "font-size": 0.165, "text-anchor": "middle", fill: INK }, a));
    for (const sx of [cx - 2.3, cx + 2.3]) el("circle", { cx: sx, cy: 0.62, r: 0.07, fill: C.steelD, stroke: INK, "stroke-width": 0.02 }, a);
  }
  star6(a, 9.1, 1.5, 0.5, { sw: 0.05 });
}, { name: "apron" });
PHYS.drain = { y: 40.3 };
PHYS.flippers = [{ pivot: G.flipL, side: "left" }, { pivot: G.flipR, side: "right" }];

// ---------------------------------------------------------------- cabinet
const HC = 3.4, RAIL = 1.75;
function plankTop(g, x0, x1, y0, y1, z, seed) {
  const q = [proj(x0, y0, z), proj(x1, y0, z), proj(x1, y1, z), proj(x0, y1, z)];
  el("path", { d: poly(q), fill: "#7d4622", stroke: INK, "stroke-width": SW }, g);
  const R = rng(seed);
  const n = 3;
  for (let i = 1; i < n; i++) { const x = lerp(x0, x1, i / n); el("path", { d: poly([proj(x, y0, z), proj(x, y1, z)], false), stroke: INK, "stroke-width": 1.4 }, g); }
  for (let i = 0; i < n; i++) {
    const xa = lerp(x0, x1, i / n), xb = lerp(x0, x1, (i + 1) / n);
    el("path", { d: poly([proj(lerp(xa, xb, 0.55), y0, z), proj(xb, y0, z), proj(xb, y1, z), proj(lerp(xa, xb, 0.55), y1, z)]), fill: "#5e3218", opacity: 0.8 }, g);
    for (let k = 0; k < 4; k++) { const yy = y0 + R() * (y1 - y0), len = 2 + R() * 4; el("path", { d: poly([proj(lerp(xa, xb, 0.2 + R() * 0.3), yy, z), proj(lerp(xa, xb, 0.2 + R() * 0.3), yy + len, z)], false), stroke: "#5e3418", "stroke-width": 1, opacity: 0.8 }, g); }
    for (let k = 0; k < 9; k++) { const p = proj(lerp(xa, xb, 0.5), y0 + (k + 0.5) * ((y1 - y0) / 9), z); el("circle", { cx: p[0], cy: p[1], r: 1.4, fill: "#3a2219" }, g); }
  }
}
function cabinetBack(g) {
  const back = [proj(0, 0, 0), proj(W, 0, 0), proj(W, 0, HC), proj(0, 0, HC)];
  el("path", { d: poly(back), fill: "#2a160d", stroke: INK, "stroke-width": SW }, g);
  for (const x of [0, W]) el("path", { d: poly([proj(x, -0.01, 0), proj(x, L + 3.4, 0), proj(x, L + 3.4, HC), proj(x, -0.01, HC)]), fill: "#24130b", stroke: INK, "stroke-width": SW }, g);
}
function cabinetFront(g) {
  plankTop(g, -RAIL, 0, -RAIL, L + 3.4, HC, 3);
  plankTop(g, W, W + RAIL, -RAIL, L + 3.4, HC, 7);
  // brass trim along the glass
  for (const [xa, xb] of [[-0.2, 0.0], [W, W + 0.2]]) {
    const q = [proj(xa, -0.2, HC + 0.03), proj(xb, -0.2, HC + 0.03), proj(xb, L + 3.4, HC + 0.03), proj(xa, L + 3.4, HC + 0.03)];
    el("path", { d: poly(q), fill: "#d9a441", stroke: INK, "stroke-width": 1.4 }, g);
  }
  el("path", { d: poly([proj(-RAIL, -RAIL, HC), proj(W + RAIL, -RAIL, HC), proj(W + RAIL, 0, HC), proj(-RAIL, 0, HC)]), fill: "#7a4220", stroke: INK, "stroke-width": SW }, g);
  const yF = L + 3.4;
  el("path", { d: poly([proj(-RAIL, yF, HC), proj(W + RAIL, yF, HC), proj(W + RAIL, yF, HC - 9), proj(-RAIL, yF, HC - 9)]), fill: "#5e3418", stroke: INK, "stroke-width": SW }, g);
  for (let i = 1; i < 4; i++) el("path", { d: poly([proj(-RAIL, yF, HC - i * 1.6), proj(W + RAIL, yF, HC - i * 1.6)], false), stroke: INK, "stroke-width": 1.6 }, g);
  const sb = el("g", { transform: frame([W / 2, yF + 0.01, HC - 2.2], [1, 0, 0], [0, 0, -1]) }, g);
  star6(sb, 0, 0, 0.95, { sw: 0.09 });
  // plunger with a wooden handle
  const px = W - 0.85, pz = HC - 1.6, c = proj(px, yF, pz), s = cam.scale(px, yF, pz);
  el("ellipse", { cx: c[0], cy: c[1], rx: 0.7 * s, ry: 0.7 * s, fill: C.gold, stroke: INK, "stroke-width": SW }, g);
  const tip = proj(px, yF + 1.1, pz);
  el("path", { d: poly([c, tip], false), stroke: INK, "stroke-width": 0.3 * s + 4, "stroke-linecap": "round" }, g);
  el("path", { d: poly([c, tip], false), stroke: C.steel, "stroke-width": 0.3 * s, "stroke-linecap": "round" }, g);
  el("ellipse", { cx: tip[0], cy: tip[1] + 4, rx: 0.5 * s, ry: 0.62 * s, fill: C.brownL, stroke: INK, "stroke-width": SW }, g);
  el("ellipse", { cx: tip[0] - 0.15 * s, cy: tip[1], rx: 0.15 * s, ry: 0.25 * s, fill: "#e9b47a" }, g);
  // brass lockdown bar
  const y0 = L + 2.2, y1 = L + 3.4, z0 = HC, z1 = HC + 0.42;
  const topQ = [proj(-RAIL, y0, z1), proj(W + RAIL, y0, z1), proj(W + RAIL, y1, z1), proj(-RAIL, y1, z1)];
  const frontQ = [proj(-RAIL, y1, z1), proj(W + RAIL, y1, z1), proj(W + RAIL, y1, z0 - 0.2), proj(-RAIL, y1, z0 - 0.2)];
  el("path", { d: poly(topQ), fill: "#d9a441", stroke: INK, "stroke-width": SW }, g);
  el("path", { d: poly([topQ[0], topQ[1], proj(W + RAIL, y0 + 0.5, z1), proj(-RAIL, y0 + 0.5, z1)]), fill: "#b8862e" }, g);
  el("path", { d: poly(topQ), fill: "none", stroke: INK, "stroke-width": SW }, g);
  el("path", { d: poly(frontQ), fill: "#8f6420", stroke: INK, "stroke-width": SW }, g);
  el("path", { d: poly([proj(-RAIL + 0.3, y0 + 0.4, z1), proj(W + RAIL - 0.3, y0 + 0.4, z1)], false), stroke: "#fff3c4", "stroke-width": 3 }, g);
}
// ---------------------------------------------------------------- draw
// Shadows cast on the floor (hard-edged comic shadows: offset, flat, multiplied).
const SH0 = { circles: SH.circles.slice(), loops: SH.loops.slice(), blobs: SH.blobs.slice() };
function drawShadows(sg) {
  const off = (h) => [0.12 + h * 0.12, 0.1 + h * 0.15];
  for (const [x, y, r, h] of SH.circles) { const [dx, dy] = off(h); el("ellipse", { cx: x + dx, cy: y + dy, rx: r, ry: r * 1.05, fill: "#3a1206" }, sg); }
  for (const [loop, h] of SH.loops) { const [dx, dy] = off(h); el("path", { d: poly(loop.map(([x, y]) => [x + dx, y + dy])), fill: "#3a1206" }, sg); }
  for (const b of SH.blobs) {
    if (b[0] === "ramp") {
      const pts = RL.map(([x, y, z]) => [x + 0.2 + z * 0.35, y + 0.15 + z * 0.4]).concat(RR.slice().reverse().map(([x, y, z]) => [x + 0.2 + z * 0.35, y + 0.15 + z * 0.4]));
      el("path", { d: poly(pts), fill: "#3a1206" }, sg);
    } else el("ellipse", { cx: b[0] + 0.4, cy: b[1] + 0.25, rx: b[2], ry: b[3], fill: "#3a1206" }, sg);
  }
}
function glass(g) {
  const gq = (t0, t1, w) => [proj(lerp(0, W, t0), 0, HC), proj(lerp(0, W, t0) + w, 0, HC), proj(lerp(0, W, t1) + w, L, HC), proj(lerp(0, W, t1), L, HC)];
  el("path", { d: poly(gq(0.62, 0.05, 1.6)), fill: "#fff6e0", opacity: 0.07 }, g);
  el("path", { d: poly(gq(0.74, 0.17, 0.5)), fill: "#fff6e0", opacity: 0.08 }, g);
}
const GI = [[4.6, 33.4, 3.0, 1], [14.4, 33.4, 3.0, 1], [9.6, 18.5, 3.2, 0.7], [12.6, 11.0, 4.0, 0.6], [17.0, 14.0, 2.6, 0.6], [5.4, 9.5, 3.0, 0.7]];
const BACKBOX = { x: 1050, y: 26, w: 830, h: 1030 };
items.sort((a, b) => a.key - b.key);

if (!EXPORT) {
  cabinetBack(el("g", {}, ov));
  for (const it of items) it.fn(el("g", {}, ov));
  glass(topL);
  cabinetFront(el("g", {}, topL));
  drawShadows(el("g", { opacity: 0.38 }, fs));
  drawFloorLight(fs, fdefs, { C, W, L, lit: FL.lit, gi: GI });
  drawBackbox(head, { C, mix, shade, box: BACKBOX });
} else {
  const clearAll = () => {
    for (const s of [bg, topL, head]) s.innerHTML = "";
    ov.innerHTML = "";
    odefs.innerHTML = "";
    ov.appendChild(odefs);
    floorDiv.innerHTML = "";
  };
  const newFloor = (opts) => {
    const s = el("svg", { width: K * W, height: K * L, viewBox: `0 0 ${W} ${L}` });
    floorDiv.appendChild(s);
    const d = el("defs", {}, s);
    return { s, d, fl: drawFloor(s, d, { C, G, mix, shade, archPts, W, L, ...opts }) };
  };
  window.SCENE = {
    items: items.map((it, i) => ({ i, key: it.key, name: it.name, flat: it.flat })),
    dynamic: Object.fromEntries(Object.entries(DYN).map(([k, v]) => [k, { key: v.key, states: v.states }])),
    // The room, the cabinet behind the table, the floor with every lamp off, the shadows of
    // everything that doesn't move, and the flat parts.
    background() {
      clearAll();
      drawBackground(bg, { C, mix, shade });
      cabinetBack(el("g", {}, ov));
      const { s, d } = newFloor({ allOff: true });
      SH.circles = SH0.circles.slice(); SH.loops = SH0.loops.slice(); SH.blobs = SH0.blobs.slice();
      const scratch = el("g", {}, ov);
      for (const it of items) it.fn(scratch);
      G.bumpers.forEach((_, i) => DYN[`bumper-${i}`].fn(scratch, false));
      scratch.remove();
      drawShadows(el("g", { opacity: 0.38 }, s));
      drawFloorLight(s, d, { C, W, L, lit: [], gi: GI });
      for (const it of items) if (it.flat) it.fn(el("g", {}, ov));
    },
    item(i) { clearAll(); items[i].fn(el("g", {}, ov)); },
    state(name, i) { clearAll(); DYN[name].fn(el("g", {}, ov), DYN[name].states[i]); },
    // One lamp, lit, with its glow: drawn over the floor when it's on.
    lamp(name) {
      clearAll();
      const { s, d, fl } = newFloor({ only: name });
      drawFloorLight(s, d, { C, W, L, lit: fl.lit, gi: [], onlyLamps: true });
    },
    lamps() { clearAll(); newFloor({ allOff: true }); return INSERTS.map((i) => i.name); },
    // The cabinet's front rails, lockdown bar and glass: over everything on the table.
    front() { clearAll(); glass(topL); cabinetFront(el("g", {}, topL)); },
    backbox(opts) { clearAll(); return drawBackbox(head, { C, mix, shade, box: BACKBOX, ...opts }); },
    // The ball, at the middle of the table, for the game to scale wherever it is.
    ball() { clearAll(); ball(el("g", {}, ov), W / 2, L / 2); return { at: [W / 2, L / 2, 0.53], scale: cam.scale(W / 2, L / 2, 0.53) }; },
    physics() {
      return {
        size: [W, L],
        camera: { tilt: 39, dist: 84, fitTop: 112, fitBottom: 932, centerX: 520 },
        flipper: FLIPPER,
        ...PHYS,
        rubbers: PHYS.rubbers.map(({ circles }) => circles),
      };
    },
  };
}
window.READY = true;
