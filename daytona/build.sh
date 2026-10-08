#!/usr/bin/env bash
# Builds Daytona USA (Sega Model 2, MAME set `daytona`, Revision A) as a standalone Emscripten
# module that speaks the libretro API, so web/emulator/libretro.js and worker.js drive it like
# the FBNeo and Supermodel cores. The "emulator" is daytona-arcade-recomp: the game's i960, TGP
# and sound 68000 programs statically recompiled to C++ from the user's ROM set, on a native
# Model 2 board (geometrizer, software rasterizer, tilemaps, YM3438, MultiPCM, comm board).
# One libretro machine is one or two linked cabinets (shim/libretro.cpp).
#
#   daytona/dist/daytona.mjs + .wasm         for the page or worker (CPU framebuffer, no WebGL)
#   daytona/dist/headless/daytona.mjs        for Node (bench.mjs, check.mjs, make-nvram.mjs)
#   daytona/.cache/native/bench              native build of the same code, the baseline
#
#   ./daytona/build.sh [web] [headless] [native]    (all three when none given)
#
# The game code is generated from the ROM set at build time: with $ROMS/daytona.zip present
# (ROMS defaults to ~/Downloads) it is imported and recompiled here, natively, by the recomp's own
# tools; without it (or with DAYTONA_GEN=stub) the core links a stub instead, which proves the
# toolchain but cannot run the game (loading fails cleanly, or the first frame stops with
# "no recompiled code"). The generated code and everything in .cache/ and dist/ is derived from
# the ROM when it is real: never commit or publish it.
#
# Steps: the pinned emsdk (emulator/emsdk.sh), the pinned recomp checkout with our patches and its
# pinned SoftFloat and ymfm, the recomp's tools built natively (CMake), the generated code (or the
# stub), then core.mk compiles and links.
set -euo pipefail

RECOMP_REPO="https://github.com/alphanu1/daytona-arcade-recomp.git"
RECOMP_COMMIT="1877da9cc1aa5ad476e7868234d8fe1d0961dba6" # 2026-10-06
# Pinned as the recomp's scripts/setup.py (and scripts/fetch_softfloat.sh) pin them.
SOFTFLOAT_REPO="https://github.com/ucb-bar/berkeley-softfloat-3.git"
SOFTFLOAT_COMMIT="a0c6494cdc11865811dec815d5c0049fba9d82a8"
YMFM_REPO="https://github.com/aaronsgiles/ymfm.git"
YMFM_COMMIT="81aec25ccbb98f4873a255f7551ac4dadac59b4a"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HERE="$ROOT/daytona"
CACHE="$HERE/.cache"
SRC="$CACHE/recomp"
TOOLS="$CACHE/tools"
GEN="$CACHE/gen"
IMAGES="$CACHE/rom_cache/daytona"
ROMS="${ROMS:-$HOME/Downloads}"
ROM="$ROMS/daytona.zip"
EMSDK_DIR="${EMSDK_DIR:-$ROOT/emulator/.cache/emsdk}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"
NATIVE_CC="${NATIVE_CC:-clang}"
NATIVE_CXX="${NATIVE_CXX:-clang++}"
TARGETS=("$@")
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(web headless native)

mkdir -p "$CACHE"

# 1. emsdk (only the wasm targets need it).
for target in "${TARGETS[@]}"; do
  if [ "$target" != native ]; then
    [ -d "$EMSDK_DIR" ] || "$ROOT/emulator/emsdk.sh"
    # shellcheck disable=SC1091
    source "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1
    break
  fi
done

# 2. The recomp at the pinned commit with our patches (each applied once), and its dependencies
#    in its extern/ (git-ignored there), as its setup would fetch them.
fetch() { # dir repo commit
  local dir=$1 repo=$2 commit=$3
  if [ ! -d "$dir/.git" ]; then
    git init -q "$dir"
    git -C "$dir" remote add origin "$repo"
  fi
  if [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null)" != "$commit" ]; then
    git -C "$dir" fetch -q --depth 1 origin "$commit"
    git -C "$dir" checkout -q --force FETCH_HEAD
  fi
}
fetch "$SRC" "$RECOMP_REPO" "$RECOMP_COMMIT"
fetch "$SRC/extern/softfloat" "$SOFTFLOAT_REPO" "$SOFTFLOAT_COMMIT"
fetch "$SRC/extern/ymfm" "$YMFM_REPO" "$YMFM_COMMIT"
shopt -s nullglob
for patch in "$HERE"/patches/*.patch; do
  if ! git -C "$SRC" apply --reverse --check "$patch" 2>/dev/null; then
    git -C "$SRC" apply "$patch"
  fi
done
shopt -u nullglob

# SoftFloat's file list, as the recomp's CMake has it.
sed -n 's|^ *\${SOFTFLOAT_DIR}/\(.*\.c\)$|  \1 \\|p' "$SRC/cmake/softfloat_sources.cmake" |
  { echo "SOFTFLOAT_C := \\"; cat; echo; } > "$CACHE/softfloat.mk"

# 3. The recomp's own tools, natively (no SDL, no 7z: only what the recompile needs; built with
#    or without a ROM set, so the toolchain is known to work before one arrives).
cmake -S "$SRC" -B "$TOOLS" -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release -DM2_ROMSET=daytona \
  -DCMAKE_C_COMPILER="$NATIVE_CC" -DCMAKE_CXX_COMPILER="$NATIVE_CXX" >/dev/null
cmake --build "$TOOLS" --target m2import m2recomp m2tgprecomp m2sndrecomp -j"$JOBS" >/dev/null

# 4. The generated game code: from the ROM set when there is one, else the stub.
GEN_MODE=stub
if [ "${DAYTONA_GEN:-}" != stub ] && [ -f "$ROM" ]; then
  if [ ! -f "$IMAGES/tgp_program.bin" ] || [ "$ROM" -nt "$IMAGES/tgp_program.bin" ] || [ "$TOOLS/m2import" -nt "$IMAGES/tgp_program.bin" ]; then
    rm -rf "$IMAGES"
    if ! "$TOOLS/m2import" "$ROM" "$IMAGES"; then
      echo "build.sh: $ROM was rejected (above: the file that is missing or wrong). Only MAME's daytona (Revision A) set works." >&2
      exit 3
    fi
  fi
  # M2RECOMP_CHUNK: i960 instructions per generated function (m2recomp --chunk), m2recomp's own
  # 1500 by default. With the game, 300 made no difference worth having (README: measured).
  M2RECOMP_CHUNK="${M2RECOMP_CHUNK:-1500}"
  STAMP="$GEN/.stamp"
  STAMP_TEXT="$RECOMP_COMMIT chunk=$M2RECOMP_CHUNK"
  if [ ! -f "$STAMP" ] || [ "$(cat "$STAMP")" != "$STAMP_TEXT" ] || [ "$IMAGES/tgp_program.bin" -nt "$STAMP" ] ||
     [ "$TOOLS/m2recomp" -nt "$STAMP" ] || [ "$TOOLS/m2tgprecomp" -nt "$STAMP" ] || [ "$TOOLS/m2sndrecomp" -nt "$STAMP" ] ||
     [ "$SRC/seeds/daytona.txt" -nt "$STAMP" ] || [ "$SRC/seeds/daytona_hooks.txt" -nt "$STAMP" ]; then
    rm -rf "$GEN"
    mkdir -p "$GEN/daytona" "$GEN/daytona_tgp" "$GEN/daytona_snd"
    "$TOOLS/m2recomp" "$IMAGES/program.bin" "$GEN/daytona" --seeds "$SRC/seeds/daytona.txt" \
      --hooks "$SRC/seeds/daytona_hooks.txt" --chunk "$M2RECOMP_CHUNK"
    "$TOOLS/m2tgprecomp" "$IMAGES/tgp_program.bin" "$GEN/daytona_tgp/tgp_gen.cpp"
    "$TOOLS/m2sndrecomp" "$IMAGES/sound_program.bin" "$GEN/daytona_snd/snd_gen.cpp"
    echo "$STAMP_TEXT" > "$STAMP"
  fi
  GEN_MODE=rom
elif [ "${DAYTONA_GEN:-}" != stub ]; then
  echo "build.sh: no $ROM: linking the stub game code (the core loads no game). ROMS=<dir> to point elsewhere." >&2
fi

# 5. Compile and link (the shim names the recomp's commit; the header is rewritten only when it changes).
mkdir -p "$CACHE/obj"
VERSION_H="#define DAYTONA_RECOMP_COMMIT \"$RECOMP_COMMIT\""
[ "$(cat "$CACHE/obj/daytona_version.h" 2>/dev/null)" = "$VERSION_H" ] || echo "$VERSION_H" > "$CACHE/obj/daytona_version.h"
for target in "${TARGETS[@]}"; do
  make -s -f "$HERE/core.mk" -j"$JOBS" "$target" \
    TARGET="$target" SRC="$SRC" GEN="$GEN" GEN_MODE="$GEN_MODE" OBJ="$CACHE/obj" OUT="$HERE/dist" HERE="$HERE" \
    SOFTFLOAT_MK="$CACHE/softfloat.mk" NATIVE_CC="$NATIVE_CC" NATIVE_CXX="$NATIVE_CXX"
done

echo "Daytona USA recomp ($RECOMP_COMMIT, game code: $GEN_MODE) built$(command -v emcc >/dev/null && echo " with $(emcc --version | head -1)"):"
ls -lh "$HERE"/dist/*.mjs "$HERE"/dist/*.wasm "$HERE"/dist/headless/* "$CACHE"/native/bench 2>/dev/null || true
