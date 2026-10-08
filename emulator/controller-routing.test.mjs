import assert from 'node:assert/strict';
import { test } from 'node:test';
import { controllerRouting, routedButton } from '../web/emulator/controller-routing.js';
const bit = id => 1 << id;
const ordinary = controllerRouting([new Map([[2, 'Coin'], [3, 'Start']]), new Map([[2, 'Coin'], [3, 'Start']])]);
const asteroid = controllerRouting([new Map([[2, 'Coin'], [3, 'Start'], [15, '2P Start']]), new Map()]);

test('Asteroids seat two Start presses the actual two-player Start, not player one Start', () => {
  const inputs = [0, bit(3), 0, 0];
  assert.equal(routedButton(inputs, 0, 15, asteroid, true), 1);
  assert.equal(routedButton(inputs, 0, 3, asteroid, true), 0);
  assert.equal(routedButton(inputs, 0, 15, ordinary, true), 0);
});

test('a shared coin chute accepts either seat, without changing ordinary independent coins', () => {
  const inputs = [0, bit(2), 0, 0];
  assert.equal(routedButton(inputs, 0, 2, asteroid, true), 1);
  assert.equal(routedButton(inputs, 0, 2, ordinary, true), 0);
  assert.equal(routedButton(inputs, 1, 2, ordinary, true), 1);
});

test('shared gameplay reaches both input banks, but Start and Coin remain independent', () => {
  const inputs = [bit(6) | bit(2), bit(0) | bit(3), 0, 0];
  for (const port of [0, 1]) {
    assert.equal(routedButton(inputs, port, 6, ordinary, true), 1);
    assert.equal(routedButton(inputs, port, 0, ordinary, true), 1);
  }
  assert.equal(routedButton(inputs, 0, 3, ordinary, true), 0);
  assert.equal(routedButton(inputs, 1, 2, ordinary, true), 0);
});

test('ordinary simultaneous and third/fourth-seat controls are unchanged', () => {
  const inputs = [bit(6), bit(0), bit(8), bit(11)];
  const copy = [...inputs];
  for (let port = 0; port < 4; port++) {
    for (let id = 0; id < 16; id++) assert.equal(routedButton(inputs, port, id, ordinary), (inputs[port] >> id) & 1);
  }
  assert.equal(routedButton(inputs, 2, 6, ordinary, true), 0);
  assert.equal(routedButton(inputs, 3, 11, ordinary, true), 1);
  assert.deepEqual(inputs, copy);
});

test('a lightgun aim in the high 16 bits never reaches the RetroPad, shared or not', () => {
  const inputs = Uint32Array.of(bit(6) | 0xffff0000, bit(0) | bit(3) | 0x80400000, 0, 0);
  for (let id = 0; id < 16; id++) {
    assert.equal(routedButton(inputs, 0, id, ordinary, true), id === 6 || id === 0 ? 1 : 0);
    assert.equal(routedButton(inputs, 1, id, ordinary), id === 0 || id === 3 ? 1 : 0);
  }
  assert.equal(routedButton(inputs, 0, 15, asteroid, true), 1);
});

// FBNeo's driving games (Out Run): Accelerate on R2, Brake on L2, the wheel on the left stick.
const outrun = controllerRouting(
  [new Map([[2, 'Coin 1'], [3, 'Start 1'], [13, 'Accelerate'], [12, 'Brake'], [0, 'Gear'], [6, 'Steering (Fake Digital Left)'], [7, 'Steering (Fake Digital Right)']])],
  [new Map([['0/0', 'Steering']])],
);

test('a driving game takes Up and Down as its pedals, on its own port only', () => {
  assert.deepEqual(outrun.pedals, [true]);
  assert.equal(routedButton([bit(4), 0, 0, 0], 0, 13, outrun), 1);
  assert.equal(routedButton([bit(4), 0, 0, 0], 0, 12, outrun), 0);
  assert.equal(routedButton([bit(5), 0, 0, 0], 0, 12, outrun), 1);
  assert.equal(routedButton([bit(5), 0, 0, 0], 0, 13, outrun), 0);
  assert.equal(routedButton([0, bit(4), 0, 0], 1, 13, outrun), 0);
  // A game that uses Up and Down keeps them, L2 and R2 or not.
  const fighter = controllerRouting([new Map([[4, 'Up'], [5, 'Down'], [12, 'L2'], [13, 'R2']])]);
  assert.deepEqual(fighter.pedals, [false]);
  assert.equal(routedButton([bit(4), 0, 0, 0], 0, 13, fighter), 0);
  assert.equal(routedButton([bit(4), 0, 0, 0], 0, 4, fighter), 1);
});

test("a driving game's arrows turn its wheel, never FBNeo's full-lock Left and Right", () => {
  assert.deepEqual(outrun.wheel, [true]);
  for (const id of [6, 7]) assert.equal(routedButton([bit(6) | bit(7), 0, 0, 0], 0, id, outrun), 0);
  assert.equal(routedButton([bit(0), 0, 0, 0], 0, 0, outrun), 1);
  // An analog stick without pedals (Terminator 2's gun) is no wheel: its arrows stay arrows.
  const gun = controllerRouting([new Map([[4, 'Gun Y'], [7, 'Gun X'], [0, 'Button 1']])], [new Map([['0/0', 'Gun X']])]);
  assert.deepEqual(gun.wheel, [false]);
  assert.equal(routedButton([bit(7), 0, 0, 0], 0, 7, gun), 1);
});
