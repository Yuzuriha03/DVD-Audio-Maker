#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图记录的 byte1 置 0x04（= 巴赫的写法）。

巴赫（1 title / 18 轨 / ASVS 单记录）导航与封面都正常，其静图记录每轨为
  01 04 ...  —— 第 2 字节 = 0x04
而我们写 00。源码里 0x04 只在传了 `--stilloptions manual` 时才写：

    if ((img->options) && (img->options[s]) && (img->options[s]->manual))
      atsi[i] = 0x04;

这里无条件写 0x04（对齐巴赫）。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "byte1 置 0x04（巴赫式）"

OLD = """              if ((img->options)
                  && (img->options[s])
                  && (img->options[s]->manual))
                atsi[i] = 0x04;
"""

NEW = """              /* byte1 置 0x04（巴赫式；源码只在 --stilloptions manual
                 时才写，这里无条件写以对齐商业盘）。 */
              atsi[i] = 0x04;
"""


def main():
    p = SRC / "atsi2.c"
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    n = t.count(OLD)
    if n != 1:
        print("[FAIL] 匹配 %d 次" % n)
        return 1
    p.write_text(t.replace(OLD, NEW, 1), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   atsi2.c：静图 byte1 = 0x04")
    return 0


if __name__ == "__main__":
    sys.exit(main())
