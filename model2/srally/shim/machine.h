/* The machine: everything a save state holds. The recomp keeps its whole state in static
 * variables (registers g0..g15 and r0..r15 are globals, RAM windows are static arrays, the
 * lifted code keeps host pointers in registers) plus what it allocates; the game runs on its
 * own stack (a fiber, coro.h) and stops at every frame's vblank. So the machine is
 *
 *   1. the static data of the recomp's objects and machine.c, which the link puts between
 *      state_begin.c and state_end.c (two ranges: initialised data and zeroed data), less the
 *      maincpu ROM image (a static array, constant once loaded);
 *   2. the heap arena below its top (arena.h);
 *
 * and a save state is a copy of those bytes. That works because a build puts its statics,
 * the arenas and the fiber's stack at the same addresses in every instance, so the pointers
 * inside the copy are right wherever it is loaded (state.c checks the layout at load).
 * The shim's own globals (callbacks, options, timings) are linked outside the markers and
 * stay out of it. */
#ifndef SRALLY_MACHINE_H
#define SRALLY_MACHINE_H

#include "arena.h"

#include <stdint.h>

#ifdef __EMSCRIPTEN__
#include <emscripten/fiber.h>
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* The game's C stack (the deepest seen is in README.md "Measured") and, in wasm, the buffer
 * Asyncify unwinds the game's call stack into when it yields. */
#ifndef SRALLY_FIBER_STACK
#define SRALLY_FIBER_STACK (512u << 10)
#endif
#define SRALLY_ASYNCIFY_STACK (128u << 10)

/* The cabinet's controls as the shim drives them from the RetroPad (libretro.c). */
typedef struct srally_seat
{
  int32_t steer;   /* from centre, -0x60 (full left) .. +0x60 */
  int32_t gear;    /* 1-4 on the H-shifter, shifted sequentially */
  uint32_t held;   /* last frame's RetroPad mask */
  int32_t turning; /* frames LEFT (< 0) or RIGHT (> 0) has been held */
} srally_seat_t;

typedef struct srally_machine
{
  uint32_t magic;
  uint32_t halted;       /* the game's main loop ended: nothing runs any more */
  uint32_t was_opaque2d; /* last frame was a 2D-only screen (geometry cleared on entry) */
  uint64_t frames;       /* frames since power-on */
  uint64_t samples;      /* 44.1 kHz stereo samples made since power-on */
  uint64_t sample_carry; /* the fraction of a sample, in 1/16,000,000 */
  srally_seat_t seat;
  srally_arena_t heap;   /* the heap arena's books (its memory: arena.c) */
#ifdef __EMSCRIPTEN__
  emscripten_fiber_t fiber;
  uint8_t asyncify[SRALLY_ASYNCIFY_STACK] __attribute__((aligned(16)));
#else
  /* A ucontext_t (coro.c; its size depends on _XOPEN_SOURCE, so it is opaque here). */
  uint8_t context[2048] __attribute__((aligned(16)));
#endif
  uint8_t stack[SRALLY_FIBER_STACK] __attribute__((aligned(16)));
} srally_machine_t;

extern srally_machine_t srally_machine;

/* Markers (state_begin.c, state_end.c): the machine's static data lies between them. */
extern char srally_state_data_begin[], srally_state_data_end[];
extern char srally_state_bss_begin[], srally_state_bss_end[];
char *srally_state_static_bss_begin(void);
char *srally_state_static_bss_end(void);

#ifdef __cplusplus
}
#endif

#endif
