// PowerPC basic-block -> WebAssembly codegen (recompiler milestone 2+). Each covered instruction
// emits WASM that reads and writes the guest registers in the shared `ppc` struct, at linear
// address `base` for the GPR array. Registers go through memory for now (correct, not yet fast);
// keeping the live ones in WASM locals is the milestone-8 performance pass. An uncovered
// instruction returns false and the caller falls back to the interpreter (the hybrid model).
#pragma once
#include "wasm_emit.h"
#include <cstdint>

namespace jit {

// Emit code for one instruction; false if not covered yet.
inline bool compile_instr(Code &c, uint32_t base, uint32_t instr) {
  const uint32_t op = instr >> 26;
  const uint32_t d = (instr >> 21) & 31;   // rD or rS
  const uint32_t a = (instr >> 16) & 31;   // rA
  const uint32_t b = (instr >> 11) & 31;   // rB
  const int32_t simm = (int16_t)(instr & 0xFFFF);
  const uint32_t uimm = instr & 0xFFFF;

  auto loadr = [&](uint32_t n) { c.i32_const((int32_t)(base + 4u * n)); c.i32_load(0); };
  auto addr = [&](uint32_t n) { c.i32_const((int32_t)(base + 4u * n)); };

  switch (op) {
    case 14: // addi rD,rA,SIMM  (rA==0 -> literal)
      addr(d); if (a == 0) c.i32_const(simm); else { loadr(a); c.i32_const(simm); c.op(OP_I32_ADD); } c.i32_store(0);
      return true;
    case 15: { // addis rD,rA,SIMM<<16
      int32_t hi = (int32_t)(uimm << 16);
      addr(d); if (a == 0) c.i32_const(hi); else { loadr(a); c.i32_const(hi); c.op(OP_I32_ADD); } c.i32_store(0);
      return true;
    }
    case 24: addr(a); loadr(d); c.i32_const((int32_t)uimm); c.op(OP_I32_OR); c.i32_store(0); return true;         // ori
    case 25: addr(a); loadr(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_OR); c.i32_store(0); return true; // oris
    case 26: addr(a); loadr(d); c.i32_const((int32_t)uimm); c.op(OP_I32_XOR); c.i32_store(0); return true;        // xori
    case 27: addr(a); loadr(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_XOR); c.i32_store(0); return true;// xoris
    case 31: {
      const uint32_t xo = (instr >> 1) & 0x3FF;
      if (instr & 1) return false; // Rc: condition-register side effect, milestone 4
      switch (xo) {
        case 266: addr(d); loadr(a); loadr(b); c.op(OP_I32_ADD); c.i32_store(0); return true; // add  rD,rA,rB
        case 40:  addr(d); loadr(b); loadr(a); c.op(OP_I32_SUB); c.i32_store(0); return true; // subf rD,rA,rB = rB-rA
        case 444: addr(a); loadr(d); loadr(b); c.op(OP_I32_OR);  c.i32_store(0); return true; // or   rA,rS,rB
        case 28:  addr(a); loadr(d); loadr(b); c.op(OP_I32_AND); c.i32_store(0); return true; // and  rA,rS,rB
        case 316: addr(a); loadr(d); loadr(b); c.op(OP_I32_XOR); c.i32_store(0); return true; // xor  rA,rS,rB
        default: return false;
      }
    }
    default: return false;
  }
}

} // namespace jit
