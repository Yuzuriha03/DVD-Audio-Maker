#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图编码加峰值码率上限，把 ASVS 体积压到播放器缓冲以内。

## 依据（2026-09-23 实测）

PowerDVD 8 的**静图缓冲疑似 2048 扇区（4 MB）**：

| 盘 | ASVS VOB | 每图 | 结果 |
|---|---|---|---|
| 巴赫 | 792 扇区 | 44 | ✅ 18 轨全出图 |
| 李娜 | 436 扇区 | 36 | ✅ 全出图 |
| **Enigma** | **1950 扇区（4 MB 的 95%）** | 19.7 | ✅ 出图（作者显然按 4 MB 预算做的） |
| 盘2（56 轨） | 1424 扇区 | 25.4 | ✅ 全出图 |
| **盘1（91 轨）** | **2281 扇区（超 4 MB 11%）** | 25.1 | ❌ **完全不出图** |

盘1 与盘2 的**结构逐字节同构**（同一构建、同一套表、91 张图全部有效且能解码、
指针全对），唯一差别就是总量越过了缓冲上限。

## 为什么不靠 JPEG 质量

`make_still` 已经用 `-quality 90`；实测把 JPEG 质量降到 60~80 对最终
MPEG-2 体积**几乎没影响**（mpeg2enc 的输出由码率控制决定，不是 JPEG 细节）：

    jpeg q90 → 45634 B (23 扇区)      jpeg q60 → 49318 B (25 扇区)

真正有效的是 mpeg2enc 的 `-b`（峰值码率）：

    -b 4000 → 27498 B (14 扇区)      -b 2500 → 19676 B (10 扇区)
    （默认     → 45634 B (23 扇区)）

## 改法

`menu.c` 里静图与菜单背景共用同一组 mpeg2enc 参数。只在
`img->action == STILLPICS`（播放静图）时加 `-b 3200`，菜单背景保持原样。

目标：91 图 × (12+4≈16 扇区) ≈ 1450 扇区，与**已验证可用**的盘2（1424）相当，
且远低于 2048。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "静图峰值码率上限"

OLD = """  char *argsmpeg2enc[] = {MPEG2ENC_BASENAME,  "-f", "8", "-n", norm,  "-o", tempfile, "-a", img->aspect, NULL};
"""

NEW = """  /* 静图峰值码率上限（见 patch_still_bitrate.py）。

     ASVS 的静图总量必须落在播放器的静图缓冲内 —— 实测 PowerDVD 8 的边界
     在 2048 扇区（4 MB）附近：Enigma 商业盘 1950 扇区可正常出图，
     本工程盘1（91 图 / 2281 扇区）则完全不出图，而盘2（56 图 / 1424 扇区）
     正常。故给静图加峰值码率上限，把每图从 23 扇区压到 12 左右。

     只作用于 STILLPICS；菜单背景（ANIMATEDVIDEO）沿用默认码率。 */
  static char still_peak_bps[] = "3200";
  char *argsmpeg2enc_still[] = {MPEG2ENC_BASENAME, "-f", "8", "-n", norm,
                                "-o", tempfile, "-a", img->aspect,
                                "-b", still_peak_bps, NULL};
  char *argsmpeg2enc_plain[] = {MPEG2ENC_BASENAME,  "-f", "8", "-n", norm,  "-o", tempfile, "-a", img->aspect, NULL};
  char **argsmpeg2enc = (img->action == STILLPICS)
                        ? argsmpeg2enc_still : argsmpeg2enc_plain;
"""


def main():
    p = SRC / "menu.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    if t.count(OLD) != 1:
        print("[FAIL] 目标行匹配 %d 次（应为 1）" % t.count(OLD))
        return 1
    t = t.replace(OLD, NEW, 1)
    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 静图 mpeg2enc 加 -b 3200（菜单背景不变）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
