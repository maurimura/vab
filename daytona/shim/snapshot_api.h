// The recomp runtime's save states, as patches/0001-snapshot.patch adds them
// (src/runtime/snapshot.h). Until that patch is in, DAYTONA_NO_SNAPSHOT (core.mk) declares the
// same interface here and shim/snapshot_stub.cpp stands in for it.
#pragma once

#include "runtime/game_loop.h"

#ifndef DAYTONA_NO_SNAPSHOT
#include "runtime/snapshot.h"
#else
#include <cstddef>
#include <cstdint>
#include <vector>

namespace rt {
// The whole machine (board, CPUs, TGP, sound board, comm board) as bytes.
std::vector<uint8_t> save_state(GameLoop &game);
// Back onto a machine of the same build and ROM set (a freshly made one included).
bool load_state(GameLoop &game, const uint8_t *data, size_t size);
// At least the size save_state makes, for a fixed-size libretro state.
size_t state_size_bound(const GameLoop &game);
// The i960's main RAM, for the frontend's desync hashes.
uint8_t *main_ram(M2Board &board, size_t *size);
} // namespace rt
#endif
