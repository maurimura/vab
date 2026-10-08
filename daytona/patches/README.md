# Patches to daytona-arcade-recomp

Applied with `git apply`, in order, to <https://github.com/alphanu1/daytona-arcade-recomp>
(BSD-3-Clause) at the pinned commit `1877da9` (2026-10-06). Each applies cleanly to that
commit on its own; `daytona/build.sh` applies them. The upstream repository's own rules
(rules.md) are followed in the code: no fast-math, exact FP (floats and doubles saved as their
bits), their naming and comment style.

## 0001-snapshot.patch: save states

Adds `src/runtime/snapshot.h` / `snapshot.cpp` to the `runtime` library, namespace `rt`:

```cpp
std::vector<uint8_t> save_state(GameLoop &game);                    // everything needed to continue identically
bool load_state(GameLoop &game, const uint8_t *data, size_t size);  // false on bad data, machine untouched
size_t state_size_bound(const GameLoop &game);                      // for callers that pre-allocate
uint8_t *main_ram(M2Board &board, size_t *size);                    // the i960's 1 MB work RAM, for desync hashes
```

What a joining player or spectator needs: a second GameLoop of the same ROM set, loaded with
a state, runs on exactly as the machine it came from. Held to that on the real game (Revision
A, a race and attract, frame by frame: screen, 3D layer, work RAM, audio, instruction counts,
and the whole state 300 frames later) and without a ROM by `tests/test_snapshot.cpp`. Details,
measurements and risks: [../snapshot-notes.md](../snapshot-notes.md).

The format is versioned (64-byte header: magic `M2SNAPST`, version, body size, a 64-bit hash of
the body, the ROM set name, fingerprints of the i960 and sound program images), then sections
with a four-character tag, a version and a size each. A load checks all of it before it changes
anything. About 8.8 MB (7.2 MB of it the board's RAMs, much of it zeros: texture RAM's unused
halves, the frame buffers the game never writes); zlib level 6 takes a race state to 1.03 MB.
Save and load take about 1 ms each natively.

Upstream files changed, besides the new ones:

| File | Change |
| --- | --- |
| `lockstep.h/.cpp` | `add_callback` takes a `Callback` tag (default `Untagged`); events keep it, so pending callbacks can be saved and made again |
| `game_loop.h/.cpp`, `m2_board.h/.cpp` | the frame probe and the UART shift callbacks are made by `probe_fn()` / `uart_shift_fn()` and tagged |
| `snd_sched.h` | `Sched::at` takes a `Tag` (kind, argument); events keep it |
| `sound_board.h/.cpp` | `event(Tag)` makes the two event kinds (a byte arriving at the UART, a YM timer), used by `send`, `ymfm_set_timer` and a load alike |
| `enhance.h/.cpp` | the draw-list hook's "budget raised" flag moves from a function-local static to `Enhance::budget_raised`, so a state carries it |
| `video.h`, `raster.h`, `geo.h`, `m2_tgp_board.h`, `comm_board.h`, `multipcm.h` (and the above) | `friend class SnapshotAccess` (snapshot.cpp reads and writes their private state) |
| `CMakeLists.txt` | `snapshot.cpp` in `runtime`; `test_snapshot` (ROM-free, `ctest`); `test_snapshot_rom` in the generated-code block, skipped (77) without the imported images, with the comm board looped back for Revision A (its factory settings wait for a link) |

None of these change what the game does: the tags only label callbacks, the lambdas are the
same code moved into one function each. The Dreamcast build (`M2_DC_MEMORY` / `M2_DC_SPEED`) is
not covered: `snapshot.cpp` refuses to compile there, and that build lists its own sources,
so nothing changes for it.

Regenerating the patch from a working clone at 1877da9 with the changes:

```sh
git add -A src tests CMakeLists.txt && git diff --cached 1877da9 > 0001-snapshot.patch
```

## 0004-polygon-limit-drops-the-frame.patch: a display list past the polygon limit drops that frame's 3D

Upstream (as MAME, whose `fatalerror` it turned into `throw GeoFatal`) stops the machine when a
frame's display list makes more than 32768 polygons ("SEGA 3D: Max polygon limit exceeded").
With the patch, `Geo::parse` catches it instead: that frame's 3D is dropped (`render_frame_start`
again: an empty polygon list, the z range and window reset; the tilemaps and the HUD are drawn
as ever), the rasterizer's command decoder is left idle (`cur_command`, `command_index` 0, as
after a list that ended), and `Geo::dropped_frames()` counts it. The next frame's list is
parsed as ever. Nothing the game reads changes (its RAM, the TGP, the board's counters), and
what the list did before the limit (the geometrizer's own registers and RAMs) stays done, the
same on every machine. The count is not machine state (save states leave it out); the shim
reads it through `M2Board::geo()` and logs "Cabinet N: a display list ran past the
geometrizer's 32768 polygons: that frame's 3D was dropped (k so far)" the first time and every
100th.

Why: two linked cabinets where only cabinet 1 starts a game (one player alone at the bar's
cabinet). Cabinet 2 shows its challenger countdown, goes back to its attract mode and, on the
3D road of its 通信システム screen, at one phase of its frames, the geometrizer parses the
display list at 0x10000 before the game has finished it: no end code, 9,897 commands, past
32,768 polygons; the next frame parses the same buffer whole (62 commands, 1,706 polygons), so
the double buffer slipped a frame. It hits when that frame lands on 1 mod 16: 9 of 206 sampled
start timings (Start first pressed at frames 61-1500, step 7), 3 of 29 early ones (2-58), and
2 of the 3 browser solo runs that pressed Start near frame 16, before the patch; not seen with
one cabinet, or with both cabinets starting together. The
upstream runtime alone (1877da9 with only 0002, for the shim) stops at the same frame, so it is
the recomp's, not our patches'. The arcade shows nothing of it. The root cause, somewhere in the
recomp's vblank / geometrizer timing (HLE), is not found: worth reporting upstream with the
repro. With the patch the machine goes on: one frame of cabinet 2 without its 3D, then the
attract mode as ever (4,200 frames, the same RAM hash natively and in wasm, and across a save
and load on either side of the frame); `check.mjs`'s hashes are unchanged.

Repro (`0004-repro-inputs.txt`, the recomp's scripts/inputs format for the shim's `script0`):

```sh
DAYTONA_OPTIONS="nvram_dir=$PWD/daytona/nvram/2,script0=$PWD/daytona/patches/0004-repro-inputs.txt" \
  daytona/.cache/native/bench ~/Downloads/daytona.zip 4200 0 2
# without the patch: "The game: SEGA 3D: Max polygon limit exceeded. The machine has stopped" (frame 2001)
# with it: "Cabinet 2: a display list ran past ... dropped (1 so far)", 4200 frames, main RAM hash e686fad2
```

Upstream files changed: `geo.cpp` (the limit throws a type of its own, `PolygonListFull`, which
`parse` catches), `geo.h` (`dropped_frames()`), `m2_board.h` (`geo()`). It applies cleanly to
1877da9 on its own and after 0001-0003.
