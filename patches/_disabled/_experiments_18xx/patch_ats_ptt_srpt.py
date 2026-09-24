#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""补写 `ATS_PTT_SRPT`（Part Of Title Search Pointer Table）。

⚠️⚠️ 状态：**已废弃 · 不要接线**

试过一次，结果：
  · 「下一曲」依然不前进；
  · 而且 **PowerDVD 跳到后面的曲目会直接闪退**。

闪退说明这张表的布局/取值有错（或者它根本不是「下一曲」的解析入口）——
播放器按错误的指针寻址就越界了。既然会崩，就绝不能留在构建流程里。

## 为什么错

本表结构是**从 DVD-Video 的 `VTS_PTT_SRPT` 类推**的：
  · `foo_input_dvda` 的 `ifo.h` 里有 DVD-Video 版本的结构定义；
  · dvda-author 源码里**完全没有** PTT 相关代码；
  · 本地 Docs 与公开资料都查不到 DVD-Audio 版本的定义。

也就是说，**我并不知道 DVD-Audio 这张表的真实布局**，是靠类推写出来的。
这类「猜一个二进制结构」的改动风险极高：错了不会报错，而是让播放器崩。

## 以后若要再试，必须先满足

  1. 只接线本脚本、其他改动不动（失败可秒回退）；
  2. 有一张**同类型的已知良好参考盘**（商业 DVD-Audio）可对比该字段，
     而不是靠推断；
  3. 先在可丢弃的测试盘上验证，不要动成品。

## 下面的实现仅作记录

`main()` 已被改成直接拒绝执行。
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


def _disabled_main():
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



def main():
    print("[SKIP] patch_ats_ptt_srpt.py 已废弃 —— 上次尝试导致 PowerDVD 崩溃。")
    print("       本表结构是类推出来的、未经证实；详见本脚本头部说明。")
    print("       如确需再试，请先满足头部列出的三个前提。")
    return 0


if __name__ == "__main__":
    import sys
    sys.exit(main())
