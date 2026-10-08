import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  LinkedSession,
  decodeRecords,
  encodeRecords,
  makeLinkState,
  markLinked,
  parseLinkState,
  unmarkLinked,
} from '../web/emulator/linked.js';

// Linked boards (web/emulator/linked.js): the session's rule (before tick k, exactly what the
// other board sent after its tick k - D, once), the bytes watchers get, and the start-up state
// files. A stand-in board transmits [side, tick] each tick and remembers what it was handed.

/** A pretend board: transmits [side, tick & 0xff] every tick, keeps what it was handed. */
function fakeBoard(side) {
  const board = { side, ticks: 0, handed: [], inputs: [] };
  board.link = {
    incoming: (bytes) => board.handed.push(Array.from(bytes)),
    outgoing: () => Uint8Array.of(side, board.ticks & 0xff),
  };
  board.machine = (deliver) => ({
    run: ([input]) => {
      board.inputs.push(input);
      board.ticks++;
    },
    send: (packets) => packets.forEach(([to, packet]) => deliver(to, packet)),
  });
  return board;
}

/** The other board's hello, proposing `delay`. */
function hello(delay) {
  const bytes = new Uint8Array(5);
  new DataView(bytes.buffer).setUint32(0, 0xffffffff, true);
  bytes[4] = delay;
  return bytes;
}

/** Two linked sessions; packets go into `wire` until `flush` delivers them (in any order). */
function pair(delay, carried = [[], []]) {
  const boards = [fakeBoard(0), fakeBoard(1)];
  const wire = [];
  const delays = [delay].flat();
  const sessions = boards.map((board, local) =>
    new LinkedSession({ link: board.link, local, delay: delays[local] ?? delays[0], carried: carried[local] }));
  // Each says hello first (the worker sends it before the first tick).
  sessions.forEach((session, from) => session.outgoing().forEach(([to, packet]) => wire.push([to, packet])));
  const machines = boards.map((board) => board.machine((to, packet) => wire.push([to, packet])));
  const flush = (order = (packets) => packets) => {
    for (const [to, packet] of order(wire.splice(0))) sessions[to].receive(1 - to, packet);
  };
  const advance = (i, input = 0) => sessions[i].advance(input, machines[i]);
  return { boards, sessions, wire, flush, advance };
}

test('each tick a board is handed exactly what the other sent D ticks earlier, once', () => {
  const D = 3;
  const { boards, flush, advance } = pair(D);
  for (let k = 0; k < 20; k++) {
    assert.ok(advance(0));
    assert.ok(advance(1));
    flush();
  }
  for (const board of boards) {
    const other = 1 - board.side;
    board.handed.forEach((bytes, k) => assert.deepEqual(bytes, k < D ? [] : [other, (k - D + 1) & 0xff]));
  }
});

test('a board runs up to D ticks ahead, then waits for the other; a press while waiting counts', () => {
  const D = 2;
  const { boards, sessions, flush, advance } = pair(D);
  assert.equal(sessions[0].lookahead(), D - 1);
  assert.ok(advance(0, 1 << 2));
  assert.ok(advance(0));
  // Tick 2 needs the other's tick 0, which hasn't run.
  assert.equal(sessions[0].lookahead(), -1);
  assert.equal(advance(0, (1 << 0) | (200 << 16)), false);
  assert.ok(advance(1));
  flush();
  assert.equal(sessions[0].lookahead(), 0);
  assert.ok(advance(0, 77 << 16));
  // The trigger pressed while it waited goes into the frame that ran next, with the latest aim.
  assert.equal(boards[0].inputs[2], ((1 << 0) | (77 << 16)) >>> 0);
  assert.equal(sessions[0].framesAhead(), 2);
});

test('messages may arrive out of order and twice: each is handed over once, in tick order', () => {
  const board = fakeBoard(0);
  const session = new LinkedSession({ link: board.link, local: 0, delay: 2 });
  const machine = board.machine(() => {});
  const packet = (tick) => {
    const bytes = new Uint8Array(6);
    new DataView(bytes.buffer).setUint32(0, tick, true);
    bytes.set([1, tick], 4);
    return bytes;
  };
  const ticks = [5, 4, 3, 2, 1, 0, 0, 3, 5];
  for (const tick of ticks) session.receive(1, packet(tick));
  // Nothing past our own proposal for D without theirs.
  for (let k = 0; k < 2; k++) assert.ok(session.advance(0, machine));
  assert.equal(session.advance(0, machine), false);
  session.receive(1, hello(2));
  session.receive(0, packet(6)); // not from the other board
  for (let k = 2; k < 8; k++) assert.ok(session.advance(0, machine));
  assert.equal(session.advance(0, machine), false);
  assert.deepEqual(board.handed, [[], [], [1, 0], [1, 1], [1, 2], [1, 3], [1, 4], [1, 5]]);
  // A copy of one already handed over changes nothing.
  session.receive(1, packet(2));
  assert.equal(session.lookahead(), -1);
});

test('bytes carried by a start-up state go first; a longer delay leaves a quiet tick or two', () => {
  const carried = [[Uint8Array.of(9, 1), Uint8Array.of(9, 2)], [Uint8Array.of(8, 1), Uint8Array.of(8, 2)]];
  const { boards, flush, advance } = pair(4, carried);
  for (let k = 0; k < 6; k++) {
    advance(0);
    advance(1);
    flush();
  }
  assert.deepEqual(boards[0].handed, [[9, 1], [9, 2], [], [], [1, 1], [1, 2]]);
  assert.deepEqual(boards[1].handed, [[8, 1], [8, 2], [], [], [0, 1], [0, 2]]);
});

test('with a shorter delay than was carried, the rest goes in at its last tick', () => {
  const carried = [[Uint8Array.of(1), Uint8Array.of(2), Uint8Array.of(3)], []];
  const { boards, flush, advance } = pair(2, carried);
  for (let k = 0; k < 3; k++) {
    advance(0);
    advance(1);
    flush();
  }
  assert.deepEqual(boards[0].handed, [[1], [2, 3], [1, 1]]);
});

test('both boards take the larger of their proposed delays', () => {
  const { boards, sessions, flush, advance } = pair([2, 5]);
  // The first board runs its first tick knowing only its own proposal...
  assert.ok(advance(0));
  assert.equal(sessions[0].agreedDelay, undefined);
  flush();
  assert.equal(sessions[0].agreedDelay, 5);
  assert.equal(sessions[1].agreedDelay, 5);
  for (let k = 0; k < 9; k++) {
    if (k > 0) assert.ok(advance(0));
    assert.ok(advance(1));
    flush();
  }
  for (const board of boards) {
    const other = 1 - board.side;
    board.handed.forEach((bytes, k) => assert.deepEqual(bytes, k < 5 ? [] : [other, k - 5 + 1]));
  }
});

test('the frames run are kept for watchers, each with its input and what the board was handed', () => {
  const { sessions, flush, advance } = pair(2);
  for (let k = 0; k < 5; k++) {
    advance(0, k);
    advance(1);
    flush();
  }
  const records = sessions[0].records(3);
  assert.deepEqual(records.map(({ frame, input, bytes }) => [frame, input, Array.from(bytes)]), [[3, 3, [1, 2]], [4, 4, [1, 3]]]);
  assert.deepEqual(sessions[0].records(-1), []);
});

test('watch records round-trip, and a cut-off record at the end is left out', () => {
  const records = [
    { frame: 7, input: 0xff80_0001, bytes: Uint8Array.of() },
    { frame: 8, input: 2, bytes: Uint8Array.from({ length: 1400 }, (_, i) => i & 0xff) },
    { frame: 9, input: 0, bytes: Uint8Array.of(1, 2, 3) },
  ];
  const encoded = encodeRecords(records);
  assert.equal(encoded.length, 3 * 10 + 1403);
  const decoded = decodeRecords(encoded);
  assert.deepEqual(decoded.map((r) => [r.frame, r.input, Array.from(r.bytes)]), records.map((r) => [r.frame, r.input, Array.from(r.bytes)]));
  assert.equal(decodeRecords(encoded.subarray(0, encoded.length - 1)).length, 2);
  assert.throws(() => encodeRecords([{ frame: 0, input: 0, bytes: new Uint8Array(0x10000) }]));
});

test('a linked board marks its watch state with its side; other states pass through', () => {
  const state = Uint8Array.of(0x76, 0x61, 0x62, 0x7a, 1, 2, 3); // "vabz": packed, as worker.js does
  assert.equal(markLinked(state, undefined), state);
  assert.deepEqual(unmarkLinked(state), { state });
  const marked = markLinked(state, 1);
  const { side, state: back } = unmarkLinked(marked);
  assert.equal(side, 1);
  assert.deepEqual(Array.from(back), Array.from(state));
});

test('start-up state files hold the side, the carried bytes and the state', () => {
  const carried = [Uint8Array.of(1, 2), Uint8Array.of(), Uint8Array.of(3)];
  const state = Uint8Array.of(0x76, 0x61, 0x62, 0x7a, 9, 9);
  const parsed = parseLinkState(makeLinkState({ side: 1, carried, state }));
  assert.equal(parsed.side, 1);
  assert.deepEqual(parsed.carried.map((c) => Array.from(c)), [[1, 2], [], [3]]);
  assert.deepEqual(Array.from(parsed.state), Array.from(state));
  assert.equal(parseLinkState(state), undefined);
});
