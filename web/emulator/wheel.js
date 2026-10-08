// A steering wheel turned with the arrows, for a game steered with one (`wheel` in
// assets/games.ron: Out Run, Cruis'n USA). A d-pad's full lock at once, or a ramp too quick for a
// tap, would have the car across the road; a keyboard player needs a wheel that a tap barely
// turns and a hold turns all the way. So, at the game's frame rate:
//
// - Held, Left or Right turns the wheel toward that side along a curve: `lock` seconds from the
//   middle to full lock, at `f^curve` of full lock after `f` of that time (curve 1 turns evenly;
//   at 2 a hold for half the time turns it a quarter of the way, a short tap next to nothing,
//   and it speeds up toward the lock). The wheel goes on from wherever it is: from the time a
//   hold would have taken to get it there, a frame more.
// - Let go (or both held), it comes back to the middle evenly, from full lock in `back` seconds.
// - The other arrow brings it straight to the middle and on from there, as Cannonball (Out Run's
//   engine, ported) turns its wheel from keys.
//
// It runs where the player's controls are sampled (worker.js), once a frame, and goes out in the
// input itself, bits 16-23, a signed byte: -127 full left to 127 full right, 0 (any input without
// one) straight ahead. So every machine that runs the frame (a watcher's, a rollback running it
// again) turns the game's wheel the same, whatever state it started from, and the ramp is the
// local player's business alone: `/wheel` in the chat changes it ("wheel" in worker.js) without
// any other machine knowing.

export const WHEEL_LOCK = 127;
const LEFT = 1 << 6;
const RIGHT = 1 << 7;

export class Wheel {
  /** Where the wheel is: -127 full left .. 127 full right, between whole steps too. */
  position = 0;
  /** Seconds from the middle to full lock, held; from full lock back to the middle, let go. */
  lock = 0.3;
  back = 0.3;
  /** The turn's curve while held (1: even). */
  curve = 1;
  /** The game's frames a second. */
  fps = 60;

  /** `ramp`: any of { lock, back, curve } (world::Game's `wheel`), from the next frame on. */
  constructor(ramp = {}, fps = 60) {
    this.set(ramp, fps);
  }

  /** Changes how the wheel turns: any of `lock`, `back` and `curve`, and the game's `fps`. It stays where it is. */
  set({ lock = this.lock, back = this.back, curve = this.curve } = {}, fps = this.fps) {
    Object.assign(this, { lock, back, curve, fps });
  }

  /** The next frame's input: `input`'s buttons, and the wheel turned by its arrows in bits 16-23. */
  turn(input) {
    const left = (input & LEFT) !== 0;
    const right = (input & RIGHT) !== 0;
    const toward = left === right ? 0 : left ? -1 : 1;
    if (toward) {
      // How far it is toward that side (none, on the other), and how long a hold takes to get there.
      const turned = Math.max(0, toward * this.position) / WHEEL_LOCK;
      const held = this.lock * turned ** (1 / this.curve);
      const along = Math.min(1, (held + 1 / this.fps) / this.lock);
      this.position = toward * WHEEL_LOCK * along ** this.curve;
    } else {
      const step = WHEEL_LOCK / (this.back * this.fps);
      this.position = this.position > 0 ? Math.max(0, this.position - step) : Math.min(0, this.position + step);
    }
    const byte = Math.sign(this.position) * Math.round(Math.abs(this.position));
    return ((input & 0xffff) | ((byte & 0xff) << 16)) >>> 0;
  }
}

/** The wheel in an input: bits 16-23, signed. */
export const wheelIn = (input) => (input << 8) >> 24;
