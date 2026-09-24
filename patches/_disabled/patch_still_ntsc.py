#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""静图改用 **NTSC** 编码（菜单画面保持原制式）。

## 依据（2026-09-23，五张盘完整对照）

| 盘 | 静图制式 | 静图 | 上一段/下一段 |
|---|---|---|---|
| 巴赫（商业） | **720x480 NTSC** | ✅ | ✅ |
| 李娜（商业） | **720x480 NTSC** | ✅ | ✅ |
| Enigma（商业） | 720x576 PAL | ❌ 有缺陷 | ✅ |
| 本工程 | 720x576 PAL | ✅/❌ | ❌ |

**两张从不出现任何问题的商业盘都是 NTSC；PAL 的两张都出问题。**

在此之前，其它所有变量都已逐个实测排除：

| 已排除 | 怎么排的 |
|---|---|
| title 数（1 / 17） | Q 版 17 title、R 版 1 title，上下段都坏 |
| ASVS `0x0E` | 改 `0x0000` 无效 |
| ASVS 调色板 | 改 `00101010` 无效 |
| ATS 静图表 `f2`/`f3` | 改递进（照巴赫/Enigma）无效 |
| ATS 静图表 `byte1` | 改 `0x04` 无效 |
| 图索引 `track` | 改 0（照巴赫）无效（W 版） |
| 静图声明码率 | 改 9000（照商业盘）无效（T 版） |
| IFO 规范版本号 | 改 1.1 形态无效（I 版） |
| SAMG 绝对扇区指针 | 逐轨核对正确 |
| SAMG 音频属性 12 字节 | 只有巴赫有，Enigma/李娜全零 → 非必需 |
| ATS PGC / title 描述符 / 扇区表 / PTS 表 | 与 Enigma 逐字段同构 |
| att_srpt / aott_srpt | `0xc1` 是合法的「最后一个 title」标记（Enigma 末项也是） |
| AOB 布局与 ats_last_sector | 全部正确 |
| 硬件加速 | 关掉后商业盘正常、我们仍坏 |
| 播放器缓存 | 清空后无改善 |

## 改法

`menu.c` 的 `create_mpg()` 里，静图与菜单共用同一组编码参数。本补丁只在
`img->action == STILLPICS` 时把制式改成 NTSC：

    帧率   : img->framerate  →  "30000/1001"
    制式   : img->norm       →  "n"

菜单画面（ANIMATEDVIDEO）不受影响，仍是 `config.sh` 里设定的制式。

配套改动（另见 `menu_assets.py` 的 `STILL_W/STILL_H`）：静图素材要生成
**720x480**，与本补丁的 NTSC 编码一致。

ASVS 的 `video_attr`（`0x18`）也要相应改成 `0x43`（NTSC）—— 见
`patches/patch_asvs_video_attr.py`。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "静图改用 NTSC"

OLD = """  char norm[2];
  norm[0] = img->norm[0];
  norm[1] = 0;
"""

NEW = """  char norm[2];
  norm[0] = img->norm[0];
  norm[1] = 0;

  /* 静图改用 **NTSC**（见 patch_still_ntsc.py）。

     两张从不出问题的商业盘（巴赫/李娜）静图都是 720x480 NTSC；
     PAL 的 Enigma 静图有缺陷、本工程 PAL 静图在上一段/下一段上异常。
     菜单画面不受影响，仍用 img->norm / img->framerate。 */
  const char *still_framerate = img->framerate;
  if (img->action == STILLPICS)
    {
      norm[0] = 'n';
      still_framerate = "30000/1001";
    }
"""

OLD2 = """  char *argsjpeg2yuv[] = {JPEG2YUV_BASENAME, "-f", img->framerate, "-I", "p", "-n", "1", "-j", pict, "-A", img->aspectratio, NULL};"""

NEW2 = """  char *argsjpeg2yuv[] = {JPEG2YUV_BASENAME, "-f", (char *) still_framerate, "-I", "p", "-n", "1", "-j", pict, "-A", img->aspectratio, NULL};"""


def main():
    p = SRC / "menu.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    n = 0
    for old, new, tag in ((OLD, NEW, "norm / framerate 覆盖"),
                          (OLD2, NEW2, "jpeg2yuv 用覆盖后的帧率")):
        if t.count(old) != 1:
            print("[FAIL] %s 匹配 %d 次（应为 1）" % (tag, t.count(old)))
            return 1
        t = t.replace(old, new, 1)
        n += 1
    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 静图改用 NTSC（30000/1001、norm=n）—— %d 处" % n)
    return 0


if __name__ == "__main__":
    sys.exit(main())
