// The arcade bar's side of Flycast's libretro shell: what RetroArch would do for a hardware-
// rendered core, done inside the module so web/emulator/libretro.js drives Flycast like the
// software cores (FBNeo, MAME) and Supermodel. patches/0001 hooks it in:
//
//   - retro_set_environment: the core's environment goes through Environment() below, which
//     answers RETRO_ENVIRONMENT_SET_HW_RENDER (and the preferred-renderer question) itself and
//     settles Flycast's core options (kOptions), passing everything else on to the frontend.
//   - retro_load_game: once the game is in, vab_game_loaded() makes the OpenGL context and calls
//     the core's context_reset, as RetroArch does when its video driver starts.
//   - retro_set_video_refresh: the core reports a frame with RETRO_HW_FRAME_BUFFER_VALID;
//     VideoRefresh() queues it for reading back from the GPU and hands the frontend RGBA bytes
//     (its pixel format 100) of the frame read back, or a duplicate (NULL) when none is due.
//   - retro_run: vab_frame_start() takes a queued frame out first (see "Reading frames back").
//
// Where it draws: in the page (the worker), the frontend gives the module an OffscreenCanvas
// (Module.canvas); the shim sizes it, makes a WebGL2 context on it and Flycast draws into its
// default framebuffer. Without a canvas (Node: bench.mjs, check.mjs) Flycast gets the no-op GL
// (gl_null.mjs) through the same get_proc_address: one build for both, and the machine runs the
// same either way, because nothing drawn ever goes back into it (kOptions: no render-to-VRAM, no
// framebuffer emulation).
#include <libretro.h>
#include <GLES3/gl3.h>
#include <emscripten.h>
#include <emscripten/html5.h>
#include <algorithm>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

extern "C" void (*vab_null_gl_proc(const char *name))(void);
// WebGL2 has it (Emscripten implements it), OpenGL ES 3.0 headers don't.
extern "C" void glGetBufferSubData(GLenum target, GLintptr offset, GLsizeiptr size, void *data);
extern "C" void retro_get_system_av_info(struct retro_system_av_info *info);
unsigned long long sh4_sched_now64(); // core/hw/sh4/sh4_sched.cpp: SH4 cycles (200 MHz) so far

namespace {

enum class GL { None, WebGL, Null };

retro_environment_t s_frontend;
retro_video_refresh_t s_video;
retro_hw_render_callback s_hw;
bool s_hwRequested;
GL s_gl = GL::None;
bool s_rgbaFrames = true;      // the frontend takes RGBA bytes (format 100); else XRGB8888
unsigned s_canvasW, s_canvasH;
std::vector<uint8_t> s_frame;  // the frame for the frontend, top row first
std::map<std::string, std::string> s_overrides; // flycast_set()

// Statistics for bench.mjs and the harness (flycast_stats).
struct Stats
{
  unsigned runs, presented, delivered, dropped;
  double readbackMs, copyMs, getMs, frontendMs;
  unsigned long long cycles; // guest time covered by `timedRuns` of the runs
  unsigned timedRuns;
} s_stats;
unsigned long long s_lastCycles;

void Log(retro_log_level level, const char *fmt, ...)
{
  char text[512];
  va_list args;
  va_start(args, fmt);
  vsnprintf(text, sizeof(text), fmt, args);
  va_end(args);
  retro_log_callback log{};
  if (s_frontend && s_frontend(RETRO_ENVIRONMENT_GET_LOG_INTERFACE, &log) && log.log)
    log.log(level, "[flycast] %s\n", text);
  else
    fprintf(stderr, "[flycast] %s\n", text);
}

/******************************************************************************
 Core options
******************************************************************************/

// Flycast's libretro options are named reicast_* (shell/libretro/libretro_core_option_defines.h).
// Forced ones keep the machine exactly the same everywhere and the frame loop the bar's; the rest
// are defaults that flycast_set() (games.ron options) or the frontend's OPTIONS map can change.
struct Option
{
  const char *key;
  const char *value;
  bool forced;
};
const Option kOptions[] = {
  // One emulated frame per retro_run, on the calling thread: no render thread (there are no
  // threads), no frame skipping, no swap-interval guessing (it would change the fps mid-game).
  { "reicast_threaded_rendering", "disabled", true },
  { "reicast_auto_skip_frame", "disabled", true },
  { "reicast_frame_skipping", "disabled", true },
  { "reicast_detect_vsync_swap_interval", "disabled", true },
  // Nothing drawn by the GPU goes back into the machine: what the host GPU rasterises (and the
  // no-op GL in Node doesn't) would otherwise land in VRAM and make players' machines differ.
  { "reicast_enable_rttb", "disabled", true },
  { "reicast_emulate_framebuffer", "disabled", true },
  // The real SH4 clock, the real BIOS, nothing from the network or the disk.
  { "reicast_sh4clock", "200", true },
  { "reicast_hle_bios", "disabled", true },
  { "reicast_emulate_bba", "disabled", true },
  { "reicast_upnp", "disabled", true },
  { "reicast_dcnet", "disabled", true },
  { "reicast_network_output", "disabled", true },
  { "reicast_custom_textures", "disabled", true },
  { "reicast_preload_custom_textures", "disabled", true },
  { "reicast_dump_textures", "disabled", true },
  { "reicast_dump_replaced_textures", "disabled", true },
  { "reicast_gdrom_fast_loading", "disabled", true },
  // Defaults, as the bar's frontend sets them too (web/emulator/libretro.js OPTIONS): free play
  // (Start plays; Select still drops a coin), the USA BIOS (Virtua Tennis rather than Power
  // Smash), the NAOMI's own 640x480, the cheaper per-triangle sorting (per-pixel needs OpenGL
  // 4.3), the AICA's DSP (reverb and filters; costs some CPU), and a frame counted as new when
  // the game swaps to it rather than when it starts drawing it. None of these change how the
  // machine runs from one player's computer to another's.
  { "reicast_force_freeplay", "enabled", false },
  { "reicast_delay_frame_swapping", "enabled", false },
  { "reicast_region", "USA", false },
  { "reicast_language", "English", false },
  { "reicast_broadcast", "NTSC", false },
  { "reicast_cable_type", "VGA", false },
  { "reicast_internal_resolution", "640x480", false },
  { "reicast_alpha_sorting", "per-triangle (normal)", false },
  { "reicast_enable_dsp", "enabled", false },
  { "reicast_allow_service_buttons", "disabled", false },
  { "reicast_screen_rotation", "horizontal", false },
  { "reicast_widescreen_hack", "disabled", false },
  { "reicast_widescreen_cheats", "disabled", false },
  { "reicast_texupscale", "1", false },
  { "reicast_mipmapping", "enabled", false },
  { "reicast_anisotropic_filtering", "off", false },
  { "reicast_texture_filtering", "0", false },
  { "reicast_pvr2_filtering", "disabled", false },
  { "reicast_native_depth_interpolation", "disabled", false },
  { "reicast_fog", "enabled", false },
  { "reicast_volume_modifier_enable", "enabled", false },
  { "reicast_vmu_sound", "disabled", false },
  { "reicast_per_content_vmus", "disabled", false },
  { "reicast_lightgun1_crosshair", "disabled", false },
  { "reicast_lightgun2_crosshair", "disabled", false },
  { "reicast_lightgun3_crosshair", "disabled", false },
  { "reicast_lightgun4_crosshair", "disabled", false },
};

const Option *FindOption(const char *key)
{
  for (const Option &o : kOptions)
    if (!strcmp(o.key, key)) return &o;
  return nullptr;
}

// Forced, then flycast_set(), then the frontend's own (its OPTIONS map), then our default.
bool GetVariable(retro_variable *var)
{
  if (!var || !var->key) return false;
  const Option *option = FindOption(var->key);
  if (option && option->forced)
  {
    var->value = option->value;
    return true;
  }
  auto set = s_overrides.find(var->key);
  if (set != s_overrides.end())
  {
    var->value = set->second.c_str();
    return true;
  }
  if (s_frontend(RETRO_ENVIRONMENT_GET_VARIABLE, var) && var->value) return true;
  if (option)
  {
    var->value = option->value;
    return true;
  }
  var->value = nullptr;
  return false;
}

/******************************************************************************
 OpenGL
******************************************************************************/

uintptr_t CurrentFramebuffer()
{
  return 0; // the canvas's own
}

// WebGL2 reports itself as "OpenGL ES 3.0 (WebGL 2.0 (...))"; Flycast only needs the plain form.
const GLubyte *GetStringWebGL(GLenum name)
{
  if (name == GL_VERSION) return (const GLubyte *)"OpenGL ES 3.0 WebGL 2.0";
  if (name == GL_SHADING_LANGUAGE_VERSION) return (const GLubyte *)"OpenGL ES GLSL ES 3.00";
  return glGetString(name);
}

retro_proc_address_t ProcAddress(const char *sym)
{
  if (s_gl == GL::Null) return (retro_proc_address_t)vab_null_gl_proc(sym);
  if (!strcmp(sym, "glGetString")) return (retro_proc_address_t)GetStringWebGL;
  return (retro_proc_address_t)emscripten_webgl_get_proc_address(sym);
}

bool CreateWebGL(unsigned width, unsigned height)
{
  if (!EM_ASM_INT({ return Module["canvas"] ? 1 : 0; })) return false;
  EmscriptenWebGLContextAttributes attrs;
  emscripten_webgl_init_context_attributes(&attrs);
  attrs.majorVersion = 2;
  attrs.minorVersion = 0;
  attrs.alpha = false; // read back with A = 255
  attrs.depth = true;
  attrs.stencil = true;
  attrs.antialias = false;
  attrs.preserveDrawingBuffer = false;
  attrs.powerPreference = EM_WEBGL_POWER_PREFERENCE_HIGH_PERFORMANCE;
  // The frontend's canvas (an OffscreenCanvas in the worker) comes at any size: the drawing
  // buffer is made big enough for Flycast's output. Emscripten finds it through this table.
  EM_ASM({
    const canvas = Module["canvas"];
    canvas.width = $0;
    canvas.height = $1;
    Module["specialHTMLTargets"]["!canvas"] = canvas;
  }, width, height);
  EMSCRIPTEN_WEBGL_CONTEXT_HANDLE context = emscripten_webgl_create_context("!canvas", &attrs);
  if (context <= 0)
  {
    Log(RETRO_LOG_ERROR, "No WebGL2 context on the module's canvas (%d): the game runs but draws nothing.", (int)context);
    return false;
  }
  emscripten_webgl_make_context_current(context);
  s_canvasW = width;
  s_canvasH = height;
  return true;
}

/******************************************************************************
 Reading frames back
******************************************************************************/

// Reading a frame back synchronously waits for the GPU, and in Chrome for its GPU process to run
// every command queued before it (Flycast's frame is thousands of them). So a presented frame is
// only queued for reading (glReadPixels into a PIXEL_PACK buffer) at the end of retro_run, and
// taken out (glGetBufferSubData) at the start of a later retro_run, before that frame's own
// drawing is queued. The frontend gets each frame that many retro_runs late, a duplicate when
// none is due. Measured in a Virtua Tennis match (Chrome, M-series Mac): taking it out at the end
// of the same retro_run costs ~3.5 ms; at the start of the next ~1-5 ms (the GPU is often not
// done yet); two retro_runs later ~0.7-2 ms. Two is the default; flycast_set("vab_readback_latency",
// "1") trades that back for a frame less of latency.
struct PixelBuffer
{
  GLuint buffer = 0;
  size_t size = 0;
  unsigned width = 0, height = 0;
  unsigned age = 0;             // retro_runs since it was queued (0: nothing queued)
};
PixelBuffer s_pixelBuffers[2];
unsigned s_latency = 2;
bool s_frameReady;              // s_frame holds a frame for this retro_run's video callback
unsigned s_frameW, s_frameH;

bool VideoWanted()
{
  // Bit 0: the frontend shows this frame (it doesn't while re-running frames for a rollback).
  int enable = 3;
  if (!s_frontend(RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE, &enable)) enable = 3;
  return enable & 1;
}

void StartReadback(unsigned width, unsigned height)
{
  if (width > s_canvasW || height > s_canvasH)
  {
    // Flycast drew past the canvas (a bigger resolution than the av info said): grow it for the
    // next frames; this one is lost.
    unsigned w = std::max(width, s_canvasW), h = std::max(height, s_canvasH);
    Log(RETRO_LOG_WARN, "frame %ux%u is bigger than the canvas %ux%u: resizing", width, height, s_canvasW, s_canvasH);
    EM_ASM({ Module["canvas"].width = $0; Module["canvas"].height = $1; }, w, h);
    s_canvasW = w;
    s_canvasH = h;
    return;
  }
  double start = emscripten_get_now();
  PixelBuffer *p = !s_pixelBuffers[0].age ? &s_pixelBuffers[0] : !s_pixelBuffers[1].age ? &s_pixelBuffers[1]
    : s_pixelBuffers[0].age > s_pixelBuffers[1].age ? &s_pixelBuffers[0] : &s_pixelBuffers[1];
  if (p->age) s_stats.dropped++; // the oldest, never taken out: overwritten
  size_t bytes = (size_t)width * height * 4;
  if (!p->buffer) glGenBuffers(1, &p->buffer);
  glBindBuffer(GL_PIXEL_PACK_BUFFER, p->buffer);
  if (p->size < bytes)
  {
    glBufferData(GL_PIXEL_PACK_BUFFER, bytes, nullptr, GL_STREAM_READ);
    p->size = bytes;
  }
  glBindFramebuffer(GL_READ_FRAMEBUFFER, 0);
  glPixelStorei(GL_PACK_ALIGNMENT, 4);
  glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, nullptr);
  glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);
  glFlush();
  p->age = 1;
  p->width = width;
  p->height = height;
  s_stats.readbackMs += emscripten_get_now() - start;
}

// A queued frame, out of its pixel buffer into s_frame, top row first.
void FinishReadback(PixelBuffer &p)
{
  double start = emscripten_get_now();
  unsigned width = p.width, height = p.height;
  size_t row = (size_t)width * 4;
  static std::vector<uint8_t> raw;
  raw.resize(row * height);
  s_frame.resize(row * height);
  glBindBuffer(GL_PIXEL_PACK_BUFFER, p.buffer);
  glGetBufferSubData(GL_PIXEL_PACK_BUFFER, 0, raw.size(), raw.data());
  glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);
  s_stats.getMs += emscripten_get_now() - start;
  // OpenGL's rows run bottom to top.
  for (unsigned y = 0; y < height; y++)
  {
    const uint8_t *src = &raw[(height - 1 - y) * row];
    uint8_t *dst = &s_frame[y * row];
    if (s_rgbaFrames)
    {
      memcpy(dst, src, row);
      continue;
    }
    uint32_t *out = (uint32_t *)dst;
    for (unsigned x = 0; x < width; x++, src += 4)
      out[x] = 0xFF000000u | ((uint32_t)src[0] << 16) | ((uint32_t)src[1] << 8) | src[2];
  }
  p.age = 0;
  s_frameReady = true;
  s_frameW = width;
  s_frameH = height;
  s_stats.copyMs += emscripten_get_now() - start;
}

void VideoRefresh(const void *data, unsigned width, unsigned height, size_t pitch)
{
  s_stats.runs++;
  unsigned long long now = sh4_sched_now64();
  if (s_lastCycles && now > s_lastCycles && now - s_lastCycles < 200000000ull)
  {
    s_stats.cycles += now - s_lastCycles;
    s_stats.timedRuns++;
  }
  s_lastCycles = now;
  if (data == RETRO_HW_FRAME_BUFFER_VALID)
  {
    s_stats.presented++;
    if (s_gl == GL::WebGL && VideoWanted()) StartReadback(width, height);
    else if (s_gl == GL::Null && VideoWanted())
    {
      // The no-op GL drew nothing: a black frame of that size, right away, so a frontend with no
      // canvas (Node: emulator/netplay-check.mjs) sees the machine present frames as the page's
      // does. Opaque black is the same bytes in both pixel formats.
      s_frame.resize((size_t)width * height * 4);
      std::fill_n((uint32_t *)s_frame.data(), (size_t)width * height, 0xFF000000u);
      s_frameReady = true;
      s_frameW = width;
      s_frameH = height;
    }
  }
  else if (data)
  {
    s_video(data, width, height, pitch); // a software frame (Flycast doesn't make them)
    return;
  }
  if (s_frameReady)
  {
    s_frameReady = false;
    s_stats.delivered++;
    double start = emscripten_get_now();
    s_video(s_frame.data(), s_frameW, s_frameH, (size_t)s_frameW * 4);
    s_stats.frontendMs += emscripten_get_now() - start;
    return;
  }
  s_video(nullptr, width, height, 0);
}

/******************************************************************************
 The environment
******************************************************************************/

bool Environment(unsigned cmd, void *data)
{
  switch (cmd)
  {
  case RETRO_ENVIRONMENT_SET_HW_RENDER:
  {
    auto *hw = (retro_hw_render_callback *)data;
    hw->get_current_framebuffer = CurrentFramebuffer;
    hw->get_proc_address = ProcAddress;
    s_hw = *hw;
    s_hwRequested = true;
    return true;
  }
  case RETRO_ENVIRONMENT_GET_PREFERRED_HW_RENDER:
    *(unsigned *)data = RETRO_HW_CONTEXT_OPENGLES3;
    return true;
  case RETRO_ENVIRONMENT_GET_HW_RENDER_INTERFACE:
  case RETRO_ENVIRONMENT_SET_HW_SHARED_CONTEXT:
    return false;
  case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
    return true; // the core's software format; ours was settled with the frontend (vab_wrap_environment)
  case RETRO_ENVIRONMENT_GET_VARIABLE:
    return GetVariable((retro_variable *)data);
  default:
    return s_frontend(cmd, data);
  }
}

} // namespace

/******************************************************************************
 Hooks (patches/0001) and exports
******************************************************************************/

extern "C" retro_environment_t vab_wrap_environment(retro_environment_t frontend)
{
  s_frontend = frontend;
  // Our frontend (web/emulator/libretro.js) takes RGBA bytes as WebGL reads them back (100).
  unsigned format = 100;
  s_rgbaFrames = frontend(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  if (!s_rgbaFrames)
  {
    format = RETRO_PIXEL_FORMAT_XRGB8888;
    frontend(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  }
  // Flycast keeps its data (BIOS lookups, NVRAM) in <system>/dc and its arcade saves under
  // <save>/reicast: make sure they exist in the in-memory file system.
  EM_ASM({
    try { FS.mkdirTree("/system/dc"); } catch (e) {}
    try { FS.mkdirTree("/save/reicast"); } catch (e) {}
  });
  return Environment;
}

extern "C" retro_video_refresh_t vab_wrap_video(retro_video_refresh_t frontend)
{
  s_video = frontend;
  return VideoRefresh;
}

extern "C" void vab_frame_start(void)
{
  if (s_gl != GL::WebGL) return;
  // The oldest queued frame once it has waited s_latency retro_runs.
  PixelBuffer *oldest = nullptr;
  for (PixelBuffer &p : s_pixelBuffers)
    if (p.age && (!oldest || p.age > oldest->age)) oldest = &p;
  if (oldest && oldest->age >= s_latency) FinishReadback(*oldest);
  for (PixelBuffer &p : s_pixelBuffers)
    if (p.age) p.age++;
}

extern "C" void vab_game_loaded(void)
{
  if (!s_hwRequested)
  {
    Log(RETRO_LOG_ERROR, "Flycast asked for no OpenGL context: nothing will be drawn.");
    return;
  }
  if (s_gl == GL::None)
  {
    // As big as Flycast's output can get at its resolution (16:9 at the rendering height).
    retro_system_av_info av{};
    retro_get_system_av_info(&av);
    unsigned width = std::max(640u, av.geometry.max_width);
    unsigned height = std::max(480u, width * 9 / 16);
    s_gl = CreateWebGL(width, height) ? GL::WebGL : GL::Null;
    Log(RETRO_LOG_INFO, "drawing with %s", s_gl == GL::WebGL ? "WebGL2" : "the no-op GL (no canvas)");
  }
  if (s_hw.context_reset) s_hw.context_reset();
}

// A core option for the next game load (Core.setOption: games.ron's options), e.g.
// flycast_set("reicast_region", "Japan"). Forced options (kOptions) can't be changed.
extern "C" EMSCRIPTEN_KEEPALIVE void flycast_set(const char *key, const char *value)
{
  if (!key || !value) return;
  if (!strcmp(key, "vab_readback_latency"))
  {
    s_latency = strtoul(value, nullptr, 10) >= 2 ? 2 : 1;
    return;
  }
  const Option *option = FindOption(key);
  if (option && option->forced)
  {
    Log(RETRO_LOG_WARN, "%s is fixed at %s in this build", key, option->value);
    return;
  }
  s_overrides[key] = value;
}

// Frames since the last call, as JSON, for bench.mjs and the harness: retro_runs, frames the game
// presented, frames handed to the frontend, read-backs skipped, and the read-back's own cost.
extern "C" EMSCRIPTEN_KEEPALIVE const char *flycast_stats(void)
{
  static char text[256];
  snprintf(text, sizeof(text),
           "{\"runs\":%u,\"presented\":%u,\"delivered\":%u,\"dropped\":%u,\"readbackMs\":%.3f,\"copyMs\":%.3f,\"getMs\":%.3f,\"frontendMs\":%.3f,\"gl\":\"%s\",\"cycles\":%llu,\"timedRuns\":%u}",
           s_stats.runs, s_stats.presented, s_stats.delivered, s_stats.dropped, s_stats.readbackMs, s_stats.copyMs, s_stats.getMs, s_stats.frontendMs,
           s_gl == GL::WebGL ? "webgl2" : s_gl == GL::Null ? "null" : "none", s_stats.cycles, s_stats.timedRuns);
  s_stats = Stats{};
  return text;
}
