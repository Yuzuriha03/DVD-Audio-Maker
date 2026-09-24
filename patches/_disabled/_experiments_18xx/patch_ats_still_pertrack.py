#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图记录的后两个字段改成**每轨不同**（照巴赫的规律）。

## 依据：巴赫（1 个 title？18 轨）

巴赫是「1 title + 18 轨 + ASVS 单记录（图数=18）」，而它**导航与封面都正常**。
其 ATS 静图表（6 字节/轨）每轨数值都不同：

    轨1: 01 04 00 6c 00 75      → 第2字段 108、第3字段 117
    轨2: 01 04 00 76 00 7f      → 118、127
    轨3: 01 04 00 80 00 89      → 128、137
    ...每轨 +10，第3 = 第2 + 9

即：第2字段 = 6 × 轨数 + 10 × 轨序；第3字段 = 第2 + 9。
（dvda-author 原式把它写成「整轨相同」→ 播放器无法区分当前轨 → 曲目 0。）
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "DVDA_STILL_PERTRACK"

OLD2 = """              uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);
"""

NEW2 = """              /* 第2字段 = 6×轨数 + 10×**轨序**（巴赫式，每轨不同） */
              uint16_copy(&atsi[i],
                          (uint16_t)(0x06 * ntitletracks[j] + 10 * r));
"""

OLD3 = """              uint16_copy(&atsi[i],
                          (ntitletracks[j] - 1) * 0x6
                          + 0x0F + (ntitlepics[j] - 1) * 0xA);
"""

NEW3 = """              /* 第3字段 = 第2 + 9（巴赫式） */
              uint16_copy(&atsi[i],
                          (uint16_t)(0x06 * ntitletracks[j] + 10 * r + 9));
"""


def main():
    p = SRC / "atsi2.c"
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    for old, new, what in ((OLD2, NEW2, "第2字段每轨递增"),
                           (OLD3, NEW3, "第3字段 = 第2+9")):
        if t.count(old) != 1:
            print("[FAIL] %s 匹配 %d 次" % (what, t.count(old)))
            return 1
        t = t.replace(old, new, 1)
    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   atsi2.c：静图记录后两字段改为每轨不同（巴赫式）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
