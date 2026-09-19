#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""为 mlp8 构建树应用与 FFmpeg 版本无关的基础修复（幂等）。

修复项：
1. winport.h：4 参内联 close_handles 与 5 参实现同名冲突 → 改名；
   并补上 Linux 下按值传参的 close_handles 声明。
2. winport.c：实现改为按值传参的 4 参版本。
3. libsoxconvert.c：加 WITHOUT_sox 守卫与桩函数。
"""
import pathlib
import sys

ROOT = pathlib.Path("/root/dvda-author-mlp8")
H = ROOT / "libutils/src/include/winport.h"
C = ROOT / "libutils/src/winport.c"
SOX = ROOT / "src/libsoxconvert.c"

changed = False


def sub(path, old, new, label):
    global changed
    text = path.read_text(encoding="utf-8", errors="surrogateescape")
    if old not in text:
        if new in text:
            print("[SKIP] %s" % label)
        else:
            print("[MISS] %s" % label)
        return
    path.write_text(text.replace(old, new, 1), encoding="utf-8", errors="surrogateescape")
    print("[OK] %s" % label)
    changed = True


# 1) winport.h 内联函数改名
sub(
    H,
    "static inline void close_handles(FILE_DESCRIPTOR a, FILE_DESCRIPTOR b, FILE_DESCRIPTOR c, FILE_DESCRIPTOR d)",
    "static inline void close_file_descriptors(FILE_DESCRIPTOR a, FILE_DESCRIPTOR b, FILE_DESCRIPTOR c, FILE_DESCRIPTOR d)",
    "winport.h 内联函数改名",
)

# 2) winport.h 补声明
text = H.read_text(encoding="utf-8", errors="surrogateescape")
if "void close_handles(int tube0" not in text:
    text += "\n#ifndef _WIN32\nvoid close_handles(int tube0, int tube1, int tubeerr0, int tubeerr1);\n#endif\n"
    H.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("[OK] winport.h 补 close_handles 声明")
    changed = True
else:
    print("[SKIP] winport.h close_handles 声明")

# 3) winport.c 按值传参实现
OLD_IMPL = """void close_handles(
        int* tube0,
        int* tube1,
        int* tubeerr0,
        int* tubeerr1,
        int* GCC_UNUSED parent_stderr)
{
      close(*tube1);
      close(*tube0);
      close(*tubeerr1);
      close(*tubeerr0);
}"""
NEW_IMPL = """void close_handles(int tube0, int tube1, int tubeerr0, int tubeerr1)
{
      close(tube1);
      close(tube0);
      close(tubeerr1);
      close(tubeerr0);
}"""
if OLD_IMPL in C.read_text(encoding="utf-8", errors="surrogateescape"):
    sub(C, OLD_IMPL, NEW_IMPL, "winport.c 实现改为按值传参")
else:
    # 或许已被第一步的部分修改影响，尝试更宽松替换
    t = C.read_text(encoding="utf-8", errors="surrogateescape")
    import re
    m = re.search(r"void close_handles\(\s*int\* tube0,.*?\n\}", t, re.S)
    if m and "close(*tube1)" in m.group(0):
        C.write_text(t.replace(m.group(0), NEW_IMPL, 1), encoding="utf-8", errors="surrogateescape")
        print("[OK] winport.c 实现改为按值传参(regex)")
        changed = True
    else:
        print("[SKIP] winport.c close_handles")

# 4) libsoxconvert.c 守卫
t = SOX.read_text(encoding="utf-8", errors="surrogateescape")
if "#ifdef WITHOUT_sox" not in t:
    STUB = '''#ifdef WITHOUT_sox
/* 关闭 SoX 时的桩实现；音频格式转换一律由 ffmpeg 预先完成，不会调用 */
#include "c_utils.h"
#include "structures.h"

int soxconvert(char *input, char *output, globalData *globals)
{
  (void) input; (void) output; (void) globals;
  return -1;
}

int resample(char *in, char *out, unsigned int channels, unsigned int bitrate, unsigned int samplerate)
{
  (void) in; (void) out; (void) channels; (void) bitrate; (void) samplerate;
  return -1;
}

#else
'''
    SOX.write_text(STUB + t + "\n#endif /* WITHOUT_sox */\n", encoding="utf-8", errors="surrogateescape")
    print("[OK] libsoxconvert.c 加 WITHOUT_sox 守卫")
    changed = True
else:
    print("[SKIP] libsoxconvert.c 守卫")

print("[DONE] 基础修复完成" if changed else "[DONE] 无需修改")
