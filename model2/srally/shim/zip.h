/* A zip archive in memory: its central directory, and inflating an entry (zlib). */
#ifndef SRALLY_ZIP_H
#define SRALLY_ZIP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct srally_zip_entry
{
  char name[128];        /* without any directory part */
  uint32_t crc;
  uint32_t size;         /* uncompressed */
  uint32_t csize;        /* compressed */
  uint16_t method;       /* 0 stored, 8 deflated */
  uint32_t local_offset; /* the local header */
} srally_zip_entry_t;

/* Reads the central directory: the number of entries (up to max), -1 when not a zip. */
int srally_zip_list(const uint8_t *zip, size_t size, srally_zip_entry_t *entries, int max);
/* Inflates an entry into out (entry->size bytes) and checks its CRC: 0, or -1 with why. */
int srally_zip_read(const uint8_t *zip, size_t size, const srally_zip_entry_t *entry, uint8_t *out, const char **why);

#ifdef __cplusplus
}
#endif

#endif
