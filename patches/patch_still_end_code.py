#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把静图 VOB 的程序结束码从「扇区末 4 字节」改成「独占扇区 + 补 0xFF」。

## 实测依据（2026-09-22）

`AUDIO_SV.VOB` 里每张静图末尾都应有一个 MPEG 程序结束码 `00 00 01 B9`。
三张商业盘都有，且**位置一致**——独占一个扇区，其后用 `0xFF` 填满扇区：

    Enigma《15 Years After》99 张 → 99 个，全部落在扇区开头（偏移 % 2048 == 0）
    李娜精选集            12 张 → 12 个
    巴赫布兰登堡          18 张 → 18 个

本工程（dvda-author + mplex）把结束码写在**段末扇区的最后 4 字节**
（偏移 % 2048 == 2044），紧随其后就是下一段静图的 pack 头：

    商业盘:  ... pack 数据 ... | B9 FF FF ... FF |   ← 结束码独占扇区
    我们:    ... pack 数据 ... FF FF FF FF B9 | BA ...   ← 挤在扇区尾

DVD 规范要求程序结束码**之后**填充到扇区边界；我们的填充在结束码之前，
等于「结束码后面直接是新的 pack」。严格解码器可能据此把两段静图当成
一个程序，从而算错「当前在第几轨」。

## 修法

对每张静图的 mpg（`generate_background_mpg` 里 `create_mpg` 之后、
`stat_file_size` 之前）后处理：

  1. 若末扇区最后 4 字节是 `00 00 01 B9`，把它们改成 `FF FF FF FF`；
  2. 在文件末尾追加一个扇区：`B9` + 2044 个 `FF`。

这样每段静图末尾就是「结束码 + 填充」，与三张商业盘一致。

⚠️ 必须在 `stat_file_size()` **之前**做，因为它决定
`img->stillpicvobsize[]`，而 ASVS 表里的每张图起始扇区就是由它累加出来的；
改完自动一致，不必另改 ASVS。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "程序结束码独占扇区（对齐商业盘）"

HELPER = r'''
/* 程序结束码独占扇区（对齐商业盘）：见 patches/patch_still_end_code.py。
   mplex 把 00 00 01 B9 写在本段最后一个扇区的最后 4 字节；商业盘则是让
   结束码独占一个扇区、其后补 0xFF 到扇区边界。这里把前者改写成后者。 */
static int dvda_pad_program_end(const char *path)
{
  FILE *f = fopen(path, "rb+");
  unsigned char tail[4];
  unsigned char *blk;
  long sz;

  if (f == NULL) return -1;
  if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return -1; }
  sz = ftell(f);
  if (sz < 2048 || (sz % 2048) != 0) { fclose(f); return -1; }

  if (fseek(f, sz - 4, SEEK_SET) != 0) { fclose(f); return -1; }
  if (fread(tail, 1, 4, f) != 4) { fclose(f); return -1; }

  /* 已经是商业盘形态（结束码在扇区开头）或本段没有结束码 → 不动 */
  if (!(tail[0] == 0x00 && tail[1] == 0x00
        && tail[2] == 0x01 && tail[3] == 0xB9))
    {
      fclose(f);
      return 0;
    }

  {
    unsigned char ff[4] = {0xFF, 0xFF, 0xFF, 0xFF};
    if (fseek(f, sz - 4, SEEK_SET) != 0) { fclose(f); return -1; }
    if (fwrite(ff, 1, 4, f) != 4) { fclose(f); return -1; }
  }

  blk = (unsigned char *) malloc(2048);
  if (blk == NULL) { fclose(f); return -1; }
  memset(blk, 0xFF, 2048);
  blk[0] = 0x00; blk[1] = 0x00; blk[2] = 0x01; blk[3] = 0xB9;

  if (fseek(f, 0, SEEK_END) != 0) { fclose(f); free(blk); return -1; }
  if (fwrite(blk, 1, 2048, f) != 2048) { fclose(f); free(blk); return -1; }
  free(blk);
  fclose(f);
  return 0;
}

'''

CALL_OLD = """            create_mpg(img, rank, mp2track, tempfile, globals);
            img->stillpicvobsize[rank] = (uint32_t)(stat_file_size(img->backgroundmpg[rank]) / 0x800);"""

CALL_NEW = """            create_mpg(img, rank, mp2track, tempfile, globals);
            /* 结束码独占扇区（对齐商业盘）。必须在 stat_file_size 之前做，
               否则 stillpicvobsize 与 ASVS 表里的扇区指针会与实物不符。 */
            dvda_pad_program_end(img->backgroundmpg[rank]);
            img->stillpicvobsize[rank] = (uint32_t)(stat_file_size(img->backgroundmpg[rank]) / 0x800);"""


def main():
    p = SRC / "menu.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0

    # 1) 在 create_mpg 之前插入 helper
    anchor = "int create_mpg(pic *img, uint16_t rank, char *mp2track, char *tempfile, globalData *globals)\n"
    if t.count(anchor) != 1:
        print("[FAIL] 找不到 create_mpg 定义（匹配 %d 次）" % t.count(anchor))
        return 1
    t = t.replace(anchor, HELPER.lstrip("\n") + anchor, 1)

    # 2) 在 stillpics 分支调用它
    if t.count(CALL_OLD) != 1:
        print("[FAIL] 找不到 stillpicvobsize 赋值处（匹配 %d 次）"
              % t.count(CALL_OLD))
        return 1
    t = t.replace(CALL_OLD, CALL_NEW, 1)

    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 新增 dvda_pad_program_end() 并在静图分支调用")
    return 0


if __name__ == "__main__":
    sys.exit(main())
