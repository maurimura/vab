import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Core, romPath } from '../web/emulator/libretro.js';

// Where the files a game needs land in the core's file system (Core.addFile, romPath), as the
// worker puts them there from the start's `files` before loading the ROM set: a BIOS set next to
// it, a NAOMI game's disc in a folder named after it (where Flycast looks). The core is a
// stand-in for an Emscripten one, with a file system that, like Emscripten's, only writes a file
// into a folder that's there, and just enough memory for retro_load_game's path.

/** An Emscripten file system in a Map: path -> bytes, or null for a folder. */
function memfs() {
  const nodes = new Map([['/', null]]);
  const parent = (path) => path.slice(0, path.lastIndexOf('/')) || '/';
  return {
    nodes,
    mkdirTree(path) {
      let at = '';
      for (const part of path.split('/').filter(Boolean)) {
        at += `/${part}`;
        if (nodes.get(at) !== undefined && nodes.get(at) !== null) throw new Error(`${at}: a file`);
        nodes.set(at, null);
      }
    },
    writeFile(path, bytes) {
      if (nodes.get(parent(path)) !== null) throw new Error(`${path}: no such folder`);
      nodes.set(path, bytes);
    },
  };
}

async function core() {
  const FS = memfs();
  const heap = new ArrayBuffer(1 << 16);
  const bytes = new Uint8Array(heap);
  const view = new DataView(heap);
  let top = 8;
  const loaded = [];
  const module = {
    FS,
    HEAPU8: bytes,
    HEAPU32: new Uint32Array(heap),
    _malloc: (size) => (top = Math.ceil(top / 8) * 8 + size) - size,
    _free() {},
    addFunction: () => 0,
    setValue: (at, value) => view.setInt32(at, value, true),
    getValue: (at, type) => (type === 'double' ? view.getFloat64(at, true) : type === 'float' ? view.getFloat32(at, true) : view.getInt32(at, true)),
    lengthBytesUTF8: (text) => Buffer.byteLength(text),
    stringToUTF8: (text, at) => bytes.set([...Buffer.from(text), 0], at),
    UTF8ToString(at) {
      let text = '';
      for (let c; (c = bytes[at++]); ) text += String.fromCharCode(c);
      return text;
    },
    _retro_set_environment() {},
    _retro_set_video_refresh() {},
    _retro_set_audio_sample() {},
    _retro_set_audio_sample_batch() {},
    _retro_set_input_poll() {},
    _retro_set_input_state() {},
    _retro_init() {},
    // struct retro_game_info: the path first.
    _retro_load_game: (info) => loaded.push(module.UTF8ToString(view.getInt32(info, true))),
    _retro_set_controller_port_device() {},
    _retro_get_system_av_info: (av) => {
      view.setFloat64(av + 24, 59.94, true);
      view.setFloat64(av + 32, 44100, true);
    },
  };
  const it = await Core.create(async () => module, { onFrame() {}, onAudio() {} });
  return { it, files: FS.nodes, loaded };
}

/** What the worker does with a start's `rom` and `files`, downloaded (web/emulator/worker.js). */
function start(it, rom, files) {
  files.forEach((url, i) => it.addFile(romPath(url), Uint8Array.of(i)));
  return it.loadGame(rom.split('/').pop(), Uint8Array.of(0xff));
}

/** The files under /roms, path -> the first byte (which download it was). */
const roms = (files) =>
  Object.fromEntries([...files].filter(([path, bytes]) => path.startsWith('/roms/') && bytes).map(([path, bytes]) => [path, bytes[0]]));

test('a file goes where the site serves it under /roms/, folders and all', () => {
  assert.equal(romPath('/roms/neogeo.zip'), 'neogeo.zip');
  assert.equal(romPath('/roms/vtennisg/gds-0011.chd'), 'vtennisg/gds-0011.chd');
  assert.equal(romPath('https://vab.example.workers.dev/roms/vtennisg/gds-0011.chd'), 'vtennisg/gds-0011.chd');
  // A check's file:// URLs: by name, unless they come from a folder of ROMs laid out as R2 is.
  assert.equal(romPath('file:///Users/m/Downloads/neogeo.zip'), 'neogeo.zip');
  assert.equal(romPath('file:///Users/m/vab/flycast/.cache/roms/vtennisg/gds-0011.chd'), 'vtennisg/gds-0011.chd');
});

test('a NAOMI game: the ROM set in /roms, its disc in the folder named after it', async () => {
  const { it, files, loaded } = await core();
  const { fps, sampleRate } = start(it, '/roms/vtennisg.zip', ['/roms/vtennisg/gds-0011.chd']);
  assert.deepEqual(roms(files), { '/roms/vtennisg/gds-0011.chd': 0, '/roms/vtennisg.zip': 0xff });
  assert.equal(files.get('/roms/vtennisg'), null); // a folder
  assert.deepEqual(loaded, ['/roms/vtennisg.zip']);
  assert.deepEqual([fps, sampleRate], [59.94, 44100]);
});

test('a BIOS set still lands next to the ROM set, as before', async () => {
  const { it, files, loaded } = await core();
  start(it, '/roms/mslug.zip', ['/roms/neogeo.zip']);
  assert.deepEqual(roms(files), { '/roms/neogeo.zip': 0, '/roms/mslug.zip': 0xff });
  assert.deepEqual(loaded, ['/roms/mslug.zip']);
});

test('a BIOS and a disc together, and deeper folders, each made once', async () => {
  const { it, files } = await core();
  start(it, '/roms/game.zip', ['/roms/naomi.zip', '/roms/game/gds-0001.chd', '/roms/game/disc2/track.bin']);
  assert.deepEqual(roms(files), {
    '/roms/naomi.zip': 0,
    '/roms/game/gds-0001.chd': 1,
    '/roms/game/disc2/track.bin': 2,
    '/roms/game.zip': 0xff,
  });
  // Again into folders that are there already: Emscripten's mkdirTree takes that, and so must we.
  it.addFile('game/gds-0001.chd', Uint8Array.of(7));
  assert.equal(files.get('/roms/game/gds-0001.chd')[0], 7);
});
