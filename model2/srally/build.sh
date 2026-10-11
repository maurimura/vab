#!/usr/bin/env bash
# Builds Sega Rally Championship (Sega Model 2A, MAME set `srallyc`, Revision B program) from
# segarally95-recomp: the game's i960 program lifted to C, on that project's Model 2 runtime
# (TGP and geometrizer HLE, 68000 + SCSP sound board, System 24 tiles), as a standalone
# Emscripten module that speaks the libretro API (web/emulator/libretro.js drives it), and the
# recomp's own SDL viewer.
#
#   model2/srally/dist/srally.mjs + .wasm      for the page or worker (WebGL2 renderer)
#   model2/srally/dist/headless/srally.mjs     for Node (check.mjs, bench.mjs; no picture)
#   model2/srally/.cache/native/bench          the same core natively (clang), the baseline
#   model2/srally/.cache/native/segamod2       the recomp's SDL viewer (its CMake build)
#
#   ./model2/srally/build.sh [web] [headless] [native] [viewer]   (web headless native when none given)
#
# Unlike daytona/, nothing is generated from the ROM set at build time (the game is lifted C in
# the recomp's tree), so the core builds without it; it loads the set at run time from the zip
# the frontend hands it (MAME's srallyc files with the Revision B program EPROMs,
# epr-17888b.12 / epr-17889b.13, as srallycb.zip has them). The viewer reads the files from
# $SEGAMOD2_ROM_DIR. Nothing in .cache/ or dist/ is committed.
#
# Steps: the pinned emsdk (emulator/emsdk.sh), the pinned recomp checkout with our patches,
# then core.mk compiles and links (or CMake, for the viewer).
set -euo pipefail

RECOMP_REPO="https://github.com/xandoxan65/segarally95-recomp.git"
RECOMP_COMMIT="a80d71aaeaa66297002b4455af3c5cf06d66c947" # 2026-10-08

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/model2/srally"
CACHE="$HERE/.cache"
SRC="$CACHE/src"
NATIVE="$CACHE/native"
EMSDK_DIR="${EMSDK_DIR:-$ROOT/emulator/.cache/emsdk}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"
NATIVE_CC="${NATIVE_CC:-clang}"
TARGETS=("$@")
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(web headless native)

mkdir -p "$CACHE"

# 1. emsdk (only the wasm targets need it).
for target in "${TARGETS[@]}"; do
  if [ "$target" = web ] || [ "$target" = headless ]; then
    [ -d "$EMSDK_DIR" ] || "$ROOT/emulator/emsdk.sh"
    # shellcheck disable=SC1091
    source "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1
    break
  fi
done

# 2. The recomp at the pinned commit with our patches (each applied once).
if [ ! -d "$SRC/.git" ]; then
  git clone -q "$RECOMP_REPO" "$SRC"
fi
if [ "$(git -C "$SRC" rev-parse HEAD)" != "$RECOMP_COMMIT" ]; then
  git -C "$SRC" fetch -q origin
  git -C "$SRC" checkout -q --force "$RECOMP_COMMIT"
fi
# Our patches, in order, each applied once. A patch that neither applies nor reverse-applies has
# changed since it was applied (or the checkout was edited): the checkout is put back at the
# pinned commit and every patch applied again.
shopt -s nullglob
PATCHES=("$HERE"/patches/*.patch)
shopt -u nullglob
apply_patches() {
  local patch
  for patch in "${PATCHES[@]}"; do
    git -C "$SRC" apply --reverse --check "$patch" 2>/dev/null && continue
    git -C "$SRC" apply --check "$patch" 2>/dev/null || return 1
    git -C "$SRC" apply "$patch"
  done
}
if ! apply_patches; then
  echo "build.sh: a patch no longer matches the checkout: resetting $SRC to $RECOMP_COMMIT and applying all patches" >&2
  git -C "$SRC" checkout -q -- .
  apply_patches
fi

# 3. Build (the shim names the recomp's commit; the header is rewritten only when it changes).
mkdir -p "$CACHE/obj"
VERSION_H="#define SRALLY_RECOMP_COMMIT \"$RECOMP_COMMIT\"
#define SRALLY_RECOMP_COMMIT_SHORT \"${RECOMP_COMMIT:0:7}\""
[ "$(cat "$CACHE/obj/srally_version.h" 2>/dev/null)" = "$VERSION_H" ] || echo "$VERSION_H" > "$CACHE/obj/srally_version.h"
for target in "${TARGETS[@]}"; do
  case "$target" in
    viewer)
      # The recomp's own CMake build (SDL2, libpng and OpenGL from Homebrew when present).
      cmake -S "$SRC" -B "$NATIVE" -DCMAKE_BUILD_TYPE=Release >"$CACHE/cmake.log" 2>&1 || { tail -20 "$CACHE/cmake.log"; exit 1; }
      grep -E 'SDL2=|PNG=|GL=' "$CACHE/cmake.log" || true
      cmake --build "$NATIVE" -j "$JOBS" 2>&1 | grep -E 'error|Built target' | tail -5
      ls -la "$NATIVE/segamod2"
      ;;
    web|headless|native)
      make -s -f "$HERE/core.mk" -j"$JOBS" "$target" \
        TARGET="$target" SRC="$SRC" OBJ="$CACHE/obj" OUT="$HERE/dist" HERE="$HERE" NATIVE_CC="$NATIVE_CC"
      ;;
    *)
      echo "build.sh: unknown target $target (web, headless, native, viewer)" >&2
      exit 2
      ;;
  esac
done

echo "Sega Rally recomp (${RECOMP_COMMIT:0:7}) built$(command -v emcc >/dev/null && echo " with $(emcc --version | head -1)"):"
ls -lh "$HERE"/dist/*.mjs "$HERE"/dist/*.wasm "$HERE"/dist/headless/* "$NATIVE"/bench 2>/dev/null || true
