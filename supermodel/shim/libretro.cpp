// Supermodel (Sega Model 3) behind the libretro API, so web/emulator/libretro.js and worker.js
// drive it like an FBNeo core: load a ROM set, run a frame, read the picture and sound, save and
// load the machine.
//
// The frontend decides what is drawn on: in the page the module gets an OffscreenCanvas
// (Module.canvas) and a WebGL2 context is made on it; in Node and the native build there is no
// canvas and the OpenGL calls land in the no-op GL (gl_null.c), so frames come out black but the
// whole machine, scene walk included, still runs and can be timed (bench.mjs).
#include "libretro.h"
#include "Supermodel.h"
#include "BlockFile.h"
#include "GameLoader.h"
#include "Graphics/New3D/New3D.h"
#include "Graphics/Render2D.h"
#include "Graphics/SuperAA.h"
#include "Inputs/Inputs.h"
#include "Model3/Model3.h"
#include "OSD/FileSystemPath.h"
#include "RetroInputSystem.h"
#include "Util/NewConfig.h"
#include "osd/RetroAudio.h"
#include <GL/glew.h>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <memory>
#include <string>
#include <ctime>
#include <sys/stat.h>
#include <vector>
#ifdef __EMSCRIPTEN__
#include <emscripten/em_asm.h>
#include <emscripten/html5.h>
#endif

extern "C" void sm_set_data_dir(const char *dir);
extern "C" void ppc_jit_set_enabled(int on); // recompiler toggle (ppc.cpp)
// patches/0002: the Model 3's clock chip reads this moment instead of the host's clock, so
// players online keep identical machines. Wednesday 1998-04-01 12:00:00 UTC.
extern time_t supermodel_fixed_time;
// Defined by gl_null.c: present only in the builds whose GL is the no-op one.
extern "C" int sm_gl_is_null __attribute__((weak));

static const unsigned WIDTH = 496;
static const unsigned HEIGHT = 384;
static const int STATE_FILE_VERSION = 6; // as Supermodel's own save states (OSD/SDL/Main.cpp)
static const char *const STATE_BLOCK = "Supermodel Save State";

static retro_environment_t environ_cb;
static retro_video_refresh_t video_cb;
static retro_audio_sample_batch_t audio_batch_cb;
static retro_input_poll_t input_poll_cb;
static retro_input_state_t input_state_cb;
static retro_log_printf_t log_cb;

static Util::Config::Node s_config("Global");
static std::string s_gamesXml = "/Games.xml";
static std::string s_dataDir = "/supermodel";
static std::unique_ptr<CModel3> s_model3;
static Game s_game;
static CInputs *s_inputs;
static CRender2D *s_render2D;
static IRender3D *s_render3D;
static SuperAA *s_superAA;
static bool s_glReady;
static size_t s_stateSize;
static std::vector<uint8_t> s_rgba;    // the frame as OpenGL reads it back, bottom row first
static std::vector<uint32_t> s_frame;  // the frame for the frontend, top row first
static bool s_rgbaFrames;              // the frontend takes RGBA bytes as they are (else XRGB8888)
static uint64_t s_sum[7];              // microseconds per part of a frame since supermodel_timings()
static uint64_t s_sumFrames;

/******************************************************************************
 Logging
******************************************************************************/

class RetroLogger : public CLogger
{
  void Log(retro_log_level level, const char *fmt, va_list vl)
  {
    char text[2048];
    vsnprintf(text, sizeof(text), fmt, vl);
    size_t n = strlen(text);
    while (n > 0 && (text[n - 1] == '\n' || text[n - 1] == '\r')) text[--n] = 0;
    if (!n) return;
    if (log_cb) log_cb(level, "%s\n", text);
    else fprintf(stderr, "%s\n", text);
  }
  void DebugLog(const char *fmt, va_list vl) override { (void)fmt; (void)vl; } // too chatty for the console
  void InfoLog(const char *fmt, va_list vl) override { Log(RETRO_LOG_INFO, fmt, vl); }
  void ErrorLog(const char *fmt, va_list vl) override { Log(RETRO_LOG_ERROR, fmt, vl); }
};

/******************************************************************************
 Configuration
******************************************************************************/

// Which Supermodel input stands for which RetroPad button. Cabinet buttons left to right (short
// pass, long pass, shoot; punch, kick, guard, escape) go on the pad's bottom row Y B A then X.
struct Mapping
{
  const char *input;    // Supermodel's input id (Inputs/Inputs.cpp), without the "Input" prefix
  unsigned port;        // RetroPad port
  unsigned id;          // RETRO_DEVICE_ID_JOYPAD_*
  uint32_t game_flags;  // Game::Inputs this belongs to
  const char *label;
};
static const Mapping kMappings[] = {
  { "Start1", 0, RETRO_DEVICE_ID_JOYPAD_START, Game::INPUT_COMMON, "Start" },
  { "Coin1", 0, RETRO_DEVICE_ID_JOYPAD_SELECT, Game::INPUT_COMMON, "Coin" },
  { "Start2", 1, RETRO_DEVICE_ID_JOYPAD_START, Game::INPUT_COMMON, "Start" },
  { "Coin2", 1, RETRO_DEVICE_ID_JOYPAD_SELECT, Game::INPUT_COMMON, "Coin" },
  { "JoyUp", 0, RETRO_DEVICE_ID_JOYPAD_UP, Game::INPUT_JOYSTICK1, "Up" },
  { "JoyDown", 0, RETRO_DEVICE_ID_JOYPAD_DOWN, Game::INPUT_JOYSTICK1, "Down" },
  { "JoyLeft", 0, RETRO_DEVICE_ID_JOYPAD_LEFT, Game::INPUT_JOYSTICK1, "Left" },
  { "JoyRight", 0, RETRO_DEVICE_ID_JOYPAD_RIGHT, Game::INPUT_JOYSTICK1, "Right" },
  { "JoyUp2", 1, RETRO_DEVICE_ID_JOYPAD_UP, Game::INPUT_JOYSTICK2, "Up" },
  { "JoyDown2", 1, RETRO_DEVICE_ID_JOYPAD_DOWN, Game::INPUT_JOYSTICK2, "Down" },
  { "JoyLeft2", 1, RETRO_DEVICE_ID_JOYPAD_LEFT, Game::INPUT_JOYSTICK2, "Left" },
  { "JoyRight2", 1, RETRO_DEVICE_ID_JOYPAD_RIGHT, Game::INPUT_JOYSTICK2, "Right" },
  { "ShortPass", 0, RETRO_DEVICE_ID_JOYPAD_Y, Game::INPUT_SOCCER, "Short Pass" },
  { "LongPass", 0, RETRO_DEVICE_ID_JOYPAD_B, Game::INPUT_SOCCER, "Long Pass" },
  { "Shoot", 0, RETRO_DEVICE_ID_JOYPAD_A, Game::INPUT_SOCCER, "Shoot" },
  { "ShortPass2", 1, RETRO_DEVICE_ID_JOYPAD_Y, Game::INPUT_SOCCER, "Short Pass" },
  { "LongPass2", 1, RETRO_DEVICE_ID_JOYPAD_B, Game::INPUT_SOCCER, "Long Pass" },
  { "Shoot2", 1, RETRO_DEVICE_ID_JOYPAD_A, Game::INPUT_SOCCER, "Shoot" },
  { "Punch", 0, RETRO_DEVICE_ID_JOYPAD_Y, Game::INPUT_FIGHTING, "Punch" },
  { "Kick", 0, RETRO_DEVICE_ID_JOYPAD_B, Game::INPUT_FIGHTING, "Kick" },
  { "Guard", 0, RETRO_DEVICE_ID_JOYPAD_A, Game::INPUT_FIGHTING, "Guard" },
  { "Escape", 0, RETRO_DEVICE_ID_JOYPAD_X, Game::INPUT_FIGHTING, "Escape" },
  { "Punch2", 1, RETRO_DEVICE_ID_JOYPAD_Y, Game::INPUT_FIGHTING, "Punch" },
  { "Kick2", 1, RETRO_DEVICE_ID_JOYPAD_B, Game::INPUT_FIGHTING, "Kick" },
  { "Guard2", 1, RETRO_DEVICE_ID_JOYPAD_A, Game::INPUT_FIGHTING, "Guard" },
  { "Escape2", 1, RETRO_DEVICE_ID_JOYPAD_X, Game::INPUT_FIGHTING, "Escape" },
};

// Supermodel's defaults (OSD/SDL/Main.cpp DefaultConfig), single-threaded and without a window.
static void SetDefaultConfig(Util::Config::Node &config)
{
  // The real clock (0: 166 MHz on Step 2.x) costs twice a frame's budget in WebAssembly without a
  // recompiler (README); 50 MHz, Supermodel's default for years, runs the games at 60 fps.
  config.Set("PowerPCFrequency", 50u);
  config.Set("MultiThreaded", false);
  config.Set("GPUMultiThreaded", false);
  config.Set("EmulateSound", true);
  config.Set("EmulateDSB", true);
  config.Set("SoundVolume", 100);
  config.Set("MusicVolume", 100);
  config.Set("LegacySoundDSP", false);
  config.Set("FlipStereo", false);
  config.Set("ForceFeedback", false);
  config.Set("New3DEngine", true);
  config.Set("QuadRendering", false);
  config.Set("WideScreen", false);
  config.Set("WideBackground", false);
  config.Set("NoWhiteFlash", false);
  config.Set("Network", false);
  config.Set("SimulateNet", true);
  config.Set("PortIn", 1970u);
  config.Set("PortOut", 1971u);
  config.Set<std::string>("AddressOut", "127.0.0.1");
  config.Set("DumpMemory", false);
  config.Set("DumpTextures", false);
  config.Set("Balance", 0.0f);
  config.Set("BalanceLeftRight", 0.0f);
  config.Set("BalanceFrontRear", 0.0f);
  config.Set("NbSoundChannels", 4);
  config.Set("SoundFreq", 57.6f);
  config.Set("MultiTexture", false);
  config.Set<std::string>("VertexShader", "");
  config.Set<std::string>("FragmentShader", "");
  config.Set("Supersampling", 1);
  config.Set("CRTcolors", 0);
  config.Set("UpscaleMode", 0);
  config.Set("XResolution", 496u);
  config.Set("YResolution", 384u);
  config.Set("Stretch", false);
  config.Set("Crosshairs", 0);
  config.Set<std::string>("Outputs", "none");
  for (const Mapping &m : kMappings)
  {
    std::string mapping = "KEY" + std::to_string(m.port + 1) + "_" + CRetroInputSystem::KeyName(m.id);
    config.Set(std::string("Input") + m.input, mapping);
  }
}

// Settings a frontend changes before the game loads (bench.mjs, main_native.cpp): a few typed
// keys of Supermodel's configuration and the two paths this build adds. Kept and applied over
// the defaults when the game loads, whenever they were set.
static std::vector<std::pair<std::string, std::string>> s_overrides;

extern "C" void supermodel_set(const char *key, const char *value)
{
  s_overrides.emplace_back(key, value);
}

static void ApplyOverrides(Util::Config::Node &config)
{
  for (const auto &[k, v] : s_overrides)
  {
    if (k == "GameXMLFile") s_gamesXml = v;
    else if (k == "DataDir") { s_dataDir = v; sm_set_data_dir(v.c_str()); }
    else if (k == "PowerPCFrequency") config.Set(k, (unsigned)strtoul(v.c_str(), nullptr, 10));
    else if (k == "Jit") ppc_jit_set_enabled(v == "true");
    else if (k == "SoundVolume" || k == "MusicVolume") config.Set(k, (int)strtol(v.c_str(), nullptr, 10));
    else if (v == "true" || v == "false") config.Set(k, v == "true");
    else config.Set(k, v);
  }
}

// Where the frames since the last call went, in microseconds (OSD/Thread.cpp's ticks), as JSON:
// {"frames":n,"ppc":..,"render":..,"sound":..,"drive":..,"total":..,"readback":..,"convert":..}
// (render includes the read-back and the conversion). For bench.mjs and the harness.
extern "C" const char *supermodel_timings(void)
{
  static char text[320];
  snprintf(text, sizeof(text), "{\"frames\":%llu,\"ppc\":%llu,\"render\":%llu,\"sound\":%llu,\"drive\":%llu,\"total\":%llu,\"readback\":%llu,\"convert\":%llu}",
           (unsigned long long)s_sumFrames, (unsigned long long)s_sum[0], (unsigned long long)s_sum[1],
           (unsigned long long)s_sum[2], (unsigned long long)s_sum[3], (unsigned long long)s_sum[4],
           (unsigned long long)s_sum[5], (unsigned long long)s_sum[6]);
  memset(s_sum, 0, sizeof(s_sum));
  s_sumFrames = 0;
  return text;
}

/******************************************************************************
 Video: Supermodel's OSD hooks around a frame, and the read-back for the frontend
******************************************************************************/

bool BeginFrameVideo()
{
  return s_glReady;
}

// Reading the frame back synchronously waits for the GPU, 4 to 5 ms of a frame's budget in
// Chrome. Instead each frame is read into a pixel buffer behind a fence and taken out a frame
// later, once the fence says the GPU is done: the picture the frontend gets is the previous
// frame's. A frame whose fence isn't done yet keeps the picture before it.
static GLuint s_pixelBuffers[2];
static GLsync s_fences[2];
static unsigned s_frames;

void EndFrameVideo()
{
  if (!s_glReady || &sm_gl_is_null) return; // nothing to read back from the no-op GL
  UINT32 start = CThread::GetTicks();
  if (!s_pixelBuffers[0])
  {
    glGenBuffers(2, s_pixelBuffers);
    for (GLuint buffer : s_pixelBuffers)
    {
      glBindBuffer(GL_PIXEL_PACK_BUFFER, buffer);
      glBufferData(GL_PIXEL_PACK_BUFFER, WIDTH * HEIGHT * 4, nullptr, GL_STREAM_READ);
    }
  }
  unsigned now = s_frames & 1, before = now ^ 1;
  glBindFramebuffer(GL_FRAMEBUFFER, 0);
  glBindBuffer(GL_PIXEL_PACK_BUFFER, s_pixelBuffers[now]);
  glReadPixels(0, 0, WIDTH, HEIGHT, GL_RGBA, GL_UNSIGNED_BYTE, nullptr);
  if (s_fences[now]) glDeleteSync(s_fences[now]);
  s_fences[now] = glFenceSync(GL_SYNC_GPU_COMMANDS_COMPLETE, 0);
  glFlush();
  s_frames++;
  bool ready = s_fences[before] != nullptr;
  if (ready)
  {
    GLenum state = glClientWaitSync(s_fences[before], 0, 0);
    ready = state == GL_ALREADY_SIGNALED || state == GL_CONDITION_SATISFIED;
  }
  if (!ready)
  {
    glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);
    s_sum[5] += CThread::GetTicks() - start;
    return;
  }
  glBindBuffer(GL_PIXEL_PACK_BUFFER, s_pixelBuffers[before]);
  glGetBufferSubData(GL_PIXEL_PACK_BUFFER, 0, WIDTH * HEIGHT * 4, s_rgba.data());
  glBindBuffer(GL_PIXEL_PACK_BUFFER, 0);
  UINT32 read = CThread::GetTicks();
  for (unsigned y = 0; y < HEIGHT; y++)
  {
    const uint8_t *src = &s_rgba[(HEIGHT - 1 - y) * WIDTH * 4];
    uint32_t *dst = &s_frame[y * WIDTH];
    if (s_rgbaFrames)
    {
      memcpy(dst, src, WIDTH * 4);
      continue;
    }
    for (unsigned x = 0; x < WIDTH; x++, src += 4)
      dst[x] = 0xFF000000u | ((uint32_t)src[0] << 16) | ((uint32_t)src[1] << 8) | src[2];
  }
  s_sum[5] += read - start;
  s_sum[6] += CThread::GetTicks() - read;
}

static bool CreateGL()
{
  if (&sm_gl_is_null) return true;
#ifdef __EMSCRIPTEN__
  EmscriptenWebGLContextAttributes attrs;
  emscripten_webgl_init_context_attributes(&attrs);
  attrs.majorVersion = 2;
  attrs.minorVersion = 0;
  attrs.alpha = false;
  attrs.depth = true;
  attrs.stencil = true;
  attrs.antialias = false;
  attrs.preserveDrawingBuffer = false;
  attrs.powerPreference = EM_WEBGL_POWER_PREFERENCE_HIGH_PERFORMANCE;
  // The frontend hands the module its canvas (Module.canvas, an OffscreenCanvas in the worker),
  // at any size: the drawing buffer is made at the Model 3's. Emscripten only looks up canvases
  // by CSS selector or through this table.
  EM_ASM({
    const canvas = Module["canvas"];
    if (canvas) { canvas.width = $0; canvas.height = $1; }
    Module["specialHTMLTargets"]["!canvas"] = canvas || 0;
  }, WIDTH, HEIGHT);
  EMSCRIPTEN_WEBGL_CONTEXT_HANDLE context = emscripten_webgl_create_context("!canvas", &attrs);
  if (context <= 0)
  {
    ErrorLog("No WebGL2 context on the module's canvas (%d): the game runs but draws nothing.", (int)context);
    return false;
  }
  emscripten_webgl_make_context_current(context);
  return true;
#else
  return false;
#endif
}

/******************************************************************************
 Save states, through Supermodel's block files on the data directory
******************************************************************************/

// Save/load through a FILE*, so an in-memory stream (rollback, handovers; no MEMFS) works like a
// real file. Rollback saves every frame, so this stays off disk (patch 0004).
static bool WriteStateTo(FILE *fp)
{
  CBlockFile file;
  if (Result::OKAY != file.CreateFromFile(fp, STATE_BLOCK, "Supermodel Version " SUPERMODEL_VERSION)) return false;
  int32_t version = STATE_FILE_VERSION;
  file.Write(&version, sizeof(version));
  file.Write(s_game.name);
  s_model3->SaveState(&file);
  file.Close();
  return true;
}

static bool ReadStateFrom(FILE *fp)
{
  CBlockFile file;
  if (Result::OKAY != file.LoadFromFile(fp)) return false;
  if (Result::OKAY != file.FindBlock(STATE_BLOCK)) { file.Close(); return false; }
  int32_t version = 0;
  file.Read(&version, sizeof(version));
  if (version != STATE_FILE_VERSION) { file.Close(); return false; }
  s_model3->LoadState(&file);
  file.Close();
  return true;
}

/******************************************************************************
 libretro API
******************************************************************************/

RETRO_API unsigned retro_api_version(void) { return RETRO_API_VERSION; }

RETRO_API void retro_set_environment(retro_environment_t cb)
{
  environ_cb = cb;
  // Our frontend (web/emulator/libretro.js) takes RGBA bytes as read back from WebGL, format 100;
  // any other gets XRGB8888.
  enum retro_pixel_format format = (enum retro_pixel_format)100;
  s_rgbaFrames = cb(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  if (!s_rgbaFrames)
  {
    format = RETRO_PIXEL_FORMAT_XRGB8888;
    cb(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  }
  struct retro_log_callback logging;
  if (cb(RETRO_ENVIRONMENT_GET_LOG_INTERFACE, &logging)) log_cb = logging.log;
}

RETRO_API void retro_set_video_refresh(retro_video_refresh_t cb) { video_cb = cb; }
RETRO_API void retro_set_audio_sample(retro_audio_sample_t cb) { (void)cb; }
RETRO_API void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) { audio_batch_cb = cb; }
RETRO_API void retro_set_input_poll(retro_input_poll_t cb) { input_poll_cb = cb; }
RETRO_API void retro_set_input_state(retro_input_state_t cb) { input_state_cb = cb; }

RETRO_API void retro_init(void)
{
  SetLogger(std::make_shared<RetroLogger>());
  supermodel_fixed_time = 891432000;
  s_rgba.assign(WIDTH * HEIGHT * 4, 0);
  s_frame.assign(WIDTH * HEIGHT, 0xFF000000u);
}

RETRO_API void retro_deinit(void)
{
  retro_unload_game();
}

RETRO_API void retro_get_system_info(struct retro_system_info *info)
{
  memset(info, 0, sizeof(*info));
  info->library_name = "Supermodel";
  info->library_version = SUPERMODEL_VERSION;
  info->valid_extensions = "zip";
  info->need_fullpath = true;
  info->block_extract = true;
}

RETRO_API void retro_get_system_av_info(struct retro_system_av_info *info)
{
  memset(info, 0, sizeof(*info));
  info->geometry.base_width = WIDTH;
  info->geometry.base_height = HEIGHT;
  info->geometry.max_width = WIDTH;
  info->geometry.max_height = HEIGHT;
  info->geometry.aspect_ratio = (float)WIDTH / (float)HEIGHT;
  // Supermodel makes 44100/60 samples a frame, so the picture runs at 60 Hz here rather than the
  // Model 3's 57.5 Hz to keep the sound in step.
  info->timing.fps = 60.0;
  info->timing.sample_rate = 44100.0;
}

RETRO_API void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port; (void)device; }

RETRO_API bool retro_load_game(const struct retro_game_info *info)
{
  if (!info || !info->path) return false;
  retro_unload_game();

  SetDefaultConfig(s_config);
  ApplyOverrides(s_config);
  s_glReady = CreateGL();

  GameLoader loader(s_gamesXml);
  ROMSet rom_set;
  if (loader.Load(&s_game, &rom_set, info->path)) // true means failure
  {
    ErrorLog("%s is not a Model 3 ROM set this build knows.", info->path);
    return false;
  }

  s_model3 = std::make_unique<CModel3>(s_config);
  if (Result::OKAY != s_model3->Init() || Result::OKAY != s_model3->LoadGame(s_game, rom_set))
  {
    s_model3.reset();
    return false;
  }
  rom_set = ROMSet();

  s_inputs = new CInputs(std::make_shared<CRetroInputSystem>(input_poll_cb, input_state_cb));
  if (!s_inputs->Initialize()) return false;
  s_inputs->LoadFromConfig(s_config);
  s_model3->AttachInputs(s_inputs);

  s_superAA = new SuperAA(1, CRTcolor::None);
  s_superAA->Init(WIDTH, HEIGHT);
  s_render2D = new CRender2D(s_config);
  s_render3D = new New3D::CNew3D(s_config, s_game.name);
  if (Result::OKAY != s_render2D->Init(0, 0, WIDTH, HEIGHT, WIDTH, HEIGHT, s_superAA->GetTargetID(), UpscaleMode::Nearest)) return false;
  if (Result::OKAY != s_render3D->Init(0, 0, WIDTH, HEIGHT, WIDTH, HEIGHT, s_superAA->GetTargetID())) return false;
  s_model3->AttachRenderers(s_render2D, s_render3D, s_superAA);
  s_model3->Reset();

  if (environ_cb)
  {
    std::vector<retro_input_descriptor> descriptors;
    for (const Mapping &m : kMappings)
      if (s_game.inputs & m.game_flags) descriptors.push_back({ m.port, RETRO_DEVICE_JOYPAD, 0, m.id, m.label });
    descriptors.push_back({ 0, 0, 0, 0, nullptr });
    environ_cb(RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS, descriptors.data());
  }

  // A save state's size is fixed by the game; measure it once (to a sizing memory stream).
  s_stateSize = 0;
  {
    char *buffer = nullptr;
    size_t length = 0;
    FILE *fp = open_memstream(&buffer, &length);
    if (fp && WriteStateTo(fp)) { fclose(fp); s_stateSize = (length + 0x100000) & ~(size_t)0xFFFF; }
    else if (fp) fclose(fp);
    free(buffer);
  }
  InfoLog("%s (%s, Step %s) loaded; save states take %zu bytes; drawing with %s.", s_game.title.c_str(), s_game.name.c_str(),
          s_game.stepping.c_str(), s_stateSize, &sm_gl_is_null ? "the no-op GL" : s_glReady ? "WebGL2" : "nothing (no context)");
  return true;
}

RETRO_API bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t num)
{
  (void)type; (void)info; (void)num;
  return false;
}

RETRO_API void retro_unload_game(void)
{
  s_frames = 0;
  s_model3.reset(); // before the renderers and inputs it points at
  delete s_render3D; s_render3D = nullptr;
  delete s_render2D; s_render2D = nullptr;
  delete s_superAA; s_superAA = nullptr;
  delete s_inputs; s_inputs = nullptr;
}

RETRO_API void retro_reset(void)
{
  if (s_model3) s_model3->Reset();
}

RETRO_API void retro_run(void)
{
  if (!s_model3) return;
  s_inputs->Poll(&s_game, 0, 0, WIDTH, HEIGHT);
  RetroAudio::Clear();
  s_model3->RunFrame(); // renders too (EndFrameVideo fills s_frame)
  FrameTimings t = s_model3->GetTimings();
  s_sum[0] += t.ppcTicks; s_sum[1] += t.renderTicks; s_sum[2] += t.sndTicks; s_sum[3] += t.drvTicks; s_sum[4] += t.frameTicks;
  s_sumFrames++;
  if (video_cb) video_cb(s_frame.data(), WIDTH, HEIGHT, WIDTH * sizeof(uint32_t));
  size_t frames = 0;
  const int16_t *samples = RetroAudio::Samples(&frames);
  if (audio_batch_cb && frames) audio_batch_cb(samples, frames);
}

RETRO_API size_t retro_serialize_size(void) { return s_stateSize; }

RETRO_API bool retro_serialize(void *data, size_t size)
{
  if (!s_model3 || size < s_stateSize) return false;
  FILE *fp = fmemopen(data, size, "wb"); // straight into the caller's buffer
  if (!fp) return false;
  bool ok = WriteStateTo(fp);
  long written = ftell(fp);
  if (fclose(fp) != 0 || written < 0) ok = false;
  if (!ok) { ErrorLog("The save state did not fit %zu bytes.", size); return false; }
  memset((uint8_t *)data + written, 0, size - (size_t)written);
  return true;
}

RETRO_API bool retro_unserialize(const void *data, size_t size)
{
  if (!s_model3) return false;
  FILE *fp = fmemopen(const_cast<void *>(data), size, "rb");
  if (!fp) return false;
  bool ok = ReadStateFrom(fp);
  fclose(fp);
  return ok;
}

RETRO_API void retro_cheat_reset(void) {}
RETRO_API void retro_cheat_set(unsigned index, bool enabled, const char *code) { (void)index; (void)enabled; (void)code; }
RETRO_API unsigned retro_get_region(void) { return RETRO_REGION_NTSC; }
// The 8 MB PowerPC RAM (patch 0003): the gameplay-relevant memory. Rollback desync checks hash
// this, not the whole save state, which also holds sound and render caches that drift harmlessly.
RETRO_API void *retro_get_memory_data(unsigned id)
{
  return (id == RETRO_MEMORY_SYSTEM_RAM && s_model3) ? s_model3->GetRAMPtr() : nullptr;
}
RETRO_API size_t retro_get_memory_size(unsigned id)
{
  return (id == RETRO_MEMORY_SYSTEM_RAM && s_model3) ? 0x800000 : 0;
}
