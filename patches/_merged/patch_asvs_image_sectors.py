#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 ASVS 的「每图偏移表」：必须是**相对本 title 起点**、每个 title 归零。

## 实测三张商业盘（2026-09-22）

`AUDIO_SV.IFO` 偏移 `0x378` 起是 2 字节/图的偏移表，**按 title 分段**，
每段第一项恒为 0，其后是该图**相对本 title 起点**的扇区偏移：

    Enigma:
      title1（15 图，每图 23 扇区）: 0  23  46  69 ... 322
      title2（12 图，每图 20 扇区）: 0  20  40  60 ... 220     ← 重新从 0 起
      title3（12 图，每图 19 扇区）: 0  19  38  57 ... 209     ← 又归零

各段长度恰好 = 该 title 的图数（= 轨数）。

## 我们的错在哪

`asvs.c` 里 `totpicsectors` 是**全局累加**的（ASVS 记录里的「起始扇区」
必须是全局绝对值，所以它不能改）。但它被**同一个变量**拿去写这张表：

    if (j)                                  /* 还跳过了 j==0 */
      uint16_copy(&asvs[t], totpicsectors); /* 写的是全局累计值 */

于是写出「全局绝对扇区、且从不归零」的一条平铺表：

    我们: 0 26 52 78 104 130 156 181 206 ... 1398

后果：只有 **title1**（起点恰好是 0）的取值与「相对偏移」碰巧一致；
title2 之后全部偏大 → 播放器按「图号 + 偏移」去 VOB 取图时越界/取错，
表现为**只有 title1 的封面能显示，后面的全都没有**（实测现象）。

## 修法

新增**每 title 归零**的计数器 `titlesectors`：

  · 表里写 `titlesectors`（先写后加）→ 首项自然为 0，其后为相对偏移；
  · `totpicsectors` 保持全局累加，继续供 ASVS 记录的「起始扇区」与
    文件末尾的「总扇区」使用（这两处**必须**是全局值）。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "base_sect+off_sect（商业盘形态）"

DECL_OLD = """  uint16_t pict = 0, npics = 0, totnumtitles = 0, k, j, t;
"""

DECL_NEW = """  uint16_t pict = 0, npics = 0, totnumtitles = 0, k, j, t;
  /* 本 title 内归零的相对偏移（见 patch_asvs_image_sectors.py）。
     totpicsectors 是全局累计，用作 base_sect；两者不能混用。 */
  uint16_t titlesectors = 0;
"""

LOOP_OLD = """      npics = ntitlepics[titleset][title];
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
"""

LOOP_NEW = """      npics = ntitlepics[titleset][title];
      if (npics)
        {
          asvs[k] = npics;
          k += 2;
          uint16_copy(&asvs[k], pict + 1); // 1-based
          pict += npics;
          k += 2;
          /* base_sect = 本 title 的**全局起始扇区**（商业盘写法）。

             规范里绝对位置 = base_sect + off_sect。因此：
               · base_sect 写本 title 在 AUDIO_SV.VOB 里的全局起点
               · 每图的 off_sect 写**本 title 内的相对偏移**（首图恒为 0）
             这正是 Enigma/李娜/巴赫 的形态：
                 Enigma title1: base=0,   off = 0 23 46 ... 322
                 Enigma title2: base=345, off = 0 20 40 ... 220
             （此前写成了「每 title 归零的相对表」，等价于把 base_sect 当 0
               且 off 用相对值 —— 绝对位置全错，只有 title1 碰巧正确。） */
          uint32_copy(&asvs[k], totpicsectors);

          titlesectors = 0;
          for (j = 0; j < npics; ++j)
            {
              uint16_copy(&asvs[t], titlesectors);
              t += 2;

              //pict [] is 0-based: pict[0] for first track

              totpicsectors += img->stillpicvobsize[index + j];
              titlesectors  += (uint16_t) img->stillpicvobsize[index + j];
              if (totpicsectors > 1024)
                foutput(ERR "Exceeding stillpic buffer limit (2 MB) \\
at pict #%d.\\n", j);
            }
"""


def apply(path, pairs):
    if not path.exists():
        print("[FAIL] 找不到 %s" % path)
        return False
    t = path.read_text(encoding="utf-8", errors="surrogateescape")
    for old, new, what in pairs:
        n = t.count(old)
        if n != 1:
            print("[FAIL] %s: %s 匹配 %d 次（应为 1）" % (path.name, what, n))
            return False
        t = t.replace(old, new, 1)
    path.write_text(t, encoding="utf-8", errors="surrogateescape")
    for _o, _n, what in pairs:
        print("[OK]   %s：%s" % (path.name, what))
    return True


def main():
    p = SRC / "asvs.c"
    if p.exists() and MARK in p.read_text(encoding="utf-8",
                                          errors="surrogateescape"):
        print("[SKIP] 已应用过")
        return 0
    ok = apply(p, [(DECL_OLD, DECL_NEW, "新增每 title 归零的 titlesectors"),
                   (LOOP_OLD, LOOP_NEW,
                    "base_sect=本title全局起点、off_sect=本title内相对（照抄商业盘）")])
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
