#!/usr/bin/env bash
# Builds Supermodel (the Sega Model 3 emulator) as a standalone Emscripten module that speaks the
# libretro API, so web/emulator/libretro.js and worker.js can drive it like an FBNeo core
# (emulator/build.sh). shim/ holds what replaces Supermodel's SDL front end: libretro.cpp, OpenGL
# ES stand-ins, single-threaded OSD, RetroPad inputs.
#
#   supermodel/dist/supermodel.mjs + .wasm    for the page: WebGL2 on the module's canvas
#   supermodel/dist/headless/supermodel.mjs   for Node: same core, no-op OpenGL (bench.mjs)
#   supermodel/.cache/native/bench            native build of the same code, the baseline
#
#   ./supermodel/build.sh [web] [headless] [native]    (all three when none given)
#
# Steps: the pinned emsdk (emulator/emsdk.sh), a pinned Supermodel checkout with our patches, the
# generated Musashi 68K core and no-op GL, then core.mk compiles and links.
set -euo pipefail

SUPERMODEL_REPO="https://github.com/trzy/Supermodel.git"
SUPERMODEL_COMMIT="8b4de239bf3e0fd921f4383edcd3c21c2e62067b" # 2026-09-28

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HERE="$ROOT/supermodel"
CACHE="$HERE/.cache"
SRC="$CACHE/Supermodel"
GEN="$CACHE/gen"
EMSDK_DIR="${EMSDK_DIR:-$ROOT/emulator/.cache/emsdk}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"
TARGETS=("$@")
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(web headless native)

mkdir -p "$CACHE" "$GEN"

# 1. emsdk
[ -d "$EMSDK_DIR" ] || "$ROOT/emulator/emsdk.sh"
# shellcheck disable=SC1091
source "$EMSDK_DIR/emsdk_env.sh" >/dev/null

# 2. Supermodel at the pinned commit, with our patches (each applied once).
if [ ! -d "$SRC/.git" ]; then
  git init -q "$SRC"
  git -C "$SRC" remote add origin "$SUPERMODEL_REPO"
fi
if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null)" != "$SUPERMODEL_COMMIT" ]; then
  git -C "$SRC" fetch --depth 1 origin "$SUPERMODEL_COMMIT"
  git -C "$SRC" checkout -q --force FETCH_HEAD
fi
for patch in "$HERE"/patches/*.patch; do
  if ! git -C "$SRC" apply --reverse --check "$patch" 2>/dev/null; then
    git -C "$SRC" apply "$patch"
  fi
done

# 3. Generated sources: the Musashi 68K opcode handlers (built by Supermodel's own m68kmake, run
#    natively) and the no-op OpenGL for the headless and native builds. The Khronos GL headers
#    come from emsdk; the native build includes them from here.
MUSASHI="$SRC/Src/CPU/68K/Musashi"
if [ ! -x "$GEN/m68kmake" ] || [ "$MUSASHI/m68kmake.c" -nt "$GEN/m68kmake" ]; then
  cc -O2 -o "$GEN/m68kmake" "$MUSASHI/m68kmake.c"
fi
if [ ! -f "$GEN/m68kops.c" ] || [ "$MUSASHI/m68k_in.c" -nt "$GEN/m68kops.c" ]; then
  "$GEN/m68kmake" "$GEN" "$MUSASHI/m68k_in.c" >/dev/null
fi
KHRONOS="$EMSDK_DIR/upstream/emscripten/system/include"
mkdir -p "$GEN/khronos/GLES2" "$GEN/khronos/GLES3" "$GEN/khronos/KHR"
cp "$KHRONOS"/GLES2/gl2*.h "$GEN/khronos/GLES2/"
cp "$KHRONOS"/GLES3/gl3*.h "$GEN/khronos/GLES3/"
cp "$KHRONOS"/KHR/khrplatform.h "$GEN/khronos/KHR/"
node "$HERE/shim/gl_null.mjs" "$KHRONOS/GLES3/gl3.h" > "$GEN/gl_null.c"

# 4. Compile and link.
for target in "${TARGETS[@]}"; do
  make -s -f "$HERE/core.mk" -j"$JOBS" "$target" \
    TARGET="$target" SRC="$SRC" GEN="$GEN" OBJ="$CACHE/obj" OUT="$HERE/dist" HERE="$HERE"
done

echo "Supermodel ($SUPERMODEL_COMMIT) built with $(emcc --version | head -1):"
ls -lh "$HERE"/dist/*.mjs "$HERE"/dist/*.wasm "$HERE"/dist/headless/* "$CACHE"/native/bench 2>/dev/null || true
