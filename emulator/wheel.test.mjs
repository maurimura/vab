import assert from 'node:assert/strict';
import { test } from 'node:test';
import { WHEEL_LOCK, Wheel, wheelIn } from '../web/emulator/wheel.js';

const LEFT = 1 << 6;
const RIGHT = 1 << 7;
const UP = 1 << 4;
/** assets/games.ron's wheels, at the cores' frame rates. */
const OUT_RUN = { lock: 0.3, back: 0.3, curve: 1 };
const CRUISN = { lock: 0.6, back: 0.1, curve: 2 };
const CRUISN_FPS = 57.9;

/** The wheel, frame by frame, with `input` held for `frames` frames. */
const hold = (wheel, input, frames) => Array.from({ length: frames }, () => wheelIn(wheel.turn(input)));

test("Out Run's wheel turns evenly, full lock in 0.3 s held, and comes back as fast", () => {
  const wheel = new Wheel(OUT_RUN, 60);
  const turns = hold(wheel, LEFT | UP, 20);
  assert.equal(turns[0], -7);
  assert.equal(turns[1], -14);
  // A step a frame of 7 or 8 (7.06 on average), then held at full lock.
  turns.slice(1, 18).forEach((turn, i) => assert.ok([7, 8].includes(turns[i] - turn), `${turns[i]} -> ${turn}`));
  assert.equal(turns.indexOf(-WHEEL_LOCK), 17, 'full lock at 18 frames, 0.3 s');
  assert.deepEqual(turns.slice(17), [-127, -127, -127]);
  const back = hold(wheel, UP, 20);
  assert.equal(back[0], -120);
  assert.equal(back.indexOf(0), 17, 'back in 18 frames');
});

test("Cruis'n USA's wheel barely turns for a tap and speeds up toward full lock", () => {
  const wheel = new Wheel(CRUISN, CRUISN_FPS);
  const turns = hold(wheel, RIGHT, 40);
  // Along the curve: after n frames, (n / (0.6 s * fps))^2 of full lock.
  turns.forEach((turn, i) => {
    const along = Math.min(1, (i + 1) / (CRUISN.lock * CRUISN_FPS));
    assert.equal(turn, Math.round(WHEEL_LOCK * along ** 2), `frame ${i + 1}`);
  });
  // A 6-frame tap (0.1 s): 4 of 127, where Out Run's even ramp would be at 42.
  assert.equal(turns[5], 4);
  // Half the time, a quarter of the way; full lock at 35 frames (0.6 s), held there.
  assert.equal(turns[16], Math.round(WHEEL_LOCK * (17 / 34.74) ** 2));
  assert.equal(turns.indexOf(WHEEL_LOCK), 34);
  assert.equal(turns.at(-1), WHEEL_LOCK);
  // Each step bigger than the last (but for rounding), up to the last one, short of the lock.
  for (let i = 1; i < 34; i++) assert.ok(turns[i] - turns[i - 1] >= turns[i - 1] - (turns[i - 2] ?? 0) - 1, 'speeds up');
  // Let go, it comes straight back, evenly: 0.1 s, 6 frames.
  const back = hold(wheel, 0, 8);
  assert.deepEqual(back, [105, 83, 61, 39, 17, 0, 0, 0]);
  // The other way, the same.
  const left = new Wheel(CRUISN, CRUISN_FPS);
  assert.deepEqual(hold(left, LEFT, 40), turns.map((turn) => 0 - turn || 0));
});

test('the other arrow brings the wheel back through the middle at once', () => {
  for (const [ramp, fps] of [[OUT_RUN, 60], [CRUISN, CRUISN_FPS]]) {
    const wheel = new Wheel(ramp, fps);
    const first = hold(new Wheel(ramp, fps), LEFT, 1)[0];
    hold(wheel, RIGHT, 10);
    assert.ok(wheel.position > 0);
    // From the middle on, as if it had just started from there.
    assert.equal(wheelIn(wheel.turn(LEFT)), first);
    // Both arrows at once: straightening, as with neither.
    hold(wheel, LEFT, 3);
    const before = wheel.position;
    wheel.turn(LEFT | RIGHT);
    assert.ok(Math.abs(wheel.position - Math.min(0, before + WHEEL_LOCK / (ramp.back * fps))) < 1e-9);
  }
});

test('held again on its way back, the wheel goes on from where it is, along the curve', () => {
  const wheel = new Wheel(CRUISN, CRUISN_FPS);
  hold(wheel, RIGHT, 40);
  hold(wheel, 0, 2); // 127 -> 83.1, coming back
  const at = wheel.position;
  assert.ok(at > 80 && at < 86, `${at}`);
  // The frame after a hold that got it there: not from the middle again.
  const held = CRUISN.lock * Math.sqrt(at / WHEEL_LOCK);
  const next = WHEEL_LOCK * ((held + 1 / CRUISN_FPS) / CRUISN.lock) ** 2;
  wheel.turn(RIGHT);
  assert.ok(Math.abs(wheel.position - next) < 1e-9);
  assert.ok(wheel.position - at > 6, 'well up the curve: a bigger step than a fresh tap');
  // Back to full lock in the rest of the time a hold from the middle would take.
  const frames = hold(wheel, RIGHT, 20).indexOf(WHEEL_LOCK) + 2;
  assert.equal(frames, 35 - Math.round(held * CRUISN_FPS));
});

test('a new ramp takes over from the next frame, the wheel where it was', () => {
  const wheel = new Wheel(CRUISN, CRUISN_FPS);
  hold(wheel, LEFT, 20);
  const at = wheel.position;
  wheel.set({ lock: 0.3, curve: 1 });
  assert.equal(wheel.position, at);
  assert.deepEqual([wheel.lock, wheel.back, wheel.curve, wheel.fps], [0.3, 0.1, 1, CRUISN_FPS]);
  // Evenly now, from where it was: 127 over 0.3 s, 7.3 a frame.
  wheel.turn(LEFT);
  assert.ok(Math.abs(wheel.position - (at - WHEEL_LOCK / (0.3 * CRUISN_FPS))) < 1e-9);
  // Out Run's 60 frames a second, or any game's: the ramp keeps its seconds.
  wheel.set({}, 60);
  assert.deepEqual([wheel.lock, wheel.fps], [0.3, 60]);
  // No lock time: full lock at once, as a d-pad; no time back: the middle at once.
  wheel.set({ lock: 0, back: 0 });
  assert.equal(wheelIn(wheel.turn(RIGHT)), WHEEL_LOCK);
  assert.equal(wheelIn(wheel.turn(0)), 0);
});

test("the wheel rides in the input's bits 16-23, the buttons untouched", () => {
  const wheel = new Wheel(OUT_RUN, 60);
  const buttons = 0xffff & ~RIGHT;
  const input = wheel.turn(buttons | 0xabcd0000);
  assert.equal(input & 0xffff, buttons);
  assert.equal(wheelIn(input), -7);
  assert.equal(input >>> 24, 0);
  assert.equal(wheelIn(0), 0);
  assert.equal(wheelIn(127 << 16), 127);
  assert.equal(wheelIn(0x81 << 16), -127);
  // Whole steps only, the same each way.
  const right = new Wheel(CRUISN, CRUISN_FPS);
  const left = new Wheel(CRUISN, CRUISN_FPS);
  for (let frame = 0; frame < 12; frame++) assert.equal(wheelIn(right.turn(RIGHT)) + wheelIn(left.turn(LEFT)), 0);
});
