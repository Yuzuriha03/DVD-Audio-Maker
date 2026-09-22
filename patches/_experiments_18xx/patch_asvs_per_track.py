#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""方案 B：把「播放封面」的表从**按 title** 改成**按轨**。

## 症状

一个音频组里有多轨（现在是「一专辑一个 title」，组内若干轨）时，
**不管播哪一首**都只显示**第一个专辑**的封面。

## 根因（两张表都按 title 组织，而播放器按「当前轨」查表）

- `AUDIO_SV.IFO`（ASVS）的记录是**每个 title 一条**：
  `图数(u8) + 起始图号(u16) + 起始扇区(u32)`，加上 `0x378` 起每图 2 字节的
  扇区表。当「每轨自成一个 title」时，**一条记录恰好对应一轨**。
- ATSI 的静图记录首字节是「**第几条记录**」，由 `pictitlecount`
  （「第几个有图的 **title**」）给出。

原来两者都是「按 title」，而 title == track，所以一一对应。
一个 title 装多轨后：ASVS 只写出 **1 条**记录，`pictitlecount` 恒为 **1**
—— 于是所有轨都引用第 1 条记录 → 全显示第一张图。

## 修法（只把迭代粒度从 title 换成 track，不动任何未知结构）

| 文件 | 原来 | 改成 |
|---|---|---|
| `asvs.c` | 每个 **title** 写一条记录 | 每个**有图的轨**写一条 |
| `atsi2.c` | `pictitlecount` 按 **title** 累加 | 按**有图的轨**累加 |

**与原来完全兼容**：一 title 一轨时，两者输出的字节与原来完全一致
（那时「有图的轨」就是「有图的 title」）。

这样 ASVS 会写出 N 条记录（N = 有图的轨数），而 ATSI 引用 1..N，
**都落在表内**。两边**必须成对改**：只改 ATSI 的引用会让它越界、
播放器直接崩。

## 为什么没有开关

早先本补丁用环境变量 `DVDA_ASVS_PER_TRACK` 包着，不设置就走原行为。
既然改动在两种布局下都正确（见上面的兼容性论证），那个开关只是多余的
一层，已删除 —— 本补丁现在**无条件生效**。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "每「有图的轨」一条记录"

# ------------------------------------------------------------------ asvs.c
ASVS_EXTERN_OLD = """#include "asvs.h"
"""

ASVS_EXTERN_NEW = """#include "asvs.h"

/* 总轨数（定义见 command_line_parsing.c）。静图记录按**轨**写，
   所以需要它 —— 按 title 写时用不到（那时 title == track）。 */
extern uint16_t totntracks;
"""

ASVS_OLD = """  while ((titleset < naudio_groups) && (title < numtitles[titleset]))
    {
      npics = ntitlepics[titleset][title];
      if (npics)
        {
          asvs[k] = npics;
          k += 2;
          uint16_copy(&asvs[k], pict + 1); // 1-based
          pict += npics;
          k += 2;
          uint32_copy(&asvs[k], totpicsectors);
          for (j = 0; j < npics; ++j)
            {
              if (j)
                uint16_copy(&asvs[t], totpicsectors);
              t += 2;

              //pict [] is 0-based: pict[0] for first track

              totpicsectors += img->stillpicvobsize[index + j];
              if (totpicsectors > 1024)
                foutput(ERR "Exceeding stillpic buffer limit (2 MB) \\
at pict #%d.\\n", j);
            }
          k += 4;
          index += npics;
          ++totnumtitles;
        }

      ++title;

      if (title == numtitles[titleset])
        {
          ++titleset;
          title = 0;
        }
      ++loop;

    }
"""

ASVS_NEW = """  /* 每「有图的轨」一条记录（原来是每 title 一条）。

     播放器是按**当前轨**去查这张表的；一个 title 里有多轨时，按 title
     只会写出 1 条记录 → 所有轨都查到第 1 张图（即「不管播哪首都显示
     第一个专辑的封面」）。

     改为按轨后：专辑首曲那一轨 → 记录 1，下一个专辑首曲那一轨 → 记录 2 …
     而同专辑后续的轨自身没有图、ATSI 那边沿用上一条，行为正合预期。

     一 title 一轨时（即 title == track）与原来输出的字节完全一致，
     所以不需要任何开关。 */
  {
    uint16_t tt;
    for (tt = 0; tt < totntracks; ++tt)
      {
        npics = (img->npics) ? img->npics[tt] : 0;
        if (npics)
          {
            asvs[k] = (uint8_t) npics;
            k += 2;
            uint16_copy(&asvs[k], pict + 1); // 1-based
            pict += npics;
            k += 2;
            uint32_copy(&asvs[k], totpicsectors);
            for (j = 0; j < npics; ++j)
              {
                if (j)
                  uint16_copy(&asvs[t], totpicsectors);
                t += 2;

                /* pict[] 是 0-based，index 指向本轨第一张图 */
                totpicsectors += img->stillpicvobsize[index + j];
                if (totpicsectors > 1024)
                  foutput(ERR "Exceeding stillpic buffer limit (2 MB) \\
at pict #%d.\\n", j);
              }
            k += 4;
            index += npics;
            ++totnumtitles;
          }
      }
  }
"""

# ------------------------------------------------------------------ atsi2.c
# 原来在进入轨循环**之前**按「本 title 有图」累加一次；改成按轨计数后
# 这步必须去掉，累加移进循环内、按「本轨有图」做。
ATSI_INC_OLD = """          if (ntitlepics[j])
            ++pictitlecount;
"""

ATSI_INC_NEW = """          /* pictitlecount 改在下面的轨循环里、按「有图的轨」累加
             （见 patch_asvs_per_track.py）。此处不能再按 title 加，
             否则计数会与 ASVS 那张按轨写的表错位。 */
"""

ATSI_USE_OLD = """              if (pictitlecount == 0)
                continue;

              // title-with-pics rank (1-based)

              atsi[i++] = pictitlecount;
"""

ATSI_USE_NEW = """              /* 本轨有自己的图 → 推进到下一条 ASVS 记录（它同样按轨
                 写）。这与 asvs.c 的改动**必须成对**：引用值与记录条数
                 不一致就会越界、播放器崩。 */
              if (img->npics && img->npics[trackcount - 1])
                ++pictitlecount;

              if (pictitlecount == 0)
                continue;

              // ASVS 记录序号 (1-based)

              atsi[i++] = pictitlecount;
"""


def apply(path, pairs):
    if not path.exists():
        print("[FAIL] 找不到 %s" % path)
        return False
    text = path.read_text(encoding="utf-8", errors="surrogateescape")
    for old, new, what in pairs:
        n = text.count(old)
        if n != 1:
            print("[FAIL] %s: %s 匹配 %d 次（应为 1）" % (path.name, what, n))
            return False
        text = text.replace(old, new, 1)
    path.write_text(text, encoding="utf-8", errors="surrogateescape")
    for _o, _n, what in pairs:
        print("[OK]   %s：%s" % (path.name, what))
    return True


def main():
    sh = SRC / "asvs.c"
    if sh.exists() and MARK in sh.read_text(encoding="utf-8",
                                            errors="surrogateescape"):
        print("[SKIP] 已应用过（asvs.c 含「%s」）" % MARK)
        return 0
    if not apply(SRC / "asvs.c",
                 [(ASVS_EXTERN_OLD, ASVS_EXTERN_NEW, "声明 extern totntracks"),
                  (ASVS_OLD, ASVS_NEW, "静图记录改为按轨写")]):
        return 1
    if not apply(SRC / "atsi2.c",
                 [(ATSI_INC_OLD, ATSI_INC_NEW, "去掉「按 title」累加"),
                  (ATSI_USE_OLD, ATSI_USE_NEW, "改为「按有图的轨」累加引用值")]):
        return 1
    print("\n方案 B 补丁完成（无条件生效，无开关）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
