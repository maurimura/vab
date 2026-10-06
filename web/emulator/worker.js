// Runs an FBNeo core off the main thread for one cabinet. Posts each frame to the page (which
// hands it to Bevy) and streams audio to the AudioWorklet (audio.js).
//
// Alone, the local player's buttons drive their seat's controller port. Online, every seated
// player's machine runs the same game in step with rollback (netplay/src/lib.rs): GGRS guesses
// the others' input and re-runs frames once the real input arrives. Its packets go out and come
// in on `port` as [seat, bytes], and the page carries them between the players. A `lockstep`
// game (Supermodel: a 32 MB state, too slow to save every frame) runs in step without guessing:
// some frames of input delay, and a wait whenever the others' input is late. Each starts
// with its session's epoch, so packets still in flight from an earlier session are dropped.
//
// Frames run on a precise clock (Alarm): one frame at its slot, the picture posted, then the
// next. A heavy core (a Model 3 frame is ~12 ms) never runs two frames back to back to catch
// up, since that holds its inputs and its picture for the whole burst; it catches up one frame
// per wake-up. GGRS's packets leave the moment it makes them, so online the two machines each
// run their frame in the same slot and swap one input per frame. A lockstep game's input delay
// follows the other machines' lateness: a frame more when their input keeps arriving late, a
// frame less after a quiet stretch.
//
// When players join or leave, one machine (the page picks it) captures the game as it is and
// every player starts a new session from that capture.
//
// People watching the cabinet run the game too, a little behind: one player's machine (the page
// picks it) streams them a state and then every player's input for each frame from there on,
// only frames no rollback can change anymore. The watchers' machines play those frames as they
// come in, keeping a few in hand so they play evenly.
//
// In:  { type: "start", core, rom, files, state, seat, turns, lockstep, port, hold }
//        Loads the game. `files` (a BIOS) and `state` are optional: skipped if missing. Plays
//        alone right away, or with `hold` waits for "online" (joining a game in progress) or
//        "watch-state" (watching; no `seat` or `port` then).
//      { type: "capture", epoch } Stops and captures the machine, for a change of players.
//        States that leave this worker (captured, watch-state) are deflated: a Model 3's 30 MB
//        is two thirds zeros and packs to 5 MB; they come back the same way (online, watch-state).
//      { type: "online", epoch, seats, state, roundTrip } Plays in step with the players in
//        `seats` (their seat numbers, ascending) from `state`, or from this machine's capture
//        for `epoch`; `roundTrip` (ms, to the farthest of them) sets a lockstep game's first
//        input delay.
//      { type: "solo" } Everyone else left: play on alone.
//      { type: "stream", on } Streams this machine's game to the watchers, or stops.
//      { type: "snapshot", to } A state for a new watcher, to go on from with the stream.
//      { type: "watch-state", bytes } | { type: "watch-inputs", frame, inputs } Watching: a
//        stream's state and inputs, as they're sent out (below).
//      { type: "input", mask } | { type: "audio", port, sampleRate } the speaker's port and rate
// Out: { type: "ready" } the game is loaded | { type: "frame", rgba, width, height } |
//      { type: "netplay", event, seat, ... } | { type: "captured", epoch, state } |
//      { type: "buttons", buttons } what the game calls player 1's buttons, [RetroPad id, name]
//      pairs, once known | { type: "watch-state", stream, bytes, to } the machine for watchers
//      to start from, [frame: u32 LE][state], for the watcher `to` or, without it, for all |
//      { type: "watch-inputs", stream, frame, inputs } a Uint16Array with a mask per controller
//      port (4) for each frame from `frame` on, a few times a second. `stream` counts up each
//      time the stream starts over (a new session); inputs go on from that stream's states.
//      Online, every 120 frames the machines compare a hash of the game's RAM (the worker's
//      own packets, marked with epoch 0xffff): a mismatch is the "desync" netplay event
//      (seat, frame), once per session, and the page then has one machine hand its game to
//      everyone again. The "stats" netplay event, once a second: ping (ms), delay (frames of input delay),
//      rollback (frames; 0 in lockstep), fps (frames shown), stalls (waits for the others'
//      input in that second), stallMs (the longest), look (the others' input in hand at a frame:
//      [least, median, most] frames), prefills (looks that waited for more) and framesAhead.
import { Core } from "./libretro.js";
import { Resampler } from "./resample.js";

/** Controller ports, as many as libretro.js has. */
const PORTS = 4;
/** How often watchers get the frames since the last time, in ms. */
const STREAM_EVERY = 100;
/** Frames a watcher has in hand before playing: a little more than arrive at once. */
const WATCH_BUFFER = 12;
/**
 * Game packets carry their session's epoch; this one marks the worker's own packets instead:
 * a checkpoint, 'h', the epoch (u16), the frame (u32) and a hash of the game's RAM there (u32).
 * The machines compare them to notice when they have drifted apart.
 */
const CONTROL = 0xffff;
/** Frames between checkpoints. */
const HASH_EVERY = 120;
/** Every this many words of RAM go into a checkpoint's hash: a drift spreads, so a sample catches it within a checkpoint or two. */
const HASH_STRIDE = 4;
/** Marks a deflated state: "vabz", then the deflate-raw bytes. */
const PACKED = Uint8Array.of(0x76, 0x61, 0x62, 0x7a);

/** Deflates a state for the trip to the other players, marked as such. */
async function pack(state) {
  const stream = new Blob([state]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const packed = new Uint8Array(await new Response(stream).arrayBuffer());
  const out = new Uint8Array(PACKED.length + packed.length);
  out.set(PACKED);
  out.set(packed, PACKED.length);
  return out;
}

/** The state back from `pack`; bytes not marked as packed are taken as they are. */
async function unpack(bytes) {
  if (bytes.length < PACKED.length || PACKED.some((b, i) => bytes[i] !== b)) return bytes;
  const stream = new Blob([bytes.subarray(PACKED.length)]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Input delay when the round trip is unknown, in frames. */
const DEFAULT_DELAY = 5;
/** The most input delay a lockstep game takes, in frames; past that it would stall anyway. */
const MAX_DELAY = 12;
/** Jitter allowance when picking a lockstep game's input delay from the round trip, in ms. */
const DELAY_SLACK_MS = 10;
/**
 * A frame costing more than this (ms) to run makes the core heavy: it runs one frame per
 * wake-up (a burst would hold its picture and its inputs for 25 ms or more) and online it
 * covers the ping with input delay rather than roll back often (Supermodel).
 */
const HEAVY_RERUN_MS = 6;
/**
 * While waiting for the others' input, GGRS still gets a look this often (ms), for its
 * keep-alives and resends; a packet arriving wakes the game at once.
 */
const WAIT_POLL_MS = 16;
/** The least input delay a lockstep game takes, in frames. */
const MIN_DELAY = 2;
/** A frame is late when it was waiting for input longer than this (ms): it shows. */
const LATE_MS = 2;
/**
 * Lockstep pacing. After a wait for the others' input, the game goes on once it has their
 * input for this many frames in hand (within PREFILL_MAX_MS of the wait's start), not the
 * moment the first arrives: on that edge, the slightest delay in their input would be another
 * wait. And a frame runs EDGE_PACE_MS later than its slot while their input for the next
 * frame isn't in hand: only the machine ahead is ever on the edge, so slowing it brings the
 * two together with input to spare on both sides.
 */
const PREFILL = 2;
const PREFILL_MAX_MS = 150;
const EDGE_PACE_MS = 1;
/** A wait longer than this many frames is a hiccup (a lost connection, a hidden tab), not
 *  lateness a frame more of delay would cover. */
const HICCUP_FRAMES = 10;
/** Frames a session has run before its input delay is tuned: the start is never representative. */
const WARMUP_FRAMES = 120;
/** A lockstep game doesn't change its input delay more often than this (ms). */
const RAISE_COOLDOWN_MS = 3000;
const LOWER_COOLDOWN_MS = 2000;
/** A lockstep game lowers its input delay after this long without a late frame (ms)... */
const QUIET_MS = 5000;
/** ...but not back to a delay that saw late frames this recently (ms). */
const REMEMBER_LATE_MS = 60000;

let audioPort;
let speakerRate = 48000;
let localMask = 0;
/** Buttons that went down since the game last read the controls: a tap between two frames
 *  still counts for the next one. */
let pressed = 0;
let cabinet;
const waiting = [];

// GGRS, loading while the game downloads.
let Session;
const netplay = import("../netplay/netplay.js").then(async (module) => {
  await module.default();
  Session = module.Session;
});

onmessage = ({ data: msg }) => {
  if (msg.type === "input") {
    pressed |= msg.mask & ~localMask;
    localMask = msg.mask;
  } else if (msg.type === "audio") ({ port: audioPort, sampleRate: speakerRate = speakerRate } = msg);
  else if (msg.type === "start") start(msg);
  // Anything else is for the loaded game; it can arrive while the game still downloads.
  else if (cabinet) cabinet.handle(msg);
  else waiting.push(msg);
};

/** The local controls for the frame about to run. */
function sampleInput() {
  const mask = localMask | pressed;
  pressed = 0;
  return mask;
}

// "no-cache" checks with the server every time (a 304 when unchanged), so newly uploaded or
// replaced ROMs, BIOS sets and states are picked up.
const download = async (url) => {
  const response = await fetch(url, { cache: "no-cache" });
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
};
const downloadIfPresent = (url) => url && download(url).catch(() => undefined);

async function start({ core: coreUrl, rom: romUrl, files = [], state: stateUrl, seat = 0, turns = false, lockstep = false, port, hold }) {
  const { default: createCore } = await import(coreUrl);
  const [rom, state, ...extras] = await Promise.all([
    download(romUrl),
    downloadIfPresent(stateUrl),
    ...files.map(downloadIfPresent),
  ]);
  // The canvas is for a core that draws with WebGL (Supermodel), which sizes it; FBNeo ignores
  // it. Node (emulator/netplay-check.mjs) has none.
  const canvas = typeof OffscreenCanvas === "function" ? new OffscreenCanvas(1, 1) : undefined;
  const cab = new Cabinet(await Core.create(createCore, {
    onFrame: (rgba, width, height) => cab.muted || (cab.frame = { type: "frame", rgba, width, height }),
    onAudio: (samples) => {
      if (cab.muted) return;
      if (cab.resampler) samples = cab.resampler.process(samples);
      audioPort?.postMessage(samples, [samples.buffer]);
    },
    onLog: (level, text) => level >= 2 && console.warn(text),
  }, { canvas }), { seat, turns, lockstep, port });
  const core = cab.core;
  core.netplay = true;
  files.forEach((url, i) => extras[i] && core.addFile(url.split("/").pop(), extras[i]));
  const { fps, sampleRate } = core.loadGame(romUrl.split("/").pop(), rom);
  cab.fps = fps;
  // The speaker runs at one rate; a core at another (Supermodel, 44.1 kHz) is brought to it.
  if (Math.round(sampleRate) !== speakerRate) cab.resampler = new Resampler(sampleRate, speakerRate);
  // A start-up state (emulator/snapshot.mjs) skips the boot screens and adds credits. States
  // from an older core build don't load; the game then just boots normally.
  try {
    if (state) core.unserialize(state);
  } catch (error) {
    console.warn(`${stateUrl}: ${error.message}`);
  }
  cab.paused = Boolean(hold);
  await netplay;
  cabinet = cab;
  postMessage({ type: "ready" });
  for (const msg of waiting.splice(0)) cab.handle(msg);
  cab.tick();
}

/** A mask per controller port: `inputs[i]` on seat `seats[i]`'s, nothing on the others. */
function byPort(inputs, seats) {
  const ports = new Uint16Array(PORTS);
  inputs.forEach((mask, i) => (ports[seats[i]] = mask));
  return ports;
}

/**
 * Calls back at a chosen moment, precisely. A timer alone can't: a nested setTimeout waits at
 * least 4 ms and fires a few ms late, which next to a 12 ms frame pushes every frame past its
 * slot. So a timer covers all but the last few ms, and messages to ourselves (MessageChannel,
 * not clamped) the rest.
 */
class Alarm {
  #fn;
  #at = Infinity;
  #timer;
  #port;
  #yielding = false;

  constructor(fn) {
    this.#fn = fn;
    const channel = new MessageChannel();
    this.#port = channel.port2;
    channel.port1.onmessage = () => {
      this.#yielding = false;
      this.#check();
    };
  }

  /** Calls back at `at` (performance.now() ms), or earlier if already due to. */
  at(at) {
    if (at >= this.#at) return;
    this.#at = at;
    this.#arm();
  }

  /** Calls back as soon as possible. */
  now() {
    this.at(0);
  }

  #arm() {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    const left = this.#at - performance.now();
    if (left > 6) {
      this.#timer = setTimeout(() => {
        this.#timer = undefined;
        this.#check();
      }, left - 4);
    } else this.#yield();
  }

  #yield() {
    if (this.#yielding) return;
    this.#yielding = true;
    this.#port.postMessage(0);
  }

  #check() {
    if (this.#at === Infinity) return;
    const left = this.#at - performance.now();
    if (left <= 0) {
      this.#at = Infinity;
      this.#fn();
    } else if (left > 6 && !this.#timer) this.#arm();
    else this.#yield();
  }
}

class Cabinet {
  /** The newest frame to post. */
  frame;
  /** True while frames run only to measure, not to show or hear. */
  muted = false;
  /** Not running frames: waiting to join a game, or for the others after a capture. */
  paused = false;
  fps = 60;
  /** Brings the core's sound to the speaker's rate, when they differ. */
  resampler;

  #seat;
  #lockstep;
  #port;
  /** Online: the GGRS session, its epoch, and the seats of its players in handle order. */
  #session;
  #epoch;
  #seats;
  /** How this core plays online: rollback limit, input delay, whether it's heavy. */
  #tuned;
  /** The input delay in use, in frames, and the one the round trip called for at the start
   *  (the floor until GGRS has measured the ping itself). */
  #delay;
  #delayFloor = MIN_DELAY;
  /** This machine at the last change of players, until the next session starts from it. */
  #captured;
  /**
   * Streaming to watchers: the stream's number, the frame they get next, inputs (a mask per
   * port per frame) not sent yet, and when inputs last went out.
   */
  #stream;
  #streams = 0;
  /**
   * Watching: frames to run (a mask per port each), the frame after the last, and whether it's
   * waiting to have a few in hand.
   */
  #watch;
  /** When the next frame is due. */
  #next = performance.now();
  #alarm = new Alarm(() => this.tick());
  /** How long a frame takes to run, on average (ms): heavy cores run one per wake-up. */
  #runMs = 0;
  /** Online: waiting for the others' input (or, rolling back, for them to catch up). */
  #waiting = false;
  /** When the current wait began, and how long each wait in the current second lasted. */
  #stall;
  #stalls = [];
  /** Diagnostics for the stats: the others' input in hand at each frame, and prefill waits. */
  #looks = [];
  #prefills = 0;
  /**
   * Checkpoints: hashes of this machine's RAM by frame, the others' that came before this
   * machine got there (frame -> seat -> hash), whether a drift was reported this session, and
   * what happened lately (delay changes, long waits), for the report.
   */
  #hashes = new Map();
  #theirHashes = new Map();
  #desynced = false;
  #events = [];
  /**
   * Lockstep input delay tuning: the last late frame, when the delay may change next, and the
   * last delay that saw late frames (and when), not to go back to for a while.
   */
  #quietSince = 0;
  #retuneAt = 0;
  #lateAt = { delay: 0, time: -Infinity };
  #nextStats = 0;
  #frames = 0;       // displayed frames since the last stats report, for fps
  #framesSince = performance.now();
  #buttonsSent = false;

  constructor(core, { seat, turns, lockstep, port }) {
    this.core = core;
    core.turns = turns;
    this.#seat = seat;
    this.#lockstep = lockstep;
    this.#port = port;
    if (!port) return; // watching
    port.onmessage = ({ data: [seat, packet] }) => {
      const handle = this.#seats?.indexOf(seat) ?? -1;
      const bytes = new Uint8Array(packet);
      const epoch = bytes[0] | (bytes[1] << 8);
      if (epoch === CONTROL) return this.#control(seat, bytes);
      if (handle < 0 || epoch !== this.#epoch || !this.#session) return;
      this.#session.receive(handle, bytes.subarray(2));
      // Waiting for exactly this, most likely: run the frame now rather than at the next look.
      if (this.#waiting) this.#alarm.now();
    };
  }

  handle(msg) {
    if (msg.type === "capture") this.#capture(msg.epoch);
    if (msg.type === "online") this.#online(msg);
    if (msg.type === "solo") {
      this.#leaveSession();
      if (this.#stream) this.#startStream();
    }
    if (msg.type === "stream") {
      if (!msg.on) this.#stream = undefined;
      else if (!this.#stream) this.#startStream();
    }
    if (msg.type === "snapshot" && this.#stream) this.#sendState(msg.to);
    if (msg.type === "watch-state") this.#watchFrom(msg.bytes);
    if (msg.type === "watch-inputs") this.#watchInputs(msg);
  }

  /** Frames to run per wake-up at most: one for a heavy core, a few to catch up otherwise. */
  #perWake() {
    return this.#runMs > HEAVY_RERUN_MS ? 1 : 4;
  }

  tick = () => {
    const session = this.#session;
    const frameMs = 1000 / this.fps;
    // After a long pause (hidden tab), carry on from now instead of fast-forwarding.
    if (performance.now() - this.#next > 250) this.#next = performance.now();
    if (session) {
      session.poll();
      this.#send();
      for (const { type, player, ...fields } of session.events()) {
        postMessage({ type: "netplay", event: type, seat: this.#seats[player], ...fields });
      }
      if (session.running() && !this.paused) {
        let ran = 0;
        let stalled = false;
        while (ran < this.#perWake() && performance.now() >= this.#next) {
          const startedAt = performance.now();
          const lookahead = session.lookahead();
          if (this.#stall !== undefined && lookahead < PREFILL && startedAt - this.#stall < PREFILL_MAX_MS) {
            stalled = true; // waiting on, for a little of the others' input in hand
            this.#prefills++;
            break;
          }
          this.#looks.push(lookahead);
          // GGRS sends our input for a frame ahead before the frame runs (Machine.send).
          const advanced = session.advance(sampleInput(), this.#machine);
          this.#send(); // anything else it queued: acknowledgements, reports
          // False: the others' input isn't here yet, or they're too far behind to keep
          // guessing. A packet arriving wakes us; failing that, the next look.
          if (!advanced) {
            stalled = true;
            break;
          }
          ran++;
          this.#next += this.#pace(session, lookahead, frameMs);
          if (session.currentFrame() % HASH_EVERY === 0) this.#checkpoint(session.currentFrame());
          if (this.#stall !== undefined) {
            // The wait is over. Making up the time waited would only run through the inputs
            // in hand and wait again a round trip later, the two machines taking turns: carry
            // on from here instead.
            this.#stalls.push(startedAt - this.#stall);
            if (startedAt - this.#stall >= frameMs) this.#note(session, `waited ${Math.round(startedAt - this.#stall)} ms`);
            this.#stall = undefined;
            this.#next = startedAt + frameMs;
            break;
          }
        }
        const now = performance.now();
        this.#waiting = stalled;
        if (stalled) this.#stall ??= now;
        this.#stats(session, now);
        this.#alarm.at(stalled ? now + WAIT_POLL_MS : this.#next);
      } else {
        this.#next = performance.now();
        this.#waiting = false;
        this.#alarm.at(this.#next + 5);
      }
    } else if (this.paused) {
      this.#next = performance.now();
      this.#alarm.at(this.#next + 5);
    } else if (this.#watch) {
      const watch = this.#watch;
      if (watch.waiting && watch.frames.length >= WATCH_BUFFER) watch.waiting = false;
      if (watch.waiting) {
        this.#next = performance.now();
      } else {
        for (let i = 0; i < this.#perWake() && performance.now() >= this.#next; i++) {
          // Ran out: wait to have a few in hand again rather than stutter frame by frame.
          if (!watch.frames.length) {
            watch.waiting = true;
            break;
          }
          this.#run(watch.frames.shift(), true);
          // A little faster while far behind the stream (frames came in after a hiccup).
          this.#next += watch.frames.length > WATCH_BUFFER * 2.5 ? frameMs * 0.9 : frameMs;
        }
      }
      this.#alarm.at(watch.waiting ? performance.now() + WAIT_POLL_MS : this.#next);
    } else {
      for (let i = 0; i < this.#perWake() && performance.now() >= this.#next; i++) {
        const ports = byPort([sampleInput()], [this.#seat]);
        this.#run(ports, true);
        this.#stream?.inputs.push(...ports);
        this.#next += frameMs;
      }
      this.#alarm.at(this.#next);
    }
    if (this.#stream) this.#flush();
    if (this.frame) {
      postMessage(this.frame, [this.frame.rgba.buffer]);
      this.frame = undefined;
    }
  };

  /**
   * How long after this frame the next one is due (ms). Rolling back: a little slower while
   * ahead of the others, so all run in step. Lockstep: a little slower while the others' input
   * for the next frame isn't in hand yet (`lookahead`, as of before this frame).
   */
  #pace(session, lookahead, frameMs) {
    if (this.#tuned.rollback > 0) return session.framesAhead() > 0 ? frameMs * 1.1 : frameMs;
    return lookahead < 1 ? frameMs + EDGE_PACE_MS : frameMs;
  }

  /** Sends GGRS's packets to the other players, as soon as it has any. */
  #send() {
    if (this.#session) this.#post(this.#session.outgoing());
  }

  /** GGRS packets, `[handle, bytes]` pairs, to the other players, stamped with the epoch. */
  #post(packets) {
    for (const [handle, packet] of packets) {
      const framed = new Uint8Array(2 + packet.length);
      framed.set([this.#epoch & 0xff, this.#epoch >> 8]);
      framed.set(packet, 2);
      this.#port.postMessage([this.#seats[handle], framed.buffer], [framed.buffer]);
    }
  }

  /** Once a second: how the session is doing, to the page, and the input delay tuned. */
  #stats(session, now) {
    if (now < this.#nextStats) return;
    const fps = Math.round((this.#frames * 1000) / (now - this.#framesSince));
    const stalls = this.#stalls;
    const looks = this.#looks.sort((a, b) => a - b);
    postMessage({
      type: "netplay", event: "stats", ping: session.ping(), delay: this.#delay, rollback: this.#tuned.rollback, fps,
      stalls: stalls.length, stallMs: Math.round(Math.max(0, ...stalls)),
      look: looks.length ? [looks[0], looks[looks.length >> 1], looks[looks.length - 1]] : [], prefills: this.#prefills, framesAhead: session.framesAhead(),
    });
    this.#looks = [];
    this.#prefills = 0;
    this.#tune(session, stalls, now);
    this.#stalls = [];
    this.#frames = 0;
    this.#framesSince = now;
    this.#nextStats = now + 1000;
  }

  /**
   * A heavy core covers the ping with input delay (it can't roll back cheaply), so the delay
   * follows the others' lateness: a frame more when their input was late more than once this
   * second, or by a whole frame; a frame less after a quiet while, down to what the round trip
   * calls for (GGRS's ping, which the start's measurement may have overstated, through the
   * relay before the direct link opened), but not back to a delay that was late recently. The delay is
   * this machine's own: the others need no notice (netplay/src/lib.rs).
   */
  #tune(session, stalls, now) {
    if (!this.#tuned.heavy || session.currentFrame() < WARMUP_FRAMES) return;
    const frameMs = 1000 / this.fps;
    const late = stalls.filter((ms) => ms > LATE_MS && ms < HICCUP_FRAMES * frameMs);
    if (late.length) {
      this.#quietSince = now;
      this.#lateAt = { delay: this.#delay, time: now };
      if (late.length < 2 && Math.max(...late) < frameMs) return;
      if (this.#delay < MAX_DELAY && now >= this.#retuneAt) {
        this.#setDelay(this.#delay + 1);
        this.#retuneAt = now + RAISE_COOLDOWN_MS;
      }
      return;
    }
    if (now - this.#quietSince < QUIET_MS || now < this.#retuneAt) return;
    // GGRS's ping includes up to a frame of polling at each end, so a frame less of it.
    const ping = session.ping();
    const floor = ping === undefined ? this.#delayFloor : onlineDelay(Math.max(0, ping - frameMs), this.fps);
    const lower = this.#delay - 1;
    if (lower < floor || (lower <= this.#lateAt.delay && now - this.#lateAt.time < REMEMBER_LATE_MS)) return;
    this.#setDelay(lower);
    this.#retuneAt = now + LOWER_COOLDOWN_MS;
  }

  #setDelay(delay) {
    this.#session.setDelay(delay);
    this.#delay = delay;
    this.#tuned.delay = delay;
    this.#note(this.#session, `input delay ${delay}`);
  }

  /** Remembers what happened, with the frame, for the report when the machines drift apart. */
  #note(session, what) {
    this.#events.push(`${session.currentFrame()}: ${what}`);
    if (this.#events.length > 40) this.#events.shift();
  }

  /**
   * A checkpoint: the machine has just reached `frame`. Its RAM is hashed, compared with what
   * the others reported for that frame, and reported to them.
   */
  #checkpoint(frame) {
    const hash = hashRam(this.core.systemRam(), HASH_STRIDE);
    this.#hashes.set(frame, hash);
    for (const old of this.#hashes.keys()) if (old < frame - HASH_EVERY * 8) this.#hashes.delete(old);
    for (const [seat, theirs] of this.#theirHashes.get(frame) ?? []) this.#compare(seat, frame, theirs, hash);
    this.#theirHashes.delete(frame);
    for (const seat of this.#seats) {
      if (seat === this.#seat) continue;
      const packet = new Uint8Array(13);
      const view = new DataView(packet.buffer);
      view.setUint16(0, CONTROL, true);
      packet[2] = 0x68; // 'h'
      view.setUint16(3, this.#epoch, true);
      view.setUint32(5, frame, true);
      view.setUint32(9, hash, true);
      this.#port.postMessage([seat, packet.buffer], [packet.buffer]);
    }
  }

  /** One of the worker's own packets from `seat`: a checkpoint of theirs. */
  #control(seat, bytes) {
    if (bytes.length < 13 || bytes[2] !== 0x68 || !this.#session) return;
    const view = new DataView(bytes.buffer, bytes.byteOffset);
    if (view.getUint16(3, true) !== this.#epoch) return; // an earlier session's
    const frame = view.getUint32(5, true);
    const hash = view.getUint32(9, true);
    const mine = this.#hashes.get(frame);
    if (mine !== undefined) return this.#compare(seat, frame, hash, mine);
    if (frame < this.#session.currentFrame() - HASH_EVERY * 8) return; // too old to check
    const theirs = this.#theirHashes.get(frame) ?? new Map();
    theirs.set(seat, hash);
    this.#theirHashes.set(frame, theirs);
  }

  #compare(seat, frame, theirs, mine) {
    if (theirs === mine || this.#desynced) return;
    this.#desynced = true;
    console.warn(`Out of step with seat ${seat} at frame ${frame} (their RAM hashes ${theirs.toString(16)}, ours ${mine.toString(16)}); lately: ${this.#events.join("; ") || "nothing of note"}`);
    postMessage({ type: "netplay", event: "desync", seat, frame });
  }

  // GGRS's requests, run on the core.
  #machine = {
    save: (slot, checksum) => {
      this.core.saveSlot(slot);
      return checksum ? hashRam(this.core.systemRam()) : undefined;
    },
    load: (slot) => this.core.loadSlot(slot),
    run: (inputs, present) => this.#run(byPort(inputs, this.#seats), present),
    send: (packets) => this.#post(packets),
  };

  /** Runs a frame with a mask per controller port. */
  #run(masks, present) {
    const ports = this.core.inputs;
    ports.set(masks);
    // Core routes shared upright gameplay and descriptor-defined Start/Coin aliases;
    // the original masks remain intact for rollback and the spectator input stream.
    this.core.present = present;
    const startedAt = performance.now();
    this.core.run();
    const ms = performance.now() - startedAt;
    this.#runMs = this.#runMs ? this.#runMs * 0.9 + ms * 0.1 : ms;
    if (present) this.#frames++;
    // The core names the game's buttons on its first frame.
    if (!this.#buttonsSent && this.core.buttons.size) {
      this.#buttonsSent = true;
      postMessage({ type: "buttons", buttons: [...this.core.buttons] });
    }
  }

  async #capture(epoch) {
    const state = this.core.serialize();
    this.#captured = { epoch, state };
    this.paused = true;
    const packed = await pack(state);
    postMessage({ type: "captured", epoch, state: packed }, [packed.buffer]);
  }

  async #online({ epoch, seats, state, roundTrip }) {
    state = state ? await unpack(state) : this.#captured?.epoch === epoch ? this.#captured.state : undefined;
    if (!state) return;
    this.#leaveSession();
    // Power-on, then the state: every machine then has the same of what the state leaves out.
    this.core.reset();
    this.core.unserialize(state);
    this.#captured = undefined;
    if (this.#lockstep) {
      this.#tuned = { rollback: 0, heavy: true }; // GGRS never saves: no slots needed
    } else if (!this.#tuned) {
      this.muted = true;
      this.#tuned = tune(this.core, this.fps);
      this.muted = false;
      this.core.allocSlots(this.#tuned.rollback + 2);
    }
    const rollback = this.#tuned.rollback;
    // Heavy cores (and lockstep) cover the ping with input delay so re-simulations are rare and
    // the frame rate stays smooth; cheap cores keep the small delay tune picked and roll back.
    const delay = this.#tuned.heavy ? onlineDelay(roundTrip, this.fps) : this.#tuned.delay;
    this.#tuned.delay = delay;
    this.#delay = delay;
    this.#delayFloor = delay;
    this.#epoch = epoch & 0xffff;
    this.#seats = seats;
    this.#session = new Session(seats.length, seats.indexOf(this.#seat), delay, rollback, Math.round(this.fps));
    this.paused = false;
    this.#waiting = false;
    this.#stall = undefined;
    this.#stalls = [];
    this.#quietSince = performance.now();
    this.#retuneAt = 0;
    this.#lateAt = { delay: 0, time: -Infinity };
    this.#hashes = new Map();
    this.#theirHashes = new Map();
    this.#desynced = false;
    this.#events = [`session from epoch ${this.#epoch}, delay ${delay}`];
    this.#next = performance.now();
    if (this.#stream) this.#startStream();
    this.#alarm.now();
  }

  #leaveSession() {
    this.#session?.free();
    this.#session = undefined;
    this.#seats = undefined;
    this.#waiting = false;
    this.paused = false;
  }

  /** Streams from here: the machine as it is now, or online as of the last confirmed frame. */
  #startStream() {
    const frame = this.#session ? this.#session.confirmedFrame() + 1 : 0;
    this.#stream = { id: ++this.#streams, frame, inputs: [], sentAt: 0 };
    this.#sendState();
  }

  /**
   * Sends the frames since the last time, a few times a second (or now with `now`). Online,
   * those are the frames GGRS has confirmed since; alone, every frame run.
   */
  #flush(now = false) {
    const stream = this.#stream;
    if (this.#session) {
      const players = this.#seats.length;
      const confirmed = this.#session.confirmedInputs(stream.frame + stream.inputs.length / PORTS);
      for (let i = 0; i < confirmed.length; i += players) {
        stream.inputs.push(...byPort(confirmed.subarray(i, i + players), this.#seats));
      }
    }
    // While a state is being packed, inputs wait: they must reach the watchers after it.
    if (stream.packing || !stream.inputs.length || (!now && performance.now() - stream.sentAt < STREAM_EVERY)) return;
    const inputs = Uint16Array.from(stream.inputs);
    postMessage({ type: "watch-inputs", stream: stream.id, frame: stream.frame, inputs }, [inputs.buffer]);
    stream.frame += stream.inputs.length / PORTS;
    stream.inputs = [];
    stream.sentAt = performance.now();
  }

  /**
   * The machine at the frame the stream goes on from, for one watcher or all. Online it's the
   * save GGRS made before that frame, unless the machine is there now (always, in lockstep).
   */
  async #sendState(to) {
    this.#flush(true);
    const stream = this.#stream;
    const { id, frame } = stream;
    const session = this.#session;
    const saved = session && !this.#lockstep && frame < session.currentFrame();
    stream.packing = true;
    const state = await pack(saved ? this.core.slotBytes(session.slot(frame)) : this.core.serialize());
    stream.packing = false;
    if (this.#stream !== stream) return; // the stream started over meanwhile
    const bytes = new Uint8Array(4 + state.length);
    new DataView(bytes.buffer).setUint32(0, frame, true);
    bytes.set(state, 4);
    postMessage({ type: "watch-state", stream: id, bytes, to }, [bytes.buffer]);
    this.#flush(true); // the frames run while packing
  }

  async #watchFrom(bytes) {
    const frame = new DataView(bytes.buffer, bytes.byteOffset).getUint32(0, true);
    // The stream's inputs from this frame on may arrive while the state inflates: kept from
    // now, run once the machine is at the state.
    const watch = (this.#watch = { frames: [], end: frame, waiting: true });
    this.paused = true;
    const state = await unpack(bytes.subarray(4));
    if (this.#watch !== watch) return; // a newer state came meanwhile
    this.core.reset();
    this.core.unserialize(state);
    this.paused = false;
    this.#alarm.now();
  }

  #watchInputs({ frame, inputs }) {
    const watch = this.#watch;
    if (!watch) return;
    if (frame > watch.end) {
      console.warn(`Watching: frames ${watch.end}-${frame - 1} never came`);
      return;
    }
    for (let i = (watch.end - frame) * PORTS; i < inputs.length; i += PORTS) {
      watch.frames.push(inputs.subarray(i, i + PORTS));
    }
    watch.end = Math.max(watch.end, frame + inputs.length / PORTS);
  }
}

/**
 * Picks the rollback limit from how fast this machine runs the game: as many re-run frames as
 * fit in 3/4 of a frame next to the shown one, 2 to 8. Slow games (Mortal Kombat II) get fewer
 * and one more frame of input delay, so rollbacks stay short. Leaves the machine as it found it.
 */
function tune(core, fps) {
  const before = core.serialize();
  const msPerFrame = (present, frames) => {
    core.present = present;
    const startedAt = performance.now();
    for (let i = 0; i < frames; i++) core.run();
    return (performance.now() - startedAt) / frames;
  };
  msPerFrame(false, 30); // warm up
  const rerun = msPerFrame(false, 60);
  const shown = msPerFrame(true, 20);
  core.present = true;
  core.unserialize(before);
  const fits = Math.floor(((1000 / fps) * 0.75 - shown) / rerun);
  const rollback = Math.min(8, Math.max(2, fits));
  // A heavy core (Supermodel) can't re-simulate cheaply, so it covers the ping with input delay
  // to keep rollbacks rare, rather than rolling back often.
  return { rollback, delay: rollback < 6 ? 3 : 2, heavy: rerun > HEAVY_RERUN_MS };
}

/**
 * A lockstep game's input delay, in frames, for a round trip: the others' input for a frame,
 * sent when they ran the frame this many frames earlier, has to be here by the time the frame
 * is due. Half the round trip and some slack for jitter, plus a frame for the two machines'
 * slots not lining up and one for landing while this machine is busy with a frame (it polls
 * between frames). MIN_DELAY to MAX_DELAY frames; the game then adds to that when the others'
 * input still arrives late (Cabinet.#tune).
 */
function onlineDelay(roundTrip, fps) {
  if (roundTrip === undefined) return DEFAULT_DELAY;
  const frameMs = 1000 / fps;
  const frames = Math.ceil((roundTrip / 2 + DELAY_SLACK_MS) / frameMs) + 2;
  return Math.min(MAX_DELAY, Math.max(MIN_DELAY, frames));
}

/**
 * FNV-1a over the game's RAM, every `stride`-th word of it (all of it by default). All
 * machines' hashes match while they're in step.
 */
function hashRam(bytes, stride = 1) {
  let hash = 0x811c9dc5;
  if (bytes.byteOffset % 4 === 0) {
    const words = new Uint32Array(bytes.buffer, bytes.byteOffset, bytes.length >> 2);
    for (let i = 0; i < words.length; i += stride) hash = Math.imul(hash ^ words[i], 0x01000193);
    if (stride === 1) for (let i = words.length * 4; i < bytes.length; i++) hash = Math.imul(hash ^ bytes[i], 0x01000193);
  } else {
    for (let i = 0; i < bytes.length; i += stride) hash = Math.imul(hash ^ bytes[i], 0x01000193);
  }
  return hash >>> 0;
}
