#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ASVS 记录改成「每张图一条」。配合 ATS 图号逐轨递增（1..56）。

真机实测（PowerDVD）：
  · 导航正常 ⟺ 只有一个 title
  · 封面正常 ⟺ ATS 的「图号」是合法的 ASVS 记录索引

于是：title 只有 1 个（导航），ASVS 写 56 条记录（图号 1..56 正好索引它们）。
每条记录 1 张图，off_sect=0、base_sect=该图的全局起点 → 绝对位置正确。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "DVDA_ASVS_PER_IMAGE"

OLD = """  while ((titleset < naudio_groups) && (title < numtitles[titleset]))
    {
      npics = ntitlepics[titleset][title];
"""

NEW = """"""

LABEL_OLD = """      ++loop;

    }

  uint16_copy(&asvs[0xC], totnumtitles);
"""

LABEL_NEW = """      ++loop;

    }

  asvs_album_done:

  uint16_copy(&asvs[0xC], totnumtitles);
"""


def main():
    p = SRC / "asvs.c"
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    for old, new, what in ((OLD, NEW, "每图一条记录"),
                           (LABEL_OLD, LABEL_NEW, "标签")):
        if t.count(old) != 1:
            print("[FAIL] %s 匹配 %d 次" % (what, t.count(old)))
            return 1
        t = t.replace(old, new, 1)
    if "extern uint16_t totntracks" not in t:
        t = t.replace('#include "asvs.h"',
                      '#include "asvs.h"\n\nextern uint16_t totntracks;', 1)
    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   asvs.c：每图一条记录")
    return 0


if __name__ == "__main__":
    sys.exit(main())
