#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ASVS 调色板（0x20 起 64 字节）改成商业盘的值：`00 10 10 10` × 16。

## 依据（四盘实测）

| 盘 | 0x20 起 16 组 | 封面 |
|---|---|---|
| 巴赫（1 title / 18 轨）| **`00 10 10 10` × 16（全同）** | ✔ 正常 |
| 李娜（1 记录 / 24 轨）| **`00 10 10 10` × 16（全同）** | ✔ 正常 |
| Enigma（8 记录 / 99 轨）| `00 10 80 80` × 16 | ✔ 正常 |
| 本工程 | `00 e6 80 7f` / `00 00 00 00` / `00 e6 80 7f` / `00 90 22 35` / `00 88 b3 3a` / 其余 0 | ✗ 黑屏 |

上游从命令行 `--active*-palette`（**菜单**那套配色：背景/文字/高亮/选中）
取值写进 ASVS —— 但 ASVS 的调色板服务于**静图显示**，用菜单配色在
PowerDVD 上表现为**黑屏**（画面不显示、也不随曲切换）。

两张「单记录」形态的商业盘（巴赫、李娜）都是 `00 10 10 10` × 16，
即 YUV = (0x10, 0x10, 0x10) 的同一色，16 组全同。本工程对齐它们。

## 改法

`asvs.c` 里原本是 4 个 `uint32_copy`（取 `img->active*-palette`），
外加一段被注释掉的 11 个值。整体替换为循环写 `0x00101010` × 16。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "调色板照商业盘：00101010 × 16"

OLD = """  // This palette is taken as is from a commercial DVD: unselected
  // text (display)
  // Status: probably broken

  uint32_copy(&asvs[0x20],
              (uint32_t) strtoul(img->activebgcolor_palette, NULL, 16));

  //uint32_copy(&asvs[0x24], (uint32_t) 0x80E6807F);

  // album, group headers and highlighted text

  uint32_copy(&asvs[0x28],
              (uint32_t) strtoul(img->activetextcolor_palette, NULL, 16));

  //highlight motif

  uint32_copy(&asvs[0x2C],
              (uint32_t) strtoul(img->activehighlightcolor_palette, NULL, 16));

  // select action text only

  uint32_copy(&asvs[0x30],
              (uint32_t) strtoul(img->activeselectfgcolor_palette, NULL, 16));
"""

NEW = """  /* 调色板照商业盘：00101010 × 16（见 patch_asvs_palette.py）。

     巴赫与李娜（两张「单记录」形态的商业盘，封面均正常）该处 16 组全为
     0x00101010；上游却从命令行 `--active*-palette`（菜单配色）取值，
     实测这样的静图在 PowerDVD 上黑屏。 */
  {
    int pal;
    for (pal = 0; pal < 16; ++pal)
      uint32_copy(&asvs[0x20 + 4 * pal], 0x00101010);
  }
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
        print("[FAIL] 目标段匹配 %d 次（应为 1）" % t.count(OLD))
        return 1
    p.write_text(t.replace(OLD, NEW, 1), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   asvs.c: 调色板 0x20..0x5F = 00 10 10 10 × 16")
    return 0


if __name__ == "__main__":
    sys.exit(main())
