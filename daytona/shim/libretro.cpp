// Daytona USA (Sega Model 2, MAME set `daytona`, Revision A) behind the libretro API, so
// web/emulator/libretro.js and worker.js drive it like the FBNeo and Supermodel cores: load the
// ROM set, run a frame, read the picture and the sound, save and load the machine.
//
// The game is daytona-arcade-recomp: its i960, TGP and sound 68000 programs recompiled to C++
// (the generated code build.sh makes from the ROM set) on a native Model 2 board. No emulated
// CPU and no GPU: a frame is the game's own code plus the board's software rasterizer, and the
// picture is a CPU framebuffer (0xAARRGGBB rows), handed to the frontend as XRGB8888 unconverted.
//
// One libretro machine is N linked cabinets (N = 1 to 8, daytona_set("cabinets", ...) before
// loading): cabinet k is driven by RetroPad port k, and the cabinets' communication boards are
// wired to each other in memory in a ring (two: cabinet 0 -> 1 -> 0, the recomp's
// tests/test_comm_board.cpp), so a race between two players is one deterministic machine that
// rollback / lockstep netplay can run on every player's computer. daytona_set("view", k) picks
// the cabinet shown and heard; the others are not rasterized (patches/0002), which changes
// nothing the game can see. The link_* options and the daytona_cabinet_* exports are for
// experiments on the ring (harness/ring-lab.mjs, ring-notes.md).
//
// The arcade mode: one cabinet alone (cabinets=1, link_topology=star, seat=k) is one seat of a
// star of up to 8 cabinets that run in other machines; the frontend carries each cabinet's own
// 448-byte link block between them (daytona_link_out / daytona_link_in / daytona_link_absent,
// class Bridge below).
#include "libretro.h"
#include "snapshot_api.h"
#include "daytona_version.h"
#include "common/input_script.h" // the recomp's tools/common: m2run's scripted inputs

#include "runtime/comm_board.h"
#include "runtime/game_loop.h"
#include "runtime/rom_import.h"
#include "runtime/sound_board.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <exception>
#include <fstream>
#include <functional>
#include <initializer_list>
#include <iterator>
#include <memory>
#include <numeric>
#include <string>
#include <sys/stat.h>
#include <utility>
#include <vector>
#ifdef __EMSCRIPTEN__
#include <emscripten.h>
#endif

// 1 when the game code is shim/gen_stub.cpp, whose strong definition replaces this weak one.
extern "C" __attribute__((weak)) int daytona_gen_is_stub = 0;

namespace {

constexpr unsigned kMaxCabinets = 8;                // the game's link: up to 8 cabinets (CAR NUMBER 1-8)
constexpr unsigned kWidth = rt::GameLoop::kWidth;   // 496: no widescreen here
constexpr unsigned kHeight = rt::GameLoop::kHeight; // 384
constexpr double kSampleRate = 48000.0;
constexpr int kLock = 0x60;                         // steering ADC from centre to full lock
constexpr uint32_t kStateMagic = 0x54594144;        // "DAYT"
constexpr uint32_t kStateVersion = 2;           // 2: the seat's turning count (the two-stage ramp)
constexpr size_t kLinkFrame = 0xe01;                // a comm board frame on the wire (0xe00 + id)
constexpr size_t kWireCap = 32 * kLinkFrame;        // bytes a cable holds unread before the link is lost
constexpr size_t kSlot = 0x1c0;                     // a cabinet's own block at the head of its data frames (the frame offset)
constexpr size_t kAudioCarryMax = 64;               // input frames a stream may carry between frames

retro_environment_t environ_cb;
retro_video_refresh_t video_cb;
retro_audio_sample_batch_t audio_batch_cb;
retro_input_poll_t input_poll_cb;
retro_input_state_t input_state_cb;
retro_log_printf_t log_cb;

void Log(retro_log_level level, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
void Log(retro_log_level level, const char *fmt, ...)
{
  char text[1024];
  va_list vl;
  va_start(vl, fmt);
  vsnprintf(text, sizeof(text), fmt, vl);
  va_end(vl);
  if (log_cb) log_cb(level, "%s\n", text);
  else fprintf(stderr, "daytona: %s\n", text);
}

// For complaints about how the frontend calls an export: the first 5, then every 1000th.
bool Often(unsigned &count) { return ++count <= 5 || count % 1000 == 0; }

uint64_t NowUs()
{
#ifdef __EMSCRIPTEN__
  return uint64_t(emscripten_get_now() * 1000.0);
#else
  using namespace std::chrono;
  return uint64_t(duration_cast<microseconds>(steady_clock::now().time_since_epoch()).count());
#endif
}

bool GenIsStub() { return daytona_gen_is_stub != 0; }

/******************************************************************************
 Link play: the cables between the cabinets' communication boards, in memory
******************************************************************************/

// The ring. The master (cabinet 0) sends to the last cabinet, which sends to the one before it,
// ..., and cabinet 1 sends to the master: the master's numbering token gives the first cabinet
// after it the highest id (comm_board.cpp: a slave takes the number and passes it on less one),
// so this way round cabinet k gets link id k + 1. Two cabinets: 0 -> 1 -> 0 either way.
unsigned s_count;                 // cabinets in this machine
unsigned Next(unsigned k) { return (k + s_count - 1) % s_count; } // the cabinet k sends to
unsigned Prev(unsigned k) { return (k + 1) % s_count; }           // the cabinet k reads from
// The order the cabinets run in a retro_run: along the ring from the master (0, N-1, ..., 1), so
// with no delay the master's frame goes all round within one retro_run (two cabinets: 0, 1).
unsigned RunOrder(unsigned i) { return i == 0 ? 0 : s_count - i; }

// Experiments on the ring (link_cut, link_pause, link_ghost; not machine state): a cut cabinet
// is powered off (not run, its cables closed: its neighbours' boards lose the link); a paused
// one is not run but its cables stay open (nothing reads or writes them); a ghost's game is not
// run but its comm board is, once a frame, so it keeps its place in the ring and passes on what
// it receives with its own block as the game last wrote it.
enum class Mode { Run, Cut, Pause, Ghost };
Mode s_mode[kMaxCabinets];
unsigned s_linkDelay = 0;  // link_delay: retro_runs a cable holds bytes before the next cabinet can read them
unsigned s_linkJitter = 0; // link_jitter: up to this many more, at random (in order: no overtaking)
uint64_t s_frame = 0;      // retro_runs since the game was loaded: the cables' clock (not machine state)

// One direction of cable (tests/test_comm_board.cpp's Wire), as the writes made to it: each is
// ready for the next cabinet to read link_delay (+ up to link_jitter) retro_runs after it was
// written. Machine state: the bytes are saved with it (not when each is ready: a state's bytes
// are all ready at once).
struct Wire
{
  struct Chunk
  {
    uint64_t ready;
    std::vector<uint8_t> bytes;
  };
  std::deque<Chunk> chunks;
  size_t head = 0;   // bytes of chunks.front() already read
  uint64_t last = 0; // when the last chunk is ready
  uint64_t rng = 0;  // xorshift64 for the jitter

  size_t Size() const
  {
    size_t n = 0;
    for (const Chunk &c : chunks) n += c.bytes.size();
    return n - head;
  }
  // Bytes the next cabinet could read now but has not: what a stalled reader leaves piling up.
  size_t ReadyBytes() const
  {
    size_t n = 0;
    for (const Chunk &c : chunks)
      if (c.ready <= s_frame) n += c.bytes.size();
    return chunks.empty() || chunks.front().ready > s_frame ? n : n - head;
  }
  std::vector<uint8_t> Bytes() const
  {
    std::vector<uint8_t> out;
    for (const Chunk &c : chunks) out.insert(out.end(), c.bytes.begin(), c.bytes.end());
    out.erase(out.begin(), out.begin() + ptrdiff_t(head));
    return out;
  }
  void Assign(const uint8_t *b, size_t n)
  {
    Clear();
    if (n) chunks.push_back({ 0, std::vector<uint8_t>(b, b + n) });
  }
  void Clear()
  {
    chunks.clear();
    head = 0;
    last = 0;
  }
  uint64_t Random()
  {
    rng ^= rng << 13;
    rng ^= rng >> 7;
    rng ^= rng << 17;
    return rng;
  }
};
Wire s_wires[kMaxCabinets];       // s_wires[k]: from cabinet k to Next(k)

// What went through a cabinet's cables (link_stats, link_frame): for ring-notes.md's protocol page.
struct LinkStats
{
  uint64_t txBytes = 0, txData = 0, txVsync = 0, txToken = 0, rxBytes = 0, rxData = 0;
  // Data frames sent whose bytes after the cabinet's own block are the first ones of the data
  // frame it last received (the shift register: comm_board.cpp stores a received frame at the
  // frame offset of the block it sends), and those whose are not.
  uint64_t shiftSame = 0, shiftDiff = 0;
  // Data frames sent whose own block differs from the last one sent, and in how many bytes.
  uint64_t slotChanged = 0, slotBytesChanged = 0;
  uint64_t assists = 0; // 0xfe frames link_assist handed over
  uint64_t dropped = 0; // frames link_full=drop threw away from the outgoing cable
  uint8_t lastRx[kLinkFrame] = {}, lastTx[kLinkFrame] = {};
  bool haveRx = false, haveTx = false;
};

bool IsData(uint8_t type) { return type >= 1 && type <= kMaxCabinets; }

int BoardLink(unsigned k);  // cabinet k's comm board's Link as an int, -1 without one (below)
int BoardCount(unsigned k); // the count cabinet k's board was numbered with, 0 without one (below)
unsigned s_linkAssist = 0;  // link_assist (below)
bool s_linkDrop = false;    // link_full=drop: a full cable drops its oldest frames instead of refusing the write
int s_linkPace = -1;        // link_pace: 1 a board is handed at most one data frame a frame, 0 not; -1 unset
bool s_star = false, s_starWanted = false; // link_topology=star, read when the game loads
bool s_blank[kMaxCabinets];                // link_blank: (star) those cabinets' blocks read as zeros
// link_pace unset: on in a star (one frame a frame is how the star works), off in the ring (as measured).
bool Paced() { return s_linkPace < 0 ? s_star : s_linkPace != 0; }

// A cabinet's end of the link, whatever carries it: what its board reads and writes, counted, and
// frames made here for it (link_assist, the star) handed over before anything else.
class Cable : public rt::LinkTransport
{
public:
  explicit Cable(unsigned k) : k_(k) {}
  void poll() override {}
  LinkStats stats;

protected:
  unsigned k_;
  std::vector<uint8_t> pending_; // a frame made here, being read
  size_t at_ = 0;
  uint64_t assisted_ = ~uint64_t(0);

  // link_assist=1: a slave whose board waits for its number while the master's link is up (it
  // was reset, or came into a ring that is already numbered) is handed the master's 0xfe frame
  // with its own id and the count, as the master's numbering would have given it, once a frame.
  void Assist()
  {
    if (!s_linkAssist || k_ == 0 || !pending_.empty() || assisted_ == s_frame) return;
    if (BoardLink(k_) != int(rt::CommBoard::Link::Waiting) || BoardLink(0) != int(rt::CommBoard::Link::Up)) return;
    Make(0xfe, uint8_t(k_ + 1), uint8_t(s_count));
    assisted_ = s_frame;
    stats.assists++;
  }
  void Make(uint8_t type, uint8_t a, uint8_t b)
  {
    pending_.assign(kLinkFrame, 0);
    pending_[0] = type;
    pending_[1] = a;
    pending_[2] = b;
    at_ = 0;
  }
  int ReadPending(uint8_t *buf, int max)
  {
    const size_t n = std::min(size_t(max), pending_.size() - at_);
    std::copy_n(pending_.begin() + ptrdiff_t(at_), n, buf);
    at_ += n;
    if (at_ == pending_.size())
    {
      Received(pending_.data(), pending_.size());
      pending_.clear();
      at_ = 0;
    }
    stats.rxBytes += n;
    return int(n);
  }
  void Received(const uint8_t *frame, size_t size)
  {
    if (size != kLinkFrame || !IsData(frame[0])) return;
    std::copy(frame, frame + size, stats.lastRx);
    stats.haveRx = true;
    stats.rxData++;
  }
  void Count(const uint8_t *buf, int n)
  {
    stats.txBytes += uint64_t(n);
    if (n < 1) return;
    if (buf[0] == 0xfc) stats.txVsync++;
    else if (!IsData(buf[0]) || size_t(n) != kLinkFrame) stats.txToken++;
    else
    {
      stats.txData++;
      if (stats.haveRx)
        (std::equal(buf + 1 + kSlot, buf + kLinkFrame, stats.lastRx + 1) ? stats.shiftSame : stats.shiftDiff)++;
      if (stats.haveTx)
      {
        const auto changed = std::inner_product(buf + 1, buf + 1 + kSlot, stats.lastTx + 1, uint64_t(0), std::plus<>(),
                                                [](uint8_t a, uint8_t b) { return uint64_t(a != b); });
        if (changed) stats.slotChanged++;
        stats.slotBytesChanged += changed;
      }
      std::copy(buf, buf + kLinkFrame, stats.lastTx);
      stats.haveTx = true;
    }
  }
};

// The ring (the default): cabinet k reads from the cable of the cabinet before it and writes to
// its own (to the next one). Never blocks; closes only when a cabinet at either end is cut
// (link_cut). A cable holding 32 frames ready and unread (the next cabinet stopped reading)
// refuses the write and the board loses the link, as a TCP link's send limit would.
class Loop : public Cable
{
  uint64_t paced_ = ~uint64_t(0); // when a data frame was last handed over (link_pace)

public:
  using Cable::Cable;
  bool rx_open() const override { return s_mode[k_] != Mode::Cut && s_mode[Prev(k_)] != Mode::Cut; }
  bool tx_open() const override { return s_mode[k_] != Mode::Cut && s_mode[Next(k_)] != Mode::Cut; }
  int read(uint8_t *buf, int max) override
  {
    Assist();
    if (!pending_.empty()) return ReadPending(buf, max);
    Wire &in = s_wires[Prev(k_)];
    int n = 0;
    while (n < max && !in.chunks.empty() && in.chunks.front().ready <= s_frame)
    {
      Wire::Chunk &c = in.chunks.front();
      // link_pace=1: one data frame a frame, however they came; the rest wait for the next ones.
      if (Paced() && in.head == 0 && c.bytes.size() == kLinkFrame && IsData(c.bytes[0]))
      {
        if (paced_ == s_frame) break;
        paced_ = s_frame;
      }
      const size_t take = std::min(size_t(max - n), c.bytes.size() - in.head);
      std::copy_n(c.bytes.begin() + ptrdiff_t(in.head), take, buf + n);
      n += int(take);
      in.head += take;
      if (in.head == c.bytes.size())
      {
        Received(c.bytes.data(), c.bytes.size());
        in.chunks.pop_front();
        in.head = 0;
      }
    }
    stats.rxBytes += uint64_t(n);
    return n;
  }
  bool write(const uint8_t *buf, int n) override
  {
    Wire &out = s_wires[k_];
    if (out.ReadyBytes() + size_t(n) > kWireCap)
    {
      if (!s_linkDrop) return false;
      // link_full=drop: the oldest frames the next cabinet has not read make room instead.
      while (!out.chunks.empty() && out.chunks.front().ready <= s_frame && out.ReadyBytes() + size_t(n) > kWireCap)
      {
        out.chunks.pop_front();
        out.head = 0;
        stats.dropped++;
      }
    }
    uint64_t ready = s_frame + s_linkDelay;
    if (s_linkJitter) ready += out.Random() % (s_linkJitter + 1);
    ready = std::max(ready, out.last);
    out.last = ready;
    out.chunks.push_back({ ready, std::vector<uint8_t>(buf, buf + n) });
    Count(buf, n);
    return true;
  }
};

// link_topology=star (an experiment): no ring. A cabinet's data frames are taken apart and only
// its own block (their first 0x1c0 bytes) goes out, to every cabinet; once a frame each cabinet
// is handed the data frame it would have received from the cabinet before it in the ring, made of
// every cabinet's latest block that has arrived (link_delay after it was sent: one hop from
// everyone). The numbering is answered here: the master's 0xff comes back with the count, its
// 0xfe reaches every slave with that slave's own id. Vsync frames and slaves' forwards go nowhere.
struct Published
{
  uint64_t ready;
  std::vector<uint8_t> block;
};
std::deque<Published> s_published[kMaxCabinets];                            // per cabinet, oldest first
std::deque<std::pair<uint64_t, std::vector<uint8_t>>> s_control[kMaxCabinets]; // numbering frames for k

class Star : public Cable
{
public:
  using Cable::Cable;
  bool rx_open() const override { return s_mode[k_] != Mode::Cut; }
  bool tx_open() const override { return s_mode[k_] != Mode::Cut; }
  int read(uint8_t *buf, int max) override
  {
    Assist();
    auto &control = s_control[k_];
    if (pending_.empty() && !control.empty() && control.front().first <= s_frame)
    {
      pending_ = std::move(control.front().second);
      control.pop_front();
      at_ = 0;
    }
    const int up = int(rt::CommBoard::Link::Up);
    if (pending_.empty() && delivered_ != s_frame && BoardLink(k_) == up)
    {
      // The frame from the cabinet before it (link id own + 1): block p is link id own + 1 + p.
      Make(uint8_t((k_ + 1) % s_count + 1), 0, 0);
      for (unsigned p = 0; p < kLinkFrame / kSlot; p++)
        if (const unsigned c = (k_ + 1 + p) % s_count; !s_blank[c])
          if (const std::vector<uint8_t> *b = Latest(c))
          std::copy(b->begin(), b->end(), pending_.begin() + ptrdiff_t(1 + p * kSlot));
      delivered_ = s_frame;
    }
    return pending_.empty() ? 0 : ReadPending(buf, max);
  }
  bool write(const uint8_t *buf, int n) override
  {
    Count(buf, n);
    if (size_t(n) != kLinkFrame) return true;
    uint64_t ready = s_frame + s_linkDelay;
    if (s_linkJitter) ready += s_wires[k_].Random() % (s_linkJitter + 1);
    if (IsData(buf[0]))
    {
      auto &mine = s_published[k_];
      mine.push_back({ ready, std::vector<uint8_t>(buf + 1, buf + 1 + kSlot) });
      while (mine.size() > 1 && mine[1].ready <= s_frame) mine.pop_front();
    }
    else if (k_ == 0 && buf[0] == 0xff && s_control[0].empty())
      s_control[0].push_back({ ready, Frame(0xff, uint8_t(s_count), 0) });
    else if (k_ == 0 && buf[0] == 0xfe)
      for (unsigned c = 1; c < s_count; c++) s_control[c].push_back({ ready, Frame(0xfe, uint8_t(c + 1), buf[2]) });
    return true;
  }

private:
  uint64_t delivered_ = ~uint64_t(0);
  static std::vector<uint8_t> Frame(uint8_t type, uint8_t a, uint8_t b)
  {
    std::vector<uint8_t> f(kLinkFrame, 0);
    f[0] = type;
    f[1] = a;
    f[2] = b;
    return f;
  }
  static const std::vector<uint8_t> *Latest(unsigned c)
  {
    const std::vector<uint8_t> *latest = nullptr;
    for (const Published &p : s_published[c])
      if (p.ready <= s_frame) latest = &p.block;
    return latest;
  }
};

// The bridge (the arcade mode): cabinets=1 with link_topology=star. This machine's one cabinet is
// seat `seat` of a star of up to 8 whose other cabinets run in other machines (every player's
// browser runs their own). The frontend carries the blocks: after each retro_run it takes this
// cabinet's own block (daytona_link_out) and sends it to the others; it hands over each other
// seat's latest block as it arrives (daytona_link_in) and says when a seat is left
// (daytona_link_absent). Once a local frame (link_pace, on by default in a star) the board is
// handed the data frame it would have received from the cabinet before it in the ring, assembled
// from the latest blocks of all seats (its own, as it last sent it, in its own slot); a numbering
// the board asks for is answered here (a master's 0xff comes back with the count, a slave waiting
// for its number is handed its 0xfe, once a frame); vsync frames and forwards go nowhere; a write
// is never refused. So the frame the board sees depends only on the blocks handed over before
// the retro_run and on the cabinet itself: a spectator replaying the same pad input and the same
// hand-overs from the same state sees the same game. The table of blocks is machine state (in
// whole-machine states), so a state carries what its cabinet was seeing.
struct BridgeSeat
{
  uint8_t block[kSlot] = {};
  bool seen = false;   // a block was handed over (since power-on, or as the loaded state says)
  bool absent = false; // daytona_link_absent since: zeros, or frozen as last seen (link_absent)
  uint64_t at = 0;     // s_frame when it was handed over or left (the status's age; not machine state)
};
struct BridgeState
{
  BridgeSeat seats[kMaxCabinets];
  uint8_t own[kSlot] = {}; // this cabinet's block as its board last sent it
  bool answer = false;     // the board sent the numbering token (0xff): it comes back at its next read
  bool fresh = false;      // a block was handed over since the board was last handed a frame (link_pace=0)
};
BridgeState s_bridge;
unsigned s_seat = 0;         // seat: this cabinet's seat, 0-7 (link id seat + 1)
bool s_absentZero = true;    // link_absent: a seat that leaves reads as zeros (zero, the default) or keeps its block (freeze)
uint8_t s_outLast[kSlot];    // what daytona_link_out returned last
bool s_outValid = false;     // false: its next call says "changed" whatever it returns
bool Bridging() { return s_star && s_count == 1; }
// Seats in the star: the count the board was numbered with (the presets' 8), else 8.
unsigned BridgeCount()
{
  const int count = BoardCount(0);
  return count >= 1 && count <= int(kMaxCabinets) ? unsigned(count) : kMaxCabinets;
}

class Bridge : public Cable
{
public:
  using Cable::Cable;
  bool rx_open() const override { return true; }
  bool tx_open() const override { return true; }
  int read(uint8_t *buf, int max) override
  {
    if (pending_.empty())
    {
      const int link = BoardLink(k_);
      const unsigned count = BridgeCount();
      if (s_bridge.answer)
      {
        // The master's numbering token, back round the star: [1] = the count.
        Make(0xff, uint8_t(count), 0);
        s_bridge.answer = false;
        stats.assists++;
      }
      else if (link == int(rt::CommBoard::Link::Waiting) && s_seat != 0 && numbered_ != s_frame)
      {
        // A slave waiting for its number: the master's 0xfe as it would reach this seat.
        Make(0xfe, uint8_t(s_seat + 1), uint8_t(count));
        numbered_ = s_frame;
        stats.assists++;
      }
      else if (link == int(rt::CommBoard::Link::Up) && delivered_ != s_frame && (Paced() || s_bridge.fresh))
      {
        // The frame from the seat before it in the ring (link id own + 1): block p is seat own + 1 + p.
        Make(uint8_t((s_seat + 1) % count + 1), 0, 0);
        for (unsigned p = 0; p < kLinkFrame / kSlot; p++)
        {
          const unsigned c = (s_seat + 1 + p) % count;
          const uint8_t *b = c == s_seat ? s_bridge.own : s_bridge.seats[c].block;
          std::copy(b, b + kSlot, pending_.begin() + ptrdiff_t(1 + p * kSlot));
        }
        delivered_ = s_frame;
        s_bridge.fresh = false;
      }
    }
    return pending_.empty() ? 0 : ReadPending(buf, max);
  }
  bool write(const uint8_t *buf, int n) override
  {
    Count(buf, n);
    if (size_t(n) != kLinkFrame) return true;
    if (IsData(buf[0])) std::copy(buf + 1, buf + 1 + kSlot, s_bridge.own);
    else if (buf[0] == 0xff) s_bridge.answer = true;
    return true; // vsync frames (0xfc) and a slave's forwarded 0xfe go nowhere
  }

private:
  uint64_t delivered_ = ~uint64_t(0), numbered_ = ~uint64_t(0);
};

/******************************************************************************
 Sound: the board's FM (55.6 kHz) and PCM (44.6 kHz) outputs to 48 kHz
******************************************************************************/

// One of a sound board's outputs on its way to 48 kHz: linear interpolation at exact rational
// positions (output sample j reads input position j * a / b), so no sample is dropped or doubled
// across frames and the result is the same on every machine. m2run's mix_into does the same
// arithmetic in doubles over a whole run.
struct Stream
{
  uint64_t a = 1, b = 1;  // input samples per output sample: a / b
  uint64_t base = 0;      // absolute index of in[0]
  std::vector<float> in;  // interleaved stereo input not yet used up

  void Rate(uint64_t clock, uint64_t divider) // the chip's clock and its divider per sample
  {
    const uint64_t num = clock, den = divider * uint64_t(kSampleRate);
    const uint64_t g = std::gcd(num, den);
    a = num / g;
    b = den / g;
  }
  uint64_t Total() const { return base + in.size() / 2; }
  // How many output samples the input so far covers (each needs the input sample after its
  // position too).
  uint64_t Available() const
  {
    const uint64_t total = Total();
    return total < 2 ? 0 : ((total - 1) * b + a - 1) / a;
  }
  void At(uint64_t j, float &l, float &r) const
  {
    const uint64_t pos = j * a, k = pos / b;
    const float f = float(pos % b) / float(b);
    const size_t i = size_t(k - base) * 2;
    l = in[i] * (1.0f - f) + in[i + 2] * f;
    r = in[i + 1] * (1.0f - f) + in[i + 3] * f;
  }
  void DropBefore(uint64_t j)
  {
    const uint64_t k = j * a / b;
    if (k <= base) return;
    in.erase(in.begin(), in.begin() + ptrdiff_t((k - base) * 2));
    base = k;
  }
};

/******************************************************************************
 Machine
******************************************************************************/

// What the shim keeps per cabinet besides the game: the pad as the cabinet's controls. Machine
// state (saved with it): the wheel's position and the gear lever follow the pad over frames.
struct Seat
{
  int32_t steer = 0;    // from centre: -kLock full left .. kLock full right
  int32_t gear = 1;     // 1-4, shifted sequentially
  uint32_t held = 0;    // last frame's RetroPad mask (shifts happen on presses)
  int32_t turning = 0;  // frames LEFT (< 0) or RIGHT (> 0) has been held, for the ramp's slow stage
};

struct Cabinet
{
  std::unique_ptr<Cable> link;         // before game: the board holds a pointer to it
  std::unique_ptr<rt::GameLoop> game;
  Seat seat;
  Stream fm, pcm;
  uint64_t samples = 0;                // 48 kHz frames made so far
  uint64_t dropped = 0;                // the geometrizer's dropped frames, as last logged (patches/0004)
};

struct Timings
{
  uint64_t frames = 0, logic[kMaxCabinets] = {}, geometry[kMaxCabinets] = {}, raster[kMaxCabinets] = {},
           sound[kMaxCabinets] = {}, audio = 0, total = 0;
};

// Options (daytona_set).
unsigned s_wantCabinets = 2;
unsigned s_view = 0;
int s_steerStep = 12;     // ADC units a frame once the slow stage is over, and back to centre: the app's 0.12 of full lock
int s_steerSlow = 3;      // ADC units a frame for the first s_steerSlowFrames of a press away from centre
int s_steerSlowFrames = 16;
bool s_drawHidden = false;
bool s_blankImages = false; // toolchain test: zeros for ROM images (test_blank_images)
std::string s_testProgram;  // with them: an i960 program image of our own (test_program)
std::string s_nvramDir = "/nvram";
bool s_presets = true;      // look in the built-in presets (/daytona/nvram) too
struct Scripted
{
  bool on = false;
  tools::Script script;
} s_scripts[kMaxCabinets];

// The machine.
bool s_loaded, s_halted;
rt::M2Board::Images s_images;     // as imported, kept so a power-on (every retro_reset) is fast
std::vector<Cabinet> s_cabs;
size_t s_stateSize;

int BoardLink(unsigned k)
{
  if (k >= s_cabs.size() || !s_cabs[k].game) return -1;
  const rt::CommBoard *board = s_cabs[k].game->board().comm_board();
  return board ? int(board->link()) : -1;
}
int BoardCount(unsigned k)
{
  if (k >= s_cabs.size() || !s_cabs[k].game) return 0;
  const rt::CommBoard *board = s_cabs[k].game->board().comm_board();
  return board && board->link() == rt::CommBoard::Link::Up ? board->count() : 0;
}

// The bridge: says so when the cabinet's link id is not its seat's (a state of another seat, or
// the wrong seat option): its block would land in the others' frames in another seat's slot.
void CheckSeat(const char *when)
{
  if (!Bridging() || s_cabs.empty() || !s_cabs[0].game) return;
  const rt::CommBoard *board = s_cabs[0].game->board().comm_board();
  if (!board || board->link() != rt::CommBoard::Link::Up || board->id() == int(s_seat + 1)) return;
  Log(RETRO_LOG_WARN, "%s: this cabinet is link id %d of %d (seat %d), but seat=%u (link id %u): load seat %u's state, or set seat=%d.",
      when, board->id(), board->count(), board->id() - 1, s_seat, s_seat + 1, s_seat, board->id() - 1);
}
std::vector<int16_t> s_audio;
Timings s_timings;

void Halt(const char *what, const char *why)
{
  s_halted = true;
  Log(RETRO_LOG_ERROR, "%s: %s. The machine has stopped; reset or load a state to go on.", what, why);
}

// The cabinet's settings EEPROM and backup RAM (tools/common/nvram.h's files) from the first of:
// <nvram_dir>/<cabinet>/ (put there by the frontend), the presets built in for this many
// cabinets (/daytona/nvram/<cabinets>/<cabinet>/, from daytona/nvram/), else the factory's
// (a linked twin cabinet: master, car 1).
void LoadNvram(unsigned k, rt::GameLoop &game)
{
  auto read = [](const std::string &path, auto &into) {
    std::ifstream f(path, std::ios::binary);
    if (!f) return false;
    const std::vector<uint8_t> d{std::istreambuf_iterator<char>(f), {}};
    if (d.size() != into.size())
    {
      Log(RETRO_LOG_WARN, "%s: %zu bytes, not %zu: not loaded.", path.c_str(), d.size(), into.size());
      return false;
    }
    std::copy(d.begin(), d.end(), into.begin());
    return true;
  };
  const std::string dirs[] = { s_nvramDir + "/" + std::to_string(k),
                               "/daytona/nvram/" + std::to_string(s_count) + "/" + std::to_string(k) };
  for (const std::string &dir : dirs)
  {
    if (!s_presets && &dir != &dirs[0]) break;
    const bool eeprom = read(dir + "/ioboard_eeprom.bin", game.board().io().eeprom);
    const bool backup = read(dir + "/backup_ram.bin", game.board().backup_ram());
    if (eeprom || backup)
    {
      Log(RETRO_LOG_INFO, "Cabinet %u: settings from %s (%s%s%s).", k + 1, dir.c_str(), eeprom ? "EEPROM" : "",
          eeprom && backup ? ", " : "", backup ? "backup RAM" : "");
      return;
    }
  }
  if (s_count > 1 && k > 0)
    Log(RETRO_LOG_WARN, "Cabinet %u: factory settings (LINK ID master, like cabinet 1): the link will not come up "
                        "without a preset that makes it a slave (daytona/nvram/README.md).", k + 1);
  else if (s_count == 1)
    Log(RETRO_LOG_WARN, "Cabinet 1: factory settings (a linked twin cabinet): alone it waits for a second cabinet "
                        "without a single-cabinet preset (daytona/nvram/README.md).");
}

// The board's memory images from the ROM set (CRC-checked, laid out as MAME loads them), ~82 MB.
// Kept: importing again would cost every retro_reset about a second in WebAssembly (the zip's
// ~46 MB of ROMs inflate at ~68 MB/s there), and the worker resets before every state it loads.
rt::M2Board::Images LoadImages(const char *path)
{
  if (!s_blankImages) return rt::import_rom_set(path);
  // Toolchain test (daytona_set("test_blank_images", "1")): no ROM set, every image zeros at
  // its size, so the machine is built and its first frame runs into the game code.
  rt::M2Board::Images img;
  img.program.assign(0x200000, 0);
  img.main_data.assign(0x2000000, 0);
  img.copro_data.assign(0x800000, 0);
  img.polygons.assign(0x1000000, 0);
  img.textures.assign(0x1000000, 0);
  img.copro_tables.assign(0x40000, 0);
  img.sound_program.assign(0x40000, 0);
  img.pcm1.assign(0x400000, 0);
  img.pcm2.assign(0x400000, 0);
  if (!s_testProgram.empty())
  {
    // A program of our own for the i960 (no sound board: there is no sound program to run).
    std::ifstream f(s_testProgram, std::ios::binary);
    std::vector<uint8_t> program{std::istreambuf_iterator<char>(f), {}};
    if (program.empty() || program.size() > img.program.size()) throw std::runtime_error("test_program: no such image, or too big");
    std::copy(program.begin(), program.end(), img.program.begin());
    img.sound_program.clear();
  }
  return img;
}

// Cabinet k at power-on: a fresh board from (a copy of) the images, its settings, its seat and
// sound carry cleared; its cables (and the other cabinets) as they are.
void PowerOnCabinet(unsigned k)
{
  Cabinet &c = s_cabs[k];
  c.game.reset(); // the old board's memory first
  c.seat = Seat{};
  c.fm = Stream{};
  c.pcm = Stream{};
  c.samples = c.dropped = 0;
  c.game = std::make_unique<rt::GameLoop>(s_images);
  c.game->set_profile_clock(NowUs);
  LoadNvram(k, *c.game);
  if (s_count > 1 || s_star)
  {
    if (!c.link)
    {
      if (Bridging()) c.link = std::make_unique<Bridge>(k);
      else if (s_star) c.link = std::make_unique<Star>(k);
      else c.link = std::make_unique<Loop>(k);
    }
    // No frame sync: the cabinets run one frame each per retro_run, in order, so they are in
    // step by construction (and a sync wait could never be met inside one thread).
    c.game->board().set_link(c.link.get(), false);
  }
  c.fm.Rate(snd::SoundBoard::kYmClock, 144);   // SoundBoard::fm_rate()
  c.pcm.Rate(snd::SoundBoard::kPcmClock, 224); // SoundBoard::pcm_rate()
}

// Every cabinet at power-on: fresh boards, settings, cables empty.
void PowerOn()
{
  s_cabs.clear(); // the old machine's memory first
  for (unsigned k = 0; k < kMaxCabinets; k++)
  {
    s_wires[k].Clear();
    s_wires[k].rng = 0x9e3779b97f4a7c15ull * (k + 1);
    s_published[k].clear();
    s_control[k].clear();
  }
  s_bridge = BridgeState{};
  s_outValid = false;
  s_cabs.resize(s_count);
  for (unsigned k = 0; k < s_count; k++) PowerOnCabinet(k);
  s_halted = false;
}

unsigned View() { return s_view < s_count ? s_view : 0; }

// The cabinet's controls from its RetroPad: LEFT/RIGHT steer (a ramp toward full lock and back:
// s_steerSlow a frame for the first s_steerSlowFrames of a press that turns the wheel away from
// centre, s_steerStep a frame after that, when counter-steering and back to centre), UP
// accelerator, DOWN brake, B/A shift down/up through gears 1-4, Y X L R the view buttons VR1-VR4,
// START start, SELECT coin. The slow stage is for the game's circuit select, which follows the
// wheel's position (+16..+64 ADC units from centre is ADVANCED, +72 and over EXPERT, measured):
// a short hold of RIGHT with the accelerator pressed picks ADVANCED, a longer one EXPERT; in a
// race a tap is a small correction and a hold reaches full lock in about a third of a second.
rt::Inputs FromPad(Seat &seat, uint32_t pad)
{
  auto held = [pad](unsigned id) { return ((pad >> id) & 1) != 0; };
  rt::Inputs in;
  const int dir = (held(RETRO_DEVICE_ID_JOYPAD_RIGHT) ? 1 : 0) - (held(RETRO_DEVICE_ID_JOYPAD_LEFT) ? 1 : 0);
  if (dir == 0) seat.turning = 0;
  else if (seat.turning != 0 && (seat.turning > 0) == (dir > 0)) seat.turning += dir;
  else seat.turning = dir;
  const int target = dir * kLock;
  const bool away = dir != 0 && (seat.steer == 0 || (seat.steer > 0) == (dir > 0)); // not counter-steering
  const int step = away && std::abs(seat.turning) <= s_steerSlowFrames ? s_steerSlow : s_steerStep;
  seat.steer += std::clamp(target - seat.steer, -step, step);
  in.steer = uint8_t(0x80 + seat.steer);
  in.accel = held(RETRO_DEVICE_ID_JOYPAD_UP) ? 0xe0 : 0x20;
  in.brake = held(RETRO_DEVICE_ID_JOYPAD_DOWN) ? 0xe0 : 0x20;
  const uint32_t pressed = pad & ~seat.held;
  if ((pressed >> RETRO_DEVICE_ID_JOYPAD_A) & 1) seat.gear = std::min(4, seat.gear + 1);
  if ((pressed >> RETRO_DEVICE_ID_JOYPAD_B) & 1) seat.gear = std::max(1, seat.gear - 1);
  seat.held = pad;
  static const uint8_t kGearValue[5] = { 0, 2, 1, 6, 5 }; // MAME daytona_gearbox_r: neutral, 1-4
  in.in1 = uint8_t((in.in1 & ~0x70) | (kGearValue[seat.gear] << 4));
  auto low = [&](uint8_t &port, uint8_t bit, unsigned id) { if (held(id)) port &= uint8_t(~bit); };
  low(in.in0, 0x01, RETRO_DEVICE_ID_JOYPAD_SELECT); // coin
  low(in.in0, 0x10, RETRO_DEVICE_ID_JOYPAD_START);
  low(in.in0, 0x20, RETRO_DEVICE_ID_JOYPAD_Y);      // VR1
  low(in.in0, 0x40, RETRO_DEVICE_ID_JOYPAD_X);      // VR2
  low(in.in0, 0x80, RETRO_DEVICE_ID_JOYPAD_L);      // VR3
  low(in.in1, 0x01, RETRO_DEVICE_ID_JOYPAD_R);      // VR4
  return in;
}

uint32_t ReadPad(unsigned port)
{
  uint32_t pad = 0;
  if (!input_state_cb) return pad;
  for (unsigned id = 0; id <= RETRO_DEVICE_ID_JOYPAD_R; id++)
    if (input_state_cb(port, RETRO_DEVICE_JOYPAD, 0, id)) pad |= 1u << id;
  return pad;
}

// This frame's sound of one cabinet at 48 kHz, into `out` (when given) as int16 stereo.
void MixSound(Cabinet &c, std::vector<int16_t> *out)
{
  snd::SoundBoard *sound = c.game->sound();
  if (!sound) return;
  const std::vector<float> fm = sound->take_fm(), pcm = sound->take_pcm();
  c.fm.in.insert(c.fm.in.end(), fm.begin(), fm.end());
  c.pcm.in.insert(c.pcm.in.end(), pcm.begin(), pcm.end());
  const uint64_t end = std::min(c.fm.Available(), c.pcm.Available());
  for (uint64_t j = c.samples; out && j < end; j++)
  {
    float fl, fr, pl, pr;
    c.fm.At(j, fl, fr);
    c.pcm.At(j, pl, pr);
    out->push_back(int16_t(std::lround(std::clamp(fl + pl, -1.0f, 1.0f) * 32767.0f)));
    out->push_back(int16_t(std::lround(std::clamp(fr + pr, -1.0f, 1.0f) * 32767.0f)));
  }
  c.samples = std::max(c.samples, end);
  c.fm.DropBefore(c.samples);
  c.pcm.DropBefore(c.samples);
}

/******************************************************************************
 Save states: [magic, version, cabinets] then per cabinet [shim state][snapshot], then the cables
******************************************************************************/

class Writer
{
public:
  Writer(uint8_t *data, size_t size) : p_(data), end_(data + size) {}
  bool ok() const { return ok_; }
  size_t used(const uint8_t *start) const { return size_t(p_ - start); }
  void bytes(const void *src, size_t n)
  {
    if (!ok_ || size_t(end_ - p_) < n) { ok_ = false; return; }
    memcpy(p_, src, n);
    p_ += n;
  }
  void u32(uint32_t v) { bytes(&v, 4); }
  void u64(uint64_t v) { bytes(&v, 8); }
  void i32(int32_t v) { bytes(&v, 4); }

private:
  uint8_t *p_, *end_;
  bool ok_ = true;
};

class Reader
{
public:
  Reader(const uint8_t *data, size_t size) : p_(data), end_(data + size) {}
  bool ok() const { return ok_; }
  const uint8_t *take(size_t n)
  {
    if (!ok_ || size_t(end_ - p_) < n) { ok_ = false; return nullptr; }
    const uint8_t *at = p_;
    p_ += n;
    return at;
  }
  void bytes(void *dst, size_t n) { if (const uint8_t *at = take(n)) memcpy(dst, at, n); }
  uint32_t u32() { uint32_t v = 0; bytes(&v, 4); return v; }
  uint64_t u64() { uint64_t v = 0; bytes(&v, 8); return v; }
  int32_t i32() { int32_t v = 0; bytes(&v, 4); return v; }

private:
  const uint8_t *p_, *end_;
  bool ok_ = true;
};

// The shim's part of a cabinet: its seat and the sound not yet turned into 48 kHz.
size_t ShimStateBound() { return 4 * 4 + 8 + 2 * (8 + 4 + kAudioCarryMax * 2 * 4); }

bool WriteShimState(Writer &w, const Cabinet &c)
{
  w.i32(c.seat.steer);
  w.i32(c.seat.gear);
  w.u32(c.seat.held);
  w.i32(c.seat.turning);
  w.u64(c.samples);
  for (const Stream *s : { &c.fm, &c.pcm })
  {
    if (s->in.size() > kAudioCarryMax * 2) return false;
    w.u64(s->base);
    w.u32(uint32_t(s->in.size()));
    w.bytes(s->in.data(), s->in.size() * sizeof(float));
  }
  return w.ok();
}

bool ReadShimState(Reader &r, Cabinet &c)
{
  Seat seat;
  seat.steer = r.i32();
  seat.gear = r.i32();
  seat.held = r.u32();
  seat.turning = r.i32();
  const uint64_t samples = r.u64();
  Stream fm = c.fm, pcm = c.pcm;
  for (Stream *s : { &fm, &pcm })
  {
    s->base = r.u64();
    const uint32_t n = r.u32();
    if (!r.ok() || n > kAudioCarryMax * 2) return false;
    s->in.resize(n);
    r.bytes(s->in.data(), n * sizeof(float));
  }
  if (!r.ok() || seat.steer < -kLock || seat.steer > kLock || seat.gear < 1 || seat.gear > 4) return false;
  c.seat = seat;
  c.samples = samples;
  c.fm = std::move(fm);
  c.pcm = std::move(pcm);
  return true;
}

// The bridge's table (cabinets=1, link_topology=star), after the cabinet: "DAYS", version 1, the
// seat it was saved as, flags (bit 0 a numbering answer due, bit 1 a block handed over since the
// last frame), the cabinet's own block as last sent, then per seat 0-7 [u32 flags: bit 0 seen,
// bit 1 absent][its block as the board would get it]. A state without it (another machine's)
// leaves every other seat empty.
constexpr uint32_t kBridgeMagic = 0x53594144; // "DAYS"
constexpr uint32_t kBridgeVersion = 1;
size_t BridgeStateSize() { return 4 * 4 + kSlot + kMaxCabinets * (4 + kSlot); }

void WriteBridge(Writer &w)
{
  w.u32(kBridgeMagic);
  w.u32(kBridgeVersion);
  w.u32(s_seat);
  w.u32((s_bridge.answer ? 1u : 0u) | (s_bridge.fresh ? 2u : 0u));
  w.bytes(s_bridge.own, kSlot);
  for (const BridgeSeat &seat : s_bridge.seats)
  {
    w.u32((seat.seen ? 1u : 0u) | (seat.absent ? 2u : 0u));
    w.bytes(seat.block, kSlot);
  }
}

// Into `into` (s_bridge once it all read); false when the bytes are not a bridge table.
bool ReadBridge(Reader &r, BridgeState &into, unsigned &seat)
{
  if (r.u32() != kBridgeMagic || r.u32() != kBridgeVersion) return false;
  seat = r.u32();
  const uint32_t flags = r.u32();
  into.answer = flags & 1;
  into.fresh = flags & 2;
  r.bytes(into.own, kSlot);
  for (BridgeSeat &s : into.seats)
  {
    const uint32_t f = r.u32();
    s.seen = f & 1;
    s.absent = f & 2;
    s.at = s_frame;
    r.bytes(s.block, kSlot);
  }
  return r.ok() && seat < kMaxCabinets;
}

size_t StateSize()
{
  size_t size = 3 * 4;
  for (const Cabinet &c : s_cabs) size += 4 + ShimStateBound() + 4 + rt::state_size_bound(*c.game);
  if (s_count > 1) size += s_count * (4 + kWireCap);
  if (Bridging()) size += BridgeStateSize();
  return (size + 0xfff) & ~size_t(0xfff);
}

/******************************************************************************
 Options and tools, for the frontend and the scripts (bench, check, make-nvram)
******************************************************************************/

} // namespace

// Options, by name (all values are strings):
//   cabinets     "1" to "8" (default "2"): linked cabinets in the machine, in a ring (cabinet k
//                gets link id k + 1 with presets that make cabinet 0 the master and the others
//                slaves); read when the game loads.
//   view         "0" (default) to cabinets - 1: the cabinet whose picture and sound the frontend
//                gets. Any time; the others are not rasterized (unless draw_hidden).
//   steer_step   ADC units the wheel moves a frame toward the pad's target after the slow stage,
//                when counter-steering and back to centre (default 12, the app's 0.12 of full
//                lock). steer_slow (default 3) and steer_slow_frames (default 16): the slow stage
//                of a press away from centre (FromPad). Machine behaviour: the same on every
//                player's machine.
//   nvram_dir    where cabinet k's ioboard_eeprom.bin and backup_ram.bin are looked for, as
//                <nvram_dir>/<k>/ (default /nvram), at power-on.
//   presets      "0": not the built-in settings presets (make-nvram.mjs starts from the factory's).
//   draw_hidden  "1": rasterize every cabinet every frame (to check that drawing changes nothing).
//   script0..7   path of an input script (the recomp's scripts/inputs format, as m2run --inputs)
//                driving that cabinet instead of its RetroPad, by the board's frame; "" stops it.
//   Experiments on the link (ring-notes.md; not machine state, none of them in save states):
//   link_delay   retro_runs each cable holds bytes before the next cabinet can read them
//                (default 0: within the same retro_run). For new bytes; any time.
//   link_jitter  up to this many more, at random per write (deterministic xorshift per cable),
//                still in order (default 0).
//   link_cut     cabinets (a comma-separated list; "" none) powered off: not run, their cables
//                closed (the boards either side lose the link). A cabinet taken off the list is
//                reconnected with both its cables emptied; daytona_cabinet_reset boots it.
//   link_pause   cabinets not run, cables left open (nobody reads or writes them).
//   link_ghost   cabinets whose game is not run but whose comm board is, once a frame: it keeps
//                the cabinet's place in the ring, passing on what it receives with the cabinet's
//                own block as the game last wrote it.
//   link_full    "refuse" (default): a cable holding 32 frames ready and unread refuses a write
//                (the writing board loses the link); "drop": its oldest frames make room.
//   link_assist  "1": a slave waiting for its number while the master is up gets the master's
//                0xfe frame (its id, the count) from its transport (Cable::Assist).
//   link_pace    "1": a board is handed at most one data frame a frame; "0": (ring) as they
//                come, (the bridge) a frame only in a retro_run after a block was handed over;
//                "" the default: 1 in a star, 0 in the ring.
//   link_topology "ring" (default) or "star" (class Star; with cabinets=1, class Bridge: the
//                arcade mode), read when the game loads.
//   link_blank   cabinets (a list, "" none) whose blocks the star hands out as zeros.
//   The arcade mode (cabinets=1, link_topology=star; class Bridge, daytona_link_in/out/absent):
//   seat         "0" to "7" (default "0"): which seat of the star this cabinet is, its block's
//                slot; it must be the loaded state's (link id seat + 1): warned about otherwise.
//                Any time (it decides the frames the board is handed from the next retro_run).
//   link_absent  "zero" (default): a seat that leaves (daytona_link_absent) reads as zeros, "no
//                cabinet" (its car leaves the track); "freeze": it keeps its last block (its car
//                stays where it stopped, in the others' way). For seats that leave after it is set.
//   test_blank_images  "1": load zeros instead of the ROM set (toolchain test of the stub build:
//                the machine is built, its first frame stops at the game code that isn't there).
//   test_program with test_blank_images: an i960 program image of our own in place of the
//                game's, and no sound board (timing recompiled code without the ROM set).
extern "C" void daytona_set(const char *key, const char *value)
{
  const std::string k = key ? key : "", v = value ? value : "";
  const int n = atoi(v.c_str());
  if (k == "cabinets")
  {
    if (n < 1 || n > int(kMaxCabinets)) { Log(RETRO_LOG_WARN, "cabinets: 1 to %u, not %s.", kMaxCabinets, v.c_str()); return; }
    s_wantCabinets = unsigned(n);
    if (s_loaded && s_wantCabinets != s_count)
      Log(RETRO_LOG_WARN, "cabinets=%u takes effect when a game is loaded next (this one has %u).", s_wantCabinets, s_count);
  }
  else if (k == "view") s_view = unsigned(std::max(0, n));
  else if (k == "steer_step") s_steerStep = std::clamp(n, 1, 2 * kLock);
  else if (k == "steer_slow") s_steerSlow = std::clamp(n, 1, 2 * kLock);
  else if (k == "steer_slow_frames") s_steerSlowFrames = std::clamp(n, 0, 600);
  else if (k == "nvram_dir") s_nvramDir = v;
  else if (k == "presets") s_presets = !(v == "0" || v == "false");
  else if (k == "draw_hidden") s_drawHidden = v == "1" || v == "true";
  else if (k == "test_blank_images") s_blankImages = v == "1" || v == "true";
  else if (k == "test_program") s_testProgram = v;
  else if (k == "link_delay") s_linkDelay = unsigned(std::clamp(n, 0, 3600));
  else if (k == "link_assist") s_linkAssist = v == "1" || v == "true";
  else if (k == "link_pace") s_linkPace = v.empty() ? -1 : v == "1" || v == "true" ? 1 : 0;
  else if (k == "seat")
  {
    if (v.empty() || n < 0 || n >= int(kMaxCabinets) || v.find_first_not_of("0123456789") != std::string::npos)
    {
      Log(RETRO_LOG_WARN, "seat: 0 to %u, not %s.", kMaxCabinets - 1, v.c_str());
      return;
    }
    s_seat = unsigned(n);
    CheckSeat("seat");
  }
  else if (k == "link_absent")
  {
    if (v != "freeze" && v != "zero") { Log(RETRO_LOG_WARN, "link_absent: freeze or zero, not %s.", v.c_str()); return; }
    s_absentZero = v == "zero";
  }
  else if (k == "link_full")
  {
    if (v != "refuse" && v != "drop") { Log(RETRO_LOG_WARN, "link_full: refuse or drop, not %s.", v.c_str()); return; }
    s_linkDrop = v == "drop";
  }
  else if (k == "link_topology")
  {
    if (v != "ring" && v != "star") { Log(RETRO_LOG_WARN, "link_topology: ring or star, not %s.", v.c_str()); return; }
    s_starWanted = v == "star";
  }
  else if (k == "link_jitter") s_linkJitter = unsigned(std::clamp(n, 0, 3600));
  else if (k == "link_blank")
  {
    std::fill(std::begin(s_blank), std::end(s_blank), false);
    for (size_t at = 0; at < v.size();)
    {
      const size_t comma = std::min(v.find(',', at), v.size());
      const int c = atoi(v.substr(at, comma - at).c_str());
      if (comma > at && c >= 0 && c < int(kMaxCabinets)) s_blank[c] = true;
      at = comma + 1;
    }
  }
  else if (k == "link_cut" || k == "link_pause" || k == "link_ghost")
  {
    const Mode mode = k == "link_cut" ? Mode::Cut : k == "link_pause" ? Mode::Pause : Mode::Ghost;
    bool listed[kMaxCabinets] = {};
    for (size_t at = 0; at < v.size();)
    {
      const size_t comma = std::min(v.find(',', at), v.size());
      const int c = atoi(v.substr(at, comma - at).c_str());
      if (comma > at && c >= 0 && c < int(kMaxCabinets)) listed[c] = true;
      at = comma + 1;
    }
    for (unsigned c = 0; c < kMaxCabinets; c++)
    {
      if (listed[c]) s_mode[c] = mode;
      else if (s_mode[c] == mode)
      {
        s_mode[c] = Mode::Run;
        // Reconnected: both its cables start empty (a powered-off cabinet's connections closed).
        if (mode == Mode::Cut && s_loaded && c < s_count && s_count > 1)
        {
          s_wires[c].Clear();
          s_wires[Prev(c)].Clear();
        }
      }
    }
  }
  else if (k.size() == 7 && k.compare(0, 6, "script") == 0 && k[6] >= '0' && k[6] < char('0' + kMaxCabinets))
  {
    Scripted &s = s_scripts[k[6] - '0'];
    s = Scripted{};
    if (v.empty()) return;
    try
    {
      s.script.load(v);
      s.on = true;
    }
    catch (const std::exception &e)
    {
      Log(RETRO_LOG_ERROR, "%s: %s", k.c_str(), e.what());
    }
  }
  else Log(RETRO_LOG_WARN, "daytona_set: no option %s.", k.c_str());
}

// Where the frames since the last call went, in microseconds summed over them, as JSON:
// {"frames":n,"cabinets":c,"logic":[..],"geometry":[..],"raster":[..],"sound":[..],"audio":..,"total":..}
// logic: the game's code (i960, TGP) and the board; geometry: the geometrizer at vblank; raster:
// composing the screen (0 for a cabinet not drawn); sound: the sound board; audio: the shim's
// resampling and mixing; total: all of retro_run. For bench.mjs and the harness.
extern "C" const char *daytona_timings(void)
{
  static std::string text;
  const Timings &t = s_timings;
  auto list = [](const uint64_t *v) {
    std::string s = "[";
    for (unsigned k = 0; k < s_count; k++) s += (k ? "," : "") + std::to_string(v[k]);
    return s + "]";
  };
  text = "{\"frames\":" + std::to_string(t.frames) + ",\"cabinets\":" + std::to_string(s_count) + ",\"logic\":" + list(t.logic) +
         ",\"geometry\":" + list(t.geometry) + ",\"raster\":" + list(t.raster) + ",\"sound\":" + list(t.sound) +
         ",\"audio\":" + std::to_string(t.audio) + ",\"total\":" + std::to_string(t.total) + "}";
  s_timings = Timings{};
  return text.c_str();
}

// The cabinets' link as their communication boards see it, as JSON:
// {"cabinets":2,"link":[{"state":"up","id":1,"count":2},{"state":"up","id":2,"count":2}]}
// state: none (one cabinet: no board), off (the game has not started it), waiting, up, lost.
// The arcade mode's bridge adds the star as this seat sees it:
// "star":{"seat":0,"count":8,"absent":"zero","seats":[{"state":"self"},{"state":"present","age":1},
//   {"state":"absent","frozen":false,"age":300},{"state":"empty"},...]} (count: the seats, as the
// board was numbered; absent: the link_absent policy; age: retro_runs since the seat's block was
// handed over, or since it left, 1 = just before the last retro_run; frozen: a seat that left
// still has a block, link_absent=freeze; empty: no block yet).
extern "C" const char *daytona_link_status(void)
{
  static std::string text;
  static const char *const kStates[] = { "off", "waiting", "up", "lost" };
  text = "{\"cabinets\":" + std::to_string(s_count) + ",\"link\":[";
  for (unsigned k = 0; k < s_cabs.size(); k++)
  {
    const rt::CommBoard *board = s_cabs[k].game ? s_cabs[k].game->board().comm_board() : nullptr;
    text += k ? "," : "";
    if (!board) text += "{\"state\":\"none\"}";
    else text += std::string("{\"state\":\"") + kStates[int(board->link())] + "\",\"id\":" + std::to_string(board->id()) +
                 ",\"count\":" + std::to_string(board->count()) + "}";
    if (s_mode[k] != Mode::Run)
    {
      static const char *const kModes[] = { "run", "cut", "pause", "ghost" };
      text.back() = ',';
      text += std::string("\"mode\":\"") + kModes[int(s_mode[k])] + "\"}";
    }
  }
  text += "]";
  if (Bridging())
  {
    // The star as this seat sees it: per seat self, present (a block handed over, age in
    // retro_runs since), absent (left: frozen at its last block, or zeros) or empty (none yet).
    const unsigned count = BridgeCount();
    text += ",\"star\":{\"seat\":" + std::to_string(s_seat) + ",\"count\":" + std::to_string(count) + ",\"absent\":\"" +
            (s_absentZero ? "zero" : "freeze") + "\",\"seats\":[";
    for (unsigned c = 0; c < count; c++)
    {
      const BridgeSeat &seat = s_bridge.seats[c];
      const std::string age = ",\"age\":" + std::to_string(s_frame - std::min(s_frame, seat.at));
      text += c ? "," : "";
      if (c == s_seat) text += "{\"state\":\"self\"}";
      else if (seat.absent)
      {
        const bool frozen = std::any_of(std::begin(seat.block), std::end(seat.block), [](uint8_t b) { return b != 0; });
        text += std::string("{\"state\":\"absent\",\"frozen\":") + (frozen ? "true" : "false") + age + "}";
      }
      else if (seat.seen) text += "{\"state\":\"present\"" + age + "}";
      else text += "{\"state\":\"empty\"}";
    }
    text += "]}";
  }
  return (text += "}").c_str();
}

// The arcade mode's bridge (cabinets=1, link_topology=star; class Bridge). Each frame the
// frontend calls daytona_link_out after retro_run and sends the block to the other seats when it
// changed; before the next retro_run it calls daytona_link_in for every block that arrived
// (latest wins) and daytona_link_absent for a seat that left. What the board is handed in a
// retro_run depends only on those calls made before it (and the cabinet itself).
// daytona_link_block_size: 448 (0x1c0), the size of a block.
// daytona_link_out: copies this cabinet's own block, as its board last sent it (the one the
//   others need now), to dst (448 bytes); 1 when it differs from what the previous call returned
//   (always 1 after power-on or a state load), else 0.
// daytona_link_in: seat's latest block (len 448: anything else is rejected and logged), for the
//   next retro_run on; this cabinet's own seat is ignored (logged).
// daytona_link_absent: seat left: its block reads as zeros (link_absent=zero, the default: its car
//   leaves the others' race) or stays as last seen (link_absent=freeze: its car stays where it
//   stopped, an obstacle); a seat never seen reads as zeros either way. The next daytona_link_in
//   for it makes it present again. Measured (ring-notes.md, "The bridge"): a seat leaving after
//   the race started costs the others nothing; one leaving between START and the race start
//   leaves its session's other entrants waiting for it for good, either way.
static unsigned s_linkMisuse = 0;
static bool LinkSeat(const char *what, int seat)
{
  if (!Bridging())
  {
    if (Often(s_linkMisuse)) Log(RETRO_LOG_WARN, "%s: only for one cabinet with link_topology=star, loaded (the arcade mode).", what);
    return false;
  }
  if (seat < 0 || seat >= int(kMaxCabinets))
  {
    if (Often(s_linkMisuse)) Log(RETRO_LOG_WARN, "%s: no seat %d (0 to %u).", what, seat, kMaxCabinets - 1);
    return false;
  }
  if (unsigned(seat) == s_seat)
  {
    if (Often(s_linkMisuse)) Log(RETRO_LOG_WARN, "%s: seat %d is this cabinet's own: ignored.", what, seat);
    return false;
  }
  return true;
}

extern "C" int daytona_link_block_size(void) { return int(kSlot); }

extern "C" int daytona_link_out(uint8_t *dst)
{
  if (!dst) return 0;
  if (!Bridging())
  {
    std::fill_n(dst, kSlot, uint8_t(0));
    if (Often(s_linkMisuse)) Log(RETRO_LOG_WARN, "daytona_link_out: only for one cabinet with link_topology=star, loaded (the arcade mode).");
    return 0;
  }
  std::copy(s_bridge.own, s_bridge.own + kSlot, dst);
  const bool changed = !s_outValid || !std::equal(s_bridge.own, s_bridge.own + kSlot, s_outLast);
  std::copy(s_bridge.own, s_bridge.own + kSlot, s_outLast);
  s_outValid = true;
  return changed ? 1 : 0;
}

extern "C" void daytona_link_in(int seat, const uint8_t *src, int len)
{
  if (!LinkSeat("daytona_link_in", seat)) return;
  if (!src || len != int(kSlot))
  {
    if (Often(s_linkMisuse)) Log(RETRO_LOG_WARN, "daytona_link_in: seat %d's block is %d bytes, not %zu: rejected.", seat, src ? len : 0, kSlot);
    return;
  }
  BridgeSeat &s = s_bridge.seats[seat];
  std::copy(src, src + kSlot, s.block);
  s.seen = true;
  s.absent = false;
  s.at = s_frame;
  s_bridge.fresh = true;
}

extern "C" void daytona_link_absent(int seat)
{
  if (!LinkSeat("daytona_link_absent", seat)) return;
  BridgeSeat &s = s_bridge.seats[seat];
  if (s_absentZero) std::fill(std::begin(s.block), std::end(s.block), uint8_t(0));
  s.absent = true;
  s.at = s_frame;
  s_bridge.fresh = true;
}

// The ring's traffic since the game was loaded, as JSON (ring-notes.md's measurements):
// {"frame":n,"delay":d,"jitter":j,"cabinets":[{"mode":"run","tx":bytes,"data":n,"vsync":n,
//  "tokens":n,"rx":bytes,"rxData":n,"shift":[same,differ],"slot":[changed,bytes],"queued":bytes}]}
// tx/rx: bytes the cabinet's board wrote and read; data/vsync/tokens: frames it wrote by kind;
// shift: data frames it sent whose bytes after its own block were the start of the data frame
// it had last received, and those that were not; slot: data frames whose own block differed from
// the previous one, and the bytes that did; queued: bytes in its outgoing cable.
extern "C" const char *daytona_link_stats(void)
{
  static std::string text;
  static const char *const kModes[] = { "run", "cut", "pause", "ghost" };
  text = "{\"frame\":" + std::to_string(s_frame) + ",\"delay\":" + std::to_string(s_linkDelay) + ",\"jitter\":" +
         std::to_string(s_linkJitter) + ",\"topology\":\"" + (s_star ? "star" : "ring") + "\",\"cabinets\":[";
  for (unsigned k = 0; k < s_cabs.size(); k++)
  {
    text += k ? "," : "";
    text += std::string("{\"mode\":\"") + kModes[int(s_mode[k])] + "\"";
    if (const Cabinet &c = s_cabs[k]; c.link)
    {
      const LinkStats &s = c.link->stats;
      auto n = [](uint64_t v) { return std::to_string(v); };
      text += ",\"tx\":" + n(s.txBytes) + ",\"data\":" + n(s.txData) + ",\"vsync\":" + n(s.txVsync) + ",\"tokens\":" +
              n(s.txToken) + ",\"rx\":" + n(s.rxBytes) + ",\"rxData\":" + n(s.rxData) + ",\"shift\":[" + n(s.shiftSame) + "," +
              n(s.shiftDiff) + "],\"slot\":[" + n(s.slotChanged) + "," + n(s.slotBytesChanged) + "],\"queued\":" +
              n(s_wires[k].Size()) + ",\"assists\":" + n(s.assists) + ",\"dropped\":" + n(s.dropped);
    }
    text += "}";
  }
  return (text += "]}").c_str();
}

// The last data frame (0xe01 bytes: the sender's id, then 0xe00) cabinet k received (sent = 0) or
// sent (sent = 1), or null: for the harness to look at the blocks.
extern "C" const uint8_t *daytona_link_frame(unsigned k, unsigned sent)
{
  if (k >= s_cabs.size() || !s_cabs[k].link) return nullptr;
  const LinkStats &s = s_cabs[k].link->stats;
  if (sent ? !s.haveTx : !s.haveRx) return nullptr;
  return sent ? s.lastTx : s.lastRx;
}

// One cabinet alone (the ring experiments: a player sitting down with a saved cabinet).
// daytona_cabinet_save: cabinet k's state, "DAYC" magic, version, [u32 size][shim state][u32
// size][rt::save_state of its GameLoop] (its settings EEPROM and backup RAM included, so its LINK
// ID and CAR NUMBER too); no cables. The bytes stay valid until the next call; *size is 0 and the
// result null when it fails. daytona_cabinet_load: such a state into cabinet k (any machine of
// this build with a link, the same or another process), the other cabinets and the cables as
// they are; false (the cabinet untouched) when it does not load. daytona_cabinet_reset: power-on
// of cabinet k alone (fresh board, its settings), the rest as it is.
static constexpr uint32_t kCabinetMagic = 0x43594144; // "DAYC"
static constexpr uint32_t kCabinetVersion = 1;
static std::vector<uint8_t> s_cabinetState;

extern "C" const uint8_t *daytona_cabinet_save(unsigned k, size_t *size)
{
  if (size) *size = 0;
  if (!s_loaded || s_halted || k >= s_cabs.size()) return nullptr;
  try
  {
    Cabinet &c = s_cabs[k];
    std::vector<uint8_t> shim(ShimStateBound());
    Writer ws(shim.data(), shim.size());
    if (!WriteShimState(ws, c)) return nullptr;
    const std::vector<uint8_t> snapshot = rt::save_state(*c.game);
    if (snapshot.empty()) return nullptr;
    const size_t used = ws.used(shim.data());
    s_cabinetState.assign(4 * 4 + used + snapshot.size(), 0);
    Writer w(s_cabinetState.data(), s_cabinetState.size());
    w.u32(kCabinetMagic);
    w.u32(kCabinetVersion);
    w.u32(uint32_t(used));
    w.bytes(shim.data(), used);
    w.u32(uint32_t(snapshot.size()));
    w.bytes(snapshot.data(), snapshot.size());
    if (!w.ok()) return nullptr;
  }
  catch (const std::exception &e)
  {
    Log(RETRO_LOG_ERROR, "Cabinet %u's state: %s", k + 1, e.what());
    return nullptr;
  }
  if (size) *size = s_cabinetState.size();
  return s_cabinetState.data();
}

extern "C" bool daytona_cabinet_load(unsigned k, const uint8_t *data, size_t size)
{
  if (!s_loaded || k >= s_cabs.size() || !data) return false;
  Reader r(data, size);
  if (r.u32() != kCabinetMagic || r.u32() != kCabinetVersion)
  {
    Log(RETRO_LOG_ERROR, "Cabinet %u: not a cabinet state of this core.", k + 1);
    return false;
  }
  const uint32_t shimSize = r.u32();
  const uint8_t *shim = r.take(shimSize);
  const uint32_t snapshotSize = r.u32();
  const uint8_t *snapshot = r.take(snapshotSize);
  Cabinet &c = s_cabs[k];
  Cabinet loaded;
  loaded.fm = c.fm;
  loaded.pcm = c.pcm;
  Reader rs(shim, shim ? shimSize : 0);
  try
  {
    if (!shim || !snapshot || !ReadShimState(rs, loaded) || !rt::load_state(*c.game, snapshot, snapshotSize))
    {
      Log(RETRO_LOG_ERROR, "Cabinet %u: the state did not load (the cabinet is as it was).", k + 1);
      return false;
    }
  }
  catch (const std::exception &e)
  {
    Halt("Cabinet state", e.what());
    return false;
  }
  c.seat = loaded.seat;
  c.samples = loaded.samples;
  c.fm = std::move(loaded.fm);
  c.pcm = std::move(loaded.pcm);
  s_outValid = false;
  CheckSeat("Cabinet state");
  return true;
}

extern "C" bool daytona_cabinet_reset(unsigned k)
{
  if (!s_loaded || k >= s_cabs.size()) return false;
  try
  {
    PowerOnCabinet(k);
  }
  catch (const std::exception &e)
  {
    Halt("Cabinet reset", e.what());
    return false;
  }
  return true;
}

// Writes cabinet k's settings EEPROM and backup RAM to <dir>/ioboard_eeprom.bin and
// backup_ram.bin (make-nvram.mjs: the presets). False without a game or on a write error.
extern "C" bool daytona_save_nvram(unsigned cabinet, const char *dir)
{
  if (!s_loaded || cabinet >= s_cabs.size() || !dir) return false;
  std::string path;
  for (const char *p = dir; *p; p++)
  {
    path += *p;
    if (p[1] == '/' || !p[1]) mkdir(path.c_str(), 0755);
  }
  rt::GameLoop &game = *s_cabs[cabinet].game;
  auto save = [&](const char *name, const auto &from) {
    std::ofstream f(path + "/" + name, std::ios::binary);
    f.write(reinterpret_cast<const char *>(from.data()), std::streamsize(from.size()));
    return bool(f);
  };
  return save("ioboard_eeprom.bin", game.board().io().eeprom) && save("backup_ram.bin", game.board().backup_ram());
}

/******************************************************************************
 libretro API
******************************************************************************/

RETRO_API unsigned retro_api_version(void) { return RETRO_API_VERSION; }

RETRO_API void retro_set_environment(retro_environment_t cb)
{
  environ_cb = cb;
  // The board's screen is 0xAARRGGBB words: XRGB8888 as it is.
  enum retro_pixel_format format = RETRO_PIXEL_FORMAT_XRGB8888;
  cb(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &format);
  struct retro_log_callback logging;
  if (cb(RETRO_ENVIRONMENT_GET_LOG_INTERFACE, &logging)) log_cb = logging.log;
}

RETRO_API void retro_set_video_refresh(retro_video_refresh_t cb) { video_cb = cb; }
RETRO_API void retro_set_audio_sample(retro_audio_sample_t cb) { (void)cb; }
RETRO_API void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) { audio_batch_cb = cb; }
RETRO_API void retro_set_input_poll(retro_input_poll_t cb) { input_poll_cb = cb; }
RETRO_API void retro_set_input_state(retro_input_state_t cb) { input_state_cb = cb; }

RETRO_API void retro_init(void) {}

RETRO_API void retro_deinit(void) { retro_unload_game(); }

RETRO_API void retro_get_system_info(struct retro_system_info *info)
{
  static const std::string version = std::string(DAYTONA_RECOMP_COMMIT).substr(0, 7) + (GenIsStub() ? " (stub game code)" : "");
  memset(info, 0, sizeof(*info));
  info->library_name = "Daytona USA (static recompilation)";
  info->library_version = version.c_str();
  info->valid_extensions = "zip";
  info->need_fullpath = true;
  info->block_extract = true;
}

RETRO_API void retro_get_system_av_info(struct retro_system_av_info *info)
{
  memset(info, 0, sizeof(*info));
  info->geometry.base_width = kWidth;
  info->geometry.base_height = kHeight;
  info->geometry.max_width = kWidth;
  info->geometry.max_height = kHeight;
  info->geometry.aspect_ratio = 4.0f / 3.0f;
  info->timing.fps = rt::GameLoop::kFrameHz; // 57.524 Hz: the board's own video timing
  info->timing.sample_rate = kSampleRate;
}

RETRO_API void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port; (void)device; }

RETRO_API bool retro_load_game(const struct retro_game_info *info)
{
  retro_unload_game();
  if (!info || !info->path)
  {
    Log(RETRO_LOG_ERROR, "No ROM set given (the core loads daytona.zip from a path).");
    return false;
  }
  s_count = s_wantCabinets;
  s_star = s_starWanted;
  try
  {
    if (s_blankImages) Log(RETRO_LOG_WARN, "Test: blank ROM images instead of %s.", info->path);
    else
    {
      // Every file of MAME's `daytona` set, checked before anything is loaded.
      bool complete = true;
      for (const rt::RomCheck &check : rt::check_rom_set(info->path))
        if (!check.ok)
        {
          Log(RETRO_LOG_ERROR, "%s: %s: %s", info->path, check.file.c_str(), check.problem.c_str());
          complete = false;
        }
      if (!complete)
      {
        Log(RETRO_LOG_ERROR, "%s is not MAME's %s set (Daytona USA Revision A) as this build needs it.", info->path, rt::rom_set_name());
        retro_unload_game();
        return false;
      }
    }
    s_images = LoadImages(info->path);
    PowerOn();
    s_stateSize = StateSize();
  }
  catch (const std::exception &e)
  {
    Log(RETRO_LOG_ERROR, "%s: %s", info->path, e.what());
    retro_unload_game();
    return false;
  }
  s_loaded = true;

  if (environ_cb)
  {
    std::vector<retro_input_descriptor> descriptors;
    for (unsigned port = 0; port < s_count; port++)
      for (const auto &[id, label] : std::initializer_list<std::pair<unsigned, const char *>>{
             { RETRO_DEVICE_ID_JOYPAD_LEFT, "Steer left" }, { RETRO_DEVICE_ID_JOYPAD_RIGHT, "Steer right" },
             { RETRO_DEVICE_ID_JOYPAD_UP, "Accelerate" }, { RETRO_DEVICE_ID_JOYPAD_DOWN, "Brake" },
             { RETRO_DEVICE_ID_JOYPAD_B, "Shift down" }, { RETRO_DEVICE_ID_JOYPAD_A, "Shift up" },
             { RETRO_DEVICE_ID_JOYPAD_Y, "View 1" }, { RETRO_DEVICE_ID_JOYPAD_X, "View 2" },
             { RETRO_DEVICE_ID_JOYPAD_L, "View 3" }, { RETRO_DEVICE_ID_JOYPAD_R, "View 4" },
             { RETRO_DEVICE_ID_JOYPAD_START, "Start" }, { RETRO_DEVICE_ID_JOYPAD_SELECT, "Coin" } })
        descriptors.push_back({ port, RETRO_DEVICE_JOYPAD, 0, id, label });
    descriptors.push_back({ 0, 0, 0, 0, nullptr });
    environ_cb(RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS, descriptors.data());
  }

  Log(RETRO_LOG_INFO, "Daytona USA (%s, recomp %.7s) loaded: %u cabinet%s%s, 496x384 at %.3f Hz, 48 kHz; save states take %zu bytes.",
      rt::rom_set_name(), DAYTONA_RECOMP_COMMIT, s_count, s_count > 1 ? "s" : "", s_count > 1 ? " linked in memory" : "",
      rt::GameLoop::kFrameHz, s_stateSize);
  if (Bridging())
    Log(RETRO_LOG_INFO, "Arcade mode: seat %u of a star (link_pace %s); load a seat state, then hand over the other seats' blocks "
                        "(daytona_link_in) before each frame and send daytona_link_out's after it.", s_seat, Paced() ? "on" : "off");
  if (GenIsStub())
    Log(RETRO_LOG_WARN, "This build has the stub game code, not the game's (build.sh found no daytona.zip): it stops at "
                        "the first frame. Rebuild with the ROM set (daytona/README.md).");
  return true;
}

RETRO_API bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t num)
{
  (void)type; (void)info; (void)num;
  return false;
}

RETRO_API void retro_unload_game(void)
{
  s_cabs.clear(); // the games before the cables they hold
  for (Wire &w : s_wires) w.Clear();
  s_frame = 0;
  s_images = rt::M2Board::Images{};
  s_loaded = s_halted = false;
  s_count = 0;
  s_stateSize = 0;
}

RETRO_API void retro_reset(void)
{
  if (!s_loaded) return;
  try
  {
    PowerOn();
  }
  catch (const std::exception &e)
  {
    s_cabs.clear(); // no half-built machine: the cabinets are all there or none is
    Halt("Reset", e.what());
  }
}

RETRO_API void retro_run(void)
{
  if (!s_loaded || s_halted || s_cabs.empty()) return;
  const uint64_t start = NowUs();
  if (input_poll_cb) input_poll_cb();
  // What the frontend wants this frame: re-runs after a rollback want neither picture nor sound.
  int enabled = 3;
  if (!environ_cb || !environ_cb(RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE, &enabled)) enabled = 3;
  const bool video = enabled & 1, audio = enabled & 2;
  const unsigned view = View();

  s_audio.clear();
  s_frame++;
  uint64_t mixing = 0;
  try
  {
    for (unsigned i = 0; i < s_count; i++)
    {
      const unsigned k = RunOrder(i);
      Cabinet &c = s_cabs[k];
      rt::GameLoop &game = *c.game;
      if (s_mode[k] == Mode::Cut || s_mode[k] == Mode::Pause) continue;
      if (s_mode[k] == Mode::Ghost)
      {
        // The board is the game's and not const; only the accessor is.
        if (const rt::CommBoard *board = game.board().comm_board()) const_cast<rt::CommBoard *>(board)->vblank();
        continue;
      }
      const rt::Inputs inputs = s_scripts[k].on ? s_scripts[k].script.at(game.board().frame()) : FromPad(c.seat, ReadPad(k));
      game.board().set_draw(video && (k == view || s_drawHidden));
      game.run_frame(inputs);
      // A display list past the polygon list's limit: that frame's 3D was dropped (patches/0004;
      // nothing the game does changes). Said the first time, then every 100th.
      if (const uint64_t dropped = game.board().geo().dropped_frames(); dropped != c.dropped)
      {
        c.dropped = dropped;
        if (dropped == 1 || dropped % 100 == 0)
          Log(RETRO_LOG_WARN, "Cabinet %u: a display list ran past the geometrizer's 32768 polygons: that frame's 3D "
                              "was dropped (%llu so far).", k + 1, (unsigned long long)dropped);
      }
      const rt::FrameProfile &p = game.last_profile();
      s_timings.logic[k] += p.core();
      s_timings.geometry[k] += p.geometry;
      s_timings.raster[k] += p.video;
      s_timings.sound[k] += p.sound;
      const uint64_t mixStart = NowUs();
      MixSound(c, k == view ? &s_audio : nullptr);
      mixing += NowUs() - mixStart;
    }
  }
  catch (const std::exception &e)
  {
    Halt("The game", e.what());
    return;
  }

  if (video && video_cb)
  {
    const rt::GameLoop &game = *s_cabs[view].game;
    const unsigned width = unsigned(game.screen_width());
    if (game.screen().size() >= size_t(width) * kHeight)
      video_cb(game.screen().data(), width, kHeight, width * sizeof(uint32_t));
  }
  if (audio && audio_batch_cb && !s_audio.empty()) audio_batch_cb(s_audio.data(), s_audio.size() / 2);
  s_timings.audio += mixing;
  s_timings.total += NowUs() - start;
  s_timings.frames++;
}

RETRO_API size_t retro_serialize_size(void) { return s_loaded ? s_stateSize : 0; }

RETRO_API bool retro_serialize(void *data, size_t size)
{
  if (!s_loaded || s_halted || s_cabs.empty() || !data) return false;
  uint8_t *const start = static_cast<uint8_t *>(data);
  Writer w(start, size);
  try
  {
    w.u32(kStateMagic);
    w.u32(kStateVersion);
    w.u32(s_count);
    for (Cabinet &c : s_cabs)
    {
      std::vector<uint8_t> shim(ShimStateBound());
      Writer ws(shim.data(), shim.size());
      if (!WriteShimState(ws, c))
      {
        Log(RETRO_LOG_ERROR, "Save state: the cabinet's sound carry is too long.");
        return false;
      }
      w.u32(uint32_t(ws.used(shim.data())));
      w.bytes(shim.data(), ws.used(shim.data()));
      const std::vector<uint8_t> snapshot = rt::save_state(*c.game);
      if (snapshot.empty())
      {
        Log(RETRO_LOG_ERROR, "Save state: the runtime made none%s.",
            rt::state_size_bound(*c.game) ? "" : " (this build has no save states: patches/0001-snapshot.patch)");
        return false;
      }
      w.u32(uint32_t(snapshot.size()));
      w.bytes(snapshot.data(), snapshot.size());
    }
    if (s_count > 1)
      for (unsigned k = 0; k < s_count; k++)
      {
        const std::vector<uint8_t> bytes = s_wires[k].Bytes();
        if (bytes.size() > kWireCap)
        {
          Log(RETRO_LOG_ERROR, "Save state: a cable holds more than 32 link frames (link_delay?): not saved.");
          return false;
        }
        w.u32(uint32_t(bytes.size()));
        w.bytes(bytes.data(), bytes.size());
      }
    if (Bridging()) WriteBridge(w);
  }
  catch (const std::exception &e)
  {
    Log(RETRO_LOG_ERROR, "Save state: %s", e.what());
    return false;
  }
  if (!w.ok())
  {
    Log(RETRO_LOG_ERROR, "Save state: more than the %zu bytes given.", size);
    return false;
  }
  memset(start + w.used(start), 0, size - w.used(start));
  return true;
}

RETRO_API bool retro_unserialize(const void *data, size_t size)
{
  if (!s_loaded || !data) return false;
  if (s_cabs.empty())
  {
    Log(RETRO_LOG_ERROR, "Load state: the machine did not power on (above); load the game again.");
    return false;
  }
  Reader r(static_cast<const uint8_t *>(data), size);
  if (r.u32() != kStateMagic || r.u32() != kStateVersion)
  {
    Log(RETRO_LOG_ERROR, "Load state: not a state of this core (or of an older version of it).");
    return false;
  }
  const uint32_t count = r.u32();
  if (count != s_count)
  {
    Log(RETRO_LOG_ERROR, "Load state: a state of %u cabinets, this machine has %u.", count, s_count);
    return false;
  }
  try
  {
    for (Cabinet &c : s_cabs)
    {
      const uint32_t shimSize = r.u32();
      const uint8_t *shim = r.take(shimSize);
      Reader rs(shim, shim ? shimSize : 0);
      const uint32_t snapshotSize = r.u32();
      const uint8_t *snapshot = r.take(snapshotSize);
      if (!shim || !snapshot || !ReadShimState(rs, c) || !rt::load_state(*c.game, snapshot, snapshotSize))
      {
        Log(RETRO_LOG_ERROR, "Load state: a cabinet's state did not load%s.",
            rt::state_size_bound(*c.game) ? "" : " (this build has no save states: patches/0001-snapshot.patch)");
        s_halted = true; // a cabinet may be half loaded: run nothing until reset or a good state
        return false;
      }
    }
    if (s_count > 1)
      for (unsigned k = 0; k < s_count; k++)
      {
        const uint32_t n = r.u32();
        const uint8_t *bytes = r.take(n);
        if (!bytes || n > kWireCap)
        {
          Log(RETRO_LOG_ERROR, "Load state: the link cables are cut short.");
          s_halted = true;
          return false;
        }
        s_wires[k].Assign(bytes, n);
      }
    if (Bridging())
    {
      // The blocks the cabinet was seeing, when the state has them (a bridge's); else none.
      Reader peek = r;
      BridgeState table;
      unsigned seat = s_seat;
      if (peek.u32() != kBridgeMagic)
        Log(RETRO_LOG_INFO, "Load state: no star blocks in it (not a bridge's state): every other seat empty until handed over.");
      else if (!ReadBridge(r, table, seat))
      {
        Log(RETRO_LOG_ERROR, "Load state: the star's blocks are cut short.");
        s_halted = true;
        return false;
      }
      else if (seat != s_seat)
        Log(RETRO_LOG_WARN, "Load state: saved as seat %u, this machine is seat %u (the seat option stays).", seat, s_seat);
      s_bridge = table;
      s_outValid = false;
    }
  }
  catch (const std::exception &e)
  {
    Halt("Load state", e.what());
    return false;
  }
  s_halted = false;
  CheckSeat("Load state");
  return true;
}

RETRO_API void retro_cheat_reset(void) {}
RETRO_API void retro_cheat_set(unsigned index, bool enabled, const char *code) { (void)index; (void)enabled; (void)code; }
RETRO_API unsigned retro_get_region(void) { return RETRO_REGION_NTSC; }

// Cabinet 0's i960 main RAM: the gameplay-relevant memory, which the frontend hashes to find
// machines that drifted apart (a linked machine's cabinets move together, so one is enough).
RETRO_API void *retro_get_memory_data(unsigned id)
{
  if (id != RETRO_MEMORY_SYSTEM_RAM || !s_loaded || s_cabs.empty()) return nullptr;
  try
  {
    return rt::main_ram(s_cabs[0].game->board(), nullptr);
  }
  catch (const std::exception &e)
  {
    Log(RETRO_LOG_ERROR, "Main RAM: %s", e.what());
    return nullptr;
  }
}

RETRO_API size_t retro_get_memory_size(unsigned id)
{
  if (id != RETRO_MEMORY_SYSTEM_RAM || !s_loaded || s_cabs.empty()) return 0;
  size_t size = 0;
  try
  {
    rt::main_ram(s_cabs[0].game->board(), &size);
  }
  catch (const std::exception &)
  {
    return 0;
  }
  return size;
}
