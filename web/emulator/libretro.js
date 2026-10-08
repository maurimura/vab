// Minimal libretro frontend for the Emscripten cores: FBNeo's (emulator/build.sh), Supermodel
// (supermodel/), Daytona USA's (daytona/), MAME (mame/) and Flycast (flycast/). Runs wherever the
// core runs: the Web Worker (worker.js) or Node.

import { controllerRouting, routedButton } from './controller-routing.js';
import { WHEEL_LOCK, wheelIn } from './wheel.js';

// https://github.com/libretro/RetroArch/blob/master/libretro-common/include/libretro.h
const ENV = {
  SET_ROTATION: 1,
  GET_CAN_DUPE: 3,
  GET_SYSTEM_DIRECTORY: 9,
  SET_PIXEL_FORMAT: 10,
  SET_INPUT_DESCRIPTORS: 11,
  GET_VARIABLE: 15,
  GET_VARIABLE_UPDATE: 17,
  GET_LOG_INTERFACE: 27,
  GET_SAVE_DIRECTORY: 31,
  GET_CORE_OPTIONS_VERSION: 52,
  GET_AUDIO_VIDEO_ENABLE: 47 | 0x10000,
  GET_SAVESTATE_CONTEXT: 72 | 0x10000,
};
const SAVESTATE_CONTEXT = { NORMAL: 0, ROLLBACK_NETPLAY: 3 };
const MEMORY_SYSTEM_RAM = 2;

// Core options we override; the rest keep the core's defaults. Each core only asks for its own.
const OPTIONS = {
  // The default opens the service menu when Start is held for a second, counting frames outside
  // the savestate, so a rollback could open it on one player's machine only.
  "fbneo-diagnostic-input": "None",
  // MAME (mame/), set here whatever its defaults: no thread mode, no throttling (the worker
  // paces frames), no menu or BIOS on boot, no configuration or states read or written behind
  // our back, no cheats, the usual renderer. Its lightgun reads RETRO_DEVICE_LIGHTGUN (see
  // #inputState; its default, "none", reads nothing), and no mouse.
  mame_thread_mode: "disabled",
  mame_lightgun_mode: "lightgun",
  mame_mouse_enable: "disabled",
  mame_throttle: "disabled",
  mame_boot_to_osd: "disabled",
  mame_boot_to_bios: "disabled",
  mame_read_config: "disabled",
  mame_write_config: "disabled",
  mame_auto_save: "disabled",
  mame_cheats_enable: "disabled",
  mame_alternate_renderer: "disabled",
  // Flycast (flycast/), whose keys still carry its old name, reicast. One we don't answer takes the
  // default in its code, which isn't always the one its option list shows, so these are set either
  // way. One thread, the frame drawn within retro_run, and every frame run and shown: no frame
  // skipping, nor a frame rate it changes on us (the worker paces frames at the rate the core gives
  // when the game loads), and the picture is the one the game puts on the screen, when it swaps,
  // not each one as soon as it's drawn. The NAOMI's own 640x480, transparent polygons sorted per
  // triangle, the usual way (per pixel needs more than WebGL 2 has), and nothing the board didn't
  // have (widescreen, texture upscaling or anisotropic filtering, custom textures) or written
  // behind our back (texture dumps); the SH4 at its 200 MHz. The USA BIOS, from the ROM set, so the
  // text is English (Japan's calls the game Power Smash), and NTSC, which its save states carry, so
  // every machine says the same. The disc read at the GD-ROM's own pace and the sound's DSP on, as
  // the board does. No network (the broadband adapter, UPnP, DCNet, outputs broadcast on a TCP
  // port), no service button on the pad, and free play: Start plays, as at the bar's other
  // cabinets. A NAOMI has no VMU, so none of those.
  reicast_threaded_rendering: "disabled",
  reicast_auto_skip_frame: "disabled",
  reicast_frame_skipping: "disabled",
  reicast_detect_vsync_swap_interval: "disabled",
  reicast_delay_frame_swapping: "enabled",
  reicast_internal_resolution: "640x480",
  reicast_alpha_sorting: "per-triangle (normal)",
  reicast_widescreen_hack: "disabled",
  reicast_widescreen_cheats: "disabled",
  reicast_texupscale: "1",
  reicast_anisotropic_filtering: "off",
  reicast_custom_textures: "disabled",
  reicast_dump_textures: "disabled",
  reicast_sh4clock: "200",
  reicast_region: "USA",
  reicast_broadcast: "NTSC",
  reicast_gdrom_fast_loading: "disabled",
  reicast_enable_dsp: "enabled",
  reicast_emulate_bba: "disabled",
  reicast_upnp: "disabled",
  reicast_dcnet: "disabled",
  reicast_network_output: "disabled",
  reicast_allow_service_buttons: "disabled",
  reicast_force_freeplay: "enabled",
};
// RGBA8888 is ours, not libretro's: bytes R, G, B, A as the page wants them, so a core that draws
// with WebGL (Supermodel) hands its frame over without a conversion on either side.
const PIXEL_FORMAT = { RGB1555: 0, XRGB8888: 1, RGB565: 2, RGBA8888: 100 };
const DEVICE = { JOYPAD: 1, LIGHTGUN: 4, ANALOG: 5, POINTER: 6 };
/** RetroPad ids (libretro.h) a driving game's arrows are named after (see #namePedals). */
const PAD = { UP: 4, DOWN: 5, LEFT: 6, RIGHT: 7, L2: 12, R2: 13 };
/** The left stick's X, all of it: -0x7fff full left .. 0x7fff full right. */
const STICK = 0x7fff;
/**
 * A wheel (wheel.js, -127..127) as the left stick's X, spread over `span`, [from, to] of the
 * stick's 0..0x7fff each way (world::Game's `wheel`): a core may see nothing in the first part of
 * the stick and be at full lock before its end. Out Run's FBNeo core does both, its frontend's
 * dead zone and the driver's: its wheel stays straight up to about a third of the stick and is
 * at full lock from 72% (measured), so the wheel's 1 to 127 go over that span, every step of it
 * a turn of the game's wheel. MAME reads the whole stick, one to one onto the wheel's range.
 */
const wheelAxis = (wheel, [from, to] = [0, STICK]) =>
  wheel === 0 ? 0 : Math.sign(wheel) * Math.round(from + (Math.abs(wheel) * (to - from)) / WHEEL_LOCK);
/**
 * A lightgun's buttons (libretro.h RETRO_DEVICE_ID_LIGHTGUN_*) and the RetroPad buttons that
 * press them: the trigger is B (key Z, the left mouse button), Aux A is A (key X, the right
 * mouse button, Space: Time Crisis II's pedal), and so on.
 */
const GUN_BUTTONS = new Map([
  [2, 0], // TRIGGER: B
  [3, 8], // AUX_A: A
  [4, 9], // AUX_B: X
  [16, 1], // RELOAD (a shot off the screen): Y
  [6, 3], // START
  [7, 2], // SELECT: a coin
  [9, 4], // DPAD_UP
  [10, 5], // DPAD_DOWN
  [11, 6], // DPAD_LEFT
  [12, 7], // DPAD_RIGHT
]);
const GUN_X = 13; // RETRO_DEVICE_ID_LIGHTGUN_SCREEN_X: -0x8000 the left edge .. 0x7fff the right
const GUN_Y = 14; // RETRO_DEVICE_ID_LIGHTGUN_SCREEN_Y: -0x8000 the top .. 0x7fff the bottom
const POINTER = { X: 0, Y: 1, PRESSED: 2, COUNT: 3 };
/** Room for a tick's link bytes in the core's memory (a tick carries ~1.4 KB at most). */
const LINK_BUFFER = 64 * 1024;
/** mame_link_status's words, in order (mame/README.md, "API"). */
const LINK_STATUS = ["linked", "side", "txFrames", "rxFrames", "txBytes", "rxBytes", "mode", "keepalive", "counter", "pending", "patches", "dsw"];
/** The 8-bit aim in a port's input, 0..255 across the screen, as libretro's -0x8000..0x7fff. */
const gunAxis = (aim) => aim * 257 - 0x8000;
/** ...and as a pointer's -0x7fff..0x7fff (-0x8000 means no pointer). */
const pointerAxis = (aim) => Math.round((aim * 0xfffe) / 255) - 0x7fff;

export class Core {
  /**
   * Each controller port's input: the RetroPad mask in the low 16 bits, bit (1 << id) per held
   * button (ids from libretro.h), and for a lightgun game where it aims in the high 16: x in
   * bits 16-23 (0 the left edge of the screen, 255 the right), y in bits 24-31 (0 the top). At
   * a driving game (steers) bits 16-23 are its wheel instead, a signed byte (wheel.js).
   */
  inputs = new Uint32Array(4);
  /**
   * A driving game's wheel, world::Game's `wheel` (assets/games.ron), or undefined: every port
   * then steers (steers), and the wheel's `span` says where on the stick the core turns the
   * game's wheel (wheelAxis). Set it before the game loads, as the other settings here.
   */
  wheel;
  /**
   * A lightgun game (Time Crisis II): the core's lightgun (and pointer) reads are answered from
   * each port's aim and buttons. Otherwise they read nothing: MAME also maps a lightgun's
   * buttons onto a game's Button 1-4, which would press a second button in Tekken 3.
   */
  gun = false;
  /** Sequential seats share the physical upright's gameplay controls. */
  turns = false;
  /**
   * Rollback netplay: FBNeo then keeps its states free of anything machine-local
   * (hiscores, the host clock) so both players' machines stay identical.
   */
  netplay = false;
  /**
   * False while re-running frames after a rollback: FBNeo skips drawing and keeps the sound to
   * itself (it still emulates it, so the machine stays exact).
   */
  present = true;

  /**
   * What the game calls player 1's buttons, from the core: RetroPad id -> name, e.g. 0 -> "Fire"
   * (B). Known once the game is loaded. A lightgun game's names for its gun's buttons win,
   * under the RetroPad buttons that press them (GUN_BUTTONS).
   */
  get buttons() {
    if (this.gun) return new Map([...this.#padNames, ...this.#gunNames]);
    // A driving game's Left and Right turn its wheel, whatever the core calls them (FBNeo's
    // "Steering (Fake Digital Left)"), so the help card lists them as Daytona USA's are.
    if (this.wheel) return new Map([...this.#padNames, [PAD.LEFT, "Steer left"], [PAD.RIGHT, "Steer right"]]);
    return this.#padNames;
  }

  /**
   * Whether the game on `port` is steered with a wheel, which the arrows turn (wheel.js) and the
   * port's input carries in bits 16-23: a driving game's, one with a `wheel`.
   */
  steers(port) {
    return Boolean(this.wheel) && port >= 0 && port < this.inputs.length;
  }

  #m;
  /** The core's own settings function, `_<core>_set(key, value)` (C strings), if it has one. */
  #set;
  /**
   * The core's link exports, `_<core>_link_*`, if it has a link the frontend carries (Daytona
   * USA's star: see linkOut), with a block-sized buffer in its memory for them.
   */
  #link;
  #slots = [];
  #slotSize = 0;
  #pixelFormat = PIXEL_FORMAT.RGB1555;
  #rotation = 0; // quarter turns counter-clockwise, for vertical games like Pac-Man
  #routing = { sharedCoin: false, secondStart: undefined, pedals: [] };
  /** Player 1's button names from the core's descriptors, for the RetroPad and for a lightgun. */
  #padNames = new Map();
  #gunNames = new Map();
  #onFrame;
  #onAudio;
  #onLog;
  #strings = new Map();

  /**
   * @param createModule the core's default export (createFBNeo from fbneo.mjs)
   * @param callbacks onFrame(rgba, width, height), onAudio(Int16Array stereo), onLog(level, text)
   * @param moduleOptions Emscripten Module settings the core needs, e.g. { canvas } for a core
   *   that draws with WebGL (Supermodel)
   */
  static async create(createModule, callbacks, moduleOptions = {}) {
    return new Core(await createModule(moduleOptions), callbacks);
  }

  constructor(module, { onFrame, onAudio, onLog = () => {} }) {
    const m = (this.#m = module);
    // Supermodel's _supermodel_set, Daytona's _daytona_set; FBNeo has none.
    this.#set = Object.keys(m).find((name) => /^_[a-z0-9]+_set$/.test(name) && typeof m[name] === "function");
    const linkOut = Object.keys(m).find((name) => /^_[a-z0-9]+_link_out$/.test(name) && typeof m[name] === "function");
    if (linkOut) {
      const prefix = linkOut.slice(0, -"out".length);
      const size = m[`${prefix}block_size`]?.() ?? 0;
      const [linkIn, absent, status] = ["in", "absent", "status"].map((name) => m[`${prefix}${name}`]);
      if (size > 0 && linkIn && absent) this.#link = { out: m[linkOut], in: linkIn, absent, status, size, buffer: m._malloc(size) };
    }
    this.#onFrame = onFrame;
    this.#onAudio = onAudio;
    this.#onLog = onLog;
    m.FS.mkdirTree("/system");
    m.FS.mkdirTree("/save");
    m.FS.mkdirTree("/roms");

    m._retro_set_environment(m.addFunction((cmd, data) => this.#environment(cmd, data), "iii"));
    m._retro_set_video_refresh(m.addFunction((...args) => this.#video(...args), "viiii"));
    m._retro_set_audio_sample(m.addFunction((l, r) => this.#onAudio(Int16Array.of(l, r)), "vii"));
    m._retro_set_audio_sample_batch(m.addFunction((data, frames) => this.#audio(data, frames), "iii"));
    m._retro_set_input_poll(m.addFunction(() => {}, "v"));
    m._retro_set_input_state(m.addFunction((port, device, index, id) => this.#inputState(port, device, index, id), "iiiii"));
    m._retro_init();
  }

  /**
   * Sets one of the core's own settings through the `_<core>_set` its module exports, e.g.
   * Daytona USA's "cabinets" (before the game loads) or "view" (any time). False when the core
   * has no settings (FBNeo); what a core does with a key it doesn't know is up to it.
   */
  setOption(key, value) {
    if (!this.#set) return false;
    this.#m[this.#set](this.#cString(key), this.#cString(value));
    return true;
  }

  /**
   * A link the frontend carries between machines (Daytona USA's cabinets on a star, each in its
   * own browser): the size of a cabinet's block of link data in bytes, 0 when the core has no
   * such link. Each frame, right after run(), linkOut() gives this cabinet's block when it
   * changed, for every other cabinet's linkIn(); see web/emulator/worker.js.
   */
  get linkBlockSize() {
    return this.#link?.size ?? 0;
  }

  /** This cabinet's block (a copy) if it changed since the last call, else undefined. */
  linkOut() {
    const link = this.#link;
    if (!link?.out(link.buffer)) return undefined;
    return this.#m.HEAPU8.slice(link.buffer, link.buffer + link.size);
  }

  /** The latest block from the cabinet at `seat`, for this one to see from the next run(). */
  linkIn(seat, block) {
    const link = this.#link;
    if (!link || block.length !== link.size) return;
    this.#m.HEAPU8.set(block, link.buffer);
    link.in(seat, link.buffer, link.size);
  }

  /** Nobody is at `seat` (anymore): the cabinet there is off the link. */
  linkAbsent(seat) {
    this.#link?.absent(seat);
  }

  /**
   * What the core says about its link, or undefined without one: Daytona USA's star, the seats
   * on it (its JSON); Time Crisis II's serial link (hasLink), as #serialLinkStatus says.
   */
  linkStatus() {
    if (this.hasLink) return this.#serialLinkStatus();
    const status = this.#link?.status;
    if (!status) return undefined;
    try {
      return JSON.parse(this.#m.UTF8ToString(status()));
    } catch {
      return undefined;
    }
  }

  /**
   * Puts a file under /roms, where the core finds what goes with a ROM set: a BIOS set next to
   * it (neogeo.zip), or a file in a folder of its own, made here if need be (a NAOMI game's disc,
   * vtennisg/gds-0011.chd, in the folder Flycast looks in, named after the ROM set).
   */
  addFile(path, bytes) {
    const at = `/roms/${path}`;
    this.#m.FS.mkdirTree(at.slice(0, at.lastIndexOf("/")));
    this.#m.FS.writeFile(at, bytes);
  }

  /** Loads a ROM set (FBNeo picks the game from the file name, e.g. mk2.zip). */
  loadGame(fileName, bytes) {
    const m = this.#m;
    const path = `/roms/${fileName}`;
    this.addFile(fileName, bytes);
    // struct retro_game_info { const char *path; const void *data; size_t size; const char *meta; }
    const info = m._malloc(16);
    m.HEAPU32.fill(0, info >> 2, (info >> 2) + 4);
    m.setValue(info, this.#cString(path), "i32");
    const loaded = m._retro_load_game(info);
    m._free(info);
    if (!loaded) throw new Error(`The core could not load ${fileName}`);
    // A RetroPad on each port, a lightgun game's too: MAME reads its lightgun whatever the
    // port's device (its retro_set_controller_port_device does nothing).
    for (let port = 0; port < this.inputs.length; port++) {
      m._retro_set_controller_port_device(port, DEVICE.JOYPAD);
    }

    // struct retro_system_av_info { geometry { u32 w, h, max_w, max_h; f32 aspect } timing { f64 fps, sample_rate } }
    const av = m._malloc(40);
    m._retro_get_system_av_info(av);
    const result = {
      width: m.getValue(av, "i32"),
      height: m.getValue(av + 4, "i32"),
      aspectRatio: m.getValue(av + 16, "float"),
      fps: m.getValue(av + 24, "double"),
      sampleRate: m.getValue(av + 32, "double"),
    };
    m._free(av);
    return result;
  }

  /** Runs one emulated frame. */
  run() {
    this.#m._retro_run();
  }

  /**
   * Resets the machine, as powering it on. Before loading a state that another machine made:
   * whatever a core leaves out of its states (Supermodel leaves out a few timers and device
   * registers) is then at power-on on both, instead of each machine's own.
   */
  reset() {
    this.#m._retro_reset();
  }

  /** Snapshot of the whole machine. Only loads into the same core build and game. */
  serialize() {
    const m = this.#m;
    const size = m._retro_serialize_size();
    const ptr = m._malloc(size);
    const ok = m._retro_serialize(ptr, size);
    const state = m.HEAPU8.slice(ptr, ptr + size);
    m._free(ptr);
    if (!ok) throw new Error("The core could not save its state");
    return state;
  }

  unserialize(state) {
    const m = this.#m;
    const ptr = m._malloc(state.length);
    m.HEAPU8.set(state, ptr);
    const ok = m._retro_unserialize(ptr, state.length);
    m._free(ptr);
    if (!ok) throw new Error("The core could not load the state");
  }

  /**
   * Allocates `count` save-state slots inside the core's memory. Rollback saves every frame,
   * so saveSlot / loadSlot copy within wasm memory and never into JavaScript.
   */
  allocSlots(count) {
    const m = this.#m;
    this.#slots.forEach((ptr) => m._free(ptr));
    this.#slotSize = m._retro_serialize_size();
    this.#slots = Array.from({ length: count }, () => m._malloc(this.#slotSize));
  }

  saveSlot(slot) {
    if (!this.#m._retro_serialize(this.#slots[slot], this.#slotSize)) {
      throw new Error("The core could not save its state");
    }
  }

  loadSlot(slot) {
    if (!this.#m._retro_unserialize(this.#slots[slot], this.#slotSize)) {
      throw new Error("The core could not load the state");
    }
  }

  /**
   * The game's main RAM (what RetroAchievements reads). Both players' copies match while in
   * sync, unlike whole savestates, which also hold sound and drawing caches.
   * A view valid until the next allocation in the core.
   */
  systemRam() {
    const m = this.#m;
    const ptr = m._retro_get_memory_data(MEMORY_SYSTEM_RAM);
    return m.HEAPU8.subarray(ptr, ptr + m._retro_get_memory_size(MEMORY_SYSTEM_RAM));
  }

  /** A view of a slot's bytes, valid until the next allocation in the core. */
  slotBytes(slot) {
    const ptr = this.#slots[slot];
    return this.#m.HEAPU8.subarray(ptr, ptr + this.#slotSize);
  }

  /**
   * Whether the core emulates Time Crisis II's link between two boards (MAME's
   * `mame_link_*`, mame/README.md "Linked cabinets"); the link calls below need it.
   */
  get hasLink() {
    return typeof this.#m._mame_link_set === "function";
  }

  /**
   * Plugs the link cable in (on this board's `side`, 0 Left/Red or 1 Right/Blue) or out. Before
   * `loadGame`, or between two frames; not part of states, so before loading one made linked.
   */
  linkSet(enabled, side = 0) {
    this.#m._mame_link_set(enabled ? 1 : 0, side);
  }

  /** What the board transmitted on the link since the last call (opaque bytes for the other board). */
  linkOutgoing() {
    const m = this.#m;
    let size = m._mame_link_outgoing(this.#linkBuffer(), this.#linkSize);
    if (size > this.#linkSize) {
      this.#linkBuffer(size); // more than the buffer: it took nothing, so again with room for it
      size = m._mame_link_outgoing(this.#linkBuffer(), this.#linkSize);
    }
    return m.HEAPU8.slice(this.#linkPtr, this.#linkPtr + size);
  }

  /** Hands the board what the other board transmitted (`linkOutgoing`'s bytes), between frames. */
  linkIncoming(bytes) {
    if (!bytes.length) return;
    const ptr = this.#linkBuffer(bytes.length);
    this.#m.HEAPU8.set(bytes, ptr);
    this.#m._mame_link_incoming(ptr, bytes.length);
  }

  /**
   * The link as the game and the core see it: whether it's plugged in, the side, frames and
   * bytes each way, the game's link mode word (2: linked gameplay), its keepalive word, its
   * link state machine's counter, bytes waiting to be taken, the code patches and the DIPs.
   */
  #serialLinkStatus() {
    const m = this.#m;
    const ptr = this.#linkBuffer();
    const count = m._mame_link_status(ptr, LINK_STATUS.length);
    const words = new Uint32Array(m.HEAPU8.buffer, ptr, count);
    return Object.fromEntries(LINK_STATUS.slice(0, count).map((key, i) => [key, words[i]]));
  }

  /** The staging area link bytes go through in the core's memory, at least `size` bytes. */
  #linkPtr = 0;
  #linkSize = 0;
  #linkBuffer(size = 0) {
    if (this.#linkSize < Math.max(size, LINK_BUFFER)) {
      if (this.#linkPtr) this.#m._free(this.#linkPtr);
      this.#linkSize = Math.max(size, LINK_BUFFER);
      this.#linkPtr = this.#m._malloc(this.#linkSize);
    }
    return this.#linkPtr;
  }

  /**
   * An FBNeo driving game's Up and Down, by what they do there (controller-routing.js): they take
   * the pedals' names (Out Run's "Accelerate" and "Brake"), so the help card lists them so.
   */
  #namePedals() {
    const names = this.#padNames;
    if (this.#routing.pedals[0]) {
      if (names.has(PAD.R2)) names.set(PAD.UP, names.get(PAD.R2));
      if (names.has(PAD.L2)) names.set(PAD.DOWN, names.get(PAD.L2));
    }
  }

  #environment(cmd, data) {
    const m = this.#m;
    switch (cmd) {
      case ENV.SET_ROTATION:
        this.#rotation = m.getValue(data, "i32") & 3;
        return 1;
      case ENV.GET_CAN_DUPE:
        m.setValue(data, 1, "i8");
        return 1;
      case ENV.GET_SYSTEM_DIRECTORY:
        m.setValue(data, this.#cString("/system"), "i32");
        return 1;
      case ENV.GET_SAVE_DIRECTORY:
        m.setValue(data, this.#cString("/save"), "i32");
        return 1;
      case ENV.SET_INPUT_DESCRIPTORS: {
        // struct retro_input_descriptor { unsigned port, device, index, id; const char *description; }[],
        // ended by one without a description.
        this.#padNames = new Map();
        this.#gunNames = new Map();
        const descriptors = Array.from({ length: this.inputs.length }, () => new Map());
        for (let at = data; ; at += 20) {
          const description = m.getValue(at + 16, "i32");
          if (!description) break;
          const [port, device, index, id] = [0, 4, 8, 12].map((offset) => m.getValue(at + offset, "i32"));
          if (device === DEVICE.JOYPAD && index === 0 && descriptors[port]) {
            const label = m.UTF8ToString(description);
            descriptors[port].set(id, label);
            if (port === 0) this.#padNames.set(id, label);
          } else if (device === DEVICE.LIGHTGUN && port === 0 && GUN_BUTTONS.has(id)) {
            this.#gunNames.set(GUN_BUTTONS.get(id), m.UTF8ToString(description));
          }
        }
        this.#routing = controllerRouting(descriptors);
        this.#namePedals();
        return 1;
      }
      case ENV.SET_PIXEL_FORMAT: {
        const format = m.getValue(data, "i32");
        if (!Object.values(PIXEL_FORMAT).includes(format)) return 0;
        this.#pixelFormat = format;
        return 1;
      }
      case ENV.GET_LOG_INTERFACE:
        m.setValue(data, m.addFunction((level, fmt, args) =>
          this.#onLog(level, this.#format(fmt, args)), "viii"), "i32");
        return 1;
      case ENV.GET_CORE_OPTIONS_VERSION:
        // Version 0: the core sets plain variables and asks for each (GET_VARIABLE below).
        m.setValue(data, 0, "i32");
        return 1;
      case ENV.GET_VARIABLE: {
        // struct retro_variable { const char *key; const char *value; }
        const value = OPTIONS[m.UTF8ToString(m.getValue(data, "i32"))];
        if (value === undefined) return 0;
        m.setValue(data + 4, this.#cString(value), "i32");
        return 1;
      }
      case ENV.GET_VARIABLE_UPDATE:
        m.setValue(data, 0, "i8");
        return 1;
      case ENV.GET_AUDIO_VIDEO_ENABLE:
        m.setValue(data, this.present ? 1 | 2 : 0, "i32"); // bit 0 video, bit 1 audio
        return 1;
      case ENV.GET_SAVESTATE_CONTEXT:
        // The core first asks with no pointer, only to learn that we answer.
        if (data) {
          const context = this.netplay ? SAVESTATE_CONTEXT.ROLLBACK_NETPLAY : SAVESTATE_CONTEXT.NORMAL;
          m.setValue(data, context, "i32");
        }
        return 1;
      default:
        return 0;
    }
  }

  /**
   * The core asks about a control (retro_input_state). A RetroPad's buttons come from the low
   * 16 bits of the port's input (controller-routing.js shares an upright's panel). A lightgun
   * game's gun (MAME asks every frame, for every port) is where the port aims, from the high 16
   * bits, with GUN_BUTTONS for its buttons; as a pointer (MAME's "touchscreen" lightgun mode)
   * the trigger is a press, the pedal a second finger. A driving game's wheel (steers) is the
   * left stick's X, from bits 16-23 (wheel.js), and its arrows are only that wheel: never the
   * core's own Left and Right, FBNeo's "fake digital" full lock at once or MAME's key ramp.
   */
  #inputState(port, device, index, id) {
    if (device === DEVICE.JOYPAD) {
      if (this.steers(port) && (id === PAD.LEFT || id === PAD.RIGHT)) return 0;
      return id < 16 ? routedButton(this.inputs, port, id, this.#routing, this.turns) : 0;
    }
    if (device === DEVICE.ANALOG) {
      return index === 0 && id === 0 && this.steers(port) ? wheelAxis(wheelIn(this.inputs[port]), this.wheel.span) : 0;
    }
    if (!this.gun || port >= this.inputs.length) return 0;
    const input = this.inputs[port];
    const held = (bit) => (input >>> bit) & 1;
    const x = (input >>> 16) & 0xff;
    const y = input >>> 24;
    if (device === DEVICE.LIGHTGUN) {
      if (id === GUN_X) return gunAxis(x);
      if (id === GUN_Y) return gunAxis(y);
      return GUN_BUTTONS.has(id) ? held(GUN_BUTTONS.get(id)) : 0; // IS_OFFSCREEN, AUX_C: 0
    }
    if (device === DEVICE.POINTER) {
      if (id === POINTER.X) return pointerAxis(x);
      if (id === POINTER.Y) return pointerAxis(y);
      if (id === POINTER.PRESSED) return held(0);
      // MAME presses its gun's button (count - 1): 1 the trigger, 2 Aux A, the pedal.
      if (id === POINTER.COUNT) return held(8) ? 2 : held(0);
    }
    return 0;
  }

  #video(data, width, height, pitch) {
    if (!data) return; // duplicated frame
    const heap = this.#m.HEAPU8;
    const rgba = new Uint8ClampedArray(width * height * 4);
    const out = new Uint32Array(rgba.buffer);
    for (let y = 0; y < height; y++) {
      const row = data + y * pitch;
      const o = y * width;
      if (this.#pixelFormat === PIXEL_FORMAT.RGBA8888) {
        rgba.set(heap.subarray(row, row + width * 4), o * 4);
      } else if (this.#pixelFormat === PIXEL_FORMAT.XRGB8888) {
        const src = new Uint32Array(heap.buffer, row, width);
        for (let x = 0; x < width; x++) {
          const v = src[x];
          out[o + x] = 0xff000000 | ((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff);
        }
      } else {
        const src = new Uint16Array(heap.buffer, row, width);
        const is565 = this.#pixelFormat === PIXEL_FORMAT.RGB565;
        for (let x = 0; x < width; x++) {
          const v = src[x];
          const r = (v >> (is565 ? 11 : 10)) & 0x1f;
          const g = is565 ? (v >> 5) & 0x3f : (v >> 5) & 0x1f;
          const b = v & 0x1f;
          const g8 = is565 ? (g << 2) | (g >> 4) : (g << 3) | (g >> 2);
          out[o + x] = 0xff000000 | (((b << 3) | (b >> 2)) << 16) | (g8 << 8) | ((r << 3) | (r >> 2));
        }
      }
    }
    if (this.#rotation === 0) {
      this.#onFrame(rgba, width, height);
    } else {
      const turned = rotate(out, width, height, this.#rotation);
      const sideways = this.#rotation % 2 === 1;
      this.#onFrame(new Uint8ClampedArray(turned.buffer), sideways ? height : width, sideways ? width : height);
    }
  }

  #audio(data, frames) {
    this.#onAudio(this.#m.HEAP16.slice(data >> 1, (data >> 1) + frames * 2));
    return frames;
  }

  // Strings handed to the core must stay alive, so each one is allocated once.
  #cString(text) {
    let ptr = this.#strings.get(text);
    if (!ptr) {
      const size = this.#m.lengthBytesUTF8(text) + 1;
      ptr = this.#m._malloc(size);
      this.#m.stringToUTF8(text, ptr, size);
      this.#strings.set(text, ptr);
    }
    return ptr;
  }

  // Enough printf for the core's log lines (%s %d %i %u %x %c %f); wasm32 passes varargs
  // as a pointer to a buffer with each argument aligned to its size.
  #format(fmtPtr, argsPtr) {
    const m = this.#m;
    let offset = argsPtr;
    const next = (size) => {
      offset = Math.ceil(offset / size) * size;
      const at = offset;
      offset += size;
      return at;
    };
    return m.UTF8ToString(fmtPtr).replace(/%[-+ #0-9.]*(l{0,2}|z|h{0,2})([sdiuxXcf%])/g, (_, len, type) => {
      if (type === "%") return "%";
      if (type === "f") return m.getValue(next(8), "double").toFixed(2);
      if (len === "ll") return String(m.getValue(next(8), "i64"));
      const value = m.getValue(next(4), "i32");
      if (type === "s") return m.UTF8ToString(value);
      if (type === "c") return String.fromCharCode(value);
      if (type === "u") return String(value >>> 0);
      if (type === "x" || type === "X") return (value >>> 0).toString(16);
      return String(value);
    }).trimEnd();
  }
}

/**
 * Where a file the site serves goes in a core's file system, under /roms (Core.addFile): at its
 * path under /roms/, folders and all, so a NAOMI game's disc (/roms/vtennisg/gds-0011.chd) lands
 * in the folder named after its ROM set, where Flycast looks, and a BIOS set (/roms/neogeo.zip)
 * next to the ROM set. A URL without /roms/ in it (a check's file:// URL) goes by its name.
 */
export function romPath(url) {
  const at = url.lastIndexOf("/roms/");
  return at < 0 ? url.split("/").pop() : url.slice(at + "/roms/".length);
}

/** Turns a width x height image `quarterTurns` times counter-clockwise. */
function rotate(pixels, width, height, quarterTurns) {
  const out = new Uint32Array(pixels.length);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const pixel = pixels[y * width + x];
      if (quarterTurns === 1) out[(width - 1 - x) * height + y] = pixel;
      else if (quarterTurns === 2) out[(height - 1 - y) * width + (width - 1 - x)] = pixel;
      else out[x * height + (height - 1 - y)] = pixel;
    }
  }
  return out;
}
