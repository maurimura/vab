// Supermodel's audio OSD. The sound board hands over a frame of four float channels (front and
// rear stereo); they are mixed to 16-bit stereo the way Supermodel's SDL build does for a
// two-channel host, and kept for the libretro audio callback.
#include "OSD/Audio.h"
#include "osd/RetroAudio.h"
#include <algorithm>
#include <vector>

static std::vector<int16_t> s_samples;
static Game::AudioTypes s_type = Game::STEREO_LR;

void SetAudioCallback(AudioCallbackFPtr callback, void *data)
{
  (void)callback; (void)data; // only used by Supermodel's sound thread, which this build has not
}

void SetAudioEnabled(bool enabled)
{
  (void)enabled;
}

void SetAudioType(Game::AudioTypes type)
{
  s_type = type;
}

Result OpenAudio(const Util::Config::Node &config)
{
  (void)config;
  return Result::OKAY;
}

void CloseAudio()
{
}

static int16_t Mix(float a, float b)
{
  return (int16_t)std::clamp(a + b, -32768.0f, 32767.0f);
}

bool OutputAudio(unsigned numSamples, const float *leftFront, const float *rightFront, const float *leftRear, const float *rightRear, bool flipStereo)
{
  if (s_type == Game::STEREO_RL || s_type == Game::QUAD_1_FRL_2_RRL || s_type == Game::QUAD_1_RRL_2_FRL)
    flipStereo = !flipStereo;
  s_samples.reserve(s_samples.size() + numSamples * 2);
  for (unsigned i = 0; i < numSamples; i++)
  {
    int16_t left = Mix(leftFront[i], leftRear[i]);
    int16_t right = Mix(rightFront[i], rightRear[i]);
    if (flipStereo) std::swap(left, right);
    s_samples.push_back(left);
    s_samples.push_back(right);
  }
  return false; // the buffer is never "full": the frontend paces the frames
}

namespace RetroAudio
{
  const int16_t *Samples(size_t *frames)
  {
    *frames = s_samples.size() / 2;
    return s_samples.data();
  }

  void Clear()
  {
    s_samples.clear();
  }
}
