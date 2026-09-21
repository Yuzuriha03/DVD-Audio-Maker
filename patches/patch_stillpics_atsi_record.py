#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""候选修补：给 ATSI 的静图记录补上「沿用上一张」的引用。

> ⚠️ **状态：未接入 `build_dvda_author_mlp.sh`；实盘验证后判定**目前不需要**。**
>
> 2026-09-21 用成品盘验证：播放时**每首曲子都能显示所属专辑的封面**
> （盘1 28/28、盘2 17/17 个专辑全部正确）。也就是说 `atsi2.c` 里那个
> `continue` 虽然确实跳过了这些轨的 ATSI 静图引用，**播放器仍然按顺序
> 沿用上一张显示** —— 下面「缺陷 / 修法」的分析是针对 ATSI 引用的，
> 与实际播放行为不符。
>
> 因此本脚本**保留档案，但不要接线**。只有在换用别的播放器、确认封面
> 不显示时，才考虑接入，且接入后必须在真机上复验（见
> docs/TROUBLESHOOTING.md 16.26）。
>
> `build_dvda_author_mlp.sh` 的菜单补丁列表里**没有**调用本脚本。

## 结构（两侧配合的关系）

- `asvs.c` 生成 `AUDIO_SV.IFO`，里面有一张**紧凑**的 title 表：每个
  「有独立图片的 title」占一条（8 字节）。本项目盘1 有 28 个专辑 →
  28 条，`totnumtitles = 28`。
- `atsi2.c` 生成 `ATS_xx_0.IFO`，其中的 stills 记录里写
  **「用第几个 title 条目」的序号**（1-based）：

      if (ntitlepics[j]) ++pictitlecount;        // 有图片的 title 累加
      ...
      atsi[i++] = pictitlecount;                // title-with-pics rank

  也就是说 title → ASVS 条目的映射由 ATSI 承担，ASVS 侧保持紧凑是对的。

## 缺陷

同一段里还有一句：

    for (r = 0; r < ntitletracks[j]; ++r)      // ← 一轨一条记录的设计
      {
        ++trackcount;
        //  This might be taken off in some unclear cases.
        if ((ntitlepics[j] == 0) && (img->npics[trackcount - 1] == 0))
          continue;                            // ← 跳过
        atsi[i++] = pictitlecount;
        ...
      }

`--stillpics` 用**空项**表示「沿用上一张图」时，那些轨的 `img->npics` 是 0
（MLP 每首歌自成 title，所以 `ntitlepics[title]` 也是 0）→ 这些轨被整个跳过。
两个后果：

1. 本 title 写出的记录数 < `ntitletracks[j]`，破坏「一轨一条记录」的格式约定；
2. 这些轨在 ATSI 里**没有任何静图引用** → 播放时不显示封面。

实测盘1：91 个 title 里只有 28 个（专辑首曲）有记录，**63 条轨没有封面**。
（`asvs.c` 那边是 28 条紧凑条目，与这 28 个 title 对应，本身没问题。）

## 修法

去掉这个跳过：只要前面已经出现过带图片的 title，就照常写入
`pictitlecount` —— 它此刻的值正是**最近一个有图片 title 的序号**，
于是这些轨复用同一张封面。`asvs.c` 里那张图本来就已存在，**不额外占扇区**。

只有当「至今没有任何 title 带图片」时才跳过（保持“完全没有封面”时的原行为）。

ATSI 因此每个 title 多写约 6 字节 × 缺失轨数（本项目盘1 约 +378 字节），
由 `patch_atsi_dynamic.py` 的动态分配吸收。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/atsi2.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """              //  This might be taken off in some unclear cases.

              if ((ntitlepics[j] == 0) && (img->npics[trackcount - 1] == 0))
                continue;

              // title-with-pics rank (1-based)

              atsi[i++] = pictitlecount;
"""

NEW = """              /* 本循环是「一轨一条记录」，**不能**因本 title 没有独立图片就
                 跳过 —— 那样写出的记录数会少于 ntitletracks[j]，破坏格式约定；
                 而且这些轨在 ATSI 里就没有静图引用了，播放时不显示封面。
                 --stillpics 的空项（沿用上一张图）正会走到这里：
                 MLP 每首歌自成 title，所以 ntitlepics[j] 与 img->npics 都是 0。
                 照常写入 pictitlecount —— 它此刻正是**最近一个有图片 title
                 的序号**，于是这些轨复用同一张封面（ASVS 里那张图已存在，
                 不额外占扇区）。只有至今毫无图片时才跳过。 */
              if (pictitlecount == 0)
                continue;

              // title-with-pics rank (1-based)

              atsi[i++] = pictitlecount;
"""

if OLD not in text:
    if "本循环是「一轨一条记录」" in text:
        print("[SKIP] 已应用过")
        sys.exit(0)
    print("[MISS] 未找到 stills 记录的跳过判断")
    sys.exit(1)

if text.count(OLD) != 1:
    print("[MISS] 匹配到 %d 处，需唯一" % text.count(OLD))
    sys.exit(1)

PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                errors="surrogateescape")
print("[OK] 静图记录不再跳过「沿用上一张图」的轨")
print("\n播放封面修复完成，共 1 项")
