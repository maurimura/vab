/* Sega Rally Championship's picture for the libretro core: the System 24 tile
 * layers and the Model 2 3D layer composited into one 496x384 frame on the GPU
 * (OpenGL ES 3.0 / WebGL2, or desktop GL 2.1 natively), read back to CPU RGBA.
 *
 * Builds:
 *   SRALLY_HAVE_GL + I960_HOST_HAVE_GL   the real thing. Under Emscripten the
 *                                        renderer uses <GLES3/gl3.h>; natively
 *                                        desktop GL through model2_gl.h.
 *   neither                              no GL (headless Node): render is a
 *                                        no-op and pixels() is a black frame.
 *
 * All calls on the thread that owns the GL context. See model2/srally/video-notes.md.
 */
#ifndef SRALLY_VIDEO_H
#define SRALLY_VIDEO_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SRALLY_VIDEO_WIDTH 496
#define SRALLY_VIDEO_HEIGHT 384

/* A GL context is current (Emscripten: the module's canvas; native: SDL's).
 * width x height is the picture: 496x384 is the arcade's; an integer multiple
 * renders the 3D sharper (tiles scale nearest). 0 on success. */
int srally_video_init(int width, int height);
void srally_video_shutdown(void);

/* After a game frame (once per geo_vsync_wait; in the libretro shim after
 * DecodeFrame, only for frames the frontend shows): the tile layers under and
 * over the 3D layer into one picture, in MAME's model2_v composite order (the
 * viewer's), and the read-back started. By default it only draws (it reads
 * emulated RAM, changes nothing): see srally_video_set_decode. */
void srally_video_render(void);

/* The latest finished picture: RGBA8, top-down, width*height*4 bytes (alpha
 * 255), valid until the next render. NULL before the first. With the fenced
 * read-back (the default) it is the previous render's picture. A build
 * without GL returns a black frame. */
const uint8_t *srally_video_pixels(void);

/* ---- Additions to the interface ---- */

/* Emscripten only: make the WebGL2 context on Module.canvas (the worker's
 * OffscreenCanvas) with the attributes this renderer wants and make it
 * current. Needs -sEXPORTED_RUNTIME_METHODS=specialHTMLTargets. 0 on success;
 * -1 elsewhere (natively SDL makes the context). Optional: a shim that makes
 * its own context can skip it. */
int srally_video_create_context(void);

/* Read-back: SRALLY_VIDEO_READBACK_FENCED (default) reads each picture into a
 * pixel-pack buffer behind a fence and hands it out a render later, once the
 * fence has signalled (no GPU stall); SRALLY_VIDEO_READBACK_SYNC reads the
 * current picture with a blocking glReadPixels. Without fences (desktop GL
 * lacking ARB_sync) it is always SYNC. */
#define SRALLY_VIDEO_READBACK_FENCED 0
#define SRALLY_VIDEO_READBACK_SYNC 1
void srally_video_set_readback(int mode);

/* Who runs the frame's geometry decode. That is machine state (the decoder's
 * mesh), so a core whose machine must be the same whether a frame is drawn or
 * not does it itself, every frame:
 *   SRALLY_VIDEO_DECODE_CALLER (default)  render only draws. The caller has
 *       already done, this frame: on entering a 2D-only screen
 *       model2_geo_clear(), else model2_geo_decode() (the libretro shim's
 *       DecodeFrame);
 *   SRALLY_VIDEO_DECODE_SYNC   render does that itself first (synchronous
 *       model2_geo_decode: on this thread without the decode worker, else it
 *       waits for the worker);
 *   SRALLY_VIDEO_DECODE_KICK   render does it the SDL viewer's way: clear,
 *       then model2_geo_kick() and draw the latest mesh the worker published
 *       (usually the previous frame's; without a worker, never updated). */
#define SRALLY_VIDEO_DECODE_CALLER 0
#define SRALLY_VIDEO_DECODE_SYNC 1
#define SRALLY_VIDEO_DECODE_KICK 2
void srally_video_set_decode(int mode);

/* When the tile layers are redrawn. 0 (default): when the viewer's signature
 * changes (viewer_tile_sig: samples of tile map, palette and char RAM), as the
 * SDL viewer, which misses some changes: the HUD timer's hundredths digit is
 * often a frame or more behind. 1: on an exact compare of tile map, palette
 * RAM, char RAM and main mode; the tiles are then the current RAM's, but in a
 * race they change every frame and each redraw rebuilds sys24_tile's layers
 * (~3 ms more a frame natively, see video-notes.md). */
void srally_video_set_exact_tiles(int on);

/* After a state load or a reset (the machine changed under the renderer):
 * the tile layers are redrawn and the palette LUTs and texture sheets rebuilt
 * at the next render, and pictures still in flight are never handed out (the
 * last one handed out stays until a new one is ready). */
void srally_video_invalidate(void);

/* Microseconds spent since the last call, as JSON:
 * {"frames":n,"decode":..,"tiles":..,"geo":..,"composite":..,"readback":..,"total":..,"late":n}
 * (CPU time on the calling thread: decode is the geometry decode, or the
 * kick; a sync read-back includes the GPU wait). late counts fenced
 * read-backs that were not done by the next render (that picture is skipped
 * and the one before it stays). */
const char *srally_video_timings(void);

#ifdef __cplusplus
}
#endif

#endif /* SRALLY_VIDEO_H */
