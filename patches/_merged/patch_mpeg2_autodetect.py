#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""让 dvda-author 自己探测「制式」与「逐行/隔行」，并如实填写参数。

## 为什么要有这个补丁

有两处参数此前是**写死**的，与实际码流无关：

| 位置 | 上游写法 | 问题 |
|---|---|---|
| `asvs.c` 的 `asvs[0x18]` | `0x53`（= PAL） | 传 `-4 ntsc` 时仍写 PAL →「声明 NTSC 实播 PAL」 |
| `amg2.c` 的 `amg[0x100]` / `unknown2` | `0x53000000` | 同上（菜单区视频属性） |
| 序列扩展 `progressive_sequence` | 照抄 `mpeg2enc` 的 **0** | 静图实际是逐行单帧，头里却声明隔行 |

前两处靠「照抄商业盘」的字节补丁修过（`patch_asvs_video_attr.py`，已停用）；
第三处靠 `patch_still_headers.py` **无条件**把该位改成 1（也已停用）。
无条件改和写死一样不可靠 —— 一旦输入换成隔行素材就会写出假值。

## 本补丁的做法：探测后如实填写

`menu.c` 里新增一段码流自检，直接从**实际产出的 MPEG-2** 里读：

    序列头       00 00 01 B3   尺寸 / aspect_ratio_information /
                               frame_rate_code / bit_rate_value
    序列扩展     00 00 01 B5 + 0x1?   progressive_sequence
    图像编码扩展 00 00 01 B5 + 0x8?   picture_structure /
                               frame_pred_frame_dct / progressive_frame

由此得到两个结论：

1. **制式**：`frame_rate_code`（3/6 = 625/50，其余 = 525/60）与序列头里的
   画面高度（576/480）互相印证，冲突时以画面高度为准 —— 显示制式由它决定。
   据此算出 `video_attr` 字节：`0x43` = 525/60，`0x53` = 625/50。
   写入 ASVS `0x18` 与 AMG `0x100`（菜单区）。
   探测不到序列头时退回 `--norm` 的设置，并打 `[WAR]`。
2. **逐行/隔行**：仅当图像编码扩展表明画面确实是「整帧 + frame_pred_frame_dct
   + progressive_frame」而序列扩展却写着 0 时，才把该位写回 1。
   内容真的是隔行就一个字节都不动。

`progressive_sequence` 的改动与 `patch_still_end_code.py` 同理，**必须在
`stat_file_size()` 之前**执行，否则 ASVS 的扇区指针与实物不符。

## 只改这两个参数

序列头声明的码率**不改**：那是编码器自己的设置，本身自洽（本工程 7.5 Mbps、
商业盘 9.0 Mbps 只是两套编码参数，不是真假问题）。自检报告里会打印出来供核对。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "MPEG-2 码流自检"

# ── 1. menu.c：注入自检实现（插在 generate_background_mpg 之前）──────────
C_ANCHOR = """int generate_background_mpg(pic *img, globalData *globals)
{
  uint16_t rank = 0;
"""

C_HELPER = r'''/* ── MPEG-2 码流自检：制式（NTSC/PAL）与逐行/隔行 ──────────────────────

   见 patches/patch_mpeg2_autodetect.py。

   dvda-author 原先把视频属性写死成 PAL（0x53），并照抄 mpeg2enc 的
   progressive_sequence=0。这两项都能从**实际产出的码流**里读出来，
   故此处改为探测后如实填写，不再依赖硬编码。 */

typedef struct
{
  int      seq_ok;          /* 找到序列头 */
  int      ext_ok;          /* 找到序列扩展 */
  int      pce_ok;          /* 找到图像编码扩展 */
  int      ntsc;            /* 1 = 525/60 (NTSC)，0 = 625/50 (PAL/SECAM) */
  int      width, height;   /* 序列头里的画面尺寸 */
  int      aspect;          /* aspect_ratio_information */
  int      frame_rate;      /* frame_rate_code */
  uint32_t bit_rate_kbps;   /* 序列头声明的码率 */
  int      progressive;     /* 序列扩展的 progressive_sequence */
  int      progressive_pic; /* 图像编码扩展：整帧 + frame_pred_frame_dct
                               + progressive_frame */
  long     ext_at;          /* 序列扩展起始码在文件内的偏移 */
} dvda_mpeg2_t;

/* 序列头/序列扩展/图像编码扩展都在文件开头，读前 64 KB 足够 */
#define DVDA_PROBE_BYTES 65536

static int dvda_mpeg2_probe(const char *path, dvda_mpeg2_t *p)
{
  unsigned char buf[DVDA_PROBE_BYTES];
  FILE *f;
  size_t n, i;
  long seq = -1, ext = -1, pce = -1;

  memset(p, 0, sizeof *p);
  if (path == NULL) return 0;

  f = fopen(path, "rb");
  if (f == NULL) return 0;
  n = fread(buf, 1, sizeof buf, f);
  fclose(f);
  if (n < 16) return 0;

  for (i = 0; i + 4 <= n; ++i)
    {
      unsigned id;

      if (buf[i] != 0x00 || buf[i + 1] != 0x00 || buf[i + 2] != 0x01)
        continue;

      if (buf[i + 3] == 0xB3 && seq < 0 && i + 12 <= n)
        {
          seq = (long) i;
          continue;
        }

      if (buf[i + 3] != 0xB5 || i + 10 > n) continue;

      id = buf[i + 4] >> 4;
      if (id == 1 && ext < 0)      ext = (long) i;  /* sequence_extension */
      else if (id == 8 && pce < 0) pce = (long) i;  /* picture_coding_ext */

      if (seq >= 0 && ext >= 0 && pce >= 0) break;
    }

  if (seq >= 0)
    {
      const unsigned char *b = buf + seq + 4;
      int by_size = -1;

      p->seq_ok     = 1;
      p->width      = (b[0] << 4) | (b[1] >> 4);
      p->height     = ((b[1] & 0x0F) << 8) | b[2];
      p->aspect     = b[3] >> 4;
      p->frame_rate = b[3] & 0x0F;
      /* bit_rate_value 单位 400 bps */
      p->bit_rate_kbps = (uint32_t)((((unsigned long) b[4] << 10)
                                     | ((unsigned long) b[5] << 2)
                                     | (b[6] >> 6)) * 400UL / 1000UL);

      if (p->height == 480 || p->height == 240)      by_size = 1; /* 525/60 */
      else if (p->height == 576 || p->height == 288) by_size = 0; /* 625/50 */

      /* frame_rate_code: 1=23.976 2=24 3=25 4=29.97 5=30 6=50 7=59.94 8=60 */
      if (p->frame_rate == 3 || p->frame_rate == 6)      p->ntsc = 0;
      else if (p->frame_rate >= 1 && p->frame_rate <= 8) p->ntsc = 1;
      else                                               p->ntsc = (by_size == 1);

      /* 画面高度是显示制式的直接判据；与帧率冲突时以它为准 */
      if (by_size >= 0 && by_size != p->ntsc) p->ntsc = by_size;
    }

  if (ext >= 0)
    {
      p->ext_ok      = 1;
      p->ext_at      = ext;
      p->progressive = (buf[ext + 5] >> 3) & 1;
    }

  if (pce >= 0)
    {
      const unsigned char *b = buf + pce + 4;
      int structure = (b[2] >> 6) & 3;   /* 3 = 整帧 */
      int fpfd      = (b[3] >> 6) & 1;   /* frame_pred_frame_dct */
      int pf        = (b[4] >> 7) & 1;   /* progressive_frame */

      p->pce_ok          = 1;
      p->progressive_pic = (structure == 3 && fpfd && pf);
    }

  return p->seq_ok;
}

/* DVD 视频属性字节：0x43 = 525/60 (NTSC)，0x53 = 625/50 (PAL)。
   取自码流自检；探测不到时退回 --norm 的设置（上游默认 pal）并告警。 */
uint8_t dvda_video_attr_of(const char *path, pic *image, globalData *globals)
{
  dvda_mpeg2_t p;
  int ntsc;

  if (dvda_mpeg2_probe(path, &p) && p.seq_ok)
    {
      ntsc = p.ntsc;
      foutput(MSG_TAG "视频属性自检: %s\n"
              "       %dx%d, %s, 帧率码 %d, 声明 %u kbps, 图像编码 %s\n",
              (path != NULL) ? path : "(none)",
              p.width, p.height, ntsc ? "NTSC" : "PAL",
              p.frame_rate, (unsigned) p.bit_rate_kbps,
              p.pce_ok ? (p.progressive_pic ? "逐行" : "隔行") : "未检出");
    }
  else
    {
      ntsc = (image != NULL && image->norm != NULL
              && strcmp(image->norm, "ntsc") == 0);
      foutput(WAR "视频属性自检: %s 读不到序列头，退回 --norm=%s → %s\n",
              (path != NULL) ? path : "(none)",
              (image != NULL && image->norm != NULL) ? image->norm : "pal",
              ntsc ? "NTSC" : "PAL");
    }

  return (uint8_t)(ntsc ? 0x43 : 0x53);
}

/* 序列扩展的 progressive_sequence：mpeg2enc 无条件写 0，但它产出的每张静图
   实际就是单个逐行 I 帧（图像编码扩展写明 整帧 + frame_pred_frame_dct
   + progressive_frame）。此处按实际内容写回 1；实际是隔行则一个字节都不动。

   必须排在 stat_file_size() 之前（同 patch_still_end_code.py），
   否则 ASVS 的扇区指针与实物不符。 */
static unsigned long dvda_prog_seen = 0, dvda_prog_fixed = 0;

static int dvda_fix_sequence_progressive(const char *path, globalData *globals)
{
  dvda_mpeg2_t p;
  FILE *f;
  unsigned char b;

  ++dvda_prog_seen;

  if (!dvda_mpeg2_probe(path, &p) || !p.ext_ok || !p.pce_ok) return 0;

  if (dvda_prog_seen == 1)   /* 所有静图共用一套编码参数，报一次即可 */
    foutput(MSG_TAG "静图码流自检: %dx%d %s, 序列扩展=%s, 图像编码=%s\n",
            p.width, p.height, p.ntsc ? "NTSC" : "PAL",
            p.progressive ? "逐行" : "隔行",
            p.progressive_pic ? "逐行" : "隔行");

  if (p.progressive || !p.progressive_pic) return 0;   /* 已如实描述 */

  f = fopen(path, "rb+");
  if (f == NULL) return -1;

  if (fseek(f, p.ext_at + 5, SEEK_SET) != 0 || fread(&b, 1, 1, f) != 1)
    {
      fclose(f);
      return -1;
    }

  b = (unsigned char)(b | 0x08);   /* progressive_sequence = 1 */

  if (fseek(f, p.ext_at + 5, SEEK_SET) != 0 || fwrite(&b, 1, 1, f) != 1)
    {
      fclose(f);
      return -1;
    }

  fclose(f);
  ++dvda_prog_fixed;
  return 1;
}

static void dvda_report_sequence_progressive(globalData *globals)
{
  if (dvda_prog_seen == 0) return;

  foutput(MSG_TAG "静图 progressive_sequence 自检: %lu/%lu 张按实际画面写回 1，"
          "其余保持原值\n", dvda_prog_fixed, dvda_prog_seen);
}

'''

# ── 2. menu.c：在静图循环里接上（必须排在 stat_file_size 之前）───────────
LOOP_OLD = """            dvda_pad_program_end(img->backgroundmpg[rank]);
            img->stillpicvobsize[rank] = (uint32_t)(stat_file_size(img->backgroundmpg[rank]) / 0x800);
"""

LOOP_NEW = """            dvda_pad_program_end(img->backgroundmpg[rank]);
            /* 序列扩展的 progressive_sequence 按实际画面写回
               （见 patch_mpeg2_autodetect.py）。同样必须在
               stat_file_size 之前。 */
            dvda_fix_sequence_progressive(img->backgroundmpg[rank], globals);
            img->stillpicvobsize[rank] = (uint32_t)(stat_file_size(img->backgroundmpg[rank]) / 0x800);
"""

REPORT_OLD = """      // The first backgroundmpg file is the one that is used to create AUDIO_SV.VOB in amg2.c
"""

REPORT_NEW = """      dvda_report_sequence_progressive(globals);
      // The first backgroundmpg file is the one that is used to create AUDIO_SV.VOB in amg2.c
"""

# ── 3. menu.h：导出 video_attr 探测 ───────────────────────────────────────
H_OLD = """void compute_pointsize(pic* img, uint16_t maxtracklength, uint8_t maxnumtracks, globalData*);
#endif
"""

H_NEW = """void compute_pointsize(pic* img, uint16_t maxtracklength, uint8_t maxnumtracks, globalData*);
/* 由码流自检得出 DVD 视频属性字节（0x43 = NTSC，0x53 = PAL）。
   见 patches/patch_mpeg2_autodetect.py。 */
uint8_t dvda_video_attr_of(const char* path, pic* image, globalData* globals);
#endif
"""

# ── 4. asvs.c：制式改为自检得出 ──────────────────────────────────────────
A_INC_OLD = """#include "asvs.h"
"""
A_INC_NEW = """#include "asvs.h"
#include "menu.h"
"""

A_OLD = """  asvs[0x18] = 0x53; // unknown, or 0x43
"""

A_NEW = """  /* 视频属性（制式）由**实际产出的 AUDIO_SV.VOB** 自检得出：
     0x43 = 525/60 (NTSC)，0x53 = 625/50 (PAL)。
     上游此处硬编码 0x53，故传 -4 ntsc 时会写成「声明 PAL 实播 NTSC」。
     见 patches/patch_mpeg2_autodetect.py。 */
  asvs[0x18] = dvda_video_attr_of(img->stillvob, img, globals);
"""

# ── 5. amg2.c：菜单区制式改为自检得出（两处，同一字段 0x100）─────────────
G1_OLD = """  uint32_copy(unknown2, menusector ? 0x53000000 : 0);
"""
G1_NEW = """  /* 菜单区视频属性（制式）由 AUDIO_TS.VOB 自检得出，
     见 patches/patch_mpeg2_autodetect.py。 */
  uint32_copy(unknown2, (menusector)
              ? ((uint32_t) dvda_video_attr_of(img->tsvob, img, globals) << 24) : 0);
"""

G2_OLD = """  uint32_copy(&amg[0x100], (menusector) ? 0x53000000 : 0); // Unknown;  // 0x1E000000 used to be uset in SET2
"""
G2_NEW = """  /* 菜单区视频属性（制式）由 AUDIO_TS.VOB 自检得出（上游硬编码 0x53=PAL），
     见 patches/patch_mpeg2_autodetect.py。 */
  uint32_copy(&amg[0x100], (menusector)
              ? ((uint32_t) dvda_video_attr_of(img->tsvob, img, globals) << 24) : 0);
"""


def sub(path, old, new, what, count=1):
    """把 old 换成 new；匹配数不对就报 FAIL 并返回 False。

    path 相对 src/，例如 "menu.c" 或 "include/menu.h"。"""
    p = SRC / path
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return False
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    n = t.count(old)
    if n != count:
        print("[FAIL] %s: %s 匹配 %d 次（应为 %d）" % (path, what, n, count))
        return False
    p.write_text(t.replace(old, new), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   %s: %s" % (path, what))
    return True


def inject_menu_c():
    p = SRC / "menu.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return False
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if t.count(C_ANCHOR) != 1:
        print("[FAIL] menu.c: 注入锚点匹配 %d 次（应为 1）" % t.count(C_ANCHOR))
        return False
    p.write_text(t.replace(C_ANCHOR, C_HELPER + C_ANCHOR),
                 encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 注入码流自检实现")
    return True


def main():
    if (SRC / "menu.c").exists():
        t = (SRC / "menu.c").read_text(encoding="utf-8",
                                       errors="surrogateescape")
        if MARK in t:
            print("[SKIP] 已应用过（menu.c 含标记）")
            return 0

    ok = inject_menu_c()
    ok &= sub("menu.c", LOOP_OLD, LOOP_NEW, "静图循环接入 progressive 自检")
    ok &= sub("menu.c", REPORT_OLD, REPORT_NEW, "静图循环末尾输出自检汇总")
    ok &= sub("include/menu.h", H_OLD, H_NEW, "导出 dvda_video_attr_of")
    ok &= sub("asvs.c", A_INC_OLD, A_INC_NEW, "包含 menu.h", count=1)
    ok &= sub("asvs.c", A_OLD, A_NEW, "ASVS 0x18 制式改为自检得出")
    ok &= sub("amg2.c", G1_OLD, G1_NEW, "decode_amg unknown2 制式改为自检得出")
    ok &= sub("amg2.c", G2_OLD, G2_NEW, "AMG 0x100 制式改为自检得出")

    if not ok:
        return 1

    print("[OK]   制式与 i/p 改为按实际码流自检填写")
    return 0


if __name__ == "__main__":
    sys.exit(main())
