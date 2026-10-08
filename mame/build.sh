#!/usr/bin/env bash
# Builds MAME's Namco System 12 (Tekken 3) and System 23 (Time Crisis II) drivers as a standalone
# Emscripten ES module that speaks the libretro API, so web/emulator/libretro.js and worker.js
# drive it like the FBNeo cores (emulator/build.sh) and Supermodel (supermodel/build.sh).
#
#   mame/dist/mame.mjs + mame.wasm    for the page, the worker and Node (bench.mjs)
#
#   ./mame/build.sh            everything: checkout, MAME's own build, link
#   ./mame/build.sh link       just the final link (after a change to the link flags)
#   (a change under .cache/mame needs the whole thing; MAME's make only rebuilds what changed)
#
# Steps:
#   1. The pinned emsdk (emulator/emsdk.sh; EMSDK_DIR overrides where it is)
#   2. A pinned checkout of libretro/mame, the libretro fork of MAME: it has a libretro OSD
#      (src/osd/libretro) whose retro_run() steps the machine one frame and returns, no threads
#      and no coroutines, which is what WebAssembly needs. Our fixes on top: patches/
#   3. MAME's own GENie build for Emscripten (TARGETOS=asmjs, which the makefile picks when CC is
#      emcc) with the retro OSD and only the two drivers (SUBTARGET + SOURCES; makedep finds the
#      devices they use). patches/0001 makes the libretro target an archive on asmjs.
#   4. Our own link of those archives into mame.mjs + mame.wasm, exporting the libretro API
#      (exports.json), as emulator/build.sh does for FBNeo.
set -euo pipefail

MAME_REPO="https://github.com/libretro/mame.git"
MAME_COMMIT="9069f39340f2d2b1795df8e71bb1d3d0fbc76598" # 2026-09-24

# The drivers in the core: Namco System 12 (Tekken 3, Soul Calibur, ...) and System 23 / Super
# System 23 (Time Crisis II, Motocross Go!, ...; its R4650 is MAME's MIPS III core, which brings
# the DRC with its C backend: Emscripten builds have NOASM, so FORCE_DRC_C_BACKEND).
SOURCES="src/mame/namco/namcos12.cpp,src/mame/namco/namcos23.cpp"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HERE="$ROOT/mame"
CACHE="$HERE/.cache"
SRC="$CACHE/mame"
OUT="$HERE/dist"
EMSDK_DIR="${EMSDK_DIR:-$ROOT/emulator/.cache/emsdk}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"
STEP="${1:-all}"

mkdir -p "$CACHE" "$OUT"

# 1. emsdk
[ -d "$EMSDK_DIR" ] || "$ROOT/emulator/emsdk.sh"
# shellcheck disable=SC1091
source "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1
# MAME's makefile wants it (upstream's emsdk_env no longer sets it).
export EMSCRIPTEN="$EMSDK_DIR/upstream/emscripten"

if [ "$STEP" = all ]; then
  # 2. MAME at the pinned commit, with our patches (each applied once: a patch that reverses
  #    cleanly is already in).
  if [ ! -d "$SRC/.git" ]; then
    git init -q "$SRC"
    git -C "$SRC" remote add origin "$MAME_REPO"
  fi
  if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null)" != "$MAME_COMMIT" ]; then
    git -C "$SRC" fetch --depth 1 origin "$MAME_COMMIT"
    git -C "$SRC" checkout -q --force FETCH_HEAD # drops patches applied to the old commit
  fi
  for patch in "$HERE"/patches/*.patch; do
    [ -e "$patch" ] || continue
    if ! git -C "$SRC" apply --reverse --check "$patch" 2>/dev/null; then
      git -C "$SRC" apply "$patch"
    fi
  done

  # 3. MAME's build: its own GENie (built natively), then every library for asmjs.
  #    CONFIG=libretro is the fork's configuration (retro OSD, no BGFX/MIDI); -fwasm-exceptions
  #    because MAME throws (emu_fatalerror and friends) and WebAssembly exceptions are much
  #    cheaper than Emscripten's JavaScript ones.
  started=$(date +%s)
  emmake make -C "$SRC" -j"$JOBS" \
    OSD=retro CONFIG=libretro SUBTARGET=vab SOURCES="$SOURCES" \
    NOWERROR=1 NO_USE_MIDI=1 NO_USE_PORTAUDIO=1 NO_OPENGL=1 USE_QTDEBUG=0 DONT_USE_NETWORK=1 \
    TOOLS=0 REGENIE=1 IGNORE_GIT=1 PYTHON_EXECUTABLE=python3 \
    ARCHOPTS="-fwasm-exceptions"
  echo "MAME libraries built in $(( $(date +%s) - started )) s"
fi

# 4. The module: the libretro target's own objects (the retro_* entry points, the driver list)
#    and all of MAME's archives (lld resolves symbols across them in any order). Memory: a
#    System 23 set is ~40 MB of zip in the in-memory file system and 120 MB of ROM once MAME
#    loads it.
OBJ="$SRC/build/libretro/obj/libretro"
MAIN_OBJS=("$OBJ/src/mame/mame.o" "$OBJ/src/osd/libretro/libretro-internal/libretro.o"
  "$OBJ/src/osd/libretro/retromain.o" "$OBJ/generated/mame/vab/drivlist.o" "$OBJ/generated/version.o")
LIBS=()
while IFS= read -r lib; do LIBS+=("$lib"); done < <(find "$SRC/build/libretro/bin" -name '*.a' | sort)
started=$(date +%s)
em++ "${MAIN_OBJS[@]}" "${LIBS[@]}" -O3 -fwasm-exceptions -o "$OUT/mame.mjs" \
  -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createMAME \
  -sENVIRONMENT=web,worker,node \
  -sINITIAL_MEMORY=536870912 -sALLOW_MEMORY_GROWTH=1 -sMAXIMUM_MEMORY=2147483648 \
  -sSTACK_SIZE=8388608 \
  -sALLOW_TABLE_GROWTH=1 -sFORCE_FILESYSTEM=1 \
  -sEXPORTED_FUNCTIONS=@"$HERE/exports.json" \
  -sEXPORTED_RUNTIME_METHODS=FS,addFunction,removeFunction,UTF8ToString,stringToUTF8,lengthBytesUTF8,getValue,setValue,HEAPU8,HEAP16,HEAPU16,HEAP32,HEAPU32
echo "Linked in $(( $(date +%s) - started )) s"

echo "MAME ($MAME_COMMIT, libretro fork) built with $(emcc --version | head -1):"
ls -lh "$OUT"/mame.mjs "$OUT"/mame.wasm
