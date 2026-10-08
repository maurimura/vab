import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Core } from '../web/emulator/libretro.js';

// What the frontend answers when a core asks about the controls (retro_input_state), with a
// stand-in for an Emscripten core: just enough memory for the input descriptors.
const JOYPAD = 1;
const LIGHTGUN = 4;
const POINTER = 6;
const GUN = { SCREEN_X: 13, SCREEN_Y: 14, IS_OFFSCREEN: 15, TRIGGER: 2, RELOAD: 16, AUX_A: 3, AUX_B: 4, AUX_C: 8, START: 6, SELECT: 7, UP: 9, DOWN: 10, LEFT: 11, RIGHT: 12 };
const SET_INPUT_DESCRIPTORS = 11;
const bit = (id) => 1 << id;
/** A port's input: RetroPad buttons, and where it aims (0..255 across and down). */
const input = (buttons, x = 0, y = 0) => (buttons | (x << 16) | (y << 24)) >>> 0;

async function core({ gun = false, descriptors = [] } = {}) {
  const heap = new DataView(new ArrayBuffer(1 << 16));
  const functions = [];
  let inputState;
  let environment;
  const module = {
    FS: { mkdirTree() {} },
    addFunction: (fn) => functions.push(fn) - 1,
    getValue: (at, type) => (type === 'i32' ? heap.getInt32(at, true) : heap.getUint8(at)),
    UTF8ToString(at) {
      let text = '';
      for (let c; (c = heap.getUint8(at++)); ) text += String.fromCharCode(c);
      return text;
    },
    _retro_set_environment: (fn) => (environment = functions[fn]),
    _retro_set_video_refresh() {},
    _retro_set_audio_sample() {},
    _retro_set_audio_sample_batch() {},
    _retro_set_input_poll() {},
    _retro_set_input_state: (fn) => (inputState = functions[fn]),
    _retro_init() {},
  };
  const it = await Core.create(async () => module, { onFrame() {}, onAudio() {} });
  it.gun = gun;
  // struct retro_input_descriptor { port, device, index, id; description } [], then a blank one.
  let strings = 0x8000;
  descriptors.forEach(([port, device, id, name], i) => {
    const at = i * 20;
    [port, device, 0, id].forEach((value, field) => heap.setInt32(at + field * 4, value, true));
    heap.setInt32(at + 16, strings, true);
    for (const c of name) heap.setUint8(strings++, c.charCodeAt(0));
    heap.setUint8(strings++, 0);
  });
  environment(SET_INPUT_DESCRIPTORS, 0);
  it.ask = (port, device, id) => inputState(port, device, 0, id);
  return it;
}

test('the RetroPad reads only the low 16 bits, whatever aim is above them', async () => {
  const pad = await core();
  pad.inputs[0] = input(bit(0) | bit(8), 255, 255);
  pad.inputs[1] = input(bit(3), 17, 200);
  for (let id = 0; id < 16; id++) {
    assert.equal(pad.ask(0, JOYPAD, id), id === 0 || id === 8 ? 1 : 0);
    assert.equal(pad.ask(1, JOYPAD, id), id === 3 ? 1 : 0);
  }
  // Ids past the RetroPad's 16 buttons (or the whole-mask query, 256) are not the aim.
  for (const id of [16, 23, 24, 31, 256]) assert.equal(pad.ask(0, JOYPAD, id), 0);
});

test('a game without a gun answers nothing for one, so MAME presses no button twice', async () => {
  const pad = await core();
  pad.inputs[0] = input(0xffff, 255, 255);
  for (let id = 0; id <= 16; id++) {
    assert.equal(pad.ask(0, LIGHTGUN, id), 0);
    assert.equal(pad.ask(0, POINTER, id), 0);
  }
});

test("a lightgun aims across the whole screen, libretro's -0x8000 to 0x7fff", async () => {
  const gun = await core({ gun: true });
  const at = (x, y) => {
    gun.inputs[0] = input(0, x, y);
    return [gun.ask(0, LIGHTGUN, GUN.SCREEN_X), gun.ask(0, LIGHTGUN, GUN.SCREEN_Y)];
  };
  assert.deepEqual(at(0, 0), [-0x8000, -0x8000]);
  assert.deepEqual(at(255, 255), [0x7fff, 0x7fff]);
  assert.deepEqual(at(255, 0), [0x7fff, -0x8000]);
  assert.deepEqual(at(128, 127), [128, -129]);
  let last = -Infinity;
  for (let x = 0; x < 256; x++) {
    const [screenX] = at(x, 0);
    assert.ok(screenX > last && screenX >= -0x8000 && screenX <= 0x7fff);
    last = screenX;
  }
  assert.equal(gun.ask(0, LIGHTGUN, GUN.IS_OFFSCREEN), 0);
});

test("a lightgun's buttons are the RetroPad buttons that press them, port by port", async () => {
  const gun = await core({ gun: true });
  const pressing = { TRIGGER: 0, AUX_A: 8, AUX_B: 9, RELOAD: 1, START: 3, SELECT: 2, UP: 4, DOWN: 5, LEFT: 6, RIGHT: 7 };
  for (const [button, id] of Object.entries(pressing)) {
    gun.inputs.fill(0);
    gun.inputs[1] = input(bit(id), 40, 50);
    for (const [other, otherId] of Object.entries(GUN)) {
      if (otherId === GUN.SCREEN_X || otherId === GUN.SCREEN_Y) continue;
      assert.equal(gun.ask(1, LIGHTGUN, otherId), other === button ? 1 : 0, `${button} -> ${other}`);
      assert.equal(gun.ask(0, LIGHTGUN, otherId), 0);
    }
  }
  gun.inputs[0] = input(0xffff, 9, 9);
  assert.equal(gun.ask(0, LIGHTGUN, GUN.AUX_C), 0);
  assert.equal(gun.ask(4, LIGHTGUN, GUN.TRIGGER), 0); // MAME asks for 8 ports; there are 4
  assert.equal(gun.ask(7, LIGHTGUN, GUN.SCREEN_X), 0);
});

test("as a pointer (MAME's touchscreen mode), the trigger is a finger and the pedal two", async () => {
  const gun = await core({ gun: true });
  gun.inputs[0] = input(0, 0, 255);
  assert.deepEqual([gun.ask(0, POINTER, 0), gun.ask(0, POINTER, 1)], [-0x7fff, 0x7fff]);
  assert.deepEqual([gun.ask(0, POINTER, 2), gun.ask(0, POINTER, 3)], [0, 0]);
  gun.inputs[0] = input(bit(0), 128, 128);
  assert.deepEqual([gun.ask(0, POINTER, 2), gun.ask(0, POINTER, 3)], [1, 1]);
  gun.inputs[0] = input(bit(8), 128, 128);
  assert.deepEqual([gun.ask(0, POINTER, 2), gun.ask(0, POINTER, 3)], [0, 2]);
});

test("a lightgun game's buttons take its gun's names, others keep the RetroPad's", async () => {
  const descriptors = [
    [0, JOYPAD, 0, 'B'],
    [0, JOYPAD, 8, 'A'],
    [0, JOYPAD, 3, 'Start'],
    [1, JOYPAD, 0, 'B'],
    [0, LIGHTGUN, GUN.TRIGGER, 'Gun Trigger'],
    [0, LIGHTGUN, GUN.AUX_A, 'Foot Pedal'],
    [1, LIGHTGUN, GUN.TRIGGER, 'P2 Trigger'],
  ];
  const gun = await core({ gun: true, descriptors });
  assert.deepEqual([...gun.buttons].sort(([a], [b]) => a - b), [[0, 'Gun Trigger'], [3, 'Start'], [8, 'Foot Pedal']]);
  const pad = await core({ descriptors });
  assert.deepEqual([...pad.buttons].sort(([a], [b]) => a - b), [[0, 'B'], [3, 'Start'], [8, 'A']]);
});
