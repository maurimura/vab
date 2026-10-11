// Small drawing kit shared by the table illustrations: SVG helpers, a tilted-table camera whose
// floor plane is applied to a flat SVG with a CSS matrix3d, and 3D-ish primitives drawn in screen space.
export const SVGNS = "http://www.w3.org/2000/svg";

export function el(tag, attrs = {}, parent) {
  const e = document.createElementNS(SVGNS, tag);
  for (const k in attrs) {
    const v = attrs[k];
    if (v !== undefined && v !== null && v !== false) e.setAttribute(k, v);
  }
  if (parent) parent.appendChild(e);
  return e;
}
export function txt(t, attrs, parent) {
  const e = el("text", attrs, parent);
  e.textContent = t;
  return e;
}
let uid = 0;
export const id = (p = "i") => `${p}${++uid}`;

export const f2 = (n) => (Math.round(n * 100) / 100).toString();
export function poly(pts, close = true) {
  if (!pts.length) return "";
  let d = `M${f2(pts[0][0])},${f2(pts[0][1])}`;
  for (let i = 1; i < pts.length; i++) d += `L${f2(pts[i][0])},${f2(pts[i][1])}`;
  return close ? d + "Z" : d;
}
// Smooth closed/open curve through points (Catmull-Rom -> cubic Bezier).
export function smooth(pts, close = true, t = 0.5) {
  const n = pts.length;
  if (n < 3) return poly(pts, close);
  const P = (i) => (close ? pts[(i + n) % n] : pts[Math.max(0, Math.min(n - 1, i))]);
  let d = `M${f2(pts[0][0])},${f2(pts[0][1])}`;
  const last = close ? n : n - 1;
  for (let i = 0; i < last; i++) {
    const p0 = P(i - 1), p1 = P(i), p2 = P(i + 1), p3 = P(i + 2);
    const c1 = [p1[0] + ((p2[0] - p0[0]) * t) / 3, p1[1] + ((p2[1] - p0[1]) * t) / 3];
    const c2 = [p2[0] - ((p3[0] - p1[0]) * t) / 3, p2[1] - ((p3[1] - p1[1]) * t) / 3];
    d += `C${f2(c1[0])},${f2(c1[1])} ${f2(c2[0])},${f2(c2[1])} ${f2(p2[0])},${f2(p2[1])}`;
  }
  return close ? d + "Z" : d;
}
export function arcPts(cx, cy, r, a0, a1, n = 24, ry = r) {
  const out = [];
  for (let i = 0; i <= n; i++) {
    const a = ((a0 + ((a1 - a0) * i) / n) * Math.PI) / 180;
    out.push([cx + r * Math.cos(a), cy + ry * Math.sin(a)]);
  }
  return out;
}
export function circlePts(cx, cy, r, n = 36) {
  return arcPts(cx, cy, r, 0, 360, n).slice(0, n);
}
export const lerp = (a, b, t) => a + (b - a) * t;
export const lerp2 = (a, b, t) => [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
export const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

// Offsets an open polyline by d on its left (d>0) or right side, mitred.
export function offsetLine(pts, d) {
  const out = [];
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i];
    const a = pts[Math.max(0, i - 1)], b = pts[Math.min(pts.length - 1, i + 1)];
    let n1 = null, n2 = null;
    if (i > 0) { const dx = p[0] - a[0], dy = p[1] - a[1], l = Math.hypot(dx, dy); n1 = [-dy / l, dx / l]; }
    if (i < pts.length - 1) { const dx = b[0] - p[0], dy = b[1] - p[1], l = Math.hypot(dx, dy); n2 = [-dy / l, dx / l]; }
    let n = n1 && n2 ? [n1[0] + n2[0], n1[1] + n2[1]] : n1 || n2;
    const l = Math.hypot(n[0], n[1]);
    n = [n[0] / l, n[1] / l];
    let k = 1;
    if (n1 && n2) { const c = n[0] * n1[0] + n[1] * n1[1]; k = 1 / Math.max(0.35, c); }
    out.push([p[0] + n[0] * d * k, p[1] + n[1] * d * k]);
  }
  return out;
}
// Footprint polygon of a wall of thickness w along a polyline.
export function band(pts, w) {
  return offsetLine(pts, w / 2).concat(offsetLine(pts, -w / 2).reverse());
}
// Convex hull of circles (for rubber rings stretched around posts), returned as a point loop.
export function ringHull(circles, n = 20) {
  const pts = [];
  for (const [cx, cy, r] of circles) for (let i = 0; i < 48; i++) {
    const a = (i / 48) * Math.PI * 2;
    pts.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
  }
  pts.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const cross = (o, a, b) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower = [], upper = [];
  for (const p of pts) { while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 0) lower.pop(); lower.push(p); }
  for (let i = pts.length - 1; i >= 0; i--) { const p = pts[i]; while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 0) upper.pop(); upper.push(p); }
  upper.pop(); lower.pop();
  return lower.concat(upper);
}

// Camera looking at a tilted table. Table units (inches): x right, y down the table (toward player), z up.
export function makeCamera({ W, L, tilt, dist: D, targetY = L / 2, fitTop, fitBottom, centerX }) {
  const th = (tilt * Math.PI) / 180;
  const C = [W / 2, targetY + D * Math.sin(th), D * Math.cos(th)];
  const u = [0, -Math.cos(th), Math.sin(th)];
  const f = [0, -Math.sin(th), -Math.cos(th)];
  const raw = (x, y, z = 0) => {
    const q = [x - C[0], y - C[1], z - C[2]];
    const yc = q[1] * u[1] + q[2] * u[2];
    const zc = q[1] * f[1] + q[2] * f[2];
    return [q[0] / zc, -yc / zc];
  };
  const t = raw(W / 2, 0, 0), b = raw(W / 2, L, 0);
  const s = (fitBottom - fitTop) / (b[1] - t[1]);
  const ox = centerX, oy = fitTop - s * t[1];
  const proj = (x, y, z = 0) => { const r = raw(x, y, z); return [ox + s * r[0], oy + s * r[1]]; };
  const scale = (x, y, z = 0) => { const a = proj(x - 0.5, y, z), c = proj(x + 0.5, y, z); return c[0] - a[0]; };
  function homography(src, dst) {
    const A = [], B = [];
    for (let i = 0; i < 4; i++) {
      const [u0, v0] = src[i], [x, y] = dst[i];
      A.push([u0, v0, 1, 0, 0, 0, -u0 * x, -v0 * x]); B.push(x);
      A.push([0, 0, 0, u0, v0, 1, -u0 * y, -v0 * y]); B.push(y);
    }
    const n = 8;
    for (let c = 0; c < n; c++) {
      let p = c;
      for (let r = c + 1; r < n; r++) if (Math.abs(A[r][c]) > Math.abs(A[p][c])) p = r;
      [A[c], A[p]] = [A[p], A[c]]; [B[c], B[p]] = [B[p], B[c]];
      for (let r = 0; r < n; r++) if (r !== c) {
        const k = A[r][c] / A[c][c];
        for (let k2 = c; k2 < n; k2++) A[r][k2] -= k * A[c][k2];
        B[r] -= k * B[c];
      }
    }
    return B.map((v, i) => v / A[i][i]).concat([1]);
  }
  // CSS transform putting an element of size (K*W x K*L) px onto the floor plane at height z.
  function floorMatrix(K, z = 0, x0 = 0, y0 = 0, w = W, l = L) {
    const src = [[0, 0], [K * w, 0], [K * w, K * l], [0, K * l]];
    const dst = [proj(x0, y0, z), proj(x0 + w, y0, z), proj(x0 + w, y0 + l, z), proj(x0, y0 + l, z)];
    const h = homography(src, dst);
    return `matrix3d(${h[0]},${h[3]},0,${h[6]},${h[1]},${h[4]},0,${h[7]},0,0,1,0,${h[2]},${h[5]},0,${h[8]})`;
  }
  // Linearised projection around a point: an SVG matrix() mapping local (u, v) along table vectors ux, uy.
  function frame(o, ux = [1, 0, 0], uy = [0, 1, 0]) {
    const e = 0.05;
    const p0 = proj(o[0], o[1], o[2] || 0);
    const pu = proj(o[0] + ux[0] * e, o[1] + ux[1] * e, (o[2] || 0) + ux[2] * e);
    const pv = proj(o[0] + uy[0] * e, o[1] + uy[1] * e, (o[2] || 0) + uy[2] * e);
    return `matrix(${(pu[0] - p0[0]) / e},${(pu[1] - p0[1]) / e},${(pv[0] - p0[0]) / e},${(pv[1] - p0[1]) / e},${p0[0]},${p0[1]})`;
  }
  return { proj, scale, floorMatrix, frame, W, L, C };
}

// Projected path of a loop of table points at height z.
export function projPath(cam, pts, z = 0, close = true) {
  return poly(pts.map((p) => cam.proj(p[0], p[1], p[2] ?? z)), close);
}
export function projSmooth(cam, pts, z = 0, close = true) {
  return smooth(pts.map((p) => cam.proj(p[0], p[1], p[2] ?? z)), close);
}

// Halftone dots inside a clip path. fn(x, y) -> 0..1 ink coverage.
export function halftone(parent, { x0, y0, x1, y1, pitch, angle = 45, fn, fill, clip, opacity = 1 }) {
  const a = (angle * Math.PI) / 180, ca = Math.cos(a), sa = Math.sin(a);
  const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2, R = Math.hypot(x1 - x0, y1 - y0) / 2 + pitch;
  let d = "";
  for (let i = -R; i <= R; i += pitch) for (let j = -R; j <= R; j += pitch) {
    const x = cx + i * ca - j * sa, y = cy + i * sa + j * ca;
    if (x < x0 - pitch || x > x1 + pitch || y < y0 - pitch || y > y1 + pitch) continue;
    const v = Math.max(0, Math.min(1, fn(x, y)));
    if (v < 0.02) continue;
    const r = pitch * 0.62 * Math.sqrt(v);
    d += `M${f2(x - r)},${f2(y)}a${f2(r)},${f2(r)} 0 1,0 ${f2(2 * r)},0a${f2(r)},${f2(r)} 0 1,0 ${f2(-2 * r)},0`;
  }
  return el("path", { d, fill, "clip-path": clip ? `url(#${clip})` : null, opacity }, parent);
}

export function linGrad(defs, stops, { x1 = 0, y1 = 0, x2 = 1, y2 = 0, units } = {}) {
  const gid = id("g");
  const g = el("linearGradient", { id: gid, x1, y1, x2, y2, gradientUnits: units }, defs);
  for (const [o, c, op] of stops) el("stop", { offset: o, "stop-color": c, "stop-opacity": op ?? 1 }, g);
  return `url(#${gid})`;
}
export function radGrad(defs, stops, { cx = 0.5, cy = 0.5, r = 0.5, fx, fy, units } = {}) {
  const gid = id("r");
  const g = el("radialGradient", { id: gid, cx, cy, r, fx, fy, gradientUnits: units }, defs);
  for (const [o, c, op] of stops) el("stop", { offset: o, "stop-color": c, "stop-opacity": op ?? 1 }, g);
  return `url(#${gid})`;
}

// Seeded random.
export function rng(seed = 1) {
  let s = seed >>> 0;
  return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
}
