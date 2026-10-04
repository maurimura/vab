// Capture a real display from an already supplied local game, without downloading ROMs.
// node tools/art/capture_screen.mjs <core.mjs> <rom.zip> <output.png> <state|-> [--confirm[=count]] [--action=1] [--frames=600] [extra.zip ...]
import { readFileSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import { Core } from '../../web/emulator/libretro.js';

const [corePath, romPath, output, statePath, ...extraPaths] = process.argv.slice(2);
if (!corePath || !romPath || !output || !statePath) throw new Error('Expected core, ROM, output, state|-, then optional local BIOS/parent sets');
const { default: createModule } = await import(resolve(corePath));
let frame;
const core = await Core.create(createModule, {
  onFrame(rgba, width, height) { frame = { bytes: Buffer.from(rgba), width, height }; },
  onAudio() {},
});
const framesArg = extraPaths.find(path => path.startsWith('--frames='));
const frames = framesArg ? Number(framesArg.slice('--frames='.length)) : 600;
const confirmArg = extraPaths.find(path => path === '--confirm' || path.startsWith('--confirm='));
const confirmations = confirmArg ? (confirmArg === '--confirm' ? 1 : Number(confirmArg.slice('--confirm='.length))) : 0;
if (!Number.isInteger(frames) || frames < 1 || frames > 3600) throw new Error('frames must be an integer from 1 to 3600');
const actionArg = extraPaths.find(path => path.startsWith('--action='));
const actionIndex = actionArg ? Number(actionArg.slice('--action='.length)) : 1;
if (!Number.isInteger(confirmations) || confirmations < 0 || confirmations > 20) throw new Error('confirm count must be an integer from 0 to 20');
if (!Number.isInteger(actionIndex) || actionIndex < 1 || actionIndex > 6) throw new Error('action must be an integer from 1 to 6');
for (const path of extraPaths.filter(path => path !== '--confirm' && !path.startsWith('--confirm=') && !path.startsWith('--action=') && !path.startsWith('--frames='))) {
  core.addFile(basename(path), readFileSync(path));
}
const av = core.loadGame(basename(romPath), readFileSync(romPath));
if (statePath !== '-') core.unserialize(readFileSync(statePath));
else for (let i = 0; i < Math.round(av.fps * 15); i++) core.run();
core.inputs[0] = 1 << 3;
for (let i = 0; i < 15; i++) core.run();
core.inputs[0] = 0;
if (confirmations) {
  // Tetris has one selector; NBA Jam needs repeated confirms for initials/team.
  const action = [...core.buttons].filter(([, label]) => /rotate|fire|button/i.test(label))[actionIndex - 1];
  if (!action) throw new Error('Selected action button descriptor not found to confirm selection');
  for (let step = 0; step < confirmations; step++) {
    for (let i = 0; i < 120; i++) core.run();
    core.inputs[0] = 1 << action[0];
    for (let i = 0; i < 15; i++) core.run();
    core.inputs[0] = 0;
  }
}
for (let i = 0; i < frames; i++) core.run();
// Libretro reports pre-rotation geometry; vertical games rotate in onFrame.
const matchesGeometry = frame && (
  (frame.width === av.width && frame.height === av.height) ||
  (frame.width === av.height && frame.height === av.width)
);
if (!matchesGeometry || !core.systemRam().length) {
  throw new Error('Core did not produce a valid game display; inspect ROM compatibility');
}

// Minimal RGBA PNG writer: Node's built-in zlib, no image-library dependency.
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, bytes) {
  const name = Buffer.from(type);
  const size = Buffer.alloc(4); size.writeUInt32BE(bytes.length);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(Buffer.concat([name, bytes])));
  return Buffer.concat([size, name, bytes, crc]);
}
const header = Buffer.alloc(13);
header.writeUInt32BE(frame.width, 0); header.writeUInt32BE(frame.height, 4);
header[8] = 8; header[9] = 6;
const stride = frame.width * 4;
const rows = Buffer.alloc((stride + 1) * frame.height);
for (let row = 0; row < frame.height; row++) frame.bytes.copy(rows, row * (stride + 1) + 1, row * stride, (row + 1) * stride);
writeFileSync(output, Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk('IHDR', header), chunk('IDAT', deflateSync(rows)), chunk('IEND', Buffer.alloc(0)),
]));
console.log(JSON.stringify({ output, width: frame.width, height: frame.height, buttons: [...core.buttons.values()] }));
