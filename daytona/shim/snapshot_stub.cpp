// Stand-in for the runtime's save states until patches/0001-snapshot.patch lands (core.mk builds
// this with DAYTONA_NO_SNAPSHOT when src/runtime/snapshot.cpp is not there): no states, and the
// work RAM copied out through the bus, so the frontend's desync hashes still see the game.
#include "snapshot_api.h"

#ifdef DAYTONA_NO_SNAPSHOT
namespace rt {

std::vector<uint8_t> save_state(GameLoop &) { return {}; }
bool load_state(GameLoop &, const uint8_t *, size_t) { return false; }
size_t state_size_bound(const GameLoop &) { return 0; }

uint8_t *main_ram(M2Board &board, size_t *size) {
    // The i960's work RAM (0x00500000, 1 MB), copied on every call: the frontend asks every 120
    // frames, and only for cabinet 0.
    constexpr uint32_t kBase = 0x00500000, kSize = 0x100000;
    static std::vector<uint8_t> copy(kSize);
    for (uint32_t i = 0; i < kSize; i++) copy[i] = board.read_byte(kBase + i);
    if (size) *size = kSize;
    return copy.data();
}

} // namespace rt
#endif
