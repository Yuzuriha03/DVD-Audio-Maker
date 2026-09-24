#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ASVS 头部字段对齐：`0x18` = 视频属性（制式）、`0x0E` = 记录数配套值。

## 0x18 是 ASVS 的 video attribute（四张盘实测，判决性证据）

`0x43` 与 `0x53` **只差 bit4**：

    0x43 = 0100 0011   MPEG-2 / video_format=0 (NTSC) / 4:3
    0x53 = 0101 0011   MPEG-2 / video_format=1 (PAL)  / 4:3
                       ↑ bit4 = video_format

这与 AMG 的 `amgm_video_attr`（`AUDIO_TS.IFO` 偏移 `0x100`）用的是**同一套编码**：

| 盘 | 静图制式 | ASVS `0x18` | AMG `0x100` |
|---|---|---|---|
| 巴赫布兰登堡 | NTSC 720x480 | `0x43` | `00`（该盘无菜单） |
| 李娜精选集   | NTSC 720x480 | `0x43` | `0x43` |
| Enigma       | PAL  720x576 | `0x53` | `0x53` |
| **本工程**   | **PAL 720x576** | **`0x53`** | `0x53` |

→ **本工程是 PAL，`0x18` 必须是 `0x53`。**
dvda-author 原本无条件硬编码 `0x53`，刚好是对的（源码注释写着
`asvs[0x18] = 0x53; // unknown, or 0x43` —— 作者也不知道这是视频属性）。

⚠️ **历史教训（本补丁曾经的错误）**：曾照「单记录形态」的**巴赫**把这个字节
一起抄成了 `0x43`。巴赫是 NTSC，所以它那里 0x43 是对的；本工程是 PAL，
抄过来就成了「**声明 NTSC + 实际 PAL 码流**」→ 播放器按 NTSC 建解码器、
拿到的却是 PAL 序列 → 静默失败 → **黑屏**。
「单记录形态」只适用于 `0x0E`，**制式必须跟着自己的静图走，不能连制式一起抄**。

## 0x0E

`0x0E` 与记录数（`0x0C`）成套：

| 盘 | 记录数 `0x0C` | `0x0E` |
|---|---|---|
| Enigma | 8 | `0x0012` |
| 李娜 / 巴赫 | 1 | `0x0000` |

本工程 1 条记录，取 `0x0000`。
（dvda-author 无条件写 `0x0012`，那是 Enigma 的值。）

## 改法

    asvs[0x18] : 保持 0x53（PAL —— 对本工程本来就正确，不动）
    asvs[0x0E] : 0x0012 → 0x0000

## 幂等 / 纠错

本补丁会**纠正已处于错误状态**的源码（曾把 0x18 写成 0x43 的版本），
重跑即自动修好，不需要手工回滚。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")

MARK_V2 = "0x18 是 ASVS 的视频属性"
# 曾错误写出的版本的指纹（注意源文件里带 ** 记号）
MARK_V1 = "按**单条记录**形态的商业盘取值"

# 原始（未打过本补丁）
OLD = """  uint16_copy(&asvs[0xE], 0x0012);  // DVD Spec
  asvs[0x13] = 2; // unknown
  asvs[0x18] = 0x53; // unknown, or 0x43
"""

# 曾经错误地写出的版本（把巴赫的 NTSC 制式一起抄了过来）
BAD_V1 = """  /* 按**单条记录**形态的商业盘取值（见 patch_asvs_header_mode.py）。

     `0x0C`（记录数）与这两个字段成套：
       多记录（Enigma 8 条）        : 0x0E=0x0012 0x18=0x53
       单记录（李娜/巴赫，各 1 条）: 0x0E=0x0000 0x18=0x43
     dvda-author 无条件写 0x0012/0x53（那两个值是 Enigma 的），
     「单记录」的盘拿到「多记录」的头 → 播放器判错：静图集不随曲切换、
     封面只认第一张、UI 里还多出「上一张/下一张幻灯片」。
     本工程的静图集与巴赫同形，故对齐巴赫。 */
  uint16_copy(&asvs[0xE], 0x0000);
  asvs[0x13] = 2; // unknown
  asvs[0x18] = 0x43; // unknown, or 0x53
"""

# 正确版本
NEW = """  /* 按**单条记录**形态的商业盘取值（见 patch_asvs_header_mode.py）。

     `0x0E` 与记录数（0x0C）成套：
       多记录（Enigma 8 条）        : 0x0E=0x0012
       单记录（李娜/巴赫，各 1 条）: 0x0E=0x0000
     本工程 1 条记录，故取 0x0000。

     `0x18` 是 ASVS 的视频属性（video_attr）：
       0x43 = 0100 0011  MPEG-2 / video_format=0 (NTSC) / 4:3
       0x53 = 0101 0011  MPEG-2 / video_format=1 (PAL)  / 4:3
     实测：巴赫(NTSC)=0x43、李娜(NTSC)=0x43、Enigma(PAL)=0x53。
     **本工程静图是 PAL 720x576，故必须保持 0x53** ——
     曾经照巴赫（NTSC）抄成 0x43，等于「声明 NTSC + 实播 PAL」，播放器黑屏。
     「单记录形态」只适用于 0x0E，制式不能跟着抄。 */
  uint16_copy(&asvs[0xE], 0x0000);
  asvs[0x13] = 2; // unknown
  asvs[0x18] = 0x53; // ASVS video_attr: PAL MPEG-2 4:3（跟着本工程静图制式走）
"""


def main():
    p = SRC / "asvs.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")

    if MARK_V2 in t:
        print("[SKIP] 已应用过（0x18=0x53 PAL、0x0E=0x0000）")
        return 0

    if t.count(BAD_V1) == 1:
        # 纠正曾把 0x18 抄成 0x43(NTSC) 的版本
        p.write_text(t.replace(BAD_V1, NEW, 1), encoding="utf-8",
                     errors="surrogateescape")
        print("[OK]   纠正：asvs.c 0x18 0x43(NTSC) → 0x53(PAL)")
        return 0

    if MARK_V1 in t:
        print("[FAIL] 处于错误版本状态，但与 BAD_V1 不逐字符相符，请人工检查")
        return 1

    if t.count(OLD) != 1:
        print("[FAIL] 原始模式匹配 %d 次（应为 1）" % t.count(OLD))
        return 1
    p.write_text(t.replace(OLD, NEW, 1), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   asvs.c: 0x0E → 0x0000，0x18 保持 0x53 (PAL)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
