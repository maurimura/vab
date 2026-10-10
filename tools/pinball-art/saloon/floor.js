// Saloon playfield print: sunset sky, saloon floorboards, the boardwalk and the dusty street, in comic ink.
import { el, txt, id, poly, smooth, arcPts, halftone, linGrad, radGrad, rng, f2 } from "./lib.js";
import { star6 } from "./chars.js";

// `only`: just that insert, lit (the game's lamp overlays). `allOff`: every insert unlit (the
// floor under them). Otherwise as in the mockup.
export const INSERTS = [];
export function drawFloor(fs, defs, { C, G, mix, shade, archPts, W, L, only, allOff = false }) {
  const INK = C.ink, IW = 0.075;
  const R = rng(17);
  const P2 = (g, d, fill, stroke = INK, sw = IW, extra = {}) => el("path", { d, fill, stroke, "stroke-width": sw, "stroke-linejoin": "round", "stroke-linecap": "round", ...extra }, g);
  const clip = (d) => { const cid = id("fc"); el("path", { d }, el("clipPath", { id: cid }, defs)); return cid; };
  {
    const fg = el("filter", { id: "grain", x: 0, y: 0, width: "100%", height: "100%", filterUnits: "userSpaceOnUse" }, defs);
    el("feTurbulence", { type: "fractalNoise", baseFrequency: 7, numOctaves: 2, seed: 2, result: "n" }, fg);
    el("feColorMatrix", { type: "matrix", values: "0 0 0 0 0.35  0 0 0 0 0.2  0 0 0 0 0.1  0 0 0 -1.2 0.75" }, fg);
    const fb = el("filter", { id: "blur", x: "-100%", y: "-100%", width: "300%", height: "300%" }, defs);
    el("feGaussianBlur", { stdDeviation: 0.3 }, fb);
  }
  const art = el("g", only !== undefined ? { visibility: "hidden" } : {}, fs);

  // ------------------------------------------------ dusty street base
  el("rect", { x: 0, y: 0, width: W, height: L, fill: "#f0b96c" }, art);
  halftone(art, { x0: 0, y0: 18, x1: W, y1: L, pitch: 0.24, angle: 20, fn: (x, y) => 0.05 + 0.3 * Math.abs(x - 9.5) / 9.5 + (y > 36 ? 0.15 : 0), fill: "#d68f48" });
  // long sunset shadows thrown across the street by the saloon and the posts
  {
    const sh = el("g", { opacity: 0.7 }, art);
    P2(sh, "M2.2,20.0 L16.5,20.0 L13.2,23.6 L-1.0,23.6 Z", "#d08443", "none", 0);
    P2(sh, "M0.4,23.6 L13.2,23.6 L12.0,25.0 L0.4,25.0 Z", "#c47a3c", "none", 0, { opacity: 0.6 });
    for (const x of [4.2, 8.0, 11.2, 15.0]) P2(sh, `M${x},23.4 L${x - 3.6},29.5 L${x - 3.1},29.5 L${x + 0.35},23.4 Z`, "#c47a3c", "none", 0);
  }
  // wagon ruts and hoofprints
  for (const off of [-1.1, 1.1]) P2(art, smooth([[9.5 + off - 2.5, 41], [9.5 + off - 0.8, 34], [9.5 + off + 0.4, 28], [9.5 + off * 1.4, 22]], false), "none", "#b86e30", 0.16, { opacity: 0.8 });
  for (let i = 0; i < 14; i++) {
    const x = 1 + R() * 17, y = 20.5 + R() * 18;
    if (Math.hypot(x - G.star[0], y - G.star[1]) < 3.2) continue;
    P2(art, `M${x - 0.13},${y} a0.13,0.15 0 1,1 0.26,0`, "none", "#a95f26", 0.06);
  }
  for (let i = 0; i < 40; i++) { const x = 0.5 + R() * 18, y = 20 + R() * 20; el("circle", { cx: x, cy: y, r: 0.04 + R() * 0.06, fill: "#9a5420", opacity: 0.7 }, art); }
  // tumbleweed
  const tumble = (x, y, r) => { const gg = el("g", {}, art); for (let k = 0; k < 7; k++) P2(gg, `M${x + r * Math.cos(k)},${y + r * Math.sin(k * 1.7)} Q${x + r * Math.sin(k * 2.1)},${y - r * Math.cos(k)} ${x - r * Math.cos(k * 1.3)},${y + r * Math.sin(k)}`, "none", "#7a4a1e", 0.05); P2(gg, `M${x - r * 1.7},${y + r * 0.9} l0.5,0 M${x - r * 2},${y + r * 0.6} l0.6,0`, "none", INK, 0.05); };
  tumble(4.9, 26.6, 0.55);
  tumble(14.6, 28.6, 0.45);

  // ------------------------------------------------ sunset sky under the arch
  const horizon = 7.35;
  const skyD = poly(archPts) + `L${W - 0.25},${horizon}L0.4,${horizon}Z`;
  el("path", { d: `M0,0H${W}V${G.ARCH.cy + 0.3}H0Z`, fill: "#3b2219" }, art);
  const sky = el("g", { "clip-path": `url(#${clip(skyD)})` }, art);
  const bands = ["#b9381c", "#d8501f", "#ec7428", "#f6a03a", "#ffc94f"];
  bands.forEach((c, i) => el("rect", { x: 0, y: i * 1.5 - 0.3, width: W, height: 10, fill: c }, sky));
  // sun with rays
  const sun = [12.6, horizon + 0.2];
  for (let i = 0; i < 18; i++) { const a0 = Math.PI + (i / 18) * Math.PI, a1 = a0 + Math.PI / 36; if (i % 2) continue; P2(sky, `M${sun[0]},${sun[1]}L${sun[0] + 14 * Math.cos(a0)},${sun[1] + 14 * Math.sin(a0)}L${sun[0] + 14 * Math.cos(a1)},${sun[1] + 14 * Math.sin(a1)}Z`, "#ffe08a", "none", 0, { opacity: 0.35 }); }
  el("circle", { cx: sun[0], cy: sun[1], r: 3.1, fill: "#ffe7a0", stroke: INK, "stroke-width": IW }, sky);
  for (let k = 0; k < 3; k++) el("rect", { x: sun[0] - 3.2, y: sun[1] - 1.1 + k * 0.42, width: 6.4, height: 0.16, fill: "#ffc94f" }, sky);
  // mesas
  P2(sky, `M0,${horizon} L0,5.2 L1.6,5.1 L2.0,4.2 L4.6,4.1 L5.1,5.3 L6.4,5.6 L7.2,${horizon} Z`, "#8a3f3a", INK, IW);
  P2(sky, `M3.4,4.15 L4.6,4.1 L5.1,5.3 L6.4,5.6 L7.2,${horizon} L4.4,${horizon} Z`, "#6a2c33", "none", 0);
  P2(sky, `M14.8,${horizon} L15.6,5.9 L16.4,5.7 L16.8,4.6 L19.0,4.5 L19.4,5.6 L${W},5.8 L${W},${horizon} Z`, "#8a3f3a", INK, IW);
  P2(sky, `M18.2,4.52 L19.0,4.5 L19.4,5.6 L${W},5.8 L${W},${horizon} L17.6,${horizon} Z`, "#6a2c33", "none", 0);
  P2(sky, `M7.0,${horizon} L7.6,6.7 L8.4,6.6 L8.9,${horizon} Z`, "#9b4a3c", INK, IW * 0.8);
  // vultures
  for (const [x, y, s] of [[8.4, 1.6, 0.35], [9.5, 1.1, 0.28], [14.6, 1.5, 0.3]]) P2(sky, `M${x - s},${y} q${s / 2},${-s / 2} ${s},0 q${s / 2},${-s / 2} ${s},0`, "none", INK, 0.07);
  P2(art, `M0.4,${horizon}H${W - 0.25}`, "none", INK, IW * 1.4);

  // ------------------------------------------------ saloon floorboards (inside)
  const wallY = 18.3;
  const inside = `M0.4,${horizon}H18.6V${wallY}H0.4Z`;
  const ins = el("g", { "clip-path": `url(#${clip(inside)})` }, art);
  const boards = ["#b46d37", "#a8622f", "#bd7841", "#ad6834"];
  for (let i = 0; i < 26; i++) {
    const x = 0.4 + i * 0.72;
    el("rect", { x, y: horizon, width: 0.72, height: wallY - horizon, fill: boards[i % 4] }, ins);
    el("rect", { x: x + 0.48, y: horizon, width: 0.24, height: wallY - horizon, fill: "#8f5228", opacity: 0.55 }, ins);
    P2(ins, `M${x},${horizon}V${wallY}`, "none", INK, 0.05);
    let y = horizon + R() * 3;
    while (y < wallY) { P2(ins, `M${x},${y}h0.72`, "none", INK, 0.04); el("circle", { cx: x + 0.12, cy: y + 0.12, r: 0.04, fill: INK }, ins); el("circle", { cx: x + 0.6, cy: y + 0.12, r: 0.04, fill: INK }, ins); y += 3 + R() * 4; }
    if (R() < 0.4) el("ellipse", { cx: x + 0.36, cy: horizon + 1 + R() * 9, rx: 0.12, ry: 0.22, fill: "none", stroke: "#6e3a1a", "stroke-width": 0.04 }, ins);
  }
  halftone(ins, { x0: 0.4, y0: horizon, x1: 18.6, y1: wallY, pitch: 0.2, angle: 45, fn: (x, y) => 0.5 - (y - horizon) / 14 + Math.abs(x - 10) / 30, fill: "#6e3818", opacity: 0.55 });
  // poker table painted under the bumpers (green felt, cards and chips)
  el("ellipse", { cx: 12.6, cy: 10.6, rx: 3.9, ry: 3.25, fill: "#7a3d1e", stroke: INK, "stroke-width": IW }, art);
  el("ellipse", { cx: 12.6, cy: 10.6, rx: 3.55, ry: 2.9, fill: "#3f7a46", stroke: INK, "stroke-width": IW * 0.8 }, art);
  halftone(art, { x0: 8.6, y0: 7.5, x1: 16.6, y1: 13.7, pitch: 0.18, angle: 45, fn: (x, y) => (Math.hypot((x - 12.6) / 3.55, (y - 10.6) / 2.9) - 0.6) * 1.6, fill: "#2a5631", clip: clip(`M${12.6 - 3.55},10.6a3.55,2.9 0 1,0 7.1,0a3.55,2.9 0 1,0 -7.1,0Z`) });
  for (const [x, y, r, c] of [[9.6, 11.9, -20, C.cream], [9.95, 12.1, 10, C.cream], [15.6, 11.8, 25, C.cream]]) {
    const gg = el("g", { transform: `translate(${x},${y}) rotate(${r})` }, art);
    el("rect", { x: -0.28, y: -0.4, width: 0.56, height: 0.8, rx: 0.06, fill: c, stroke: INK, "stroke-width": 0.04 }, gg);
    P2(gg, "M0,-0.18 l0.14,0.18 l-0.14,0.18 l-0.14,-0.18z", C.red, "none", 0);
  }
  for (const [x, y, c] of [[11.0, 12.9, C.red], [11.2, 13.05, C.cream], [14.6, 12.7, C.red], [14.9, 12.85, "#2b4f7a"]]) { el("ellipse", { cx: x, cy: y, rx: 0.26, ry: 0.22, fill: c, stroke: INK, "stroke-width": 0.04 }, art); el("ellipse", { cx: x, cy: y, rx: 0.16, ry: 0.13, fill: "none", stroke: C.cream, "stroke-width": 0.03, "stroke-dasharray": "0.05 0.05" }, art); }

  // ------------------------------------------------ the saloon's front wall and the boardwalk outside
  const bwY0 = wallY + 0.45, bwY1 = wallY + 1.75;
  const walk = el("g", {}, art);
  P2(walk, `M2.2,${bwY0}H16.5V${bwY1}H2.2Z`, "#c58448", INK, IW);
  for (let i = 1; i < 3; i++) P2(walk, `M2.2,${bwY0 + i * 0.43}H16.5`, "none", INK, 0.04);
  for (let i = 0; i < 9; i++) P2(walk, `M${2.6 + i * 1.6},${bwY0}v0.43 M${3.4 + i * 1.6},${bwY0 + 0.43}v0.43 M${2.9 + i * 1.6},${bwY0 + 0.86}v0.44`, "none", INK, 0.04);
  el("rect", { x: 2.2, y: bwY1 - 0.18, width: 14.3, height: 0.18, fill: "#8f5228" }, walk);
  // wall (seen from above): thick dark beam with the doorway gap at the centre lane
  for (const [x0, x1] of [[2.2, G.lane.x0 - 0.1], [G.lane.x1 + 0.1, 16.5]]) {
    P2(art, `M${x0},${wallY}H${x1}V${bwY0}H${x0}Z`, "#4a2615", INK, IW);
    P2(art, `M${x0},${wallY + 0.12}H${x1}`, "none", "#7a4220", 0.06);
  }

  // ------------------------------------------------ boardwalk inlanes / outlanes
  const plankStrip = (x0, x1, y0, y1) => {
    P2(art, `M${x0},${y0}H${x1}V${y1}H${x0}Z`, "#b9773f", INK, IW);
    for (let y = y0 + 0.55; y < y1; y += 0.55) P2(art, `M${x0},${y}H${x1}`, "none", INK, 0.035);
    el("rect", { x: x1 - 0.25, y: y0, width: 0.25, height: y1 - y0, fill: "#8f5228", opacity: 0.6 }, art);
  };
  plankStrip(0.3, 1.74, 29.8, 40.4);
  plankStrip(17.26, 18.7, 29.8, 40.4);
  // left orbit lane floor
  P2(art, `M0.35,${horizon + 0.3}H1.97V25.9H0.35Z`, "#9a5a2c", INK, IW);
  for (let y = horizon + 0.9; y < 25.9; y += 0.6) P2(art, `M0.35,${y}H1.97`, "none", INK, 0.035);
  // slingshot beds: sand with dots
  for (const s of [G.slingL, G.slingR]) {
    P2(art, poly(s), "#f0bf72", INK, IW);
  }
  // right shooter lane floor: planks
  P2(art, `M18.85,9.6H20.0V42H18.85Z`, "#8f5228", INK, IW);

  // ------------------------------------------------ inserts
  const lit = [];
  function insert(x, y, { name, r = 0.36, color = C.orange, on = false, shape = "circle", rot = 0, label, lsize = 0.15, icon } = {}) {
    if (!INSERTS.some((i) => i.name === name)) INSERTS.push({ name, x, y, r });
    if (allOff) on = false;
    if (only !== undefined) on = name === only;
    const g = el("g", { transform: `translate(${x},${y}) rotate(${rot})`, visibility: only === name ? "visible" : null }, art);
    let d;
    if (shape === "circle") d = `M${-r},0a${r},${r} 0 1,0 ${2 * r},0a${r},${r} 0 1,0 ${-2 * r},0Z`;
    else if (shape === "arrow") d = `M0,${-r * 1.35}L${r * 1.05},${r * 0.1}L${r * 0.45},${r * 0.1}L${r * 0.45},${r * 1.15}L${-r * 0.45},${r * 1.15}L${-r * 0.45},${r * 0.1}L${-r * 1.05},${r * 0.1}Z`;
    else if (shape === "oval") d = `M${-r * 1.6},0a${r * 1.6},${r} 0 1,0 ${3.2 * r},0a${r * 1.6},${r} 0 1,0 ${-3.2 * r},0Z`;
    else if (shape === "rect") d = `M${-r * 1.7},${-r * 0.7}H${r * 1.7}V${r * 0.7}H${-r * 1.7}Z`;
    else if (shape === "shoe") d = `M${-r},${r}L${-r},0A${r},${r} 0 1,1 ${r},0L${r},${r}L${r * 0.45},${r}L${r * 0.45},0A${r * 0.45},${r * 0.45} 0 1,0 ${-r * 0.45},0L${-r * 0.45},${r}Z`;
    P2(g, d, INK, INK, IW * 3.2);
    const body = on ? radGrad(defs, [[0, "#fffbe6"], [0.35, mix(color, "#ffffff", 0.5)], [1, color]], { cx: 0.5, cy: 0.5, r: 0.6 }) : shade(color, 0.6);
    P2(g, d, body, "none", 0);
    // hard cel highlight on the plastic
    P2(g, d, "none", on ? "#ffffff" : mix(color, "#ffffff", 0.3), r * 0.1, { opacity: on ? 0.9 : 0.5, transform: `translate(${-r * 0.08},${-r * 0.1}) scale(0.72)` });
    if (icon) icon(g, r, on);
    if (label) {
      const lines = label.split("\n");
      lines.forEach((ln, i) => txt(ln, { x: 0, y: (i - (lines.length - 1) / 2) * lsize * 1.1 + lsize * 0.36, "font-family": "Superclarendon", "font-weight": 900, "font-size": lsize, "text-anchor": "middle", fill: on ? shade(color, 0.35) : shade(color, 0.25) }, g));
    }
    if (on) lit.push({ x, y, r, color });
    return g;
  }

  // the sheriff's star: six modes and the showdown in the middle
  {
    const [sx, sy] = G.star;
    const g = el("g", {}, art);
    el("circle", { cx: sx, cy: sy, r: 3.3, fill: "#cf2e25", stroke: INK, "stroke-width": IW }, g);
    halftone(g, { x0: sx - 3.4, y0: sy - 3.4, x1: sx + 3.4, y1: sy + 3.4, pitch: 0.2, angle: 45, fn: (x, y) => ((x - sx) + (y - sy)) / 6 + 0.25, fill: "#951c18", clip: clip(`M${sx - 3.3},${sy}a3.3,3.3 0 1,0 6.6,0a3.3,3.3 0 1,0 -6.6,0Z`) });
    for (let i = 0; i < 24; i++) { const a = (i / 24) * Math.PI * 2; el("circle", { cx: sx + 3.05 * Math.cos(a), cy: sy + 3.05 * Math.sin(a), r: 0.07, fill: C.sunL }, g); }
    star6(g, sx, sy, 2.55, { sw: IW, fill: C.gold, shade: C.goldD });
    const modes = ["BRAWL", "DUEL", "BULL\nRIDE", "GOLD\nRUSH", "STAGE\nCOACH", "JAIL\nBREAK"];
    const done = [true, true, false, false, true, false];
    for (let i = 0; i < 6; i++) {
      const a = -Math.PI / 2 + (i * Math.PI) / 3;
      insert(sx + 1.62 * Math.cos(a), sy + 1.62 * Math.sin(a), { r: 0.43, color: [C.red, C.orange, C.red, C.orange, C.red, C.orange][i], on: done[i], label: modes[i], lsize: 0.13, name: `mode-${i}` });
    }
    insert(sx, sy, { r: 0.62, color: C.sun, on: false, label: "SHOW\nDOWN", lsize: 0.17, name: "showdown" });
  }
  // rank ladder (left) and bounty ladder (right)
  const ranks = ["MARSHAL", "SHERIFF", "DEPUTY", "COWHAND", "GREENHORN"];
  ranks.forEach((r, i) => {
    const x = 3.55 + i * 0.12, y = 22.6 + i * 1.08;
    insert(x, y, { shape: "rect", r: 0.33, color: i >= 2 ? C.sun : C.orange, on: i >= 2, label: r, lsize: 0.115, name: `rank-${4 - i}` });
  });
  star6(art, 3.15, 21.55, 0.32, { sw: 0.04 });
  const bounty = ["$10K", "$5000", "$2500", "$1000", "$500"];
  bounty.forEach((b, i) => {
    const x = 15.45 - i * 0.12, y = 22.6 + i * 1.08;
    insert(x, y, { name: `bounty-${4 - i}`, r: 0.37, color: C.sun, on: i >= 3, label: b, lsize: 0.15, icon: (g, r) => el("circle", { cx: 0, cy: 0, r: r * 0.78, fill: "none", stroke: C.goldD, "stroke-width": 0.04 }, g) });
  });
  // painted details: horseshoe prints, a hitching post, water trough
  for (const [x, y, a] of [[6.0, 30.6, -10], [6.5, 32.0, 10], [12.8, 31.0, 15], [12.3, 32.4, -5]]) P2(art, `M-0.16,0.16 L-0.16,0 A0.16,0.16 0 1,1 0.16,0 L0.16,0.16`, "none", "#9a5420", 0.07, { transform: `translate(${x},${y}) rotate(${a})` });
  // top lane horseshoes
  for (let i = 0; i < 3; i++) insert((G.guides[i] + G.guides[i + 1]) / 2, 6.45, { shape: "shoe", r: 0.4, color: C.sun, on: i !== 1, name: `lane-${i}` });
  // shot arrows
  insert(1.16, 27.3, { name: "orbit-arrow", shape: "arrow", r: 0.4, color: C.red, on: true });
  insert(1.16, 28.7, { name: "orbit-light", shape: "circle", r: 0.24, color: C.sun, on: false });
  G.posters.forEach(([x, y], i) => insert(x + 0.5 * 1.15, y + 0.866 * 1.15, { r: 0.2, color: C.sun, on: i === 0, name: `poster-light-${i}` }));
  insert(6.35, 22.75, { name: "ramp-arrow", shape: "arrow", r: 0.44, color: C.orange, on: true });
  insert(6.35, 24.2, { name: "lock", shape: "rect", r: 0.3, color: C.sun, on: true, label: "LOCK", lsize: 0.2 });
  insert(9.6, 20.05, { name: "doors-arrow", shape: "arrow", r: 0.42, color: C.red, on: false });
  insert(9.6, 18.95, { name: "extra-ball", shape: "rect", r: 0.3, color: C.sun, on: false, label: "EXTRA\nBALL", lsize: 0.13 });
  insert(12.6, 18.5, { name: "bottles-arrow", shape: "arrow", r: 0.42, color: C.orange, on: false, rot: -14 });
  insert(15.05, 17.25, { name: "scoop-arrow", shape: "arrow", r: 0.4, color: C.red, on: true, rot: 22 });
  insert(17.35, 24.3, { name: "bull-arrow", shape: "arrow", r: 0.4, color: C.orange, on: false, rot: 6 });
  insert(17.45, 25.75, { name: "jackpot", shape: "rect", r: 0.28, color: C.sun, on: true, label: "JACKPOT", lsize: 0.13, rot: 6 });
  // inlanes / outlanes
  insert(1.02, 36.3, { name: "kickback", shape: "oval", r: 0.3, color: C.red, on: false, rot: 90, label: "KICK\nBACK", lsize: 0.12 });
  insert(17.98, 36.3, { name: "special", shape: "oval", r: 0.3, color: C.red, on: true, rot: -90, label: "SPECIAL", lsize: 0.12 });
  insert(2.85, 35.7, { name: "inlane-left", shape: "arrow", r: 0.28, color: C.sun, on: true, rot: 150 });
  insert(16.15, 35.7, { name: "inlane-right", shape: "arrow", r: 0.28, color: C.sun, on: true, rot: -150 });
  // shoot again between flippers
  insert(9.5, 39.15, { name: "shoot-again", shape: "shoe", r: 0.42, color: C.red, on: false });
  // bumper lights by the poker table
  insert(9.15, 9.2, { name: "bumper-light-0", r: 0.2, color: C.sun, on: true });
  insert(16.2, 8.7, { name: "bumper-light-1", r: 0.2, color: C.sun, on: true });
  // scoop hole in front of the piano
  el("circle", { cx: G.scoop[0], cy: G.scoop[1], r: 0.5, fill: "#120805" }, art);
  el("circle", { cx: G.scoop[0] + 0.12, cy: G.scoop[1] + 0.12, r: 0.3, fill: "#000" }, art);
  // six-shooter cylinder: ball-save chambers above the drain
  {
    const [rx, ry] = [9.5, 33.9];
    el("circle", { cx: rx, cy: ry, r: 1.35, fill: "#8d857b", stroke: INK, "stroke-width": IW }, art);
    el("path", { d: `M${rx},${ry - 1.35}A1.35,1.35 0 0,1 ${rx + 1.35},${ry}A1.35,1.35 0 0,1 ${rx},${ry + 1.35}Z`, fill: "#6b645c" }, art);
    for (let i = 0; i < 6; i++) { const a = -Math.PI / 2 + (i * Math.PI) / 3; P2(art, `M${rx + 1.28 * Math.cos(a + Math.PI / 6)},${ry + 1.28 * Math.sin(a + Math.PI / 6)}L${rx + 0.95 * Math.cos(a + Math.PI / 6)},${ry + 0.95 * Math.sin(a + Math.PI / 6)}`, "none", INK, 0.06); }
    el("circle", { cx: rx, cy: ry, r: 1.35, fill: "none", stroke: INK, "stroke-width": IW }, art);
    for (let i = 0; i < 6; i++) { const a = -Math.PI / 2 + (i * Math.PI) / 3; insert(rx + 0.78 * Math.cos(a), ry + 0.78 * Math.sin(a), { name: `chamber-${i}`, r: 0.27, color: C.sun, on: i < 4, icon: (g, r) => el("circle", { cx: 0, cy: 0, r: r * 0.45, fill: "none", stroke: C.goldD, "stroke-width": 0.04 }, g) }); }
    el("circle", { cx: rx, cy: ry, r: 0.22, fill: "#4a443e", stroke: INK, "stroke-width": 0.05 }, art);
  }
  // outhole under the apron
  el("rect", { x: 0.3, y: 40.05, width: 18.4, height: 2.4, fill: "#2a160d" }, art);

  // texture
  if (only === undefined) el("rect", { x: 0, y: 0, width: W, height: L, filter: "url(#grain)", style: "mix-blend-mode:multiply", opacity: 0.45 }, fs);
  return { lit };
}

export function drawFloorLight(fs, defs, { C, W, L, lit, gi, onlyLamps = false }) {
  const g = el("g", {}, fs);
  if (!onlyLamps) el("rect", { x: 0, y: 0, width: W, height: L, fill: radGrad(defs, [[0, "#000", 0], [0.6, "#000", 0.03], [1, "#2a0e04", 0.35]], { cx: 0.5, cy: 0.55, r: 0.72 }), style: "mix-blend-mode:multiply" }, g);
  if (!onlyLamps) for (const [x, y, r, k] of gi) el("circle", { cx: x, cy: y, r, fill: radGrad(defs, [[0, "#ffd88a", 0.5 * k], [0.5, "#ffb84a", 0.18 * k], [1, "#ffb84a", 0]]), style: "mix-blend-mode:screen" }, g);
  for (const { x, y, r, color } of lit) el("circle", { cx: x, cy: y, r: r * 3.0, fill: radGrad(defs, [[0, "#fff4cc", 0.85], [0.3, color, 0.55], [1, color, 0]]), style: "mix-blend-mode:screen" }, g);
}
