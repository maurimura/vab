// Supermodel's threading OSD for a single-threaded build: the emulator runs with
// MultiThreaded=false, so no thread is ever started; locks and semaphores never block.
#include "OSD/Thread.h"
#include <chrono>

void CThread::Sleep(UINT32 ms)
{
  (void)ms;
}

// Microseconds, not Supermodel's milliseconds: the emulator only uses ticks for its per-frame
// timings (CModel3::GetTimings), which libretro.cpp reports, and a frame is a few milliseconds.
UINT32 CThread::GetTicks()
{
  using namespace std::chrono;
  return (UINT32)duration_cast<microseconds>(steady_clock::now().time_since_epoch()).count();
}

CThread *CThread::CreateThread(const std::string &name, ThreadStart start, void *startParam)
{
  (void)name; (void)start; (void)startParam;
  return nullptr;
}

CSemaphore *CThread::CreateSemaphore(UINT32 initVal)
{
  return new CSemaphore(new UINT32(initVal));
}

CCondVar *CThread::CreateCondVar()
{
  return new CCondVar(nullptr);
}

CMutex *CThread::CreateMutex()
{
  return new CMutex(nullptr);
}

const char *CThread::GetLastError()
{
  return "this build runs single-threaded";
}

CThread::CThread(const std::string &name, void *impl) : m_name(name), m_impl(impl) {}
CThread::~CThread() {}
const std::string &CThread::GetName() const { return m_name; }
UINT32 CThread::GetId() { return 0; }
int CThread::Wait() { return 0; }

CSemaphore::CSemaphore(void *impl) : m_impl(impl) {}
CSemaphore::~CSemaphore() { delete (UINT32 *)m_impl; }
UINT32 CSemaphore::GetValue() { return *(UINT32 *)m_impl; }
bool CSemaphore::Wait() { return true; }
bool CSemaphore::Post() { return true; }

CCondVar::CCondVar(void *impl) : m_impl(impl) {}
CCondVar::~CCondVar() {}
bool CCondVar::Wait(CMutex *mutex) { (void)mutex; return true; }
bool CCondVar::Signal() { return true; }
bool CCondVar::SignalAll() { return true; }

CMutex::CMutex(void *impl) : m_impl(impl) {}
CMutex::~CMutex() {}
bool CMutex::Lock() { return true; }
bool CMutex::Unlock() { return true; }
