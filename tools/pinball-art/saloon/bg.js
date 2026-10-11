// Saloon wall behind the machine: dark planks in comic ink, warm lamplight.
import { el, radGrad, rng } from "./lib.js";
export function drawBackground(svg, { C }) {
  const defs = el("defs", {}, svg);
  el("rect", { x: 0, y: 0, width: 1920, height: 1080, fill: "#3a1f12" }, svg);
  const R = rng(3);
  for (let x = -10; x < 1920; x += 64) {
    const c = ["#3f2214", "#371d10", "#432517"][Math.floor(R() * 3)];
    el("rect", { x, y: 0, width: 64, height: 1080, fill: c }, svg);
    el("rect", { x: x + 44, y: 0, width: 20, height: 1080, fill: "#2a150b", opacity: 0.6 }, svg);
    el("path", { d: `M${x},0V1080`, stroke: "#160a05", "stroke-width": 4 }, svg);
    for (let y = 40 + R() * 200; y < 1080; y += 260 + R() * 200) { el("circle", { cx: x + 12, cy: y, r: 3, fill: "#160a05" }, svg); el("circle", { cx: x + 52, cy: y, r: 3, fill: "#160a05" }, svg); }
  }
  // wainscot rail
  el("rect", { x: 0, y: 760, width: 1920, height: 18, fill: "#5e3418", stroke: "#160a05", "stroke-width": 4 }, svg);
  el("rect", { x: 0, y: 778, width: 1920, height: 302, fill: "#2a150b", opacity: 0.55 }, svg);
  // lamp glow
  el("rect", { x: 0, y: 0, width: 1920, height: 1080, fill: radGrad(defs, [[0, "#ffb050", 0.28], [1, "#ffb050", 0]], { cx: 0.55, cy: 0.05, r: 0.6 }) }, svg);
  el("rect", { x: 0, y: 0, width: 1920, height: 1080, fill: radGrad(defs, [[0, "#000", 0], [0.6, "#000", 0.2], [1, "#000", 0.7]], { cx: 0.5, cy: 0.45, r: 0.75 }) }, svg);
}
