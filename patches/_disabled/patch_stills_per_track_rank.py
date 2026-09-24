#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ATS 静图记录：后两个字段改为**按轨递进**（byte1 保持 0x00，不开启翻页）。

## 依据（巴赫《布兰登堡协奏曲》，1 title / 18 轨 / ASVS 单记录）

巴赫是最贴近「单 title + ASVS 单记录」形态的商业盘，**导航与封面都正常**。
其静图表（6 字节/轨，偏移 = 描述符 +14）实测：

    轨1: 01 04 00 6c 00 75   → (260, 108, 117)
    轨2: 01 04 00 76 00 7f   → (260, 118, 127)
    轨3: 01 04 00 80 00 89   → (260, 128, 137)
    轨4: 01 04 00 8a 00 93   → (260, 138, 147)
    ...

规律：
  · **第 2 字段 = 6×轨数 + 10×轨序**（108 = 6×18 是表长；每轨 +10）
  · **第 3 字段 = 第 2 字段 + 9**
  · 第 1 字段 = 图号（巴赫 260，因 title 号编码方式不同）
  · byte1 = 0x04 —— **这一项本工程**不**照抄**：源码里它对应
    `--stilloptions manual`「Enable browsable (manual advance) pictures」，
    即「可手动翻页的幻灯片」。巴赫是那个形态，而本工程要的是
    「画面随曲切换、不提供翻页」，故 byte1 保持 0x00。

## 上游的写法为什么不行

    uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);            /* 整轨相同 */
    uint16_copy(&atsi[i], (ntitletracks[j]-1)*0x6
                          + 0x0F + (ntitlepics[j]-1)*0xA);    /* 整轨相同 */

两个字段都写成**与轨无关的常数** —— 所有轨的记录都指回同一个位置。
播放器逐轨查表时无法区分当前轨：曲目号显示不出、封面只认第一张。

## 改法

  · byte1 保持 0x00（不写 0x04，不开启手动翻页）
  · 第 2 字段 = `6×轨数 + 10×r`（r = title 内轨序，0-based）
  · 第 3 字段 = `6×轨数 + 10×r + 9`

标题为「单 title」时 ntitletracks[j] = 全盘轨数，与巴赫同形。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "第 2 字段 = 6×轨数 + 10×"

OLD = """              atsi[i++] = pictitlecount;

              if ((img->options)
                  && (img->options[s])
                  && (img->options[s]->manual))
                atsi[i] = 0x04;

              i++;

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

NEW = """              atsi[i++] = pictitlecount;

              /* byte1 保持 0x00（**不得**写 0x04）。

                 实测（2026-09-22）：写 0x04 会让 PowerDVD 出现
                 「上一张/下一张幻灯片」选项 —— 即把静图当成可手动翻页的
                 幻灯片。这与源码里 `--stilloptions manual`
                 （"Enable browsable (manual advance) pictures"）写同一个位
                 完全吻合，故该位就是「可翻页」标志，本工程不要。 */
              i++;

              /* 第 2 字段 = 6×轨数 + 10×轨序（每轨不同）。 */
              uint16_copy(&atsi[i], 0x06 * ntitletracks[j] + 10 * r);
              i += 2;

              if (globals->veryverbose)
                foutput(MSG_TAG "ntitlepics[%d]=%d, ntitletracks[%d]=%d\\n",
                        j, ntitlepics[j], j, ntitletracks[j]);

              /* 第 3 字段 = 第 2 字段 + 9 */
              uint16_copy(&atsi[i], 0x06 * ntitletracks[j] + 10 * r + 9);

              i += 2;
"""


def main():
    p = SRC / "atsi2.c"
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
    print("[OK]   atsi2.c：静图记录后两字段按轨递进（byte1 保持 0x00）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
