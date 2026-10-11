/* Native libretro host for the core (the same code as the wasm build, with clang): runs the
 * ROM set for a number of frames with scripted RetroPad input and reports the game's mode
 * words, RAM hashes, timings and (with --state) a save / run / load / run check.
 *
 *   .cache/native/bench ZIP [--frames N] [--script SPEC] [--hash-every N] [--state F:M]
 *                           [--options k=v,...] [--quiet]
 *                           [--no-video] [--timed-from F] [--watch OFF,...] [--shots F,...] [--shot-dir D]
 *
 * SPEC: comma-separated FROM-TO:BUTTONS (TO empty: to the end; BUTTONS joined with +, of
 * UP DOWN LEFT RIGHT A B X Y L R START SELECT), e.g. 900-905:SELECT,1000-1005:START,1400-:UP */
#include "libretro.h"
#include "sys24_tile.h"
#include "model2_rom.h"

#include <zlib.h>

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

void srally_set(const char *key, const char *value);
const char *srally_timings(void);
unsigned srally_stack_used(void);
size_t srally_heap_peak(void);
size_t srally_heap_top(void);

typedef struct
{
  long from, to;
  uint32_t mask;
} Event;

static Event s_events[64];
static int s_eventCount;
static long s_frame;
static uint32_t s_pad;
static int s_quiet;
static int s_video = 1;
static uint64_t s_audioHash = 2166136261u;
static uint64_t s_samples;

static void LogCb(enum retro_log_level level, const char *fmt, ...)
{
  va_list vl;
  if (s_quiet && level < RETRO_LOG_WARN) return;
  va_start(vl, fmt);
  fprintf(stderr, "[core %d] ", (int)level);
  vfprintf(stderr, fmt, vl);
  va_end(vl);
}

static bool Environment(unsigned cmd, void *data)
{
  switch (cmd)
  {
  case RETRO_ENVIRONMENT_GET_LOG_INTERFACE:
    ((struct retro_log_callback *)data)->log = LogCb;
    return true;
  case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
    return *(const int *)data == 100; /* RGBA bytes, as web/emulator/libretro.js */
  case RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE:
    *(int *)data = s_video ? 3 : 2;
    return true;
  case RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS:
    return true;
  default:
    return false;
  }
}

static void Video(const void *data, unsigned w, unsigned h, size_t pitch) { (void)data; (void)w; (void)h; (void)pitch; }

static size_t AudioBatch(const int16_t *data, size_t frames)
{
  const uint8_t *p = (const uint8_t *)data;
  for (size_t i = 0; i < frames * 4; i++) s_audioHash = (s_audioHash ^ p[i]) * 1099511628211ull;
  s_samples += frames;
  return frames;
}

static void InputPoll(void) {}

static int16_t InputState(unsigned port, unsigned device, unsigned index, unsigned id)
{
  (void)index;
  return port == 0 && device == RETRO_DEVICE_JOYPAD && ((s_pad >> id) & 1);
}

static uint32_t Button(const char *name, size_t len)
{
  static const struct { const char *name; unsigned id; } names[] = {
    { "B", 0 }, { "Y", 1 }, { "SELECT", 2 }, { "START", 3 }, { "UP", 4 }, { "DOWN", 5 }, { "LEFT", 6 },
    { "RIGHT", 7 }, { "A", 8 }, { "X", 9 }, { "L", 10 }, { "R", 11 },
  };
  for (size_t i = 0; i < sizeof(names) / sizeof(names[0]); i++)
    if (strlen(names[i].name) == len && !strncmp(names[i].name, name, len)) return 1u << names[i].id;
  fprintf(stderr, "bench: no button %.*s\n", (int)len, name);
  exit(2);
}

static void ParseScript(const char *spec)
{
  const char *p = spec;
  while (*p && s_eventCount < 64)
  {
    Event *e = &s_events[s_eventCount++];
    char *end;
    e->from = strtol(p, &end, 10);
    p = end;
    e->to = -1;
    if (*p == '-')
    {
      p++;
      if (*p >= '0' && *p <= '9') e->to = strtol(p, &end, 10), p = end;
    }
    else e->to = e->from;
    if (*p++ != ':') { fprintf(stderr, "bench: bad script at %s\n", p - 1); exit(2); }
    e->mask = 0;
    for (;;)
    {
      size_t len = strcspn(p, "+,");
      e->mask |= Button(p, len);
      p += len;
      if (*p == '+') { p++; continue; }
      break;
    }
    if (*p == ',') p++;
  }
}

static uint32_t PadAt(long frame)
{
  uint32_t mask = 0;
  for (int i = 0; i < s_eventCount; i++)
    if (frame >= s_events[i].from && (s_events[i].to < 0 || frame <= s_events[i].to)) mask |= s_events[i].mask;
  return mask;
}

static uint32_t RamHash(void)
{
  const uint8_t *ram = retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM);
  const size_t n = retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM);
  uint32_t h = 2166136261u;
  for (size_t i = 0; i < n; i++) h = (h ^ ram[i]) * 16777619u;
  return h;
}

static uint32_t Word(size_t offset)
{
  const uint8_t *ram = retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM);
  uint32_t v;
  memcpy(&v, ram + offset, 4);
  return v;
}

/* The System 24 tile layers alone (no 3D: the native build has no GL), as a PNG, for seeing
 * where the game is (menus, HUD). */
static void PutU32(uint8_t *p, uint32_t v) { p[0] = v >> 24; p[1] = v >> 16; p[2] = v >> 8; p[3] = v; }

static void Chunk(FILE *f, const char *type, const uint8_t *data, uint32_t n)
{
  uint8_t head[8];
  uint32_t crc = crc32(0, (const Bytef *)type, 4);
  if (n) crc = crc32(crc, data, n);
  PutU32(head, n);
  memcpy(head + 4, type, 4);
  fwrite(head, 1, 8, f);
  if (n) fwrite(data, 1, n, f);
  PutU32(head, crc);
  fwrite(head, 1, 4, f);
}

static void TileShot(const char *path)
{
  static sys24_tile_state_t *tile;
  static uint32_t bitmap[SYS24_FB_WIDTH * SYS24_FB_HEIGHT];
  const int w = SYS24_FB_WIDTH, h = SYS24_FB_HEIGHT;
  uint8_t *raw = malloc((size_t)h * (w * 3 + 1)), *z;
  uLongf zn = compressBound((uLong)h * (w * 3 + 1));
  uint8_t ihdr[13] = { 0 };
  FILE *f;
  if (!tile) tile = sys24_tile_create(SYS24_TILE_MASK_M2);
  sys24_tile_bind(tile, model2_tile_map_ptr(), model2_tile_char_ptr());
  sys24_tile_draw_layers_rgb32(tile, bitmap, model2_palram_ptr(), 0xff000000u, SYS24_PASS_ALL);
  for (int y = 0; y < h; y++)
  {
    uint8_t *row = raw + (size_t)y * (w * 3 + 1);
    row[0] = 0;
    for (int x = 0; x < w; x++)
    {
      const uint32_t v = bitmap[y * w + x];
      row[1 + x * 3] = v >> 16, row[2 + x * 3] = v >> 8, row[3 + x * 3] = v;
    }
  }
  z = malloc(zn);
  compress(z, &zn, raw, (uLong)h * (w * 3 + 1));
  PutU32(ihdr, w);
  PutU32(ihdr + 4, h);
  ihdr[8] = 8, ihdr[9] = 2;
  if ((f = fopen(path, "wb")))
  {
    fwrite("\x89PNG\r\n\x1a\n", 1, 8, f);
    Chunk(f, "IHDR", ihdr, 13);
    Chunk(f, "IDAT", z, (uint32_t)zn);
    Chunk(f, "IEND", NULL, 0);
    fclose(f);
  }
  free(raw);
  free(z);
}

static double NowMs(void)
{
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return t.tv_sec * 1000.0 + t.tv_nsec / 1e6;
}

static void RunFrame(void)
{
  s_pad = PadAt(s_frame);
  retro_run();
  s_frame++;
}

int main(int argc, char **argv)
{
  const char *zip = argc > 1 ? argv[1] : NULL, *options = NULL, *dump = NULL;
  long frames = 1200, hashEvery = 60, stateAt = -1, stateRun = 300, timedFrom = 0;
  long watch[16], shots[32];
  int shotCount = 0;
  const char *shotDir = ".";
  uint32_t watched[16];
  int watchCount = 0;
  struct retro_game_info info = { 0 };
  uint32_t lastMode = ~0u, lastInner = ~0u, lastScene = ~0u;
  int32_t lastRace = -1;
  double worst = 0, sum = 0;
  double *times;

  if (!zip) { fprintf(stderr, "usage: bench ZIP [--frames N] [--script SPEC] [--hash-every N] [--state F:M] [--options k=v,...] [--quiet]\n"); return 2; }
  for (int i = 2; i < argc; i++)
  {
    if (!strcmp(argv[i], "--frames") && i + 1 < argc) frames = atol(argv[++i]);
    else if (!strcmp(argv[i], "--script") && i + 1 < argc) ParseScript(argv[++i]);
    else if (!strcmp(argv[i], "--hash-every") && i + 1 < argc) hashEvery = atol(argv[++i]);
    else if (!strcmp(argv[i], "--state") && i + 1 < argc) sscanf(argv[++i], "%ld:%ld", &stateAt, &stateRun);
    else if (!strcmp(argv[i], "--options") && i + 1 < argc) options = argv[++i];
    else if (!strcmp(argv[i], "--quiet")) s_quiet = 1;
    else if (!strcmp(argv[i], "--no-video")) s_video = 0;
    else if (!strcmp(argv[i], "--timed-from") && i + 1 < argc) timedFrom = atol(argv[++i]);
    else if (!strcmp(argv[i], "--dump-ram") && i + 1 < argc) dump = argv[++i];
    else if (!strcmp(argv[i], "--shots") && i + 1 < argc)
    {
      char *p = argv[++i];
      while (*p && shotCount < 32)
      {
        shots[shotCount++] = strtol(p, &p, 10);
        if (*p == ',') p++;
      }
    }
    else if (!strcmp(argv[i], "--shot-dir") && i + 1 < argc) shotDir = argv[++i];
    else if (!strcmp(argv[i], "--watch") && i + 1 < argc)
    {
      char *p = argv[++i];
      while (*p && watchCount < 16)
      {
        watch[watchCount] = strtol(p, &p, 16);
        watched[watchCount++] = 0xdeadbeef;
        if (*p == ',') p++;
      }
    }
    else { fprintf(stderr, "bench: unknown argument %s\n", argv[i]); return 2; }
  }
  retro_set_environment(Environment);
  retro_set_video_refresh(Video);
  retro_set_audio_sample_batch(AudioBatch);
  retro_set_input_poll(InputPoll);
  retro_set_input_state(InputState);
  retro_init();
  if (options)
  {
    char buf[1024], *save = NULL;
    snprintf(buf, sizeof(buf), "%s", options);
    for (char *kv = strtok_r(buf, ",", &save); kv; kv = strtok_r(NULL, ",", &save))
    {
      char *eq = strchr(kv, '=');
      if (eq) { *eq = 0; srally_set(kv, eq + 1); }
    }
  }
  info.path = zip;
  {
    double t = NowMs();
    if (!retro_load_game(&info)) { fprintf(stderr, "bench: load failed\n"); return 1; }
    printf("load %.0f ms, state %zu bytes\n", NowMs() - t, retro_serialize_size());
  }
  times = calloc((size_t)frames + 1, sizeof(double));
  srally_timings();
  for (long f = 0; f < frames; f++)
  {
    double t = NowMs(), dt;
    RunFrame();
    dt = NowMs() - t;
    times[f] = dt;
    if (f >= timedFrom)
    {
      sum += dt;
      if (dt > worst) worst = dt;
    }
    if (f + 1 == timedFrom) srally_timings();
    {
      const uint32_t mode = Word(0x2098), inner = Word(0x209c), scene = Word(0x20ac);
      const int32_t race = (int32_t)Word(0x14120) > 0;
      if (mode != lastMode || inner != lastInner || scene != lastScene || race != lastRace)
      {
        printf("frame %ld: mode %u inner %u scene %u race %d\n", s_frame, mode, inner, scene, race);
        lastMode = mode, lastInner = inner, lastScene = scene, lastRace = race;
      }
    }
    for (int k = 0; k < shotCount; k++)
      if (shots[k] == s_frame)
      {
        char path[512];
        snprintf(path, sizeof(path), "%s/tiles-%06ld.png", shotDir, s_frame);
        TileShot(path);
      }
    for (int w = 0; w < watchCount; w++)
    {
      const uint32_t v = Word((size_t)watch[w]);
      if (v != watched[w]) printf("frame %ld: [%05lx] %08x -> %08x\n", s_frame, watch[w], watched[w], v);
      watched[w] = v;
    }
    if (hashEvery > 0 && s_frame % hashEvery == 0) printf("hash %ld %08x audio %016llx\n", s_frame, RamHash(), (unsigned long long)s_audioHash);
    if (s_frame % 600 == 0 && !s_quiet) printf("timings %ld %s stack %u heap %zu/%zu\n", s_frame, srally_timings(), srally_stack_used(), srally_heap_top(), srally_heap_peak());
    if (s_frame == stateAt)
    {
      const size_t size = retro_serialize_size();
      uint8_t *state = malloc(size);
      uint32_t *hashes = calloc((size_t)stateRun + 1, sizeof(uint32_t));
      uint64_t audio0, audio1;
      int same = 1;
      double ts = NowMs(), tl;
      if (!retro_serialize(state, size)) { printf("state: save failed\n"); return 1; }
      ts = NowMs() - ts;
      audio0 = s_audioHash;
      for (long i = 0; i < stateRun; i++) { RunFrame(); hashes[i] = RamHash(); }
      audio1 = s_audioHash;
      tl = NowMs();
      if (!retro_unserialize(state, size)) { printf("state: load failed\n"); return 1; }
      tl = NowMs() - tl;
      s_frame = stateAt;
      s_audioHash = audio0;
      for (long i = 0; i < stateRun; i++)
      {
        RunFrame();
        if (RamHash() != hashes[i])
        {
          if (same) printf("state: frame %ld differs after the load\n", s_frame);
          same = 0;
        }
      }
      printf("state: save %.1f ms, load %.1f ms, %zu bytes; %ld frames after the load %s, audio %s\n", ts, tl, size, stateRun,
             same ? "identical" : "DIFFERENT", audio1 == s_audioHash ? "identical" : "DIFFERENT");
      f += 2 * stateRun;
      free(hashes);
      free(state);
    }
  }
  {
    /* Frame times from --timed-from: average, p95, worst; and where they went. */
    long n = frames - timedFrom;
    double *sorted = malloc(sizeof(double) * (size_t)n);
    memcpy(sorted, times + timedFrom, sizeof(double) * (size_t)n);
    for (long i = 1; i < n; i++)
    {
      double v = sorted[i];
      long j = i - 1;
      while (j >= 0 && sorted[j] > v) { sorted[j + 1] = sorted[j]; j--; }
      sorted[j + 1] = v;
    }
    printf("frames %ld-%ld: avg %.2f ms, p95 %.2f ms, worst %.2f ms; samples %llu; stack %u bytes; heap peak %zu\n", timedFrom, frames,
           sum / n, sorted[(long)(n * 0.95)], worst, (unsigned long long)s_samples, srally_stack_used(), srally_heap_peak());
    if (timedFrom > 0) printf("timings %s\n", srally_timings());
    printf("final hash %08x audio %016llx\n", RamHash(), (unsigned long long)s_audioHash);
    if (dump)
    {
      FILE *f = fopen(dump, "wb");
      if (f) fwrite(retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM), 1, retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM), f), fclose(f);
    }
    free(sorted);
  }
  retro_unload_game();
  retro_deinit();
  return 0;
}
