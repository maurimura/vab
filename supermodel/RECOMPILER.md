# PowerPC → WebAssembly recompiler for Supermodel

## Why

Online play of the Model 3 games is capped by one fact: emulating a frame costs ~14 ms because
the PowerPC is interpreted (a fetch, a decode, an indirect dispatch, and flag bookkeeping per
instruction). That heaviness means neither netcode works well at a real ping:

- **Lockstep** waits for the other player each frame, so motion is smooth but input is delayed by
  the ping.
- **Rollback** shows input instantly but re-simulates to correct a mis-prediction, and each
  re-simulated frame is that same ~14 ms, so corrections stutter and drop the frame rate.

The only way to get smooth *and* instant is to make a frame cheap to (re-)simulate. A recompiler
translates each PowerPC basic block into a WebAssembly function once, keeps guest registers in
locals, and drops the per-instruction fetch/decode/dispatch, which is a 3–5× speedup (the same
move that took Flycast's SH4 port from ~2 fps to a locked 60). A ~3 ms re-sim makes rollback
affordable, so online play becomes smooth with instant local input.

## Architecture

The emulator is itself WebAssembly, so the recompiler generates *new* WebAssembly at runtime and
shares the emulator's linear memory with it (a module can't modify itself, but the embedder can
compile new modules that import the same memory and table — this is the mechanism milestone 1
proves).

- **Block decoder** (C++): from a guest PC, decode PowerPC instructions until a block-ending one
  (branch, `rfi`, `sc`, a page boundary). Supermodel's interpreter in `Src/CPU/PowerPC/` is the
  reference for every instruction's exact behaviour.
- **Codegen** (`shim/jit/wasm_emit.h`): emit one WASM function per block. Guest registers live in
  the `ppc` struct in linear memory; the fast version loads the live ones into WASM locals at
  block entry and writes them back at exit.
- **Runtime compile + link** (JS, via `EM_JS`): `WebAssembly.compile` the emitted bytes with the
  emulator's `memory` (and later a shared `Table`) imported; put the exported block function into
  the table and return its index.
- **Block cache** (C++): guest PC → table index. Execute a known block with `call_indirect`;
  decode+compile on a miss. Falls back to the interpreter for not-yet-covered instructions
  (hybrid), so it is always correct while coverage grows.
- **Memory**: a fast inline path for the 8 MB RAM (mask + load/store in linear memory); a call-out
  to the C++ `Bus` handlers for MMIO.
- **Invalidation**: self-modifying code and DMA into code pages invalidate cached blocks.
- **Determinism**: the JIT must be byte-exact with the interpreter, or rollback desyncs. Every
  step is validated against the interpreter with the existing determinism harness
  (two runs, compare the 8 MB game RAM — see `retro_get_memory_data`).

## Milestones

1. **Runtime codegen works.** ✅ Done. `shim/jit/` generates a WASM module at runtime whose code
   reads/writes the emulator's own linear memory (`supermodel_jit_selftest`, run via
   `node .../jit-selftest.mjs`). This de-risks the whole approach.
2. Decode + compile a block of the common integer ops. ✅ Done: addi/addis/ori/oris/xori/xoris/
   add/subf/or/and/xor, validated byte-exact against the interpreter over 1000 random blocks
   (`supermodel_jit_test_integer`), with an interpreter fallback for uncovered ops.
3. Broaden integer coverage (rlwinm/rlwimi, shifts, mul/div, load/store variants, update forms).
4. Condition register and XER. ⏳ In progress: CR0 for record forms and andi./andis. (signed
   compare to zero plus the summary-overflow bit) done and validated; XER carry/overflow next.
5. Floating point (the 603's FP, matching the interpreter's rounding).
6. Branch family: `b`/`bc`/`bclr`/`bcctr`, LR/CTR, the link bit.
7. Invalidation for self-modifying code and DMA.
8. Register allocation into WASM locals, idle-loop detection, batch compilation; perf pass.
9. Measure and tune, target a re-simulated frame under ~4 ms, then switch the game to rollback.

## Status

Milestones 1-2 complete and validated; milestone 4 (flags) underway (CR0 done). This is a
multi-week effort; each remaining milestone is gated on byte-exact agreement with the interpreter.
