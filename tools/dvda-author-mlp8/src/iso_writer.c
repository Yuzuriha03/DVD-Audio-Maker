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

#define ISO_SECTOR 2048U

typedef struct IsoNode IsoNode;

struct IsoNode {
  char *name;
  char *path;
  int file;
  uint32_t lba;
  uint32_t size;
  uint16_t path_index;
  uint16_t parent_path_index;
  IsoNode *parent;
  IsoNode **children;
  size_t child_count;
  size_t child_capacity;
};

static uint32_t sector_count(uint32_t size) {
  return size == 0 ? 0 : (size + ISO_SECTOR - 1U) / ISO_SECTOR;
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

static void assign_files(IsoNode *node, uint32_t *next) {
  for (size_t i = 0; i < node->child_count; ++i) {
    IsoNode *child = node->children[i];
    if (child->file) {
      child->lba = *next;
      *next += sector_count(child->size);
    } else {
      assign_files(child, next);
    }
  }
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
  for (size_t i = 0; i < node->child_count; ++i) {
    if (node->children[i]->file) {
      if (write_file(output, node->children[i]) != 0) return -1;
    } else if (write_files(output, node->children[i]) != 0) {
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
  sector[181] = 1;
  sector[188] = 1;
  sector[189] = 0;
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
  uint32_t next = 18U + path_sectors * 2U;
  assign_directories(root, &next);
  assign_files(root, &next);

  FILE *output = fopen(destination, "wb");
  if (!output) {
    free_tree(root);
    return -1;
  }
  int success = 0;
  if (write_zeroes(output, 16U * ISO_SECTOR) != 0 ||
      write_volume_descriptors(output, root, next, path_size, 18U,
                               18U + path_sectors, volume_identifier) != 0) {
    goto done;
  }
  uint32_t written = 0;
  if (write_path_table(output, root, 0, path_sectors, &written) != 0 ||
      finish_path_table(output, written, path_sectors) != 0) {
    goto done;
  }
  written = 0;
  if (write_path_table(output, root, 1, path_sectors, &written) != 0 ||
      finish_path_table(output, written, path_sectors) != 0 ||
      write_directories(output, root) != 0 || write_files(output, root) != 0) {
    goto done;
  }
  success = 1;
done:
  fclose(output);
  if (!success) remove(destination);
  free_tree(root);
  return success ? 0 : -1;
}
