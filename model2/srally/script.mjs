// The inputs the checks and the bench drive the game with (RetroPad masks, web/emulator's ids),
// and where the game keeps what they watch.
export const B = 1 << 0, Y = 1 << 1, SELECT = 1 << 2, START = 1 << 3, UP = 1 << 4, DOWN = 1 << 5, LEFT = 1 << 6, RIGHT = 1 << 7, A = 1 << 8;

/** A u32 of the i960's RAM as RETRO_MEMORY_SYSTEM_RAM lays it out: CRX RAM (0x200000) at 0, work RAM (0x500000) at 0x40000. */
export function word(ram, offset) {
  return (ram[offset] | (ram[offset + 1] << 8) | (ram[offset + 2] << 16) | (ram[offset + 3] << 24)) >>> 0;
}

/** The game's state words (sys24_viewer.c, lift_skip_practice.c): main mode 0x202098 (0 boot,
 * 2 attract, 3 game, 4 test menu), its inner step 0x20209c, the game's scene 0x2020ac, and the
 * race clock 0x214120 (> 0 once the race is on). */
export function modes(ram) {
  return { mode: word(ram, 0x2098), inner: word(ram, 0x209c), scene: word(ram, 0x20ac), race: (word(ram, 0x14120) | 0) > 0 ? 1 : 0 };
}

// From power-on: the attract mode, two coins (the factory setting is 2 coins a credit), START,
// then the accelerator held through the menus (the car and transmission select time out on
// their own) and down the first straight.
export const RACE_SCRIPT = { coins: [900, 930], start: 990, drive: 1150, until: 2400 };

export function raceMask(f) {
  const s = RACE_SCRIPT;
  let mask = 0;
  for (const c of s.coins) if (f >= c && f < c + 6) mask |= SELECT;
  if (f >= s.start && f < s.start + 6) mask |= START;
  if (f >= s.drive) mask |= UP;
  return mask;
}

/** FNV-1a over 32-bit words, as web/emulator/worker.js hashes RAM. */
export function hash(bytes) {
  let h = 0x811c9dc5;
  const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.length >> 2);
  for (let i = 0; i < words.length; i++) h = Math.imul(h ^ words[i], 0x01000193);
  return (h >>> 0).toString(16).padStart(8, "0");
}
