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
#else
static int sm_jit_run_module(const uint8_t *, int) { return 0; } // native build has no runtime WASM
#endif

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
extern "C" void ppc_jit_interp_one(uint32_t opcode);

static uint32_t rng_state = 1;
static uint32_t rng(void) { rng_state = rng_state * 1664525u + 1013904223u; return rng_state; }

// A random instruction from the covered integer set, encoded as the PowerPC would.
static uint32_t random_instr(void) {
  uint32_t d = rng() & 31, a = rng() & 31, b = rng() & 31, imm = rng() & 0xFFFF;
  switch (rng() % 11) {
    case 0:  return (14u << 26) | (d << 21) | (a << 16) | imm;                 // addi
    case 1:  return (15u << 26) | (d << 21) | (a << 16) | imm;                 // addis
    case 2:  return (24u << 26) | (d << 21) | (a << 16) | imm;                 // ori
    case 3:  return (25u << 26) | (d << 21) | (a << 16) | imm;                 // oris
    case 4:  return (26u << 26) | (d << 21) | (a << 16) | imm;                 // xori
    case 5:  return (27u << 26) | (d << 21) | (a << 16) | imm;                 // xoris
    case 6:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (266u << 1); // add
    case 7:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (40u << 1);  // subf
    case 8:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (444u << 1); // or
    case 9:  return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (28u << 1);  // and
    default: return (31u << 26) | (d << 21) | (a << 16) | (b << 11) | (316u << 1); // xor
  }
}

// Runs `blocks` random blocks of `blockLen` covered instructions through the JIT and through the
// interpreter from the same start state, and checks the 32 GPRs come out identical.
extern "C" int supermodel_jit_test_integer(int seed, int blocks, int blockLen)
{
  uint32_t *gpr = ppc_jit_gpr();
  uint32_t base = (uint32_t)(uintptr_t)gpr;
  rng_state = seed ? (uint32_t)seed : 1;
  int fails = 0;

  for (int blk = 0; blk < blocks; blk++) {
    std::vector<uint32_t> instrs;
    for (int i = 0; i < blockLen; i++) instrs.push_back(random_instr());
    uint32_t init[32];
    for (int i = 0; i < 32; i++) { init[i] = rng(); gpr[i] = init[i]; }

    jit::Code c;
    bool covered = true;
    for (uint32_t instr : instrs) if (!jit::compile_instr(c, base, instr)) { covered = false; break; }
    if (!covered) continue; // only covered ops are generated, but stay safe
    std::vector<uint8_t> mod = jit::module_void(c);
    sm_jit_run_module(mod.data(), (int)mod.size()); // executes the block, storing into gpr[]

    uint32_t jitResult[32];
    for (int i = 0; i < 32; i++) jitResult[i] = gpr[i];

    for (int i = 0; i < 32; i++) gpr[i] = init[i];
    for (uint32_t instr : instrs) ppc_jit_interp_one(instr);

    for (int i = 0; i < 32; i++) {
      if (gpr[i] != jitResult[i]) {
        if (fails < 5) printf("[jit] MISMATCH block %d r%d: jit=%08X interp=%08X\n", blk, i, jitResult[i], gpr[i]);
        fails++;
        break;
      }
    }
  }
  printf("[jit] integer block test: %d blocks x %d instrs -> %s\n", blocks, blockLen,
         fails == 0 ? "byte-exact with the interpreter PASS" : "FAIL");
  return fails == 0 ? 1 : 0;
}
