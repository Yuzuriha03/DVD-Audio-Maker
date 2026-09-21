#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""方案 B：把「播放封面」的表从**按 title** 改成**按轨**。

## 症状

合并成一个 title 之后（`patch_mlp_one_title.py`），**不管播哪一首**都只显示
**第一个专辑**的封面（`patch_stillpics_atsi_record.py` 的说明里记了这条）。

## 根因（两张表都按 title 组织，而播放器按「当前轨」查表）

- `AUDIO_SV.IFO`（ASVS）的记录是**每个 title 一条**：
  `图数(u8) + 起始图号(u16) + 起始扇区(u32)`，加上 `0x378` 起每图 2 字节的
  扇区表。原来「每轨自成一个 title」时，**一条记录恰好对应一轨**。
- ATSI 的静图记录首字节是「**第几条记录**」，由 `pictitlecount`
  （「第几个有图的 **title**」）给出。

原来两者都是「按 title」，而 title == track，所以一一对应。
合并成一个 title 后：ASVS 只写出 **1 条**记录，`pictitlecount` 恒为 **1**
—— 于是所有轨都引用第 1 条记录 → 全显示第一张图。

## 修法（只把迭代粒度从 title 换成 track，不动任何未知结构）

| 文件 | 原来 | 改成 |
|---|---|---|
| `asvs.c` | 每个 **title** 写一条记录 | 每个**有图的轨**写一条 |
| `atsi2.c` | `pictitlecount` 按 **title** 累加 | 按**有图的轨**累加 |

**与原来完全兼容**：一 title 一轨时，两者输出的字节与原来完全一致。

这样 ASVS 会写出 17 条记录（17 个专辑各一张图），而 ATSI 引用 1..17，
**都落在表内** —— 这也是上一轮崩溃的原因：那次我把 ATSI 的引用改成 1..17，
但 ASVS 仍然只有 1 条记录，引用 2..17 就越界了。
**本次两边一起改，引用与表项数必然一致。**

## 开关

由环境变量 `DVDA_ASVS_PER_TRACK` 控制（见 `build-one.sh` / `env.sh`）：

    DVDA_ASVS_PER_TRACK=1 ... 02_build.py     # 开启（本次要试的）
    不设置                                     # 完全保持原行为

不需要重新编译就能切换，失败可秒回退。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "DVDA_ASVS_PER_TRACK"

# ------------------------------------------------------------------ asvs.c
ASVS_EXTERN_OLD = """#include "asvs.h"
"""

ASVS_EXTERN_NEW = """#include "asvs.h"

/* 总轨数（见 command_line_parsing.c）。用于「每轨一条静图记录」模式 ——
   原来按 title 记录时用不到它，因为 title == track。 */
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

ASVS_NEW = """  if (getenv("DVDA_ASVS_PER_TRACK"))
    {
      /* 每「有图的轨」一条记录（原来是每 title 一条）。

         播放器是按**当前轨**去查这张表的，而按 title 分时，合并成一个
         title 后只会写出 1 条记录 → 所有轨都查到第 1 张图（也就是
         「不管播哪首都显示第一个专辑的封面」）。

         改为按轨后：轨 1(专辑1 首曲) → 记录 1，轨 7(专辑2 首曲) → 记录 2 …
         而同专辑后续的轨自身没有图、ATSI 那边沿用上一条，行为正合预期。

         一 title 一轨时（旧布局）与下面原循环输出的字节完全一致。 */
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
  else
    {
      while ((titleset < naudio_groups) && (title < numtitles[titleset]))
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
    }
"""

# ------------------------------------------------------------------ atsi2.c
ATSI_INC_OLD = """          if (ntitlepics[j])
            ++pictitlecount;
"""

ATSI_INC_NEW = """          /* DVDA_ASVS_PER_TRACK 时改为按「有图的轨」累加（见下面轨循环）；
             否则维持原来的按「有图的 title」累加。 */
          if (!getenv("DVDA_ASVS_PER_TRACK"))
            {
              if (ntitlepics[j])
                ++pictitlecount;
            }
"""

ATSI_USE_OLD = """              if (pictitlecount == 0)
                continue;

              // title-with-pics rank (1-based)

              atsi[i++] = pictitlecount;
"""

ATSI_USE_NEW = """              /* 开了 DVDA_ASVS_PER_TRACK 时按**轨**累加：本轨有自己的图
                 就推进到下一条 ASVS 记录（它同样按轨写）。这与 asvs.c 的
                 改动**必须成对**：引用值与记录条数不一致就会越界、播放器崩。 */
              if (getenv("DVDA_ASVS_PER_TRACK")
                  && img->npics && img->npics[trackcount - 1])
                ++pictitlecount;

              if (pictitlecount == 0)
                continue;

              // title-with-pics rank (1-based)

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
        print("[SKIP] 已应用过（asvs.c 含 %s）" % MARK)
        return 0
    if not apply(SRC / "asvs.c",
                 [(ASVS_EXTERN_OLD, ASVS_EXTERN_NEW, "声明 extern totntracks"),
                  (ASVS_OLD, ASVS_NEW, "按轨写静图记录（带 DVDA_ASVS_PER_TRACK 开关）")]):
        return 1
    if not apply(SRC / "atsi2.c",
                 [(ATSI_INC_OLD, ATSI_INC_NEW, "按 title 累加改为受开关控制"),
                  (ATSI_USE_OLD, ATSI_USE_NEW, "按有图的轨累加引用值")]):
        return 1
    print("\n方案 B 补丁完成（用 DVDA_ASVS_PER_TRACK=1 启用）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
