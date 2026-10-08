// Runs a cabinet's core (FBNeo, Supermodel, MAME or Daytona USA's, through libretro.js) off the
// main thread. Posts each frame to the page (which hands it to Bevy) and streams audio to the
// AudioWorklet (audio.js).
//
// Alone, the local player's buttons drive their seat's controller port. Online, every seated
// player's machine runs the same game in step with rollback (netplay/src/lib.rs): GGRS guesses
// the others' input and re-runs frames once the real input arrives. Its packets go out and come
// in on `port` as [seat, bytes], and the page carries them between the players. A `lockstep`
// game (Supermodel: a 32 MB state, too slow to save every frame) runs in step without guessing:
// some frames of input delay, and a wait whenever the others' input is late. Each starts
// with its session's epoch, so packets still in flight from an earlier session are dropped.
//
// Frames run on a precise clock (Alarm) at the core's own rate (FBNeo and Supermodel say 60 Hz,
// Daytona USA's Model 2 57.524 Hz): one frame at its slot, the picture posted, then the next.
// The sound keeps up by itself: a frame's samples are 1/fps of a second of it, however many
// that is (800 at 60 Hz and 48 kHz, 834 or 835 at 57.524 Hz). A heavy core (a Model 3 frame
// is ~12 ms) never runs two frames back to back to catch up, since that holds its inputs and
// its picture for the whole burst; it catches up one frame per wake-up. GGRS's packets leave
// the moment it makes them, so online the two machines each run their frame in the same slot
// and swap one input per frame. A lockstep game's input delay follows the other machines'
// lateness: a frame more when their input keeps arriving late, a frame less after a quiet
// stretch.
//
// When players join or leave, one machine (the page picks it) captures the game as it is and
// every player starts a new session from that capture.
//
// A `linked` game (Time Crisis II) is two boards joined by the game's serial link, one per
// player, as its twin cabinet is (linked.js): each player's controls go to their own board
// only, on port 0, and what each board transmits goes to the other D frames later (D from the
// round trip, fixed for the session). Seat 0 is the Left/Red board, seat 1 the Right/Blue. The
// link is set at power-on, so a game can't become linked midway: when the second player sits
// down both boards start over from their side's start-up state (linked, in attract mode, with
// credits), and when one leaves the other starts over alone from the game's own start-up
// state. No GGRS: a LinkedSession, paced as lockstep is.
//
// People watching the cabinet run the game too, a little behind: one player's machine (the page
// picks it) streams them a state and then every player's input for each frame from there on,
// only frames no rollback can change anymore. The watchers' machines play those frames as they
// come in, keeping a few in hand so they play evenly. A linked board streams its own frames
// instead, each its player's input and the link bytes the board was handed before it, and the
// watcher's board, linked on the same side, is handed the same.
//
// An `arcade` game (Daytona USA) is linked cabinets, as in an arcade, and none of the above:
// this machine is the player's own cabinet only (the core's `seat` setting, which this sets),
// started from their seat's state and running from then on, whoever else sits down or leaves;
// nothing waits for anyone, there is no capture or session. The cabinets' link boards talk
// through the players' browsers instead, the way the arcade's link cable joined them, so the
// game's own rules say who races whom. After each frame this cabinet's block of link data goes
// out on `port` (an ArrayBuffer for every other cabinet: [0xda][seat: u8][frame: u32 LE][block])
// when it changed, and again now and then when it didn't (in case one went missing); the
// others' come in on `port` as [seat, packet], and the newest from each seat goes into the core
// before the next frame (Core.linkIn). A cabinet that hears nothing from a seat goes on with
// what it last heard. Watching one: its machine's state, the blocks it had then, and for each
// frame its controls and the blocks that went in before it ("watch-frames"), so the watcher's
// machine sees exactly what the player's did. A cabinet left waiting for good by a player who
// stood up between Start and the race (Daytona USA has no timeout for it) goes back to its
// seat's state, the attract mode (CabinetLink, #watchGame). (Not to be confused with a `linked`
// game, above: two boards in lockstep, each player's own, joined by a serial link.)
//
// Each player's controls for a frame are a u32 (the "input"): the RetroPad mask in the low 16
// bits, and for a `gun` game where the lightgun aims in the high 16, x in bits 16-23 (0 the
// left edge of the screen, 255 the right) and y in bits 24-31 (0 the top, 255 the bottom). At a
// driving game (Out Run) bits 16-23 are where the wheel is turned, which this worker works out
// from the local player's arrows as it samples them (wheel.js); the page only sends buttons.
//
// In:  { type: "start", core, rom, files, state, seat, turns, lockstep, gun, linked, linkState,
//        arcade, options, port, hold }
//        Loads the game. `files` (a BIOS) and `state` are optional: skipped if missing. Plays
//        alone right away, or with `hold` waits for "online" (joining a game in progress) or
//        "watch-state" (watching; no `seat` or `port` then). `linkState`, for a linked game's
//        player: their side's start-up state for linked play (mame/link-states.mjs). `options`
//        are the core's own settings ({ key: value }, strings; Core.setOption), made before the
//        game loads. With a `view` among them the machine has a screen per seat (Daytona USA as
//        the twin cabinet): it shows `seat`'s (a watcher's, seat 0's), set again after every
//        state it loads, since states may carry the one of the machine that made them.
//        An `arcade` game also gets `seat` among them (a watcher's: the seat it watches), and
//        `state` is the seat's (start-up states may be deflated, as below).
//      { type: "view", view } Watching such a game: shows seat `view`'s screen instead.
//      { type: "link-seats", seats } An arcade game: who sits at each seat ({ seat: player id },
//        or null). A seat left empty is off the link before the next frame (Core.linkAbsent);
//        someone new at a seat starts over the frame count its packets carry.
//      { type: "capture", epoch } Stops and captures the machine, for a change of players.
//        States that leave this worker (captured, watch-state) are deflated: a Model 3's 30 MB
//        is two thirds zeros and packs to 5 MB; they come back the same way (online, watch-state).
//      { type: "online", epoch, seats, state, roundTrip } Plays in step with the players in
//        `seats` (their seat numbers, ascending) from `state`, or from this machine's capture
//        for `epoch`; `roundTrip` (ms, to the farthest of them) sets a lockstep game's first
//        input delay. A linked game starts over linked instead (no state: its own start-up
//        one), `roundTrip` setting the link's delay.
//      { type: "solo" } Everyone else left: play on alone (a linked game starts over alone).
//      { type: "stream", on } Streams this machine's game to the watchers, or stops.
//      { type: "snapshot", to } A state for a new watcher, to go on from with the stream.
//      { type: "watch-state", bytes, stream, seat } | { type: "watch-inputs", frame, inputs } |
//        { type: "watch-link", records } | { type: "watch-frames", frame, bytes } Watching: a
//        stream's state and frames, as they're sent out (below); `stream` names the stream, so
//        that another state of it can be passed over, and an arcade game's state is the
//        cabinet at `seat` (the core's seat is set before it loads).
//      { type: "probe", at } For checks: a "probe" message when the machine gets to frame `at`
//        (playing online or watching), or without `at` after the next frame it runs.
//      { type: "input", input } the local player's controls (a u32, above) |
//      { type: "audio", port, sampleRate } the speaker's port and rate
// Out: { type: "ready", state } the game is loaded (`state`: from the start's state) |
//      { type: "frame", rgba, width, height } |
//      { type: "netplay", event, seat, ... } | { type: "captured", epoch, state } |
//      { type: "buttons", buttons } what the game calls player 1's buttons, [RetroPad id, name]
//      pairs, once known | { type: "watch-state", stream, bytes, to } the machine for watchers
//      to start from, [frame: u32 LE][state], for the watcher `to` or, without it, for all |
//      { type: "watch-inputs", stream, frame, inputs } a Uint32Array with an input per controller
//      port (4) for each frame from `frame` on, a few times a second. `stream` counts up each
//      time the stream starts over (a new session); inputs go on from that stream's states.
//      A linked board's stream: { type: "watch-link", stream, records } instead, its frames
//      as linked.js encodes them ([frame][input][link bytes] each), and its watch-state's
//      state is marked with the board's side (linked.js markLinked) | { type: "probe", frame,
//      hash, link } the machine at a probe's frame: a hash of its RAM, and the link's status.
//      An arcade game's watch-state is [frame: u32 LE][the link: per seat (8) a byte, 1 when a
//      block follows, | 2 when the seat is off the link, and the block][state], and its frames
//      come as { type: "watch-frames", stream, frame, bytes }: per frame [controls: u16 LE]
//      [count: u8] and that many [seat: u8, | 0x80 when it went off the link][the block, as
//      changes from that seat's last: runs of [unchanged: u8][changed: u8][changed bytes]].
//      The controls are the input's low 16 bits, all there is to an arcade game's (it has no
//      gun): the cabinet runs on them alone, and the watcher's machine widens them back to an
//      input. { type: "arcade", fps, frame, blocksIn, blocksOut, link } An arcade game, once a
//      second: frames shown, frames run, blocks taken in and sent out in that second, and the
//      core's link status (Core.linkStatus). { type: "arcade-cancelled" } its cabinet went back
//      to the attract mode, its race left waiting by someone who stood up before it began.
//      In lockstep, every 120 frames the machines compare a hash of the game's RAM (the worker's
//      own packets, marked with epoch 0xffff): a mismatch is the "desync" netplay event
//      (seat, frame), once per session, and the page then has one machine hand its game to
//      everyone again. The "stats" netplay event, once a second: ping (ms), delay (frames of input delay),
//      rollback (frames; 0 in lockstep), fps (frames shown), stalls (waits for the others'
//      input in that second), stallMs (the longest), look (the others' input in hand at a frame:
//      [least, median, most] frames), prefills (looks that waited for more), framesAhead, frame
//      (the session's next frame), runMs (what a frame costs the core, on average); and
//      for a linked game `link`: the board's link as the game has it (linked.js) and `lost`,
//      true while nothing comes from the other board.
import { Core } from "./libretro.js";
import { LinkedSession, decodeRecords, encodeRecords, markLinked, parseLinkState, unmarkLinked } from "./linked.js";
import { Resampler } from "./resample.js";
import { Wheel } from "./wheel.js";

/** Controller ports, as many as libretro.js has. */
const PORTS = 4;
/** How often watchers get the frames since the last time, in ms. */
const STREAM_EVERY = 100;
/** Frames a watcher has in hand before playing: a little more than arrive at once. */
const WATCH_BUFFER = 12;
/**
 * Game packets carry their session's epoch; this one marks the worker's own packets instead:
 * a checkpoint, 'h', the epoch (u16), the frame (u32) and a hash of the game's RAM there (u32).
 * Lockstep machines compare them to notice when they have drifted apart (rollback games leave
 * that to GGRS, which hashes confirmed frames).
 */
const CONTROL = 0xffff;
/** Frames between checkpoints. */
const HASH_EVERY = 120;
/** Every this many words of RAM go into a checkpoint's hash: a drift spreads, so a sample catches it within a checkpoint or two. */
const HASH_STRIDE = 4;
/** Marks a deflated state: "vabz", then the deflate-raw bytes. */
const PACKED = Uint8Array.of(0x76, 0x61, 0x62, 0x7a);
/** An arcade game's link packets start with this, then [seat: u8][frame: u32 LE][block]. */
const LINK_PACKET = 0xda;
/** Seats on an arcade game's link (Daytona USA's cabinets). */
const LINK_SEATS = 8;
/** An arcade cabinet sends its block again after this many frames without a change. */
const RESEND_FRAMES = 60;
/**
 * Daytona USA, the arcade game, as its main RAM says (from 0x500000; daytona/ring-notes.md, "The
 * bridge"): the cars on the track (10 outside a race, 16 in a linked race of two, 40 alone), the
 * entrants in the session this cabinet sees (0 in the attract mode; an idle cabinet counts
 * another's session's too), and the game's mode: 3 to 11 the attract mode, 16 idle while
 * another's session takes entries (WAITING FOR YOUR ENTRY), ENTERED from this cabinet's Start
 * (circuit select, mission select) until its race (22).
 */
const CARS = 0x1080;
const ENTRANTS = 0x40027;
const MODE = 0x10a0;
const ENTERED = 18;
const NO_RACE = 10;
/**
 * A cabinet is ENTERED for 1866 frames (32.4 s) at most when nobody leaves: from the session's
 * first Start, with no circuit or mission chosen and every count run out (1580 with them chosen).
 * One still there this many frames (37.5 s) after its Start, a player having left meanwhile,
 * waits for them for good: the game has no timeout.
 */
const STRANDED_FRAMES = 2160;

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

/** A "probe" for whichever frame runs next. */
const NEXT_FRAME = -1;
/** A linked game's packets kept for a session that hasn't started here yet: a minute's worth. */
const MAX_EARLY = 3600;
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
/** The local player's controls as last sent: buttons and, on a lightgun game, aim. */
let localInput = 0;
/** Buttons that went down since the game last read the controls: a tap between two frames
 *  still counts for the next one. The aim is simply the latest. */
let pressed = 0;
/** The RetroPad mask in an input; the aim is above it. */
const BUTTONS = 0xffff;
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
    const input = msg.input >>> 0;
    pressed |= input & ~localInput & BUTTONS;
    localInput = input;
  } else if (msg.type === "audio") ({ port: audioPort, sampleRate: speakerRate = speakerRate } = msg);
  else if (msg.type === "start") start(msg);
  // Anything else is for the loaded game; it can arrive while the game still downloads.
  else if (cabinet) cabinet.handle(msg);
  else waiting.push(msg);
};

/** At a driving game, the wheel the local player's arrows turn (wheel.js). */
const wheel = new Wheel();

/**
 * The local controls for the frame about to run. At a driving game the wheel, turned a frame's
 * worth by the arrows held, goes in it (bits 16-23), so whoever runs the frame turns it the same.
 */
function sampleInput() {
  const input = (localInput | pressed) >>> 0;
  pressed = 0;
  return cabinet?.steers ? wheel.turn(input) : input;
}

// "no-cache" checks with the server every time (a 304 when unchanged), so newly uploaded or
// replaced ROMs, BIOS sets and states are picked up.
const download = async (url) => {
  const response = await fetch(url, { cache: "no-cache" });
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
};
const downloadIfPresent = (url) => url && download(url).catch(() => undefined);

async function start({
  core: coreUrl, rom: romUrl, files = [], state: stateUrl, seat = 0, turns = false, lockstep = false, gun = false,
  linked = false, linkState: linkStateUrl, arcade = false, options = {}, port, hold,
}) {
  const { default: createCore } = await import(coreUrl);
  const [rom, packedState, packedLinkState, ...extras] = await Promise.all([
    download(romUrl),
    downloadIfPresent(stateUrl),
    downloadIfPresent(linkStateUrl),
    ...files.map(downloadIfPresent),
  ]);
  // Start-up states may come deflated (worker states are; mame/link-states.mjs makes them so).
  const state = packedState && (await unpack(packedState));
  let linkState = packedLinkState && parseLinkState(packedLinkState);
  if (linkState) linkState = { ...linkState, state: await unpack(linkState.state) };
  else if (linkStateUrl && linked) console.warn(`${linkStateUrl}: no start-up state for linked play; linked boards boot instead`);
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
  }, { canvas }), { seat, turns, lockstep, linked, arcade, port });
  const core = cab.core;
  core.netplay = true;
  core.gun = gun;
  files.forEach((url, i) => extras[i] && core.addFile(url.split("/").pop(), extras[i]));
  // An arcade game's machine is the seat's cabinet.
  if (arcade) options = { ...options, seat: String(seat) };
  for (const [key, value] of Object.entries(options)) core.setOption(key, String(value));
  const { fps, sampleRate } = core.loadGame(romUrl.split("/").pop(), rom);
  // Pacing, input delay and the stats all go by the core's rate, whatever it is (see the top).
  cab.fps = fps;
  // The speaker runs at one rate; a core at another (Supermodel, 44.1 kHz) is brought to it.
  if (Math.round(sampleRate) !== speakerRate) cab.resampler = new Resampler(sampleRate, speakerRate);
  // A start-up state (emulator/snapshot.mjs) skips the boot screens and adds credits. States
  // from an older core build don't load; the game then just boots normally. An arcade seat's
  // is the cabinet on the link, in the attract mode (loaded after a power-on, as any state from
  // another machine).
  let loaded = false;
  try {
    if (state) {
      if (arcade) core.reset();
      core.unserialize(state);
      loaded = true;
      cab.soloState = state;
      // An arcade cabinet goes back to it when its race is left waiting for good (CabinetLink).
      if (arcade && !hold) cab.seatState = state;
    }
  } catch (error) {
    console.warn(`${stateUrl}: ${error.message}`);
  }
  cab.linkState = linkState;
  if (arcade && !hold && !loaded) console.warn(`${stateUrl ?? "No state"}: the cabinet starts from power-on, off the link`);
  if ("view" in options) cab.show(seat);
  cab.paused = Boolean(hold);
  await netplay;
  cabinet = cab;
  postMessage({ type: "ready", state: loaded });
  for (const msg of waiting.splice(0)) cab.handle(msg);
  cab.tick();
}

/** An input per controller port: `inputs[i]` on seat `seats[i]`'s, nothing on the others. */
function byPort(inputs, seats) {
  const ports = new Uint32Array(PORTS);
  inputs.forEach((input, i) => (ports[seats[i]] = input));
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
  /** The core's frame rate (loadGame's), until the game loads. */
  fps = 60;
  /** An arcade game: the seat's state the cabinet started from. */
  seatState;
  /** Brings the core's sound to the speaker's rate, when they differ. */
  resampler;
  /** The game's start-up state, alone (linked games start over from it when left alone). */
  soloState;
  /** A linked game's start-up state for this seat's board (linked.js parseLinkState). */
  linkState;

  #seat;
  #lockstep;
  /** A linked game (linked.js): the board's side while its link is plugged in, undefined while not. */
  #linked;
  #linkSide;
  /** The link's status at the last stats, and whether it has gone quiet (lost). */
  #link;
  #linkLost = false;
  /** Frames whose RAM hash checks asked for ("probe"). */
  #probes = new Set();
  /**
   * A linked game's packets that came before their session started here ([seat, epoch,
   * bytes]): the other board starts ticking as soon as it can, and nothing is sent twice.
   */
  #early = [];
  #port;
  /** Which seat's screen the machine shows, for a core with one per seat; else undefined. */
  #view;
  /** An arcade game: the player's cabinet on the link (CabinetLink), or a watcher's (true). */
  #arcade;
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
   * Streaming to watchers: the stream's number, the frame they get next, inputs (one per port
   * per frame) not sent yet, and when inputs last went out.
   */
  #stream;
  #streams = 0;
  /** The last state for watchers on its way (`#sendState`), the next one goes after it. */
  #statesSent = Promise.resolve();
  /**
   * Watching: frames to run (an input per port each), the frame after the last, and whether
   * it's waiting to have a few in hand.
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

  constructor(core, { seat, turns, lockstep, linked, arcade, port }) {
    this.core = core;
    core.turns = turns;
    this.#seat = seat;
    this.#lockstep = lockstep;
    this.#linked = linked && core.hasLink;
    this.#port = port;
    if (arcade) this.#arcade = port ? new CabinetLink(core, seat, port) : true;
    if (!port || arcade) return; // watching, or the link takes the port
    port.onmessage = ({ data: [seat, packet] }) => {
      const handle = this.#seats?.indexOf(seat) ?? -1;
      const bytes = new Uint8Array(packet);
      const epoch = bytes[0] | (bytes[1] << 8);
      if (epoch === CONTROL) return this.#control(seat, bytes);
      if (handle < 0 || epoch !== this.#epoch || !this.#session) {
        // A later session's (epochs count up): kept for when it starts here.
        const later = this.#epoch === undefined || ((epoch - this.#epoch) & 0xffff) - 1 < 0x7fff;
        if (this.#linked && later && this.#early.length < MAX_EARLY) this.#early.push([seat, epoch, bytes]);
        return;
      }
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
      if (this.#linkSide !== undefined) this.#unlink();
      if (this.#stream) this.#startStream();
    }
    if (msg.type === "stream") {
      if (!msg.on) this.#stream = undefined;
      else if (!this.#stream) this.#startStream();
    }
    if (msg.type === "snapshot" && this.#stream) this.#sendState(msg.to);
    if (msg.type === "watch-state") this.#watchFrom(msg.bytes, msg.stream, msg.seat);
    if (msg.type === "watch-inputs") this.#watchInputs(msg);
    if (msg.type === "watch-link") this.#watchLink(msg.records);
    if (msg.type === "watch-frames") this.#watchFrames(msg);
    if (msg.type === "link-seats" && this.#arcade instanceof CabinetLink) this.#arcade.seated(msg.seats);
    if (msg.type === "view" && this.#view !== undefined) this.show(msg.view);
    if (msg.type === "probe") this.#probes.add(msg.at ?? NEXT_FRAME);
  }

  /**
   * Shows seat `view`'s screen, on a core with one per seat (its "view" setting), or again the
   * one it showed: after loading a state, which may carry the view of the machine that made it.
   */
  show(view = this.#view) {
    if (view === undefined) return;
    this.#view = view;
    this.core.setOption("view", String(view));
  }

  /**
   * Whether the local player steers a wheel (a driving game): on their own controller port, the
   * seat's, or the first on a linked board or an arcade cabinet.
   */
  get steers() {
    return this.core.steers(this.#linked || this.#arcade ? 0 : this.#seat);
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
          this.#probe(session.currentFrame());
          this.#next += this.#pace(session, lookahead, frameMs);
          // Lockstep only: a rollback game's latest frame may have run on guessed inputs, so
          // its RAM can rightly differ for a while; GGRS compares confirmed frames there itself.
          if (this.#lockstep && session.currentFrame() % HASH_EVERY === 0) this.#checkpoint(session.currentFrame());
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
          const frame = watch.frames.shift();
          if (this.#arcade) this.#watchLinked(frame);
          else {
            const { ports, link } = frame;
            // A linked board's frame: the link bytes the player's board was handed before it.
            if (link) this.core.linkIncoming(link);
            this.#run(ports, true);
            if (link) this.core.linkOutgoing(); // what it transmits goes nowhere
          }
          this.#probe(watch.end - watch.frames.length);
          // A little faster while far behind the stream (frames came in after a hiccup).
          this.#next += watch.frames.length > WATCH_BUFFER * 2.5 ? frameMs * 0.9 : frameMs;
        }
      }
      this.#alarm.at(watch.waiting ? performance.now() + WAIT_POLL_MS : this.#next);
    } else if (this.#arcade) {
      for (let i = 0; i < this.#perWake() && performance.now() >= this.#next; i++) {
        this.#runLinked(sampleInput());
        this.#next += frameMs;
      }
      this.#arcadeStats(performance.now());
      this.#alarm.at(this.#next);
    } else {
      for (let i = 0; i < this.#perWake() && performance.now() >= this.#next; i++) {
        // A linked game's player plays their own board, on its player 1 controls.
        const ports = byPort([sampleInput()], [this.#linked ? 0 : this.#seat]);
        this.#run(ports, true);
        this.#probe(NEXT_FRAME);
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
      frame: session.currentFrame(), runMs: Math.round(this.#runMs * 10) / 10,
      ...(this.#linkSide !== undefined && this.#linkStats(session)),
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
    // A linked game's delay is the link's: it can't change during the session (linked.js).
    if (!this.#tuned.heavy || this.#linkSide !== undefined || session.currentFrame() < WARMUP_FRAMES) return;
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

  /**
   * The link as the game has it, for the stats once a second: plugged in, the side, the game's
   * link mode word (2 in a linked game, 0 in attract mode), its keepalive word, its link state
   * machine's counter, bytes waiting, frames and bytes each way, and `lost`: a whole second of
   * frames run with nothing from the other board. Linked boards talk all the time, a few link
   * frames a tick in attract mode and about one a tick in a game (mame/README.md), so silence
   * means the other board's game has let the link go. (The mode word alone can't say: it also
   * leaves 2 when a linked game ends.)
   */
  #linkStats(session) {
    const was = this.#link;
    const link = (this.#link = { ...this.core.linkStatus(), frame: session.currentFrame() });
    if (was && link.frame - was.frame >= 30) this.#linkLost = link.rxFrames === was.rxFrames;
    const { linked, side, mode, keepalive, counter, pending, txFrames, rxFrames, txBytes, rxBytes } = link;
    return {
      // The session's D, once both boards' proposals are in (linked.js).
      delay: session.agreedDelay ?? session.delay,
      link: { linked, side, mode, keepalive, counter, pending, txFrames, rxFrames, txBytes, rxBytes, heard: session.heard },
      lost: this.#linkLost && session.heard,
    };
  }

  // GGRS's requests, run on the core (and a linked session's: its board's own input on port 0).
  #machine = {
    save: (slot, checksum) => {
      this.core.saveSlot(slot);
      return checksum ? hashRam(this.core.systemRam()) : undefined;
    },
    load: (slot) => this.core.loadSlot(slot),
    run: (inputs, present) => this.#run(byPort(inputs, this.#linkSide !== undefined ? [0] : this.#seats), present),
    send: (packets) => this.#post(packets),
  };

  /**
   * A check asked for the machine at this frame, or after whichever frame runs next ("probe"):
   * a hash of its RAM, and the link.
   */
  #probe(frame) {
    if (!this.#probes.delete(frame) && !this.#probes.delete(NEXT_FRAME)) return;
    const link = this.core.hasLink ? this.core.linkStatus() : undefined;
    postMessage({ type: "probe", frame, hash: hashRam(this.core.systemRam()), link });
  }

  /** Runs a frame with an input per controller port. */
  #run(inputs, present) {
    const ports = this.core.inputs;
    ports.set(inputs);
    // Core routes shared upright gameplay and descriptor-defined Start/Coin aliases;
    // the original inputs remain intact for rollback and the spectator input stream.
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
    if (this.#linked) return this.#linkUp(epoch, seats, roundTrip);
    state = state ? await unpack(state) : this.#captured?.epoch === epoch ? this.#captured.state : undefined;
    if (!state) return;
    this.#leaveSession();
    // Power-on, then the state: every machine then has the same of what the state leaves out.
    this.core.reset();
    this.core.unserialize(state);
    this.show();
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
    // GGRS takes whole frames per second, for its guess of how far ahead the others are (57.524
    // makes 58: under 1% off, and lockstep doesn't pace by that guess anyway).
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

  /**
   * A linked game's two players: this board starts over linked, on its seat's side, from its
   * start-up state for that (as the other board does at the same time), and the session hands
   * it the other board's link bytes D ticks after they were sent: D from the round trip, the
   * larger of the two boards' (linked.js).
   */
  #linkUp(epoch, seats, roundTrip) {
    this.#leaveSession();
    const side = this.#seat;
    const delay = onlineDelay(roundTrip, this.fps);
    this.core.linkSet(true, side);
    this.core.reset();
    // Without a start-up state the boards boot, linked from power-on: they link up at the
    // NAMCO screen after the power-on test, about 15 s.
    if (this.linkState) this.core.unserialize(this.linkState.state);
    this.#linkSide = side;
    this.#link = undefined;
    this.#linkLost = false;
    this.#captured = undefined;
    this.#tuned = { rollback: 0, heavy: true, delay };
    this.#delay = delay;
    this.#delayFloor = delay;
    this.#epoch = epoch & 0xffff;
    this.#seats = seats;
    this.#session = new LinkedSession({
      link: { incoming: (bytes) => this.core.linkIncoming(bytes), outgoing: () => this.core.linkOutgoing() },
      local: seats.indexOf(this.#seat),
      delay,
      carried: this.linkState?.carried ?? [],
      roundTrip,
    });
    for (const [seat, packetEpoch, bytes] of this.#early.splice(0)) {
      if (packetEpoch === this.#epoch && seats.includes(seat)) this.#session.receive(seats.indexOf(seat), bytes.subarray(2));
    }
    this.paused = false;
    this.#waiting = false;
    this.#stall = undefined;
    this.#stalls = [];
    this.#events = [`linked session from epoch ${this.#epoch}, side ${side}, delay ${delay}`];
    this.#next = performance.now();
    if (this.#stream) this.#startStream();
    this.#alarm.now();
  }

  /** The other player left a linked game: this board starts over alone, link unplugged. */
  #unlink() {
    this.core.linkSet(false);
    this.core.reset();
    if (this.soloState) this.core.unserialize(this.soloState);
    this.#linkSide = undefined;
    this.#link = undefined;
  }

  #leaveSession() {
    this.#session?.free();
    this.#session = undefined;
    this.#seats = undefined;
    this.#waiting = false;
    this.paused = false;
  }

  /**
   * Streams from here: the machine as it is now, or online as of the last confirmed frame. A
   * linked board streams its own frames, each with the link bytes it was handed (`linked`); an
   * arcade cabinet counts its frames from its start, the number its link packets carry.
   */
  #startStream() {
    const link = this.#arcade instanceof CabinetLink ? this.#arcade : undefined;
    const frame = this.#session ? this.#session.confirmedFrame() + 1 : link ? link.frame : 0;
    // `fresh` until its first state is out: its frames wait for it. Its `records` are a linked
    // board's frames (linked.js) or an arcade cabinet's (#runLinked); `inputs` anyone else's.
    this.#stream = { id: ++this.#streams, frame, inputs: [], records: [], linked: this.#linkSide !== undefined, sentAt: 0, fresh: true, packing: true };
    this.#sendState();
  }

  /**
   * Sends the frames since the last time, a few times a second (or now with `now`). Online,
   * those are the frames GGRS has confirmed since; alone, every frame run.
   */
  #flush(now = false) {
    const stream = this.#stream;
    if (this.#arcade) {
      if (stream.packing || !stream.records.length || (!now && performance.now() - stream.sentAt < STREAM_EVERY)) return;
      const bytes = new Uint8Array(stream.records.reduce((total, record) => total + record.length, 0));
      stream.records.reduce((at, record) => (bytes.set(record, at), at + record.length), 0);
      postMessage({ type: "watch-frames", stream: stream.id, frame: stream.frame, bytes }, [bytes.buffer]);
      stream.frame += stream.records.length;
      stream.records = [];
      stream.sentAt = performance.now();
      return;
    }
    if (stream.linked) return this.#flushLinked(stream, now);
    if (this.#session) {
      const players = this.#seats.length;
      const confirmed = this.#session.confirmedInputs(stream.frame + stream.inputs.length / PORTS);
      for (let i = 0; i < confirmed.length; i += players) {
        stream.inputs.push(...byPort(confirmed.subarray(i, i + players), this.#seats));
      }
    }
    // While a state is being packed, inputs wait: they must reach the watchers after it.
    if (stream.packing || !stream.inputs.length || (!now && performance.now() - stream.sentAt < STREAM_EVERY)) return;
    const inputs = Uint32Array.from(stream.inputs);
    postMessage({ type: "watch-inputs", stream: stream.id, frame: stream.frame, inputs }, [inputs.buffer]);
    stream.frame += stream.inputs.length / PORTS;
    stream.inputs = [];
    stream.sentAt = performance.now();
  }

  /** A linked board's frames since the last time, as linked.js encodes them. */
  #flushLinked(stream, now) {
    const records = stream.records;
    if (this.#session) records.push(...this.#session.records(stream.frame + records.length));
    if (stream.packing || !records.length || (!now && performance.now() - stream.sentAt < STREAM_EVERY)) return;
    // A few seconds' worth at most per message (the room takes up to 1 MiB).
    for (let at = 0; at < records.length; at += 120) {
      const encoded = encodeRecords(records.slice(at, at + 120));
      postMessage({ type: "watch-link", stream: stream.id, records: encoded }, [encoded.buffer]);
    }
    stream.frame += records.length;
    stream.records = [];
    stream.sentAt = performance.now();
  }

  /**
   * The machine at the frame the stream goes on from, for one watcher or all. Online it's the
   * save GGRS made before that frame, unless the machine is there now (always, in lockstep and
   * on a linked board, whose state then says which side it's linked on). One state at a time,
   * in order: packing a big one (a Model 3's or a System 23's) takes a while, and a state that
   * went out after a newer one would send a watcher back to frames it never gets again.
   */
  #sendState(to) {
    const stream = this.#stream;
    this.#statesSent = this.#statesSent.then(() => this.#packState(stream, to)).catch((error) => console.warn(`Watch state: ${error.message}`));
  }

  async #packState(stream, to) {
    if (this.#stream !== stream) return; // the stream started over (or stopped) meanwhile
    const session = this.#session;
    const cabinetLink = this.#arcade instanceof CabinetLink ? this.#arcade : undefined;
    if (stream.fresh) {
      // Nothing of this stream has gone out yet (its frames wait for its first state, which
      // may have waited for an earlier one): it starts where the machine is now (an arcade
      // cabinet's at the frame count its link packets carry).
      stream.frame = session
        ? session.confirmedFrame() + 1
        : cabinetLink
          ? cabinetLink.frame
          : stream.frame + stream.inputs.length / PORTS;
      stream.inputs = [];
      stream.records = [];
    } else this.#flush(true);
    const { id, frame } = stream;
    const saved = session && !this.#lockstep && !stream.linked && frame < session.currentFrame();
    const side = this.#linkSide;
    // The frames run while it packs wait: they must reach the watchers after it.
    stream.packing = true;
    // An arcade cabinet's link goes with it: the blocks it has from the others, which its state
    // leaves out. Taken with the state, at the same frame: nothing runs before both are.
    const link = cabinetLink ? cabinetLink.table() : new Uint8Array(0);
    let state;
    try {
      state = markLinked(await pack(saved ? this.core.slotBytes(session.slot(frame)) : this.core.serialize()), side);
    } finally {
      stream.packing = stream.fresh;
    }
    if (this.#stream !== stream) return; // the stream started over meanwhile
    const bytes = new Uint8Array(4 + link.length + state.length);
    new DataView(bytes.buffer).setUint32(0, frame, true);
    bytes.set(link, 4);
    bytes.set(state, 4 + link.length);
    postMessage({ type: "watch-state", stream: id, bytes, to }, [bytes.buffer]);
    stream.fresh = stream.packing = false;
    this.#flush(true); // the frames run while packing
  }

  async #watchFrom(bytes, stream, seat) {
    const frame = new DataView(bytes.buffer, bytes.byteOffset).getUint32(0, true);
    // Another state of the stream this machine plays already (sent to everyone as it started,
    // and to us as we came, say), which it has the frames to get to: it goes on as it is, as
    // starting over would drop the frames that came in between. After a gap it starts over.
    if (stream !== undefined && this.#watch?.stream === stream && this.#watch.end >= frame) return;
    // An arcade cabinet's: the blocks the player's machine had then, the base its frames' blocks
    // are written against.
    const link = this.#arcade ? readLinkTable(bytes, 4, this.core.linkBlockSize) : undefined;
    // The stream's inputs from this frame on may arrive while the state inflates: kept from
    // now, run once the machine is at the state.
    const watch = (this.#watch = { frames: [], end: frame, waiting: true, stream, blocks: link?.blocks.map((block) => block.slice()) });
    this.paused = true;
    // A linked board's: ours is linked on its side too, and gets what it got.
    const { side, state: packed } = unmarkLinked(bytes.subarray(link ? link.end : 4));
    const state = await unpack(packed);
    if (this.#watch !== watch) return; // a newer state came meanwhile
    // An arcade cabinet is the seat's: set before its state, which carries the core's link
    // table, so nothing goes in but what the frames say (the table here is only their base).
    if (link && seat !== undefined) this.core.setOption("seat", String(seat));
    if (this.core.hasLink) this.core.linkSet(side !== undefined, side);
    this.core.reset();
    this.core.unserialize(state);
    this.show();
    this.paused = false;
    this.#alarm.now();
  }

  /** Watching an arcade cabinet: its frames (see the top), kept to run in turn. */
  #watchFrames({ frame, bytes }) {
    const watch = this.#watch;
    if (!watch?.blocks) return;
    if (frame > watch.end) {
      console.warn(`Watching: frames ${watch.end}-${frame - 1} never came`);
      return;
    }
    try {
      for (let at = 0; at < bytes.length; frame++) {
        const controls = bytes[at] | (bytes[at + 1] << 8);
        const count = bytes[at + 2];
        at += 3;
        // Frames the machine has already are read past, not into the blocks.
        const keep = frame >= watch.end;
        const blocks = [];
        for (let i = 0; i < count; i++) {
          const seat = bytes[at++] ?? LINK_SEATS;
          if ((seat & 0x7f) >= LINK_SEATS) throw new Error(`seat ${seat}`);
          if (seat & 0x80) {
            blocks.push([seat & 0x7f, null]);
            continue;
          }
          const block = keep ? watch.blocks[seat] : watch.blocks[seat].slice();
          at = readChange(bytes, at, block);
          blocks.push([seat, block.slice()]);
        }
        if (keep) watch.frames.push({ controls, blocks });
      }
      watch.end = Math.max(watch.end, frame);
    } catch (error) {
      console.warn(`Watching: frames from ${frame} don't read (${error.message})`);
      this.#watch = undefined;
      this.paused = true;
    }
  }

  /**
   * Watching an arcade cabinet: a frame of it, with the blocks the player's took in before it,
   * and its controls (16 bits: see #runLinked) as an input.
   */
  #watchLinked({ controls, blocks }) {
    for (const [seat, block] of blocks) {
      if (block) this.core.linkIn(seat, block);
      else this.core.linkAbsent(seat);
    }
    const ports = new Uint32Array(PORTS);
    ports[0] = controls;
    this.#run(ports, true);
    this.core.linkOut(); // as the player's machine did after it
  }

  /**
   * An arcade game: a frame of the player's cabinet, the others' newest blocks into it first and
   * its own out after, and for watchers what it took in and the controls.
   */
  #runLinked(input) {
    const link = this.#arcade;
    // An arcade game has no gun: its controls are the input's buttons, the low 16 bits, which
    // is all its watchers get (see the top), so all the cabinet runs on.
    const controls = input & BUTTONS;
    const record = this.#stream ? [controls & 0xff, controls >> 8] : undefined;
    link.feed(record);
    // The machine is one cabinet, on the first controller.
    const ports = new Uint32Array(PORTS);
    ports[0] = controls;
    this.#run(ports, true);
    const stranded = link.ran();
    if (record) this.#stream.records.push(Uint8Array.from(record));
    if (stranded && this.seatState) this.#backToAttract();
  }

  /**
   * An arcade game: our cabinet's race is waiting for good for a player who left before it
   * began (see CabinetLink). The cabinet goes back to the seat's state, the attract mode, still
   * on the link, and watchers start over from there.
   */
  #backToAttract() {
    console.warn(`Frame ${this.#arcade.frame}: the race waits for a player who left; back to the attract mode`);
    this.core.reset();
    this.core.unserialize(this.seatState);
    this.#arcade.reloaded();
    if (this.#stream) this.#startStream();
    postMessage({ type: "arcade-cancelled" });
  }

  /** An arcade game, once a second: how its cabinet and the link are doing, to the page. */
  #arcadeStats(now) {
    if (now < this.#nextStats) return;
    const link = this.#arcade;
    const fps = Math.round((this.#frames * 1000) / (now - this.#framesSince));
    const { frame, blocksIn, blocksOut } = link;
    postMessage({ type: "arcade", fps, frame, blocksIn, blocksOut, link: this.core.linkStatus() });
    link.blocksIn = link.blocksOut = 0;
    this.#frames = 0;
    this.#framesSince = now;
    this.#nextStats = now + 1000;
  }

  #watchInputs({ frame, inputs }) {
    const watch = this.#watch;
    if (!watch) return;
    if (frame > watch.end) {
      console.warn(`Watching: frames ${watch.end}-${frame - 1} never came`);
      return;
    }
    for (let i = (watch.end - frame) * PORTS; i < inputs.length; i += PORTS) {
      watch.frames.push({ ports: inputs.subarray(i, i + PORTS) });
    }
    watch.end = Math.max(watch.end, frame + inputs.length / PORTS);
  }

  /** A linked board's frames (linked.js encodeRecords): its input on port 0, and the link bytes. */
  #watchLink(encoded) {
    const watch = this.#watch;
    if (!watch) return;
    for (const { frame, input, bytes } of decodeRecords(encoded)) {
      if (frame < watch.end) continue;
      if (frame > watch.end) {
        console.warn(`Watching: frames ${watch.end}-${frame - 1} never came`);
        return;
      }
      watch.frames.push({ ports: byPort([input], [0]), link: bytes });
      watch.end++;
    }
  }
}

/**
 * An arcade game's cabinet on its link (see the top): the other seats' blocks as they come in,
 * the newest of each into the core before a frame, this cabinet's out after one. What went into
 * the core is kept for watchers: each seat's latest block (zeros before any), whether one ever
 * came, and whether the seat is off the link.
 */
class CabinetLink {
  /** Frames this cabinet has run, the number its packets carry. */
  frame = 0;
  /** Blocks taken in from the others, and sent out, since the page last heard. */
  blocksIn = 0;
  blocksOut = 0;
  #core;
  #seat;
  #port;
  #size;
  #blocks;
  #seats = Array.from({ length: LINK_SEATS }, () => ({ taken: false, absent: false }));
  /** Per seat: who sits there (a player id, or null), and the newest frame of theirs taken. */
  #peers = Array.from({ length: LINK_SEATS }, () => ({ id: null, newest: -1 }));
  /** What goes into the core before the next frame, by seat: a block, or null (off the link). */
  #pending = new Map();
  /** Our block as it last went out, and the frame it did. */
  #sent;
  #sentAt = 0;
  /**
   * The game, for a race left waiting by a player who stood up (Daytona USA's RAM: see MODE):
   * the frame our cabinet entered a session (undefined outside one, and once racing), and
   * whether a player left it meanwhile.
   */
  #enteredAt;
  #someoneLeft = false;

  constructor(core, seat, port) {
    this.#core = core;
    this.#seat = seat;
    this.#port = port;
    this.#size = core.linkBlockSize;
    if (!this.#size) console.warn("This core has no link: the cabinet plays alone");
    this.#blocks = Array.from({ length: LINK_SEATS }, () => new Uint8Array(this.#size));
    port.onmessage = ({ data: [seat, packet] }) => this.#receive(seat, new Uint8Array(packet));
  }

  /** Who sits where now ({ seat: player id, or null }): a seat left empty goes off the link. */
  seated(seats) {
    for (let seat = 0; seat < LINK_SEATS; seat++) {
      const id = seats[seat] ?? null;
      const peer = this.#peers[seat];
      if (seat === this.#seat || peer.id === id) continue;
      peer.id = id;
      peer.newest = -1;
      if (id === null) {
        this.#pending.set(seat, null);
        this.#left();
      }
    }
  }

  /**
   * Someone left. If our cabinet is in a session with others that hasn't begun its race, the
   * leaver may have been in it, and then it waits for them for good (Daytona USA has no
   * timeout): see #watchGame. Their block is still in, so they still count among the entrants.
   */
  #left() {
    if (this.#enteredAt !== undefined && this.#core.systemRam()[ENTRANTS] >= 2) this.#someoneLeft = true;
  }

  /** Another cabinet's packet; only the newest block of each seat's counts. */
  #receive(seat, bytes) {
    if (bytes.length !== 6 + this.#size || bytes[0] !== LINK_PACKET || bytes[1] !== seat || seat === this.#seat) return;
    const peer = this.#peers[seat];
    if (!peer || peer.id === null) return; // nobody there, as far as the page said
    const frame = new DataView(bytes.buffer, bytes.byteOffset).getUint32(2, true);
    if (frame <= peer.newest) return; // a newer one came first (another way round)
    peer.newest = frame;
    this.#pending.set(seat, bytes.subarray(6));
  }

  /** Before a frame: the newest blocks into the core, and written down in `record`, if any. */
  feed(record) {
    record?.push(this.#pending.size);
    for (const [seat, block] of this.#pending) {
      const known = this.#seats[seat];
      if (block === null) {
        this.#core.linkAbsent(seat);
        known.absent = true;
        record?.push(seat | 0x80);
        continue;
      }
      this.#core.linkIn(seat, block);
      if (record) {
        record.push(seat);
        writeChange(block, this.#blocks[seat], record);
      }
      this.#blocks[seat].set(block);
      known.taken = true;
      known.absent = false;
      this.blocksIn++;
    }
    this.#pending.clear();
  }

  /**
   * After a frame: our block out to the others when it changed, or again after a while. True
   * when our cabinet is stranded in a session someone left before its race (#watchGame).
   */
  ran() {
    this.frame++;
    if (!this.#size) return false;
    const stranded = this.#watchGame();
    this.#send();
    return stranded;
  }

  /**
   * Whether our cabinet is stranded: ENTERED for longer than any session takes to start its race,
   * and someone left while it was. Never in the attract mode, idle, or racing (its mode then
   * isn't ENTERED). The entrant count can't tell: the leaver's zeros take them off it, down to
   * none when the first to press Start left.
   */
  #watchGame() {
    const ram = this.#core.systemRam();
    if (ram.length <= ENTRANTS) return false;
    if (ram[MODE] !== ENTERED || ram[CARS] !== NO_RACE) {
      this.#enteredAt = undefined;
      this.#someoneLeft = false;
      return false;
    }
    this.#enteredAt ??= this.frame;
    return this.#someoneLeft && this.frame - this.#enteredAt >= STRANDED_FRAMES;
  }

  /** The cabinet went back to its seat's state: the others' newest blocks go in again. */
  reloaded() {
    this.#seats.forEach((known, seat) => {
      if (known.taken && !known.absent && this.#peers[seat].id !== null && !this.#pending.has(seat)) {
        this.#pending.set(seat, this.#blocks[seat].slice());
      }
      known.taken = known.absent = false;
      this.#blocks[seat].fill(0);
    });
    this.#enteredAt = undefined;
    this.#someoneLeft = false;
  }

  #send() {
    const block = this.#core.linkOut();
    if (block) this.#sent = block;
    else if (!this.#sent || this.frame - this.#sentAt < RESEND_FRAMES) return;
    const packet = new Uint8Array(6 + this.#size);
    packet[0] = LINK_PACKET;
    packet[1] = this.#seat;
    new DataView(packet.buffer).setUint32(2, this.frame, true);
    packet.set(this.#sent, 6);
    this.#port.postMessage(packet.buffer, [packet.buffer]);
    this.#sentAt = this.frame;
    this.blocksOut++;
  }

  /** The link as it is in the core, for a watcher to start from (see the top). */
  table() {
    const bytes = [];
    this.#seats.forEach(({ taken, absent }, seat) => {
      bytes.push((taken ? 1 : 0) | (absent ? 2 : 0));
      if (taken) bytes.push(...this.#blocks[seat]);
    });
    return Uint8Array.from(bytes);
  }
}

/** A link written by CabinetLink.table, from `at` in `bytes`: each seat's block, and where it ends. */
function readLinkTable(bytes, at, size) {
  const blocks = [];
  for (let seat = 0; seat < LINK_SEATS; seat++) {
    const taken = Boolean(bytes[at++] & 1);
    blocks.push(taken ? bytes.slice(at, at + size) : new Uint8Array(size));
    if (taken) at += size;
  }
  return { blocks, end: at };
}

/**
 * Writes `block` to `out` (an array of bytes) as its changes from `base`, the same size: runs of
 * [unchanged: u8][changed: u8][the changed bytes], to the end of the block. A block changes by
 * a few dozen bytes a frame, so a frame's blocks for a watcher come to little.
 */
function writeChange(block, base, out) {
  for (let at = 0; at < block.length; ) {
    let same = 0;
    while (at < block.length && block[at] === base[at] && same < 255) {
      at++;
      same++;
    }
    const from = at;
    while (at < block.length && block[at] !== base[at] && at - from < 255) at++;
    out.push(same, at - from);
    for (let i = from; i < at; i++) out.push(block[i]);
  }
}

/** Reads what writeChange wrote, from `at` in `bytes`, onto `block` (the base); returns where it ends. */
function readChange(bytes, at, block) {
  for (let pos = 0; pos < block.length; ) {
    if (at + 2 > bytes.length) throw new Error("a block is cut short");
    pos += bytes[at++];
    const count = bytes[at++];
    if (pos + count > block.length || at + count > bytes.length) throw new Error("a block runs over");
    block.set(bytes.subarray(at, at + count), pos);
    at += count;
    pos += count;
  }
  return at;
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
