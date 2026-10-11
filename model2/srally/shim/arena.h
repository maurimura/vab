/* The recomp's heap, in two arenas at fixed addresses (static arrays), so that a machine's
 * pointers are the same in every instance of the same build and a save state is a copy of
 * memory (machine.h):
 *
 *   ROM arena   the ROM images, allocated while loading (srally_alloc_set_rom(1)); never
 *               written after the load, never in a save state.
 *   heap arena  everything the recomp allocates after that (geometry RAM, mesh and draw
 *               buffers, FIFO upload buffers); in the save state up to its top.
 *
 * The recomp's C files are compiled with -include srally_alloc.h, which turns malloc, calloc,
 * realloc and free into srally_*. Memory handed out is always zeroed (what a program reads
 * from memory it never wrote is then the same everywhere), and a block is first fit from an
 * address-ordered free list, so the same calls give the same addresses on every machine. */
#ifndef SRALLY_ARENA_H
#define SRALLY_ARENA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct srally_arena
{
  uint8_t *base;
  size_t size;      /* bytes in the arena */
  size_t top;       /* bytes from base in use or on the free list; above it, all free */
  size_t peak;      /* the highest top so far (not machine state) */
  uint32_t free_head; /* offset of the first free block (address order), 0 = none */
  uint32_t last;      /* offset of the block just below top, 0 = none */
  uint32_t blocks;  /* blocks in use */
  size_t used;      /* bytes in use, headers included */
} srally_arena_t;

void srally_arena_init(srally_arena_t *a, void *mem, size_t size);
void *srally_arena_alloc(srally_arena_t *a, size_t n);
void srally_arena_free(srally_arena_t *a, void *p);
void *srally_arena_realloc(srally_arena_t *a, void *p, size_t n);
int srally_arena_owns(const srally_arena_t *a, const void *p);
/* Walks the blocks: 0 when consistent, else -1 (with what is wrong in `why`). */
int srally_arena_check(const srally_arena_t *a, const char **why);

/* The two arenas (arena.c). */
extern srally_arena_t srally_rom_arena;   /* not machine state */
extern srally_arena_t *srally_heap_arena; /* inside the machine (machine.h) */
void srally_alloc_set_rom(int on);         /* route new allocations to the ROM arena */
uint8_t *srally_rom_arena_memory(size_t *size);
uint8_t *srally_heap_arena_memory(size_t *size);
/* Heap allocations that failed (out of arena) since the load: the game is in trouble. */
extern unsigned srally_alloc_failures;

void *srally_malloc(size_t n);
void *srally_calloc(size_t n, size_t size);
void *srally_realloc(void *p, size_t n);
void srally_free(void *p);

#ifdef __cplusplus
}
#endif

#endif
