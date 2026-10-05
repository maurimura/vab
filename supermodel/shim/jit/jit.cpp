// Milestone 1 of the PowerPC recompiler (supermodel/RECOMPILER.md): prove that the emulator can
// generate a WebAssembly function at runtime and have it share the emulator's own linear memory.
// Everything else in the recompiler is built on this mechanism.
#include "wasm_emit.h"
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
