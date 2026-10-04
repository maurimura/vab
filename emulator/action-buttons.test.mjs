import assert from 'node:assert/strict';
import { test } from 'node:test';
import { actionButtons } from './action-buttons.mjs';

const navigation = [[2, 'Coin'], [3, 'Start'], [4, 'Up'], [5, 'Down'], [6, 'Left'], [7, 'Right']];

test('MK named actions retain driver order, with High Punch first for CMOS confirmation', () => {
  const actions = [[1, 'High Punch'], [10, 'Block'], [9, 'High Kick'], [0, 'Low Punch'], [8, 'Low Kick'], [11, 'Run']];
  assert.deepEqual(actionButtons(new Map([...navigation, ...actions])), actions);
});

test('Neo Geo keeps its four actions and optional combination buttons', () => {
  const actions = [[0, 'Button A'], [1, 'Button B'], [8, 'Button C'], [9, 'Button D'], [10, 'Buttons AB'], [11, 'Buttons CD'], [12, 'Buttons ABC'], [13, 'Buttons BCD']];
  assert.deepEqual(actionButtons(new Map([...navigation, ...actions])), actions);
});

test('actions do not depend on English labels and never include system controls', () => {
  const buttons = new Map([...navigation, [1, 'Puñetazo alto'], [14, 'Service'], [15, 'Reset']]);
  assert.deepEqual(actionButtons(buttons), [[1, 'Puñetazo alto']]);
  assert.equal(buttons.size, 9);
});

test('joystick-only games have no action buttons', () => {
  assert.deepEqual(actionButtons(new Map(navigation)), []);
});
