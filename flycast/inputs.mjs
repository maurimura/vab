// What check.mjs, bench.mjs and the Chrome harness (harness/core-worker.mjs) share, so a run in
// Node and one in the browser can be compared: the scripted inputs and the RAM hash.

/** FNV-1a over 32-bit words, as web/emulator/worker.js's hashRam. */
export function hash(bytes) {
  let h = 0x811c9dc5;
  const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.length >> 2);
  for (let i = 0; i < words.length; i++) h = Math.imul(h ^ words[i], 0x01000193);
  return (h >>> 0).toString(16).padStart(8, "0");
}

/**
 * Both players' inputs for `frames` frames, [player 1, player 2] RetroPad masks: random buttons
 * and directions held for 2-20 frames (emulator/rollback-check.mjs's generator). SHOT1 (B),
 * SHOT2 (A), Start and the stick; no coins, test or service buttons.
 */
export function inputs(frames, seed = 1) {
  let s = seed;
  const random = () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const USABLE = 0b0000_0001_1111_1001; // B START UP DOWN LEFT RIGHT A
  const out = [];
  let masks = [0, 0];
  let hold = 0;
  for (let f = 0; f < frames; f++) {
    if (hold-- <= 0) {
      masks = [Math.floor(random() * 0x10000) & USABLE, Math.floor(random() * 0x10000) & USABLE];
      hold = 2 + Math.floor(random() * 18);
    }
    out.push(masks);
  }
  return out;
}
