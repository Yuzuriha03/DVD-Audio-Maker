#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ASVS 的 `video_attr`（`AUDIO_SV.IFO` 偏移 `0x18`）改成 **NTSC**。

## 字段含义（2026-09-23 实测确认）

`0x43` 与 `0x53` **只差 bit4**，而 bit4 正是 `video_format`：

    0x43 = 0100 0011   MPEG-2 / video_format=0 (NTSC) / 4:3
    0x53 = 0101 0011   MPEG-2 / video_format=1 (PAL)  / 4:3
                       ↑ bit4 = video_format

与 AMG 的 `amgm_video_attr`（`AUDIO_TS.IFO` 偏移 `0x100`）同一套编码。

## 四张盘的取值（与静图实际制式 4/4 吻合）

| 盘 | 静图实际码流 | ASVS `0x18` | AMG `0x100` |
|---|---|---|---|
| 巴赫 | 720x480 29.97 NTSC | `0x43` | `00`（该盘无菜单） |
| 李娜 | 720x480 29.97 NTSC | `0x43` | `0x43` |
| Enigma | 720x576 25 PAL | `0x53` | `0x53` |
| 本工程（改前） | 720x576 25 PAL | `0x53` | `0x53` |

## 为什么现在改成 0x43

本补丁配合 `patch_still_ntsc.py`：**静图编码改用 NTSC（720x480）**，
所以 ASVS 的 `video_attr` 也必须声明 NTSC —— 声明与实际必须一致。

依据：两张从不出问题的商业盘（巴赫/李娜）静图都是 NTSC；PAL 的 Enigma
静图有缺陷、本工程 PAL 静图在上一段/下一段上异常。其它所有变量都已逐个
实测排除（见 `patch_still_ntsc.py` 的完整清单）。

⚠️ 上游 `dvda-author` 原本硬编码 `0x53`（PAL，见 `asvs.c` 的
`asvs[0x18] = 0x53; // unknown, or 0x43` —— 作者也不知道这是视频属性）。
本补丁把它改成与静图实际制式匹配的值。

**菜单画面的制式不受影响** —— 那由 AMG 的 `amgm_video_attr`（`0x100`）声明，
仍是 `config.sh` 设定的制式（PAL 时为 `0x53`）。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "ASVS video_attr: NTSC"

OLD = """  asvs[0x18] = 0x53; // unknown, or 0x43
"""

NEW = """  /* ASVS video_attr: NTSC MPEG-2 4:3（见 patch_asvs_video_attr.py）。

     0x43 = 0100 0011 → video_format=0 (NTSC)
     0x53 = 0101 0011 → video_format=1 (PAL)，只差 bit4

     与静图实际制式必须一致：本工程静图已改用 NTSC 720x480
     （见 patch_still_ntsc.py）。上游硬编码 0x53，作者注释自称 "unknown"。 */
  asvs[0x18] = 0x43; // ASVS video_attr: NTSC MPEG-2 4:3
"""


def main():
    p = SRC / "asvs.c"
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
    p.write_text(t.replace(OLD, NEW, 1), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   asvs.c: 0x18 → 0x43（ASVS video_attr = NTSC）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
