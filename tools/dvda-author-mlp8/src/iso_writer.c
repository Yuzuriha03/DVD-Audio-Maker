#include "iso_writer.h"

#ifdef _WIN32
#include <windows.h>
#else
#error "The in-process ISO writer currently targets Windows builds."
#endif

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#define ISO_SECTOR 2048U
#define UDF_PARTITION_START 257U
#define UDF_DIRECTORY_START 259U
#define UDF_MAIN_VDS 32U
#define UDF_RESERVE_VDS 48U
#define UDF_INTEGRITY_LBA 64U
#define UDF_ANCHOR_LBA 256U

static uint16_t udf_pending_tag_id;
static uint32_t udf_pending_tag_location;
static uint16_t udf_pending_crc_length;

typedef struct IsoNode IsoNode;

struct IsoNode {
  char *name;
  char *path;
  int file;
  uint32_t lba;
  uint32_t size;
  uint32_t udf_directory_lba;
  uint32_t udf_directory_length;
  uint32_t udf_directory_size;
  uint32_t udf_file_entry_lba;
  uint16_t path_index;
  uint16_t parent_path_index;
  IsoNode *parent;
  IsoNode **children;
  size_t child_count;
  size_t child_capacity;
};

static uint32_t sector_count(uint32_t size) {
  return size / ISO_SECTOR + (size % ISO_SECTOR != 0U);
}

static char *duplicate_text(const char *text) {
  size_t length = strlen(text) + 1U;
  char *copy = (char *)malloc(length);
  if (copy) memcpy(copy, text, length);
  return copy;
}

static void free_tree(IsoNode *node) {
  if (!node) return;
  for (size_t i = 0; i < node->child_count; ++i) free_tree(node->children[i]);
  free(node->children);
  free(node->name);
  free(node->path);
  free(node);
}

static int append_child(IsoNode *parent, IsoNode *child) {
  if (parent->child_count == parent->child_capacity) {
    size_t capacity = parent->child_capacity ? parent->child_capacity * 2U : 16U;
    IsoNode **items = (IsoNode **)realloc(parent->children, capacity * sizeof(*items));
    if (!items) return -1;
    parent->children = items;
    parent->child_capacity = capacity;
  }
  parent->children[parent->child_count++] = child;
  return 0;
}

static int compare_nodes(const void *left, const void *right) {
  const IsoNode *a = *(const IsoNode *const *)left;
  const IsoNode *b = *(const IsoNode *const *)right;
  return _stricmp(a->name, b->name);
}

static int dvd_audio_file_rank(const IsoNode *node) {
  if (!node->file || !node->parent || !node->parent->parent ||
      _stricmp(node->parent->name, "AUDIO_TS") != 0 ||
      node->parent->parent->parent != NULL || strlen(node->name) != 12U) {
    return -1;
  }

  static const char *const global_prefixes[] = {"AUDIO_PP", "AUDIO_TS", "AUDIO_SV"};
  static const char *const global_extensions[] = {".IFO", ".VOB", ".BUP"};
  for (int prefix = 0; prefix < 3; ++prefix) {
    if (_strnicmp(node->name, global_prefixes[prefix], 8) != 0) continue;
    for (int extension = 0; extension < 3; ++extension) {
      if (_stricmp(node->name + 8, global_extensions[extension]) == 0) {
        return prefix * 3 + extension;
      }
    }
  }

  if (_strnicmp(node->name, "ATS_", 4) != 0 ||
      node->name[4] < '0' || node->name[4] > '9' ||
      node->name[5] < '0' || node->name[5] > '9' || node->name[6] != '_' ||
      node->name[7] < '0' || node->name[7] > '9') {
    return -1;
  }
  int title_set = (node->name[4] - '0') * 10 + node->name[5] - '0';
  int segment = node->name[7] - '0';
  if (title_set == 0) return -1;
  int extension = -1;
  if (_stricmp(node->name + 8, ".IFO") == 0) extension = 0;
  if (_stricmp(node->name + 8, ".AOB") == 0) extension = 1;
  if (_stricmp(node->name + 8, ".BUP") == 0) extension = 2;
  if (extension < 0) return -1;
  return 9 + title_set * 30 + extension * 10 + segment;
}

static int compare_data_nodes(const void *left, const void *right) {
  const IsoNode *a = *(const IsoNode *const *)left;
  const IsoNode *b = *(const IsoNode *const *)right;
  int a_rank = dvd_audio_file_rank(a);
  int b_rank = dvd_audio_file_rank(b);
  if (a_rank >= 0 && b_rank < 0) return -1;
  if (a_rank < 0 && b_rank >= 0) return 1;
  if (a_rank >= 0 && b_rank >= 0 && a_rank != b_rank) {
    return a_rank < b_rank ? -1 : 1;
  }
  return _stricmp(a->name, b->name);
}

static IsoNode *make_node(const char *name, const char *path, int file,
                          IsoNode *parent) {
  IsoNode *node = (IsoNode *)calloc(1, sizeof(*node));
  if (!node) return NULL;
  node->name = duplicate_text(name);
  node->path = path ? duplicate_text(path) : NULL;
  node->file = file;
  node->parent = parent;
  if (!node->name || (path && !node->path) ||
      (parent && append_child(parent, node) != 0)) {
    free_tree(node);
    return NULL;
  }
  return node;
}

static IsoNode *read_tree(const char *path, const char *name, IsoNode *parent) {
  IsoNode *node = make_node(name, path, 0, parent);
  if (!node) return NULL;

  WIN32_FIND_DATAA data;
  char pattern[MAX_PATH * 4];
  if (snprintf(pattern, sizeof(pattern), "%s\\*", path) < 0) {
    free_tree(node);
    return NULL;
  }
  HANDLE handle = FindFirstFileA(pattern, &data);
  if (handle == INVALID_HANDLE_VALUE) {
    free_tree(node);
    return NULL;
  }

  int success = 1;
  do {
    if (!strcmp(data.cFileName, ".") || !strcmp(data.cFileName, "..")) continue;
    char child_path[MAX_PATH * 4];
    if (snprintf(child_path, sizeof(child_path), "%s\\%s", path,
                 data.cFileName) < 0) {
      success = 0;
      break;
    }
    if (data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
      if (!read_tree(child_path, data.cFileName, node)) {
        success = 0;
        break;
      }
    } else {
      IsoNode *child = make_node(data.cFileName, child_path, 1, node);
      if (!child) {
        success = 0;
        break;
      }
      HANDLE file = CreateFileA(child_path, GENERIC_READ,
                                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
      LARGE_INTEGER file_size;
      if (file == INVALID_HANDLE_VALUE || !GetFileSizeEx(file, &file_size) ||
          file_size.QuadPart < 0 || file_size.QuadPart > UINT32_MAX) {
        if (file != INVALID_HANDLE_VALUE) CloseHandle(file);
        success = 0;
        break;
      }
      CloseHandle(file);
      child->size = (uint32_t)file_size.QuadPart;
    }
  } while (FindNextFileA(handle, &data));
  FindClose(handle);
  if (!success) {
    free_tree(node);
    return NULL;
  }
  if (node->child_count > 1) {
    qsort(node->children, node->child_count, sizeof(*node->children),
          compare_nodes);
  }
  return node;
}

static size_t iso_name_length(const IsoNode *node) {
  if (!node->parent || node == node->parent) return 1U;
  if (node == node->parent->parent) return 1U;
  size_t length = strlen(node->name);
  if (node->file) length += 2U;
  return length;
}

static size_t record_length(const IsoNode *node, int special) {
  size_t name_length = special ? 1U : iso_name_length(node);
  return 33U + name_length + ((name_length % 2U) == 0U ? 1U : 0U);
}

static uint32_t directory_size(const IsoNode *node) {
  size_t total = 34U + 34U;
  for (size_t i = 0; i < node->child_count; ++i) {
    total += record_length(node->children[i], 0);
  }
  return (uint32_t)(((total + ISO_SECTOR - 1U) / ISO_SECTOR) * ISO_SECTOR);
}

static void assign_path_indices(IsoNode *node, uint32_t *next,
                                uint16_t parent_index) {
  node->path_index = (uint16_t)(*next);
  node->parent_path_index = parent_index;
  ++(*next);
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file) {
      assign_path_indices(node->children[i], next, node->path_index);
    }
  }
}

static uint32_t path_table_size(const IsoNode *node) {
  size_t name_length = node->parent ? strlen(node->name) : 1U;
  size_t entry_length = 8U + name_length + (name_length % 2U);
  uint32_t total = (uint32_t)entry_length;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file) total += path_table_size(node->children[i]);
  }
  return total;
}

static void assign_directories(IsoNode *node, uint32_t *next) {
  node->size = directory_size(node);
  node->lba = *next;
  *next += sector_count(node->size);
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file) assign_directories(node->children[i], next);
  }
}

static int collect_sorted_files(const IsoNode *node, IsoNode ***files,
                                size_t *count) {
  size_t file_count = 0;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (node->children[i]->file) ++file_count;
  }
  IsoNode **items = file_count ?
      (IsoNode **)malloc(file_count * sizeof(*items)) : NULL;
  if (file_count && !items) return -1;
  size_t index = 0;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (node->children[i]->file) items[index++] = node->children[i];
  }
  if (file_count > 1) {
    qsort(items, file_count, sizeof(*items), compare_data_nodes);
  }
  *files = items;
  *count = file_count;
  return 0;
}

static int assign_files(IsoNode *node, uint32_t *next) {
  IsoNode **files = NULL;
  size_t file_count = 0;
  if (collect_sorted_files(node, &files, &file_count) != 0) return -1;
  for (size_t i = 0; i < file_count; ++i) {
    IsoNode *child = files[i];
    child->lba = *next;
    *next += sector_count(child->size);
  }
  free(files);
  for (size_t i = 0; i < node->child_count; ++i) {
    IsoNode *child = node->children[i];
    if (!child->file && assign_files(child, next) != 0) {
      return -1;
    }
  }
  return 0;
}

static void little16(unsigned char *p, uint16_t value) {
  p[0] = (unsigned char)value;
  p[1] = (unsigned char)(value >> 8);
}

static void little32(unsigned char *p, uint32_t value) {
  p[0] = (unsigned char)value;
  p[1] = (unsigned char)(value >> 8);
  p[2] = (unsigned char)(value >> 16);
  p[3] = (unsigned char)(value >> 24);
}

static void little64(unsigned char *p, uint64_t value) {
  little32(p, (uint32_t)value);
  little32(p + 4, (uint32_t)(value >> 32));
}

static void big16(unsigned char *p, uint16_t value) {
  p[0] = (unsigned char)(value >> 8);
  p[1] = (unsigned char)value;
}

static void big32(unsigned char *p, uint32_t value) {
  p[0] = (unsigned char)(value >> 24);
  p[1] = (unsigned char)(value >> 16);
  p[2] = (unsigned char)(value >> 8);
  p[3] = (unsigned char)value;
}

static void both16(unsigned char *p, uint16_t value) {
  little16(p, value);
  big16(p + 2, value);
}

static void both32(unsigned char *p, uint32_t value) {
  little32(p, value);
  big32(p + 4, value);
}

static int write_zeroes(FILE *output, uint32_t count) {
  static const unsigned char zeroes[ISO_SECTOR] = {0};
  while (count) {
    uint32_t chunk = count > ISO_SECTOR ? ISO_SECTOR : count;
    if (fwrite(zeroes, 1, chunk, output) != chunk) return -1;
    count -= chunk;
  }
  return 0;
}

static int write_directory_record(FILE *output, const IsoNode *parent,
                                  const IsoNode *node, int special,
                                  int directory) {
  unsigned char record[256];
  memset(record, 0, sizeof(record));
  unsigned char *name = record + 33;
  size_t name_length;
  if (special == 1) {
    name[0] = 0;
    name_length = 1;
  } else if (special == 2) {
    name[0] = 1;
    name_length = 1;
  } else {
    name_length = strlen(node->name);
    if (node->file) {
      if (name_length > 248U) return -1;
      memcpy(name, node->name, name_length);
      name[name_length++] = ';';
      name[name_length++] = '1';
    } else {
      if (name_length > 250U) return -1;
      memcpy(name, node->name, name_length);
    }
  }
  size_t length = 33U + name_length + ((name_length % 2U) == 0U ? 1U : 0U);
  if (length > sizeof(record)) return -1;
  record[0] = (unsigned char)length;
  both32(record + 2, node->lba);
  both32(record + 10, node->size);
  record[25] = directory ? 2U : 0U;
  both16(record + 28, 1U);
  record[32] = (unsigned char)name_length;
  (void)parent;
  return fwrite(record, 1, length, output) == length ? 0 : -1;
}

static int write_directory(FILE *output, const IsoNode *node) {
  uint32_t written = 0;
  if (write_directory_record(output, node, node, 1, 1) != 0) return -1;
  written += 34U;
  if (write_directory_record(output, node, node->parent ? node->parent : node,
                             2, 1) != 0) return -1;
  written += 34U;
  for (size_t i = 0; i < node->child_count; ++i) {
    IsoNode *child = node->children[i];
    size_t length = record_length(child, 0);
    if (written + length > node->size ||
        write_directory_record(output, node, child, 0, !child->file) != 0) {
      return -1;
    }
    written += (uint32_t)length;
  }
  return write_zeroes(output, node->size - written);
}

static int write_directories(FILE *output, const IsoNode *node) {
  if (write_directory(output, node) != 0) return -1;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        write_directories(output, node->children[i]) != 0) {
      return -1;
    }
  }
  return 0;
}

static int write_file(FILE *output, const IsoNode *node) {
  FILE *input = fopen(node->path, "rb");
  if (!input) return -1;
  unsigned char buffer[64U * 1024U];
  uint32_t written = 0;
  size_t read_count;
  while ((read_count = fread(buffer, 1, sizeof(buffer), input)) != 0) {
    if (fwrite(buffer, 1, read_count, output) != read_count) {
      fclose(input);
      return -1;
    }
    written += (uint32_t)read_count;
  }
  int error = ferror(input);
  fclose(input);
  if (error || written != node->size) return -1;
  return write_zeroes(output, sector_count(node->size) * ISO_SECTOR - node->size);
}

static int write_files(FILE *output, const IsoNode *node) {
  IsoNode **files = NULL;
  size_t file_count = 0;
  if (collect_sorted_files(node, &files, &file_count) != 0) return -1;
  for (size_t i = 0; i < file_count; ++i) {
    if (write_file(output, files[i]) != 0) {
      free(files);
      return -1;
    }
  }
  free(files);
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        write_files(output, node->children[i]) != 0) {
      return -1;
    }
  }
  return 0;
}

static int write_path_entry(FILE *output, const IsoNode *node, int big_endian) {
  unsigned char entry[256];
  memset(entry, 0, sizeof(entry));
  size_t name_length = node->parent ? strlen(node->name) : 1U;
  if (name_length > 31U) return -1;
  size_t length = 8U + name_length + (name_length % 2U);
  entry[0] = (unsigned char)name_length;
  entry[1] = 0;
  if (big_endian) {
    big32(entry + 2, node->lba);
    big16(entry + 6, node->parent_path_index);
  } else {
    little32(entry + 2, node->lba);
    little16(entry + 6, node->parent_path_index);
  }
  if (node->parent) memcpy(entry + 8, node->name, name_length);
  else entry[8] = 0;
  return fwrite(entry, 1, length, output) == length ? 0 : -1;
}

static int write_path_table(FILE *output, const IsoNode *node,
                            int big_endian, uint32_t table_sectors,
                            uint32_t *written) {
  if (write_path_entry(output, node, big_endian) != 0) return -1;
  size_t name_length = node->parent ? strlen(node->name) : 1U;
  *written += (uint32_t)(8U + name_length + (name_length % 2U));
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        write_path_table(output, node->children[i], big_endian,
                         table_sectors, written) != 0) {
      return -1;
    }
  }
  (void)table_sectors;
  return 0;
}

static int finish_path_table(FILE *output, uint32_t written,
                             uint32_t table_sectors) {
  uint32_t capacity = table_sectors * ISO_SECTOR;
  if (written > capacity) return -1;
  return write_zeroes(output, capacity - written);
}

static void copy_padded(unsigned char *destination, size_t capacity,
                        const char *source) {
  memset(destination, ' ', capacity);
  if (!source) return;
  size_t length = strlen(source);
  if (length > capacity) length = capacity;
  memcpy(destination, source, length);
}

static int encode_osta_name(const char *source, unsigned char *destination,
                            size_t capacity) {
  uint32_t codepoints[256];
  size_t count = 0;
  const unsigned char *p = (const unsigned char *)source;
  while (*p) {
    uint32_t value;
    size_t length;
    if (*p < 0x80U) {
      value = *p;
      length = 1;
    } else if ((*p & 0xe0U) == 0xc0U && p[1] &&
               (p[1] & 0xc0U) == 0x80U) {
      value = ((uint32_t)(p[0] & 0x1fU) << 6) | (p[1] & 0x3fU);
      length = 2;
    } else if ((*p & 0xf0U) == 0xe0U && p[1] && p[2] &&
               (p[1] & 0xc0U) == 0x80U && (p[2] & 0xc0U) == 0x80U) {
      value = ((uint32_t)(p[0] & 0x0fU) << 12) |
              ((uint32_t)(p[1] & 0x3fU) << 6) | (p[2] & 0x3fU);
      length = 3;
    } else if ((*p & 0xf8U) == 0xf0U && p[1] && p[2] && p[3] &&
               (p[1] & 0xc0U) == 0x80U && (p[2] & 0xc0U) == 0x80U &&
               (p[3] & 0xc0U) == 0x80U) {
      value = ((uint32_t)(p[0] & 0x07U) << 18) |
              ((uint32_t)(p[1] & 0x3fU) << 12) |
              ((uint32_t)(p[2] & 0x3fU) << 6) | (p[3] & 0x3fU);
      length = 4;
    } else {
      return -1;
    }
    if ((length == 2 && value < 0x80U) ||
        (length == 3 && value < 0x800U) ||
        (length == 4 && (value < 0x10000U || value > 0x10ffffU)) ||
        (value >= 0xd800U && value <= 0xdfffU) || count == 256U) {
      return -1;
    }
    codepoints[count++] = value;
    p += length;
  }

  int use_16_bit = 0;
  for (size_t i = 0; i < count; ++i) {
    if (codepoints[i] > 0xffU) use_16_bit = 1;
  }
  size_t required = 1U;
  for (size_t i = 0; i < count; ++i) {
    required += use_16_bit ? (codepoints[i] > 0xffffU ? 4U : 2U) : 1U;
  }
  if (required > capacity || required > 255U) return -1;
  destination[0] = use_16_bit ? 16U : 8U;
  size_t written = 1;
  for (size_t i = 0; i < count; ++i) {
    uint32_t value = codepoints[i];
    if (!use_16_bit) {
      destination[written++] = (unsigned char)value;
    } else if (value <= 0xffffU) {
      destination[written++] = (unsigned char)(value >> 8);
      destination[written++] = (unsigned char)value;
    } else {
      value -= 0x10000U;
      uint32_t high = 0xd800U + (value >> 10);
      uint32_t low = 0xdc00U + (value & 0x3ffU);
      destination[written++] = (unsigned char)(high >> 8);
      destination[written++] = (unsigned char)high;
      destination[written++] = (unsigned char)(low >> 8);
      destination[written++] = (unsigned char)low;
    }
  }
  return (int)written;
}

static uint16_t udf_crc16(const unsigned char *data, size_t length) {
  uint16_t crc = 0;
  for (size_t i = 0; i < length; ++i) {
    crc ^= (uint16_t)data[i] << 8;
    for (int bit = 0; bit < 8; ++bit) {
      crc = (crc & 0x8000U) ? (uint16_t)((crc << 1) ^ 0x1021U)
                            : (uint16_t)(crc << 1);
    }
  }
  return crc;
}

static void udf_set_tag(unsigned char *descriptor, uint16_t id,
                        uint32_t location, uint16_t descriptor_length) {
  little16(descriptor, id);
  little16(descriptor + 2, 2U);
  descriptor[4] = 0;
  descriptor[5] = 0;
  little16(descriptor + 6, 1U);
  little16(descriptor + 8, udf_crc16(descriptor + 16, descriptor_length));
  little16(descriptor + 10, descriptor_length);
  little32(descriptor + 12, location);
  unsigned char checksum = 0;
  for (size_t i = 0; i < 16; ++i) checksum = (unsigned char)(checksum + descriptor[i]);
  descriptor[4] = checksum;
}

static void udf_set_entity(unsigned char *descriptor, const char *name,
                           const unsigned char suffix[3]) {
  size_t length = strlen(name);
  if (length > 23U) length = 23U;
  descriptor[0] = 0U;
  memcpy(descriptor + 1, name, length);
  if (suffix) memcpy(descriptor + 24, suffix, 3U);
}

static void udf_set_charspec(unsigned char *descriptor) {
  memcpy(descriptor + 1, "OSTA Compressed Unicode", 23U);
}

static int udf_set_dstring(unsigned char *descriptor, size_t field_size,
                           const char *text) {
  if (field_size < 2U) return -1;
  memset(descriptor, 0, field_size);
  int length = encode_osta_name(text, descriptor, field_size - 1U);
  if (length < 0) {
    /* UDF d-strings reserve one byte for the compression id and one for
       the encoded length.  Keep the longest valid UTF-8 prefix when a UI
       volume label is longer than the destination field. */
    char prefix[1025];
    size_t used = 0;
    const unsigned char *p = (const unsigned char *)text;
    prefix[0] = 0;
    while (*p && used + 1U < sizeof(prefix)) {
      size_t character_length;
      if (*p < 0x80U) {
        character_length = 1U;
      } else if ((*p & 0xe0U) == 0xc0U) {
        character_length = 2U;
      } else if ((*p & 0xf0U) == 0xe0U) {
        character_length = 3U;
      } else if ((*p & 0xf8U) == 0xf0U) {
        character_length = 4U;
      } else {
        return -1;
      }
      if (used + character_length >= sizeof(prefix)) break;
      memcpy(prefix + used, p, character_length);
      prefix[used + character_length] = 0;
      int candidate = encode_osta_name(prefix, descriptor, field_size - 1U);
      if (candidate < 0) {
        if (used == 0) return -1;
        break;
      }
      used += character_length;
      p += character_length;
      length = candidate;
    }
    if (used == 0 && *text) return -1;
  }
  descriptor[field_size - 1U] = (unsigned char)length;
  return 0;
}

static void udf_set_extent(unsigned char *descriptor, uint32_t location,
                           uint32_t length) {
  little32(descriptor, length);
  little32(descriptor + 4, location);
}

static void udf_set_long_ad(unsigned char *descriptor, uint32_t location,
                            uint32_t length) {
  little32(descriptor, length);
  little32(descriptor + 4, location);
  little16(descriptor + 8, 0U);
}

static uint32_t udf_fid_size(const IsoNode *node, int parent_record) {
  unsigned char name[255];
  int name_length = parent_record ? 0 : encode_osta_name(node->name, name, sizeof(name));
  if (name_length < 0) return 0;
  return (uint32_t)((38U + (uint32_t)name_length + 3U) & ~3U);
}

static uint32_t udf_directory_content_size(const IsoNode *node) {
  uint64_t offset = 0;
  uint32_t length = udf_fid_size(node, 1);
  if (length == 0) return 0;
  offset += length;
  for (size_t i = 0; i < node->child_count; ++i) {
    IsoNode *child = node->children[i];
    length = udf_fid_size(child, 0);
    if (length == 0) return 0;
    if (offset % ISO_SECTOR + length > ISO_SECTOR) {
      offset = ((offset + ISO_SECTOR - 1U) / ISO_SECTOR) * ISO_SECTOR;
    }
    offset += length;
  }
  return offset <= UINT32_MAX ? (uint32_t)offset : 0;
}

static int assign_udf_directories(IsoNode *node, uint32_t *next) {
  node->udf_directory_length = udf_directory_content_size(node);
  if (node->udf_directory_length == 0) return -1;
  uint64_t padded_size =
      ((uint64_t)node->udf_directory_length + ISO_SECTOR - 1U) /
      ISO_SECTOR * ISO_SECTOR;
  if (padded_size > UINT32_MAX) return -1;
  node->udf_directory_size = (uint32_t)padded_size;
  node->udf_directory_lba = *next;
  *next += sector_count(node->udf_directory_size);
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        assign_udf_directories(node->children[i], next) != 0) return -1;
  }
  return 0;
}

static int assign_udf_file_entries(IsoNode *node, uint32_t *next,
                                  uint32_t *file_count,
                                  uint32_t *directory_count) {
  node->udf_file_entry_lba = (*next)++;
  if (node->file) ++*file_count;
  else ++*directory_count;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        assign_udf_file_entries(node->children[i], next, file_count,
                                directory_count) != 0) return -1;
    if (node->children[i]->file) {
      node->children[i]->udf_file_entry_lba = (*next)++;
      ++*file_count;
    }
  }
  return 0;
}

static void udf_set_timestamp(unsigned char *timestamp) {
  little16(timestamp, 0x1000U);
  little16(timestamp + 2, 2000U);
  timestamp[4] = 1U;
  timestamp[5] = 1U;
}

static int udf_write_fid(FILE *output, const IsoNode *directory,
                         const IsoNode *target, int parent_record,
                         uint32_t *directory_offset) {
  unsigned char encoded[255];
  int name_length = parent_record ? 0 :
      encode_osta_name(target->name, encoded, sizeof(encoded));
  if (name_length < 0) return -1;
  uint32_t length = (38U + (uint32_t)name_length + 3U) & ~3U;
  uint32_t within_sector = *directory_offset % ISO_SECTOR;
  if (within_sector + length > ISO_SECTOR) {
    uint32_t padding = ISO_SECTOR - within_sector;
    if (write_zeroes(output, padding) != 0) return -1;
    *directory_offset += padding;
  }
  unsigned char record[296];
  memset(record, 0, sizeof(record));
  little16(record + 16, 1U);
  record[18] = (unsigned char)((target->file ? 0U : 2U) |
                               (parent_record ? 8U : 0U));
  record[19] = (unsigned char)name_length;
  udf_set_long_ad(record + 20,
                  target->udf_file_entry_lba - UDF_PARTITION_START,
                  ISO_SECTOR);
  if (name_length) memcpy(record + 38, encoded, (size_t)name_length);
  udf_set_tag(record, 257U,
              directory->udf_directory_lba - UDF_PARTITION_START +
                  *directory_offset / ISO_SECTOR,
              (uint16_t)(length - 16U));
  if (fwrite(record, 1, length, output) != length) return -1;
  *directory_offset += length;
  return 0;
}

static int write_udf_directories(FILE *output, const IsoNode *node) {
  uint32_t written = 0;
  const IsoNode *parent = node->parent ? node->parent : node;
  if (udf_write_fid(output, node, parent, 1, &written) != 0) return -1;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (udf_write_fid(output, node, node->children[i], 0, &written) != 0) return -1;
  }
  if (written > node->udf_directory_length ||
      write_zeroes(output, node->udf_directory_size - written) != 0) return -1;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (!node->children[i]->file &&
        write_udf_directories(output, node->children[i]) != 0) return -1;
  }
  return 0;
}

static int write_udf_file_entry(FILE *output, const IsoNode *node) {
  unsigned char descriptor[ISO_SECTOR];
  memset(descriptor, 0, sizeof(descriptor));
  little16(descriptor + 20, 4U);
  little16(descriptor + 24, 1U);
  descriptor[27] = node->file ? 5U : 4U;
  little16(descriptor + 34, 0x0230U);
  little32(descriptor + 36, UINT32_MAX);
  little32(descriptor + 40, UINT32_MAX);
  little32(descriptor + 44, 0x00000111U);
  little16(descriptor + 48, 1U);
  little64(descriptor + 56, node->file ? node->size : node->udf_directory_length);
  little64(descriptor + 64, node->file ? sector_count(node->size) :
                                             sector_count(node->udf_directory_size));
  udf_set_timestamp(descriptor + 72);
  udf_set_timestamp(descriptor + 84);
  udf_set_timestamp(descriptor + 96);
  little32(descriptor + 108, 1U);
  udf_set_entity(descriptor + 128, "*DVD-Audio Maker", NULL);
  little64(descriptor + 160, node->udf_file_entry_lba - UDF_PARTITION_START + 1U);

  unsigned char *allocation = descriptor + 176;
  uint32_t allocation_bytes = 0;
  if (node->file) {
    uint32_t remaining = node->size;
    uint32_t location = node->lba - UDF_PARTITION_START;
    while (remaining) {
      uint32_t extent = remaining > 0x3ffff800U ? 0x3ffff800U : remaining;
      little32(allocation + allocation_bytes, extent);
      little32(allocation + allocation_bytes + 4U, location);
      allocation_bytes += 8U;
      uint32_t extent_sectors = sector_count(extent);
      location += extent_sectors;
      remaining -= extent;
      if (allocation_bytes > ISO_SECTOR - 176U) return -1;
    }
  } else {
    little32(allocation, node->udf_directory_length);
    little32(allocation + 4U,
             node->udf_directory_lba - UDF_PARTITION_START);
    allocation_bytes = 8U;
  }
  little32(descriptor + 168, 0U);
  little32(descriptor + 172, allocation_bytes);
  udf_set_tag(descriptor, 261U,
              node->udf_file_entry_lba - UDF_PARTITION_START,
              (uint16_t)(176U + allocation_bytes - 16U));
  return fwrite(descriptor, 1, sizeof(descriptor), output) == sizeof(descriptor)
             ? 0 : -1;
}

static int write_udf_file_entries(FILE *output, const IsoNode *node) {
  if (write_udf_file_entry(output, node) != 0) return -1;
  for (size_t i = 0; i < node->child_count; ++i) {
    if (node->children[i]->file) {
      if (write_udf_file_entry(output, node->children[i]) != 0) return -1;
    } else if (write_udf_file_entries(output, node->children[i]) != 0) {
      return -1;
    }
  }
  return 0;
}

static int write_udf_volume_descriptor(FILE *output, unsigned char *descriptor) {
  udf_set_tag(descriptor, udf_pending_tag_id, udf_pending_tag_location,
              udf_pending_crc_length);
  return fwrite(descriptor, 1, ISO_SECTOR, output) == ISO_SECTOR ? 0 : -1;
}

static void make_udf_volume_descriptor(unsigned char *descriptor, uint16_t id,
                                       uint32_t location, uint16_t crc_length) {
  memset(descriptor, 0, ISO_SECTOR);
  udf_pending_tag_id = id;
  udf_pending_tag_location = location;
  udf_pending_crc_length = crc_length;
}

static int write_udf_descriptor_sequences(FILE *output, uint32_t volume_space,
                                          uint32_t volume_end_anchor,
                                          uint32_t file_count,
                                          uint32_t directory_count,
                                          const char *volume_identifier) {
  static const unsigned char udf_domain_suffix[3] = {2U, 1U, 3U};
  static const unsigned char udf_revision_suffix[3] = {2U, 1U, 0U};
  unsigned char descriptor[ISO_SECTOR];
  for (int copy = 0; copy < 2; ++copy) {
    uint32_t base = copy ? UDF_RESERVE_VDS : UDF_MAIN_VDS;
    make_udf_volume_descriptor(descriptor, 1U, base, 496U);
    little32(descriptor + 16, 0U);
    little32(descriptor + 20, 0U);
    if (udf_set_dstring(descriptor + 24, 32U, volume_identifier) != 0) return -1;
    little16(descriptor + 56, 1U);
    little16(descriptor + 58, 1U);
    little16(descriptor + 60, 2U);
    little16(descriptor + 62, 2U);
    little32(descriptor + 64, 1U);
    little32(descriptor + 68, 1U);
    if (udf_set_dstring(descriptor + 72, 128U, volume_identifier) != 0) return -1;
    udf_set_charspec(descriptor + 200);
    udf_set_charspec(descriptor + 264);
    udf_set_timestamp(descriptor + 376);
    udf_set_entity(descriptor + 388, "*DVD-Audio Maker", NULL);
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;

    make_udf_volume_descriptor(descriptor, 4U, base + 1U, 496U);
    little32(descriptor + 16, 1U);
    udf_set_entity(descriptor + 20, "*UDF LV Info", udf_revision_suffix);
    udf_set_charspec(descriptor + 52);
    if (udf_set_dstring(descriptor + 116, 128U, volume_identifier) != 0) return -1;
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;

    make_udf_volume_descriptor(descriptor, 5U, base + 2U, 496U);
    little32(descriptor + 16, 2U);
    little16(descriptor + 20, 1U);
    little16(descriptor + 22, 0U);
    udf_set_entity(descriptor + 24, "+NSR02", NULL);
    descriptor[24] = 2U;
    little32(descriptor + 184, 1U);
    little32(descriptor + 188, UDF_PARTITION_START);
    little32(descriptor + 192, volume_end_anchor - UDF_PARTITION_START);
    udf_set_entity(descriptor + 196, "*DVD-Audio Maker", NULL);
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;

    make_udf_volume_descriptor(descriptor, 6U, base + 3U, 430U);
    little32(descriptor + 16, 3U);
    udf_set_charspec(descriptor + 20);
    if (udf_set_dstring(descriptor + 84, 128U, volume_identifier) != 0) return -1;
    little32(descriptor + 212, ISO_SECTOR);
    udf_set_entity(descriptor + 216, "*OSTA UDF Compliant", udf_domain_suffix);
    udf_set_long_ad(descriptor + 248, 0U, 2U * ISO_SECTOR);
    little32(descriptor + 264, 6U);
    little32(descriptor + 268, 1U);
    udf_set_entity(descriptor + 272, "*DVD-Audio Maker", NULL);
    udf_set_extent(descriptor + 432, UDF_INTEGRITY_LBA, 2U * ISO_SECTOR);
    descriptor[440] = 1U;
    descriptor[441] = 6U;
    little16(descriptor + 442, 1U);
    little16(descriptor + 444, 0U);
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;

    make_udf_volume_descriptor(descriptor, 7U, base + 4U, 8U);
    little32(descriptor + 16, 4U);
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;

    make_udf_volume_descriptor(descriptor, 8U, base + 5U, 496U);
    if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;
    if (!copy) {
      if (write_zeroes(output, 10U * ISO_SECTOR) != 0) return -1;
    } else if (write_zeroes(output, 10U * ISO_SECTOR) != 0) {
      return -1;
    }
  }

  make_udf_volume_descriptor(descriptor, 9U, UDF_INTEGRITY_LBA, 118U);
  udf_set_timestamp(descriptor + 16);
  little32(descriptor + 28, 1U);
  little64(descriptor + 40, volume_end_anchor - UDF_PARTITION_START + 1U);
  little32(descriptor + 72, 1U);
  little32(descriptor + 76, 46U);
  little32(descriptor + 84, volume_end_anchor - UDF_PARTITION_START);
  udf_set_entity(descriptor + 88, "*DVD-Audio Maker", NULL);
  little32(descriptor + 120, file_count);
  little32(descriptor + 124, directory_count);
  little16(descriptor + 128, 0x102U);
  little16(descriptor + 130, 0x102U);
  little16(descriptor + 132, 0x102U);
  if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;
  make_udf_volume_descriptor(descriptor, 8U, UDF_INTEGRITY_LBA + 1U, 496U);
  if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;
  if (volume_space <= UDF_INTEGRITY_LBA + 1U) return -1;
  (void)volume_space;
  return 0;
}

static int write_udf_anchor(FILE *output, uint32_t location) {
  unsigned char descriptor[ISO_SECTOR];
  make_udf_volume_descriptor(descriptor, 2U, location, 496U);
  udf_set_extent(descriptor + 16, UDF_MAIN_VDS, 16U * ISO_SECTOR);
  udf_set_extent(descriptor + 24, UDF_RESERVE_VDS, 16U * ISO_SECTOR);
  return write_udf_volume_descriptor(output, descriptor);
}

static int write_udf_file_set(FILE *output, const char *volume_identifier,
                              uint32_t root_file_entry_lba) {
  unsigned char descriptor[ISO_SECTOR];
  make_udf_volume_descriptor(descriptor, 256U, 0U, 496U);
  udf_set_timestamp(descriptor + 16);
  little16(descriptor + 28, 3U);
  little16(descriptor + 30, 3U);
  little32(descriptor + 32, 1U);
  little32(descriptor + 36, 1U);
  udf_set_charspec(descriptor + 48);
  if (udf_set_dstring(descriptor + 112, 128U, volume_identifier) != 0) return -1;
  udf_set_charspec(descriptor + 240);
  if (udf_set_dstring(descriptor + 304, 32U, volume_identifier) != 0) return -1;
  udf_set_long_ad(descriptor + 400,
                  root_file_entry_lba - UDF_PARTITION_START, ISO_SECTOR);
  udf_set_entity(descriptor + 416, "*OSTA UDF Compliant",
                 (const unsigned char[]){2U, 1U, 3U});
  if (write_udf_volume_descriptor(output, descriptor) != 0) return -1;
  make_udf_volume_descriptor(descriptor, 8U, 1U, 496U);
  return write_udf_volume_descriptor(output, descriptor);
}

static int write_udf_recognition(FILE *output) {
  static const char *const identifiers[] = {"BEA01", "NSR02", "TEA01"};
  unsigned char sector[ISO_SECTOR];
  for (size_t i = 0; i < sizeof(identifiers) / sizeof(identifiers[0]); ++i) {
    memset(sector, 0, sizeof(sector));
    memcpy(sector + 1, identifiers[i], 5U);
    sector[6] = 1U;
    if (fwrite(sector, 1, sizeof(sector), output) != sizeof(sector)) return -1;
  }
  return 0;
}

static int write_volume_descriptors(FILE *output, const IsoNode *root,
                                    uint32_t volume_space, uint32_t path_size,
                                    uint32_t path_lba, uint32_t path_m_lba,
                                    const char *volume_identifier) {
  unsigned char sector[ISO_SECTOR];
  memset(sector, 0, sizeof(sector));
  sector[0] = 1;
  memcpy(sector + 1, "CD001", 5);
  sector[6] = 1;
  copy_padded(sector + 8, 32, "DVDA-MAKER");
  copy_padded(sector + 40, 32,
              volume_identifier ? volume_identifier : "DVD-AUDIO");
  both32(sector + 80, volume_space);
  both16(sector + 120, 1);
  both16(sector + 124, 1);
  both16(sector + 128, ISO_SECTOR);
  both32(sector + 132, path_size);
  little32(sector + 140, path_lba);
  little32(sector + 144, 0);
  big32(sector + 148, path_m_lba);
  big32(sector + 152, 0);
  sector[156] = 34;
  both32(sector + 158, root->lba);
  both32(sector + 166, root->size);
  sector[181] = 2;
  both16(sector + 184, 1U);
  sector[188] = 1U;
  sector[189] = 0U;
  sector[881] = 1U;
  if (fwrite(sector, 1, sizeof(sector), output) != sizeof(sector)) return -1;

  memset(sector, 0, sizeof(sector));
  sector[0] = 255;
  memcpy(sector + 1, "CD001", 5);
  sector[6] = 1;
  return fwrite(sector, 1, sizeof(sector), output) == sizeof(sector) ? 0 : -1;
}

int dvda_iso_write(const char *source_directory, const char *destination,
                   const char *volume_identifier) {
  const char *end = source_directory + strlen(source_directory);
  while (end > source_directory &&
         (end[-1] == '\\' || end[-1] == '/')) {
    --end;
  }
  const char *separator = end;
  while (separator > source_directory && separator[-1] != '\\' &&
         separator[-1] != '/') {
    --separator;
  }
  const char *root_name = separator == source_directory ? source_directory
                                                          : separator;
  if (separator != source_directory) ++root_name;
  char root_name_buffer[256];
  size_t root_name_length = (size_t)(end - root_name);
  if (root_name_length == 0 || root_name_length >= sizeof(root_name_buffer)) {
    strcpy(root_name_buffer, "ROOT");
    root_name = root_name_buffer;
  } else {
    memcpy(root_name_buffer, root_name, root_name_length);
    root_name_buffer[root_name_length] = 0;
    root_name = root_name_buffer;
  }

  IsoNode *root = read_tree(source_directory, root_name, NULL);
  if (!root) return -1;
  uint32_t next_path_index = 1;
  assign_path_indices(root, &next_path_index, 1);
  uint32_t path_size = path_table_size(root);
  uint32_t path_sectors = (path_size + ISO_SECTOR - 1U) / ISO_SECTOR;
  if (path_sectors == 0 || path_sectors > 0x7fffU) {
    free_tree(root);
    return -1;
  }
  uint32_t udf_next = UDF_DIRECTORY_START;
  uint32_t udf_file_count = 0;
  uint32_t udf_directory_count = 0;
  if (assign_udf_directories(root, &udf_next) != 0 ||
      assign_udf_file_entries(root, &udf_next, &udf_file_count,
                              &udf_directory_count) != 0) {
    free_tree(root);
    return -1;
  }
  uint32_t path_lba = udf_next;
  uint32_t path_m_lba = path_lba + path_sectors;
  uint32_t next = path_m_lba + path_sectors;
  assign_directories(root, &next);
  if (assign_files(root, &next) != 0 || next >= UINT32_MAX) {
    free_tree(root);
    return -1;
  }
  uint32_t end_anchor_lba = next;
  uint32_t volume_space = end_anchor_lba + 1U;
  const char *udf_volume_identifier =
      volume_identifier ? volume_identifier : "DVD-AUDIO";

  FILE *output = fopen(destination, "wb");
  if (!output) {
    free_tree(root);
    return -1;
  }
  int success = 0;
  int failure_errno = 0;
#define ISO_TRY(stage, expression)                                             \
  do {                                                                          \
    if ((expression) != 0) {                                                    \
      failure_errno = errno;                                                   \
      fprintf(stderr, "[ISO] %s failed: %s\n", (stage),                      \
              failure_errno ? strerror(failure_errno) : "unknown error");     \
      goto done;                                                                \
    }                                                                           \
  } while (0)
  ISO_TRY("initial zero sectors", write_zeroes(output, 16U * ISO_SECTOR));
  ISO_TRY("volume descriptors", write_volume_descriptors(
      output, root, volume_space, path_size, path_lba, path_m_lba,
      volume_identifier));
  ISO_TRY("UDF recognition", write_udf_recognition(output));
  ISO_TRY("descriptor padding", write_zeroes(output, 11U * ISO_SECTOR));
  ISO_TRY("UDF descriptor sequences", write_udf_descriptor_sequences(
      output, volume_space, end_anchor_lba, udf_file_count,
      udf_directory_count, udf_volume_identifier));
  ISO_TRY("UDF integrity padding", write_zeroes(
      output, (UDF_ANCHOR_LBA - UDF_INTEGRITY_LBA - 2U) * ISO_SECTOR));
  ISO_TRY("initial UDF anchor", write_udf_anchor(output, UDF_ANCHOR_LBA));
  ISO_TRY("UDF file set", write_udf_file_set(output, udf_volume_identifier,
                                               root->udf_file_entry_lba));
  ISO_TRY("UDF directories", write_udf_directories(output, root));
  ISO_TRY("UDF file entries", write_udf_file_entries(output, root));
  uint32_t written = 0;
  ISO_TRY("little-endian path table", write_path_table(
      output, root, 0, path_sectors, &written));
  ISO_TRY("little-endian path table padding", finish_path_table(
      output, written, path_sectors));
  written = 0;
  ISO_TRY("big-endian path table", write_path_table(
      output, root, 1, path_sectors, &written));
  ISO_TRY("big-endian path table padding", finish_path_table(
      output, written, path_sectors));
  ISO_TRY("ISO directories", write_directories(output, root));
  ISO_TRY("ISO files", write_files(output, root));
  ISO_TRY("final UDF anchor", write_udf_anchor(output, end_anchor_lba));
  success = 1;
done:
#undef ISO_TRY
  if (fclose(output) != 0 && !failure_errno) failure_errno = errno;
  if (!success) {
    remove(destination);
    if (failure_errno) errno = failure_errno;
  }
  free_tree(root);
  return success ? 0 : -1;
}
