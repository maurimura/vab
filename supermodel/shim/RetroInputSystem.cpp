#include "RetroInputSystem.h"
#include "Inputs/Input.h" // ForceFeedbackCmd
#include <cstring>

// RetroPad button id -> one of Supermodel's valid key names (Inputs/InputSystem.cpp), in id order:
// B Y SELECT START UP DOWN LEFT RIGHT A X L R L2 R2 L3 R3.
static const char *const kKeyNames[16] = {
  "Z", "X", "N", "M", "UP", "DOWN", "LEFT", "RIGHT", "C", "V", "A", "S", "D", "F", "G", "H",
};

const char *CRetroInputSystem::KeyName(unsigned id)
{
  return id < 16 ? kKeyNames[id] : nullptr;
}

CRetroInputSystem::CRetroInputSystem(retro_input_poll_t poll, retro_input_state_t state)
  : CInputSystem("RetroPad"), m_poll(poll), m_state(state)
{
  for (int port = 0; port < PORTS; port++)
    snprintf(m_keyDetails[port].name, sizeof(m_keyDetails[port].name), "RetroPad %d", port + 1);
}

CRetroInputSystem::~CRetroInputSystem()
{
}

bool CRetroInputSystem::InitializeSystem()
{
  return true;
}

int CRetroInputSystem::GetKeyIndex(const char *keyName)
{
  for (int id = 0; id < 16; id++)
    if (strcasecmp(keyName, kKeyNames[id]) == 0) return id;
  return -1;
}

const char *CRetroInputSystem::GetKeyName(int keyIndex)
{
  return KeyName((unsigned)keyIndex);
}

bool CRetroInputSystem::IsKeyPressed(int kbdNum, int keyIndex) const
{
  if (!m_state || kbdNum < 0 || kbdNum >= PORTS || keyIndex < 0 || keyIndex >= 16) return false;
  return m_state((unsigned)kbdNum, RETRO_DEVICE_JOYPAD, 0, (unsigned)keyIndex) != 0;
}

int CRetroInputSystem::GetMouseAxisValue(int, int) const { return 0; }
int CRetroInputSystem::GetMouseWheelDir(int) const { return 0; }
bool CRetroInputSystem::IsMouseButPressed(int, int) const { return false; }
int CRetroInputSystem::GetJoyAxisValue(int, int) const { return 0; }
bool CRetroInputSystem::IsJoyPOVInDir(int, int, int) const { return false; }
bool CRetroInputSystem::IsJoyButPressed(int, int) const { return false; }
bool CRetroInputSystem::ProcessForceFeedbackCmd(int, int, ForceFeedbackCmd) { return false; }

int CRetroInputSystem::GetNumKeyboards() const { return PORTS; }
int CRetroInputSystem::GetNumMice() const { return 0; }
int CRetroInputSystem::GetNumJoysticks() const { return 0; }

const KeyDetails *CRetroInputSystem::GetKeyDetails(int kbdNum)
{
  return kbdNum >= 0 && kbdNum < PORTS ? &m_keyDetails[kbdNum] : nullptr;
}

const MouseDetails *CRetroInputSystem::GetMouseDetails(int) { return nullptr; }
const JoyDetails *CRetroInputSystem::GetJoyDetails(int) { return nullptr; }

bool CRetroInputSystem::Poll()
{
  if (m_poll) m_poll();
  return true;
}

void CRetroInputSystem::SetMouseVisibility(bool)
{
}
