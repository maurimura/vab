import assert from 'node:assert/strict';
import { test } from 'node:test';

// What a watched game's frames look like through the room (web/room.js): a cabinet's inputs
// (WATCH_INPUTS, a u32 per port per frame), an arcade cabinet's frames (WATCH_FRAMES) and a
// linked board's frames (WATCH_LINK, each numbered, with its link bytes), sent by one Room and
// read back by another, with a stand-in for the WebSocket that just keeps what's sent.
class FakeSocket {
  static OPEN = 1;
  static last;
  readyState = 1;
  sent = [];
  constructor() {
    FakeSocket.last = this;
  }
  send(message) {
    this.sent.push(message);
  }
}
globalThis.WebSocket = FakeSocket;
const { Room } = await import('../web/room.js');
const { decodeRecords, encodeRecords } = await import('../web/emulator/linked.js');

/** A room, its socket, and what it heard. */
function room() {
  const heard = [];
  const events = new Proxy({}, { get: (_, name) => (...args) => heard.push([name, ...args]) });
  const it = new Room('ws://test/ws/main', events);
  return { it, socket: FakeSocket.last, heard };
}

/** A message as the room relays it: the sender's id in place of the address. */
function relayed(message, from) {
  const bytes = new Uint8Array(message).slice();
  new DataView(bytes.buffer).setUint32(0, from, true);
  return bytes.buffer;
}

test('a linked board\'s frames reach the watchers as they were encoded', () => {
  const sender = room();
  const watcher = room();
  const records = [
    { frame: 120, input: (1 | (40 << 16) | (200 << 24)) >>> 0, bytes: Uint8Array.of(0, 4, 1, 2, 3, 4) },
    { frame: 121, input: 0, bytes: Uint8Array.of() },
  ];
  sender.it.watchLink(3, encodeRecords(records));
  const [message] = sender.socket.sent;
  assert.equal(new DataView(message.buffer).getUint32(0, true), 0); // to everyone watching
  watcher.socket.onmessage({ data: relayed(message, 42) });
  const [[event, from, stream, got]] = watcher.heard;
  assert.deepEqual([event, from, stream], ['watchLink', 42, 3]);
  assert.deepEqual(decodeRecords(got).map((r) => [r.frame, r.input, Array.from(r.bytes)]),
    records.map((r) => [r.frame, r.input, Array.from(r.bytes)]));
});

test('a cabinet\'s inputs still go as a u32 per port per frame', () => {
  const sender = room();
  const watcher = room();
  const inputs = Uint32Array.of(1, 2, 3, 0xffffffff, 5, 6, 7, 8);
  sender.it.watchInputs(9, 300, inputs);
  watcher.socket.onmessage({ data: relayed(sender.socket.sent[0], 7) });
  const [[event, from, stream, frame, got]] = watcher.heard;
  assert.deepEqual([event, from, stream, frame], ['watchInputs', 7, 9, 300]);
  assert.deepEqual(Array.from(got), Array.from(inputs));
});

test('an arcade cabinet\'s frames and a linked board\'s arrive as what they are', () => {
  const sender = room();
  const watcher = room();
  sender.it.watchFrames(5, 11, 600, Uint8Array.of(1, 2, 3));
  sender.it.watchLink(12, encodeRecords([{ frame: 600, input: 0, bytes: Uint8Array.of(9) }]));
  const [frames, link] = sender.socket.sent;
  assert.notEqual(frames[4], link[4]); // their kinds
  assert.equal(new DataView(frames.buffer).getUint32(0, true), 5); // to that watcher only
  watcher.socket.onmessage({ data: relayed(frames, 8) });
  watcher.socket.onmessage({ data: relayed(link, 8) });
  const [[event, from, stream, frame, bytes], [other]] = watcher.heard;
  assert.deepEqual([event, from, stream, frame, Array.from(bytes)], ['watchFrames', 8, 11, 600, [1, 2, 3]]);
  assert.equal(other, 'watchLink');
});
