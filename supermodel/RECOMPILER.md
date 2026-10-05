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

Done and validated: runtime codegen (M1), integer blocks (M2), CR0 flags (M3 part), load/store
(call into the Bus handlers), and the dispatch integration -- the JIT is wired into the execute
loop and runs the real game BYTE-IDENTICAL to the interpreter over 600 frames (no desync).

Coverage now includes integer, flags, load/store, compares, rlwinm, shifts and the logical ops;
registers are kept in WASM locals per block. Measured on the real game: the JIT runs 55% of all
instructions and stays byte-identical to the interpreter, but it is still 0.79x (slightly slower).
Byte-identical has since been confirmed from boot as well (not just a mid-game state): RAM matches
the interpreter at every checkpoint through 2000 frames.

**Hot-block threshold.** Installing a block means `new WebAssembly.Module` + growing the shared
table. That is cheap in Node but the game has tens of thousands of distinct basic blocks (most run
only during boot), and compiling them all up front froze the browser worker -- the page rendered a
near-black screen because the worker never caught up. So a block is compiled only after it has
executed HOT_THRESHOLD (128) times: boot/one-shot code stays on the interpreter, hot inner loops
cross the threshold within a frame or two, and their compiles spread out instead of storming. On
vs298 this is ~1,460 installs over the first ~500 frames (peak ~7/frame) instead of tens of
thousands at once, and the browser renders normally with the recompiler on. A global INSTALL_CAP is
a last-resort guardrail. This changes only *when* a block compiles, never its result.

Why: blocks average only ~2 instructions, because a branch ends the block and each block returns
to the dispatch loop. The per-block cost then dominates. A direct-mapped block cache and advancing
the code pointer directly (instead of ppc_change_pc) got it from 0.70x to 0.79x.

Update-form loads/stores (lwzu/stwu/...) and mfspr/mtspr for LR/CTR/XER are now covered too (they
bracket every call, so they used to chop blocks). That lifted coverage to ~56% and steady-state
(warm cache) to ~0.87x -- still below 1.0x, and the reason is now measured, not guessed. A
per-hot-block terminator histogram (supermodel_jit_term) shows what ends hot blocks on vs298:

  bc / b / bclr-bcctr   ~7,300   (branches -- unavoidable without linking)
  lfs / lfd / stfs / fp ~1,700   (floating point, not yet covered)
  lwzx and other op31    ~900    (indexed loads)

So branches are ~two thirds of block ends. No amount of extra opcode coverage lifts the ~2-instr
average while every branch returns to the loop. **The one structural fix is block linking**: let a
run of blocks execute without returning to the C dispatch loop (and ideally keep registers in
locals across them), so effective block length becomes tens of instructions and the per-block
overhead is amortized. In WASM this means either the tail-call extension (return_call_indirect) or
an in-WASM dispatch trampoline that replicates the exact icount/decrementer timing -- the latter is
determinism-sensitive and must stay byte-identical (verify with the boot RAM-hash check before any
real-machine use). Covering FP would further cut the interpreted fraction but is secondary to
linking. Enable with supermodel_set("Jit","true") or ?jit=1; off by default. ppc_jit_stat() reports
coverage and block length; supermodel_jit_installs() / supermodel_jit_term() report compile counts
and block terminators.
