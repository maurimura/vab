// Drives the same libretro shim natively, for the baseline the WebAssembly timings are compared
// against: supermodel/.cache/native/bench <Games.xml> <rom.zip> [frames] [warm-up frames] [PowerPC MHz]
// Prints ms per frame (average, median, 95th percentile, worst) and the save state's size and time.
#include "libretro.h"
#include <algorithm>
#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

extern "C" void supermodel_set(const char *key, const char *value);
extern "C" const char *supermodel_timings(void);

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
  if (argc < 3)
  {
    fprintf(stderr, "usage: bench <Games.xml> <rom.zip> [frames] [warmup] [ppc-mhz]\n");
    return 2;
  }
  int frames = argc > 3 ? atoi(argv[3]) : 600;
  int warmup = argc > 4 ? atoi(argv[4]) : 120;
  supermodel_set("GameXMLFile", argv[1]);
  supermodel_set("DataDir", "/tmp/supermodel-bench");
  if (argc > 5) supermodel_set("PowerPCFrequency", argv[5]);

  retro_set_environment(Environment);
  retro_set_video_refresh(Video);
  retro_set_audio_sample_batch(Audio);
  retro_set_input_poll(Poll);
  retro_set_input_state(State);
  retro_init();
  struct retro_game_info info = { argv[2], nullptr, 0, nullptr };
  if (!retro_load_game(&info)) return 1;

  for (int i = 0; i < warmup; i++) retro_run();
  supermodel_timings(); // drop the warm-up's
  std::vector<double> times;
  for (int i = 0; i < frames; i++)
  {
    double start = Now();
    retro_run();
    times.push_back(Now() - start);
  }
  std::vector<double> sorted = times;
  std::sort(sorted.begin(), sorted.end());
  double total = 0;
  for (double t : times) total += t;
  printf("native: %d frames, %.2f ms/frame average, %.2f median, %.2f p95, %.2f worst; %.1f audio frames per frame\n",
         frames, total / frames, sorted[frames / 2], sorted[(size_t)(frames * 0.95)], sorted.back(), (double)s_audioFrames / (warmup + frames));

  printf("  boards (microseconds over %d frames): %s\n", frames, supermodel_timings());

  size_t size = retro_serialize_size();
  std::vector<uint8_t> state(size);
  double start = Now();
  bool saved = retro_serialize(state.data(), size);
  double saveMs = Now() - start;
  start = Now();
  bool loaded = saved && retro_unserialize(state.data(), size);
  double loadMs = Now() - start;
  printf("state: %zu bytes, save %.2f ms (%s), load %.2f ms (%s)\n", size, saveMs, saved ? "ok" : "failed", loadMs, loaded ? "ok" : "failed");
  retro_unload_game();
  return 0;
}
