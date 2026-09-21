#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 ATSI 静图记录里的**图号**：必须按「轨」累计，而不是按「title」计数。

## 症状

不管播放哪一首，都显示**第一个专辑**的封面。

## 根因

ATSI 每条静图记录的第 1 个字节是「图号」，而 dvda-author 写的是

    atsi[i++] = pictitlecount;      // 「第几个有图的 title」，按 title 计数

`AUDIO_SV.IFO` 那边记的是**全局**图号：

    uint16_copy(&asvs[k], pict + 1);   // pict 跨 title 累计 → 全局图号
    pict += npics;

原来「每轨自成一个 title」（28 个 title、每个 1 张图）时两者**恰好相等**：
title k 的 `pictitlecount` = k+1，全局图号也是 k+1。

合并成一个 title 之后 `pictitlecount` 恒为 **1**，于是 **56 条记录的图号全是
1**（实测），所有轨都指向第 1 张图 → 不管播哪首都是第一个专辑的封面。

这与「下一曲跳回第 1 首」是同一类错误：**把「按 title 计数」当成了「按轨」**。

## 修法

图号改为「该轨第一张图的**全局**序号」：

    图号 = s + （本 title 内已经消耗过的图数）

`s` 就是 dvda-author 自己维护的「前面所有 title 的图数累计」（同一条循环里
已经在用它取 `img->options[s]`），本 title 内的消耗量另计。

**这个式子与原来完全兼容**：一 title 一轨一图时 `s = k`、本 title 内消耗 1，
于是图号 = k+1 = 原来的 `pictitlecount`。

本轨没有自己的图（`img->npics[轨] == 0`，即 `--stillpics` 的空项 =
「沿用上一张」）时，沿用上一轨算出的图号；至今一张图都没有则整条跳过。

原来那个跳过条件 `if ((ntitlepics[j] == 0) && (img->npics[轨] == 0)) continue;`
在「一轨一 title」时永不触发；现在按**轨**判断（`npics` 为 0 就是没有自己的
图）。`pictitlecount` 仍照旧累加（只用于 veryverbose 日志）。

## 状态

产物已被验证：盘2 的 56 条记录图号从「全是 1」变成按轨递增
（1,2,3,…,17 —— 正好对应 17 个专辑）。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/atsi2.c")

MARK = "图号 = 该轨第一张图的**全局**序号"

DECL_OLD = """          uint16_t r, u = 0,  trackcount_save = trackcount;
          s += (j) ? ntitlepics[j - 1]  : 0;
"""

DECL_NEW = """          uint16_t r, u = 0,  trackcount_save = trackcount;
          /* 本 title 内已消耗的图数（加上 s 即当前轨第一张图的全局序号）；
             picnum 记住当前生效的图号，供「本轨没有自己的图」时沿用。 */
          uint16_t pics_consumed = 0, picnum = 0;
          s += (j) ? ntitlepics[j - 1]  : 0;
"""

OLD = """          for (r = 0; r < ntitletracks[j]; ++r)
            {
              ++trackcount;

              //  This might be taken off in some unclear cases.

              if ((ntitlepics[j] == 0) && (img->npics[trackcount - 1] == 0))
                continue;

              // title-with-pics rank (1-based)

              atsi[i++] = pictitlecount;
"""

NEW = """          /* 图号 = 该轨第一张图的**全局**序号 = s + 本 title 内已消耗的图数。
             `s` 是 dvda-author 自己维护的「前面所有 title 的图数累计」
             （下面 img->options[s] 就是靠它索引）。
             ⚠️ 原式写的是 pictitlecount（「第几个有图的 title」）：原来每轨
             自成一个 title 时两者恰好相等，合并成一个 title 后它恒为 1 ——
             于是所有轨都指向第 1 张图，不管播哪首都是第一个专辑的封面。 */
          for (r = 0; r < ntitletracks[j]; ++r)
            {
              ++trackcount;

              /* 本轨自己的图数；0 = 沿用上一张封面（--stillpics 的空项） */
              uint16_t npics_here = (img->npics) ? img->npics[trackcount - 1] : 0;
              if (npics_here)
                {
                  pics_consumed += npics_here;
                  picnum = (uint16_t) (s + pics_consumed);
                }

              if (picnum == 0)
                continue;              /* 至今没有任何图可引用 */

              atsi[i++] = (uint8_t) picnum;
"""


def main():
    if not PATH.exists():
        print("[FAIL] 找不到 %s" % PATH)
        return 1
    text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in text:
        print("[SKIP] %s 已修过图号" % PATH.name)
        return 0
    for old, what in ((DECL_OLD, "声明 pics_consumed/picnum"),
                      (OLD, "静图记录循环")):
        n = text.count(old)
        if n != 1:
            print("[FAIL] %s: %s 匹配 %d 次（应为 1）" % (PATH.name, what, n))
            return 1
    text = text.replace(DECL_OLD, DECL_NEW, 1)
    text = text.replace(OLD, NEW, 1)
    PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("[OK]   %s：图号改为按轨累计的全局序号" % PATH.name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
