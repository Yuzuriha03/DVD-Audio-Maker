#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""补写 `ATS_PTT_SRPT`（Part Of Title Search Pointer Table），让「下一曲 / 上一曲」
能按曲目推进。

## 症状与推断

PowerDVD 里：曲目号显示 `0/56`，按「下一曲」变成 `1/56`，**但音频不移动**
（同一首歌）。而**直接选曲能跳对**（无论是本工具链的菜单跳转，还是播放器
自己的曲目列表）。

这两件事在 DVD 里走的是不同机制：

- **直接选曲** = 显式寻址 —— 读 ATSI 里每轨的 PTS 表与扇区表。
  我们有这两张表，且内容正确，所以能跳对。
- **「下一曲」** = 跳到下一个 **PTT（Part Of Title）** ——
  这正是 DVD-Video 里「下一章」的定义，靠 PTT 表解析。

而 `ATSI_MAT` 的 `ATS_PTT_SRPT` 字段（偏移 `0xC8`）**dvda-author 从来不写**，
恒为 0 —— 实测成品盘：`ats_pgcit = 1`，而 `ATS_PTT_SRPT = 0`。
于是播放器找不到任何「分曲点」，只能把整个 title 当一个单元：
曲目号会推进，但寻址总是回到第 1 个。

## 表结构

与 DVD-Video 的 `VTS_PTT_SRPT` 同构（DVD-Audio 沿用同一套导航模型）：

```
+0x00  u16  nr_of_srpts            本 titleset 内的 title 数
+0x02  u16  zero
+0x04  u32  last_byte             表内最后一个字节的偏移（相对本表起点）
+0x08  u32  ttu_offset[nr_of_srpts]   每个 title 的 TTU 偏移（相对本表起点）

TTU（每个 title 一个）：
+0x00  u16  nr_of_ptts           该 title 内的曲目数
+0x02  u16  zero
+0x04  { u16 pgcn; u16 pgn; }[nr_of_ptts]   每项 4 字节
             pgcn = PGC 号（本工具链每组只有一个 PGC，故恒为 1）
             pgn  = program 号（= 曲目号）
```

即「第 n 首」被显式映射到 `(PGC 1, program n)`。这与本工具链现在的结构
（一个 PGC、N 个 cell、每个 cell 一首歌）正好对应。

## 放在哪里、以及为什么不会破坏别的字段

表写在 ATSI 文件里 **PGCI 之后的扇区对齐位置**，并把 `atsi[0xC8]` 填成该
扇区号。写完把 `i` 推进到表末尾，后面的
`*atsi_sectors = ceil(i/2048)` 会**自动**把 ATSI 撑大 —— 而
`atsi[196]`（AOB 起始扇区）、`atsi[28]`（ATSI 末扇区）、`atsi[12]`
（ATS 末扇区）全都由 `*atsi_sectors` 派生，所以布局会自动跟着调整。

## 状态

⚠️ **实验性**：这是对「下一曲走 PTT 表」这一推断的实现，尚未在真机确认。
若无效，从 `build_dvda_author_mlp.sh` 的补丁列表里去掉本脚本即可回退
（`[1b]` 会把 `atsi2.c` 从原始源码还原）。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/atsi2.c")

MARK = "ATS_PTT_SRPT"

OLD = """  // Pointer to following data

  uint32_copy(&atsi[0x0804], i - 0x801);
"""

NEW = """  // Pointer to following data

  uint32_copy(&atsi[0x0804], i - 0x801);

  /* ---- ATS_PTT_SRPT：把「第 n 首」映射到 (PGC, program) ----

     这是播放器解析「下一曲 / 上一曲」（= 下一个/上一个 Part Of Title）
     唯一的依据，而原版**从不写这张表**（atsi[0xC8] 恒为 0），于是播放器
     只能把整个 title 当一个单元：曲目号会推进，但寻址总是回到第 1 个。

     结构（与 DVD-Video 的 VTS_PTT_SRPT 同构）：
       +0x00 u16 nr_of_srpts     +0x02 u16 zero     +0x04 u32 last_byte
       +0x08 u32 ttu_offset[nr_of_srpts]
       TTU: +0x00 u16 nr_of_ptts +0x02 u16 zero
            +0x04 { u16 pgcn; u16 pgn; }[nr_of_ptts]

     表放在 PGCI 之后的扇区对齐位置；i 推进到表末尾，后面的
     *atsi_sectors 计算会自动把 ATSI 撑大（AOB 起始扇区等都由它派生）。 */
  {
    uint32_t poff = (uint32_t) (((i + 2047) / 2048) * 2048);
    uint32_t p = poff + 8;                    /* 头部 8 字节，last_byte 稍后回填 */
    uint32_t offs = p;
    int t, r;

    p += 4 * (uint32_t) numtitles;            /* ttu_offset[] */

    for (t = 0; t < numtitles; ++t)
      {
        uint32_copy(&atsi[offs + 4 * t], p - poff);
        uint16_copy(&atsi[p], (uint16_t) ntitletracks[t]);   /* nr_of_ptts */
        p += 4;
        for (r = 0; r < ntitletracks[t]; ++r)
          {
            uint16_copy(&atsi[p], 1);                        /* pgcn = 1 */
            uint16_copy(&atsi[p + 2], (uint16_t) (r + 1));   /* pgn = 曲目号 */
            p += 4;
          }
      }

    uint16_copy(&atsi[poff], (uint16_t) numtitles);
    uint32_copy(&atsi[poff + 4], p - poff - 1);              /* last_byte */
    uint32_copy(&atsi[0xC8], poff / 2048);                   /* ats_ptt_srpt */

    i = (int) p;
  }
"""


def main():
    if not PATH.exists():
        print("[FAIL] 找不到 %s" % PATH)
        return 1
    text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in text:
        print("[SKIP] %s 已包含 ATS_PTT_SRPT" % PATH.name)
        return 0
    if text.count(OLD) != 1:
        print("[FAIL] %s: 锚点匹配 %d 次（应为 1）" % (PATH.name, text.count(OLD)))
        return 1
    PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                    errors="surrogateescape")
    print("[OK]   %s：写入 ATS_PTT_SRPT（每轨映射到 PGC1/program N）"
          % PATH.name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
