// Supermodel input system fed by the libretro input callbacks. Each RetroPad port is a "keyboard"
// whose keys are the pad's buttons, so Supermodel's own mapping strings address them:
// KEY<port+1>_<KeyName(button)>, e.g. KEY1_UP for player 1's stick up (see libretro.cpp).
#pragma once
#include "Inputs/InputSystem.h"
#include "libretro.h"

class CRetroInputSystem : public CInputSystem
{
public:
  static constexpr int PORTS = 4;
  /** Supermodel's name for a RetroPad button (RETRO_DEVICE_ID_JOYPAD_*), or null. */
  static const char *KeyName(unsigned id);

  CRetroInputSystem(retro_input_poll_t poll, retro_input_state_t state);
  ~CRetroInputSystem();

  int GetNumKeyboards() const override;
  int GetNumMice() const override;
  int GetNumJoysticks() const override;
  const KeyDetails *GetKeyDetails(int kbdNum) override;
  const MouseDetails *GetMouseDetails(int mseNum) override;
  const JoyDetails *GetJoyDetails(int joyNum) override;
  bool Poll() override;
  void SetMouseVisibility(bool visible) override;

protected:
  bool InitializeSystem() override;
  int GetKeyIndex(const char *keyName) override;
  const char *GetKeyName(int keyIndex) override;
  bool IsKeyPressed(int kbdNum, int keyIndex) const override;
  int GetMouseAxisValue(int mseNum, int axisNum) const override;
  int GetMouseWheelDir(int mseNum) const override;
  bool IsMouseButPressed(int mseNum, int butNum) const override;
  int GetJoyAxisValue(int joyNum, int axisNum) const override;
  bool IsJoyPOVInDir(int joyNum, int povNum, int povDir) const override;
  bool IsJoyButPressed(int joyNum, int butNum) const override;
  bool ProcessForceFeedbackCmd(int joyNum, int axisNum, ForceFeedbackCmd ffCmd) override;

private:
  retro_input_poll_t m_poll;
  retro_input_state_t m_state;
  KeyDetails m_keyDetails[PORTS];
};
