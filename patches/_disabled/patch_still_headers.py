#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图 MPEG-2 头部对齐商业盘：`progressive_sequence=1` + 声明码率 9.0 Mbps。

## 依据（2026-09-23，五张盘逐字节实测）

静图的 MPEG-2 **序列头 + 序列扩展**里有两项与三张商业盘不一致：

| 项目 | 巴赫 | 李娜 | Enigma | 本工程（修前） |
|---|---|---|---|---|
| `progressive_sequence` | **1** | **1** | **1** | 0 |
| 序列头声明码率 | **9.0 Mbps** | **9.0** | **9.0** | 3.2 / 7.5 |

原始字节（序列扩展开头 `14 8a` vs `14 82`，差的正是 bit12）：

    商业盘: 00 00 01 b5 | 14 8a 00 01 00 00    progressive_sequence=1
    本工程: 00 00 01 b5 | 14 82 00 01 00 00    progressive_sequence=0

### 为什么是安全的自洽改动

本工程每张静图实际编码的就是**单个逐行 I 帧**，各项标志本来就是逐行的：

    picture_structure      = 3 (Frame)
    frame_pred_frame_dct   = 1
    progressive_frame      = 1
    top_field_first        = 0

只有 `progressive_sequence` 这一个位被 `mpeg2enc` 写成了 0。实测命令行无法影响它
（`-I 0/1/2`、`-q`、`-s` 全试过，输出恒为 `14 82`；`-I 2` 还会段错误）。
既然实际码流是逐行的，把这个位改成 1 是**如实描述**，不是伪造。

### 声明码率

序列头 `bit_rate_value`（单位 400 bps）与 `vbv_buffer_size_value` 构成 VBV 模型：
声明码率只需**不低于**实际峰值即可。商业盘一律声明 9.0 Mbps（= 最大档），
本工程声明的是编码器实际用的值。改成 9000 与商业盘一致，且对数据无影响。

## 改法

`dvda_pad_program_end()`（`patch_still_end_code.py` 加进 menu.c 的那个后处理
函数）里顺带把这两处改掉 —— 它已经拿到整段 mpg，且**必须在 `stat_file_size()`
之前**执行（否则 ASVS 的部位指针与实物不符）。

只动**头部字节**，不动任何图像数据：
  · 序列扩展的 `progressive_sequence` 0 → 1
  · 序列头的 `bit_rate_value` → 9000 kbps
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "静图头部对齐商业盘"

# 插在 dvda_pad_program_end 定义之前
HELPER = r'''
/* 静图头部对齐商业盘（见 patches/patch_still_headers.py）。

   mpeg2enc 把序列扩展的 progressive_sequence 恒写成 0，命令行改不了
   （-I 0/1/2、-q、-s 全试过）。但本工程每张静图实际就是单个逐行 I 帧
   （picture_structure=Frame、frame_pred_frame_dct=1、progressive_frame=1），
   三张商业盘此处也一律是 1 —— 改成 1 是如实描述。
   顺带把序列头声明码率改成商业盘的 9.0 Mbps（VBV 模型只需不低于实际峰值）。

   只改序列头/序列扩展的定长字段，不碰任何图像数据。 */
static int dvda_align_still_header(const char *path)
{
  static const unsigned char SEQ[] = {0x00, 0x00, 0x01, 0xB3};
  static const unsigned char EXT[] = {0x00, 0x00, 0x01, 0xB5};
  FILE *f = fopen(path, "rb+");
  unsigned char buf[4096];
  size_t n;
  long seq_off = -1, ext_off = -1;
  size_t i;

  if (f == NULL) return -1;

  n = fread(buf, 1, sizeof(buf), f);
  for (i = 0; i + 4 <= n; ++i)
    {
      if (seq_off < 0 && memcmp(buf + i, SEQ, 4) == 0)
        {
          seq_off = (long) i;
          continue;
        }
      /* 序列扩展：ext id = 1（字节高 4 位） */
      if (ext_off < 0 && memcmp(buf + i, EXT, 4) == 0
          && i + 8 <= n && (buf[i + 4] >> 4) == 1)
        ext_off = (long) i;
      if (seq_off >= 0 && ext_off >= 0) break;
    }

  if (seq_off < 0 || ext_off < 0 || ext_off + 6 > (long) n
      || seq_off + 12 > (long) n)
    {
      fclose(f);
      return 0;                 /* 没有头部就什么都不做 */
    }

  /* 1) 序列头声明码率 -> 9000 kbps（单位 400 bps => 22500） */
  {
    unsigned char *p = buf + seq_off + 4;
    unsigned long br = 22500UL;
    p[4] = (unsigned char) ((br >> 10) & 0xFF);
    p[5] = (unsigned char) ((br >> 2) & 0xFF);
    p[6] = (unsigned char) ((p[6] & 0x3F) | ((br & 0x3) << 6));
  }

  /* 2) 序列扩展 progressive_sequence (bit 12) -> 1 */
  buf[ext_off + 5] |= 0x08;

  if (fseek(f, seq_off + 4, SEEK_SET) != 0
      || fwrite(buf + seq_off + 4, 1, 8, f) != 8
      || fseek(f, ext_off + 4, SEEK_SET) != 0
      || fwrite(buf + ext_off + 4, 1, 6, f) != 6)
    {
      fclose(f);
      return -1;
    }

  fclose(f);
  return 0;
}

'''

CALL_OLD = """            dvda_pad_program_end(img->backgroundmpg[rank]);"""

CALL_NEW = """            dvda_align_still_header(img->backgroundmpg[rank]);
            dvda_pad_program_end(img->backgroundmpg[rank]);"""


def main():
    p = SRC / "menu.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0

    anchor = "static int dvda_pad_program_end(const char *path)"
    if t.count(anchor) != 1:
        print("[FAIL] 找不到 dvda_pad_program_end（计数 %d）" % t.count(anchor))
        return 1
    t = t.replace(anchor, HELPER.lstrip("\n") + anchor, 1)

    if t.count(CALL_OLD) != 1:
        print("[FAIL] 找不到调用点（计数 %d）" % t.count(CALL_OLD))
        return 1
    t = t.replace(CALL_OLD, CALL_NEW, 1)

    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 静图头部对齐（progressive_sequence=1、码率 9000）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
