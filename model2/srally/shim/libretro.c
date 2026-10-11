/* Sega Rally Championship (Sega Model 2A, MAME set `srallyc`, Revision B program) behind the
 * libretro API, so web/emulator/libretro.js and worker.js drive it like the other cores: load
 * the ROM set, run a frame, read the picture and the sound, save and load the machine.
 *
 * The game is segarally95-recomp: the i960 program lifted to C, on that project's Model 2
 * runtime (TGP and geometrizer HLE, the 68000 + SCSP sound board, System 24 tile layers).
 * The recomp's own host is a program that owns the main loop; here the game runs on a
 * coroutine (coro.h) and stops at the end of every frame (the frame_end host op in its
 * vblank wait, patches/0001), so retro_run is one frame. The sound board and the geometry
 * decode, threads in the recomp, run inline: the board for exactly this frame's 44.1 kHz
 * samples, the decode at the frame's end. Everything the game is lives in static memory and
 * two fixed arenas (machine.h, arena.h), so a save state is a copy of that memory.
 *
 * The picture comes from the renderer (video_api.h: WebGL2 in the browser, nothing in Node). */
#include "libretro.h"
#include "arena.h"
#include "coro.h"
#include "machine.h"
#include "video_api.h"
#include "zip.h"
#include "srally_version.h"

#include "i960_host.h"
#include "i960_lift.h"
#include "i960_mem.h"
#include "lift_syms.h"
#include "model2_geo.h"
#include "model2_geo_lift.h"
#include "model2_geo_render.h"
#include "model2_geo_tex.h"
#include "model2_hw.h"
#include "model2_hw_host.h"
#include "model2_hw_lift.h"
#include "model2_nvram.h"
#include "model2_polygon_rom.h"
#include "model2_rom.h"
#include "model2_snd.h"
#include "model2_snd_rom.h"
#include "model2_texture_rom.h"
#include "model2_tgp_fw.h"

#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#include <emscripten/html5.h>
#include <emscripten/stack.h>
#endif

/* The board's video timing: 16 MHz pixel clock, 656 x 424 (MAME's model2 screen), 57.524 Hz. */
#define PIXEL_CLOCK 16000000u
#define FRAME_CLOCKS (656u * 424u)
#define SAMPLE_RATE 44100u
#define WIDTH SRALLY_VIDEO_WIDTH
#define HEIGHT SRALLY_VIDEO_HEIGHT
#define LOCK 0x60 /* full lock from centre, in ADC units: 0x20 left, 0xe0 right (the viewer's) */
#define PEDAL_DOWN 0xe0
#define PEDAL_UP 0x00
#define MAX_SAMPLES 800 /* a frame's samples: 766 or 767 */
#define MACHINE_MAGIC 0x53524c59u /* "SRLY" */
#define STATE_MAGIC 0x54535253u   /* "SRST" */
#define STATE_VERSION 1u

static retro_environment_t environ_cb;
static retro_video_refresh_t video_cb;
static retro_audio_sample_batch_t audio_batch_cb;
static retro_input_poll_t input_poll_cb;
static retro_input_state_t input_state_cb;
static retro_log_printf_t log_cb;

static void Log(enum retro_log_level level, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
static void Log(enum retro_log_level level, const char *fmt, ...)
{
  char text[1024];
  va_list vl;
  va_start(vl, fmt);
  vsnprintf(text, sizeof(text), fmt, vl);
  va_end(vl);
  if (log_cb) log_cb(level, "%s\n", text);
  else fprintf(stderr, "srally: %s\n", text);
}

static uint64_t NowUs(void)
{
#ifdef __EMSCRIPTEN__
  return (uint64_t)(emscripten_get_now() * 1000.0);
#else
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return (uint64_t)t.tv_sec * 1000000u + (uint64_t)t.tv_nsec / 1000u;
#endif
}

/******************************************************************************
 Options (srally_set) and timings (srally_timings)
******************************************************************************/

static int s_steerStep = 12;      /* ADC units a frame once the slow stage is over, and back to centre */
static int s_steerSlow = 3;       /* ADC units a frame for the first s_steerSlowFrames of a press away from centre */
static int s_steerSlowFrames = 16;
static char s_region[32] = "international"; /* the SDL viewer's default (model2_nvram.c g_region) */
static char s_nvramDir[256] = "/nvram";

static struct
{
  uint64_t frames, logic, geometry, raster, sound, audio, total;
} s_timings;

static int clampi(int v, int lo, int hi) { return v < lo ? lo : v > hi ? hi : v; }

void srally_set(const char *key, const char *value)
{
  const char *k = key ? key : "", *v = value ? value : "";
  const int n = atoi(v);
  if (!strcmp(k, "steer_step")) s_steerStep = clampi(n, 1, 2 * LOCK);
  else if (!strcmp(k, "steer_slow")) s_steerSlow = clampi(n, 1, 2 * LOCK);
  else if (!strcmp(k, "steer_slow_frames")) s_steerSlowFrames = clampi(n, 0, 600);
  else if (!strcmp(k, "region"))
  {
    if (strcmp(v, "international") && strcmp(v, "japan") && strcmp(v, "us"))
    {
      Log(RETRO_LOG_WARN, "region: international, japan or us, not %s.", v);
      return;
    }
    snprintf(s_region, sizeof(s_region), "%s", v);
  }
  else if (!strcmp(k, "nvram_dir")) snprintf(s_nvramDir, sizeof(s_nvramDir), "%s", v);
  else if (!strcmp(k, "log"))
  {
    /* The recomp's own stderr lines (before loading): status (ROM load, boot, NVRAM), lift
     * (its diagnostic traces), or none. */
    setenv("I960_LIFT_VERBOSE", !strcmp(v, "status") || !strcmp(v, "lift") ? "1" : "0", 1);
    setenv("I960_LIFT_LOG", !strcmp(v, "lift") ? "1" : "0", 1);
  }
  else Log(RETRO_LOG_WARN, "srally_set: no option %s.", k);
}

/* Microseconds summed over the frames since the last call, as JSON:
 * {"frames":n,"logic":..,"geometry":..,"raster":..,"sound":..,"audio":..,"total":..}
 * logic: the game's code (i960, TGP, the frame's tile work); geometry: decoding the frame's
 * display list into triangles; raster: the renderer; sound: the 68000 and SCSP for the
 * frame's samples; audio: handing the samples over; total: all of retro_run. */
const char *srally_timings(void)
{
  static char text[256];
  snprintf(text, sizeof(text),
           "{\"frames\":%llu,\"logic\":%llu,\"geometry\":%llu,\"raster\":%llu,\"sound\":%llu,\"audio\":%llu,\"total\":%llu}",
           (unsigned long long)s_timings.frames, (unsigned long long)s_timings.logic,
           (unsigned long long)s_timings.geometry, (unsigned long long)s_timings.raster,
           (unsigned long long)s_timings.sound, (unsigned long long)s_timings.audio,
           (unsigned long long)s_timings.total);
  memset(&s_timings, 0, sizeof(s_timings));
  return text;
}

/******************************************************************************
 The machine's memory: save states (machine.h)
******************************************************************************/

typedef struct
{
  uint8_t *p;
  size_t n;
} Range;

static Range s_ranges[12];
static unsigned s_rangeCount;
static size_t s_heapCap;          /* the heap arena's bytes a state can hold */
static size_t s_stateSize;        /* retro_serialize_size: fixed per load */
static int s_statesOk;            /* the layout check passed */
static const char *s_statesWhy = "no game loaded";
static uint32_t s_layout;         /* a hash of the ranges: states only load into this layout */
static uint8_t *s_powerOn;        /* the machine at power-on, as a state (retro_reset) */
static uint8_t *s_pristineData;   /* the machine's initialised data before the first load */
static size_t s_pristineDataSize;

#define HEADER_WORDS 8 /* magic, version, layout, heap top (lo, hi), frames (lo, hi), reserved */

static uint32_t Fnv(uint32_t h, const void *data, size_t n)
{
  const uint8_t *p = (const uint8_t *)data;
  for (size_t i = 0; i < n; i++) h = (h ^ p[i]) * 16777619u;
  return h;
}

static int Inside(const void *p, const Range *r) { return (const uint8_t *)p >= r->p && (const uint8_t *)p < r->p + r->n; }

static int InAnyRange(const void *p)
{
  for (unsigned i = 0; i < s_rangeCount; i++)
    if (Inside(p, &s_ranges[i])) return 1;
  return 0;
}

/* The machine's static data, from the markers, less the maincpu ROM image; checked against a
 * few addresses that must be in it and a few that must not (the C library's, the arenas', the
 * stack's). Sets s_statesOk / s_statesWhy. */
static void FindRanges(void)
{
  uint8_t *db = (uint8_t *)srally_state_data_begin, *de = (uint8_t *)srally_state_data_end;
  uint8_t *bb = (uint8_t *)srally_state_bss_begin, *be = (uint8_t *)srally_state_bss_end;
  uint8_t *rom = model2_maincpu_rom, *romEnd = model2_maincpu_rom + MAINCPU_SIZE;
  size_t heapSize, romArenaSize;
  uint8_t *heap = srally_heap_arena_memory(&heapSize), *romArena = srally_rom_arena_memory(&romArenaSize);
  void *probe = malloc(16);
  const void *mustIn[] = { &g0, &fp, model2_workram, model2_crx_ram, &srally_machine, srally_machine.stack,
                           &srally_machine.stack[SRALLY_FIBER_STACK - 1] };
  const void *mustOut[] = { stdout, stderr, &errno, probe, heap, heap + heapSize - 1, romArena, romArena + romArenaSize - 1,
                            &s_ranges, &environ_cb, model2_maincpu_rom, model2_geo_render_scratch_prg,
                            model2_geo_render_scratch_mtx + 262144 * 12 - 1 };
  unsigned i;

  s_rangeCount = 0;
  s_statesOk = 0;
  if (!(db < de && bb < be && (de <= bb || be <= db)))
  {
    s_statesWhy = "the markers are not in order (the link did not keep input order)";
    free(probe);
    return;
  }
  s_ranges[s_rangeCount++] = (Range){ db, (size_t)(de - db) };
  {
    uint8_t *sb = (uint8_t *)srally_state_static_bss_begin(), *se = (uint8_t *)srally_state_static_bss_end();
    if (se + 64 >= bb && sb <= be + 64)
    {
      if (sb < bb) bb = sb;
      if (se > be) be = se;
    }
  }
  {
    /* The zeroed data less what is not state: the maincpu ROM image (constant once loaded) and
     * the geometry decode's scratch (rewritten before it is read, patches/0003). */
    Range holes[3] = { { rom, (size_t)(romEnd - rom) },
                       { (uint8_t *)model2_geo_render_scratch_prg, sizeof(model2_geo_render_scratch_prg) },
                       { (uint8_t *)model2_geo_render_scratch_mtx, sizeof(model2_geo_render_scratch_mtx) } };
    uint8_t *at = bb;
    /* In address order. */
    for (int a = 0; a < 3; a++)
      for (int b = a + 1; b < 3; b++)
        if (holes[b].p < holes[a].p)
        {
          Range t = holes[a];
          holes[a] = holes[b];
          holes[b] = t;
        }
    for (int h = 0; h < 3; h++)
    {
      if (holes[h].p < at || holes[h].p + holes[h].n > be) continue; /* elsewhere (Mach-O statics) */
      if (holes[h].p > at) s_ranges[s_rangeCount++] = (Range){ at, (size_t)(holes[h].p - at) };
      at = holes[h].p + holes[h].n;
    }
    if (be > at) s_ranges[s_rangeCount++] = (Range){ at, (size_t)(be - at) };
  }
  {
    /* Zeroed statics, where the object format keeps them apart (Mach-O's __bss); in wasm they
     * are with the rest (the markers' own statics a few bytes either side). */
    uint8_t *sb = (uint8_t *)srally_state_static_bss_begin(), *se = (uint8_t *)srally_state_static_bss_end();
    if (!(sb < se))
    {
      s_statesWhy = "the static markers are not in order";
      free(probe);
      return;
    }
    if (!(se + 64 >= bb && sb <= be + 64))
    {
      if (sb < de && se > db)
      {
        s_statesWhy = "the static markers overlap the data";
        free(probe);
        return;
      }
      {
        /* The decode's scratch is static there. */
        uint8_t *at = sb, *p1 = (uint8_t *)model2_geo_render_scratch_prg, *p2 = (uint8_t *)model2_geo_render_scratch_mtx;
        Range holes[2] = { { p1 < p2 ? p1 : p2, p1 < p2 ? sizeof(model2_geo_render_scratch_prg) : sizeof(model2_geo_render_scratch_mtx) },
                           { p1 < p2 ? p2 : p1, p1 < p2 ? sizeof(model2_geo_render_scratch_mtx) : sizeof(model2_geo_render_scratch_prg) } };
        for (int h = 0; h < 2; h++)
        {
          if (holes[h].p < at || holes[h].p + holes[h].n > se) continue;
          if (holes[h].p > at) s_ranges[s_rangeCount++] = (Range){ at, (size_t)(holes[h].p - at) };
          at = holes[h].p + holes[h].n;
        }
        if (se > at) s_ranges[s_rangeCount++] = (Range){ at, (size_t)(se - at) };
      }
    }
  }
  for (i = 0; i < sizeof(mustIn) / sizeof(mustIn[0]); i++)
    if (!InAnyRange(mustIn[i]))
    {
      s_statesWhy = "a machine variable is outside the machine's ranges";
      free(probe);
      return;
    }
  for (i = 0; i < sizeof(mustOut) / sizeof(mustOut[0]); i++)
    if (InAnyRange(mustOut[i]))
    {
      s_statesWhy = "something not the machine's is inside its ranges";
      free(probe);
      return;
    }
  free(probe);
#ifdef __EMSCRIPTEN__
  {
    /* The main stack must be apart too. */
    uint8_t *base = (uint8_t *)emscripten_stack_get_base(), *end = (uint8_t *)emscripten_stack_get_end();
    for (i = 0; i < s_rangeCount; i++)
      if (s_ranges[i].p < base && s_ranges[i].p + s_ranges[i].n > end)
      {
        s_statesWhy = "the main stack is inside the machine's ranges";
        return;
      }
  }
#endif
  s_layout = 2166136261u;
  for (i = 0; i < s_rangeCount; i++)
  {
    uintptr_t a = (uintptr_t)s_ranges[i].p;
    uint64_t n = s_ranges[i].n;
    s_layout = Fnv(s_layout, &a, sizeof(a));
    s_layout = Fnv(s_layout, &n, sizeof(n));
  }
  {
    uintptr_t a = (uintptr_t)heap;
    s_layout = Fnv(s_layout, &a, sizeof(a));
    s_layout = Fnv(s_layout, SRALLY_RECOMP_COMMIT, strlen(SRALLY_RECOMP_COMMIT));
  }
  s_heapCap = heapSize;
  s_statesOk = 1;
  if (getenv("SRALLY_DEBUG_RANGES"))
    for (i = 0; i < s_rangeCount; i++) Log(RETRO_LOG_INFO, "machine range %u: %p + %zu", i, (void *)s_ranges[i].p, s_ranges[i].n);
  s_statesWhy = "";
}

static size_t MachineBytes(void)
{
  size_t n = 0;
  for (unsigned i = 0; i < s_rangeCount; i++) n += s_ranges[i].n;
  return n;
}

static size_t StateSize(void) { return HEADER_WORDS * 4 + MachineBytes() + s_heapCap; }

/* The machine into dst (StateSize() bytes; what the heap does not use is zeros). */
static int SaveMachine(uint8_t *dst, size_t size)
{
  uint32_t header[HEADER_WORDS] = { 0 };
  const size_t top = srally_machine.heap.top;
  uint8_t *at = dst;
  if (!s_statesOk || size < StateSize() || top > s_heapCap) return 0;
  header[0] = STATE_MAGIC;
  header[1] = STATE_VERSION;
  header[2] = s_layout;
  header[3] = (uint32_t)top;
  header[4] = (uint32_t)((uint64_t)top >> 32);
  header[5] = (uint32_t)srally_machine.frames;
  header[6] = (uint32_t)(srally_machine.frames >> 32);
  memcpy(at, header, sizeof(header));
  at += sizeof(header);
  for (unsigned i = 0; i < s_rangeCount; i++)
  {
    memcpy(at, s_ranges[i].p, s_ranges[i].n);
    at += s_ranges[i].n;
  }
  memcpy(at, srally_heap_arena_memory(NULL), top);
  memset(at + top, 0, (size_t)(dst + size - (at + top)));
  return 1;
}

static int LoadMachine(const uint8_t *src, size_t size, const char **why)
{
  uint32_t header[HEADER_WORDS];
  size_t top;
  const uint8_t *at = src;
  if (!s_statesOk) { *why = s_statesWhy; return 0; }
  if (size < StateSize()) { *why = "too short"; return 0; }
  memcpy(header, src, sizeof(header));
  if (header[0] != STATE_MAGIC) { *why = "not a Sega Rally state"; return 0; }
  if (header[1] != STATE_VERSION) { *why = "another state version"; return 0; }
  if (header[2] != s_layout) { *why = "made by another build of the core"; return 0; }
  top = (size_t)((uint64_t)header[3] | ((uint64_t)header[4] << 32));
  if (top > s_heapCap) { *why = "bad heap size"; return 0; }
  at += sizeof(header);
  for (unsigned i = 0; i < s_rangeCount; i++)
  {
    memcpy(s_ranges[i].p, at, s_ranges[i].n);
    at += s_ranges[i].n;
  }
  memcpy(srally_heap_arena_memory(NULL), at, top);
  coro_after_restore();
  return 1;
}

/******************************************************************************
 The game's side: its coroutine and the host ops
******************************************************************************/

static int s_loaded;
static uint8_t *s_ram;    /* RETRO_MEMORY_SYSTEM_RAM: CRX RAM then work RAM, copied out */
static size_t s_ramSize;
static int16_t s_audio[MAX_SAMPLES * 2];
static int s_rgba;        /* the frontend takes RGBA bytes (format 100) */
static uint32_t *s_xrgb;  /* else the picture converted here */
static int s_videoReady;
static char s_workDir[512], s_romDir[512];

/* The coroutine's entry: the cold boot, then the game's own main loop, which never returns
 * unless the game halts (the recomp's dispatch halt). */
static void GameMain(void)
{
  boot_entry_host(0, 0, 0);
  i960_host_run_post_reset(0, 0, 0);
  srally_machine.halted = 1;
}

/* The frame's end, inside the game's vblank wait (patches/0001): as the SDL viewer's present
 * (vsync_flip_one_frame), the boot screen's tile sync, then back to retro_run. */
static void HostFrameEnd(void)
{
  if (i960_host_boot_screen()) boot_tile_splash_frame(0, 0, 0);
  coro_yield();
}

static int HostDisplayWanted(void) { return 0; }
static int HostDisplayFlip(void) { return 0; }

static void BindHost(void)
{
  model2_hw_host_ops_t ops;
  model2_hw_host_ops_from_lift(&ops);
  ops.display_wanted = HostDisplayWanted;
  ops.display_flip = HostDisplayFlip;
  ops.boot_vblank = NULL;
  ops.frame_end = HostFrameEnd;
  model2_hw_bind_host(&ops);
}

/* The SDL viewer's scene tests (sys24_viewer.c): screens that are tiles only, where it drops
 * the 3D layer and stops decoding. The decode follows the same rule here. */
static int SplashHold(uint32_t inner, uint32_t frame)
{
  return (inner == 2u || inner == 4u || inner == 6u) && (int32_t)frame < 0;
}

static int StartBanner(void)
{
  static const uint32_t tables[2] = { 0x005ba820u, 0x005ba880u };
  uint32_t frame;
  if (i960_ld_u32(I960_WORKRAM, 0x202098, 0) != 3u) return 0;
  frame = i960_ld_u32(I960_WORKRAM, 0x2020ac, 0) & 15u;
  for (int t = 0; t < 2; t++)
  {
    uint32_t handler = i960_ld_u32(I960_WORKRAM, tables[t], frame << 2);
    if (handler == 0u) handler = model2_workram_mirror_u32(tables[t] + (frame << 2));
    if (handler == 0x005ba940u || handler == 0x0001b940u) return 1;
    if (handler >= 0x005a0000u && handler < 0x005c0000u && (0x1000u + (handler - 0x005a0000u)) == 0x0001b940u) return 1;
  }
  return 0;
}

static int TilesOpaque(void)
{
  const uint32_t inner = i960_ld_u32(I960_WORKRAM, 0x20209c, 0), frame = i960_ld_u32(I960_WORKRAM, 0x20a808, 0);
  return SplashHold(inner, frame) || i960_ld_u32(I960_WORKRAM, 0x202098, 0) == 4u || StartBanner();
}

/* The frame's geometry, every frame (shown or not): the display list the game latched,
 * decoded into the renderer's triangles, as the viewer's flip has its worker do. And the one
 * thing drawing does to the machine: the first time there is a textured mesh to draw, the GL
 * renderer asks for the texture sheets, and model2_tex_sheet_bank seeds a still empty texture
 * RAM from the ROM. Done here at that same moment, the machine is the same whether the frame is
 * drawn or not (the renderer's own call then finds it seeded). */
static void DecodeFrame(void)
{
  const int opaque = TilesOpaque();
  if (opaque && !srally_machine.was_opaque2d) model2_geo_clear();
  srally_machine.was_opaque2d = (uint32_t)opaque;
  if (!opaque)
  {
    const float *xyzuv = NULL;
    const model2_geo_tri_mat_t *mats = NULL;
    unsigned nverts = 0, ntris = 0;
    model2_geo_decode();
    if (model2_geo_lock_textured(&xyzuv, &nverts, &mats, &ntris) == 0)
    {
      const int draws = xyzuv && mats && ntris > 0 && nverts >= 3;
      model2_geo_unlock();
      if (draws) (void)model2_tex_sheet_bank(0);
    }
  }
}

/******************************************************************************
 Inputs
******************************************************************************/

/* The cabinet's controls from the RetroPad: LEFT/RIGHT steer (a ramp toward full lock and
 * back, daytona/'s: s_steerSlow a frame for the first s_steerSlowFrames of a press that turns
 * the wheel away from centre, s_steerStep after that, when counter-steering and back to
 * centre), UP accelerator, DOWN brake, B/A shift down/up through the H-shifter's gears 1-4,
 * Y the VIEW CHANGE button, START, SELECT coin. Pedals full or off, as the viewer's keys. */
static void ApplyPad(uint32_t pad)
{
  srally_seat_t *seat = &srally_machine.seat;
#define HELD(id) (((pad >> (id)) & 1u) != 0)
  const int dir = (HELD(RETRO_DEVICE_ID_JOYPAD_RIGHT) ? 1 : 0) - (HELD(RETRO_DEVICE_ID_JOYPAD_LEFT) ? 1 : 0);
  const uint32_t pressed = pad & ~seat->held;
  int target, away, step, delta;

  if (dir == 0) seat->turning = 0;
  else if (seat->turning != 0 && (seat->turning > 0) == (dir > 0)) seat->turning += dir;
  else seat->turning = dir;
  target = dir * LOCK;
  away = dir != 0 && (seat->steer == 0 || (seat->steer > 0) == (dir > 0));
  step = away && abs(seat->turning) <= s_steerSlowFrames ? s_steerSlow : s_steerStep;
  delta = clampi(target - seat->steer, -step, step);
  seat->steer += delta;
  model2_io_analog_set(MODEL2_IO_AN_STEER, (uint8_t)(0x80 + seat->steer));
  model2_io_analog_set(MODEL2_IO_AN_ACCEL, HELD(RETRO_DEVICE_ID_JOYPAD_UP) ? PEDAL_DOWN : PEDAL_UP);
  model2_io_analog_set(MODEL2_IO_AN_BRAKE, HELD(RETRO_DEVICE_ID_JOYPAD_DOWN) ? PEDAL_DOWN : PEDAL_UP);
  if ((pressed >> RETRO_DEVICE_ID_JOYPAD_A) & 1u && seat->gear < 4) seat->gear++;
  if ((pressed >> RETRO_DEVICE_ID_JOYPAD_B) & 1u && seat->gear > 1) seat->gear--;
  model2_io_shifter_set((uint32_t)seat->gear);
  model2_io_in0_set_mask(MODEL2_IO_IN0_COIN1, HELD(RETRO_DEVICE_ID_JOYPAD_SELECT));
  model2_io_in0_set_mask(MODEL2_IO_IN0_START1, HELD(RETRO_DEVICE_ID_JOYPAD_START));
  model2_io_in0_set_mask(MODEL2_IO_IN0_VR, HELD(RETRO_DEVICE_ID_JOYPAD_Y));
  seat->held = pad;
#undef HELD
}

static uint32_t ReadPad(void)
{
  uint32_t pad = 0;
  if (!input_state_cb) return 0;
  for (unsigned id = 0; id <= RETRO_DEVICE_ID_JOYPAD_R; id++)
    if (input_state_cb(0, RETRO_DEVICE_JOYPAD, 0, id)) pad |= 1u << id;
  return pad;
}

/******************************************************************************
 Loading
******************************************************************************/

/* What the recomp reads of MAME's srallyc set (with the Revision B program, srallycb's). */
static const char *const kWanted[] = {
  "epr-17888b.12", "epr-17889b.13", "mpr-17746.10", "mpr-17747.11", "mpr-17744.8", "mpr-17745.9", "mpr-17884.6",
  "mpr-17885.7", "mpr-17748.16", "mpr-17750.20", "mpr-17749.17", "mpr-17751.21", "mpr-17753.25", "mpr-17752.24",
  "epr-17890a.30", "epr-17890.30", "mpr-17756.31", "mpr-17757.32", "mpr-17886.36", "mpr-17887.37",
  "mpr-17754.28", "mpr-17755.29", "mpr-17754.29", "mpr-17755.28",
};

static int Wanted(const char *name)
{
  for (size_t i = 0; i < sizeof(kWanted) / sizeof(kWanted[0]); i++)
    if (!strcmp(name, kWanted[i])) return 1;
  return 0;
}

static int MakeDirs(const char *path)
{
  char buf[512];
  snprintf(buf, sizeof(buf), "%s", path);
  for (char *p = buf + 1; *p; p++)
    if (*p == '/')
    {
      *p = 0;
      if (mkdir(buf, 0755) != 0 && errno != EEXIST) return -1;
      *p = '/';
    }
  return mkdir(buf, 0755) != 0 && errno != EEXIST ? -1 : 0;
}

static uint8_t *ReadFile(const char *path, size_t *size)
{
  FILE *f = fopen(path, "rb");
  uint8_t *data = NULL;
  long n;
  if (!f) return NULL;
  if (fseek(f, 0, SEEK_END) == 0 && (n = ftell(f)) > 0 && fseek(f, 0, SEEK_SET) == 0 && (data = malloc((size_t)n)) &&
      fread(data, 1, (size_t)n, f) == (size_t)n)
    *size = (size_t)n;
  else
  {
    free(data);
    data = NULL;
  }
  fclose(f);
  return data;
}

/* The files the recomp reads, out of the zip into s_romDir. */
static int ExtractRoms(const uint8_t *zip, size_t size)
{
  enum { MAX_ENTRIES = 256 };
  srally_zip_entry_t *entries = calloc(MAX_ENTRIES, sizeof(*entries));
  int n = entries ? srally_zip_list(zip, size, entries, MAX_ENTRIES) : -1, extracted = 0, ok = 1;
  if (n < 0)
  {
    Log(RETRO_LOG_ERROR, "The ROM set is not a zip archive.");
    free(entries);
    return -1;
  }
  if (MakeDirs(s_romDir) != 0)
  {
    Log(RETRO_LOG_ERROR, "Cannot make %s.", s_romDir);
    free(entries);
    return -1;
  }
  for (int i = 0; i < n && ok; i++)
  {
    const srally_zip_entry_t *e = &entries[i];
    uint8_t *data;
    const char *why = "";
    char path[640];
    FILE *f;
    if (!Wanted(e->name)) continue;
    if (!(data = malloc(e->size ? e->size : 1)) || srally_zip_read(zip, size, e, data, &why) != 0)
    {
      Log(RETRO_LOG_ERROR, "%s in the zip: %s.", e->name, data ? why : "out of memory");
      ok = 0;
    }
    else
    {
      snprintf(path, sizeof(path), "%s/%s", s_romDir, e->name);
      if (!(f = fopen(path, "wb")) || fwrite(data, 1, e->size, f) != e->size)
      {
        Log(RETRO_LOG_ERROR, "Cannot write %s.", path);
        ok = 0;
      }
      if (f) fclose(f);
      extracted++;
    }
    free(data);
  }
  free(entries);
  return ok ? extracted : -1;
}

/* Removes what loading wrote to the file system (the ROM files and the recomp's extracted
 * program images): the images are in memory, and a power-on restores them from there. */
static void CleanFiles(void)
{
  char path[640];
  for (size_t i = 0; i < sizeof(kWanted) / sizeof(kWanted[0]); i++)
  {
    snprintf(path, sizeof(path), "%s/%s", s_romDir, kWanted[i]);
    unlink(path);
  }
  rmdir(s_romDir);
  snprintf(path, sizeof(path), "%s/out/i960/maincpu_deinterleaved.bin", s_workDir);
  unlink(path);
  snprintf(path, sizeof(path), "%s/out/i960/main_data_deinterleaved.bin", s_workDir);
  unlink(path);
}

static void SetEnv(const char *key, const char *value) { setenv(key, value, 1); }

/* The recomp's knobs as its SDL viewer sets them for a cold boot into the attract mode
 * (lift_boot_screen.c, live view), less the wall clock: frames are stepped here. */
static void SetEnvironment(void)
{
  char nvram[640];
  SetEnv("SEGAMOD2_ROM_DIR", s_romDir);
  SetEnv("SEGAMOD2_ROOT", s_workDir);
  snprintf(nvram, sizeof(nvram), "%s/srally.yaml", s_nvramDir);
  SetEnv("I960_HOST_NVRAM", nvram);
  SetEnv("I960_HOST_VIDEO_SYNC", "0");
  SetEnv("I960_HOST_LIVE_VIEW", "0");
  SetEnv("I960_HOST_BOOT_SCREEN", "1");
  SetEnv("I960_HOST_BOOT_FAST_COUNTDOWN", "0");
  SetEnv("I960_HOST_MAX_DISPATCH", "0");
  SetEnv("I960_HOST_TRACE_QUIET", "1");
  SetEnv("I960_HOST_ASPECT", "4:3");
  unsetenv("I960_HOST_MILESTONE_BOOT");
  unsetenv("I960_HOST_MILESTONE_SOUND_INIT");
  unsetenv("I960_HOST_SKIP_PRACTICE");
  unsetenv("I960_HOST_FRAME_PACE");
  unsetenv("I960_PALETTE_DUMP");
  unsetenv("I960_GEO_DUMP");
  unsetenv("I960_COPRO_DUMP");
  unsetenv("I960_GEO_SUMMARY");
  unsetenv("I960_SND_DUMP");
}

/* Back to the state before any load: the machine's initialised data as the module started
 * with it, its zeroed data zero, the arenas empty. */
static void Pristine(void)
{
  uint8_t *db = (uint8_t *)srally_state_data_begin, *de = (uint8_t *)srally_state_data_end;
  uint8_t *bb = (uint8_t *)srally_state_bss_begin, *be = (uint8_t *)srally_state_bss_end;
  size_t romSize;
  if (!s_pristineData)
  {
    s_pristineDataSize = de > db ? (size_t)(de - db) : 0;
    s_pristineData = malloc(s_pristineDataSize ? s_pristineDataSize : 1);
    memcpy(s_pristineData, db, s_pristineDataSize);
  }
  else
  {
    uint8_t *sb = (uint8_t *)srally_state_static_bss_begin(), *se = (uint8_t *)srally_state_static_bss_end();
    memcpy(db, s_pristineData, s_pristineDataSize);
    if (be > bb) memset(bb, 0, (size_t)(be - bb));
    if (se > sb && !(sb >= bb && se <= be)) memset(sb, 0, (size_t)(se - sb));
  }
  srally_arena_init(&srally_rom_arena, srally_rom_arena_memory(&romSize), romSize);
}

/* Everything up to the game's first frame; 0 on success. */
static int PowerOnFromFiles(void)
{
  size_t heapSize;
  srally_machine.magic = MACHINE_MAGIC;
  srally_machine.seat.gear = 1;
  srally_arena_init(&srally_machine.heap, srally_heap_arena_memory(&heapSize), heapSize);
  if (model2_nvram_set_region_name(s_region) != 0) Log(RETRO_LOG_WARN, "Unknown region %s.", s_region);

  /* The ROM images, into the ROM arena: the CRC check, the program and data images, the sound
   * board's, the polygons and textures, the TGP's road data. */
  srally_alloc_set_rom(1);
  if (i960_host_load_rom_checked() != 0)
  {
    srally_alloc_set_rom(0);
    Log(RETRO_LOG_ERROR, "Not MAME's srallyc set with the Revision B program (srallycb): the files above are missing or wrong.");
    return -1;
  }
  if (model2_snd_rom_load(s_romDir) != 0 || model2_polygon_rom_load_default() != 0 ||
      model2_texture_rom_load_default() != 0)
  {
    srally_alloc_set_rom(0);
    Log(RETRO_LOG_ERROR, "Loading the sound, polygon or texture ROMs failed.");
    return -1;
  }
  if (model2_tgp_fw_load_copro_data_default() != 0) Log(RETRO_LOG_WARN, "No TGP road data (mpr-17754/55): cars fall through the mountain road.");
  srally_alloc_set_rom(0);

  /* The board, as the recomp's host resets it, with our host ops; the sound board and the
   * geometry decode inline. */
  model2_snd_set_inline(1);
  if (i960_host_reset_checked() != 0)
  {
    Log(RETRO_LOG_ERROR, "The board's reset failed.");
    return -1;
  }
  BindHost();
  i960_host_trace_init();
  model2_geo_render_set_sync(1);
  if (model2_geo_init_from_lift() != 0 || model2_geo_render_load_roms() != 0)
  {
    Log(RETRO_LOG_ERROR, "The geometry decoder did not start.");
    return -1;
  }
  (void)model2_nvram_load(NULL);
  /* The game starts on the first retro_run (a swap must not happen in an export that returns
   * something: coro.c). */
  coro_init(GameMain);
  return 0;
}

/******************************************************************************
 libretro API
******************************************************************************/

RETRO_API unsigned retro_api_version(void) { return RETRO_API_VERSION; }

RETRO_API void retro_set_environment(retro_environment_t cb)
{
  enum retro_pixel_format format = (enum retro_pixel_format)100;
  struct retro_log_callback logging;
  environ_cb = cb;
  /* Our frontend (web/emulator/libretro.js) takes RGBA bytes, format 100, as read back from
   * WebGL; any other gets XRGB8888. */
  s_rgba = cb(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format) ? 1 : 0;
  if (!s_rgba)
  {
    format = RETRO_PIXEL_FORMAT_XRGB8888;
    cb(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  }
  if (cb(RETRO_ENVIRONMENT_GET_LOG_INTERFACE, &logging)) log_cb = logging.log;
}

RETRO_API void retro_set_video_refresh(retro_video_refresh_t cb) { video_cb = cb; }
RETRO_API void retro_set_audio_sample(retro_audio_sample_t cb) { (void)cb; }
RETRO_API void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) { audio_batch_cb = cb; }
RETRO_API void retro_set_input_poll(retro_input_poll_t cb) { input_poll_cb = cb; }
RETRO_API void retro_set_input_state(retro_input_state_t cb) { input_state_cb = cb; }
RETRO_API void retro_init(void) {}
RETRO_API void retro_deinit(void) { retro_unload_game(); }

RETRO_API void retro_get_system_info(struct retro_system_info *info)
{
  memset(info, 0, sizeof(*info));
  info->library_name = "Sega Rally Championship (segarally95-recomp)";
  info->library_version = SRALLY_RECOMP_COMMIT_SHORT;
  info->valid_extensions = "zip";
  info->need_fullpath = false;
  info->block_extract = true;
}

RETRO_API void retro_get_system_av_info(struct retro_system_av_info *info)
{
  memset(info, 0, sizeof(*info));
  info->geometry.base_width = WIDTH;
  info->geometry.base_height = HEIGHT;
  info->geometry.max_width = WIDTH;
  info->geometry.max_height = HEIGHT;
  info->geometry.aspect_ratio = 4.0f / 3.0f;
  info->timing.fps = (double)PIXEL_CLOCK / FRAME_CLOCKS; /* 57.524 Hz */
  info->timing.sample_rate = SAMPLE_RATE;
}

RETRO_API void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port; (void)device; }

/* The renderer may define this (video_api.h): after a state load or a reset the machine changed
 * under it. */
__attribute__((weak)) void srally_video_invalidate(void) {}

static void RendererAfterLoad(void)
{
  if (s_videoReady) srally_video_invalidate();
}

RETRO_API bool retro_load_game(const struct retro_game_info *info)
{
  const uint8_t *zip;
  size_t zipSize = 0;
  uint8_t *owned = NULL;
  uint64_t start = NowUs();
  int files;

  retro_unload_game();
  if (!info || (!info->data && !info->path))
  {
    Log(RETRO_LOG_ERROR, "No ROM set given.");
    return false;
  }
  if (info->data && info->size)
  {
    zip = (const uint8_t *)info->data;
    zipSize = info->size;
  }
  else if (!(zip = owned = ReadFile(info->path, &zipSize)))
  {
    Log(RETRO_LOG_ERROR, "Cannot read %s.", info->path);
    return false;
  }
#ifdef __EMSCRIPTEN__
  snprintf(s_workDir, sizeof(s_workDir), "/srally");
#else
  {
    const char *tmp = getenv("TMPDIR");
    snprintf(s_workDir, sizeof(s_workDir), "%s/srally-%d", tmp && *tmp ? tmp : "/tmp", (int)getpid());
  }
#endif
  snprintf(s_romDir, sizeof(s_romDir), "%s/roms", s_workDir);
  files = ExtractRoms(zip, zipSize);
  free(owned);
  if (files < 0) return false;

  SetEnvironment();
  Pristine();
  FindRanges();
  if (PowerOnFromFiles() != 0)
  {
    CleanFiles();
    return false;
  }
  CleanFiles();

  s_ramSize = 0x40000u + WORKRAM_SIZE;
  s_ram = calloc(1, s_ramSize);
  s_stateSize = s_statesOk ? StateSize() : 0;
  if (s_statesOk && (s_powerOn = malloc(s_stateSize)) && !SaveMachine(s_powerOn, s_stateSize))
  {
    free(s_powerOn);
    s_powerOn = NULL;
  }
  if (!s_statesOk) Log(RETRO_LOG_WARN, "Save states are off: %s.", s_statesWhy);

  if (!s_videoReady)
  {
    /* The renderer's allocations are its own (the C library's), not the machine's. */
    int gl = 0;
#if defined(SRALLY_HAVE_VIDEO_H) && defined(__EMSCRIPTEN__)
    gl = srally_video_create_context() == 0;
#endif
    s_videoReady = srally_video_init(WIDTH, HEIGHT) == 0;
#ifdef SRALLY_HAVE_VIDEO_H
    /* The shim decodes every frame (DecodeFrame); the renderer only draws what is published
     * (its default too). */
    srally_video_set_decode(SRALLY_VIDEO_DECODE_CALLER);
#endif
    if (!s_videoReady) Log(RETRO_LOG_WARN, "The renderer did not start: the game runs but draws nothing.");
    else if (!gl) Log(RETRO_LOG_INFO, "No WebGL context: the picture is black.");
  }
  if (!s_rgba) s_xrgb = malloc(WIDTH * HEIGHT * 4);

  if (environ_cb)
  {
    static const struct retro_input_descriptor descriptors[] = {
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_LEFT, "Steer left" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_RIGHT, "Steer right" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_UP, "Accelerate" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_DOWN, "Brake" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_B, "Shift down" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_A, "Shift up" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_Y, "View change" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_START, "Start" },
      { 0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_SELECT, "Coin" },
      { 0, 0, 0, 0, NULL },
    };
    environ_cb(RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS, (void *)descriptors);
  }
  s_loaded = 1;
  memset(&s_timings, 0, sizeof(s_timings));
  Log(RETRO_LOG_INFO, "Sega Rally Championship (srallyc, Revision B program; recomp %s) loaded in %.0f ms: 496x384 at %.3f Hz, "
      "44.1 kHz; region %s; save states %s (%zu bytes); ROM images %.1f MB (arena peak %.1f of %.0f MB).",
      SRALLY_RECOMP_COMMIT_SHORT, (NowUs() - start) / 1000.0, (double)PIXEL_CLOCK / FRAME_CLOCKS, s_region,
      s_statesOk ? "on" : "off", s_stateSize, srally_rom_arena.used / 1048576.0, srally_rom_arena.peak / 1048576.0,
      srally_rom_arena.size / 1048576.0);
  return true;
}

RETRO_API bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t num)
{
  (void)type;
  (void)info;
  (void)num;
  return false;
}

RETRO_API void retro_unload_game(void)
{
  if (!s_loaded) return;
  /* The coroutine is abandoned where it is parked: its stack is the machine's, and the next
   * load starts from pristine memory (Pristine). */
  free(s_ram);
  s_ram = NULL;
  free(s_powerOn);
  s_powerOn = NULL;
  free(s_xrgb);
  s_xrgb = NULL;
  s_loaded = 0;
  s_stateSize = 0;
}

RETRO_API void retro_reset(void)
{
  const char *why = "no power-on state";
  if (!s_loaded) return;
  if (!s_powerOn || !LoadMachine(s_powerOn, s_stateSize, &why))
  {
    Log(RETRO_LOG_ERROR, "Reset failed: %s.", why);
    return;
  }
  RendererAfterLoad();
}

RETRO_API void retro_run(void)
{
  const uint64_t start = NowUs();
  uint64_t gameEnd, geometryEnd, rasterEnd, soundEnd;
  int enabled = 3, video, audio;
  unsigned samples;
  const uint8_t *pixels = NULL;

  if (!s_loaded || srally_machine.halted) return;
  if (input_poll_cb) input_poll_cb();
  /* What the frontend wants this frame: re-runs after a rollback want neither. */
  if (!environ_cb || !environ_cb(RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE, &enabled)) enabled = 3;
  video = enabled & 1;
  audio = (enabled & 2) != 0;

  ApplyPad(ReadPad());
  coro_resume(); /* the game's frame, up to its next vblank */
  srally_machine.frames++;
  gameEnd = NowUs();
  if (srally_machine.halted)
  {
    Log(RETRO_LOG_ERROR, "The game's main loop ended (the recomp halted): nothing runs until a reset or a state load.");
    return;
  }
  DecodeFrame();
  geometryEnd = NowUs();
  if (video && s_videoReady)
  {
    srally_video_render();
    pixels = srally_video_pixels();
  }
  rasterEnd = NowUs();
  /* This frame's samples: 44100 x 278144 / 16000000 = 766.6344 a frame, the fraction carried. */
  srally_machine.sample_carry += (uint64_t)SAMPLE_RATE * FRAME_CLOCKS;
  samples = (unsigned)(srally_machine.sample_carry / PIXEL_CLOCK);
  srally_machine.sample_carry %= PIXEL_CLOCK;
  model2_snd_run(s_audio, samples);
  srally_machine.samples += samples;
  soundEnd = NowUs();

  if (video && video_cb && pixels)
  {
    if (s_rgba) video_cb(pixels, WIDTH, HEIGHT, WIDTH * 4);
    else if (s_xrgb)
    {
      for (unsigned i = 0; i < WIDTH * HEIGHT; i++)
        s_xrgb[i] = 0xff000000u | ((uint32_t)pixels[i * 4] << 16) | ((uint32_t)pixels[i * 4 + 1] << 8) | pixels[i * 4 + 2];
      video_cb(s_xrgb, WIDTH, HEIGHT, WIDTH * 4);
    }
  }
  if (audio && audio_batch_cb) audio_batch_cb(s_audio, samples);
  s_timings.logic += gameEnd - start;
  s_timings.geometry += geometryEnd - gameEnd;
  s_timings.raster += rasterEnd - geometryEnd;
  s_timings.sound += soundEnd - rasterEnd;
  s_timings.audio += NowUs() - soundEnd;
  s_timings.total += NowUs() - start;
  s_timings.frames++;
}

RETRO_API size_t retro_serialize_size(void) { return s_loaded ? s_stateSize : 0; }

RETRO_API bool retro_serialize(void *data, size_t size)
{
  static unsigned complaints;
  if (!s_loaded || !data) return false;
  if (!s_statesOk)
  {
    if (++complaints <= 5) Log(RETRO_LOG_ERROR, "Save states are off: %s.", s_statesWhy);
    return false;
  }
  if (srally_machine.heap.top > s_heapCap)
  {
    Log(RETRO_LOG_ERROR, "The heap has grown past what a state holds.");
    return false;
  }
  return SaveMachine((uint8_t *)data, size) != 0;
}

RETRO_API bool retro_unserialize(const void *data, size_t size)
{
  const char *why = "";
  if (!s_loaded || !data) return false;
  if (!LoadMachine((const uint8_t *)data, size, &why))
  {
    Log(RETRO_LOG_ERROR, "The state does not load: %s.", why);
    return false;
  }
  RendererAfterLoad();
  return true;
}

RETRO_API void retro_cheat_reset(void) {}
RETRO_API void retro_cheat_set(unsigned index, bool enabled, const char *code)
{
  (void)index;
  (void)enabled;
  (void)code;
}
RETRO_API unsigned retro_get_region(void) { return RETRO_REGION_NTSC; }

/* RETRO_MEMORY_SYSTEM_RAM: the i960's RAM, copied out on each call: the CRX RAM (0x200000,
 * 256 KB: the game's variables, its main mode word at 0x2098) then the work RAM (0x500000,
 * 1 MB) at offset 0x40000. */
RETRO_API void *retro_get_memory_data(unsigned id)
{
  if (id != RETRO_MEMORY_SYSTEM_RAM || !s_loaded || !s_ram) return NULL;
  memcpy(s_ram, model2_crx_ram, 0x40000u);
  memcpy(s_ram + 0x40000u, model2_workram, WORKRAM_SIZE);
  return s_ram;
}

RETRO_API size_t retro_get_memory_size(unsigned id)
{
  return id == RETRO_MEMORY_SYSTEM_RAM && s_loaded ? s_ramSize : 0;
}

/* For the checks and the bench (not machine state). */
unsigned srally_stack_used(void) { return coro_stack_used(); }
size_t srally_heap_peak(void) { return srally_machine.heap.peak; }
size_t srally_heap_top(void) { return srally_machine.heap.top; }
