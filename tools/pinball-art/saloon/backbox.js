// Saloon backbox: a false-front facade with the sign, the High Noon backglass, a DMD and the rank lamps.
import { el, txt, id, poly, smooth, halftone, linGrad, radGrad, rng, f2 } from "./lib.js";
import { C, cel, star6, sheriff, outlaw, cactus, mechBull } from "./chars.js";

const FONT = {
  "A": ".###.#...##...#######...##...##...#",
  "B": "####.#...##...#####.#...##...#####.",
  "C": ".###.#...##....#....#....#...#.###.",
  "D": "####.#...##...##...##...##...#####.",
  "E": "######....#....####.#....#....#####",
  "F": "######....#....####.#....#....#....",
  "G": ".###.#...##....#.####...##...#.####",
  "H": "#...##...##...#######...##...##...#",
  "I": ".###...#....#....#....#....#...###.",
  "J": "..###...#....#....#....#.#..#..##..",
  "K": "#...##..#.#.#..##...#.#..#..#.#...#",
  "L": "#....#....#....#....#....#....#####",
  "M": "#...###.###.#.##.#.##...##...##...#",
  "N": "#...##...###..##.#.##..###...##...#",
  "O": ".###.#...##...##...##...##...#.###.",
  "P": "####.#...##...#####.#....#....#....",
  "Q": ".###.#...##...##...##.#.##..#..##.#",
  "R": "####.#...##...#####.#.#..#..#.#...#",
  "S": ".#####....#.....###.....#....#####.",
  "T": "#####..#....#....#....#....#....#..",
  "U": "#...##...##...##...##...##...#.###.",
  "V": "#...##...##...##...##...#.#.#...#..",
  "W": "#...##...##...##.#.##.#.##.#.#.#.#.",
  "X": "#...##...#.#.#...#...#.#.#...##...#",
  "Y": "#...##...#.#.#...#....#....#....#..",
  "Z": "#####....#...#...#...#...#....#####",
  "0": ".###.#...##..###.#.###..##...#.###.",
  "1": "..#...##....#....#....#....#...###.",
  "2": ".###.#...#....#...#...#...#...#####",
  "3": "#####...#...#.....#.....##...#.###.",
  "4": "...#...##..#.#.#..#.#####...#....#.",
  "5": "######....####.....#....##...#.###.",
  "6": "..##..#...#....####.#...##...#.###.",
  "7": "#####....#...#...#...#....#....#...",
  "8": ".###.#...##...#.###.#...##...#.###.",
  "9": ".###.#...##...#.####....#...#..##..",
  ",": "..........................##....#..",
  ":": "......##...##........##...##.......",
  "/": ".........#...#...#...#...#.........",
  "-": "...............#####...............",
  " ": "...................................",
  ".": "..........................##...##..",
};

function glyphCols(ch) {
  const g = FONT[ch] || FONT[" "];
  const rows = [];
  for (let r = 0; r < 7; r++) rows.push(g.slice(r * 5, r * 5 + 5).padEnd(5, "."));
  let c0 = 0, c1 = 4;
  if (ch === " ") return { rows, c0: 0, c1: 2 };
  if (",:.".includes(ch)) { while (c0 < 4 && rows.every((r) => r[c0] === ".")) c0++; while (c1 > c0 && rows.every((r) => r[c1] === ".")) c1--; }
  return { rows, c0, c1 };
}
function textWidth(s, scale = 1) { let w = 0; for (const ch of s) { const { c0, c1 } = glyphCols(ch); w += (c1 - c0 + 1 + 1) * scale; } return w - scale; }
function blit(grid, s, x, y, scale = 1) {
  for (const ch of s) {
    const { rows, c0, c1 } = glyphCols(ch);
    for (let r = 0; r < 7; r++) for (let c = c0; c <= c1; c++) if (rows[r][c] === "#")
      for (let a = 0; a < scale; a++) for (let b = 0; b < scale; b++) { const gx = x + (c - c0) * scale + b, gy = y + r * scale + a; if (gx >= 0 && gx < 128 && gy >= 0 && gy < 32) grid[gy][gx] = 1; }
    x += (c1 - c0 + 2) * scale;
  }
}

// `dark`: the dot-matrix display with every dot off and the rank lamps unlit (the game lights
// them). `onlyRank`: just that rank lamp, lit. Returns where the display's dots are.
export function drawBackbox(svg, { C: _C, mix, shade, box, dark = false, onlyRank }) {
  // Drawing just one lamp: everything else is drawn hidden.
  if (onlyRank !== undefined) { dark = true; svg = el("g", { visibility: "hidden" }, svg); }
  const defs = el("defs", {}, svg);
  const INK = C.ink;
  const P2 = (g, d, fill, stroke = INK, sw = 3, extra = {}) => el("path", { d, fill, stroke, "stroke-width": sw, "stroke-linejoin": "round", "stroke-linecap": "round", ...extra }, g);
  const clip = (d) => { const cid = id("bx"); el("path", { d }, el("clipPath", { id: cid }, defs)); return cid; };
  {
    const f = el("filter", { id: "dglow", x: "-10%", y: "-30%", width: "120%", height: "160%" }, defs);
    el("feGaussianBlur", { stdDeviation: 3.2 }, f);
    const fs = el("filter", { id: "bsh", x: "-20%", y: "-20%", width: "140%", height: "140%" }, defs);
    el("feGaussianBlur", { stdDeviation: 12 }, fs);
    const fg = el("filter", { id: "bgrain", x: 0, y: 0, width: "100%", height: "100%" }, defs);
    el("feTurbulence", { type: "fractalNoise", baseFrequency: 0.8, numOctaves: 2, seed: 6, result: "n" }, fg);
    el("feColorMatrix", { type: "matrix", values: "0 0 0 0 0.3  0 0 0 0 0.15  0 0 0 0 0.08  0 0 0 -1.2 0.7" }, fg);
  }
  const { x: bx, y: by, w: bw, h: bh } = box;
  const L = bx + 12, R = bx + bw - 12, T = by + 150, B = by + bh;
  const cx = (L + R) / 2;

  // ------------------------------------------------ cabinet body with false front
  const front = [[L, B], [L, T], [L, by + 92], [L + 60, by + 92], [L + 60, by + 52], [cx - 190, by + 52], [cx - 120, by + 14], [cx, by + 2], [cx + 120, by + 14], [cx + 190, by + 52], [R - 60, by + 52], [R - 60, by + 92], [R, by + 92], [R, B]];
  P2(svg, poly(front.map(([x, y]) => [x + 14, y + 16])), "#000", "none", 0, { opacity: 0.55, filter: "url(#bsh)" });
  P2(svg, poly(front), "#8f5228", INK, 4);
  {
    const g = el("g", { "clip-path": `url(#${clip(poly(front))})` }, svg);
    const R_ = rng(4);
    for (let y = by; y < B; y += 34) {
      el("rect", { x: L, y, width: R - L, height: 34, fill: ["#94552a", "#8a4e25", "#9c5c2e"][Math.floor((y - by) / 34) % 3] }, g);
      P2(g, `M${L},${y}H${R}`, "none", INK, 2);
      for (let x = L + 30 + R_() * 120; x < R; x += 160 + R_() * 120) { P2(g, `M${x},${y}V${y + 34}`, "none", INK, 2); el("circle", { cx: x - 8, cy: y + 9, r: 2.2, fill: INK }, g); el("circle", { cx: x - 8, cy: y + 25, r: 2.2, fill: INK }, g); }
      el("rect", { x: L, y: y + 24, width: R - L, height: 10, fill: "#6e3a1b", opacity: 0.45 }, g);
    }
    // cel shadow on the right side of the facade
    el("path", { d: `M${R - 80},${by}L${R},${by}L${R},${B}L${R - 30},${B}Z`, fill: "#3b1d0e", opacity: 0.35 }, g);
  }
  // sign board on the false front
  {
    const sx0 = cx - 330, sx1 = cx + 330, sy0 = by + 40, sy1 = by + 150;
    P2(svg, `M${sx0 + 10},${sy0 + 10}L${sx1 + 10},${sy0 + 10}L${sx1 + 10},${sy1 + 10}L${sx0 + 10},${sy1 + 10}Z`, "#2a1208", "none", 0, { opacity: 0.6 });
    P2(svg, `M${sx0},${sy0}L${sx1},${sy0}L${sx1},${sy1}L${sx0},${sy1}Z`, C.cream, INK, 4);
    halftone(svg, { x0: sx0, y0: sy0, x1: sx1, y1: sy1, pitch: 7, angle: 45, fn: (x, y) => (y - sy0) / (sy1 - sy0) * 0.6 + (x - sx0) / (sx1 - sx0) * 0.3 - 0.1, fill: C.creamD, clip: clip(`M${sx0},${sy0}L${sx1},${sy0}L${sx1},${sy1}L${sx0},${sy1}Z`) });
    P2(svg, `M${sx0 + 10},${sy0 + 10}L${sx1 - 10},${sy0 + 10}L${sx1 - 10},${sy1 - 10}L${sx0 + 10},${sy1 - 10}Z`, "none", C.red, 4);
    const tx = cx, ty = sy1 - 22;
    for (let k = 8; k > 0; k--) txt("SALOON", { x: tx + k * 1.2, y: ty + k * 1.2, "font-family": "Superclarendon", "font-weight": 900, "font-size": 104, "text-anchor": "middle", fill: k === 8 ? INK : C.redD, textLength: 540, lengthAdjust: "spacingAndGlyphs" }, svg);
    txt("SALOON", { x: tx, y: ty, "font-family": "Superclarendon", "font-weight": 900, "font-size": 104, "text-anchor": "middle", fill: C.red, stroke: INK, "stroke-width": 4, "paint-order": "stroke", textLength: 540, lengthAdjust: "spacingAndGlyphs" }, svg);
    // inline highlight
    txt("SALOON", { x: tx - 2, y: ty - 3, "font-family": "Superclarendon", "font-weight": 900, "font-size": 104, "text-anchor": "middle", fill: "none", stroke: "#ff8a6a", "stroke-width": 2, opacity: 0.8, textLength: 540, lengthAdjust: "spacingAndGlyphs" }, svg);
    star6(svg, sx0 - 2, (sy0 + sy1) / 2, 30, { sw: 3 });
    star6(svg, sx1 + 2, (sy0 + sy1) / 2, 30, { sw: 3 });
    for (const x of [sx0 + 30, sx1 - 30]) { el("circle", { cx: x, cy: sy0 + 22, r: 4, fill: C.steelD, stroke: INK, "stroke-width": 1.5 }, svg); }
  }

  // ------------------------------------------------ backglass: High Noon
  const gx = L + 28, gy = T + 20, gw = R - L - 56, gh = 488;
  P2(svg, `M${gx - 16},${gy - 16}h${gw + 32}v${gh + 32}h${-gw - 32}Z`, "#5e3418", INK, 4);
  P2(svg, `M${gx - 6},${gy - 6}h${gw + 12}v${gh + 12}h${-gw - 12}Z`, "#c98a4a", INK, 3);
  const art = el("g", { "clip-path": `url(#${clip(`M${gx},${gy}h${gw}v${gh}h${-gw}Z`)})` }, svg);
  const horizon = gy + 330;
  // sky in hard bands with Ben-Day dots
  const bands = ["#a8321a", "#c9431d", "#e2612a", "#f08a2c", "#f6aa3c", "#ffc94f"];
  bands.forEach((c, i) => el("rect", { x: gx, y: gy + i * 55 - 5, width: gw, height: 400, fill: c }, art));
  halftone(art, { x0: gx, y0: gy, x1: gx + gw, y1: horizon, pitch: 8, angle: 45, fn: (x, y) => 0.45 - (y - gy) / 700, fill: "#7a1f12", opacity: 0.55 });
  // sun with stripes
  const sun = [cx, horizon - 30];
  for (let i = 0; i < 24; i++) { if (i % 2) continue; const a0 = Math.PI + (i / 24) * Math.PI, a1 = a0 + Math.PI / 24; P2(art, `M${sun[0]},${sun[1]}L${sun[0] + 700 * Math.cos(a0)},${sun[1] + 700 * Math.sin(a0)}L${sun[0] + 700 * Math.cos(a1)},${sun[1] + 700 * Math.sin(a1)}Z`, "#ffe08a", "none", 0, { opacity: 0.22 }); }
  el("circle", { cx: sun[0], cy: sun[1], r: 165, fill: "#ffe7a0", stroke: INK, "stroke-width": 3.5 }, art);
  for (let k = 0; k < 4; k++) el("rect", { x: sun[0] - 170, y: sun[1] + 40 + k * 26, width: 340, height: 9 + k * 2, fill: "#ffc94f" }, art);
  // mesas
  P2(art, `M${gx},${horizon}L${gx},${horizon - 120}L${gx + 70},${horizon - 125}L${gx + 90},${horizon - 165}L${gx + 230},${horizon - 168}L${gx + 255},${horizon - 110}L${gx + 300},${horizon - 95}L${gx + 330},${horizon}Z`, "#8a3f3a", INK, 3);
  P2(art, `M${gx + 190},${horizon - 168}L${gx + 230},${horizon - 168}L${gx + 255},${horizon - 110}L${gx + 300},${horizon - 95}L${gx + 330},${horizon}L${gx + 200},${horizon}Z`, "#6a2c33", "none", 0);
  P2(art, `M${gx + gw},${horizon}L${gx + gw},${horizon - 100}L${gx + gw - 60},${horizon - 104}L${gx + gw - 80},${horizon - 150}L${gx + gw - 200},${horizon - 152}L${gx + gw - 225},${horizon - 90}L${gx + gw - 280},${horizon - 70}L${gx + gw - 300},${horizon}Z`, "#8a3f3a", INK, 3);
  P2(art, `M${gx + gw - 140},${horizon - 152}L${gx + gw - 80},${horizon - 150}L${gx + gw - 60},${horizon - 104}L${gx + gw},${horizon - 100}L${gx + gw},${horizon}L${gx + gw - 150},${horizon}Z`, "#6a2c33", "none", 0);
  // gold mine on the left mesa with a cart
  {
    const mx = gx + 160, my = horizon - 168;
    P2(art, `M${mx - 26},${my}V${my - 34}H${mx + 26}V${my}`, "#120905", INK, 3);
    P2(art, `M${mx - 34},${my - 34}H${mx + 34}V${my - 44}H${mx - 34}Z`, C.brownL, INK, 2.5);
    for (const x of [mx - 30, mx + 30]) P2(art, `M${x - 4},${my}V${my - 36}H${x + 4}V${my}Z`, C.brownL, INK, 2.5);
    P2(art, `M${mx + 40},${my - 4}L${mx + 72},${my - 4}L${mx + 68},${my - 22}L${mx + 44},${my - 22}Z`, "#b5502a", INK, 2.5);
    for (const x of [mx + 50, mx + 62]) el("circle", { cx: x, cy: my - 2, r: 4, fill: INK }, art);
    for (let i = 0; i < 4; i++) el("circle", { cx: mx + 48 + i * 6, cy: my - 25, r: 4, fill: C.gold, stroke: INK, "stroke-width": 1.2 }, art);
  }
  // the saloon building (centre background)
  {
    const sx0 = cx - 150, sx1 = cx + 150, sTop = horizon - 210, sBot = horizon + 40;
    P2(art, `M${sx0},${sBot}V${sTop + 40}H${sx0 + 30}V${sTop}H${sx1 - 30}V${sTop + 40}H${sx1}V${sBot}Z`, "#b8703f", INK, 3.5);
    const bc = clip(`M${sx0},${sBot}V${sTop + 40}H${sx0 + 30}V${sTop}H${sx1 - 30}V${sTop + 40}H${sx1}V${sBot}Z`);
    const bg_ = el("g", { "clip-path": `url(#${bc})` }, art);
    for (let y = sTop; y < sBot; y += 18) P2(bg_, `M${sx0},${y}H${sx1}`, "none", "#7a4220", 2);
    el("rect", { x: sx1 - 70, y: sTop, width: 70, height: sBot - sTop, fill: "#6e3818", opacity: 0.45 }, bg_);
    // balcony
    P2(art, `M${sx0 - 14},${sTop + 110}H${sx1 + 14}V${sTop + 122}H${sx0 - 14}Z`, C.brownD, INK, 2.5);
    for (let x = sx0 - 8; x < sx1 + 10; x += 14) P2(art, `M${x},${sTop + 92}V${sTop + 110}`, "none", INK, 2.2);
    P2(art, `M${sx0 - 14},${sTop + 90}H${sx1 + 14}`, "none", INK, 3);
    // windows lit
    for (const x of [sx0 + 30, cx - 22, sx1 - 74]) { P2(art, `M${x},${sTop + 50}h44v38h-44Z`, "#ffd56a", INK, 2.5); P2(art, `M${x + 22},${sTop + 50}v38M${x},${sTop + 69}h44`, "none", INK, 2); }
    // sign with a mug
    P2(art, `M${cx - 50},${sTop + 6}h100v30h-100Z`, C.cream, INK, 2.5);
    P2(art, `M${cx - 12},${sTop + 12}h18v20h-18Z`, C.sun, INK, 2);
    P2(art, `M${cx + 6},${sTop + 16}q8,0 8,6q0,6 -8,6`, "none", INK, 2);
    el("rect", { x: cx - 14, y: sTop + 9, width: 22, height: 6, rx: 3, fill: "#fff", stroke: INK, "stroke-width": 1.5 }, art);
    // doorway with batwings
    P2(art, `M${cx - 34},${sBot}V${sTop + 150}H${cx + 34}V${sBot}Z`, "#2a1208", INK, 3);
    for (const [x, d] of [[cx - 32, 1], [cx + 32, -1]]) P2(art, `M${x},${sTop + 175}l${30 * d},4l0,44l${-30 * d},0Z`, "#c97a3a", INK, 2.5);
    // porch posts
    for (const x of [sx0 + 8, sx1 - 8]) P2(art, `M${x - 5},${sBot}V${sTop + 122}H${x + 5}V${sBot}Z`, C.brownL, INK, 2.5);
  }
  // street
  P2(art, `M${gx},${horizon}H${gx + gw}V${gy + gh}H${gx}Z`, "#e8a85c", "none", 0);
  halftone(art, { x0: gx, y0: horizon, x1: gx + gw, y1: gy + gh, pitch: 7, angle: 20, fn: (x, y) => 0.15 + (y - horizon) / 400, fill: "#c97e3a" });
  P2(art, `M${gx},${horizon}H${gx + gw}`, "none", INK, 3);
  for (const k of [-1, 1]) P2(art, `M${cx + k * 30},${horizon + 40}L${cx + k * 220},${gy + gh}`, "none", "#b86e30", 4);
  // long shadows of the two gunfighters
  P2(art, `M${gx + 150},${gy + gh - 12}L${gx + 340},${gy + gh + 30}L${gx + 380},${gy + gh + 30}L${gx + 190},${gy + gh - 22}Z`, "#6a2c33", "none", 0, { opacity: 0.55 });
  P2(art, `M${gx + gw - 150},${gy + gh - 12}L${gx + gw - 340},${gy + gh + 30}L${gx + gw - 380},${gy + gh + 30}L${gx + gw - 190},${gy + gh - 22}Z`, "#6a2c33", "none", 0, { opacity: 0.55 });
  // cacti and a tumbleweed
  cactus(el("g", { transform: `translate(${gx + 60},${horizon + 70}) scale(42)` }, art), { h: 3, sw: 0.08 });
  cactus(el("g", { transform: `translate(${gx + gw - 52},${horizon + 60}) scale(-36,36)` }, art), { h: 3, sw: 0.09 });
  {
    const tw = [cx + 10, gy + gh - 52];
    el("circle", { cx: tw[0], cy: tw[1], r: 26, fill: "#c98a4a", opacity: 0.5 }, art);
    for (let k = 0; k < 9; k++) P2(art, `M${tw[0] + 24 * Math.cos(k)},${tw[1] + 24 * Math.sin(k * 1.7)} Q${tw[0] + 22 * Math.sin(k * 2.1)},${tw[1] - 22 * Math.cos(k)} ${tw[0] - 24 * Math.cos(k * 1.3)},${tw[1] + 22 * Math.sin(k)}`, "none", "#6e3a1b", 2.4);
    for (const dy of [-10, 6]) P2(art, `M${tw[0] - 46},${tw[1] + dy} h-26`, "none", INK, 2.5);
  }
  // the duel: sheriff (left) and outlaw (right)
  sheriff(el("g", { transform: `translate(${gx + 170},${gy + gh - 8}) scale(33)` }, art), { sw: 0.12 });
  outlaw(el("g", { transform: `translate(${gx + gw - 170},${gy + gh - 8}) scale(33)` }, art), { sw: 0.12 });
  // tension marks between the hands
  for (const [x, y, dx, dy] of [[gx + 270, gy + gh - 190, 18, -10], [gx + 274, gy + gh - 168, 22, 0], [gx + gw - 270, gy + gh - 190, -18, -10], [gx + gw - 274, gy + gh - 168, -22, 0]]) P2(art, `M${x},${y}l${dx},${dy}`, "none", INK, 3);
  // sparkle on the sheriff's badge
  {
    const sx = gx + 170 - 0.72 * 33, sy = gy + gh - 8 - 7.15 * 33;
    P2(art, `M${sx},${sy - 22}L${sx + 4},${sy - 4}L${sx + 22},${sy}L${sx + 4},${sy + 4}L${sx},${sy + 22}L${sx - 4},${sy + 4}L${sx - 22},${sy}L${sx - 4},${sy - 4}Z`, "#fffbe0", INK, 1.8);
  }
  el("rect", { x: gx, y: gy, width: gw, height: gh, filter: "url(#bgrain)", style: "mix-blend-mode:multiply", opacity: 0.5 }, art);
  // glass glare (comic hard-edged)
  P2(art, `M${gx + gw - 210},${gy}L${gx + gw - 150},${gy}L${gx + gw - 330},${gy + gh}L${gx + gw - 390},${gy + gh}Z`, "#ffffff", "none", 0, { opacity: 0.1 });
  P2(art, `M${gx + gw - 130},${gy}L${gx + gw - 110},${gy}L${gx + gw - 290},${gy + gh}L${gx + gw - 310},${gy + gh}Z`, "#ffffff", "none", 0, { opacity: 0.1 });

  // ------------------------------------------------ DMD
  const grid = Array.from({ length: 32 }, () => new Array(128).fill(0));
  if (!dark) {
  blit(grid, "P1", 1, 0);
  blit(grid, "DEPUTY", Math.round(64 - textWidth("DEPUTY") / 2), 0);
  blit(grid, "BALL 2", 127 - textWidth("BALL 2"), 0);
  const score = "2,451,370";
  blit(grid, score, Math.round(64 - textWidth(score, 2) / 2), 9, 2);
  const mission = "ROUND UP THE GANG 2/3";
  blit(grid, mission, Math.round(64 - textWidth(mission) / 2), 25);
  // tiny sheriff stars flanking the score
  for (const sx of [6, 117]) for (const [dx, dy] of [[2, 0], [1, 1], [2, 1], [3, 1], [0, 2], [1, 2], [2, 2], [3, 2], [4, 2], [1, 3], [2, 3], [3, 3], [0, 4], [1, 4], [3, 4], [4, 4]]) grid[13 + dy][sx + dx] = 1;
  }
  const pitch = 4.95, dw = 128 * pitch, dh = 32 * pitch;
  const dx0 = cx - dw / 2, dy0 = gy + gh + 44;
  P2(svg, `M${dx0 - 26},${dy0 - 22}h${dw + 52}v${dh + 44}h${-dw - 52}Z`, "#5e3418", INK, 4);
  for (const [x, y] of [[dx0 - 14, dy0 - 10], [dx0 + dw + 14, dy0 - 10], [dx0 - 14, dy0 + dh + 10], [dx0 + dw + 14, dy0 + dh + 10]]) el("circle", { cx: x, cy: y, r: 3.5, fill: C.gold, stroke: INK, "stroke-width": 1.5 }, svg);
  P2(svg, `M${dx0 - 8},${dy0 - 6}h${dw + 16}v${dh + 12}h${-dw - 16}Z`, "#140803", INK, 3);
  const off = el("g", {}, svg), on = el("g", {}, svg);
  let dOff = "", dOn = "";
  const r0 = pitch * 0.36;
  for (let y = 0; y < 32; y++) for (let x = 0; x < 128; x++) {
    const px = dx0 + (x + 0.5) * pitch, py = dy0 + (y + 0.5) * pitch;
    const d = `M${f2(px - r0)},${f2(py)}a${f2(r0)},${f2(r0)} 0 1,0 ${f2(2 * r0)},0a${f2(r0)},${f2(r0)} 0 1,0 ${f2(-2 * r0)},0`;
    if (grid[y][x]) dOn += d; else dOff += d;
  }
  el("path", { d: dOff, fill: "#3a1a0b" }, off);
  el("path", { d: dOn, fill: "#ff8a1f", filter: "url(#dglow)", opacity: 0.85 }, on);
  el("path", { d: dOn, fill: "#ffb54d" }, on);
  P2(svg, `M${dx0 + dw - 90},${dy0 - 6}L${dx0 + dw - 40},${dy0 - 6}L${dx0 + dw - 120},${dy0 + dh + 6}L${dx0 + dw - 170},${dy0 + dh + 6}Z`, "#fff", "none", 0, { opacity: 0.06 });

  // ------------------------------------------------ rank lamps + wagon-wheel speakers
  const ry = dy0 + dh + 78;
  const ranks = ["GREENHORN", "COWHAND", "DEPUTY", "SHERIFF", "MARSHAL"];
  ranks.forEach((r, i) => {
    const x = cx - 244 + i * 122, lit = onlyRank === i || (!dark && i <= 2);
    const svg_ = svg;
    svg = onlyRank === i ? el("g", { visibility: "visible" }, svg_) : svg_;
    if (lit) el("circle", { cx: x, cy: ry - 6, r: 34, fill: "#ffb84a", opacity: 0.45, filter: "url(#bsh)" }, svg);
    star6(svg, x, ry - 6, 22, { sw: 2.5, fill: lit ? C.sun : "#7a5a2a", shade: lit ? C.gold : "#5e4220" });
    txt(r, { x, y: ry + 34, "font-family": "Superclarendon", "font-weight": 900, "font-size": 13, "text-anchor": "middle", fill: lit ? C.sunL : "#3b1d0e", stroke: INK, "stroke-width": lit ? 3 : 0, "paint-order": "stroke" }, svg);
    svg = svg_;
  });
  for (const wx of [L + 52, R - 52]) {
    const wy = ry + 6, wr = 40;
    el("circle", { cx: wx, cy: wy, r: wr, fill: "#2a1208", stroke: INK, "stroke-width": 3 }, svg);
    for (let i = 0; i < 40; i++) { const a = i * 2.4, rr = Math.sqrt(i / 40) * wr * 0.85; el("circle", { cx: wx + rr * Math.cos(a), cy: wy + rr * Math.sin(a), r: 1.6, fill: "#5e3418" }, svg); }
    for (let k = 0; k < 8; k++) { const a = (k / 8) * Math.PI * 2; P2(svg, `M${wx},${wy}L${wx + wr * Math.cos(a)},${wy + wr * Math.sin(a)}`, "none", INK, 7); P2(svg, `M${wx},${wy}L${wx + wr * Math.cos(a)},${wy + wr * Math.sin(a)}`, "none", C.brownL, 3.5); }
    el("circle", { cx: wx, cy: wy, r: wr, fill: "none", stroke: INK, "stroke-width": 11 }, svg);
    el("circle", { cx: wx, cy: wy, r: wr, fill: "none", stroke: C.brown, "stroke-width": 6 }, svg);
    el("circle", { cx: wx, cy: wy, r: 10, fill: C.steelD, stroke: INK, "stroke-width": 2.5 }, svg);
  }
  return { dmd: { x: dx0, y: dy0, pitch, columns: 128, rows: 32 } };
}
