# Daytona save states: findings

`daytona/patches/0001-snapshot.patch` (against daytona-arcade-recomp `1877da9`) adds
`rt::save_state`, `rt::load_state`, `rt::state_size_bound` and `rt::main_ram`
(`src/runtime/snapshot.h`). What the shim needs to know, what was measured, and what was not.

## Result

A state taken between frames and loaded into a fresh `GameLoop` of the same ROM set runs on
identically: checked on the real game (the `daytona` Revision A set) at 22 points through
attract, the select screens and a race, each held for 300 frames, frame by frame, against a
straight run that never saved: the composed screen, the 3D layer, the 1 MB work RAM, the sound
board's audio samples (YM3438 and both MultiPCMs) and the i960, TGP and 68000 instruction
counts. 300 frames after each point the whole state is the same bytes as the saving machine's.
A machine that saved every 50 frames ran the same as one that never saved.

The `daytona93` set was not tested: no ROM for it here. The code has nothing set-specific.

## Use

- **Save** after `run_frame` returns (or once a deferred sound frame has completed: `save_state`
  throws `rt::Fatal` while one is pending). It also throws if a callback or sound event without
  a snapshot tag is pending; only test tools add those.
- **Load** onto a `GameLoop` built from the same images. A fresh one is the normal case, but a
  running one works too (desync recovery): everything in the state is overwritten, and what the
  screen caches from RAM (decoded tiles) is rebuilt at the next frame. `load_state` returns false
  and leaves the machine untouched for anything that is not an intact state of this build, this
  ROM set and this configuration.
- **Link play:** call `board().set_link(transport)` before `load_state`; the comm board's
  presence must match the state's (false otherwise). Bytes the transport holds are not in the
  state (see risks).
- **Sound:** a machine built with `sound_enabled = false` (a frontend with its own audio engine)
  takes a state with a sound board: those sections are skipped, and the game is the same (the
  i960 never reads the sound board). The reverse loads too; that machine's sound board keeps
  its own state. Rendered audio not yet taken (`take_fm`/`take_pcm`) is dropped on load.
- **Desync hash:** `main_ram()` is the i960's work RAM, 1 MB at 0x00500000, where the game keeps
  its variables (the frame counter at 0x00500000, the draw list at 0x005016c0). It is not the
  whole machine: hashing the state itself costs about 1 ms (`save_state`) if more is wanted.
- **Buffers:** `state_size_bound()` (12.2 MB) bounds any state the machine reaches while its
  queues stay within generous limits (8,192 kept polygons, where upstream measured 2,182 in
  the busiest frames and these runs saw up to 2,024; 64K words per TGP FIFO). A zero-padded
  buffer of that size loads (bytes after the state must be zero).
- **Global:** the draw-distance enhancement is a process-wide static (`rt::Enhance`); a state
  carries it and a load sets it, because it rewrites the game's own RAM.

## Format

64-byte header: magic `M2SNAPST`, format version 1, header size, body size, a 64-bit hash of
the body (four-lane multiply-rotate, a corruption check, not cryptographic), the ROM set name,
and fingerprints (the same hash) of the i960 and sound program images. Then sections, each a
four-character tag, a version and a size: `LOOP CPU LOCK ENH BRD RAM IO TGPB TGP GEO VID COMM SND
YM PCM1 PCM2 END`. A load checks header, hash, ROM set and fingerprints, then walks every
section once without writing (tags, versions, sizes, every count against its section, enum and
bool ranges, the indices that address arrays), and only then applies. All little-endian; floats
and doubles as their bits (memcpy). One description of each part's fields serves both
directions (a `Writer` and a `Reader`), so save and load cannot drift apart.

## What is in it

| Part | Saved | Left out (and why) |
| --- | --- | --- |
| i960 (`Cpu`) | r0-r31, the 4-frame register cache and its frame addresses and position, SAT, PRCB, PC, AC, IP, PIP, ICR, fp0-fp3 (bits), immediate-interrupt state, IRQ line states | TC (not modelled); the `on_take` hook (set by Lockstep) |
| `Lockstep` | instruction count, end count, next-event count, poke flag, interrupts taken, pending callbacks by tag (frame probe, UART shift) | the MAME-log mode (refused) |
| `GameLoop` | frames, vblank phase, frame and vblank start counts, latched inputs | profiler; display settings |
| `M2Board` | interrupt request/enable, the four line states, timers, video control, z clip, render mode, comm cn/fg registers, frame number, UART (TxRDY, shifter, hold), bytes for the sound board not yet taken, their total | ROM images, page table, `frame_skip_` (display setting), `tex_generation_` (bumped on load so a hardware renderer uploads texture RAM again) |
| RAM | main 128 KB, work 1 MB, CPU control, backup 16 KB, tile, character, palette, colour translation, texture RAM 0 and 1 (2 MB each), luma, frame buffers A and B, comm shared 16 KB: 7.25 MB | |
| I/O board | dual-port RAM, EEPROM, latched inputs | EEPROM-dirty flag, drive-board commands (host output) |
| TGP board | input and output FIFOs, program upload, control and geometrizer registers, booted flag, buffer RAM (128 KB) | tables and copro data (ROM) |
| TGP | every register, PC stack, data RAM, program RAM, instruction count, table-port bases, stall flag | test hook, bus/tables pointers |
| Geometrizer | all of MAME's raster and geo state (matrices, light, focus, LOD, clip planes, command buffer, texture RAM, log RAM, polygon RAMs) and the last frame's polygons (the game reads their count at 0x10400000; 30 Hz mode draws them again) | `pushed` (debug), the widescreen margin (display setting) |
| Video | the 8,192 palette pens (not a function of palette RAM alone: a colour-translation write recomputes only the first 4,096 at the next update), palette-dirty flag, CRTC and render offsets, whether the 3D layer is kept, the 3D layer's visible 496x384 (760 KB), HUD/coverage flags | decoded tilemaps (rebuilt from tile and character RAM on load), the composed screen (redrawn every frame), GPU snapshot copies, generation counters |
| Comm board | every register and counter, the 4 KB frame buffer, a frame received in part | the transport and what it holds |
| Sound board | 68000 registers and 64 KB RAM; the schedule by tag (bytes still on the serial line, YM timers with their generations); UART; line time; the fractional-instruction carry (double, bits); rendered-sample counts | ROMs, rendered audio not yet taken, launcher volumes |
| YM3438 | ymfm's own `save_restore`, plus the three engine counters it leaves out (active channels, modified channels, prepare count); the operators' caches are recomputed from the registers on load | |
| MultiPCM x2 | bank, slot and register selects, all 28 slots (sample, envelope, LFOs: their table pointers saved by name and row) | tables made from the clock |
| Enhancements | draw distance, the draw-list hook's budget flag | |

The generated code keeps nothing of its own: the emitters (tools/m2recomp, m2tgprecomp,
m2sndrecomp) write functions over `Cpu`/`Lockstep`, `Tgp` and `Cpu68k`/`Sched` with locals
only, and the real generated output for the `daytona` set has no `static` anywhere (checked:
26 i960 chunks, the TGP and sound files). It calls one enhancement hook (`hook_draw_list`),
whose function-local static the patch moves into `rt::Enhance`. SoftFloat's thread-local
rounding state is not on the game's path (the few FP instructions the recompiler accepts are
emitted as host float/double). The other mutable statics in the runtime are a `Video` instance
counter and a debug log file.

## Changes to upstream code

Small and behaviour-neutral (17 runtime files, +110/-22 lines, and CMakeLists.txt): callbacks in
`Lockstep` and events in the sound `Sched` carry a tag, and the lambdas they run are built by one
function per owner (`GameLoop::probe_fn`, `M2Board::uart_shift_fn`, `SoundBoard::event`) used
both when scheduling and on load; `friend class SnapshotAccess` in the classes whose private
state is saved; the `budget_raised` move. ymfm's three private counters are reached without
changing ymfm, through pointers to members named in explicit template instantiations (where the
standard does not check access); the alternative is a one-line ymfm patch adding them to its
`save_restore`, which this project does not otherwise patch.

## Findings

- **The 3D layer outlives its frame.** The rasterizer's buffer is cleared only when the next 3D
  frame is drawn, so through 2D screens (select, results) it still holds the last picture, and
  `raster_hash()` / `raster().pixels()` return it. The first version saved the layer only while
  it was shown; the real-game gate caught the difference at frame 2500 (79 frames of a 2D screen,
  3D layer only, the screen itself never differed). The layer is now always saved. In 30 Hz
  mode (video control bit 0) the layer is also shown again a frame later without a redraw; the
  bit was clear at every frame sampled in these runs of the real game, the ROM-free test sets it.
- **Revision A's factory settings wait for a network** (LINK ID MASTER, CABINET TWIN: "NETWORK
  CHECKING") and never reach attract without the comm board. Two ways through, both tested: the
  settings changed in test mode (LINK ID SINGLE, CABINET UPLIGHT; GREEN/RED are vr4/vr1, YELLOW
  vr3) and saved with `tools/common/nvram.h`, or the comm board on a cable looped back to itself:
  a ring of one cabinet, "1 of 1", the linked attract ("通信システム 1人まで対戦できます") and races.
  The ROM gate uses the loop for `M2_ROMSET=daytona` (CMakeLists), which exercises the comm board
  all the way.
- **In-flight link bytes are the host's.** During the link handshake the looped cable held up to
  7,170 bytes at a frame's end. A machine loaded at such a frame without those bytes left the
  saving machine's path on the next frame; given them, it matched for 200 frames. Once the link
  is up the game reads its frames during the frame, and the cable was empty at every frame
  sampled after that (600 to 6000). Whoever runs link play over the network has to hand over
  (or drain) what is on the wire together with the state.
- **Daytona's sound driver leaves the YM3438's FM channels silent** in these runs (active-channel
  mask 0 at every sample checked); it uses the YM's timers (always one pending at a save) and the
  MultiPCMs (up to 37 voices playing). The ROM-free test plays FM voices and checks the samples.
- **ymfm's `save_restore` changes the chip it saves**: it ends with `invalidate_caches()`,
  which makes the next sample re-prepare every channel. Saving every frame would then sum
  channels differently from a machine that never saved. The patch puts the modified mask back
  after saving; the ROM-free test checks that a machine saved every frame runs as one never saved.
- Upstream, not snapshot-related: free run appends a `std::function` to `Lockstep::calls_` every
  1,024 instructions and never frees it outside the Dreamcast build (30 to 150 a frame in these
  runs). A load empties it.

## Validation

ROM-free (`tests/test_snapshot.cpp`, `ctest -R snapshot`): a whole `GameLoop` on synthetic
images, with hand-written stand-ins for the three generated programs that call the runtime as
the generated code does. It reaches interrupts (nested and pending), the register cache spilled
to memory, the TGP FIFOs and upload, display lists turned into polygons and drawn (60 and 30 Hz),
tilemaps, palette and colour translation, the I/O board and EEPROM, sound bytes still on the
serial line at the save (17 pending), the 68000's interrupt, YM timers and voices, both
MultiPCMs with LFOs, a UART shift pending across the save, and link play (waiting with half a
frame received, and up). Output on a clean `1877da9` with the patch applied (`ctest`: 16 tests,
14 passed, the 2 Lua tests skipped as before the patch):

```
ok  : two saves of one machine are the same bytes
ok  : load into a fresh machine
ok  : a loaded machine saves to the same bytes
ok  : work RAM
ok  : frame, instruction and interrupt counts
ok  : backup RAM, EEPROM, latched inputs, sound bytes
ok  : TGP board: control, geometrizer registers, instructions, buffer RAM
ok  : video: pens, CRTC and render offsets, the 3D layer
ok  : sound board: 68000 count and RAM, bytes received
ok  : after a load: screen, 3D layer, work RAM, audio and counts equal, 90 frames
ok  : and the whole state at the end
ok  : the 3D layer changed over the run (the display list draws)
ok  : saved in 30 Hz mode after frame 86: runs on the same, to the same state
ok  : saved in 30 Hz mode after frame 87: runs on the same, to the same state
ok  : saving every frame does not change the machine (YM caches included)
ok  : load into a machine already running
ok  : and it runs on as the original
ok  : refused, machine unchanged: empty
ok  : refused, machine unchanged: truncated to 10 bytes
ok  : refused, machine unchanged: truncated to 63 bytes
ok  : refused, machine unchanged: truncated to 64 bytes
ok  : refused, machine unchanged: truncated to 100 bytes
ok  : refused, machine unchanged: truncated to 4348669 bytes
ok  : refused, machine unchanged: truncated to 8697338 bytes
ok  : refused, machine unchanged: header byte 0 changed
ok  : refused, machine unchanged: header byte 8 changed
ok  : refused, machine unchanged: header byte 12 changed
ok  : refused, machine unchanged: header byte 16 changed
ok  : refused, machine unchanged: header byte 24 changed
ok  : refused, machine unchanged: header byte 32 changed
ok  : refused, machine unchanged: header byte 48 changed
ok  : refused, machine unchanged: header byte 56 changed
ok  : refused, machine unchanged: 64 single-byte corruptions spread over the body
ok  : refused, machine unchanged: a nonzero byte after the state
ok  : the test's copy of the integrity hash matches
ok  : refused, machine unchanged: a section tag changed (rehashed)
ok  : refused, machine unchanged: a section version changed (rehashed)
ok  : refused, machine unchanged: a RAM's size changed (rehashed)
ok  : refused, machine unchanged: a bool field that is 2 (rehashed)
ok  : refused, machine unchanged: draw distance out of range (rehashed)
ok  : refused, machine unchanged: another program image
ok  : a zero-padded buffer loads
ok  : a UART shift pending across the snapshot (two callbacks: the frame probe and the shift)
ok  : and it runs on the same
ok  : save refused while a deferred sound frame is pending
ok  : a machine without a sound board takes a state with one
ok  : and runs on with the same screen, 3D layer and work RAM
ok  : a state without a sound board loads where there is one
ok  : between machines without a sound board
ok  : their untaken sound bytes are the same (665)
ok  : link: waiting for the ring, part of a frame received
ok  : link: loads where there is a link
ok  : link: and runs on the same (the token completes, the link comes up)
ok  : link: refused where there is none
ok  : link: a state without one refused where there is one
ok  : link: up after 239 frames, cabinet 1 of 1, nothing left on the line
ok  : link up: loads
ok  : link up: runs on the same, sending the same bytes (215100)
state: 8697339 bytes (bound 12215269); save 1.40 ms, load 0.79 ms (mean of 20)
ok  : the state fits its bound
test_snapshot: all passed
```

How sensitive that test is: each of 18 parts was removed from the snapshot in turn (scratch
builds, not in the patch). 16 were caught (failing checks, or a crash for the YM operator caches
and the MultiPCM LFO pointers). Two were not, and cannot be from a frame boundary: the
palette-dirty flag (always clear after a frame's update unless frame skip is on) and the tile
decode cache's invalidation (its change detection rebuilds the right tiles anyway).

Real game (`tests/test_snapshot_rom.cpp`, the `daytona` set built natively, clang, Release;
`--inputs scripts/inputs/race_basic.txt`, saves at each point, a fresh `GameLoop` loaded, 300
frames run on). Single cabinet (EEPROM set in test mode, `--nvram`):

```
reference: 5800 frames in 54.8 s
ok  : saving (116 states) left the run as the reference, 5800 frames
ok  : frame   600: 8764551 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 900: same
ok  : frame  1000: 8892925 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 1300: same
ok  : frame  1300: 8705131 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 1600: same
ok  : frame  2000: 8848505 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 2300: same
ok  : frame  2500: 8703775 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 2800: same
ok  : frame  3000: 8860742 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 3300: same
ok  : frame  3500: 8874688 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 3800: same
ok  : frame  4000: 8824500 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 4300: same
ok  : frame  4500: 8754649 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 4800: same
ok  : frame  5000: 8809947 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 5300: same
ok  : frame  5500: 8793575 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 5800: same
state size: 8793575 bytes (bound 12215269); save 1.13 ms (mean of 116), load 0.95 ms (mean of 11, into a fresh GameLoop)
test_snapshot_rom: all passed
```

Factory settings, comm board looped back (`--loop-link`):

```
reference: 5800 frames in 59.8 s
link: up, cabinet 1 of 1; most bytes on the cable at a save: 7170
ok  : saving (116 states) left the run as the reference, 5800 frames
ok  : frame   600: 8863090 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 900: same
ok  : frame  1000: 8894984 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 1300: same
ok  : frame  1300: 8709244 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 1600: same
ok  : frame  2000: 8708453 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 2300: same
ok  : frame  2600: 8767974 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 2900: same
ok  : frame  3000: 8905209 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 3300: same
ok  : frame  3500: 8814146 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 3800: same
ok  : frame  4000: 8811946 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 4300: same
ok  : frame  4500: 8888008 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 4800: same
ok  : frame  5000: 8849369 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 5300: same
ok  : frame  5500: 8821846 bytes; loaded saves the same bytes: yes; 300 frames after: 0 differ; whole state at frame 5800: same
state size: 8821846 bytes (bound 12215269); save 1.22 ms (mean of 116), load 0.95 ms (mean of 11, into a fresh GameLoop)
test_snapshot_rom: all passed
```

With the race script, frames 600 and 1000 are attract (the linked attract with the loop),
1300 just after the coins, 2000 the select screens, 2500 a 2D screen, 3000-5500 the race (lap
1, 40th). `ctest` in a `daytona` build runs the same gate with its default points (600, 1300,
2000, 3000, 4000; the loop on): passed, 86 s.

## Size and cost

| Section | Bytes | Deflated alone (zlib 6) |
| --- | ---: | ---: |
| RAM (all 14) | 7,245,880 | 860,485 |
| VID (pens and the 3D layer) | 794,660 | 79,384 |
| GEO (state 427 KB + polygons) | 624,585 | 72,627 |
| TGPB (buffer RAM, FIFOs) | 139,225 | 9,437 |
| SND (68000 RAM and the rest) | 65,773 | 3,543 |
| TGP | 20,594 | 4,055 |
| COMM, IO, YM, PCM1, PCM2, CPU, LOCK, LOOP, BRD, ENH | 14,156 | about 2,700 |
| **whole state** (race, frame 3000, link) | **8,905,209** | **1,034,436** |

Whole states measured 7.94-8.91 MB (7.94 MB on 2D-only screens before the 3D layer was always
saved; 8.70-8.91 MB now). Deflate: level 1 1.18 MB in 35 ms, level 6 1.03 MB in 103 ms, level 9
1.02 MB in 492 ms (Python's zlib on this Mac). Save 0.98-1.22 ms, load 0.86-0.95 ms into a fresh
`GameLoop` (native, Apple Silicon, Release; the load includes hashing the 8.8 MB body and the
2 MB program image, and checking every section before applying). WebAssembly was not measured
(an estimate: several times slower, a few milliseconds).

## Risks for determinism after a load

- **Link transport contents** (above): the host must carry them. With `framesync` on, the comm
  board waits on a wall clock (`kSyncTimeout`, upstream): not deterministic with or without
  snapshots.
- **Settings that change the game**: the widescreen margin widens the geometrizer's clip planes,
  so more polygons are kept, and the board answers that count to the game (0x10400000, whether
  Daytona reads it was not checked): two machines in lockstep should use the same aspect. It is a viewer setting, not in the state. Draw distance is
  in the state. Frame skip only changes which frames are drawn.
- **The screen right after a load** is redrawn at the next frame (every frame redraws it unless
  frame skip is on; then it can stay black up to that many frames).
- **A frontend's own audio engine** (`NativeSoundEngine`, fed by `take_sound_bytes`) keeps its own
  state; it is not part of `GameLoop` and not in the state. Its audio may differ after a load;
  the game does not.
- **Same build, same ROM set**: the header refuses another program image or ROM set name. Two
  builds of different seeds or compilers are not refused if their program images match; they
  run the same game code, but that was not tested.
- **Untested here**: the `daytona93` set; the Vita path (`M2_VITA_RENDER_OPT`: its tile cache
  is invalidated on load; `snapshot.cpp` compiles with it, `-fsyntax-only`, never run); the
  WebAssembly build; GCC and MSVC (built with Apple clang only); big-endian hosts (refused at
  compile time). The Dreamcast build is not covered (`snapshot.cpp` has an `#error` for it and
  is not in its source lists); the upstream files the patch changes still compile with its
  defines (`M2_DC_SPEED`, `M2_DC_MEMORY`, `M2_DC_SPIN_SKIP`; syntax check only).
