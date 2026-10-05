// Milestone 1 of the PowerPC recompiler (supermodel/RECOMPILER.md): prove that the emulator can
// generate a WebAssembly function at runtime and have it share the emulator's own linear memory.
// Everything else in the recompiler is built on this mechanism.
#include "wasm_emit.h"
#include "compiler.h"
#include <cstdint>
#include <cstdio>

#ifdef __EMSCRIPTEN__
#include <emscripten.h>

// Compile the module bytes into a fresh WebAssembly instance that imports the emulator's linear
// memory (so generated code reads/writes the same bytes as the C++ side), then call its "run"
// export. The sync Module/Instance API is fine for the small modules a block compiles to.
EM_JS(int, sm_jit_run_module, (const uint8_t *bytes, int len), {
  try {
    const mod = new WebAssembly.Module(HEAPU8.subarray(bytes, bytes + len));
    const inst = new WebAssembly.Instance(mod, { env: { memory: wasmMemory } });
    return inst.exports.run() | 0;
  } catch (e) {
    console.error("[jit] module compile/run failed:", (e && (e.stack || e.message)) || e);
    return 0;
  }
});

// Compile a block module and place its function into the emulator's own indirect function table,
// returning the table index. That index IS a function pointer in Emscripten, so C++ can then call
// the compiled block with `call_indirect` just like any function pointer -- this is the dispatch
// mechanism the recompiler uses to jump into compiled blocks. The instance is kept alive.
EM_JS(int, sm_jit_install, (const uint8_t *bytes, int len), {
  try {
    if (!Module._jitInstances) Module._jitInstances = [];
    const mod = new WebAssembly.Module(HEAPU8.slice(bytes, bytes + len));
    const inst = new WebAssembly.Instance(mod, { env: { memory: wasmMemory } });
    const idx = wasmTable.grow(1);
    wasmTable.set(idx, inst.exports.run);
    Module._jitInstances.push(inst);
    return idx;
  } catch (e) {
    console.error("[jit] install failed:", (e && (e.stack || e.message)) || e);
    return -1;
  }
});
#else
static int sm_jit_run_module(const uint8_t *, int) { return 0; }
static int sm_jit_install(const uint8_t *, int) { return -1; }
#endif

typedef void (*BlockFn)(void); // a compiled block: runs the instructions, updating the ppc struct

// Generates a module that loads two 32-bit words from fixed addresses in the shared memory, adds
// them, and returns the sum; checks the result against the C++ computation. If this passes, the
// runtime-codegen path the whole recompiler needs is working.
extern "C" int supermodel_jit_selftest(void)
{
  static volatile uint32_t a = 0x11112222;
  static volatile uint32_t b = 0x0A0B0C0D;

  jit::Code c;
  c.i32_const((int32_t)(uintptr_t)&a); c.i32_load(0); // *(&a)
  c.i32_const((int32_t)(uintptr_t)&b); c.i32_load(0); // *(&b)
  c.op(jit::OP_I32_ADD);                               // a + b, left on the stack as the result
  std::vector<uint8_t> mod = jit::module_returning_i32(c);

  uint32_t got = (uint32_t)sm_jit_run_module(mod.data(), (int)mod.size());
  uint32_t want = a + b;
  bool ok = got == want;
  printf("[jit] self-test: %zu-byte module read two words from the emulator's memory and added "
         "them, got %08X, want %08X -> %s\n", mod.size(), got, want, ok ? "PASS" : "FAIL");
  return ok ? 1 : 0;
}


// --- Milestone 2: validate block codegen against the real interpreter ---
extern "C" uint32_t *ppc_jit_gpr(void);
extern "C" uint8_t *ppc_jit_cr(void);
extern "C" uint32_t *ppc_jit_xer(void);
extern "C" void ppc_jit_interp_one(uint32_t opcode);

static uint32_t rng_state = 1;
static uint32_t rng(void) { rng_state = rng_state * 1664525u + 1013904223u; return rng_state; }

// A random instruction from the covered integer set (half the 31-form ones get the record bit,
// so CR0 is exercised), encoded as the PowerPC would.
static uint32_t random_instr(void) {
  uint32_t d = rng() & 31, a = rng() & 31, b = rng() & 31, imm = rng() & 0xFFFF;
  uint32_t rc = rng() & 1; // record bit for 31-form ops
  switch (rng() % 14) {
    case 0:  return (14u << 26) | (d << 21) | (a << 16) | imm;                 // addi
    case 1:  return (15u << 26) | (d << 21) | (a << 16) | imm;                 // addis
    case 2:  return (24u << 26) | (d << 21) | (a << 16) | imm;                 // ori
    case 3:  return (25u << 26) | (d << 21) | (a << 16) | imm;                 // oris
    case 4:  return (26u << 26) | (d << 21) | (a << 16) | imm;                 // xori
    case 5:  return (27u << 26) | (d << 21) | (a << 16) | imm;                 // xoris
    case 6:  return (28u << 26) | (d << 21) | (a << 16) | imm;                 // andi. (records)
    case 7:  return (29u << 26) | (d << 21) | (a << 16) | imm;                 // andis.
    case 8:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (266u << 1) | rc; // add[.]
    case 9:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (40u << 1) | rc;  // subf[.]
    case 10: return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (444u << 1) | rc; // or[.]
    case 11: return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (28u << 1) | rc;  // and[.]
    default: return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (316u << 1) | rc; // xor[.]
  }
}

// Runs `blocks` random blocks of `blockLen` covered instructions through the JIT and through the
// interpreter from the same start state, and checks the 32 GPRs come out identical.
extern "C" int supermodel_jit_test_integer(int seed, int blocks, int blockLen)
{
  uint32_t *gpr = ppc_jit_gpr();
  uint8_t *cr = ppc_jit_cr();
  uint32_t *xer = ppc_jit_xer();
  jit::Regs regs = { (uint32_t)(uintptr_t)gpr, (uint32_t)(uintptr_t)cr, (uint32_t)(uintptr_t)xer };
  rng_state = seed ? (uint32_t)seed : 1;
  int fails = 0;

  for (int blk = 0; blk < blocks; blk++) {
    std::vector<uint32_t> instrs;
    for (int i = 0; i < blockLen; i++) instrs.push_back(random_instr());
    uint32_t initR[32], initXer; uint8_t initCr[8];
    for (int i = 0; i < 32; i++) { initR[i] = rng(); gpr[i] = initR[i]; }
    for (int i = 0; i < 8; i++) { initCr[i] = rng() & 0xF; cr[i] = initCr[i]; }
    initXer = rng(); *xer = initXer; // random SO/OV/CA so the CR0 SO bit is exercised

    jit::Code c;
    bool covered = true;
    for (uint32_t instr : instrs) if (!jit::compile_instr(c, regs, instr)) { covered = false; break; }
    if (!covered) continue;
    std::vector<uint8_t> mod = jit::module_void(c);
    sm_jit_run_module(mod.data(), (int)mod.size());

    uint32_t jitR[32], jitXer = *xer; uint8_t jitCr[8];
    for (int i = 0; i < 32; i++) jitR[i] = gpr[i];
    for (int i = 0; i < 8; i++) jitCr[i] = cr[i];

    for (int i = 0; i < 32; i++) gpr[i] = initR[i];
    for (int i = 0; i < 8; i++) cr[i] = initCr[i];
    *xer = initXer;
    for (uint32_t instr : instrs) ppc_jit_interp_one(instr);

    bool bad = (*xer != jitXer);
    for (int i = 0; i < 32 && !bad; i++) bad = gpr[i] != jitR[i];
    for (int i = 0; i < 8 && !bad; i++) bad = cr[i] != jitCr[i];
    if (bad) {
      if (fails < 5) {
        printf("[jit] MISMATCH block %d:", blk);
        for (int i = 0; i < 32; i++) if (gpr[i] != jitR[i]) printf(" r%d(jit=%08X interp=%08X)", i, jitR[i], gpr[i]);
        for (int i = 0; i < 8; i++) if (cr[i] != jitCr[i]) printf(" cr%d(jit=%X interp=%X)", i, jitCr[i], cr[i]);
        if (*xer != jitXer) printf(" xer(jit=%08X interp=%08X)", jitXer, *xer);
        printf("\n");
      }
      fails++;
    }
  }
  printf("[jit] integer block test: %d blocks x %d instrs -> %s\n", blocks, blockLen,
         fails == 0 ? "byte-exact with the interpreter PASS" : "FAIL");
  return fails == 0 ? 1 : 0;
}


// --- Try it out: how much faster is a compiled block than the interpreter? ---
// Builds one block of covered instructions, installs it in the table, and races calling it (via
// call_indirect, the real dispatch path) against running the same instructions through the
// interpreter. Returns the speedup (interp time / jit time).
extern "C" double supermodel_jit_benchmark(int blockLen, int iterations)
{
  jit::Regs regs = { (uint32_t)(uintptr_t)ppc_jit_gpr(), (uint32_t)(uintptr_t)ppc_jit_cr(),
                     (uint32_t)(uintptr_t)ppc_jit_xer() };
  rng_state = 20260105;
  std::vector<uint32_t> instrs;
  jit::Code c;
  for (int i = 0; i < blockLen; i++) {
    uint32_t instr = random_instr();
    if (!jit::compile_instr(c, regs, instr)) { i--; continue; } // only covered ops
    instrs.push_back(instr);
  }
  std::vector<uint8_t> mod = jit::module_void(c);
  int idx = sm_jit_install(mod.data(), (int)mod.size());
  if (idx < 0) { printf("[jit] benchmark: install failed\n"); return -1; }
  BlockFn block = (BlockFn)(intptr_t)idx;

  for (int i = 0; i < 2000; i++) block(); // warm up

#ifdef __EMSCRIPTEN__
  double t0 = emscripten_get_now();
  for (int i = 0; i < iterations; i++) block();
  double jitMs = emscripten_get_now() - t0;

  t0 = emscripten_get_now();
  for (int i = 0; i < iterations; i++) for (uint32_t instr : instrs) ppc_jit_interp_one(instr);
  double interpMs = emscripten_get_now() - t0;
#else
  double jitMs = 1, interpMs = 1;
#endif

  double speedup = interpMs / jitMs;
  printf("[jit] benchmark: %d-instruction block x %d iterations -> interpreter %.1f ms, "
         "recompiled %.1f ms, %.2fx faster\n", (int)instrs.size(), iterations, interpMs, jitMs, speedup);
  return speedup;
}
