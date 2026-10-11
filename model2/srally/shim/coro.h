/* The game runs on a coroutine of its own: it calls coro_yield() at the end of every frame
 * (the frame_end host op, inside its vblank wait) and retro_run's coro_resume() runs it to the
 * next one. Its context and stack are in the machine (machine.h), so a save state taken while
 * it is parked holds where it is parked.
 *   Emscripten: emscripten_fiber_* (Asyncify: the game's call stack is unwound into
 *   srally_machine.asyncify on a yield and rewound on a resume).
 *   Native: ucontext (makecontext/swapcontext), the same stack. */
#ifndef SRALLY_CORO_H
#define SRALLY_CORO_H

#ifdef __cplusplus
extern "C" {
#endif

/* Sets up the game's context to start at entry() on the first resume. */
void coro_init(void (*entry)(void));
/* Runs the game until it yields (main side only; in wasm only from an export that returns
 * nothing, as the unwound export's return value is lost). */
void coro_resume(void);
/* Back to the main side (game side only). */
void coro_yield(void);
/* Nonzero while the game's side runs. */
int coro_in_game(void);
/* After the machine was overwritten by a save state (nothing to do at present: the one thing
 * of the parked context that belongs to the instance, the Asyncify rewind id, is set before
 * every resume). */
void coro_after_restore(void);
/* The deepest the game's stack has gone, in bytes (scans for the fill pattern). */
unsigned coro_stack_used(void);

#ifdef __cplusplus
}
#endif

#endif
