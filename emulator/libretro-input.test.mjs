import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Core } from '../web/emulator/libretro.js';

// What the frontend answers when a core asks about the controls (retro_input_state), with a
// stand-in for an Emscripten core: just enough memory for the input descriptors.
const JOYPAD = 1;
const LIGHTGUN = 4;
const ANALOG = 5;
const POINTER = 6;
const GUN = { SCREEN_X: 13, SCREEN_Y: 14, IS_OFFSCREEN: 15, TRIGGER: 2, RELOAD: 16, AUX_A: 3, AUX_B: 4, AUX_C: 8, START: 6, SELECT: 7, UP: 9, DOWN: 10, LEFT: 11, RIGHT: 12 };
const SET_INPUT_DESCRIPTORS = 11;
const bit = (id) => 1 << id;
/** A port's input: RetroPad buttons, and where it aims (0..255 across and down). */
const input = (buttons, x = 0, y = 0) => (buttons | (x << 16) | (y << 24)) >>> 0;

async function core({ gun = false, wheel, descriptors = [] } = {}) {
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
  it.wheel = wheel;
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
  it.ask = (port, device, id, index = 0) => inputState(port, device, index, id);
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

/** Out Run's controls, as FBNeo names them. */
const OUT_RUN = [
  [0, JOYPAD, 2, 'Coin 1'],
  [0, JOYPAD, 3, 'Start 1'],
  [0, ANALOG, 0, 'Steering'],
  [0, JOYPAD, 13, 'Accelerate'],
  [0, JOYPAD, 12, 'Brake'],
  [0, JOYPAD, 0, 'Gear'],
  [0, JOYPAD, 6, 'Steering (Fake Digital Left)'],
  [0, JOYPAD, 7, 'Steering (Fake Digital Right)'],
];
/** Cruis'n USA's, as MAME names them (mame/patches/0002, 0007): no analog descriptor at all. */
const CRUISN = [
  [0, JOYPAD, 4, 'Accelerate'],
  [0, JOYPAD, 5, 'Brake'],
  [0, JOYPAD, 6, 'Steer left'],
  [0, JOYPAD, 7, 'Steer right'],
  [0, JOYPAD, 0, 'Shift Down'],
  [0, JOYPAD, 8, 'Shift Up'],
  [0, JOYPAD, 3, 'Start'],
  [0, JOYPAD, 2, 'Coin'],
];
/** Their wheels, as assets/games.ron has them and the page passes them on (JSON). */
const OUT_RUN_WHEEL = { lock: 0.3, back: 0.3, curve: 1, span: [10600, 23500] };
const CRUISN_WHEEL = { lock: 0.6, back: 0.1, curve: 2, span: [0, 32767] };
/** A wheel turned `turn` (-127..127) in an input's bits 16-23 (wheel.js). */
const turned = (buttons, turn) => (buttons | ((turn & 0xff) << 16)) >>> 0;
/** What the core reads on `car`'s left stick with its wheel at `turn`. */
const stick = (car, turn, port = 0) => {
  car.inputs[port] = turned(bit(6), turn);
  return car.ask(port, ANALOG, 0, 0);
};

test("a driving game's wheel is the left stick, from bits 16-23, past FBNeo's dead zones", async () => {
  const car = await core({ descriptors: OUT_RUN, wheel: OUT_RUN_WHEEL });
  assert.equal(car.steers(0), true);
  assert.equal(stick(car, 0), 0);
  // A turn's first step is already past the third of the stick FBNeo leaves dead...
  assert.ok(stick(car, -7) < -10600 && stick(car, 7) > 10600);
  assert.equal(stick(car, -7), -stick(car, 7));
  // ...and the rest goes on to full lock, 72% of it.
  assert.equal(stick(car, 127), 23500);
  assert.equal(stick(car, -127), -23500);
  for (let turn = 2; turn <= 127; turn++) {
    assert.ok(stick(car, turn) > stick(car, turn - 1));
    assert.equal(stick(car, -turn), -stick(car, turn));
  }
  // Only the left stick's X; the arrows reach the game only as the wheel, never as FBNeo's
  // "fake digital" full lock.
  car.inputs[0] = turned(bit(6) | bit(7), 50);
  assert.equal(car.ask(0, ANALOG, 1, 0), 0);
  assert.equal(car.ask(0, ANALOG, 0, 1), 0);
  assert.equal(car.ask(0, JOYPAD, 6), 0);
  assert.equal(car.ask(0, JOYPAD, 7), 0);
  // The pedals answer Up and Down.
  car.inputs[0] = bit(4);
  assert.deepEqual([car.ask(0, JOYPAD, 13), car.ask(0, JOYPAD, 12)], [1, 0]);
  car.inputs[0] = bit(5);
  assert.deepEqual([car.ask(0, JOYPAD, 13), car.ask(0, JOYPAD, 12)], [0, 1]);
});

test("on MAME the wheel is the whole stick, one to one, and MAME's own arrows never move it", async () => {
  const car = await core({ descriptors: CRUISN, wheel: CRUISN_WHEEL });
  // Every port steers (a player's wheel is on their seat's port); MAME asks for 8, there are 4.
  for (const port of [0, 1, 2, 3]) assert.equal(car.steers(port), true);
  assert.equal(car.steers(4), false);
  assert.equal(stick(car, 0), 0);
  // No dead zone: the first step turns MAME's paddle a step (0x7fff / 127 of the stick)...
  assert.equal(stick(car, 1), 258);
  assert.equal(stick(car, -1), -258);
  // ...evenly all the way to the stick's ends, the wheel's ends (0x10 and 0xf0 in MAME).
  for (let turn = 1; turn <= 127; turn++) {
    assert.equal(stick(car, turn), Math.round((turn * 0x7fff) / 127));
    assert.equal(stick(car, -turn), -stick(car, turn));
  }
  assert.equal(stick(car, 127), 0x7fff);
  assert.equal(stick(car, -127), -0x7fff);
  // Left and Right are only the wheel: patch 0007's key ramp for them never runs.
  car.inputs[0] = turned(bit(6) | bit(7), -90);
  assert.equal(car.ask(0, JOYPAD, 6), 0);
  assert.equal(car.ask(0, JOYPAD, 7), 0);
  // Up and Down stay MAME's gas and brake (the key ramp), Z and X the shifter.
  car.inputs[0] = bit(4) | bit(5) | bit(0) | bit(8);
  for (const id of [4, 5, 0, 8]) assert.equal(car.ask(0, JOYPAD, id), 1);
  assert.equal(car.ask(0, JOYPAD, 13), 0);
  // Another seat's port reads its own input's wheel.
  car.inputs[0] = 0;
  assert.equal(stick(car, -64, 1), -Math.round((64 * 0x7fff) / 127));
  assert.equal(car.ask(0, ANALOG, 0, 0), 0);
});

test("only a game with a wheel steers: the same controls without one keep the core's own arrows", async () => {
  for (const descriptors of [OUT_RUN, CRUISN]) {
    const car = await core({ descriptors });
    assert.equal(car.steers(0), false);
    car.inputs[0] = turned(bit(6), 100);
    assert.equal(car.ask(0, ANALOG, 0, 0), 0);
    assert.equal(car.ask(0, JOYPAD, 6), 1);
  }
  const pad = await core({ descriptors: [[0, JOYPAD, 0, 'B'], [0, JOYPAD, 6, 'Left'], [0, ANALOG, 0, 'Gun X']] });
  assert.equal(pad.steers(0), false);
  pad.inputs[0] = turned(bit(6), 100);
  assert.equal(pad.ask(0, ANALOG, 0, 0), 0);
  assert.equal(pad.ask(0, JOYPAD, 6), 1);
});

test("a driving game's arrows are named for what they do, for the help card", async () => {
  const car = await core({ descriptors: OUT_RUN, wheel: OUT_RUN_WHEEL });
  const names = Object.fromEntries(car.buttons);
  assert.equal(names[4], 'Accelerate');
  assert.equal(names[5], 'Brake');
  assert.equal(names[6], 'Steer left');
  assert.equal(names[7], 'Steer right');
  assert.equal(names[0], 'Gear');
  assert.equal(names[3], 'Start 1');
  // MAME names them so already; without a wheel, FBNeo's own names stay.
  const cruisn = Object.fromEntries((await core({ descriptors: CRUISN, wheel: CRUISN_WHEEL })).buttons);
  assert.deepEqual([cruisn[4], cruisn[5], cruisn[6], cruisn[7]], ['Accelerate', 'Brake', 'Steer left', 'Steer right']);
  const unsteered = Object.fromEntries((await core({ descriptors: OUT_RUN })).buttons);
  assert.equal(unsteered[6], 'Steering (Fake Digital Left)');
});
