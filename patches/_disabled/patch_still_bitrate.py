#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图编码加峰值码率上限，把 ASVS 体积压到播放器缓冲以内。

## 依据（2026-09-23 实测确认）

**ASVS 静图总量必须 ≤ 1024 扇区（2 MB）** —— 这是 dvda-author 源码里自带的
限制（`asvs.c`: `if (totpicsectors > 1024)` 警告
"Exceeding stillpic buffer limit (2 MB)"），也是播放器的实际缓冲上限。

实测五张盘**完全吻合**：

| 盘 | 静图数 | ASVS 总量 | 结果 |
|---|---|---|---|
| 李娜 | 12 | **436 扇区** | ✅ 出图 |
| P 版（3 专辑小盘） | 17 | **431 扇区** | ✅ **出图（真机确认）** |
| 巴赫 | 18 | **792 扇区** | ✅ 出图 |
| Enigma | 99 | **1950 扇区** | ❌ 用户确认有静图问题 |
| G 版 | 56 | **1424 扇区** | ❌ **完全不出图** |

→ 超 1024 的两张全部异常，低于 1024 的三张全部正常。
（早期误以为「1 条记录就没事」、又误以为「2048 扇区上限」，都不对。）

## 实测各码率下单图扇区数

720×576 静图，含 mplex 导航扇区（已通过 mplex 实测，非推算）：

    默认    25 扇区/图    56 图 = 1400  ❌
    -b 8000 27             56 图 = 1512  ❌
    -b 6000 21             56 图 = 1176  ❌
    -b 5000 18             56 图 = 1008  ⚠ 卡线
    -b 4500 17             56 图 =  952  ✅ 余 7%
    -b 4000 16             56 图 =  896  ✅ 余 12%
    -b 3000 14             56 图 =  784  ✅

## 为什么不靠 JPEG 质量

`make_still` 已用 `-quality 90`；实测把 JPEG 质量降到 60~80 对最终
MPEG-2 体积**几乎没影响**（mpeg2enc 的输出由码率控制决定）：

    jpeg q90 → 45634 B (23 扇区)      jpeg q60 → 49318 B (25 扇区)

真正有效的是 mpeg2enc 的 `-b`（峰值码率）。

## 改法

`menu.c` 里静图与菜单背景共用同一组 mpeg2enc 参数。只在
`img->action == STILLPICS`（播放静图）时加 `-b 4500`，菜单背景保持原样。

⚠️ **盘1（91 图）需要注意**：91 × 17 = 1547 扇区，仍超 1024。
盘1 若也要能出图，需降到每图 ≤ 11 扇区（约 `-b 2000`），或减少图数。
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
  static char still_peak_bps[] = "4500";
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
    print("[OK]   menu.c: 静图 mpeg2enc 加 -b 4500（菜单背景不变）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
