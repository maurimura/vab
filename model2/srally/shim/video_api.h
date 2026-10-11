/* The renderer's entry points as the shim calls them (video.h, from the renderer's port,
 * when it is there; this is the same interface, so the shim and the stand-in build without it).
 *
 *   srally_video_init(w, h)   once at load, with a GL context current (Emscripten: a WebGL2
 *                             context on the module's canvas); 0 on success
 *   srally_video_render()     at the end of a frame the frontend wants to see: the 3D layer
 *                             and the tile layers into a 496x384 picture. The shim has decoded
 *                             the frame's geometry already (model2_geo_decode, every frame,
 *                             shown or not, so the machine is the same either way, and the
 *                             viewer's clear on entering a 2D-only screen); the decode has no
 *                             worker, so a kick does nothing. Render only draws.
 *   srally_video_pixels()     the latest finished picture, RGBA8 496*384*4, top row first, or
 *                             NULL (nothing yet); headless: a black frame
 *   srally_video_shutdown()   at unload
 *   srally_video_invalidate() after a state load or reset: the machine changed under the
 *                             renderer, drop whatever it caches (the shim has a weak no-op)
 *   srally_video_set_decode(SRALLY_VIDEO_DECODE_CALLER)  the shim decodes (video.h) */
#ifndef SRALLY_VIDEO_API_H
#define SRALLY_VIDEO_API_H

#include <stdint.h>

#define SRALLY_VIDEO_WIDTH 496
#define SRALLY_VIDEO_HEIGHT 384

/* video.c's own header when that is the renderer linked (core.mk defines SRALLY_VIDEO_C). */
#if defined(SRALLY_VIDEO_C) && defined(__has_include)
#if __has_include("video.h")
#include "video.h"
#define SRALLY_HAVE_VIDEO_H 1
#endif
#endif

#ifndef SRALLY_HAVE_VIDEO_H
int srally_video_init(int width, int height);
void srally_video_shutdown(void);
void srally_video_render(void);
const uint8_t *srally_video_pixels(void);
#endif

#endif
