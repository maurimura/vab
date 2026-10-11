/* Sega Rally Championship's picture for the libretro core (see video.h).
 *
 * What src/host/sys24_viewer.c's flip does, without SDL and without the
 * window: the System 24 tile layers drawn on the CPU by sys24_tile.c into two
 * 496x384 bitmaps (the even layers, under the polygons; the odd "priority"
 * layers, over them), the 3D layer drawn by the patched model2_geo_gl.c into
 * its own framebuffer, the three composited in one pass into a 496x384
 * framebuffer, and that read back to RGBA8. MAME model2_v screen_update order:
 *
 *   black → even tilemaps (pen 0 / black transparent) → polygons
 *         → odd tilemaps (pen 0 / black transparent)
 *
 * except the splash holds, the operator menu and the START banner, which are
 * tiles only (all layers, black opaque, no 3D).
 */

#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200112L /* clock_gettime (and snprintf on macOS) under -std=c99 */
#endif

#include "video.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static int s_width = SRALLY_VIDEO_WIDTH;
static int s_height = SRALLY_VIDEO_HEIGHT;
static uint8_t *s_pixels;    /* the picture handed out */
static int s_have_picture;
static int s_readback_mode = SRALLY_VIDEO_READBACK_FENCED;
static int s_decode = SRALLY_VIDEO_DECODE_CALLER;
static int s_exact_tiles_wanted;

/* Timing sums (µs) since the last srally_video_timings(). */
enum { T_DECODE, T_TILES, T_GEO, T_COMPOSITE, T_READBACK, T_TOTAL, T_COUNT };
static double s_sum[T_COUNT];
static unsigned long s_sum_frames;
static unsigned long s_late; /* fenced read-backs not done by the next render */

void srally_video_set_readback(int mode)
{
    s_readback_mode = mode == SRALLY_VIDEO_READBACK_SYNC ? SRALLY_VIDEO_READBACK_SYNC
                                                          : SRALLY_VIDEO_READBACK_FENCED;
}

void srally_video_set_exact_tiles(int on)
{
    s_exact_tiles_wanted = on ? 1 : 0;
}

void srally_video_set_decode(int mode)
{
    s_decode = (mode == SRALLY_VIDEO_DECODE_SYNC || mode == SRALLY_VIDEO_DECODE_KICK)
                   ? mode
                   : SRALLY_VIDEO_DECODE_CALLER;
}

const char *srally_video_timings(void)
{
    static char text[256];

    snprintf(text, sizeof(text),
             "{\"frames\":%lu,\"decode\":%.0f,\"tiles\":%.0f,\"geo\":%.0f,"
             "\"composite\":%.0f,\"readback\":%.0f,\"total\":%.0f,\"late\":%lu}",
             s_sum_frames, s_sum[T_DECODE], s_sum[T_TILES], s_sum[T_GEO],
             s_sum[T_COMPOSITE], s_sum[T_READBACK], s_sum[T_TOTAL], s_late);
    memset(s_sum, 0, sizeof(s_sum));
    s_sum_frames = 0;
    s_late = 0;
    return text;
}

#ifndef SRALLY_HAVE_GL

/* ---- No GL (headless Node): black frames ---- */

int srally_video_init(int width, int height)
{
    size_t n;
    size_t i;

    if (width < 1 || height < 1)
        return -1;
    s_width = width;
    s_height = height;
    n = (size_t)s_width * (size_t)s_height * 4u;
    free(s_pixels);
    s_pixels = (uint8_t *)calloc(n, 1);
    if (!s_pixels)
        return -1;
    for (i = 3; i < n; i += 4)
        s_pixels[i] = 0xffu;
    s_have_picture = 1;
    return 0;
}

void srally_video_shutdown(void)
{
    free(s_pixels);
    s_pixels = NULL;
    s_have_picture = 0;
}

void srally_video_render(void)
{
    (void)s_readback_mode;
    (void)s_decode;
    (void)s_exact_tiles_wanted;
}

void srally_video_invalidate(void)
{
}

const uint8_t *srally_video_pixels(void)
{
    return s_have_picture ? s_pixels : NULL;
}

int srally_video_create_context(void)
{
    return -1;
}

#else /* SRALLY_HAVE_GL */

#ifndef I960_HOST_HAVE_GL
#error "SRALLY_HAVE_GL needs I960_HOST_HAVE_GL (the recomp's GL renderer) too"
#endif

#include "i960_lift.h"
#include "i960_mem.h"
#include "lift_log.h"
#include "model2_geo.h"
#include "model2_geo_gl.h"
#include "model2_gl.h"
#include "model2_rom.h"
#include "sys24_tile.h"

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#include <emscripten/html5.h>
#endif

static int s_ready;
static sys24_tile_state_t *s_tile;
static u32 *s_bottom;   /* even layers — under polygons (host 0xAARRGGBB) */
static u32 *s_priority; /* odd layers — over polygons */
static u32 s_tile_sig;
static int s_exact_tiles;
static u8 *s_seen_map;  /* exact tiles: copies of what they were last drawn from */
static u8 *s_seen_char;
static u8 *s_seen_pal;
static u32 s_seen_mode;
static int s_tile_valid;
static int s_was_opaque2d;

static GLuint s_tex_bottom;
static GLuint s_tex_priority;
static GLuint s_fbo;
static GLuint s_fbo_tex;
static GLuint s_prog;
static GLint s_loc_size = -1;
static GLint s_loc_opaque = -1;
static GLint s_loc_geo_on = -1;
static GLuint s_vbo_quad;
#ifndef SRALLY_VIDEO_PBOS
#define SRALLY_VIDEO_PBOS 2 /* pictures in flight: handed out PBOS-1 renders late */
#endif
static GLuint s_pbo[SRALLY_VIDEO_PBOS];
static GLsync s_fence[SRALLY_VIDEO_PBOS];
static unsigned s_frame_no;

static double now_us(void)
{
    struct timespec ts;

    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1e6 + (double)ts.tv_nsec / 1e3;
}

/* ---- Game state the composite depends on (sys24_viewer.c) ---- */

static int is_splash_hold(u32 inner, u32 frame)
{
    /*
     * Splash bind seeds a negative hold in 0x20a808:
     *   inner2 → 0xfffffe5c (−420), inner4/6 → 0xffffff88 (−120).
     * Hold while signed frame < 0.
     */
    return (inner == 2u || inner == 4u || inner == 6u) && (i32)frame < 0;
}

/* Operator menu (main mode 4) is pure Sys24 tiles — no 3D underlay. */
static int is_test_menu(void)
{
    return i960_ld_u32(I960_WORKRAM, 0x202098, 0) == 4u;
}

/* Game-start (main mode 3) and attract share MAME model2_v composite order. */
static int is_game_start(void)
{
    return i960_ld_u32(I960_WORKRAM, 0x202098, 0) == 3u;
}

/* attract_hud_setup @ 0x1B940 / staged 0x5BA940 — START CGM, Sys24 only. */
static int is_start_banner(void)
{
    u32 frame;
    const u32 tables[2] = { 0x005ba820u, 0x005ba880u }; /* champ / practice */
    int t;

    if (!is_game_start())
        return 0;

    frame = i960_ld_u32(I960_WORKRAM, 0x2020ac, 0) & 15u;
    for (t = 0; t < 2; t++) {
        u32 handler = i960_ld_u32(I960_WORKRAM, tables[t], frame << 2);

        if (handler == 0u)
            handler = model2_workram_mirror_u32(tables[t] + (frame << 2));
        if (handler == 0x005ba940u || handler == 0x0001b940u)
            return 1;
        /* Staged callx: 0x5A0000 + (rom - 0x1000). */
        if (handler >= 0x005a0000u && handler < 0x005c0000u
            && (0x1000u + (handler - 0x005a0000u)) == 0x0001b940u)
            return 1;
    }
    return 0;
}

static int tiles_opaque(u32 inner, u32 frame)
{
    return is_splash_hold(inner, frame) || is_test_menu() || is_start_banner();
}

/* The viewer's tile signature (viewer_tile_sig): samples of the tile map,
 * palram and char RAM, the layer registers and the main mode. */
static u32 tile_sig(const u8 *tile_map, const u8 *char_ram, const u8 *palram)
{
    u32 h = 2166136261u;
    unsigned i;

    for (i = 0; i < 65536u; i += 8u)
        h = (h ^ tile_map[i]) * 16777619u;
    for (i = 0; i < 16384u; i += 4u)
        h = (h ^ palram[i]) * 16777619u;
    if (char_ram) {
        for (i = 0; i < 4096u; i += 16u)
            h = (h ^ char_ram[i]) * 16777619u;
    }
    /* Layer scroll/regs written by boot_tile_splash_frame (redraw, not rebuild). */
    h ^= i960_ld_u32(I960_WORKRAM, 0x20b910, 0);
    h ^= i960_ld_u32(I960_WORKRAM, 0x20b914, 0);
    h ^= i960_ld_u32(I960_WORKRAM, 0x20b918, 0);
    h ^= i960_ld_u32(I960_WORKRAM, 0x20b91c, 0);
    h ^= i960_ld_u32(I960_WORKRAM, 0x202098, 0); /* main mode — select composite */
    return h;
}

/*
 * 1 when `cur` differs from the copy in `seen` (then the copy is updated).
 * Word compares: this runs every frame over ~600 KB.
 */
static int changed(u8 *seen, const u8 *cur, size_t n)
{
    size_t i;

    for (i = 0; i + 8u <= n; i += 8u) {
        uint64_t a, b;

        memcpy(&a, seen + i, 8);
        memcpy(&b, cur + i, 8);
        if (a != b) {
            memcpy(seen, cur, n);
            return 1;
        }
    }
    return 0;
}

/*
 * srally_video_set_exact_tiles(1): whether the tile layers must be redrawn,
 * by an exact compare of what they are drawn from (tile map with its
 * scroll/control registers, palette RAM, char RAM, and the main mode that
 * selects the composite). The viewer's sampled signature misses changes,
 * e.g. the HUD timer's hundredths digit, and shows the old tiles until some
 * sampled byte changes; with this the picture depends only on the current
 * RAM. *char_changed: char RAM changed, which sys24_tile's own layer cache
 * (it samples char RAM) must be told about.
 */
static int tiles_changed(const u8 *tile_map, const u8 *char_ram, const u8 *palram,
                         int *char_changed)
{
    int any = 0;
    u32 mode = i960_ld_u32(I960_WORKRAM, 0x202098, 0);

    *char_changed = 0;
    if (changed(s_seen_map, tile_map, MODEL2_TILE_MAP_SIZE))
        any = 1;
    if (changed(s_seen_pal, palram, MODEL2_PALRAM_SIZE))
        any = 1;
    if (char_ram && changed(s_seen_char, char_ram, MODEL2_TILE_CHAR_SIZE)) {
        any = 1;
        *char_changed = 1;
    }
    if (mode != s_seen_mode) {
        s_seen_mode = mode;
        any = 1;
    }
    return any;
}

/* The tile layers into s_bottom / s_priority, as the viewer's flip. */
static void draw_tiles(const u8 *tile_map, const u8 *char_ram, const u8 *palram,
                       int opaque2d, int char_changed)
{
    u8 *reg = NULL;
    u16 ctrl_save = 0;
    int strip_ranking = 0;

    sys24_tile_bind(s_tile, tile_map, char_ram);
    if (char_changed)
        sys24_tile_refresh(s_tile); /* its layer cache samples char RAM */
    /*
     * Attract ranking leaves ctrl bit14 (0x4000) in 0x20b91c → tile_ram[0x5004]
     * (pairs 0–1 only). game_start_* only clrbit15, so special-window stays
     * armed and MAME segs24 skips odd layers of those pairs. Strip bit14 on
     * game-start (restored right after drawing).
     */
    if (is_game_start()) {
        reg = model2_ram_mut(0x0100a008u);
        if (reg) {
            u16 ctrl = (u16)reg[0] | ((u16)reg[1] << 8);

            ctrl_save = ctrl;
            if (ctrl & 0x4000u) {
                ctrl = (u16)(ctrl & ~0x4000u);
                reg[0] = (u8)ctrl;
                reg[1] = (u8)(ctrl >> 8);
                strip_ranking = 1;
            }
        }
    }
    if (opaque2d) {
        /* Splash/test: single opaque ALL-layer pass. */
        sys24_tile_draw_layers_rgb32(s_tile, s_bottom, palram, 0xff000000u,
                                     SYS24_PASS_ALL);
    } else {
        /* Even layers under polygons, odd over; transparent clear (0). */
        sys24_tile_draw_layers_rgb32(s_tile, s_bottom, palram, 0x00000000u,
                                     SYS24_PASS_BOTTOM);
        sys24_tile_draw_layers_rgb32(s_tile, s_priority, palram, 0x00000000u,
                                     SYS24_PASS_PRIORITY);
    }
    if (strip_ranking && reg) {
        reg[0] = (u8)ctrl_save;
        reg[1] = (u8)(ctrl_save >> 8);
    }
}

/* ---- GL ---- */

#ifdef MODEL2_GL_ES
static const char k_vs_head[] = "#version 300 es\nprecision highp float;\n#define ATTR in\n";
static const char k_fs_head[] =
    "#version 300 es\n"
    "precision highp float;\n"
    "precision highp sampler2D;\n"
    "#define TEX2D texture\n"
    "out vec4 m2_frag;\n"
    "#define FRAG m2_frag\n";
#else
static const char k_vs_head[] = "#version 120\n#define ATTR attribute\n";
static const char k_fs_head[] =
    "#version 120\n"
    "#define TEX2D texture2D\n"
    "#define FRAG gl_FragColor\n";
#endif

static const char k_vs[] =
    "ATTR vec2 aPos;\n"
    "void main() { gl_Position = vec4(aPos, 0.0, 1.0); }\n";

/*
 * One pass. Framebuffer row y (what glReadPixels returns as row y) is picture
 * row y from the top, so the read-back comes out top-down; the tile bitmaps
 * (uploaded row 0 first) and the 3D target (model2_geo_gl renders it
 * top-down) are sampled at the same coordinate. Tile texels are the host
 * words 0xAARRGGBB as bytes B,G,R,A (hence .bgr); a tile pixel is transparent
 * when its colour is black (the viewer's punch: pen 0 / clear,
 * copybitmap_trans key 0). 3D alpha is 1 where drawn, else 0.
 */
static const char k_fs[] =
    "uniform sampler2D uBottom;\n"
    "uniform sampler2D uPriority;\n"
    "uniform sampler2D uGeo;\n"
    "uniform vec2 uSize;\n"
    "uniform float uOpaque;\n"
    "uniform float uGeoOn;\n"
    "void main() {\n"
    "  vec2 p = gl_FragCoord.xy / uSize;\n"
    "  vec4 b = TEX2D(uBottom, p);\n"
    "  vec3 c = vec3(0.0);\n"
    "  if (uOpaque > 0.5) {\n"
    "    c = b.bgr;\n"
    "  } else {\n"
    "    vec4 o = TEX2D(uPriority, p);\n"
    "    vec4 g = TEX2D(uGeo, p);\n"
    "    if (max(max(b.r, b.g), b.b) > 0.0) c = b.bgr;\n"
    "    if (uGeoOn > 0.5 && g.a > 0.5) c = g.rgb;\n"
    "    if (max(max(o.r, o.g), o.b) > 0.0) c = o.bgr;\n"
    "  }\n"
    "  FRAG = vec4(c, 1.0);\n"
    "}\n";

static GLuint compile(GLenum type, const char *body)
{
    GLuint s = glCreateShader(type);
    const GLchar *parts[2];
    GLint ok = 0;

    parts[0] = type == GL_VERTEX_SHADER ? k_vs_head : k_fs_head;
    parts[1] = body;
    glShaderSource(s, 2, parts, NULL);
    glCompileShader(s);
    glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[512];

        glGetShaderInfoLog(s, (GLsizei)sizeof(log), NULL, log);
        fprintf(stderr, "srally video: shader compile: %s\n", log);
        glDeleteShader(s);
        return 0;
    }
    return s;
}

static int build_program(void)
{
    GLuint v = compile(GL_VERTEX_SHADER, k_vs);
    GLuint f = compile(GL_FRAGMENT_SHADER, k_fs);
    GLint ok = 0;

    if (!v || !f) {
        if (v)
            glDeleteShader(v);
        if (f)
            glDeleteShader(f);
        return -1;
    }
    s_prog = glCreateProgram();
    glAttachShader(s_prog, v);
    glAttachShader(s_prog, f);
    glBindAttribLocation(s_prog, 0, "aPos");
    glLinkProgram(s_prog);
    glDeleteShader(v);
    glDeleteShader(f);
    glGetProgramiv(s_prog, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[512];

        glGetProgramInfoLog(s_prog, (GLsizei)sizeof(log), NULL, log);
        fprintf(stderr, "srally video: shader link: %s\n", log);
        glDeleteProgram(s_prog);
        s_prog = 0;
        return -1;
    }
    glUseProgram(s_prog);
    glUniform1i(glGetUniformLocation(s_prog, "uBottom"), 0);
    glUniform1i(glGetUniformLocation(s_prog, "uPriority"), 1);
    glUniform1i(glGetUniformLocation(s_prog, "uGeo"), 2);
    s_loc_size = glGetUniformLocation(s_prog, "uSize");
    s_loc_opaque = glGetUniformLocation(s_prog, "uOpaque");
    s_loc_geo_on = glGetUniformLocation(s_prog, "uGeoOn");
    glUseProgram(0);
    return 0;
}

static GLuint make_texture(int w, int h)
{
    GLuint tex = 0;

    glGenTextures(1, &tex);
    glBindTexture(GL_TEXTURE_2D, tex);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    glBindTexture(GL_TEXTURE_2D, 0);
    return tex;
}

int srally_video_init(int width, int height)
{
    static const float quad[8] = { -1.f, -1.f, 1.f, -1.f, -1.f, 1.f, 1.f, 1.f };
    size_t fb_bytes = (size_t)SYS24_FB_WIDTH * (size_t)SYS24_FB_HEIGHT * sizeof(u32);
    size_t i;
    GLenum status;

    if (s_ready)
        srally_video_shutdown();
    if (width < 1 || height < 1)
        return -1;
    s_width = width;
    s_height = height;
    if (!model2_gl_load())
        return -1;

    s_tile = sys24_tile_create(SYS24_TILE_MASK_M2);
    s_bottom = (u32 *)calloc(1, fb_bytes);
    s_priority = (u32 *)calloc(1, fb_bytes);
    s_pixels = (uint8_t *)malloc((size_t)width * (size_t)height * 4u);
    if (!s_tile || !s_bottom || !s_priority || !s_pixels)
        goto fail;
    for (i = 0; i < (size_t)width * (size_t)height * 4u; i += 4) {
        s_pixels[i] = s_pixels[i + 1] = s_pixels[i + 2] = 0;
        s_pixels[i + 3] = 0xffu;
    }
    if (build_program() != 0)
        goto fail;

    s_tex_bottom = make_texture(SYS24_FB_WIDTH, SYS24_FB_HEIGHT);
    s_tex_priority = make_texture(SYS24_FB_WIDTH, SYS24_FB_HEIGHT);
    s_fbo_tex = make_texture(width, height);
    glGenFramebuffers(1, &s_fbo);
    glBindFramebuffer(GL_FRAMEBUFFER, s_fbo);
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, s_fbo_tex,
                           0);
    status = glCheckFramebufferStatus(GL_FRAMEBUFFER);
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    if (status != GL_FRAMEBUFFER_COMPLETE) {
        fprintf(stderr, "srally video: composite framebuffer incomplete (0x%x)\n",
                (unsigned)status);
        goto fail;
    }

    glGenBuffers(1, &s_vbo_quad);
    glBindBuffer(GL_ARRAY_BUFFER, s_vbo_quad);
    glBufferData(GL_ARRAY_BUFFER, (GLsizeiptr)sizeof(quad), quad, GL_STATIC_DRAW);
    glBindBuffer(GL_ARRAY_BUFFER, 0);

    glGenBuffers(SRALLY_VIDEO_PBOS, s_pbo);
    for (i = 0; i < SRALLY_VIDEO_PBOS; i++) {
        glBindBuffer(GL_PIXEL_PACK_BUFFER, s_pbo[i]);
        glBufferData(GL_PIXEL_PACK_BUFFER, (GLsizeiptr)((size_t)width * height * 4u), NULL,
                     GL_STREAM_READ);
    }
    glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);

    model2_geo_gl_set_target_size(width, height);
    s_tile_valid = 0;
    s_was_opaque2d = 0;
    s_have_picture = 0;
    s_frame_no = 0;
    s_ready = 1;
    lift_log("srally video: %dx%d composite, %s read-back\n", width, height,
             (s_readback_mode == SRALLY_VIDEO_READBACK_FENCED && model2_gl_have_sync())
                 ? "fenced"
                 : "synchronous");
    return 0;

fail:
    srally_video_shutdown();
    return -1;
}

void srally_video_shutdown(void)
{
    unsigned i;

    for (i = 0; i < SRALLY_VIDEO_PBOS; i++) {
        if (s_fence[i])
            glDeleteSync(s_fence[i]);
        s_fence[i] = 0;
    }
    if (s_pbo[0])
        glDeleteBuffers(SRALLY_VIDEO_PBOS, s_pbo);
    memset(s_pbo, 0, sizeof(s_pbo));
    if (s_vbo_quad)
        glDeleteBuffers(1, &s_vbo_quad);
    s_vbo_quad = 0;
    if (s_fbo)
        glDeleteFramebuffers(1, &s_fbo);
    s_fbo = 0;
    if (s_fbo_tex)
        glDeleteTextures(1, &s_fbo_tex);
    if (s_tex_bottom)
        glDeleteTextures(1, &s_tex_bottom);
    if (s_tex_priority)
        glDeleteTextures(1, &s_tex_priority);
    s_fbo_tex = s_tex_bottom = s_tex_priority = 0;
    if (s_prog)
        glDeleteProgram(s_prog);
    s_prog = 0;
    if (s_tile)
        sys24_tile_destroy(s_tile);
    s_tile = NULL;
    free(s_bottom);
    free(s_priority);
    free(s_seen_map);
    free(s_seen_char);
    free(s_seen_pal);
    free(s_pixels);
    s_seen_map = s_seen_char = s_seen_pal = NULL;
    s_bottom = s_priority = NULL;
    s_pixels = NULL;
    s_have_picture = 0;
    s_ready = 0;
}

static void upload_tiles(int opaque2d)
{
    glPixelStorei(GL_UNPACK_ALIGNMENT, 4);
    glBindTexture(GL_TEXTURE_2D, s_tex_bottom);
    glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, SYS24_FB_WIDTH, SYS24_FB_HEIGHT, GL_RGBA,
                    GL_UNSIGNED_BYTE, s_bottom);
    if (!opaque2d) {
        glBindTexture(GL_TEXTURE_2D, s_tex_priority);
        glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, SYS24_FB_WIDTH, SYS24_FB_HEIGHT, GL_RGBA,
                        GL_UNSIGNED_BYTE, s_priority);
    }
    glBindTexture(GL_TEXTURE_2D, 0);
}

/* The 3D layer into model2_geo_gl's target; its texture, or 0 for none. */
static GLuint render_geo(void)
{
    const float *xyzuv = NULL;
    const model2_geo_tri_mat_t *mats = NULL;
    unsigned nverts = 0, ntris = 0;
    model2_geo_projection_t proj;
    float mvp[16];
    GLuint tex = 0;

    /* Without GEO 0x09/0x03 projection yet: identity (no invented camera). */
    if (model2_geo_projection(&proj)) {
        model2_geo_gl_projection_matrix(&proj, 0.01f, mvp);
    } else {
        memset(mvp, 0, sizeof(mvp));
        mvp[0] = mvp[5] = mvp[10] = mvp[15] = 1.f;
    }
    model2_geo_gl_set_mvp(mvp);
    if (model2_geo_lock_textured(&xyzuv, &nverts, &mats, &ntris) == 0) {
        if (xyzuv && mats && ntris > 0u)
            tex = model2_geo_gl_render_offscreen(xyzuv, nverts, mats, ntris);
        model2_geo_unlock();
    }
    return tex;
}

static void composite(int opaque2d, GLuint geo_tex)
{
    glBindFramebuffer(GL_FRAMEBUFFER, s_fbo);
    glViewport(0, 0, s_width, s_height);
    glDisable(GL_DEPTH_TEST);
    glDisable(GL_STENCIL_TEST);
    glDisable(GL_BLEND);
    glDisable(GL_CULL_FACE);
    glDisable(GL_SCISSOR_TEST);
    glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);

    glUseProgram(s_prog);
    if (s_loc_size >= 0)
        glUniform2f(s_loc_size, (float)s_width, (float)s_height);
    if (s_loc_opaque >= 0)
        glUniform1f(s_loc_opaque, opaque2d ? 1.f : 0.f);
    if (s_loc_geo_on >= 0)
        glUniform1f(s_loc_geo_on, geo_tex ? 1.f : 0.f);
    glActiveTexture(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, s_tex_bottom);
    glActiveTexture(GL_TEXTURE1);
    glBindTexture(GL_TEXTURE_2D, s_tex_priority);
    glActiveTexture(GL_TEXTURE2);
    glBindTexture(GL_TEXTURE_2D, geo_tex ? geo_tex : s_tex_bottom);

    glBindBuffer(GL_ARRAY_BUFFER, s_vbo_quad);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 0, (const void *)0);
    glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
    glDisableVertexAttribArray(0);
    glBindBuffer(GL_ARRAY_BUFFER, 0);

    glBindTexture(GL_TEXTURE_2D, 0);
    glActiveTexture(GL_TEXTURE1);
    glBindTexture(GL_TEXTURE_2D, 0);
    glActiveTexture(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, 0);
    glUseProgram(0);
}

/* With the composite framebuffer bound. */
static void read_back(void)
{
    GLsizeiptr bytes = (GLsizeiptr)((size_t)s_width * (size_t)s_height * 4u);
    unsigned now, before;

    glPixelStorei(GL_PACK_ALIGNMENT, 4);
    if (s_readback_mode == SRALLY_VIDEO_READBACK_SYNC || !model2_gl_have_sync()) {
        glReadPixels(0, 0, s_width, s_height, GL_RGBA, GL_UNSIGNED_BYTE, s_pixels);
        s_have_picture = 1;
        return;
    }
    /*
     * Supermodel's pattern (supermodel/shim/libretro.cpp EndFrameVideo): two
     * pixel-pack buffers; each picture is read into one behind a fence and
     * taken out at the next render once the fence has passed. The previous
     * picture is taken out before this one is started.
     */
    now = s_frame_no % SRALLY_VIDEO_PBOS;
    before = (s_frame_no + 1u) % SRALLY_VIDEO_PBOS; /* the oldest in flight */
    if (s_fence[before]) {
        GLenum state = glClientWaitSync(s_fence[before], 0, 0);

        if (state == GL_ALREADY_SIGNALED || state == GL_CONDITION_SATISFIED) {
            glBindBuffer(GL_PIXEL_PACK_BUFFER, s_pbo[before]);
            glGetBufferSubData(GL_PIXEL_PACK_BUFFER, 0, bytes, s_pixels);
            s_have_picture = 1;
        } else {
            /* Not done yet: keep the picture before it (this one is dropped). */
            s_late++;
        }
        glDeleteSync(s_fence[before]);
        s_fence[before] = 0;
    }
    glBindBuffer(GL_PIXEL_PACK_BUFFER, s_pbo[now]);
    glReadPixels(0, 0, s_width, s_height, GL_RGBA, GL_UNSIGNED_BYTE, (void *)0);
    if (s_fence[now])
        glDeleteSync(s_fence[now]);
    s_fence[now] = glFenceSync(GL_SYNC_GPU_COMMANDS_COMPLETE, 0);
    glFlush();
    s_frame_no++;
    glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);
}

void srally_video_render(void)
{
    const u8 *tile_map;
    const u8 *char_ram;
    const u8 *palram;
    u32 inner, frame;
    int opaque2d;
    GLuint geo_tex = 0;
    double t0, td, t1, t2, t3, t4;

    if (!s_ready)
        return;
    t0 = now_us();
    if (s_exact_tiles != s_exact_tiles_wanted) {
        s_exact_tiles = s_exact_tiles_wanted;
        s_tile_valid = 0;
    }
    tile_map = model2_tile_map_ptr();
    char_ram = model2_tile_char_ptr();
    palram = model2_palram_ptr();
    inner = i960_ld_u32(I960_WORKRAM, 0x20209c, 0);
    frame = i960_ld_u32(I960_WORKRAM, 0x20a808, 0);
    opaque2d = tiles_opaque(inner, frame);

    /*
     * Splash / operator menu are Sys24 tiles only. Drop stale PRG mesh and
     * skip geo decode so transparent black cells do not reveal the track.
     * (SRALLY_VIDEO_DECODE_CALLER: the caller did this, render only draws.)
     */
    if (s_decode != SRALLY_VIDEO_DECODE_CALLER) {
        if (opaque2d && !s_was_opaque2d)
            model2_geo_clear();
        if (!opaque2d) {
            if (s_decode == SRALLY_VIDEO_DECODE_SYNC)
                (void)model2_geo_decode();
            else
                model2_geo_kick();
        }
    }
    if (opaque2d != s_was_opaque2d) {
        s_tile_valid = 0;
        s_was_opaque2d = opaque2d;
    }
    td = now_us();

    if (tile_map && palram) {
        int char_changed = 0;
        int dirty;

        if (s_exact_tiles && !s_seen_map) {
            s_seen_map = (u8 *)calloc(1, MODEL2_TILE_MAP_SIZE);
            s_seen_char = (u8 *)calloc(1, MODEL2_TILE_CHAR_SIZE);
            s_seen_pal = (u8 *)calloc(1, MODEL2_PALRAM_SIZE);
            if (!s_seen_map || !s_seen_char || !s_seen_pal) {
                free(s_seen_map);
                free(s_seen_char);
                free(s_seen_pal);
                s_seen_map = s_seen_char = s_seen_pal = NULL;
                s_exact_tiles = 0;
            }
            s_tile_valid = 0;
        }
        if (s_exact_tiles) {
            dirty = tiles_changed(tile_map, char_ram, palram, &char_changed);
            if (!s_tile_valid)
                char_changed = 1;
        } else {
            u32 sig = tile_sig(tile_map, char_ram, palram);

            dirty = sig != s_tile_sig;
            s_tile_sig = sig;
        }
        if (!s_tile_valid || dirty) {
            draw_tiles(tile_map, char_ram, palram, opaque2d, char_changed);
            upload_tiles(opaque2d);
            s_tile_valid = 1;
        }
    }
    t1 = now_us();

    if (!opaque2d)
        geo_tex = render_geo();
    t2 = now_us();

    composite(opaque2d, geo_tex);
    t3 = now_us();
    read_back();
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    t4 = now_us();

    s_sum[T_DECODE] += td - t0;
    s_sum[T_TILES] += t1 - td;
    s_sum[T_GEO] += t2 - t1;
    s_sum[T_COMPOSITE] += t3 - t2;
    s_sum[T_READBACK] += t4 - t3;
    s_sum[T_TOTAL] += t4 - t0;
    s_sum_frames++;
}

const uint8_t *srally_video_pixels(void)
{
    return s_have_picture ? s_pixels : NULL;
}

void srally_video_invalidate(void)
{
    unsigned i;

    if (!s_ready)
        return;
    /* Tiles redrawn and LUTs/sheets rebuilt from the new RAM at the next
     * render; pictures still in flight are from before: never handed out. */
    s_tile_valid = 0;
    for (i = 0; i < SRALLY_VIDEO_PBOS; i++) {
        if (s_fence[i])
            glDeleteSync(s_fence[i]);
        s_fence[i] = 0;
    }
    model2_geo_gl_invalidate();
}

int srally_video_create_context(void)
{
#ifdef __EMSCRIPTEN__
    EmscriptenWebGLContextAttributes attrs;
    EMSCRIPTEN_WEBGL_CONTEXT_HANDLE context;

    emscripten_webgl_init_context_attributes(&attrs);
    attrs.majorVersion = 2;
    attrs.minorVersion = 0;
    /* Everything is drawn into framebuffer objects with their own depth and
     * stencil; the canvas' drawing buffer is never shown or read. */
    attrs.alpha = 0;
    attrs.depth = 0;
    attrs.stencil = 0;
    attrs.antialias = 0;
    attrs.premultipliedAlpha = 0;
    attrs.preserveDrawingBuffer = 0;
    attrs.powerPreference = EM_WEBGL_POWER_PREFERENCE_HIGH_PERFORMANCE;
    /* The frontend hands the module its canvas (Module.canvas, an
     * OffscreenCanvas in the worker); Emscripten finds canvases by selector or
     * through this table. (A script string, not EM_ASM: that needs -std=gnu*.) */
    {
        char js[256];

        snprintf(js, sizeof(js),
                 "(function(){var c=Module['canvas'];if(c){c.width=%d;c.height=%d;}"
                 "Module['specialHTMLTargets']['!canvas']=c||0;})()",
                 SRALLY_VIDEO_WIDTH, SRALLY_VIDEO_HEIGHT);
        emscripten_run_script(js);
    }
    context = emscripten_webgl_create_context("!canvas", &attrs);
    if (context <= 0) {
        fprintf(stderr, "srally video: no WebGL2 context on the module's canvas (%d)\n",
                (int)context);
        return -1;
    }
    emscripten_webgl_make_context_current(context);
    return 0;
#else
    return -1;
#endif
}

#endif /* SRALLY_HAVE_GL */
