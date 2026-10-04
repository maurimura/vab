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
