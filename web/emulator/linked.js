// Linked cabinets (Time Crisis II's twin cabinet): two players, two boards, one per browser,
// joined by the game's serial link, which the core emulates and the frontend carries
// (mame/README.md, "Linked cabinets"). Each board is the player's own machine, fed only by its
// own player's controls; what one board transmits on the link goes to the other, a fixed
// number of link ticks (frames) D later. Nothing else crosses: there is no shared machine, so
// nothing to roll back or compare.
//
// The rule, per board, one tick per frame (k counts from 0 at the session's start): before
// running tick k, hand the board, once, exactly what the other board transmitted after its
// tick k - D (nothing while k - D < 0), run the frame, then send what this board transmitted
// to the other board, every tick, even nothing. A board may run up to D ticks ahead of the
// other; past that it waits for the other's message, as a lockstep game waits for an input.
// D is the same on both boards for the whole session: each proposes one (from the round trip
// it measured) in a hello, the first message of the session, and both take the larger. A board
// can run the ticks before its own proposal without knowing the other's: the larger D can only
// leave them as they are, with nothing from the other board yet.
//
// Messages: [tick u32 LE][the link bytes the board transmitted after that tick], and the hello,
// [0xffffffff][D u8]. The worker puts its session's epoch (u16 LE) in front.
//
// Both boards start a session at the same point: their side's start-up state (made by
// mame/link-states.mjs: both boards saved after the same tick K, linked, in attract mode, with
// credits). Bytes were in flight between them then (what each transmitted at ticks K - 2 and
// K - 1, for the other's next two ticks), so each state carries the bytes its board still had
// to receive: they're handed over at ticks 0, 1, ... and the live messages follow from tick D
// on (a session's D is never less than the state's: a quiet moment on the line in between when
// it's more, which the game takes as latency).
//
// Also here: the stream a linked board sends its watchers (every frame's input and the link
// bytes the board was handed before it), and the files the start-up states come in. Used by
// web/emulator/worker.js, and by the checks in emulator/ and mame/ (Node).

/** The RetroPad buttons in an input; the lightgun's aim is above them. */
const BUTTONS = 0xffff;
/** The hello's "tick": no board gets that far. */
const HELLO = 0xffffffff;

/**
 * A linked board's session: one tick per frame, the other board's link bytes in, ours out.
 * It has what the worker's frame loop asks a GGRS session (netplay/src/lib.rs) for, so a linked
 * game is paced like a lockstep one.
 */
export class LinkedSession {
  /** Every frame run: { frame, input, bytes } (the bytes handed to the board before it), for watchers. */
  #records = [];
  /** The next tick to run. */
  #tick = 0;
  /** The other board's messages that came, by its tick, until they're handed over. */
  #theirs = new Map();
  /** The other board's ticks all here up to this one (-1: none yet). */
  #through = -1;
  /** Packets for the other board not taken by `outgoing` yet. */
  #outbox = [];
  /** A button pressed while waiting goes into the next frame that runs. */
  #held = 0;
  #events = [];
  #heard = false;
  /** The other board's proposed D, once its hello is here. */
  #theirDelay;

  /**
   * @param link the board's link: incoming(bytes), outgoing() -> Uint8Array
   * @param local our handle, 0 or 1 (seat order); the other board is the other one
   * @param delay this board's proposal for D, in ticks (>= 1); the session's is the larger of
   *   the two boards' proposals
   * @param carried the bytes the board still had to receive when its start-up state was saved,
   *   one Uint8Array per tick, handed over at ticks 0, 1, ...
   * @param roundTrip the round trip to the other player (ms) measured at the start, for stats
   */
  constructor({ link, local, delay, carried = [], roundTrip, keep = 600 }) {
    if (!(delay >= 1)) throw new Error("A linked session needs a delay of at least a tick");
    this.link = link;
    this.local = local;
    this.peer = 1 - local;
    this.delay = delay;
    this.carried = carried;
    this.roundTrip = roundTrip;
    this.keep = keep;
    const hello = new Uint8Array(5);
    new DataView(hello.buffer).setUint32(0, HELLO, true);
    hello[4] = delay;
    this.#outbox.push([this.peer, hello]);
  }

  /** The session's D once both boards' proposals are known (undefined before). */
  get agreedDelay() {
    return this.#theirDelay === undefined ? undefined : Math.max(this.delay, this.#theirDelay);
  }

  /** The other board's message for its tick `tick`: [tick u32 LE][link bytes], or its hello. */
  receive(handle, packet) {
    if (handle !== this.peer || packet.length < 4) return;
    const tick = new DataView(packet.buffer, packet.byteOffset, packet.byteLength).getUint32(0, true);
    if (tick === HELLO) {
      if (packet.length > 4 && packet[4] >= 1) this.#theirDelay ??= packet[4];
      return;
    }
    // Already handed over (a copy), or already here.
    if (tick <= this.#through || this.#theirs.has(tick)) return;
    this.#theirs.set(tick, packet.slice(4));
    while (this.#theirs.has(this.#through + 1)) this.#through++;
    if (!this.#heard) {
      this.#heard = true;
      this.#events.push({ type: "synchronized", player: this.peer });
    }
  }

  /** Packets for the other board, as [handle, bytes] pairs; `advance` sends them itself. */
  outgoing() {
    return this.#outbox.splice(0);
  }

  poll() {}

  events() {
    return this.#events.splice(0);
  }

  running() {
    return true;
  }

  free() {}

  /** Whether anything came from the other board yet: until then it's still starting. */
  get heard() {
    return this.#heard;
  }

  /** The tick the board runs next. */
  currentFrame() {
    return this.#tick;
  }

  /** Nothing is ever undone: every tick run is final. */
  confirmedFrame() {
    return this.#tick - 1;
  }

  /**
   * The ticks in hand beyond the next: 0 when it can run but the one after can't yet, less
   * while waiting for the other board (as GGRS's lookahead in lockstep).
   */
  lookahead() {
    const delay = this.agreedDelay;
    // Without the other's proposal, only the ticks before our own: whatever D turns out to
    // be, those take nothing from the other board.
    if (delay === undefined) return this.delay - 1 - this.#tick;
    return Math.max(this.#through + delay, delay - 1) - this.#tick;
  }

  /** How many ticks this board is ahead of the other. */
  framesAhead() {
    return this.#tick - (this.#through + 1);
  }

  ping() {
    return this.roundTrip === undefined ? undefined : Math.round(this.roundTrip);
  }

  /**
   * Runs the next tick with the local player's `input` on `machine` (run(inputs, present),
   * send(packets)), if the other board's bytes for it are here. False when they aren't yet.
   */
  advance(input, machine) {
    if (this.lookahead() < 0) {
      this.#held |= input & BUTTONS;
      return false;
    }
    input = (input | this.#held) >>> 0;
    this.#held = 0;
    const tick = this.#tick;
    const bytes = this.#incoming(tick);
    this.link.incoming(bytes);
    machine.run([input], true);
    const out = this.link.outgoing();
    const packet = new Uint8Array(4 + out.length);
    new DataView(packet.buffer).setUint32(0, tick, true);
    packet.set(out, 4);
    // Out at once, every tick, even with nothing on the line: the other board waits for it.
    machine.send([...this.#outbox.splice(0), [this.peer, packet]]);
    this.#records.push({ frame: tick, input, bytes });
    if (this.#records.length > this.keep) this.#records.splice(0, this.#records.length - this.keep);
    this.#tick++;
    return true;
  }

  /** What the board is handed before running `tick`. */
  #incoming(tick) {
    const from = tick - (this.agreedDelay ?? this.delay);
    if (from >= 0) {
      const bytes = this.#theirs.get(from) ?? new Uint8Array(0);
      this.#theirs.delete(from);
      return bytes;
    }
    // Before the other board's first message can be due: what the start-up state left in
    // flight, a tick's worth at a time (all the rest at the last of these ticks, should D be
    // less than what was carried).
    const carried = this.carried;
    if (tick >= carried.length) return new Uint8Array(0);
    return tick < this.delay - 1 ? carried[tick] : concat(carried.slice(tick));
  }

  /**
   * The frames run from `from` on, { frame, input, bytes } each, as far as they're kept (empty
   * if `from` is older).
   */
  records(from) {
    const first = this.#records[0]?.frame ?? this.#tick;
    if (from < first) return [];
    return this.#records.slice(from - first);
  }
}

function concat(chunks) {
  const out = new Uint8Array(chunks.reduce((n, chunk) => n + chunk.length, 0));
  chunks.reduce((at, chunk) => (out.set(chunk, at), at + chunk.length), 0);
  return out;
}

/**
 * A linked board's frames for its watchers: [frame u32][input u32][length u16][bytes], all
 * little-endian, one after another: the board's input (port 0) and the link bytes it was
 * handed before running the frame. The watcher's board, linked on the same side and started
 * from the same state, plays them exactly as the player's did.
 */
export function encodeRecords(records) {
  const size = records.reduce((n, { bytes }) => n + 10 + bytes.length, 0);
  const out = new Uint8Array(size);
  const view = new DataView(out.buffer);
  let at = 0;
  for (const { frame, input, bytes } of records) {
    if (bytes.length > 0xffff) throw new Error(`${bytes.length} link bytes in a frame: more than a record holds`);
    view.setUint32(at, frame, true);
    view.setUint32(at + 4, input, true);
    view.setUint16(at + 8, bytes.length, true);
    out.set(bytes, at + 10);
    at += 10 + bytes.length;
  }
  return out;
}

/** The records in `encodeRecords`' bytes; a cut-off one at the end is left out. */
export function decodeRecords(data) {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const records = [];
  for (let at = 0; at + 10 <= data.length; ) {
    const length = view.getUint16(at + 8, true);
    if (at + 10 + length > data.length) break;
    records.push({
      frame: view.getUint32(at, true),
      input: view.getUint32(at + 4, true),
      bytes: data.slice(at + 10, at + 10 + length),
    });
    at += 10 + length;
  }
  return records;
}

/** Marks a watch state from a linked board: "vabl", then the board's side (u8), then the state. */
const LINKED_STATE = Uint8Array.of(0x76, 0x61, 0x62, 0x6c);

/** A state for watchers, marked with the side of the board it came from if it was linked. */
export function markLinked(state, side) {
  if (side === undefined) return state;
  const out = new Uint8Array(LINKED_STATE.length + 1 + state.length);
  out.set(LINKED_STATE);
  out[LINKED_STATE.length] = side;
  out.set(state, LINKED_STATE.length + 1);
  return out;
}

/** `markLinked`'s parts: the side (undefined for a board that wasn't linked) and the state. */
export function unmarkLinked(bytes) {
  if (bytes.length <= LINKED_STATE.length || LINKED_STATE.some((b, i) => bytes[i] !== b)) return { state: bytes };
  return { side: bytes[LINKED_STATE.length], state: bytes.subarray(LINKED_STATE.length + 1) };
}

/**
 * A linked board's start-up state file (mame/link-states.mjs): "vabk", a version (1), the side
 * (0 Left/Red, 1 Right/Blue), how many ticks of carried bytes follow, a reserved byte, then
 * each tick's [length u32 LE][bytes], then the board's state (deflated as worker.js packs
 * states, or not).
 */
const LINK_STATE = Uint8Array.of(0x76, 0x61, 0x62, 0x6b);

export function makeLinkState({ side, carried, state }) {
  const size = 8 + carried.reduce((n, bytes) => n + 4 + bytes.length, 0) + state.length;
  const out = new Uint8Array(size);
  const view = new DataView(out.buffer);
  out.set(LINK_STATE);
  out.set([1, side, carried.length, 0], 4);
  let at = 8;
  for (const bytes of carried) {
    view.setUint32(at, bytes.length, true);
    out.set(bytes, at + 4);
    at += 4 + bytes.length;
  }
  out.set(state, at);
  return out;
}

/** `makeLinkState`'s parts, or undefined if `bytes` isn't such a file. */
export function parseLinkState(bytes) {
  if (bytes.length < 8 || LINK_STATE.some((b, i) => bytes[i] !== b) || bytes[4] !== 1) return undefined;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const carried = [];
  let at = 8;
  for (let i = 0; i < bytes[6]; i++) {
    const length = view.getUint32(at, true);
    carried.push(bytes.slice(at + 4, at + 4 + length));
    at += 4 + length;
  }
  return { side: bytes[5], carried, state: bytes.subarray(at) };
}
