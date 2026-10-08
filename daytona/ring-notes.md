# Daytona USA's own link as an arcade mode: measurements

2026-10-07, M1 Max, Node 24, the headless build (`./daytona/build.sh headless`) with MAME's
`daytona` set. The question: can the game's own link play be the basis of an "arcade mode" for
the bar: up to 8 cabinets always linked, each player's browser running only their own cabinet,
players sitting down and leaving at any time, joining a race at the circuit select as the game
allows. Everything here ran in one Node process per experiment, all cabinets of the ring in one
machine (the shim's `link_*` options model the network between them).

Conventions: **cabinet k** is RetroPad port k, link id k + 1 and CAR NUMBER k + 1 (presets
`8/k`: master car 1, slaves cars 2-8). A **frame** is one `retro_run` (57.524 Hz, 17.38 ms);
frame numbers count from power-on. Screens are 496x384 PNGs; the contact sheets named below are
in `$SCRATCH/notes/` with `SCRATCH=/private/tmp/claude-501/-Users-m-vab-wt-sega-model-to-daytona-use/8a32121d-3e7e-434b-bc23-71abcc2fc332/scratchpad/ring`
(each image is labelled `<tag>-c<cabinet>-f<frame>`), the single screens beside the commands'
outputs in `$SCRATCH/m1..m6`.

## What was found, in short

1. **The game is lockstepped on link data.** Every cabinet's game advances only as data frames
   arrive from the ring; a cabinet that stops receiving them freezes (no error screen, no reboot,
   for 100 s and more) and resumes exactly where it was when data flows again (a 20 s pause in a
   race: the race goes on). A comm board only declares the link lost when its transport closes
   or refuses a write, and a lost board never recovers (the game does not restart its link);
   that is the transport's choice, not the game's.
2. **A cabinet can be swapped in from a saved state while the ring runs.** Cabinet 6 loaded
   from a state 9,000 frames old, or from a state another process made (a "preset state"), takes
   its place at once: all 8 stay up, nothing freezes, it starts a race with another cabinet and
   they race (POSITION n/2), also while another pair is mid-race. One cabinet's state is 8.7 MB
   (0.95 MB deflated), saved in 5-21 ms and loaded in 1.2 ms in Node.
3. **Join rule:** the first START opens a 15-count entry window (about 16.3 s) shown on every
   idle cabinet ("WAITING FOR YOUR ENTRY / PUSH START BUTTON / n SEC"); a START within 940 frames
   joins that race, one at 960 is ignored, one at 1000 or later opens a separate session. Several
   sessions run side by side on one ring.
4. **Latency:** once the link is formed, 30 frames a hop (521 ms; 3.6 s round the 8-ring) still
   lets a pair join and race; the boot-time NETWORK CHECKING is the strict part (8-ring: 5 frames
   a hop passes, 6 fails; star: 42 passes, 60 fails). **Jitter** (irregular arrival) slows the
   whole game down (an entry countdown ran at ~40% speed with 3+0..3 frames a hop); handing each
   board at most one data frame a frame (`link_pace`) or the star removes the slowdown.
5. **Leaving:** a cabinet that stops (powered off, or paused with a transport that refuses
   writes) freezes everyone for good. Kept as a ghost (its board relaying with its block frozen)
   the ring and races go on; its car stays where it was. With a transport that drops instead of
   refusing (`link_full=drop`), a pause only freezes the cabinets it starves until it resumes, and a fresh boot
   rejoins the ring if the transport hands it its number (`link_assist`): measured, it then races.
6. **Protocol:** each cabinet sends 7,170 bytes a frame (a 3,585-byte data frame and a 3,585-byte
   vsync frame; 412 KB/s); the useful part is a 448-byte block per cabinet, of which ~18-33 bytes
   change a frame (51-86 bytes deflated as a delta). The frame is a shift register of the 8
   blocks. **A star works** (implemented as `link_topology=star` and measured): everyone one hop
   from everyone, the tokens answered locally, empty seats need nothing (zero blocks are fine
   once formed), the master seat is not needed after the numbering.

7. **Built** (the last section, "The bridge"): one cabinet a machine on a star, from per-seat
   states, blocks through a relay: two seats race /2 from separate processes, a third START opens
   its own session, a spectator's replay is exact. Leaving mid-race: zeros (now the default)
   take the car off the track, a frozen block leaves it there as an obstacle (the car behind
   crashed into it). Leaving between START and the race start leaves the session's other
   entrants waiting for good, either way: the game's, for a frontend to handle.

**Recommendation** (details at the end): a star through a relay, each browser running its own
cabinet loaded from a per-seat preset state, a transport that never refuses (drop), paces to one
frame a frame, and answers the numbering; leaving = the seat's block freezes (or zeros). Latency
per hop up to ~0.5 s is tolerated by the game once formed; nobody ever boots over the network.

## M1: an 8-cabinet ring forms

`ring-lab.mjs form`: 8 cabinets, presets `8/0`-`8/7` through `nvram_dir`. The boards are off
until frame ~190 (the game starts its link), waiting until the master's 232-frame timer runs out,
then numbered within 10 frames: **all 8 up by frame 420, cabinet k "up, id k+1 of 8"**
(`U1/8 U2/8 ... U8/8`). From frame ~600 every cabinet is in the linked attract mode: "LINK
SYSTEM / UP TO 8 RACERS WANTED", rankings, demo races, all on the same screen at the same time
(`m1-eight-attract.png`: the 8 screens at frames 2401-2408, Ranking on all, FREEPLAY).

Cost (`ring-lab.mjs bench --base`, 600 frames from the base state, one cabinet viewed):

| | per frame |
|---|---|
| 8 cabinets, one drawn | 23.21 ms average, p50 23.48, p95 24.71, worst 27.96 (budget 17.38) |
| per cabinet (`daytona_timings`) | game code + board 1.39-1.42 ms, geometrizer 0.33-0.34, sound board 0.26-0.27 |
| the drawn cabinet's rasterizer | 6.96 ms |
| 8 cabinets, nothing drawn (`form`) | 14.1 ms |
| wasm memory (8 cabinets, base state loaded) | 1,166 MB |

So the whole ring does not fit one browser (23 ms > 17.4 ms), but a cabinet alone is ~2 ms of
game plus ~7 ms of drawing: the arcade mode has each browser run one cabinet (`cabinets=1`-like
cost: 8.1 ms a frame in Node, README "Measured").

## M2: the join rules

From the base state (frame 2408, all in attract). Cabinet 3 START at frame 3008, cabinet 5 START
2 s later (3123), both the accelerator 5 s after (BEGINNER, wheel centred), then holding it;
cabinet 7 START at 5108, after their race began (`m2-join-flow-1.png`, `m2-join-flow-2-race-and-late-starter.png`,
shots every 120 frames of cabinets 3, 5, 7, 1):

- 3008: cabinet 3 opens **Circuit Select** with a countdown (14 at 3069); every idle cabinet
  shows **"WAITING FOR YOUR ENTRY / PUSH START BUTTON / 14 SEC"** with the same count
  (`m2-start-and-join-prompt.png`, cabinet 0 at 3070).
- 3123: cabinet 5's START puts it in the same Circuit Select, same count. After choosing, both
  show "WAITING FOR OTHER CHALLENGERS" until the count reaches 0 (~3969): choosing does not cut
  the window short. Idle cabinets go back to the attract at ~4091.
- 4209 Mission Select (Automatic / Manual), 4449 GENTLEMEN START YOUR ENGINES, rolling start,
  **GO! at 5169**: they race together, **cabinet 3 "2ND/2", cabinet 5 "1ST/2"** (a linked race
  counts only the linked cars; a race alone shows n/40).
- 5108 (during the rolling start): cabinet 7's START opens **its own Circuit Select** (14) and
  the idle cabinets show "WAITING FOR YOUR ENTRY" for it; at 0 it goes to Mission Select alone:
  a separate session while 3 and 5 race. Sessions are independent: M5 below had two races of two
  on the ring at once.
- **Join window** (`ring-lab.mjs sweep`, cabinet 3 START at 3008, cabinet 5 START v frames later,
  `m2-join-window-sweep.png`, `m2-join-window-920-940.png`): v = 800, 900, 920, 940: joined, the
  race shows /2 on both. v = 960 (16.7 s): ignored (cabinet 5 stays in attract, cabinet 3 races
  alone, /40). v = 1000, 1040, 1080, 1120: a new session (cabinet 5's own Circuit Select at 14).
  The count runs at ~65 frames a step (13 at 3129, 0 at 3969), so **the window is the 15-step
  countdown, ~940 frames = 16.3 s after the first START**.
- START is taken on the press (edge), and there are dead instants: a press starting at frame 2438
  (end of a Ranking screen) is ignored however long it is held (1-40 frames), while presses
  starting at 2418, 2428, 2448 and 2458 work, as do 10-frame presses at 10 phases across the
  attract (`m2-start-phases.png`, `m2-start-dead-instant*.png`). A player whose press is lost
  presses again; a frontend should pass presses through as they come.

## M3: latency and jitter

`link_delay=d`: every cable (hop) holds bytes d frames; `link_jitter=j`: up to j more at random,
in order. Ring of 8, so the master's data reaches cabinet 1 after 7 hops; a block's age where it
is read is (hops) x d (M6).

| case | result |
|---|---|
| 8-ring, d set from power-on: 3, 4, 5 (52-87 ms a hop) | up at 460-490, attract, the pair joins and races /2 (`m3-ring8-d3.png`, `m3-ring8-d4.png`, `m3-ring8-d5.png`) |
| 8-ring, d = 6 (104 ms) | up at 510; at **frame 1010 cabinet 1 (id 2) restarts its link**, is never numbered again, and every cabinet stays on NETWORK CHECKING to frame 6700 (`m3-ring8-d6-stuck.png`, `m3-ring8-d6-network-checking.png`) |
| 8-ring, d = 12, 18 | up, then at 1010 **all** restart their link, CANCELLED on screen, renumbered, again every ~830 frames (1010, 1840, 2660, 3490, 4310, 5140, 5960): never past the check (`m3-ring8-d12-cycling.png`, `m3-ring8-d12-cancelled.png`) |
| 2-ring, d = 6, 12, 24 (up to 417 ms) | attract and race /2 (`m3-ring2-d6-d12-d24.png`) |
| 2-ring, d = 36; 48, 72 | cabinet 1 restarts at 1010, stuck; both cycle (`m3-ring2-d36-d48-d72.png`) |
| star (below), d = 6, 12, 18, 30 | attract, join, race /2 (`m6-star-d6.png`, `m3-star-d12-to-d42.png`) |
| star, d = 36, 42 (730 ms) | past the check, attract |
| star, d = 60; 90 | the slaves restart at 1010 and wait; cycling |
| **8-ring formed at d = 0, then d = 6, 12, 18, 30** (set in attract) | stays up; the pair joins and races /2 at every one (`m3-ring8-late-d6.png`, `m3-ring8-late-d12-d18-d30.png`, `m3-ring8-late-race.png`). The two racers' screens drift apart by the delay (at d = 18 the Mission Select count shows 3 on cabinet 3 and 0 on cabinet 5 in the same frame) and the race starts later the longer the delay |
| 8-ring, d = 3 + jitter 0-3 from power-on | up, attract, but every linked timer crawls: cabinet 0's entry count reads 13 at 3303, 12 at 3603, 10 at 3903, 9 at 4203, 7 at 4503, 5 at 4803 (~150 frames a step instead of ~65), the circuit highlight blinks irregularly, the race had not started at 5300 (`m3-ring8-d3-jitter3-slow.png`, `m3-ring8-d3-jitter3-consecutive.png`) |
| the same with **`link_pace=1`** (one data frame a frame per board) | normal pace: 6 SEC at 3603, GO! at ~5200 as without jitter, /2 (`m3-ring8-d3-jitter3-paced.png`, `m3-star-and-paced-race.png`) |
| star, d = 6 + jitter 0-6 | normal pace, GO! at 5201, /2 (`m3-star-d6-jitter6.png`) |

Reading it: the strict limit is the boot-time network check at frame 1010 (it passes at 35
frames master-to-farthest-slave in the 8-ring and 24 in the 2-ring, fails at 42 and 36; the star
passes at 42 a hop); after it, the game tolerates long delays. Jitter hurts because the game moves
on per data frame received (likely because the board's new-data flag, ZFG, toggles once per
frame received, so two arriving in one poll look like none; not traced in the game's code), so
the transport must hand over one frame a frame. Other cars: with
a constant delay one update arrives every frame, so they move smoothly, drawn from data
(hops) x d frames old (M6); consecutive screens (`m3-ring8-d3-consecutive.png`,
`m3-ring8-d5-consecutive.png`, `m3-ring2-d12-consecutive.png`) show smooth play but rarely the
other car close by (both drivers only hold the accelerator), so the positional lag itself is not
shown in pictures; with jitter and no pacing the screens stall (the circuit highlight, above).

## M4: leaving

From the race base (3 and 5 racing, frame 5418; holding the accelerator) and from the attract
base (2408), cabinet 5 cut / paused / ghosted at t = 60, screens of 3, 5, 4 (after 5 in the
ring), 6 (before it), 0 (master) and 1 every 60 frames for 10 s, then every 300; at t = 2400 the
mode is cleared and cabinet 5 powered on alone (`m4-early-*.png`, `m4-late-*.png`; a screen hash
tells frozen from moving):

| mode (default transport: a full cable refuses) | link | screens |
|---|---|---|
| `link_cut=5` (powered off: cables closed) | 4 and 6 lost at once, 7 after 20 frames, the master after 40 (backpressure: lost boards stop reading) | **every cabinet frozen within ~1 s** (the last change between t = 30 and 90), racers mid-race and idle ones alike, to t = 6000; no error screen |
| `link_pause=5` (not run, cables open) | 6 lost after the 32-frame cable fills (t = 82), then 7, the master | the same: all frozen |
| `link_ghost=5` (board relays, game stopped) | all up | everyone runs: cabinet 3's race goes on (time 95 → 17, it passes to 1ST/2, lap 2, TIME EXTENSION), cabinet 5's car stays where it was (`m4-race-ghost-cabinet3.png`); idle cabinets cycle the attract |
| reconnect + power-on of 5 (all three) | 5 boots, waits for its number forever (the master numbers only at its own boot); in the ghost case its fresh board does not relay while booting, so 6, 7 and the master are lost within 60 frames | all frozen to the end (100 s): **the ring does not recover** |
| attract instead of race | the same in all three | the same |

With a transport that drops its oldest frames instead of refusing (`link_full=drop`):

- `link_pause=5` for 2,340 frames: no board lost; 4, 3, 2, 1 and the master freeze, 6 (still
  fed by the master through 7) goes on; on resume **everyone continues** from where they stopped.
  In a race, a 20 s pause of racer 5: both racers frozen at time 45, then the race goes on
  (44, 43, ... lap 2) (`m4-race-pause-20s-racers.png`, `m4-race-pause-20s-resume.png`).
- pause, then power-on of 5 with `link_assist=1`: it boots, is handed its number 186 frames
  later (U6/8), goes through NETWORK CHECKING into the linked attract, everyone resumes
  (`m4-pause-drop-reset-assist.png`); then **5 and 3 start a race together, /2**
  (`m4-fresh-boot-rejoin-race.png`). Ghost then power-on with drop + assist: a ~180-frame freeze
  while 5 boots, then all resume.
- The **master as a ghost** (`link_ghost=0`, its game stopped from t = 60): all others run, 3
  and 5 join and race /2 (`m4-ghost-master.png`). The master's game is not needed after the
  numbering, only its board (in the ring it is the clock: it sends a frame every frame).

## M5: swapping a cabinet in from a saved state (the crux)

`daytona_cabinet_save(k)` / `daytona_cabinet_load(k, ...)`: cabinet k alone (its GameLoop with
the comm board, EEPROM and backup RAM, and the shim's seat and sound carry); the cables and the
other cabinets are untouched.

- **a. Same run** (`m5a-load.png`, `m5a-race.png`): from the attract base, cabinet 6 saved at
  frame 3000 (8,807,707 bytes); the ring runs on; **loaded back at frame 12000**. Link: all 8 up
  every frame to the end (15608), no cabinet frozen (screen hashes change at every shot). Cabinet
  6 goes on from its own frame-3000 attract (TRY FOUR VIEWS, the title) while the others show
  LINK SYSTEM: the attract is per cabinet. Cabinet 6 START at 12708, cabinet 2 at 12823: Circuit
  Select on both, the other cabinets show WAITING FOR YOUR ENTRY, then **they race: 6 "2ND/2"
  (car 7), 2 "1ST/2" (car 3)**.
- **b. Loaded while another pair races** (`m5b-load-mid-race.png`, `m5b-two-races.png`): 3 and
  5 race from frame ~11410 (GO!); cabinet 6 loaded at 12000 while they are on lap 1 (3: 2ND/2,
  time 39). Their race goes on undisturbed (lap 3/8 at 15010) and **6 and 2 race at the same time
  (/2)**: two races of two on one ring.
- **c. A preset state from a different run** (`m5c-preset-from-other-process.png`): a fresh
  process formed its own ring and saved cabinet 6 at its frame 5000 (`ring-lab.mjs capture`,
  8,715,472 bytes, 1,015,218 gzip -6); loaded into this run's cabinet 6 at 12000: the same as a,
  race /2. The same preset (made in a ring) loaded into cabinet 6 of a **star** machine: race /2
  (`m6-star-load-preset.png`); and into a star where 6 of the 8 seats are absent (below).
- Timings (Node): save 4.9-20.9 ms, load 1.2-1.4 ms; 8.72-8.81 MB, ~0.95 MB deflated.

What works, exactly: any state of seat k's own cabinet, saved after the link was up (it carries
link id k+1 of 8 and car k+1), loads into seat k of a running ring or star, from the same or
another process, in attract or with races running elsewhere, and plays at once. What it needs:
the seat's cables (ring) or block (star) must be kept by something while nobody sits there
(here: the cabinet running idle, or a ghost); a state carries its seat's id, so each seat needs
its own preset (8 presets for 8 seats). Not tested: loading a seat's state into another seat;
cabinets in separate processes with their own clocks (the star with jitter is the nearest).

## M6: the protocol (comm_board.cpp, MAME's m2comm simulation)

**Wire frames.** Every frame on the wire is `data_size` = frame size + 1 = **0xe01 = 3,585
bytes**: byte 0 a type, then 0xe00 bytes. The board sets frame size 0xe00 and frame offset 0x1c0
in its 16 KB shared RAM at init (the game writes CN = 1); the game's part of the shared RAM is
0x2000-0x2fff.

| type | sent by | content | what a receiver does |
|---|---|---|---|
| 0xff numbering | master, every frame from its 232-frame timer until one comes back | [1] = 1 | slave: [1] += 1, forward; relay: forward; master: count = [1], its id = 1 |
| 0xfe ids | master once, after its 0xff came back | [1] = [2] = count | slave: id = [1], count = [2], [1] -= 1, forward; the link is up (shared RAM [0] = 1, [2] = id, [3] = count) |
| 0x01..count, data | each node | the sender's shared RAM 0x2000-0x2dff | store it at 0x21c0-0x2fbf, toggle the new-data flag (ZFG); a slave answers each with its own data frame |
| 0xfc vsync | master, after its data frame each frame | [1] = 1 (the rest stale) | slaves forward it; with frame sync off (MAME's default, and here) it changes nothing |

So the first slave after the master gets the highest id and the last one id 2; the shim's ring
runs 0 → N-1 → ... → 1 → 0 so that cabinet k has id k + 1.

**Per frame**, once up: the master sends its data frame and a vsync at vblank; every slave sends
one data frame per data frame it receives and forwards the vsync. **Each cabinet sends 7,170
bytes a frame (one data, one vsync), 412 KB/s, 3.3 Mbit/s, the same for every N** (`link_stats`,
600 frames, attract and race: 7170 out, 7170 in, 1.00 data and 1.00 vsync a frame).

**The blocks.** The data frame a node sends is its shared RAM 0x2000-0x2dff = its own block
(0x2000-0x21bf, **0x1c0 = 448 bytes, written by the game**) followed by the first 0xc40 bytes of
the last frame it received (stored at 0x21c0). So a frame is a shift register of 8 blocks: the
frame node j receives (from node j+1) holds the blocks of ids j+1, j+2, ..., j+8 (mod N; with 8
nodes the last is its own, a round old). Measured: of 600 data frames per cabinet, **600 had
bytes 0x1c0-0xdff equal to the start of the frame received before (0 differ)**: a node's frame
depends on nothing but its own block and what it received. Block ages at the master by position
(`ring-lab.mjs protocol`): all 1 frame with no delay (the ring goes round within a frame here);
with 6 frames a hop, position p is 6 (p+1) frames old: 6, 12, 18, 24, 30, 36, 42, 48.

**What changes in a block** (448 bytes; per-frame deflate of the block alone, and of its XOR with
the previous one):

| | non-zero bytes | changed a frame | deflated | delta deflated |
|---|---|---|---|---|
| attract (all 8) | 94-96 | 17.9 bytes, 596 of 599 frames | 151-154 B | 51 B |
| race, the two racers | 156-157 | 32.0-32.9 bytes, every frame | 239-242 B | 82-86 B |
| race, the six idle | 117-119 | 21.4-21.5 bytes | 168-171 B | 53-54 B |

**A star instead of the ring.** Since the frame a node receives is fully determined by the other
nodes' latest blocks, each node can send only its block and each node's transport can build
"the frame from its predecessor" from the latest blocks of all: [type = predecessor's id][blocks
of ids j+1, ..., j+8]. The tokens do not make it hard: the vsync carries nothing with frame sync
off (dropped); the numbering is a function of the seats, so the transport answers it (the
master's 0xff comes back with the count, its 0xfe reaches each slave with that slave's id). Built
as `link_topology=star` and measured:

- 8 cabinets come up (by frame 420), attract, join and race /2, at 0 to 30 frames a hop with
  every block one hop old, at 6 + jitter 0-6 at normal pace; the boot check passes up to 42 a
  hop (ring: 5).
- Leaving costs nothing: the master cut, cabinet 5 paused, 4 seats cut with **zero blocks**
  (`link_blank`): the others go on and race (`m6-star-master-cut.png`, `m6-star-pause5.png`,
  `m6-star-blank-seats-two-players.png`). Only cabinets 2 and 6 running, 6 of them loaded from a
  preset made in another process, the 6 other seats absent with zero blocks, no master: they
  race /2.
- But a fresh boot with seats absent from power-on stays on NETWORK CHECKING forever
  (`m6-star-boot-four-absent.png`): the boot check wants all N seats alive. Nobody should boot
  over the network: seats start from presets.
- Traffic: a node sends its block, 448 bytes raw a frame (25.8 KB/s), ~150-240 bytes deflated,
  ~50-90 bytes as a deflated delta; it receives N-1 blocks. Against the ring's 412 KB/s each way
  for everyone, and 1 hop for every block instead of 1 to 7.

## Recommendation

- **Sitting down: yes, via a preset state.** Make 8 presets offline (a local ring formed with
  `ring-lab.mjs capture`, one per seat, ~1 MB deflated each); a player sitting at seat k loads
  seat k's preset into their own single-cabinet machine and is on the link at once, in attract,
  ready to START. Never boot over the network (the boot check is the fragile part: 5 frames a hop
  in an 8-ring).
- **Topology: a star through the room's relay** (a server or a Durable Object fan-out), not a
  ring: each browser sends its 448-byte block a frame (~50-90 B as a delta, ~240 B whole) and
  builds its received frame from the latest blocks; one network hop between any two players,
  nothing to relay for empty seats, no master to keep alive, no order between seats. The
  transport answers the numbering, drops the vsync, never refuses, and hands its board one frame
  per local frame (the latest blocks), which also absorbs jitter and clock drift (tested: jitter;
  not tested: separate clocks).
- **Delay:** once formed the game tolerated everything measured, up to 30 frames (0.5 s) a hop in
  the 8-ring (3.6 s round it) and 42 a hop in the star at boot; what players see is lag (other
  cars drawn from data that old) and screens out of step by it. One relay hop of 50-150 ms
  (3-9 frames) is comfortable. Avoid irregular delivery: pace to one frame a frame.
- **Leaving:** when a player leaves (or their tab freezes), keep their seat's block as it last
  was (the star does it by doing nothing) or zero it; the others go on, a car mid-race stays
  where it stopped. In a ring the same needs a ghost relay and a transport that drops instead of
  refusing; without them one leaver freezes all 8 for good.
- **Cost:** a browser runs one cabinet: ~2 ms of game and ~7 ms of drawing a frame in Node (8.1
  ms measured for one cabinet), the same as the twin cabinet today.

## The bridge: the arcade mode as built

2026-10-07. The recommendation above, built: the shim's class `Bridge` (`cabinets=1`,
`link_topology=star`, `seat=k`; exports `daytona_link_out` / `_in` / `_absent`, README "The
arcade mode"), one whole-machine state per seat (`make-states.mjs`: cabinet k of a ring formed in
Node, saved at frame 5000 in the linked attract, loaded into a one-cabinet star machine,
serialized and packed, ~1.05 MB), and `harness/arcade-check.mjs` (seats in separate Node
processes, blocks through a relay 6 frames late plus 0-3 of jitter). The findings below come from
it and from in-process prototypes (two or three such machines in one process passing blocks 6
frames late; screens in `$SCRATCH/../bridge/proto/`, the check's in `$SCRATCH/../bridge/arcade/`).
Frames count from the seat state's load.

- **Two seats race.** Seat 0 START at 300, seat 1 at 415 (2 s later), accelerator at 600/605:
  one Circuit Select with 2 entrants on both, Mission Select, GENTLEMEN START YOUR ENGINES, the
  race from ~1900: **1ST/2 and 2ND/2**, red car 1 and blue car 2, in two processes as in one.
- **A third seat's START after the race began** (2100) opens its own session: Circuit Select 14
  alone, then a race alone (**/40**, yellow car 3) while the pair race on. Sessions stay apart.
- **Determinism.** Seat 0's per-frame log (pad, blocks handed over) replayed in a fresh process
  from its seat state: RAM and screen the same at all 25 checkpoints (every 120 frames to 3000);
  from the state it saved at 1200 (a spectator joining there) with the log from there: 15/15.
  The table of blocks is in the state, so a spectator needs no blocks from before it.
- **Leaving mid-race** (seat 1 at 2700, ~13 s into the race; seat 0 was behind it):
  - `link_absent=freeze`: seat 1's car stays where it stopped. **Seat 0 crashed into it** 60
    frames later at full speed (its car flipping, 72 mph), then went on, 1ST/2.
  - `link_absent=zero`: seat 1's car is gone from the track; seat 0 drove on (132 mph at the same
    frame), 1ST/2. Hence zero is the default.
  - Either way the race went on to its end (RESULTS, 1st), back to the attract mode by ~9600:
    nothing at the end waits for the leaver. The HUD keeps counting /2.
- **Leaving between START and the race start** (one of the pair at 700: both entered, circuit
  chosen): **the other entrant waits for it for good** (watched for 12,000 frames, 3.5 min, when
  the joiner left; 3,000 when the first starter left), with either policy and whichever of the two
  left: zero: Circuit Select 0, WAITING FOR OTHER CHALLENGERS; freeze: the same if the first
  starter left, Mission Select "DECIDED LIKE THIS 3" if the joiner left. Handing over the leaver's
  own block from before its START (idle in the attract mode) instead: the same, Circuit Select 0.
  The entrant count in RAM (0x540027) drops with zeros (to 1, or 0 when the first starter left),
  yet it waits; the stuck cabinet's own block still changes every frame. So the game keeps a
  session's entrants and waits for each at each step, with no timeout (in the ring a cabinet
  switched off freezes everyone anyway, M4). Not solved in the shim; a frontend can (a) keep a
  seat that pressed START until its race began (or until it is sure it left), (b) reload the stuck
  players' seat states (back to the attract mode, the session lost) when a seat of their session
  leaves before GO! (the frontend sees who pressed START and when; RAM 0x501080 is 10 until a race
  begins), or (c) keep the leaver's cabinet running without input somewhere (its countdowns run
  out; its car stays on the grid). Not tried. The frontend does (b): see the mode byte below.
- **Booting a bridge** from power-on with a ring preset (`nvram_dir`, no state): the bridge
  answers the numbering (seat 0, the master: up 1 of 8 at frame 416, after its 232-frame wait;
  seat 3: handed its 0xfe, up 4 of 8 at 183), but alone each stays on the network check (its
  settings, THIS IS MASTER CONTROLLER / THIS IS SLAVE MACHINE) to frame 1800 and on, as M6 found
  for the star: seats start from their states.
- **Leaving while idle** (attract) and **seats empty from the start** (zeros): nothing happens
  to anyone; every run above had 5 or 6 empty seats.
- **Game facts used by the check** (found by comparing RAM dumps of a linked and a lone race):
  main RAM 0x501080 is the number of cars on the track (10 outside a race, 16 in a linked race of
  two, 40 in a race alone), 0x540027 the entrants in this cabinet's session (2, 1, 0 idle).
  Later (the frontend's fix for the leaver before GO!, `web/emulator/worker.js` CabinetLink):
  an idle cabinet counts another's session's entrants too, so 0x540027 can't tell an entrant;
  **0x5010a0 is the game's mode** (0x5010a4 the same): 3, 5, 7, 9, 11 through the attract mode
  (even values for a frame or two between), 16 idle while another's session takes entries
  (WAITING FOR YOUR ENTRY), 12, 17, then **18 from this cabinet's START** (circuit select,
  mission select, the grid) until its race, 19-22 the race (22), 25-30 the results, back to 8/9.
  A healthy session holds 18 for 1579 frames with choices made and 1866 (32.4 s) at most, with no
  choice made and every count run out; a stranded one holds it for good (both leaver cases,
  entrants then 1 or 0). Found by comparing RAM of entrants and others in three seats in one
  process (two leaver cases, a healthy session, 9000 frames of attract), then checked in the lab.
- **Cost** (Node): 8.25 ms a frame drawn (`bench.mjs` from seat 0's state: game code 1.57,
  rasterizer 5.32), 2.5 ms with nothing drawn; wasm memory 256 MB; reset + load of a seat state
  11 ms.
- Not tested: separate clocks (the check keeps its processes within 3 frames of each other), a
  real network, more than three seats in separate processes, a START late in another seat's
  entry window from a seat state (the window itself is M2's).

## The shim and the lab

Shim (`shim/libretro.cpp`, defaults unchanged; `node daytona/check.mjs` passes, and the
two-cabinet RAM hash after 3,600 frames of a race, `bench.mjs WARMUP=1500 PLAY=1`, is the same
`ef17be73` with the shim before these changes): `kMaxCabinets` 8 (`cabinets` 1-8, `script0`-`script7`,
the timings and status per cabinet, presets `<nvram_dir>/<k>/` then `/daytona/nvram/<N>/<k>/`),
the ring 0 → N-1 → ... → 1 → 0 run in that order (two cabinets: as before), and the experiment
options and exports in the README's "Link experiments". The cables are chunks with a ready time
(delay, jitter); whole-machine states keep the bytes, not the times, and leave the star's blocks
out.

```sh
SCRATCH=...; export ROMS=<dir with daytona.zip>
node daytona/make-nvram.mjs make --only 8/ --out $SCRATCH/nvram          # presets 8/0-8/7
L="node daytona/harness/ring-lab.mjs"; O="--out=$SCRATCH/out --nvram=$SCRATCH/nvram"
$L form $O                                                               # M1: ring, base state at 2408
$L bench --base $O                                                       # M1: cost
$L run --base $O --events="3:start@600+10,5:start@715+10,3:up@900+10,5:up@905+10,3:start@1500+10,5:start@1500+10,3:up@1800+2400,5:up@1800+2400,7:start@2700+10" --frames=4200 --every=120 --shots=3,5,7,1   # M2
$L sweep --base $O --events="3:start@600+10,3:up@900+10,5:start@{v}+10" --values=1500,1540,1560,1600 --shotAt="1660;2500" --shots=5,3   # M2 window
$L run $O --set=link_delay=6 --events="..." --frames=6700 --every=300 --shots=3,5,0                       # M3 (boot with a delay)
$L run --base $O --events="ghost=5@60,ghost=@2400,reset:5@2401" --frames=6000 --every=300 --shots=3,5,4,6,0,1   # M4
$L run --base $O --events="set:link_full=drop@0,set:link_assist=1@0,pause=5@60,pause=@600,reset:5@601,5:start@2400+10,3:start@2515+10,..."   # M4 rejoin
$L run --base $O --events="save:6@592,load:6@9592,6:start@10300+10,2:start@10415+10,..." --frames=13200 --every=600 --shots=6,5,7,0,2,3   # M5a
$L capture $O --at=5000 --cab=6 --file=$SCRATCH/preset6.cabstate          # M5c, then load:6@9592=$SCRATCH/preset6.cabstate
$L protocol --base $O --frames=600                                       # M6
$L form --out=$SCRATCH/star --nvram=$SCRATCH/nvram --set=link_topology=star   # M6 star base (then run --set=link_topology=star --base=$SCRATCH/star/base-8.state ...)
node daytona/make-states.mjs --nvram=$SCRATCH/nvram                        # the bridge: seat states -> daytona/dist/states/
node daytona/harness/arcade-check.mjs --out=$SCRATCH/../bridge/arcade [--absent=freeze]   # the bridge: separate processes
```
