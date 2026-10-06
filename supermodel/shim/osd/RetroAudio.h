// The frame's mixed audio, kept by osd/Audio.cpp for libretro.cpp to hand to the frontend.
#pragma once
#include <cstddef>
#include <cstdint>

namespace RetroAudio
{
  /** Interleaved stereo 16-bit samples mixed since the last Clear(). */
  const int16_t *Samples(size_t *frames);
  void Clear();
}
