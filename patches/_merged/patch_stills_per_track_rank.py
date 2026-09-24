#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ATS 静图记录：两个偏移字段改为**按轨递进**（byte1 保持 0x00）。

## 症状

**同一张专辑内只有第一首能显示出封面，切到同专辑的其它曲目画面不刷新**
（换专辑时才刷新）。用户 2026-09-24 报告。

## 根因（2026-09-24 实测三张盘）

ATS 的静图表布局是「**先 6 字节/轨的记录，紧跟 10 字节/图的清单**」，
两种记录都相对静图表起点（描述符 `+14`）定位。每轨那 6 字节：

    [图号/最高 title 序号:1][byte1:1][本轨清单起始偏移:2][本轨清单结束偏移:2]

上游把后两项写成**与轨无关的常量**：

    uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);                 /* 起 */
    uint16_copy(&atsi[i], (ntitletracks[j]-1)*0x6
                          + 0x0F + (ntitlepics[j]-1)*0xA);         /* 止 */

- `起 = 0x06×轨数` —— 只有**第 1 轨**碰巧正确
- `止 = (轨数-1)×6 + 0x0F + (图数-1)×0xA = 6×轨数 + 10×图数 - 1`
  —— 只有**末轨**碰巧正确

于是同一 title 内所有轨都声称「我的图在 [6n, 6n+10p-1]」→ 播放器逐轨查表
时取到的都是**第 1 张图**，曲目内自然不换图。

### 正常工作的商业盘是按轨递进的

| 盘 | 轨 | a | b |
|---|---|---|---|
| 李娜（2 title / 24 轨 / 12 图） | 轨1 | 72 = 6×12 + 10×0 | 81 = a+9 |
| | 轨2 | 82 = 6×12 + 10×1 | 91 = a+9 |
| 巴赫（1 title / 18 轨 / 18 图） | 轨1 | 108 = 6×18 + 10×0 | 117 = a+9 |
| | 轨2 | 118 = 6×18 + 10×1 | 127 = a+9 |
| **本工程（修前）** | 轨1..n | **36（常量）** | **95（常量）** |

即 `a(r) = 6×轨数 + 10×(前 r 轨的图数之和)`，`b(r) = a(r) + 10×本轨图数 - 1`。
每轨恰好 1 张图时退化为 `a = 6n + 10r`、`b = a + 9`。

> 第三方实现 foo_input_dvda 也按「每轨各自的起止」解读这两个字段。

## byte1 保持 0x00

源码里 `0x04` 对应 `--stilloptions manual`
（"Enable browsable (manual advance) pictures"，可手动翻页的幻灯片）。
本工程要的是「画面随曲切换、不提供翻页」，故不写 0x04。

**上游原本就是这样写的**（只在传了 `--stilloptions` 时才置 0x04），
本补丁**不改动这段逻辑** —— 实测本工程从不传 `--stilloptions`，
所以 byte1 一直是 0x00。

## 两个字段都不改表长

每轨仍是 6 字节 → ATSI 总长度不变，扇区数、`0x0804` 处的指针等一律不动。

## 验证（重建后实测）

- 17 个 title 全部变成 `a = [6n, 6n+10, 6n+20, …]`、`b = a+9`
- 与修复前对比：**78 个差异字节**（= 2×(56 轨 − 17 title)，
  即除首轨外的 a 低字节 + 除末轨外的 b 低字节），**全部落在静图表内**
- 其余 6 个系统文件（AUDIO_PP.IFO / AUDIO_SV.IFO/VOB / AUDIO_TS.IFO/BUP/VOB）
  与修复前**逐字节相同**
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "起止字节偏移"

# ── 1) 声明「本轨之前已写出的图片数」计数器 ──────────────────────────────
DECL_OLD = """          uint16_t r, u = 0,  trackcount_save = trackcount;
          s += (j) ? ntitlepics[j - 1]  : 0;
"""

DECL_NEW = """          uint16_t r, u = 0,  trackcount_save = trackcount;
          /* 本 title 内、本轨**之前**已写出的图片数。
             两条偏移字段都相对本 title 的静图表起点，而表的布局是
             「先 6 字节/轨的记录，紧跟 10 字节/图的清单」；
             故第 r 轨的清单起点 = 6×轨数 + 10×（前 r 轨的图数之和）。 */
          uint16_t pics_before = 0;
          s += (j) ? ntitlepics[j - 1]  : 0;
"""

# ── 2) 两个偏移字段改为按轨递进 ──────────────────────────────────────────
FIELDS_OLD = """              i++;

              // track rank index

              uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);
              i += 2;

              if (globals->veryverbose)
                foutput(MSG_TAG "ntitlepics[%d]=%d, ntitletracks[%d]=%d\\n",
                        j, ntitlepics[j], j, ntitletracks[j]);

              //if (ntitlepics[j] > ntitletracks[j])  // conditions to be tested

              // track rank index (backup)

              uint16_copy(&atsi[i],
                          (ntitletracks[j] - 1) * 0x6
                          + 0x0F + (ntitlepics[j] - 1) * 0xA);

              //else
              //uint16_copy(&atsi[i],(ntitletracks[j]-1)*0x10+0x0F);
              //// track rank index (backup)

              i += 2;
"""

FIELDS_NEW = """              i++;

              /* 本轨的图片清单在静图表里的**起止字节偏移**（相对表起点）。

                 表布局：先 6 字节/轨的记录，紧跟 10 字节/图的清单。
                 上游把这两项写成与轨无关的常量：
                   起 = 0x06×轨数                （只有第 1 轨碰巧正确）
                   止 = (轨数-1)×6 + 0x0F + (图数-1)×0xA
                        = 6×轨数 + 10×图数 - 1    （只有末轨碰巧正确）
                 于是同一 title 内**所有轨都指回第 1 张图** —— 表现为
                 「只在换专辑时刷新封面，同一专辑内切轨不换图」。

                 两张正常工作的商业盘都是按轨递进的：
                   李娜 轨1: a=72=6×12+10×0,  b=81=a+9
                   李娜 轨2: a=82=6×12+10×1,  b=91=a+9
                   巴赫 轨1: a=108=6×18+10×0, b=117=a+9

                 （第三方实现 foo_input_dvda 也按「每轨各自的起止」解读
                   这两个字段。） */
              {
                uint16_t pic_start = (uint16_t)(0x06 * ntitletracks[j]
                                                + 0x0A * pics_before);
                uint16_t pic_num   = (uint16_t) img->npics[trackcount - 1];

                uint16_copy(&atsi[i], pic_start);
                i += 2;
                uint16_copy(&atsi[i],
                            (uint16_t)(pic_start + 0x0A * pic_num - 1));
                i += 2;

                pics_before = (uint16_t)(pics_before + pic_num);
              }

              if (globals->veryverbose)
                foutput(MSG_TAG "ntitlepics[%d]=%d, ntitletracks[%d]=%d\\n",
                        j, ntitlepics[j], j, ntitletracks[j]);
"""


def sub(text, old, new, what):
    if text.count(old) != 1:
        print("[FAIL] %s: 匹配 %d 次（应为 1）" % (what, text.count(old)))
        return None
    print("[OK]   %s" % what)
    return text.replace(old, new, 1)


def main():
    p = SRC / "atsi2.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过（atsi2.c 含标记）")
        return 0

    t2 = sub(t, DECL_OLD, DECL_NEW, "声明 pics_before 计数器")
    if t2 is None:
        return 1
    t3 = sub(t2, FIELDS_OLD, FIELDS_NEW, "两个偏移字段改为按轨递进")
    if t3 is None:
        return 1

    p.write_text(t3, encoding="utf-8", errors="surrogateescape")
    print("[OK]   静图记录改为按轨递进（byte1 仍为 0x00）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
