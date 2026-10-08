// A steering wheel turned with the arrows, for a game steered with one (Out Run; libretro.js
// says which, from the core's controls): held Left or Right turns it a step a frame toward
// that lock, let go it comes back to the middle as fast, and the other arrow brings it straight
// back to the middle first, as Cannonball (Out Run's engine, ported) turns its wheel from keys.
// A d-pad's full lock at once would have the car across the road in half a second.
//
// It runs where the player's controls are sampled (worker.js), once a frame, and goes out in the
// input itself, bits 16-23, a signed byte: -127 full left to 127 full right, 0 (any input without
// one) straight ahead. So every machine that runs the frame (the other players', a watcher's, a
// rollback running it again) turns the game's wheel the same, whatever state it started from.

/** How far the wheel turns a frame: full lock (127) in 18 frames, 0.3 s. */
export const WHEEL_STEP = 7;
export const WHEEL_LOCK = 127;
const LEFT = 1 << 6;
const RIGHT = 1 << 7;

export class Wheel {
  /** Where the wheel is: -127 full left .. 127 full right. */
  position = 0;

  /** The next frame's input: `input`'s buttons, and the wheel turned by its arrows in bits 16-23. */
  turn(input) {
    const left = (input & LEFT) !== 0;
    const right = (input & RIGHT) !== 0;
    const at = this.position;
    if (left && !right) this.position = Math.max(-WHEEL_LOCK, Math.min(at, 0) - WHEEL_STEP);
    else if (right && !left) this.position = Math.min(WHEEL_LOCK, Math.max(at, 0) + WHEEL_STEP);
    else this.position = at > 0 ? Math.max(0, at - WHEEL_STEP) : Math.min(0, at + WHEEL_STEP);
    return ((input & 0xffff) | ((this.position & 0xff) << 16)) >>> 0;
  }
}

/** The wheel in an input: bits 16-23, signed. */
export const wheelIn = (input) => (input << 8) >> 24;
