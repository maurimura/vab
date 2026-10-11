/* zip.h: enough of the zip format for a MAME ROM set (no zip64, no encryption). */
#include "zip.h"

#include <string.h>
#include <zlib.h>

static uint32_t u16le(const uint8_t *p) { return (uint32_t)p[0] | ((uint32_t)p[1] << 8); }
static uint32_t u32le(const uint8_t *p) { return u16le(p) | (u16le(p + 2) << 16); }

int srally_zip_list(const uint8_t *zip, size_t size, srally_zip_entry_t *entries, int max)
{
  size_t eocd, at, end;
  uint32_t count, dir_size, dir_offset, i;
  int n = 0;

  if (!zip || size < 22) return -1;
  /* The end of central directory record, within the last 64 KB + 22 bytes (a comment). */
  for (eocd = size - 22;; eocd--)
  {
    if (u32le(zip + eocd) == 0x06054b50u) break;
    if (eocd == 0 || size - eocd > 65535 + 22) return -1;
  }
  count = u16le(zip + eocd + 10);
  dir_size = u32le(zip + eocd + 12);
  dir_offset = u32le(zip + eocd + 16);
  if ((size_t)dir_offset + dir_size > size) return -1;
  at = dir_offset;
  end = (size_t)dir_offset + dir_size;
  for (i = 0; i < count; i++)
  {
    uint32_t name_len, extra_len, comment_len;
    const char *name, *slash;
    if (at + 46 > end || u32le(zip + at) != 0x02014b50u) return -1;
    name_len = u16le(zip + at + 28);
    extra_len = u16le(zip + at + 30);
    comment_len = u16le(zip + at + 32);
    if (at + 46 + name_len > end) return -1;
    name = (const char *)zip + at + 46;
    if (n < max && name_len > 0 && name[name_len - 1] != '/')
    {
      srally_zip_entry_t *e = &entries[n];
      size_t len;
      memset(e, 0, sizeof(*e));
      e->method = (uint16_t)u16le(zip + at + 10);
      e->crc = u32le(zip + at + 16);
      e->csize = u32le(zip + at + 20);
      e->size = u32le(zip + at + 24);
      e->local_offset = u32le(zip + at + 42);
      /* Only the file name: a set zipped with a folder inside works too. */
      slash = name;
      for (len = 0; len < name_len; len++)
        if (name[len] == '/' || name[len] == '\\') slash = name + len + 1;
      len = name_len - (size_t)(slash - name);
      if (len >= sizeof(e->name)) len = sizeof(e->name) - 1;
      memcpy(e->name, slash, len);
      n++;
    }
    at += 46 + name_len + extra_len + comment_len;
  }
  return n;
}

int srally_zip_read(const uint8_t *zip, size_t size, const srally_zip_entry_t *e, uint8_t *out, const char **why)
{
  const uint8_t *data;
  size_t at = e->local_offset;
  const char *dummy;

  if (!why) why = &dummy;
  if (at + 30 > size || u32le(zip + at) != 0x04034b50u) { *why = "bad local header"; return -1; }
  at += 30 + u16le(zip + at + 26) + u16le(zip + at + 28);
  if (at + e->csize > size) { *why = "truncated"; return -1; }
  data = zip + at;
  if (e->method == 0)
  {
    if (e->csize != e->size) { *why = "bad stored size"; return -1; }
    memcpy(out, data, e->size);
  }
  else if (e->method == 8)
  {
    z_stream z;
    int rc;
    memset(&z, 0, sizeof(z));
    if (inflateInit2(&z, -MAX_WBITS) != Z_OK) { *why = "inflateInit2 failed"; return -1; }
    z.next_in = (Bytef *)data;
    z.avail_in = e->csize;
    z.next_out = out;
    z.avail_out = e->size;
    rc = inflate(&z, Z_FINISH);
    inflateEnd(&z);
    if (rc != Z_STREAM_END || z.total_out != e->size) { *why = "inflate failed"; return -1; }
  }
  else { *why = "unsupported compression method"; return -1; }
  if ((uint32_t)crc32(0L, out, e->size) != e->crc) { *why = "CRC mismatch"; return -1; }
  return 0;
}
