// PowerPC basic-block -> WebAssembly codegen (recompiler milestone 2+). Each covered instruction
// emits WASM that reads and writes the guest registers in the shared `ppc` struct, at linear
// address `base` for the GPR array. Registers go through memory for now (correct, not yet fast);
// keeping the live ones in WASM locals is the milestone-8 performance pass. An uncovered
// instruction returns false and the caller falls back to the interpreter (the hybrid model).
#pragma once
#include "wasm_emit.h"
#include <cstdint>

namespace jit {

// Addresses of the guest register file in the shared memory, passed to the compiler.
struct Regs {
  uint32_t gpr;   // &ppc.r[0]
  uint32_t cr;    // &ppc.cr[0]
  uint32_t xer;   // &ppc.xer
};

// Emit CR0 for a record-form result in GPR `dest`, matching SET_CR0 exactly:
// cr[0] = (res<0 ? 8 : res>0 ? 4 : 2) | (XER_SO ? 1 : 0).
inline void emit_cr0(Code &c, const Regs &r, uint32_t dest) {
  auto res = [&] { c.i32_const((int32_t)(r.gpr + 4u * dest)); c.i32_load(0); };
  c.i32_const((int32_t)r.cr);                 // address of cr[0] (byte store)
  // lt ? 8 : (gt ? 4 : 2)
  c.i32_const(8);
  c.i32_const(4); c.i32_const(2);
  res(); c.i32_const(0); c.op(OP_I32_GT_S);   // res > 0 (signed)
  c.op(OP_SELECT);                            // gt ? 4 : 2
  res(); c.i32_const(0); c.op(OP_I32_LT_S);   // res < 0 (signed)
  c.op(OP_SELECT);                            // lt ? 8 : (gt ? 4 : 2)
  // | SO bit (XER bit 31)
  c.i32_const((int32_t)r.xer); c.i32_load(0);
  c.i32_const(31); c.op(OP_I32_SHR_U); c.i32_const(1); c.op(OP_I32_AND);
  c.op(OP_I32_OR);
  c.i32_store8(0);
}

// Emit code for one instruction; false if not covered yet.
inline bool compile_instr(Code &c, const Regs &r, uint32_t instr) {
  const uint32_t base = r.gpr;
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
    case 28: addr(a); loadr(d); c.i32_const((int32_t)uimm); c.op(OP_I32_AND); c.i32_store(0); emit_cr0(c, r, a); return true;         // andi.  (always records)
    case 29: addr(a); loadr(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_AND); c.i32_store(0); emit_cr0(c, r, a); return true; // andis.
    case 31: {
      const uint32_t xo = (instr >> 1) & 0x3FF;
      uint32_t dest;
      switch (xo) {
        case 266: dest = d; addr(d); loadr(a); loadr(b); c.op(OP_I32_ADD); c.i32_store(0); break; // add
        case 40:  dest = d; addr(d); loadr(b); loadr(a); c.op(OP_I32_SUB); c.i32_store(0); break; // subf = rB-rA
        case 444: dest = a; addr(a); loadr(d); loadr(b); c.op(OP_I32_OR);  c.i32_store(0); break; // or
        case 28:  dest = a; addr(a); loadr(d); loadr(b); c.op(OP_I32_AND); c.i32_store(0); break; // and
        case 316: dest = a; addr(a); loadr(d); loadr(b); c.op(OP_I32_XOR); c.i32_store(0); break; // xor
        default: return false;
      }
      if (instr & 1) emit_cr0(c, r, dest); // Rc: record form sets CR0
      return true;
    }
    default: return false;
  }
}

} // namespace jit
