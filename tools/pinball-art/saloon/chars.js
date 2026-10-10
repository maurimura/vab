// Bold cartoon characters with thick ink and hard cel shading. Local units: feet at (0,0), up is -y.
import { el, id, f2 } from "./lib.js";

export const C = {
  ink: "#24140f", cream: "#f8e7c2", creamD: "#e3c48f",
  orange: "#f08a2c", orangeD: "#c95e1a", sun: "#ffc23f", sunL: "#ffe08a",
  brown: "#8b5a2b", brownD: "#5e3a1b", brownL: "#b07a42", dark: "#3b2219", darkD: "#24140f",
  red: "#cf2e25", redD: "#951c18", skin: "#efb27c", skinD: "#c9834d",
  green: "#6e9a3a", greenD: "#466b23", greenL: "#9cc35a", steel: "#c7c3bb", steelD: "#7d776f",
  gold: "#f2b62e", goldD: "#b9801a", denim: "#5b6f8a", denimD: "#3e4e66", plum: "#5a2f3e",
};

// Draws a shape with a hard cel shadow (clipped to the shape) and an ink outline on top.
export function cel(g, d, fill, shadow, shade, { sw = 0.12, hi, hiFill = "#fff6dc", hiOp = 0.85 } = {}) {
  el("path", { d, fill }, g);
  if (shadow || hi) {
    const cid = id("cc");
    const cp = el("clipPath", { id: cid }, g);
    el("path", { d }, cp);
    const cg = el("g", { "clip-path": `url(#${cid})` }, g);
    if (shadow) el("path", { d: shadow, fill: shade }, cg);
    if (hi) el("path", { d: hi, fill: hiFill, opacity: hiOp }, cg);
  }
  el("path", { d, fill: "none", stroke: C.ink, "stroke-width": sw, "stroke-linejoin": "round", "stroke-linecap": "round" }, g);
}
const line = (g, d, sw = 0.08, col = C.ink, extra = {}) => el("path", { d, fill: "none", stroke: col, "stroke-width": sw, "stroke-linecap": "round", "stroke-linejoin": "round", ...extra }, g);

export function star6(g, x, y, r, { fill = C.gold, shade = C.goldD, sw = 0.08 } = {}) {
  let d = "";
  for (let i = 0; i < 12; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 6, rr = i % 2 ? r * 0.52 : r;
    d += (i ? "L" : "M") + f2(x + rr * Math.cos(a)) + "," + f2(y + rr * Math.sin(a));
  }
  d += "Z";
  cel(g, d, fill, `M${x},${y - r * 2}H${x + r * 2}V${y + r * 2}H${x}Z`, shade, { sw });
  for (let i = 0; i < 6; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 3;
    el("circle", { cx: x + r * 1.02 * Math.cos(a), cy: y + r * 1.02 * Math.sin(a), r: r * 0.17, fill, stroke: C.ink, "stroke-width": sw * 0.8 }, g);
  }
  el("circle", { cx: x, cy: y, r: r * 0.22, fill: "none", stroke: C.ink, "stroke-width": sw * 0.7 }, g);
}

// The sheriff: tall, white hat, droopy mustache, star on the vest, hand over the holster. Faces right.
export function sheriff(g, { sw = 0.13 } = {}) {
  // boots
  cel(g, "M-1.45,-0.05 L-1.4,-0.5 C-1.3,-0.75 -0.95,-0.8 -0.75,-0.9 L-0.7,-1.7 L0.0,-1.7 L-0.02,-0.15 L0.08,0 Z", C.brownD, "M-0.35,-1.7 L0,-1.7 L-0.02,0 L-0.35,0 Z", C.darkD, { sw });
  cel(g, "M0.75,-1.7 L1.4,-1.7 L1.45,-0.85 C1.75,-0.7 2.15,-0.6 2.25,-0.35 L2.25,-0.05 L0.75,0 Z", C.brownD, "M1.1,-1.7 L1.45,-1.7 L1.45,0 L1.1,0 Z", C.darkD, { sw });
  for (const x of [-0.05, 0.72]) { el("circle", { cx: x, cy: -0.35, r: 0.18, fill: C.steel, stroke: C.ink, "stroke-width": sw * 0.6 }, g); }
  // trousers (A stance)
  cel(g, "M-0.95,-5.15 L0.95,-5.15 L1.5,-1.6 L0.7,-1.6 L0.15,-3.9 L-0.05,-3.9 L-0.05,-1.6 L-0.8,-1.6 Z", C.denim, "M0.3,-5.15 L0.95,-5.15 L1.5,-1.6 L1.05,-1.6 Z M-0.35,-4.2 L-0.05,-3.9 L-0.05,-1.6 L-0.35,-1.6Z", C.denimD, { sw });
  line(g, "M0.05,-4.9 L0.08,-3.95", sw * 0.6);
  // gun belt, buckle, holster with revolver
  cel(g, "M-1.0,-5.45 L1.0,-5.45 L1.02,-5.05 L-1.02,-5.05 Z", C.brown, "M0.4,-5.45 L1.0,-5.45 L1.0,-5.05 L0.4,-5.05Z", C.brownD, { sw: sw * 0.8 });
  cel(g, "M-0.24,-5.52 L0.24,-5.52 L0.24,-4.98 L-0.24,-4.98 Z", C.gold, "M0,-5.52 L0.24,-5.52 L0.24,-4.98 L0,-4.98Z", C.goldD, { sw: sw * 0.7 });
  cel(g, "M0.95,-5.1 C1.35,-5.15 1.45,-4.6 1.4,-3.75 L1.0,-3.6 C0.85,-4.2 0.8,-4.7 0.95,-5.1 Z", C.brownL, "M1.2,-5.1 L1.5,-5.1 L1.5,-3.6 L1.2,-3.6Z", C.brown, { sw: sw * 0.8 });
  cel(g, "M1.0,-5.2 C1.15,-5.55 1.45,-5.65 1.6,-5.5 L1.45,-5.0 Z", C.steelD, null, null, { sw: sw * 0.7 });
  // shirt + vest torso
  cel(g, "M-1.15,-8.35 C-1.25,-7.4 -1.1,-6.2 -0.95,-5.4 L0.95,-5.4 C1.1,-6.2 1.25,-7.4 1.15,-8.35 Q0,-8.65 -1.15,-8.35 Z", C.cream, "M0.45,-8.5 L1.3,-8.5 L1.3,-5.4 L0.55,-5.4Z", C.creamD, { sw });
  cel(g, "M-1.15,-8.35 C-1.25,-7.4 -1.1,-6.2 -0.95,-5.4 L-0.3,-5.4 L-0.25,-7.0 L-0.55,-8.45 Z", C.brown, "M-0.6,-8.5 L-0.25,-8.5 L-0.25,-5.4 L-0.6,-5.4Z", C.brownD, { sw: sw * 0.9 });
  cel(g, "M1.15,-8.35 C1.25,-7.4 1.1,-6.2 0.95,-5.4 L0.35,-5.4 L0.3,-7.0 L0.6,-8.45 Z", C.brown, "M0.75,-8.5 L1.3,-8.5 L1.3,-5.4 L0.75,-5.4Z", C.brownD, { sw: sw * 0.9 });
  star6(g, -0.72, -7.15, 0.32, { sw: sw * 0.6 });
  // bandana knot
  cel(g, "M-0.5,-8.5 L0.55,-8.5 L0.05,-7.85 Z", C.red, "M0.05,-8.5 L0.55,-8.5 L0.05,-7.85Z", C.redD, { sw: sw * 0.8 });
  // arms: left thumb in belt, right hand hovering over the gun
  cel(g, "M-1.1,-8.25 C-1.6,-7.6 -1.75,-6.6 -1.35,-5.75 L-1.0,-5.45 L-0.85,-5.8 C-1.15,-6.5 -1.0,-7.3 -0.75,-7.8 Z", C.cream, "M-1.6,-7.6 L-1.2,-7.6 L-1.1,-5.5 L-1.6,-5.5Z", C.creamD, { sw });
  cel(g, "M-1.25,-5.95 C-1.05,-5.65 -0.75,-5.6 -0.6,-5.75 L-0.75,-5.25 C-1.0,-5.2 -1.3,-5.4 -1.25,-5.95 Z", C.skin, null, null, { sw: sw * 0.8 });
  cel(g, "M1.1,-8.25 C1.65,-7.7 1.95,-6.9 1.85,-6.15 L1.5,-6.05 C1.5,-6.7 1.3,-7.3 0.85,-7.75 Z", C.cream, "M1.55,-7.9 L2.0,-7.9 L2.0,-6.0 L1.55,-6.0Z", C.creamD, { sw });
  cel(g, "M1.45,-6.15 C1.4,-5.85 1.55,-5.6 1.75,-5.55 L1.82,-5.75 L1.95,-5.5 L2.05,-5.75 L2.15,-5.55 L2.15,-5.9 C2.1,-6.15 1.95,-6.25 1.85,-6.2 Z", C.skin, "M1.85,-6.3 L2.2,-6.3 L2.2,-5.5 L1.85,-5.5Z", C.skinD, { sw: sw * 0.8 });
  // neck + head
  cel(g, "M-0.25,-8.45 L-0.22,-8.85 L0.3,-8.85 L0.28,-8.45 Z", C.skin, null, null, { sw: sw * 0.8 });
  cel(g, "M-0.66,-8.72 C-0.85,-9.3 -0.8,-9.95 -0.58,-10.25 L0.64,-10.25 C0.86,-9.95 0.9,-9.3 0.72,-8.76 C0.52,-8.45 -0.45,-8.42 -0.66,-8.72 Z", C.skin, "M0.22,-10.3 L0.95,-10.3 L0.95,-8.4 L0.25,-8.4 C0.45,-9.0 0.42,-9.6 0.22,-10.3Z", C.skinD, { sw });
  // eyes, bushy brows, bulb nose, droopy mustache
  for (const x of [-0.27, 0.36]) { el("ellipse", { cx: x, cy: -9.72, rx: 0.13, ry: 0.11, fill: "#fff8e6", stroke: C.ink, "stroke-width": sw * 0.5 }, g); el("circle", { cx: x + 0.04, cy: -9.71, r: 0.055, fill: C.ink }, g); }
  line(g, "M-0.45,-9.92 Q-0.27,-10.02 -0.1,-9.9 M0.2,-9.9 Q0.37,-10.02 0.56,-9.94", sw * 1.1, C.creamD);
  line(g, "M-0.45,-9.92 Q-0.27,-10.02 -0.1,-9.9 M0.2,-9.9 Q0.37,-10.02 0.56,-9.94", sw * 0.45);
  cel(g, "M0.06,-9.62 C0.28,-9.62 0.36,-9.42 0.3,-9.28 C0.22,-9.16 -0.05,-9.16 -0.1,-9.3 C-0.14,-9.45 -0.08,-9.6 0.06,-9.62 Z", "#e49a68", "M0.12,-9.7 L0.4,-9.7 L0.4,-9.1 L0.12,-9.1Z", C.skinD, { sw: sw * 0.6 });
  cel(g, "M-0.62,-9.0 C-0.35,-9.25 0.02,-9.22 0.1,-9.12 C0.2,-9.24 0.6,-9.26 0.85,-8.98 C0.92,-8.72 0.84,-8.5 0.75,-8.42 C0.62,-8.7 0.35,-8.82 0.1,-8.9 C-0.12,-8.8 -0.45,-8.7 -0.6,-8.4 C-0.72,-8.55 -0.72,-8.8 -0.62,-9.0 Z", C.creamD, "M0.12,-9.4 L1,-9.4 L1,-8.4 L0.12,-8.4Z", "#c8a26a", { sw: sw * 0.8 });
  // hat: tall pinched crown, wide curled brim
  cel(g, "M-0.75,-10.3 C-0.8,-10.9 -0.6,-11.5 -0.35,-11.6 C-0.1,-11.45 0.1,-11.45 0.35,-11.6 C0.65,-11.5 0.85,-10.9 0.8,-10.3 Z", C.cream, "M0.2,-11.7 L0.95,-11.7 L0.95,-10.3 L0.3,-10.3Z", C.creamD, { sw });
  cel(g, "M-0.78,-10.55 L0.82,-10.55 L0.82,-10.32 L-0.78,-10.32 Z", C.brownD, null, null, { sw: sw * 0.7 });
  cel(g, "M-1.85,-10.65 C-1.7,-10.35 -1.3,-10.2 -0.8,-10.25 L0.85,-10.25 C1.35,-10.2 1.75,-10.35 1.9,-10.7 C2.0,-10.35 1.7,-9.95 1.0,-9.95 L-0.95,-9.95 C-1.65,-9.95 -1.95,-10.3 -1.85,-10.65 Z", C.cream, "M0.6,-10.3 L2.1,-10.3 L2.1,-9.9 L0.6,-9.9Z", C.creamD, { sw });
  el("path", { d: "M-0.6,-9.93 L0.75,-9.93 L0.6,-9.7 L-0.5,-9.7 Z", fill: C.skinD, opacity: 0.9 }, g);
  line(g, "M-0.3,-11.35 C-0.25,-10.95 -0.3,-10.7 -0.35,-10.55", sw * 0.5, C.ink, { opacity: 0.7 });
}

// The outlaw: black hat, duster coat, red bandana over the face. Faces left.
export function outlaw(g, { sw = 0.13 } = {}) {
  const k = el("g", { transform: "scale(-1,1)" }, g);
  // boots
  cel(k, "M-1.35,-0.05 L-1.3,-0.5 C-1.2,-0.75 -0.9,-0.8 -0.7,-0.9 L-0.65,-1.6 L0.0,-1.6 L-0.02,-0.15 L0.08,0 Z", C.darkD, null, null, { sw });
  cel(k, "M0.7,-1.6 L1.35,-1.6 L1.4,-0.85 C1.7,-0.7 2.05,-0.6 2.15,-0.35 L2.15,-0.05 L0.7,0 Z", C.darkD, null, null, { sw });
  for (const x of [-0.05, 0.68]) el("circle", { cx: x, cy: -0.35, r: 0.2, fill: C.gold, stroke: C.ink, "stroke-width": sw * 0.6 }, k);
  // trousers
  cel(k, "M-0.9,-5.0 L0.9,-5.0 L1.45,-1.5 L0.65,-1.5 L0.12,-3.8 L-0.05,-3.8 L-0.05,-1.5 L-0.78,-1.5 Z", C.brownD, "M0.25,-5 L0.9,-5 L1.45,-1.5 L1.0,-1.5Z", C.dark, { sw });
  // duster coat, tails flaring
  cel(k, "M-1.2,-8.3 C-1.45,-7.2 -1.6,-5.4 -2.25,-3.2 L-1.1,-3.6 L-0.65,-5.3 L-0.3,-8.3 Z", C.plum, "M-1.2,-8.3 L-0.3,-8.3 L-0.65,-5.3 L-1.1,-3.6 L-1.6,-3.4 Z", "#3d1f2a", { sw });
  cel(k, "M1.2,-8.3 C1.5,-7.2 1.7,-5.4 2.45,-3.4 L1.25,-3.7 L0.7,-5.3 L0.3,-8.3 Z", C.plum, "M0.9,-8.3 L1.7,-8.3 L2.5,-3.4 L1.6,-3.6Z", "#3d1f2a", { sw });
  // red shirt
  cel(k, "M-0.35,-8.4 L0.35,-8.4 L0.7,-5.3 L-0.65,-5.3 Z", C.red, "M0.1,-8.4 L0.7,-8.4 L0.7,-5.3 L0.2,-5.3Z", C.redD, { sw: sw * 0.9 });
  // gun belts crossing + holsters
  cel(k, "M-0.9,-5.5 L0.9,-5.5 L0.92,-5.1 L-0.92,-5.1 Z", C.brownD, null, null, { sw: sw * 0.8 });
  for (let i = 0; i < 6; i++) el("rect", { x: -0.8 + i * 0.3, y: -5.45, width: 0.12, height: 0.3, fill: C.gold, stroke: C.ink, "stroke-width": 0.03 }, k);
  cel(k, "M0.85,-5.15 C1.2,-5.15 1.35,-4.6 1.3,-3.8 L0.95,-3.65 C0.8,-4.2 0.75,-4.7 0.85,-5.15 Z", C.brown, null, null, { sw: sw * 0.8 });
  cel(k, "M-0.85,-5.15 C-1.2,-5.15 -1.35,-4.6 -1.3,-3.8 L-0.95,-3.65 C-0.8,-4.2 -0.75,-4.7 -0.85,-5.15 Z", C.brown, null, null, { sw: sw * 0.8 });
  // arms (coat sleeves), hands hovering
  cel(k, "M1.15,-8.25 C1.75,-7.7 2.05,-6.9 1.95,-6.15 L1.55,-6.05 C1.55,-6.7 1.35,-7.3 0.9,-7.75 Z", C.plum, "M1.6,-7.9 L2.1,-7.9 L2.1,-6 L1.6,-6Z", "#3d1f2a", { sw });
  cel(k, "M1.5,-6.15 C1.45,-5.85 1.6,-5.6 1.8,-5.55 L1.87,-5.75 L2.0,-5.5 L2.1,-5.75 L2.2,-5.55 L2.2,-5.9 C2.15,-6.15 2.0,-6.25 1.9,-6.2 Z", C.skin, "M1.9,-6.3 L2.25,-6.3 L2.25,-5.5 L1.9,-5.5Z", C.skinD, { sw: sw * 0.8 });
  cel(k, "M-1.15,-8.25 C-1.75,-7.7 -2.05,-6.9 -1.95,-6.15 L-1.55,-6.05 C-1.55,-6.7 -1.35,-7.3 -0.9,-7.75 Z", C.plum, null, null, { sw });
  cel(k, "M-1.5,-6.15 C-1.45,-5.85 -1.6,-5.6 -1.8,-5.55 L-1.87,-5.75 L-2.0,-5.5 L-2.1,-5.75 L-2.2,-5.55 L-2.2,-5.9 C-2.15,-6.15 -2.0,-6.25 -1.9,-6.2 Z", C.skin, null, null, { sw: sw * 0.8 });
  // head with bandana mask
  cel(k, "M-0.5,-8.65 C-0.65,-9.25 -0.62,-9.9 -0.42,-10.2 L0.52,-10.2 C0.72,-9.9 0.75,-9.25 0.58,-8.7 C0.4,-8.45 -0.3,-8.4 -0.5,-8.65 Z", C.skin, "M0.15,-10.3 L0.9,-10.3 L0.9,-8.4 L0.2,-8.4Z", C.skinD, { sw });
  cel(k, "M-0.62,-9.35 L0.75,-9.35 C0.8,-9.0 0.65,-8.55 0.5,-8.35 L0.05,-7.8 L-0.45,-8.35 C-0.65,-8.6 -0.68,-9.0 -0.62,-9.35 Z", C.red, "M0.15,-9.4 L0.9,-9.4 L0.9,-7.8 L0.1,-7.8Z", C.redD, { sw });
  for (const [x, y] of [[-0.3, -9.0], [0.2, -8.85], [-0.05, -8.4], [0.45, -9.1]]) el("circle", { cx: x, cy: y, r: 0.06, fill: C.cream }, k);
  // mean eyes and brows
  line(k, "M-0.42,-9.62 L-0.08,-9.58 M0.3,-9.58 L0.6,-9.62", sw * 0.9);
  line(k, "M-0.48,-9.95 L-0.05,-9.78 M0.25,-9.78 L0.66,-9.95", sw * 0.8);
  // black hat, flat brim
  cel(k, "M-0.72,-10.25 C-0.75,-10.8 -0.6,-11.3 -0.3,-11.35 L0.35,-11.35 C0.65,-11.3 0.8,-10.8 0.78,-10.25 Z", C.darkD, "M0.25,-11.4 L0.9,-11.4 L0.9,-10.25 L0.35,-10.25Z", "#120a07", { sw, hi: "M-0.55,-11.1 L-0.35,-11.2 L-0.42,-10.5 L-0.6,-10.5Z", hiFill: "#6d4a3d", hiOp: 0.9 });
  cel(k, "M-0.75,-10.5 L0.8,-10.5 L0.8,-10.3 L-0.75,-10.3 Z", C.red, null, null, { sw: sw * 0.6 });
  cel(k, "M-1.75,-10.35 C-1.4,-10.15 -1.0,-10.1 -0.75,-10.12 L0.8,-10.12 C1.1,-10.1 1.5,-10.15 1.8,-10.35 C1.75,-10.05 1.4,-9.9 0.95,-9.9 L-0.9,-9.9 C-1.4,-9.9 -1.7,-10.05 -1.75,-10.35 Z", C.darkD, null, null, { sw });
  el("path", { d: "M-0.55,-9.88 L0.7,-9.88 L0.55,-9.68 L-0.45,-9.68 Z", fill: C.skinD, opacity: 0.95 }, k);
}

// A mechanical bucking bull with saddle on a sprung pedestal. Faces left. Origin at base centre.
export function mechBull(g, { sw = 0.12, body = "#a8452a", bodyD = "#7a2c1a" } = {}) {
  // pedestal
  cel(g, "M-1.2,0 L1.2,0 L1.0,-0.45 L-1.0,-0.45 Z", C.steelD, null, null, { sw });
  cel(g, "M-0.35,-0.45 L0.35,-0.45 L0.28,-1.3 L-0.28,-1.3 Z", C.steel, "M0.05,-1.4 L0.4,-1.4 L0.4,-0.4 L0.05,-0.4Z", C.steelD, { sw });
  for (let i = 0; i < 4; i++) line(g, `M-0.3,${-0.6 - i * 0.18} L0.3,${-0.68 - i * 0.18}`, sw * 0.5);
  // legs (stubby, mechanical)
  for (const [x, w] of [[-1.25, 0.32], [-0.75, 0.32], [0.7, 0.32], [1.15, 0.32]]) cel(g, `M${x - w / 2},-1.9 L${x + w / 2},-1.9 L${x + w / 2 - 0.05},-1.15 L${x - w / 2 + 0.05},-1.15 Z`, bodyD, null, null, { sw: sw * 0.9 });
  // body barrel, tilted (bucking)
  cel(g, "M-1.7,-2.4 C-1.75,-3.1 -1.2,-3.45 -0.2,-3.35 C0.8,-3.3 1.6,-3.2 1.75,-2.6 C1.85,-2.0 1.3,-1.75 0.3,-1.8 C-0.8,-1.85 -1.65,-1.8 -1.7,-2.4 Z", body, "M-1.8,-2.2 C-0.6,-2.0 0.6,-2.0 1.9,-2.3 L1.9,-1.6 L-1.8,-1.6Z", bodyD, { sw, hi: "M-1.1,-3.15 C-0.5,-3.3 0.4,-3.3 0.9,-3.15 C0.4,-3.18 -0.4,-3.18 -1.1,-3.15Z", hiFill: "#f6b38d", hiOp: 0.8 });
  // tail
  line(g, "M1.7,-2.75 C2.1,-2.9 2.2,-3.4 2.05,-3.7", sw * 0.9);
  el("path", { d: "M2.05,-3.7 l-0.12,-0.25 l0.25,0.05 z", fill: C.ink }, g);
  // saddle
  cel(g, "M-0.55,-3.3 C-0.6,-3.75 -0.35,-3.9 -0.15,-3.8 C0.05,-3.6 0.3,-3.6 0.5,-3.75 C0.7,-3.85 0.85,-3.6 0.8,-3.3 Z", C.brown, "M0.3,-3.9 L0.9,-3.9 L0.9,-3.3 L0.3,-3.3Z", C.brownD, { sw });
  cel(g, "M-0.3,-3.82 L-0.2,-4.25 L0.0,-4.25 L-0.05,-3.78 Z", C.brownL, null, null, { sw: sw * 0.8 });
  cel(g, "M-0.2,-3.3 L0.35,-3.3 L0.35,-2.5 L-0.2,-2.5 Z", C.red, null, null, { sw: sw * 0.8 });
  // head lowered, horns forward, nose ring
  cel(g, "M-1.95,-3.1 C-2.35,-3.4 -1.3,-3.6 -1.0,-3.5 C-1.35,-3.35 -1.6,-3.2 -1.75,-3.0 Z", C.cream, null, null, { sw: sw * 0.9 });
  cel(g, "M-1.35,-3.2 C-1.95,-3.45 -2.65,-3.2 -2.85,-2.55 C-2.95,-2.05 -2.7,-1.55 -2.2,-1.55 C-1.75,-1.6 -1.45,-2.0 -1.3,-2.5 Z", body, "M-2.3,-2.0 C-2.5,-1.6 -2.9,-1.7 -2.95,-2.1 L-2.95,-1.4 L-2.0,-1.4Z", bodyD, { sw });
  cel(g, "M-2.9,-2.1 C-2.98,-1.75 -2.75,-1.48 -2.4,-1.5 C-2.15,-1.52 -2.02,-1.7 -2.0,-1.9 C-2.2,-2.05 -2.6,-2.15 -2.9,-2.1 Z", "#ecb092", null, null, { sw: sw * 0.8 });
  el("circle", { cx: -2.68, cy: -1.82, r: 0.06, fill: C.ink }, g);
  el("circle", { cx: -2.38, cy: -1.78, r: 0.06, fill: C.ink }, g);
  el("circle", { cx: -2.52, cy: -1.55, r: 0.14, fill: "none", stroke: C.gold, "stroke-width": sw * 0.7 }, g);
  el("circle", { cx: -2.52, cy: -1.55, r: 0.14, fill: "none", stroke: C.ink, "stroke-width": sw * 0.25 }, g);
  el("ellipse", { cx: -2.15, cy: -2.55, rx: 0.13, ry: 0.1, fill: "#fff8e6", stroke: C.ink, "stroke-width": sw * 0.5 }, g);
  el("circle", { cx: -2.2, cy: -2.53, r: 0.05, fill: C.ink }, g);
  line(g, "M-2.35,-2.75 L-1.95,-2.62", sw * 1.1);
  cel(g, "M-2.25,-3.05 C-2.6,-3.4 -3.05,-3.5 -3.35,-3.25 C-3.05,-3.2 -2.75,-3.0 -2.5,-2.8 Z", C.cream, "M-3.4,-3.3 L-3.0,-3.3 L-2.8,-2.8 L-3.4,-2.8Z", C.creamD, { sw: sw * 0.9 });
  for (const [x, y, dx, dy] of [[-3.25, -1.35, -0.3, 0.12], [-3.2, -1.15, -0.25, 0.25]]) line(g, `M${x},${y} l${dx},${dy}`, sw * 0.6);
  // bucking motion lines
  for (const [x, y, dx, dy] of [[1.95, -2.1, 0.45, 0.1], [2.0, -1.75, 0.4, 0.25], [-0.2, -4.4, 0, -0.35], [0.25, -4.3, 0.15, -0.3]]) line(g, `M${x},${y} l${dx},${dy}`, sw * 0.7);
}

// Saguaro cactus (two arms). Origin at base.
export function cactus(g, { sw = 0.1, h = 3 } = {}) {
  const s = h / 3;
  const k = el("g", { transform: `scale(${s})` }, g);
  const trunk = "M-0.35,0 L-0.35,-2.6 C-0.35,-3.05 0.35,-3.05 0.35,-2.6 L0.35,0 Z";
  const armL = "M-0.35,-1.2 L-0.85,-1.2 C-1.15,-1.2 -1.2,-1.5 -1.2,-1.7 L-1.2,-2.25 C-1.2,-2.55 -0.8,-2.55 -0.8,-2.25 L-0.8,-1.62 L-0.35,-1.62 Z";
  const armR = "M0.35,-1.6 L0.8,-1.6 C1.1,-1.6 1.15,-1.9 1.15,-2.1 L1.15,-2.45 C1.15,-2.75 0.75,-2.75 0.75,-2.45 L0.75,-2.0 L0.35,-2.0 Z";
  for (const d of [armL, armR, trunk]) cel(k, d, C.green, "M0.05,-3.2 L1.3,-3.2 L1.3,0 L0.05,0Z M-0.95,-2.6 L-0.8,-2.6 L-0.8,-1.2 L-0.95,-1.2Z", C.greenD, { sw: sw / s, hi: "M-0.25,-2.7 L-0.12,-2.75 L-0.12,-0.2 L-0.25,-0.2Z", hiFill: C.greenL, hiOp: 0.9 });
  for (let i = 0; i < 6; i++) line(k, `M${-0.38 + 0.02},${-0.4 - i * 0.4} l-0.12,-0.05 M0.36,${-0.5 - i * 0.4} l0.12,-0.05`, (sw * 0.5) / s);
}
