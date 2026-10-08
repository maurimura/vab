// Stand-in for the generated game code (the i960 program, the TGP program, the sound 68000
// program), which build.sh makes from the user's ROM set: what the runtime calls into, with no
// code behind it. It links, so the whole toolchain is proven without a ROM; a game started on it
// stops at its first instruction ("no recompiled code at ...", caught at the libretro boundary).
#include "runtime/gen_support.h"
#include "runtime/snd_gen_support.h"
#include "runtime/tgp.h"

// The shim asks (a weak reference) whether the game code is this stub.
extern "C" {
int daytona_gen_is_stub = 1;
}

namespace gen {
bool has_code(uint32_t) { return false; }
void run(Env &) {}
uint64_t native_instructions() { return 0; }
} // namespace gen

namespace sndgen {
bool has_code(uint32_t) { return false; }
void run(Env &) {}
uint64_t native_instructions() { return 0; }
} // namespace sndgen

namespace rt::tgpgen {
void run(Tgp &, uint64_t) {}
const uint32_t program_crc32 = 0;
const uint32_t program_words = 0;
} // namespace rt::tgpgen
