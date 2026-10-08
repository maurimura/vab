import assert from 'node:assert/strict';
import { test } from 'node:test';
import { WHEEL_LOCK, WHEEL_STEP, Wheel, wheelIn } from '../web/emulator/wheel.js';

const LEFT = 1 << 6;
const RIGHT = 1 << 7;
const UP = 1 << 4;

test('held, an arrow turns the wheel a step a frame up to full lock; let go, it comes back', () => {
  const wheel = new Wheel();
  const turns = [];
  for (let frame = 0; frame < 20; frame++) turns.push(wheelIn(wheel.turn(LEFT | UP)));
  assert.equal(turns[0], -WHEEL_STEP);
  assert.equal(turns[1], -2 * WHEEL_STEP);
  assert.equal(turns.at(-1), -WHEEL_LOCK);
  assert.ok(turns.indexOf(-WHEEL_LOCK) < 19, 'full lock within 0.3 s');
  let frames = 0;
  while (wheelIn(wheel.turn(UP)) !== 0) frames++;
  assert.equal(frames + 1, Math.ceil(WHEEL_LOCK / WHEEL_STEP));
});

test('the other arrow brings the wheel back through the middle at once', () => {
  const wheel = new Wheel();
  for (let frame = 0; frame < 10; frame++) wheel.turn(RIGHT);
  assert.ok(wheel.position > 0);
  assert.equal(wheelIn(wheel.turn(LEFT)), -WHEEL_STEP);
  // Both arrows at once: straightening, as with neither.
  for (let frame = 0; frame < 3; frame++) wheel.turn(LEFT);
  const before = wheel.position;
  assert.equal(wheelIn(wheel.turn(LEFT | RIGHT)), before + WHEEL_STEP);
});

test("the wheel rides in the input's bits 16-23, the buttons untouched", () => {
  const wheel = new Wheel();
  const buttons = 0xffff & ~RIGHT;
  const input = wheel.turn(buttons | 0xabcd0000);
  assert.equal(input & 0xffff, buttons);
  assert.equal(wheelIn(input), -WHEEL_STEP);
  assert.equal(input >>> 24, 0);
  assert.equal(wheelIn(0), 0);
  assert.equal(wheelIn(127 << 16), 127);
  assert.equal(wheelIn(0x81 << 16), -127);
});
