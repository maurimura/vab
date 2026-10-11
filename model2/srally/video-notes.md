# Sega Rally video: GLES renderer + CPU picture

Two pieces put Sega Rally's picture into CPU memory for the libretro core:

- `patches/0101-gles-renderer.patch` (against segarally95-recomp `a80d71a`; applies on its own and after `0001`–`0006`). It changes only `lib/model2/geo/model2_geo_gl.c`, `lib/model2/geo/include/model2_geo_gl.h`, `lib/host_compat/model2_gl.h` and `lib/host_compat/model2_gl.c`. The 3D renderer now uses only what OpenGL ES 3.0 / WebGL2 and the desktop GL 2.1 context share, and it draws into its own 496x384 framebuffer.
- `shim/video.c` + `shim/video.h`. These do the composite and the read-back that `src/host/sys24_viewer.c`'s flip does, without SDL or a window: tiles + 3D → one 496x384 RGBA8 picture.

## What the renderer does

The renderer gets each frame's triangles from `model2_geo_lock_textured` (`model2_geo_render.c`). Every triangle has three `xyzuv` vertices that are already in camera space, and one material: colorbase, lumabase, luma, texture patch, sheet, flags, `pad` and `z_sort`.

- **Projection.** A single mat4 from the GEO window/focus state (`model2_geo_gl_projection_matrix`, the same as the viewer's `geo_load_model2_projection`). It is an infinite positive-Z projection with clip w = camera z and znear 0.01. There is no far plane and no modelview. With no GEO projection yet, the transform is identity.
- **Shading.** There is no lighting. A solid triangle gets one palette colour (`model2_palette_lookup_solid`, luma >> 2). A textured triangle samples a 4-bit index from one of two 2048x1024 index sheets (nearest; patch-local wrap/mirror as in MAME `fetch_bilinear_texel`), and that index picks one of the 16 RGBA entries of its material's palette LUT (`model2_palette_build_texel_lut`, luma baked in). There is no gamma table.
- **Textures.** The sheets are R8 on ES and LUMINANCE on desktop, re-uploaded when `model2_tex_sheets_dirty_gen` moves. The LUTs live in one RGBA8 atlas of 256 x n texels (16 LUTs a row; it grows to 2048 rows). The atlas is cached until `model2_palette_state_sig` changes, like the per-material LUT textures before it.
- **Order and phases.** Triangles are sorted on the CPU, as before: world before HUD (`pad & 1`), then solid → opaque-textured → cutout, then nearer `float_to_zval` first, then last-submitted first. The phases are:
  - **A, solids:** depth LESS, depth write.
  - **B, opaque textured:** depth LEQUAL + write, stencil fillmap.
  - **C, cutouts:** stencil cleared, depth LEQUAL with no write, fillmap. A texel with LUT alpha < 0.5 is `discard`ed.
  - **HUD overlays** (tach needle): no depth, no stencil.

  Each phase is one `glDrawArrays`. Every triangle carries its material in its vertex attributes, and GL keeps primitive order within a draw, so the result is the same as the old per-material draws. Natively this is pixel-identical, checked below. It takes 4 draws a frame instead of ~1200–1600.
- **Stencil = the fillmap.** The Model 2 rasterizer is first-wins per pixel over a nearest-first list. Within B (and again within C) the stencil test is `EQUAL 0` with `INCR` on depth pass. The first sorted fragment to land on a pixel keeps it, and later co-planar polygons do not z-fight it: the desert sky sheets, decals and shadows. The game needs this, so it stays: 8 bits in the target (depth24/stencil8).
- **Blending.** There is none in the 3D. Every drawn pixel has alpha 1 and the target is cleared to alpha 0, so a composite selects 3D where alpha = 1.
- **Target.** RGBA8 texture + DEPTH24_STENCIL8 renderbuffer, 496x384 by default (`model2_geo_gl_set_target_size`). It is **top-down**: clip y is negated, so texel row 0 is the picture's top, like the tile bitmaps and the read-back. Culling is off, so the flipped winding does not matter.
- **The viewer's entry point.** `model2_geo_gl_draw_textured`, as the SDL viewer calls it, renders into the target and then blends it over the caller's framebuffer at the caller's viewport. On desktop it takes the projection from the fixed-function `GL_PROJECTION x GL_MODELVIEW` when no `model2_geo_gl_set_mvp` was given. That is why the unchanged `sys24_viewer.c` still works.
- **New entry points** (`model2_geo_gl.h`):
  - `_projection_matrix`, `_set_mvp`, `_set_target_size`
  - `_render_offscreen` (target only; returns its texture), `_target_texture`, `_composite_target`
  - `_set_lut_per_frame` (see open issues)
- **What the port dropped.** There is no fixed-function matrix stack, client arrays, immediate mode, `GL_QUADS`, `glPolygonMode`, `glPointSize`, `glEnable(GL_TEXTURE_2D)`, BGRA or 8_8_8_8_REV. `I960_GEO_FLAT` (gray debug shading) is kept, drawn with the same program. Under ES, `model2_gl.h` compiles the immediate-mode calls of `model2_geo_render_draw_gl` (`model2_geo_render.c`, a debug path nothing calls) to nothing, so that file still builds with `I960_HOST_HAVE_GL`.
- **Shaders.** One source, two headers: `#version 300 es` (with `in`/`out`, `texture`, highp) or `#version 120` (with `attribute`/`varying`, `texture2D`, `gl_FragColor`). The selector is `MODEL2_GL_ES`, set automatically under `__EMSCRIPTEN__`. Per-triangle integers are rounded after interpolation, since GLSL 1.20 has no `flat`.

## The composite (video.c): MAME model2_v order

This is what the viewer's flip does (`sys24_viewer.c` ~1204, the order comment ~469), per frame:

1. Read main mode (`0x202098`), inner (`0x20209c`) and frame (`0x20a808`). `opaque2d` is true for:
   - the splash hold (inner 2/4/6 with a negative frame),
   - the operator menu (mode 4),
   - the START banner (mode 3, practice/champ scene table on `0x1B940`).

   On entering `opaque2d`, the published mesh is dropped (`model2_geo_clear`).
2. Geometry for the frame. By default render does nothing here: the caller has already cleared or decoded it (the libretro shim's `DecodeFrame`; see decode modes below).
3. If the tile signature changed (`viewer_tile_sig`: samples of tile map, palram and char RAM, layer regs, main mode; or, with `srally_video_set_exact_tiles(1)`, an exact compare), the tiles are redrawn with `sys24_tile.c`:
   - `sys24_tile_bind`;
   - in game start (mode 3), ctrl bit14 of `0x0100a008` is cleared around the draw, as the viewer does for the ranking;
   - with `opaque2d`, one `SYS24_PASS_ALL` pass with black clear;
   - otherwise `SYS24_PASS_BOTTOM` (the even layers) and `SYS24_PASS_PRIORITY` (the odd layers), each into its own 496x384 `0xAARRGGBB` bitmap with clear 0.

   Both bitmaps are uploaded as RGBA8 textures; the bytes are B,G,R,A and the shader swizzles.
4. The 3D layer: `model2_geo_gl_render_offscreen`, with the projection from `model2_geo_projection`.
5. One fullscreen pass into the 496x384 composite framebuffer:
   - picture = black;
   - the bottom tiles where non-black (the viewer's punch: pen 0 / black is transparent, `copybitmap_trans`);
   - then 3D where alpha = 1;
   - then the priority tiles where non-black.

   With `opaque2d`, it is just the ALL-layers bitmap, black included, and no 3D.
6. Read-back (below). Framebuffer row y is picture row y from the top, so the bytes come out top-down with no CPU flip.

The viewer's other fallback is dropped. When no textured mesh is published, the viewer draws the untextured debug mesh light blue or as points; `video.c` draws no 3D layer.

### Read-back

- **`SRALLY_VIDEO_READBACK_FENCED` (default).** This is Supermodel's pattern (`supermodel/shim/libretro.cpp` EndFrameVideo). Each render first takes out the previous picture: `glClientWaitSync(fence, 0, 0)` and, if it has passed, `glGetBufferSubData` from its pixel-pack buffer. Then it `glReadPixels` into the other buffer and sets a fence. The picture handed out is the previous render's.
  - If a fence has not passed yet, the older picture stays, and `"late"` in the timings counts it. `late` was 0 in every run, native and Chrome.
  - In the browser, a fence only signals between event-loop turns, so do one render per turn. Two buffers are the default; `-DSRALLY_VIDEO_PBOS=n` makes the ring deeper (n−1 renders late).
- **`SRALLY_VIDEO_READBACK_SYNC`.** A blocking `glReadPixels`; the picture is the current render's. It is used automatically when fences are missing (desktop GL without ARB_sync).

## Interface for the shim (`shim/video.h`)

```c
int  srally_video_init(int width, int height);  /* GL context current; 496x384 (integer multiples render the 3D sharper) */
void srally_video_shutdown(void);
void srally_video_render(void);                 /* once per game frame */
const uint8_t *srally_video_pixels(void);       /* RGBA8 top-down, alpha 255; NULL before the first picture */
/* additions */
int  srally_video_create_context(void);         /* Emscripten: WebGL2 on Module.canvas with the attributes below */
void srally_video_set_readback(int mode);       /* SRALLY_VIDEO_READBACK_FENCED (default) / _SYNC */
void srally_video_set_decode(int mode);         /* SRALLY_VIDEO_DECODE_CALLER (default: render only draws) / _SYNC / _KICK */
void srally_video_set_exact_tiles(int on);      /* 0 (default): the viewer's sampled tile signature; 1: exact compare */
void srally_video_invalidate(void);             /* after a state load or reset (overrides libretro.c's weak no-op) */
const char *srally_video_timings(void);         /* {"frames","decode","tiles","geo","composite","readback","total","late"}, µs since last call */
```

**Where to call render.** `shim/libretro.c` as it stands already does the right thing, and `core.mk` already switches from `video_stub.c` to `video.c` once this patch is applied (it greps for `model2_geo_gl_render_offscreen`):
- `retro_run` runs the game's frame (the coroutine stops at the `frame_end` host op, patch `0001`), calls `DecodeFrame()`, and then calls `srally_video_render()` + `srally_video_pixels()` only when the frontend shows the frame.
- `retro_load_game` calls `srally_video_create_context()` and then `srally_video_init(496, 384)`.
- `retro_reset` and `retro_unserialize` call `srally_video_invalidate()`. `video.c` now defines it (strong), so it overrides the shim's weak no-op. It redraws the tiles, rebuilds the LUTs/sheets (`model2_geo_gl_invalidate`), and drops pictures in flight from before the load.
- `video.c` reads the same three pointers the viewer's flip gets from `lift_display_flip`: `model2_tile_map_ptr()`, `model2_tile_char_ptr()`, `model2_palram_ptr()`.
- The viewer's realtime path calls `boot_vblank` (`lift_boot_vblank` → `boot_tile_splash_frame`, boot screen only) before every flip. The offline `frame_end` path does not, so if the boot splash tiles look stuck, call it before rendering.

**Decode modes** (`srally_video_set_decode`). The geometry decode changes machine state (the decoder's mesh), so it must not depend on whether a frame is drawn.
- **`SRALLY_VIDEO_DECODE_CALLER` (default):** render only reads emulated RAM and draws. The caller does, every frame: on entering a 2D-only screen `model2_geo_clear()`, otherwise `model2_geo_decode()`. That is the shim's `DecodeFrame`; with patch `0003` (`model2_geo_render_set_sync(1)`, no worker) the decode runs on that thread.
- **`_SYNC`:** render does that itself first.
- **`_KICK`:** render does it the SDL viewer's way (`model2_geo_kick()` and the worker's latest mesh). Without a worker this never updates the 3D.

**GL state.**
- `video.c` assumes it owns the context and that nothing else draws. It sets every state it uses and leaves framebuffer 0, program 0 and no buffers or textures bound.
- All calls go on the thread that owns the context. The geometry decode does not touch GL.
- Call `srally_video_init` after the context exists. It also allocates the tile state; `sys24_tile.c` and `sys24_gfx.c` from `src/host/` must be linked.

### Emscripten

Compile `video.c` and the patched recomp sources with:

```
-DI960_HOST_HAVE_GL -DSRALLY_HAVE_GL      (MODEL2_GL_ES is set by __EMSCRIPTEN__; <GLES3/gl3.h>)
-I<src>/include -I<src>/lib/model2/include -I<src>/lib/model2/host -I<src>/lib/model2/geo/include -I<src>/lib/host_compat
```

- **No `I960_HOST_HAVE_SDL`.** `-std=c99` is fine; `video.c` uses no `EM_ASM`.
- **Link:**

  ```
  -sMAX_WEBGL_VERSION=2 -sMIN_WEBGL_VERSION=2
  -sEXPORTED_RUNTIME_METHODS=specialHTMLTargets,...   (for srally_video_create_context's "!canvas" lookup)
  ```

- **`VIDEO_LDFLAGS` (`core.mk`): empty.** The link line there already has `-sMAX_WEBGL_VERSION=2 -sMIN_WEBGL_VERSION=2` and `specialHTMLTargets`. `core.mk`'s `RENDER_CFLAGS` (`-std=gnu99`, `INCLUDES`, `-DI960_HOST_HAVE_GL -DSRALLY_HAVE_GL` for web only) are what `video.c` needs.
- **`-sFULL_ES3`: no.** The renderer uses vertex buffers only (no client arrays) and no `glMapBufferRange`. The read-back uses `glGetBufferSubData`, which Emscripten implements for WebGL2 and which `model2_gl.h` declares.
- **`-sOFFSCREEN_FRAMEBUFFER`: no**, and `-sOFFSCREENCANVAS_SUPPORT` is not needed either. The worker's `OffscreenCanvas` goes in as `Module.canvas`, exactly as Supermodel does it. That changes only if the context is made on a pthread.
- **Context attributes** (as in `srally_video_create_context`):
  - `majorVersion 2`;
  - `alpha false`, `depth false`, `stencil false`: everything is drawn into framebuffer objects with their own depth/stencil, and the canvas buffer is never shown or read;
  - `antialias false`, `premultipliedAlpha false`, `preserveDrawingBuffer false`, `powerPreference high-performance`.
- **Checked:**
  - `video.c`, `model2_geo_gl.c`, `model2_gl.c` and (unpatched) `model2_geo_render.c` compile warning-free with emsdk 6.0.10 (`-Wall -Wextra`), alone and on top of `0001`–`0006`.
  - A WebGL2 build of `video.c` + `model2_geo_gl.c` runs in headless Chrome. The shaders compile, there are no GL errors, and the frames match native (see validation).

### Headless / no GL (Node)

Leave out both `SRALLY_HAVE_GL` and `I960_HOST_HAVE_GL`.
- `model2_geo_gl.h` then turns the renderer into inline no-ops and `model2_geo_render.c` builds without GL.
- `srally_video_render` does nothing, and `srally_video_pixels` returns an opaque black frame from `srally_video_init` on. No no-op GL library (Supermodel's `gl_null`) is needed.
- Do not define `SRALLY_HAVE_GL` without a context: init then fails, and pixels stays NULL.

## Validation

All runs were in a scratch clone at `a80d71a`; the core's checkout was left alone.
- **Harness.** A validation harness hooked into `sys24_viewer.c` (`VAL_*` env) does a `--practice` run of 2400 frames:
  - the 3D is decoded synchronously at each flip;
  - after GO it holds the throttle, steps the Delta's manual shifter 1→4 and steers a fixed pattern;
  - the 496x384 picture is dumped every 60 frames, from frame 300 to 2400 (36 frames).

  The same harness went into the unpatched and the patched build, and the comparison is pixel by pixel.
- **Determinism.** Two runs of the unpatched build are identical: 0 differing pixels over all 36 frames.
- **Window at 496x384** (`VAL_NATIVE_WIN`), so both draw the 3D at arcade size:
  - The new shader/vertex-buffer code drawn straight into the window (a diagnostic) is **pixel-identical** to the old renderer on all 36 frames.
  - Drawn through the 496x384 framebuffer (the shipped path), **736 of 6,856,704 pixels differ (0.011%)**, 729 of them in one frame (840). They are window/glass decals on house walls, where co-planar cutouts land on the other side of the depth tie.
  - Apple's GL-over-Metal does not rasterize an FBO edge-for-edge like the window. A bottom-up target gave 8,216 differing pixels (0.12%). A float depth buffer, `ftransform()`, the driver's own MVP or built-in varyings changed nothing; rendering the target top-down brought it to 736.
  - The batched draw (4 draws a frame) is pixel-identical to the per-material draw it replaced.
- **`video.c` vs the viewer**, through the same patched renderer: identical. Against the unpatched viewer the difference is the same 736 pixels. With the fenced read-back each picture is exactly one frame late (dumped at 301, 361, … and compared to 300, 360, …), with no late fences.
- **Decode modes and invalidate.**
  - `SRALLY_VIDEO_DECODE_CALLER`, with the shim's `DecodeFrame` done in the harness before render, is pixel-identical to `_SYNC` over all 36 frames.
  - `srally_video_invalidate()` at frame 1500 changed only that frame's picture: the tiles were redrawn and showed the timer's current hundredths ("02" instead of the stale "00"; see exact tiles below).
- **Exact tiles** (`srally_video_set_exact_tiles(1)`). Against the viewer's sampled signature, 25 of the 36 frames differ, all in the HUD timer digits: the viewer often shows the hundredths of a frame or more ago. With exact tiles they are the current RAM's.
- **At the normal window size (992x768).** Same picture. The visible difference is that the 3D is now rendered at 496x384 and scaled up, like the tiles, instead of at window resolution: blockier edges, and more texture aliasing in the distance. The viewer could call `model2_geo_gl_set_target_size(dw, dh)` (one line in `sys24_viewer.c`, which the core agent owns) to get the old sharpness back natively.
- **Literal recordings.** `--practice --record` of the pristine build and the 0101-only build both record fine (frames 700 and 1100 looked at side by side). They are not frame-aligned, because recording is wall-clock driven.
- **WebGL2.**
  - Four frames (300, 780, 1200, 1920) were captured natively: mesh, materials, projection, tile bitmaps, decoded sheets, LUTs, and the native read-back. They were replayed through the Emscripten build of `video.c` + `model2_geo_gl.c` in headless Chrome (my own instance, ANGLE Metal on an Apple M1 Max).
  - Frames 780, 1200 and 1920 differ from native in 1,937 / 6 / 405 pixels, all the same decal z-ties.
  - Frame 300 differs in 97k pixels, but by ±1–2 levels. Native drew it with LUTs cached during the start fade that the palette signature did not see change (see open issues). Rebuilt fresh natively, it differs from the web in 315 pixels.
  - Fenced read-back in Chrome: the first picture arrives at the 2nd render, and every fence had signalled by the next render.

## Costs (Apple M1 Max)

**Native**, GL 2.1 over Metal, `video.c`, ms per frame, 2100 frames of the practice run:

| | decode | tiles | 3D (CPU submit) | composite | read-back | total |
|---|---|---|---|---|---|---|
| fenced read-back (default), decode in render (`_SYNC`) | 0.37 | 1.22 | 0.52 | 0.02 | **0.16** | 2.29 |
| fenced read-back, worker kick | – | 1.78 | 0.73 | 0.02 | **0.20** | 2.74 |
| sync `glReadPixels`, worker kick | – | 2.09 | 0.86 | 0.03 | **1.03** | 4.00 |

With `srally_video_set_exact_tiles(1)` the tiles cost **4.4 ms** a frame (sync read-back run, race part). The timer changes every frame, and each redraw rebuilds all four of `sys24_tile.c`'s 512x512 layer pixmaps (its content signature covers the whole maps) and draws two passes. The default skips frames whose sampled signature did not move: 1.2–2 ms.

The decode row is the shim's `DecodeFrame` work when render does it (`_SYNC`): ~0.4 ms natively.

The viewer's `draw_geo_layer` took 2.5–4.4 ms before the port and takes 0.38–0.7 ms after.

**Chrome headless (WebGL2, ANGLE Metal).** Replay of frame 1920 (4,432 triangles, LUTs cached, tile drawing stubbed out), 300 renders, ms per frame:

| | 3D (CPU submit) | read-back | total |
|---|---|---|---|
| fenced | 2.3–3.0 | 0.5–0.6 | 2.8–3.6 |
| sync | 2.3–4.1 | 1.1–2.0 | 3.4–6.1 |

Chrome's timer is coarse (0.1 ms), hence the ranges. The CPU tile drawing (`sys24_tile.c`, 1.2–2 ms natively) is not in these numbers and will be the biggest item in wasm.

## Open issues

- **Chrome performance warning.** Every fenced frame Chrome logs: *"READ-usage buffer was written, then fenced, but written again before being read back. This discarded the shadow copy that was created to accelerate readback."* (It stops after 256.) The pictures are right and no stall is reported, but Chrome's read-back shadow copy is not being used. It does not go away with the read reordered or with three buffers. Supermodel's shim uses the same pattern, so it probably logs the same thing; this was not looked into further.
- **Stale LUTs during palette changes (upstream).** `model2_palette_state_sig()` (`model2_geo_tex.c`) samples colorxlat and lumaram sparsely, so cached LUTs can lag a palette change. This shows during the practice start fade (frame 300: ~97k pixels off by a level or two), and the picture then depends on history, e.g. after a state load. The old renderer had the same cache.
  - `model2_geo_gl_set_lut_per_frame(1)` rebuilds the LUTs every frame. The picture then depends only on the frame's state, for ~0.3 ms a frame natively. It is off by default so the native look does not change.
  - The real fix is a complete signature in `model2_geo_tex.c` (core-owned).
- **Stale HUD tiles (viewer behavior, kept by default).** The viewer's tile signature samples tile map, palette and char RAM, so the timer's hundredths digit is often stale. Exact tiles fix it, but cost ~3 ms more a frame natively (more in wasm), almost all inside `sys24_tile.c`, which rebuilds every layer pixmap whenever any map byte changes. A dirty-tile refresh there (core-owned) would make exact tiles cheap; then exact should be the default. sys24_tile's own cache also samples char RAM, so in exact mode `video.c` calls `sys24_tile_refresh` when char RAM changes.
- **Windows and Linux are untested.**
  - The Windows loader (`model2_gl.h`/`.c`) now resolves every entry point past GL 1.1, with fences optional. It only passed a syntax check (clang `-target x86_64-pc-windows-gnu` with stand-in `windows.h`/`GL/gl.h`); that check caught and fixed a `__int64` typedef.
  - Linux uses `GL_GLEXT_PROTOTYPES` + `<GL/glext.h>`. The desktop path needs GL 2.1 + ARB_framebuffer_object (packed depth/stencil).
- **16:9 mode** (`--widescreen`). The 3D target stays 496x384 and is stretched over the 16:9 viewport, so horizontal resolution is lower there; `model2_geo_gl_set_target_size` could follow the aspect.
- **`I960_GEO_FLAT=1`.** The unpatched build did not get past window creation in that mode within 90 s in my run, so there is nothing to compare against. The port draws it.
- **Not ported (core-owned files, debug only).**
  - The viewer's untextured fallback (`draw_geo_layer` with `glPolygonMode`/`glPointSize`/`glBegin`) and `model2_geo_render_draw_gl`.
  - `sys24_viewer.c` itself is still fixed-function and is not meant for the web build.

## How the native check of video.c was wired (scratch only, not shipped)

`src/host/val_video.c` in the scratch clone `#include`s `shim/video.c` with `SRALLY_HAVE_GL`. The hook in `sys24_viewer_flip`, right after `sys24_viewer_poll_events`, does this:

1. On the first frame (`VAL_VIDEO=1`), call `srally_video_init(496, 384)` on the SDL GL context, plus the mode setters from env (`VAL_VIDEO_READBACK=sync`, `VAL_EXACT_TILES`, `VAL_CALLER`, the decode mode).
2. Every frame:
   - with `VAL_CALLER`, the shim's `DecodeFrame` (clear on entering 2D-only, else `model2_geo_decode`);
   - `srally_video_invalidate()` at `VAL_INVALIDATE_AT`;
   - `srally_video_render()`.
3. Take `srally_video_pixels()`, dump it as PNG via `sys24_write_png_rgb32` (RGBA → `0xAARRGGBB`), and show it with `glDrawPixels` scaled to the window.
4. `SDL_GL_SwapWindow`, then return, skipping the viewer's own composite.

The binding the core needs is already in `shim/libretro.c` (above). The SDL viewer is not part of the core.

The WebGL2 replay (scratch) stubs the machine side (emulated RAM reads, the tile layer draw, the geometry lock) with a captured frame's data. It links `video.c` + `model2_geo_gl.c` + `model2_gl.c` with Emscripten (`-sMAX_WEBGL_VERSION=2 -sMIN_WEBGL_VERSION=2`) and runs them in a headless Chrome driven over CDP.
