/* Force-included (-include) in every file of the recomp: its heap goes to the arenas
 * (arena.h), at addresses that are the same in every instance of a build. */
#ifndef SRALLY_ALLOC_H
#define SRALLY_ALLOC_H

#include <stdlib.h>
#include <string.h>

void *srally_malloc(size_t n);
void *srally_calloc(size_t n, size_t size);
void *srally_realloc(void *p, size_t n);
void srally_free(void *p);

#define malloc(n) srally_malloc(n)
#define calloc(n, size) srally_calloc(n, size)
#define realloc(p, n) srally_realloc(p, n)
#define free(p) srally_free(p)

#endif
