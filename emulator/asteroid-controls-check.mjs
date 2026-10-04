// Prove Asteroids' seat-two Start/Coin reach the driver's shared input bank.
// node emulator/asteroid-controls-check.mjs <classics/fbneo.mjs> <asteroid.zip> <asteroid.state>
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { Core } from '../web/emulator/libretro.js';

const [corePath, romPath, statePath] = process.argv.slice(2);
if (!corePath || !romPath || !statePath) throw new Error('Expected core, local ROM and startup state');
const { default: create } = await import(resolve(corePath));
const core = await Core.create(create, { onFrame() {}, onAudio() {} });
core.netplay = true;
core.turns = true;
core.loadGame(basename(romPath), readFileSync(romPath));
core.unserialize(readFileSync(statePath));
// FBNeo publishes the final descriptors on its deferred first frame.
for (let i = 0; i < 30; i++) core.run();
assert.equal(core.buttons.get(15), '2P Start');
const before = core.serialize();
const play = (port, id) => {
  core.unserialize(before);
  core.inputs.fill(0);
  core.inputs[port] = 1 << id;
  for (let i = 0; i < 15; i++) core.run();
  core.inputs.fill(0);
  for (let i = 0; i < 30; i++) core.run();
  return Buffer.from(core.systemRam());
};
const twoPlayers = play(0, 15); // Driver's native player-one R3 alias.
assert.deepEqual(play(1, 3), twoPlayers, 'Seat-two Start must act exactly like native 2P Start');
assert.notDeepEqual(play(0, 3), twoPlayers, 'One- and two-player Start must select different games');
assert.deepEqual(play(1, 2), play(0, 2), 'Either seat must insert into the same coin chute');
assert.deepEqual(play(1, 7), play(0, 7), 'Either seat must rotate with the same shared panel');
console.log(JSON.stringify({ p2StartSelectsTwoPlayers: true, sharedCoinWorks: true, sharedPanelWorks: true }));
