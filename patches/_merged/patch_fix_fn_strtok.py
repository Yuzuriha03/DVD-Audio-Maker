#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 `fn_strtok()` 的越界写：空串时写零长度 VLA，并踩坏调用方的 `globals`。

`auxiliary.c` 的 `fn_strtok()`：

    char *s = strdup(chain);
    ...
    uint32_t j = 1, k = 0;
    int32_t cut[strlen(s) / 2];        // ← 长度为 0 的 VLA（当 chain 是空串）
    cut[0] = -1;                       // ← 越界写栈
    do  if (s[j] == delim) { cut[++k] = j; }
    while (s[j++] != '\0');            // ← 空串时从 s[1] 起，已越过 1 字节的分配
    cut[k + 1] = j - 1;                // ← 再次越界

两处缺陷：

1. **空串**（`chain == ""`）时 `strdup` 只分配 1 字节，而扫描从 `s[1]` 开始 ——
   已越过分配边界，会一路读到堆里第一个 `\0` 才停；`cut` 又是 0 长度的 VLA，
   `cut[0] = -1` 与 `cut[k+1]` 都是**越界写栈**。
   实测后果：栈上调用方保存的 `globals` 指针被踩成 `0x7ffd00000006`，
   随后 `create_stillpic_directory()` 里
   `change_directory(globals->settings.stillpicdir, globals)` 段错误。
   **崩点与原因隔了好几层，且完全取决于栈布局** —— 同一个二进制，
   gdb 下（布局不同）不崩、正常运行崩，换成别的曲目数也可能只是"偶尔"崩。

2. **`cut` 的容量不够**：条目数 = 分隔符个数 + 2。原式按 `strlen(s) / 2`
   开，对 `",,,"` 这种每字符都是分隔符的串只需 1 项却要写 5 项。
   （`--stillpics` 里 `:::` 就是这种串。）

什么时候会走到空串：`--stillpics` 用**空项表示沿用上一张图**
（`--stillpics A.jpg::C.jpg`），解析时会对每个空项调用
`fn_strtok("", ',', ...)` —— 也就是「每专辑只存一张封面」这个省 ASVS 预算的
标准做法，必然触发。实测本项目盘2（56 轨 / 39 个空项）稳定段错误。

修法：
  · 扫描前先 `strlen(s)`；空串直接跳过扫描
  · `cut` 按 `slen + 2` 开（最坏情况每字符都是分隔符，正好够）
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/auxiliary.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """  uint32_t j = 1, k = 0;
  int32_t cut[strlen(s) / 2];
  cut[0] = -1;
  do  if (s[j] == delim)
      {
        cut[++k] = j;
      }
  while (s[j++] != '\\0');
  cut[k + 1] = j - 1;
"""

NEW = """  /* cut 的条目数 = 分隔符个数 + 2。最坏情况每个字符都是分隔符（如 ",,,"，
     --stillpics 里的 ":::" 就是），所以按 slen + 2 开 ——
     原来的 strlen(s) / 2 在那种串上不够，会越界写栈。 */
  size_t slen = strlen(s);
  int32_t cut[slen + 2];
  cut[0] = -1;
  uint32_t j = 1, k = 0;
  /* 空串必须跳过扫描：原式从 s[1] 起，而空串只有 1 字节（含结尾 '\\0'），
     s[1] 已越过分配 —— 会一路扫到堆里第一个 '\\0'，并把 cut 越界写到栈上，
     踩坏调用方保存的 globals 指针（随后在 create_stillpic_directory() 里
     段错误）。--stillpics 的空项（表示沿用上一张图）正是空串。 */
  if (slen > 0)
    do  if (s[j] == delim)
        {
          cut[++k] = j;
        }
    while (s[j++] != '\\0');
  cut[k + 1] = j - 1;
"""

if OLD not in text:
    if "cut 的条目数 = 分隔符个数 + 2" in text:
        print("[SKIP] 已应用过")
        sys.exit(0)
    print("[MISS] 未找到 fn_strtok 的 cut 代码")
    sys.exit(1)

if text.count(OLD) != 1:
    print("[MISS] 匹配到 %d 处，需唯一" % text.count(OLD))
    sys.exit(1)

PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                errors="surrogateescape")
print("[OK] fn_strtok：cut 容量按 slen+2，空串跳过扫描")
print("\nfn_strtok 越界写修复完成，共 1 项")
