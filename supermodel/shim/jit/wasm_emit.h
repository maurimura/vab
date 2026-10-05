// A minimal WebAssembly module and function emitter: enough to build, at runtime, one function
// that implements a PowerPC basic block. This is the foundation of the recompiler (see
// supermodel/RECOMPILER.md). It encodes a single-function module that imports the emulator's
// linear memory, so the generated code reads and writes the same memory as the interpreter
// (the `ppc` register struct, guest RAM), which is how a runtime-compiled module shares state.
#pragma once
#include <cstdint>
#include <vector>

namespace jit {

// WebAssembly value types and the opcodes the codegen needs (growing as instruction coverage does).
enum : uint8_t {
  WASM_I32 = 0x7F,
  WASM_I64 = 0x7E,
  OP_END = 0x0B,
  OP_LOCAL_GET = 0x20,
  OP_LOCAL_SET = 0x21,
  OP_LOCAL_TEE = 0x22,
  OP_I32_LOAD = 0x28,
  OP_I32_STORE = 0x36,
  OP_I32_CONST = 0x41,
  OP_I32_ADD = 0x6A,
  OP_I32_SUB = 0x6B,
  OP_I32_MUL = 0x6C,
  OP_I32_AND = 0x71,
  OP_I32_OR = 0x72,
  OP_I32_XOR = 0x73,
  OP_I32_SHL = 0x74,
  OP_I32_SHR_U = 0x76,
  OP_CALL = 0x10,
};

// Builds the body of one function (the instruction stream between locals and OP_END).
struct Code {
  std::vector<uint8_t> bytes;

  void u8(uint8_t b) { bytes.push_back(b); }
  // LEB128, unsigned and signed, as the WebAssembly binary format uses throughout.
  void uleb(uint32_t v) {
    do { uint8_t b = v & 0x7F; v >>= 7; if (v) b |= 0x80; bytes.push_back(b); } while (v);
  }
  void sleb(int32_t v) {
    bool more = true;
    while (more) {
      uint8_t b = v & 0x7F; v >>= 7;
      if ((v == 0 && !(b & 0x40)) || (v == -1 && (b & 0x40))) more = false; else b |= 0x80;
      bytes.push_back(b);
    }
  }
  void op(uint8_t o) { u8(o); }
  void i32_const(int32_t v) { u8(OP_I32_CONST); sleb(v); }
  void local_get(uint32_t i) { u8(OP_LOCAL_GET); uleb(i); }
  void local_set(uint32_t i) { u8(OP_LOCAL_SET); uleb(i); }
  // align is the power-of-two alignment hint; guest words are 4-byte aligned in our layout.
  void i32_load(uint32_t offset, uint32_t align = 2) { u8(OP_I32_LOAD); uleb(align); uleb(offset); }
  void i32_store(uint32_t offset, uint32_t align = 2) { u8(OP_I32_STORE); uleb(align); uleb(offset); }
  void end() { u8(OP_END); }
};

// A section's payload, length-prefixed when added to the module.
inline void put_section(std::vector<uint8_t> &out, uint8_t id, const std::vector<uint8_t> &payload) {
  out.push_back(id);
  uint32_t v = (uint32_t)payload.size();
  do { uint8_t b = v & 0x7F; v >>= 7; if (v) b |= 0x80; out.push_back(b); } while (v);
  out.insert(out.end(), payload.begin(), payload.end());
}

inline void put_uleb(std::vector<uint8_t> &out, uint32_t v) {
  do { uint8_t b = v & 0x7F; v >>= 7; if (v) b |= 0x80; out.push_back(b); } while (v);
}

// Wraps one function body into a complete module that imports `env.memory` and exports the
// function as "run" with signature () -> i32. (Milestone 1: a single no-arg block returning a
// value; later milestones add params, more functions, and imported call-outs for MMIO.)
inline std::vector<uint8_t> module_returning_i32(const Code &code) {
  std::vector<uint8_t> m = { 0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00 }; // magic + version

  // Type section: one type, () -> (i32)
  std::vector<uint8_t> types;
  put_uleb(types, 1);        // one type
  types.push_back(0x60);     // func
  put_uleb(types, 0);        // no params
  put_uleb(types, 1);        // one result
  types.push_back(WASM_I32);
  put_section(m, 1, types);

  // Import section: (import "env" "memory" (memory 0 ...)) -> memory index 0
  std::vector<uint8_t> imports;
  put_uleb(imports, 1);      // one import
  const char *mod = "env", *nm = "memory";
  put_uleb(imports, 3); imports.insert(imports.end(), mod, mod + 3);
  put_uleb(imports, 6); imports.insert(imports.end(), nm, nm + 6);
  imports.push_back(0x02);   // import kind: memory
  imports.push_back(0x00);   // limits: min only
  put_uleb(imports, 1);      // min 1 page (shared memory provides the rest)
  put_section(m, 2, imports);

  // Function section: one function, type 0
  std::vector<uint8_t> funcs; put_uleb(funcs, 1); put_uleb(funcs, 0);
  put_section(m, 3, funcs);

  // Export section: export "run" = func 0
  std::vector<uint8_t> exports;
  put_uleb(exports, 1);
  const char *rn = "run";
  put_uleb(exports, 3); exports.insert(exports.end(), rn, rn + 3);
  exports.push_back(0x00);   // export kind: func
  put_uleb(exports, 0);      // func index 0
  put_section(m, 7, exports);

  // Code section: one body, no locals, then the code, then END.
  std::vector<uint8_t> bodies;
  put_uleb(bodies, 1);       // one body
  std::vector<uint8_t> body;
  put_uleb(body, 0);         // no local declarations
  body.insert(body.end(), code.bytes.begin(), code.bytes.end());
  body.push_back(OP_END);
  put_uleb(bodies, (uint32_t)body.size());
  bodies.insert(bodies.end(), body.begin(), body.end());
  put_section(m, 10, bodies);

  return m;
}

// Wraps a function body into a module exporting a () -> () function "run" that imports env.memory.
// A compiled block stores its results into the register struct in that shared memory.
inline std::vector<uint8_t> module_void(const Code &code) {
  std::vector<uint8_t> m = { 0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00 };

  std::vector<uint8_t> types;
  put_uleb(types, 1); types.push_back(0x60); put_uleb(types, 0); put_uleb(types, 0); // () -> ()
  put_section(m, 1, types);

  std::vector<uint8_t> imports;
  put_uleb(imports, 1);
  const char *mod = "env", *nm = "memory";
  put_uleb(imports, 3); imports.insert(imports.end(), mod, mod + 3);
  put_uleb(imports, 6); imports.insert(imports.end(), nm, nm + 6);
  imports.push_back(0x02); imports.push_back(0x00); put_uleb(imports, 1);
  put_section(m, 2, imports);

  std::vector<uint8_t> funcs; put_uleb(funcs, 1); put_uleb(funcs, 0);
  put_section(m, 3, funcs);

  std::vector<uint8_t> exports;
  put_uleb(exports, 1);
  const char *rn = "run";
  put_uleb(exports, 3); exports.insert(exports.end(), rn, rn + 3);
  exports.push_back(0x00); put_uleb(exports, 0);
  put_section(m, 7, exports);

  std::vector<uint8_t> bodies;
  put_uleb(bodies, 1);
  std::vector<uint8_t> body;
  put_uleb(body, 0); // no locals
  body.insert(body.end(), code.bytes.begin(), code.bytes.end());
  body.push_back(OP_END);
  put_uleb(bodies, (uint32_t)body.size());
  bodies.insert(bodies.end(), body.begin(), body.end());
  put_section(m, 10, bodies);
  return m;
}

} // namespace jit
