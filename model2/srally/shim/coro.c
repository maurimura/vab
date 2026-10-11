/* coro.h: the game's coroutine. */
#if !defined(__EMSCRIPTEN__) && !defined(_XOPEN_SOURCE)
#define _XOPEN_SOURCE 700 /* the ucontext routines (deprecated on macOS, still there) */
#endif
#ifdef __APPLE__
#define _DARWIN_C_SOURCE 1
#endif

#include "coro.h"
#include "machine.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef __EMSCRIPTEN__
#include <emscripten/fiber.h>
#else
#include <ucontext.h>
#endif

static void (*s_entry)(void);
static int s_in_game;
#define STACK_FILL 0xa5
#define STACK_TOP_KEEP 4096 /* native: the top of the stack is not filled (coro_init) */

#ifdef __EMSCRIPTEN__
static emscripten_fiber_t s_main;
static unsigned char s_main_asyncify[64 << 10] __attribute__((aligned(16)));
static int s_main_ready;
/* A fiber resumes by rewinding the call stack Asyncify unwound into its buffer, from the
 * function at the bottom of it, which the buffer names by an id Asyncify gives functions in
 * the order an instance first sees them. Every fiber's bottom function is the same, the
 * dynCall that entered it, but its id depends on the instance's history, and a state carries
 * the saving instance's: so each instance learns its own id once, from a probe fiber that
 * yields straight back, and the game's fiber gets it before every resume. */
static int s_rewind_id = -1;
static emscripten_fiber_t s_probe;
static unsigned char s_probe_stack[16 << 10] __attribute__((aligned(16)));
static unsigned char s_probe_asyncify[4 << 10] __attribute__((aligned(16)));

static void trampoline(void *arg)
{
  (void)arg;
  s_entry();
  /* The entry never returns (libretro.c), but a fiber must not. */
  for (;;) coro_yield();
}

static void probe(void *arg)
{
  (void)arg;
  for (;;) emscripten_fiber_swap(&s_probe, &s_main);
}

void coro_init(void (*entry)(void))
{
  s_entry = entry;
  memset(srally_machine.stack, STACK_FILL, sizeof(srally_machine.stack));
  emscripten_fiber_init(&srally_machine.fiber, trampoline, NULL, srally_machine.stack, sizeof(srally_machine.stack),
                        srally_machine.asyncify, sizeof(srally_machine.asyncify));
  if (!s_main_ready)
  {
    emscripten_fiber_init_from_current_context(&s_main, s_main_asyncify, sizeof(s_main_asyncify));
    s_main_ready = 1;
  }
}

/* Only from an export that returns nothing (retro_run): a swap unwinds the export, and what it
 * returns is lost. */
void coro_resume(void)
{
  if (s_rewind_id < 0)
  {
    emscripten_fiber_init(&s_probe, probe, NULL, s_probe_stack, sizeof(s_probe_stack), s_probe_asyncify, sizeof(s_probe_asyncify));
    emscripten_fiber_swap(&s_main, &s_probe);
    s_rewind_id = s_probe.asyncify_data.rewind_id;
  }
  if (!srally_machine.fiber.entry) srally_machine.fiber.asyncify_data.rewind_id = s_rewind_id;
  s_in_game = 1;
  emscripten_fiber_swap(&s_main, &srally_machine.fiber);
  s_in_game = 0;
}

void coro_yield(void)
{
  emscripten_fiber_swap(&srally_machine.fiber, &s_main);
}

void coro_after_restore(void) {}

#else

static ucontext_t s_main;
#define CONTEXT ((ucontext_t *)srally_machine.context)
_Static_assert(sizeof(ucontext_t) <= sizeof(srally_machine.context), "machine.h: the context buffer is too small");

static void trampoline(void)
{
  s_entry();
  for (;;) coro_yield();
}

void coro_init(void (*entry)(void))
{
  s_entry = entry;
  memset(srally_machine.context, 0, sizeof(srally_machine.context));
  if (getcontext(CONTEXT) != 0)
  {
    perror("srally: getcontext");
    abort();
  }
  CONTEXT->uc_stack.ss_sp = srally_machine.stack;
  CONTEXT->uc_stack.ss_size = sizeof(srally_machine.stack);
  CONTEXT->uc_link = NULL;
  makecontext(CONTEXT, trampoline, 0);
  /* macOS's makecontext clears the whole stack: the fill goes after it, short of the top,
   * where the first frame is set up. */
  memset(srally_machine.stack, STACK_FILL, sizeof(srally_machine.stack) - STACK_TOP_KEEP);
}

void coro_resume(void)
{
  s_in_game = 1;
  swapcontext(&s_main, CONTEXT);
  s_in_game = 0;
}

void coro_yield(void)
{
  swapcontext(CONTEXT, &s_main);
}

void coro_after_restore(void) {}

#endif

int coro_in_game(void) { return s_in_game; }

unsigned coro_stack_used(void)
{
  unsigned i = 0;
  while (i < sizeof(srally_machine.stack) && srally_machine.stack[i] == STACK_FILL) i++;
  return (unsigned)sizeof(srally_machine.stack) - i;
}
