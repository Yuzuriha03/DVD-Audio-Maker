/* Project-owned directory ABI, implemented by Rust resources.rs. */
#ifndef DVDA_MSVC_DIRENT_H
#define DVDA_MSVC_DIRENT_H
#include <stdint.h>
typedef void DIR;
struct dirent {
    int32_t d_ino;
    uint16_t d_reclen;
    uint16_t d_namlen;
    char d_name[260];
};
#endif
