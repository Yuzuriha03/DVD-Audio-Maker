/* File avsv.c: Create or parse AUDIO_SV.IFO/BUP.

 * Copyright (C) 2007-2022 Fabrice Nicol fabnicol@users.sourceforge.net

 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
* FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <http://www.gnu.org/licenses/>.*/

/*

  AUDIO_SV.IFO

  00000000   44 56 44 41 55 44 49 4F  41 53 56 53 00 Y0 00 12  DVDAUDIOASVS....
  00000010   00 00 00 02 00 00 00 X0  53 00 00 00 00 00 00 00  ........S.......
  00000020   00 10 80 80 00 10 80 80  00 10 80 80 00 10 80 80  ................
  00000030   00 10 80 80 00 10 80 80  00 10 80 80 00 10 80 80  ................
  00000040   00 10 80 80 00 10 80 80  00 10 80 80 00 10 80 80  ................
  00000050   00 10 80 80 00 10 80 80  00 10 80 80 00 10 80 80  ................
  00000060   01 00 00 01 00 00 00 00  00 00 00 00 00 00 00 00  ................

  * fixed for whatever track in group 1 (just one pic),
  with X0 size of bmp pic in sectors -1.
  * Y0 = number of titles in disc (2 bytes)
  * X0 = total size of pics (bmp) in sectors -1, perhaps 1 to three bytes before.
  * Table starting 0x60, for each group and title
  Offset     bytes     value
  00         1         number of tracks in title (1-based)
  02         2         track rank (1-based) at start of title.
                       When several tracks in title, track rank for next title
  is current rank + number of tracks in current title
  06         2         sector start in VOB:
                       0, sizeof(pic1VOB), +=sizeof(pic2VOB), ...,
                       +=sizeof(pic (n-1)VOB)
  with current title starting at pic n

  * apparently no group indexing
*/

#include "commonvars.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <errno.h>
#include <stdint.h>
#ifndef __WIN32__
#include <unistd.h>
#endif
#include "structures.h"
#include "c_utils.h"
#include "winport.h"
#include "auxiliary.h"
#include "asvs.h"
#include "menu.h"

int create_asvs(char *audiotsdir,
                int naudio_groups,
                uint8_t *numtitles,
                uint16_t **ntitlepics,
                uint8_t sectors_asvs,
                pic *img,
                globalData *globals)
{

  uint8_t titleset = 0, title = 0;
  uint8_t asvs[sectors_asvs * 2048];
  memset(asvs, 0, sectors_asvs * 2048);

  uint16_t pict = 0, npics = 0, totnumtitles = 0, k, j, t;
  /* 本 title 内归零的相对偏移（见 patch_asvs_image_sectors.py）。
     totpicsectors 是全局累计，用作 base_sect；两者不能混用。 */
  uint16_t titlesectors = 0;
  errno = 0;
  uint32_t totpicsectors = 0;

  memcpy(asvs, "DVDAUDIOASVS", 12);

  uint16_copy(&asvs[0xE], 0x0012);  // DVD Spec
  asvs[0x13] = 2; // unknown
  /* 视频属性（制式）由**实际产出的 AUDIO_SV.VOB** 自检得出：
     0x43 = 525/60 (NTSC)，0x53 = 625/50 (PAL)。
     上游此处硬编码 0x53，故传 -4 ntsc 时会写成「声明 PAL 实播 NTSC」。
     见 docs/DVDA-AUTHOR-CHANGES.md。 */
  asvs[0x18] = dvda_video_attr_of(img->stillvob, img, globals);
  /* 商业盘一致为 0（Enigma 0x53/0x00、李娜 0x43/0x00、巴赫 0x43/0x00）。
     原值 1 的语义是「激活按钮」，而 --stillpics 生成的静图**没有按钮**，
     声明与实物不符；播放器可能据此进入按钮导航模式，把「上一段/下一段」
     吞掉（实测症状：曲目显示 0、上一段无反应、下一段跳回第 1 首）。
     源码注释本身也写了「or 0」。 */
  asvs[0x19] = 0x0;

  // This palette is taken as is from a commercial DVD: unselected
  // text (display)
  // Status: probably broken

  uint32_copy(&asvs[0x20],
              (uint32_t) strtoul(img->activebgcolor_palette, NULL, 16));

  //uint32_copy(&asvs[0x24], (uint32_t) 0x80E6807F);

  // album, group headers and highlighted text

  uint32_copy(&asvs[0x28],
              (uint32_t) strtoul(img->activetextcolor_palette, NULL, 16));

  //highlight motif

  uint32_copy(&asvs[0x2C],
              (uint32_t) strtoul(img->activehighlightcolor_palette, NULL, 16));

  // select action text only

  uint32_copy(&asvs[0x30],
              (uint32_t) strtoul(img->activeselectfgcolor_palette, NULL, 16));

  /*
    uint32_copy(&asvs[0x34], 0x007C6355);
    uint32_copy(&asvs[0x38], 0x006ADDCA);
    uint32_copy(&asvs[0x3C], 0x00AA10A6);
    uint32_copy(&asvs[0x40], 0x00296EF0);
    uint32_copy(&asvs[0x44], 0x002E9E9C);
    uint32_copy(&asvs[0x48], 0x0050D58E);
    uint32_copy(&asvs[0x4C], 0x00EB8080);
    uint32_copy(&asvs[0x50], 0x00AA8080);
    uint32_copy(&asvs[0x54], 0x007E8080);
    uint32_copy(&asvs[0x58], 0x00538080);
    uint32_copy(&asvs[0x5C], 0x00108080);  possible palettes */

  k = 0x60;
  t = 0x378;

  int loop = 0;
  int index = 0;

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
                foutput(ERR "Exceeding stillpic buffer limit (2 MB) \
at pict #%d.\n", j);
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

  uint16_copy(&asvs[0xC], totnumtitles);
  uint32_copy(&asvs[0x14],
              /* size of VOB associated with : change TODO */
              totpicsectors - 1);

  int nb_asv_files = create_file(audiotsdir,
                                 "AUDIO_SV.IFO",
                                 asvs, sectors_asvs * 2048, globals);

  nb_asv_files += create_file(audiotsdir,
                              "AUDIO_SV.BUP", asvs,
                              sectors_asvs * 2048, globals);
  fflush(NULL);
  return nb_asv_files;
}
