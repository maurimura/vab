/* Stand-in for the renderer (video.c, ported separately to WebGL2) until its patch to
 * model2_geo_gl.c is in patches/: the same entry points, no GL; the picture is the System 24
 * tile layers drawn on the CPU (sys24_tile.c), all of them over black, without the 3D layer.
 * core.mk links this instead of video.c then. */
#include "video_api.h"
#include "sys24_tile.h"
#include "model2_rom.h"

#include <string.h>

static sys24_tile_state_t *s_tile;
static uint32_t s_bitmap[SRALLY_VIDEO_WIDTH * SRALLY_VIDEO_HEIGHT];
static uint8_t s_rgba[SRALLY_VIDEO_WIDTH * SRALLY_VIDEO_HEIGHT * 4];
static int s_have;

int srally_video_init(int width, int height)
{
  (void)width;
  (void)height;
  if (!s_tile) s_tile = sys24_tile_create(SYS24_TILE_MASK_M2);
  return s_tile ? 0 : -1;
}

void srally_video_shutdown(void)
{
  if (s_tile) sys24_tile_destroy(s_tile);
  s_tile = NULL;
  s_have = 0;
}

void srally_video_render(void)
{
  if (!s_tile) return;
  sys24_tile_bind(s_tile, model2_tile_map_ptr(), model2_tile_char_ptr());
  sys24_tile_draw_layers_rgb32(s_tile, s_bitmap, model2_palram_ptr(), 0xff000000u, SYS24_PASS_ALL);
  for (unsigned i = 0; i < SRALLY_VIDEO_WIDTH * SRALLY_VIDEO_HEIGHT; i++)
  {
    const uint32_t v = s_bitmap[i]; /* 0xAARRGGBB */
    s_rgba[i * 4] = (uint8_t)(v >> 16);
    s_rgba[i * 4 + 1] = (uint8_t)(v >> 8);
    s_rgba[i * 4 + 2] = (uint8_t)v;
    s_rgba[i * 4 + 3] = 0xff;
  }
  s_have = 1;
}

const uint8_t *srally_video_pixels(void) { return s_have ? s_rgba : NULL; }

const char *srally_video_timings(void) { return "{}"; }
