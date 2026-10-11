/* Two arenas for the recomp's heap (arena.h): a first-fit allocator over a fixed block of
 * memory, everything it knows inside srally_arena_t and the block headers, so that copying
 * the struct and the memory below `top` copies the heap. */
#include "arena.h"
#include "machine.h"

#include <stdio.h>
#include <string.h>

/* Block header; the payload follows it, 16-byte aligned. Blocks tile [START, top). A free
 * block keeps the offsets of its neighbours on the free list (address order) at the start of
 * its payload. Two free blocks are never adjacent, and the block below top is never free. */
typedef struct
{
  uint32_t size; /* the whole block, header included, a multiple of 16 */
  uint32_t prev; /* size of the block just below, 0 for the first */
  uint32_t used; /* USED or FREE */
  uint32_t pad;
} hdr_t;

typedef struct
{
  uint32_t next, prev; /* offsets of free blocks, 0 = none */
} links_t;

enum { HDR = 16, START = 16, MIN_BLOCK = 32 };
#define USED 0x55534544u /* "USED" */
#define FREE 0x46524545u /* "FREE" */

#define AT(a, off) ((hdr_t *)((a)->base + (off)))
#define OFF(a, h) ((uint32_t)((uint8_t *)(h) - (a)->base))
#define LINKS(h) ((links_t *)((uint8_t *)(h) + HDR))

static size_t round16(size_t n) { return (n + 15u) & ~(size_t)15u; }

void srally_arena_init(srally_arena_t *a, void *mem, size_t size)
{
  memset(a, 0, sizeof(*a));
  a->base = (uint8_t *)mem;
  a->size = size & ~(size_t)15u;
  a->top = START;
  a->peak = START;
}

int srally_arena_owns(const srally_arena_t *a, const void *p)
{
  const uint8_t *q = (const uint8_t *)p;
  return a && a->base && q >= a->base + START + HDR && q < a->base + a->top;
}

static void list_remove(srally_arena_t *a, hdr_t *h)
{
  links_t *l = LINKS(h);
  if (l->prev) LINKS(AT(a, l->prev))->next = l->next;
  else a->free_head = l->next;
  if (l->next) LINKS(AT(a, l->next))->prev = l->prev;
}

/* Into the free list in address order. */
static void list_insert(srally_arena_t *a, hdr_t *h)
{
  const uint32_t off = OFF(a, h);
  uint32_t prev = 0, next = a->free_head;
  while (next && next < off)
  {
    prev = next;
    next = LINKS(AT(a, next))->next;
  }
  LINKS(h)->prev = prev;
  LINKS(h)->next = next;
  if (prev) LINKS(AT(a, prev))->next = off;
  else a->free_head = off;
  if (next) LINKS(AT(a, next))->prev = off;
}

/* The block just above h, NULL when h is the last. */
static hdr_t *above(srally_arena_t *a, hdr_t *h)
{
  const size_t off = (size_t)OFF(a, h) + h->size;
  return off < a->top ? AT(a, off) : NULL;
}

static hdr_t *below(srally_arena_t *a, hdr_t *h)
{
  return h->prev ? AT(a, OFF(a, h) - h->prev) : NULL;
}

static void *use(srally_arena_t *a, hdr_t *h)
{
  h->used = USED;
  a->blocks++;
  a->used += h->size;
  memset((uint8_t *)h + HDR, 0, h->size - HDR);
  return (uint8_t *)h + HDR;
}

void *srally_arena_alloc(srally_arena_t *a, size_t n)
{
  size_t need;
  uint32_t off;
  hdr_t *h;

  if (!a->base) return NULL;
  need = round16(n ? n : 1) + HDR;
  if (need < MIN_BLOCK) need = MIN_BLOCK;
  if (need > 0x7fff0000u) return NULL;
  /* First fit, lowest address first. */
  for (off = a->free_head; off; off = LINKS(AT(a, off))->next)
  {
    h = AT(a, off);
    if (h->size < need) continue;
    list_remove(a, h);
    if (h->size - need >= MIN_BLOCK)
    {
      /* Split: the rest stays free, where h was on the list. */
      hdr_t *rest = (hdr_t *)((uint8_t *)h + need), *up;
      rest->size = h->size - (uint32_t)need;
      rest->prev = (uint32_t)need;
      rest->used = FREE;
      rest->pad = 0;
      h->size = (uint32_t)need;
      if ((up = above(a, rest)) != NULL) up->prev = rest->size;
      list_insert(a, rest);
    }
    return use(a, h);
  }
  /* From the top. */
  if (a->top + need > a->size) return NULL;
  h = AT(a, a->top);
  h->size = (uint32_t)need;
  h->prev = a->last ? AT(a, a->last)->size : 0;
  h->pad = 0;
  a->last = (uint32_t)a->top;
  a->top += need;
  if (a->top > a->peak) a->peak = a->top;
  return use(a, h);
}

void srally_arena_free(srally_arena_t *a, void *p)
{
  hdr_t *h, *up, *down;

  if (!p) return;
  h = (hdr_t *)((uint8_t *)p - HDR);
  if (h->used != USED) return; /* not a block in use: leave it */
  h->used = FREE;
  a->blocks--;
  a->used -= h->size;
  /* Merge with a free block above. */
  if ((up = above(a, h)) != NULL && up->used == FREE)
  {
    hdr_t *upper;
    list_remove(a, up);
    h->size += up->size;
    if ((upper = above(a, h)) != NULL) upper->prev = h->size;
  }
  /* And below: that one is on the list already. */
  if ((down = below(a, h)) != NULL && down->used == FREE)
  {
    hdr_t *upper;
    list_remove(a, down);
    down->size += h->size;
    h = down;
    if ((upper = above(a, h)) != NULL) upper->prev = h->size;
  }
  if (OFF(a, h) + (size_t)h->size == a->top)
  {
    /* The last block: give it back to the top (the block below it is in use). */
    a->top = OFF(a, h);
    a->last = h->prev ? OFF(a, h) - h->prev : 0;
    return;
  }
  list_insert(a, h);
}

void *srally_arena_realloc(srally_arena_t *a, void *p, size_t n)
{
  hdr_t *h;
  size_t need, have;
  void *q;

  if (!p) return srally_arena_alloc(a, n);
  h = (hdr_t *)((uint8_t *)p - HDR);
  if (h->used != USED) return NULL;
  need = round16(n ? n : 1) + HDR;
  if (need < MIN_BLOCK) need = MIN_BLOCK;
  if (need <= h->size) return p;
  have = h->size;
  /* The last block grows into the top. */
  if (OFF(a, h) == a->last && OFF(a, h) + need <= a->size)
  {
    memset((uint8_t *)h + have, 0, need - have);
    a->used += need - have;
    h->size = (uint32_t)need;
    a->top = OFF(a, h) + need;
    if (a->top > a->peak) a->peak = a->top;
    return p;
  }
  q = srally_arena_alloc(a, n);
  if (!q) return NULL;
  memcpy(q, p, have - HDR);
  srally_arena_free(a, p);
  return q;
}

int srally_arena_check(const srally_arena_t *a, const char **why)
{
  size_t off = START, blocks = 0, used = 0, free_blocks = 0, listed = 0;
  uint32_t prev_size = 0, last = 0, f, prev_free = 0;
  int prev_was_free = 0;
  const char *dummy;

  if (!why) why = &dummy;
  *why = "";
  while (off < a->top)
  {
    const hdr_t *h = (const hdr_t *)(a->base + off);
    if (h->size < MIN_BLOCK || (h->size & 15u) || off + h->size > a->top) { *why = "bad block size"; return -1; }
    if (h->prev != prev_size) { *why = "bad prev size"; return -1; }
    if (h->used == USED) { blocks++; used += h->size; prev_was_free = 0; }
    else if (h->used == FREE)
    {
      if (prev_was_free) { *why = "two free blocks side by side"; return -1; }
      free_blocks++;
      prev_was_free = 1;
    }
    else { *why = "bad block mark"; return -1; }
    prev_size = h->size;
    last = (uint32_t)off;
    off += h->size;
  }
  if (off != a->top) { *why = "blocks do not end at top"; return -1; }
  if (last != a->last) { *why = "bad last block"; return -1; }
  if (prev_was_free) { *why = "free block below top"; return -1; }
  if (blocks != a->blocks || used != a->used) { *why = "bad counts"; return -1; }
  for (f = a->free_head; f; f = ((const links_t *)(a->base + f + HDR))->next)
  {
    const hdr_t *h = (const hdr_t *)(a->base + f);
    if (f >= a->top || h->used != FREE) { *why = "bad free list entry"; return -1; }
    if (f <= prev_free && prev_free) { *why = "free list out of order"; return -1; }
    if (((const links_t *)(a->base + f + HDR))->prev != prev_free) { *why = "bad free list back link"; return -1; }
    prev_free = f;
    if (++listed > free_blocks) { *why = "free list longer than the free blocks"; return -1; }
  }
  if (listed != free_blocks) { *why = "free blocks missing from the list"; return -1; }
  return 0;
}

/* The ROM arena (not machine state) and the heap arena's memory (the struct is the
 * machine's): static, so at the same addresses in every instance of a build. */
#ifndef SRALLY_ROM_ARENA_MB
#define SRALLY_ROM_ARENA_MB 48
#endif
#ifndef SRALLY_HEAP_ARENA_MB
#define SRALLY_HEAP_ARENA_MB 16
#endif
static uint8_t s_rom_memory[(size_t)SRALLY_ROM_ARENA_MB << 20] __attribute__((aligned(64)));
static uint8_t s_heap_memory[(size_t)SRALLY_HEAP_ARENA_MB << 20] __attribute__((aligned(64)));
srally_arena_t srally_rom_arena;
srally_arena_t *srally_heap_arena = &srally_machine.heap;
unsigned srally_alloc_failures;
static int s_rom_mode;

uint8_t *srally_rom_arena_memory(size_t *size)
{
  if (size) *size = sizeof(s_rom_memory);
  return s_rom_memory;
}

uint8_t *srally_heap_arena_memory(size_t *size)
{
  if (size) *size = sizeof(s_heap_memory);
  return s_heap_memory;
}

void srally_alloc_set_rom(int on) { s_rom_mode = on; }

static srally_arena_t *current(void) { return s_rom_mode ? &srally_rom_arena : srally_heap_arena; }

static void failed(size_t n)
{
  if (++srally_alloc_failures <= 5)
    fprintf(stderr, "srally: out of %s arena memory (%zu bytes wanted, %zu in use)\n",
            s_rom_mode ? "ROM" : "heap", n, current()->used);
}

void *srally_malloc(size_t n)
{
  void *p = srally_arena_alloc(current(), n);
  if (!p) failed(n);
  return p;
}

void *srally_calloc(size_t n, size_t size)
{
  void *p;
  if (size && n > (size_t)-1 / size) return NULL;
  p = srally_arena_alloc(current(), n * size);
  if (!p) failed(n * size);
  return p;
}

void *srally_realloc(void *p, size_t n)
{
  void *q;
  if (p && srally_arena_owns(&srally_rom_arena, p))
  {
    q = srally_arena_realloc(&srally_rom_arena, p, n);
    if (!q) failed(n);
    return q;
  }
  if (p && !srally_arena_owns(srally_heap_arena, p))
  {
    fprintf(stderr, "srally: realloc of a pointer outside the arenas (%p)\n", p);
    return NULL;
  }
  q = srally_arena_realloc(p ? srally_heap_arena : current(), p, n);
  if (!q) failed(n);
  return q;
}

void srally_free(void *p)
{
  static unsigned stray;
  if (!p) return;
  if (srally_arena_owns(srally_heap_arena, p)) srally_arena_free(srally_heap_arena, p);
  else if (srally_arena_owns(&srally_rom_arena, p)) srally_arena_free(&srally_rom_arena, p);
  else if (++stray <= 5) fprintf(stderr, "srally: free of a pointer outside the arenas (%p)\n", p);
}
