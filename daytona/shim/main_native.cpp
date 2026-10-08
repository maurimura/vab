// Drives the same libretro shim natively, for the baseline the WebAssembly timings are compared
// against: daytona/.cache/native/bench <daytona.zip> [frames] [warm-up frames] [cabinets]
// Prints ms per frame (average, median, 95th percentile, worst), where the time went, the save
// state's size and time, and an FNV-1a hash of cabinet 0's main RAM at the end (as bench.mjs).
// DAYTONA_OPTIONS="key=value,..." sets more daytona_set options first.
#include "libretro.h"
#include <algorithm>
#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

extern "C" void daytona_set(const char *key, const char *value);
extern "C" const char *daytona_timings(void);
extern "C" const char *daytona_link_status(void);

static void Log(enum retro_log_level level, const char *fmt, ...)
{
  if (level < RETRO_LOG_INFO) return;
  va_list vl;
  va_start(vl, fmt);
  vfprintf(stderr, fmt, vl);
  va_end(vl);
}

static bool Environment(unsigned cmd, void *data)
{
  switch (cmd)
  {
  case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: return *(enum retro_pixel_format *)data == RETRO_PIXEL_FORMAT_XRGB8888;
  case RETRO_ENVIRONMENT_GET_LOG_INTERFACE: ((struct retro_log_callback *)data)->log = Log; return true;
  case RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: return true;
  default: return false;
  }
}

static size_t s_audioFrames;
static void Video(const void *, unsigned, unsigned, size_t) {}
static size_t Audio(const int16_t *, size_t frames) { s_audioFrames += frames; return frames; }
static void Poll() {}
static int16_t State(unsigned, unsigned, unsigned, unsigned) { return 0; }

static double Now()
{
  using namespace std::chrono;
  return duration<double, std::milli>(steady_clock::now().time_since_epoch()).count();
}

int main(int argc, char **argv)
{
  if (argc < 2)
  {
    fprintf(stderr, "usage: bench <daytona.zip> [frames] [warmup] [cabinets]\n");
    return 2;
  }
  int frames = argc > 2 ? atoi(argv[2]) : 600;
  int warmup = argc > 3 ? atoi(argv[3]) : 120;
  if (argc > 4) daytona_set("cabinets", argv[4]);
  // DAYTONA_OPTIONS="key=value,key=value": more daytona_set options (e.g. the test_ ones).
  if (const char *options = getenv("DAYTONA_OPTIONS"))
  {
    std::string all = options;
    for (size_t at = 0; at < all.size();)
    {
      size_t end = all.find(',', at);
      if (end == std::string::npos) end = all.size();
      const std::string item = all.substr(at, end - at);
      const size_t eq = item.find('=');
      if (eq != std::string::npos) daytona_set(item.substr(0, eq).c_str(), item.substr(eq + 1).c_str());
      at = end + 1;
    }
  }

  retro_set_environment(Environment);
  retro_set_video_refresh(Video);
  retro_set_audio_sample_batch(Audio);
  retro_set_input_poll(Poll);
  retro_set_input_state(State);
  retro_init();
  struct retro_system_av_info av;
  retro_get_system_av_info(&av);
  printf("av: %ux%u, aspect %.4f, %.4f Hz, %.0f Hz audio\n", av.geometry.base_width, av.geometry.base_height,
         av.geometry.aspect_ratio, av.timing.fps, av.timing.sample_rate);
  struct retro_game_info info = { argv[1], nullptr, 0, nullptr };
  double start = Now();
  if (!retro_load_game(&info))
  {
    printf("load failed (see above)\n");
    return 1;
  }
  printf("loaded in %.2f s\n", (Now() - start) / 1000);

  for (int i = 0; i < warmup; i++) retro_run();
  daytona_timings(); // drop the warm-up's
  std::vector<double> times;
  for (int i = 0; i < frames; i++)
  {
    double t = Now();
    retro_run();
    times.push_back(Now() - t);
  }
  std::vector<double> sorted = times;
  std::sort(sorted.begin(), sorted.end());
  double total = 0;
  for (double t : times) total += t;
  printf("native: %d frames, %.2f ms/frame average, %.2f median, %.2f p95, %.2f worst; %.1f audio frames per frame\n",
         frames, total / frames, sorted[frames / 2], sorted[(size_t)(frames * 0.95)], sorted.back(), (double)s_audioFrames / (warmup + frames));
  printf("  timings (microseconds over %d frames): %s\n", frames, daytona_timings());
  printf("  link: %s\n", daytona_link_status());

  size_t size = retro_serialize_size();
  std::vector<uint8_t> state(size);
  start = Now();
  bool saved = retro_serialize(state.data(), size);
  double saveMs = Now() - start;
  start = Now();
  bool loaded = saved && retro_unserialize(state.data(), size);
  double loadMs = Now() - start;
  printf("state: %zu bytes, save %.2f ms (%s), load %.2f ms (%s)\n", size, saveMs, saved ? "ok" : "failed", loadMs, loaded ? "ok" : "failed");

  const uint8_t *ram = (const uint8_t *)retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM);
  size_t ramSize = retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM);
  // FNV-1a over 32-bit words, as web/emulator/worker.js's hashRam (and bench.mjs) compute it.
  uint32_t hash = 0x811c9dc5;
  for (size_t i = 0; ram && i + 4 <= ramSize; i += 4)
  {
    uint32_t word;
    memcpy(&word, ram + i, 4);
    hash = (hash ^ word) * 0x01000193u;
  }
  printf("main RAM: %zu bytes, hash %08x\n", ramSize, hash);
  retro_unload_game();
  return 0;
}
