#!/usr/bin/env bash
# Builds Flycast (Sega NAOMI / Dreamcast: SH4, PowerVR2, AICA) as a standalone Emscripten ES
# module that speaks the libretro API, so web/emulator/libretro.js and worker.js drive it like the
# bar's other own cores (mame/, supermodel/). One module for the page and for Node:
#
#   flycast/dist/flycast.mjs + flycast.wasm
#
# In the page it draws with WebGL2 on the OffscreenCanvas the worker gives it (Module.canvas); in
# Node (bench.mjs, check.mjs) there is no canvas and Flycast gets a no-op OpenGL instead (shim/),
# frames come out as duplicates and the whole machine still runs, identically.
#
#   ./flycast/build.sh          everything: checkouts, patches, Flycast's CMake build, our link
#   ./flycast/build.sh link     just the shim and the link (after a change under shim/)
#   (a change under patches/ needs the whole thing: patches are re-applied, CMake rebuilds what
#   changed)
#
# Steps:
#   1. The pinned emsdk (emulator/emsdk.sh; EMSDK_DIR overrides where it is).
#   2. retrom-project/flycast-wasm at a pinned commit (branch retrom/1.0, the NAOMI-fixed fork of
#      nasomers/flycast-wasm): its patch set, which turns flyinghead/flycast into an Emscripten
#      libretro core with an SH4 -> WebAssembly JIT (core/rec-wasm). We take, sha256-checked:
#      wasm-jit-phase1-modified.patch (the port and the JIT's hooks), rec_wasm.cpp, wasm_emit.h,
#      wasm_module_builder.h, fly_instrument.h (the JIT), flycast-webgl.patch (WebGL2 refuses
#      primitive-restart toggles) and flycast-rom-crc.patch (a NAOMI ROM table fix). Not taken:
#      flycast-range.patch (HTTP Range streaming through ASYNCIFY; we download whole files).
#   3. flyinghead/flycast at the commit that fork pins, with its submodules.
#   4. The fork's patches, then ours (patches/, each with a header saying why), applied once: the
#      set applied is recorded, and a different set is first reversed off the tree.
#   5. Flycast's own CMake build as a libretro core (LIBRETRO=ON, GLES3), WebAssembly exceptions.
#   6. Our shim (shim/vab_flycast.cpp: the OpenGL context, the read-back, the core options; the
#      no-op GL generated from Khronos' gl3.h by shim/gl_null.mjs) and our own link of it all into
#      flycast.mjs + flycast.wasm, exporting the libretro API (exports.json), as mame/build.sh does.
set -euo pipefail

FLYCAST_REPO="https://github.com/flyinghead/flycast.git"
FLYCAST_COMMIT="2c48c0188a2afc158b02b6d1865d898756a03071" # 2026-02-20, as the fork pins it
FORK_REPO="https://github.com/retrom-project/flycast-wasm.git"
FORK_COMMIT="cb5fed96a5c2ee9606745fe896a4851871d8816a" # retrom/1.0, 2026-10-06 (retrom-core-1.0-r4)
# What we take from the fork, and its sha256.
FORK_FILES=(
  "patches/wasm-jit-phase1-modified.patch 58525a76d02ac3ec71d294e064e54fd707d412a68e56675709cecd1d9791c23f"
  "patches/flycast-webgl.patch e7083f05038363f29ed3331c4f7964dfdb405328df75e534fd16233685af7ca4"
  "patches/flycast-rom-crc.patch 858f39468b6cd8f466a5821d15a6ebb4ec964abb71f0dfd28f538c0e468f0909"
  "patches/rec_wasm.cpp 1dade7d5370661cfc2dba72532ac02eb8f1e0df501b6af5c2f1dfdc312d4c9c8"
  "patches/wasm_emit.h c19849ea7e6105e4802f462a5e24e8fd8c82bc40389925e94dbad24d157be30d"
  "patches/wasm_module_builder.h 20952ecb5bf92fa640658632208b04398d9501987f7c9328dfcee7c11bba9a0b"
  "patches/fly_instrument.h 0426b226bbecaf9abf63f7887165cb0c21786a8b56d6804d1a06f35d2677477e"
)
FORK_PATCHES=(wasm-jit-phase1-modified flycast-webgl flycast-rom-crc)
JIT_FILES=(rec_wasm.cpp wasm_emit.h wasm_module_builder.h fly_instrument.h)

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HERE="$ROOT/flycast"
CACHE="$HERE/.cache"
SRC="$CACHE/flycast"
FORK="$CACHE/flycast-wasm"
BUILD="$CACHE/build"
GEN="$CACHE/gen"
OBJ="$CACHE/obj"
OUT="$HERE/dist"
EMSDK_DIR="${EMSDK_DIR:-$ROOT/emulator/.cache/emsdk}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"
STEP="${1:-all}"
# Fixed so nothing in the build depends on when it ran (the fork does the same).
export SOURCE_DATE_EPOCH=1771589293

mkdir -p "$CACHE" "$GEN" "$OBJ" "$OUT"

# 1. emsdk
[ -d "$EMSDK_DIR" ] || "$ROOT/emulator/emsdk.sh"
# shellcheck disable=SC1091
source "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1

checkout() { # <dir> <repo> <commit>
  if [ ! -d "$1/.git" ]; then
    git init -q "$1"
    git -C "$1" remote add origin "$2"
  fi
  if [ "$(git -C "$1" rev-parse HEAD 2>/dev/null)" != "$3" ]; then
    git -C "$1" fetch -q --depth 1 origin "$3"
    git -C "$1" checkout -q --force FETCH_HEAD
    return 0
  fi
  return 1
}

if [ "$STEP" = all ]; then
  started=$(date +%s)
  # 2. The fork, its files checked.
  checkout "$FORK" "$FORK_REPO" "$FORK_COMMIT" || true
  for entry in "${FORK_FILES[@]}"; do
    read -r file sum <<<"$entry"
    actual=$(shasum -a 256 "$FORK/$file" | cut -d' ' -f1)
    if [ "$actual" != "$sum" ]; then
      echo "$FORK/$file: sha256 $actual, expected $sum" >&2
      exit 1
    fi
  done

  # 3. Flycast and its submodules (a new checkout forgets what was applied to the old one).
  if checkout "$SRC" "$FLYCAST_REPO" "$FLYCAST_COMMIT"; then rm -rf "$CACHE/applied"; fi
  git -C "$SRC" submodule update -q --init --recursive --depth 1

  # 4. The patches: the fork's, its JIT's sources, ours. What is applied is kept in
  #    .cache/applied; when that differs from what we'd apply now, it comes off first.
  wanted="$CACHE/wanted"
  rm -rf "$wanted"
  mkdir -p "$wanted/fork" "$wanted/jit" "$wanted/ours"
  n=0
  for name in "${FORK_PATCHES[@]}"; do
    n=$((n + 1))
    cp "$FORK/patches/$name.patch" "$wanted/fork/$n-$name.patch"
  done
  for file in "${JIT_FILES[@]}"; do cp "$FORK/patches/$file" "$wanted/jit/"; done
  cp "$HERE"/patches/*.patch "$wanted/ours/"
  if ! diff -rq "$wanted" "$CACHE/applied" >/dev/null 2>&1; then
    if [ -d "$CACHE/applied" ]; then
      echo "The patches changed: taking the old ones off"
      for patch in $(ls -r "$CACHE"/applied/ours/*.patch); do git -C "$SRC" apply -R --whitespace=nowarn "$patch"; done
      rm -rf "$SRC/core/rec-wasm"
      for patch in $(ls -r "$CACHE"/applied/fork/*.patch); do git -C "$SRC" apply -R --whitespace=nowarn "$patch"; done
    fi
    rm -rf "$CACHE/applied"
    # Upstream keeps this file with CRLF line ends; the fork's patch has LF.
    perl -pi -e 's/\r\n/\n/' "$SRC/shell/libretro/audiostream.cpp"
    # (git apply notes the fork patch's two submodule lines, which change nothing here)
    for patch in "$wanted"/fork/*.patch; do git -C "$SRC" apply --whitespace=nowarn "$patch" 2>/dev/null || git -C "$SRC" apply --whitespace=nowarn "$patch"; done
    mkdir -p "$SRC/core/rec-wasm"
    cp "$wanted"/jit/* "$SRC/core/rec-wasm/"
    for patch in "$wanted"/ours/*.patch; do
      echo "Applying $(basename "$patch")"
      git -C "$SRC" apply --whitespace=nowarn "$patch"
    done
    mv "$wanted" "$CACHE/applied"
  fi
  rm -rf "$wanted"

  # 5. Flycast's build: the libretro core as a static library (the fork's CMake changes) and its
  #    dependencies. JIT_PROD_BUILD/FLY_RELEASE_BUILD compile the fork's diagnostics out;
  #    -fwasm-exceptions because Flycast throws (SH4 exceptions, failed loads) and WebAssembly
  #    exceptions cost nothing until thrown, unlike Emscripten's JavaScript ones.
  FLAGS="-DJIT_PROD_BUILD=1 -DFLY_RELEASE_BUILD=1 -fwasm-exceptions"
  emcmake cmake -S "$SRC" -B "$BUILD" -DCMAKE_BUILD_TYPE=Release -DLIBRETRO=ON -DUSE_GLES=ON -DUSE_LUA=OFF \
    -DCMAKE_C_FLAGS="$FLAGS" -DCMAKE_CXX_FLAGS="$FLAGS" >"$CACHE/cmake.log" 2>&1 ||
    { tail -30 "$CACHE/cmake.log"; exit 1; }
  emmake make -C "$BUILD" -j"$JOBS" >"$CACHE/make.log" 2>&1 ||
    { grep -E -B2 -A8 "error" "$CACHE/make.log" | head -60; echo "Flycast's build failed (.cache/make.log)" >&2; exit 1; }
  echo "Flycast built in $(( $(date +%s) - started )) s"
fi

# 6. The shim and the link.
started=$(date +%s)
KHRONOS="$EMSDK_DIR/upstream/emscripten/system/include"
node "$HERE/shim/gl_null.mjs" "$KHRONOS/GLES3/gl3.h" > "$GEN/gl_null.c"
emcc -O3 -c "$GEN/gl_null.c" -o "$OBJ/gl_null.o"
em++ -O3 -std=c++17 -fwasm-exceptions -Wall -c "$HERE/shim/vab_flycast.cpp" -o "$OBJ/vab_flycast.o" \
  -I"$SRC/core/deps/libretro-common/include"
LIBS=(
  "$BUILD/libflycast_libretro_emscripten.a" "$BUILD/libflycast-resources.a"
  "$BUILD/core/deps/libzip/lib/libzip.a" "$BUILD/core/deps/libelf/libelf.a"
  "$BUILD/core/deps/miniupnpc/libminiupnpc.a" "$BUILD/core/deps/tinygettext/libtinygettext.a"
  "$BUILD/core/deps/nowide/libnowide.a" "$BUILD/core/deps/libchdr/libchdr-static.a"
  "$BUILD/core/deps/libchdr/deps/lzma-24.05/liblzma.a"
  "$BUILD/core/deps/libchdr/deps/zstd-1.5.6/build/cmake/lib/libzstd.a"
  "$BUILD/core/deps/libchdr/deps/zlib-1.3.1/libz.a" "$BUILD/core/deps/xxHash/cmake_unofficial/libxxhash.a"
)
# Memory: a NAOMI is 32 MB of RAM, 16 MB of VRAM and 8 MB of sound RAM, the GD-ROM game is loaded
# whole into the DIMM board's memory, and the JIT's tables take ~25 MB; the disc and the ROM set
# sit in the in-memory file system besides. The JIT grows the function table (one entry per
# compiled block) and reaches the module's own exports through Module (HEAPU8 included).
# DEBUG=1 links with Emscripten's assertions and function names (readable stack traces).
DEBUG_FLAGS=()
[ -n "${DEBUG:-}" ] && DEBUG_FLAGS=(-sASSERTIONS=1 --profiling-funcs)
em++ "$OBJ/vab_flycast.o" "$OBJ/gl_null.o" "${LIBS[@]}" -O3 -fwasm-exceptions "${DEBUG_FLAGS[@]}" -o "$OUT/flycast.mjs" \
  -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createFlycast \
  -sENVIRONMENT=web,worker,node \
  -sINITIAL_MEMORY=536870912 -sALLOW_MEMORY_GROWTH=1 -sMAXIMUM_MEMORY=2147483648 \
  -sSTACK_SIZE=8388608 \
  -sALLOW_TABLE_GROWTH=1 -sFORCE_FILESYSTEM=1 \
  -sMIN_WEBGL_VERSION=2 -sMAX_WEBGL_VERSION=2 \
  -sEXPORTED_FUNCTIONS=@"$HERE/exports.json" \
  -sEXPORTED_RUNTIME_METHODS=FS,addFunction,removeFunction,UTF8ToString,stringToUTF8,lengthBytesUTF8,getValue,setValue,HEAPU8,HEAP16,HEAPU16,HEAP32,HEAPU32,specialHTMLTargets
echo "Linked in $(( $(date +%s) - started )) s"

echo "Flycast ($FLYCAST_COMMIT + retrom-project/flycast-wasm $FORK_COMMIT) built with $(emcc --version | head -1):"
ls -lh "$OUT"/flycast.mjs "$OUT"/flycast.wasm
