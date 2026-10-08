import assert from 'node:assert/strict';
import { test } from 'node:test';

// What a watched game's frames look like through the room (web/room.js): a cabinet's inputs
// (WATCH_INPUTS, a u32 per port per frame) and a linked board's frames (WATCH_LINK, each
// numbered, with its link bytes), sent by one Room and read back by another, with a stand-in
// for the WebSocket that just keeps what's sent.
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
