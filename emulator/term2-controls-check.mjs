// Verify both Terminator 2 guns respond to the existing digital pad, with independent
// saved positions and deterministic replay. No mouse coordinates or wider netplay packets.
// node emulator/term2-controls-check.mjs <midway/fbneo.mjs> <term2.zip> <term2.state>
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { Core } from '../web/emulator/libretro.js';

const [corePath, romPath, statePath] = process.argv.slice(2);
if (!corePath || !romPath || !statePath) throw new Error('Expected core, local ROM and startup state');
const { default: create } = await import(resolve(corePath));
const core = await Core.create(create, { onFrame() {}, onAudio() {} });
core.netplay = true;
core.loadGame(basename(romPath), readFileSync(romPath));
core.unserialize(readFileSync(statePath));
// Start both players and wait through the intro so game RAM observes both guns.
core.inputs[0] = core.inputs[1] = 1 << 3;
for (let i = 0; i < 15; i++) core.run();
core.inputs.fill(0);
for (let i = 0; i < 1500; i++) core.run();
assert.match(core.buttons.get(7), /Gun X/);
assert.match(core.buttons.get(5), /Gun Y/);
assert.equal(core.buttons.get(0), 'Button 1');
assert.equal(core.buttons.get(8), 'Button 2');
const before = core.serialize();
const play = (port, mask) => {
  core.unserialize(before);
  core.inputs.fill(0);
  core.inputs[port] = mask;
  for (let i = 0; i < 30; i++) core.run();
  // Rendering/audio caches in whole savestates can vary; compare machine RAM,
  // as the established rollback and spectator checks do.
  return Buffer.from(core.systemRam());
};
const neutral = play(0, 0);
assert.deepEqual(play(0, 0), neutral, 'Neutral machine RAM replay must match');
const p1 = play(0, 1 << 7);
const p2 = play(1, 1 << 7);
assert.notDeepEqual(p1, neutral, 'P1 digital aim must move the saved gun');
assert.notDeepEqual(p2, neutral, 'P2 digital aim must move the saved gun');
assert.notDeepEqual(p1, p2, 'The two controllers must move independent guns');
assert.deepEqual(play(0, 1 << 7), p1, 'Gun movement must replay identically after loading');
assert.deepEqual(play(1, 1 << 7), p2, 'P2 gun movement must replay identically after loading');
assert.notDeepEqual(play(0, 1 << 5), neutral, 'Vertical aiming must move the saved gun');
console.log(JSON.stringify({ p1AimMoves: true, p2AimMoves: true, independentGuns: true, savedAimReplayMatches: true }));
