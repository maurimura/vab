// PowerPC basic-block -> WebAssembly codegen (recompiler). Registers are kept in WASM locals for
// the duration of a block: a register is loaded from the ppc struct the first time it is read,
// operated on in a local, and written back to the struct at block exit if it changed. This drops
// the per-instruction memory traffic the interpreter pays for every operand. CR, XER and guest
// memory still go through the ppc struct / the Bus handlers. An uncovered instruction ends the
// block and the interpreter handles it (the hybrid model). Validated byte-exact against the
// interpreter on the real game.
#pragma once
#include "wasm_emit.h"
#include <cstdint>

namespace jit {

struct Regs {
  uint32_t gpr, cr, xer;                                   // addresses of ppc.r[0], ppc.cr[0], ppc.xer
  uint32_t read32, read16, read8, write32, write16, write8; // table indices of the Bus handlers
};

// Maps guest registers to WASM locals, lazily: a register gets a local the first time it is used,
// and is marked for loading at entry only if its first use is a read.
struct RegMap {
  int localOf[32];
  uint32_t loadMask = 0, writeMask = 0;
  int nloc = 0;
  RegMap() { for (int i = 0; i < 32; i++) localOf[i] = -1; }
  int ensure(uint32_t n, bool isWrite) {
    if (localOf[n] < 0) { localOf[n] = nloc++; if (!isWrite) loadMask |= (1u << n); }
    if (isWrite) writeMask |= (1u << n);
    return localOf[n];
  }
};

inline uint32_t ppc_mask(uint32_t mb, uint32_t me) {
  uint32_t m = 0;
  for (uint32_t i = mb;; i = (i + 1) & 31) { m |= (0x80000000u >> i); if (i == me) break; }
  return m;
}

// Emit the CR0 field for a record-form result in GPR `dest` (held in a local): matches SET_CR0.
inline void emit_cr0(Code &c, const Regs &r, RegMap &rm, uint32_t dest) {
  auto res = [&] { c.local_get(rm.ensure(dest, false)); };
  c.i32_const((int32_t)r.cr);
  c.i32_const(8);
  c.i32_const(4); c.i32_const(2);
  res(); c.i32_const(0); c.op(OP_I32_GT_S); c.op(OP_SELECT);
  res(); c.i32_const(0); c.op(OP_I32_LT_S); c.op(OP_SELECT);
  c.i32_const((int32_t)r.xer); c.i32_load(0); c.i32_const(31); c.op(OP_I32_SHR_U); c.i32_const(1); c.op(OP_I32_AND);
  c.op(OP_I32_OR);
  c.i32_store8(0);
}

// Emit a comparison into condition field `bf`.
inline void emit_cmp(Code &c, const Regs &r, RegMap &rm, uint32_t bf, bool sgned, uint32_t aReg, bool bIsImm, int32_t bVal) {
  auto a = [&] { c.local_get(rm.ensure(aReg, false)); };
  auto b = [&] { if (bIsImm) c.i32_const(bVal); else c.local_get(rm.ensure((uint32_t)bVal, false)); };
  c.i32_const((int32_t)(r.cr + bf));
  c.i32_const(8);
  c.i32_const(4); c.i32_const(2);
  a(); b(); c.op(sgned ? OP_I32_GT_S : OP_I32_GT_U); c.op(OP_SELECT);
  a(); b(); c.op(sgned ? OP_I32_LT_S : OP_I32_LT_U); c.op(OP_SELECT);
  c.i32_const((int32_t)r.xer); c.i32_load(0); c.i32_const(31); c.op(OP_I32_SHR_U); c.i32_const(1); c.op(OP_I32_AND);
  c.op(OP_I32_OR);
  c.i32_store8(0);
}

// Emit one instruction into `c`, using register locals from `rm`. False if not covered (no bytes
// emitted, so the block ends cleanly).
inline bool compile_one(Code &c, const Regs &r, RegMap &rm, uint32_t instr) {
  const uint32_t op = instr >> 26;
  const uint32_t d = (instr >> 21) & 31;
  const uint32_t a = (instr >> 16) & 31;
  const uint32_t b = (instr >> 11) & 31;
  const int32_t simm = (int16_t)(instr & 0xFFFF);
  const uint32_t uimm = instr & 0xFFFF;

  auto rd = [&](uint32_t n) { c.local_get(rm.ensure(n, false)); };  // read reg n
  auto wr = [&](uint32_t n) { c.local_set(rm.ensure(n, true)); };   // write reg n (value on stack)
  auto inv = [&] { c.i32_const(-1); c.op(OP_I32_XOR); };            // bitwise NOT of top of stack

  switch (op) {
    case 14: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); wr(d); return true; // addi
    case 15: { int32_t hi = (int32_t)(uimm << 16); if (a) { rd(a); c.i32_const(hi); c.op(OP_I32_ADD); } else c.i32_const(hi); wr(d); return true; } // addis
    case 24: rd(d); c.i32_const((int32_t)uimm); c.op(OP_I32_OR);  wr(a); return true; // ori
    case 25: rd(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_OR);  wr(a); return true; // oris
    case 26: rd(d); c.i32_const((int32_t)uimm); c.op(OP_I32_XOR); wr(a); return true; // xori
    case 27: rd(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_XOR); wr(a); return true; // xoris
    case 28: rd(d); c.i32_const((int32_t)uimm); c.op(OP_I32_AND); wr(a); emit_cr0(c, r, rm, a); return true; // andi.
    case 29: rd(d); c.i32_const((int32_t)(uimm << 16)); c.op(OP_I32_AND); wr(a); emit_cr0(c, r, rm, a); return true; // andis.
    case 10: emit_cmp(c, r, rm, ((instr >> 23) & 7), false, a, true, (int32_t)uimm); return true; // cmpli
    case 11: emit_cmp(c, r, rm, ((instr >> 23) & 7), true,  a, true, simm);          return true; // cmpi
    case 7:  rd(a); c.i32_const(simm); c.op(OP_I32_MUL); wr(d); return true; // mulli
    case 21: { uint32_t sh = (instr >> 11) & 31, mb = (instr >> 6) & 31, me = (instr >> 1) & 31; // rlwinm
      rd(d); c.i32_const((int32_t)sh); c.op(OP_I32_ROTL); c.i32_const((int32_t)ppc_mask(mb, me)); c.op(OP_I32_AND); wr(a);
      if (instr & 1) emit_cr0(c, r, rm, a); return true; }

    // Load/store: EA = SIMM + (rA==0 ? 0 : r[rA]); through the Bus handlers.
    case 32: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); c.i32_const((int32_t)r.read32); c.call_indirect(TYPE_READ); wr(d); return true; // lwz
    case 34: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); c.i32_const((int32_t)r.read8);  c.call_indirect(TYPE_READ); wr(d); return true; // lbz
    case 40: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); c.i32_const((int32_t)r.read16); c.call_indirect(TYPE_READ); wr(d); return true; // lhz
    case 42: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); c.i32_const((int32_t)r.read16); c.call_indirect(TYPE_READ);
             c.i32_const(16); c.op(OP_I32_SHL); c.i32_const(16); c.op(OP_I32_SHR_S); wr(d); return true; // lha
    case 36: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); rd(d); c.i32_const((int32_t)r.write32); c.call_indirect(TYPE_WRITE); return true; // stw
    case 38: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); rd(d); c.i32_const((int32_t)r.write8);  c.call_indirect(TYPE_WRITE); return true; // stb
    case 44: if (a) { rd(a); c.i32_const(simm); c.op(OP_I32_ADD); } else c.i32_const(simm); rd(d); c.i32_const((int32_t)r.write16); c.call_indirect(TYPE_WRITE); return true; // sth

    case 31: {
      const uint32_t xo = (instr >> 1) & 0x3FF;
      uint32_t dest;
      switch (xo) {
        case 0:   emit_cmp(c, r, rm, ((instr >> 23) & 7), true,  a, false, (int32_t)b); return true; // cmp
        case 32:  emit_cmp(c, r, rm, ((instr >> 23) & 7), false, a, false, (int32_t)b); return true; // cmpl
        case 266: dest = d; rd(a); rd(b); c.op(OP_I32_ADD); wr(d); break; // add
        case 40:  dest = d; rd(b); rd(a); c.op(OP_I32_SUB); wr(d); break; // subf = rB-rA
        case 235: dest = d; rd(a); rd(b); c.op(OP_I32_MUL); wr(d); break; // mullw
        case 104: dest = d; c.i32_const(0); rd(a); c.op(OP_I32_SUB); wr(d); break; // neg
        case 444: dest = a; rd(d); rd(b); c.op(OP_I32_OR);  wr(a); break; // or
        case 28:  dest = a; rd(d); rd(b); c.op(OP_I32_AND); wr(a); break; // and
        case 316: dest = a; rd(d); rd(b); c.op(OP_I32_XOR); wr(a); break; // xor
        case 124: dest = a; rd(d); rd(b); c.op(OP_I32_OR);  inv(); wr(a); break; // nor
        case 476: dest = a; rd(d); rd(b); c.op(OP_I32_AND); inv(); wr(a); break; // nand
        case 284: dest = a; rd(d); rd(b); c.op(OP_I32_XOR); inv(); wr(a); break; // eqv
        case 60:  dest = a; rd(d); rd(b); inv(); c.op(OP_I32_AND); wr(a); break; // andc = rS & ~rB
        case 412: dest = a; rd(d); rd(b); inv(); c.op(OP_I32_OR);  wr(a); break; // orc  = rS | ~rB
        case 954: dest = a; rd(d); c.i32_const(24); c.op(OP_I32_SHL); c.i32_const(24); c.op(OP_I32_SHR_S); wr(a); break; // extsb
        case 922: dest = a; rd(d); c.i32_const(16); c.op(OP_I32_SHL); c.i32_const(16); c.op(OP_I32_SHR_S); wr(a); break; // extsh
        case 24:  dest = a; rd(d); rd(b); c.i32_const(31); c.op(OP_I32_AND); c.op(OP_I32_SHL);
                  c.i32_const(0); rd(b); c.i32_const(0x20); c.op(OP_I32_AND); c.op(OP_I32_EQZ); c.op(OP_SELECT); wr(a); break; // slw
        case 536: dest = a; rd(d); rd(b); c.i32_const(31); c.op(OP_I32_AND); c.op(OP_I32_SHR_U);
                  c.i32_const(0); rd(b); c.i32_const(0x20); c.op(OP_I32_AND); c.op(OP_I32_EQZ); c.op(OP_SELECT); wr(a); break; // srw
        default: return false;
      }
      if (instr & 1) emit_cr0(c, r, rm, dest);
      return true;
    }
    default: return false;
  }
}

// Compile up to maxLen instructions into one block (stops at the first uncovered one). Loads the
// registers it reads at entry and stores the ones it writes at exit. countOut = how many compiled.
inline Code compile_block(const uint32_t *instrs, int maxLen, const Regs &r, int &countOut) {
  RegMap rm;
  Code body;
  int count = 0;
  for (int i = 0; i < maxLen; i++) { if (!compile_one(body, r, rm, instrs[i])) break; count++; }

  Code full;
  full.numLocals = rm.nloc;
  for (uint32_t n = 0; n < 32; n++)
    if (rm.loadMask & (1u << n)) { full.i32_const((int32_t)(r.gpr + 4 * n)); full.i32_load(0); full.local_set(rm.localOf[n]); }
  full.bytes.insert(full.bytes.end(), body.bytes.begin(), body.bytes.end());
  for (uint32_t n = 0; n < 32; n++)
    if (rm.writeMask & (1u << n)) { full.i32_const((int32_t)(r.gpr + 4 * n)); full.local_get(rm.localOf[n]); full.i32_store(0); }

  countOut = count;
  return full;
}

} // namespace jit
