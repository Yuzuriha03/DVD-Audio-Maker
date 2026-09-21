#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""让一个音频组只生成**一个 title**，从而支持「下一首 / 上一首」逐轨切歌。

## 症状

PowerDVD / 硬碟机里点「下一段」不能切歌：
第一首时按能跳到第二首，从第二首起再按就**回到该首开头**，永远走不到第三首。
手动选到第三、四首后按「下一段」还会跳回第二首。

## 根因

`amg2.c` 里决定「一个新 title 从哪里开始」的判断：

    if (samplerate != 前一轨.samplerate || bitspersample != ... || channels != ...
        || cga != ...
        || files[group][track].type     == AFMT_MLP     // ← 元凶
        || files[group][track - 1].type == AFMT_MLP)    // ← 元凶
      files[group][track].newtitle = 1;

紧挨着的注释写明了作者的态度：

> apparently MLP does not allow "gapless" same-audio characteristics titles, which
> means that 3 following tracks with same audio specs will create 3 titles instead
> of 1 for gapless PCM. TODO: check if this is software-dependent

也就是**上游出于「MLP 不能像 PCM 那样无缝接轨」的猜测**，让「前一轨或当前轨是
MLP」时一律另起 title。本项目全部音源都是 MLP，于是**每一首歌都自成一个
title**（实测：盘1 91 个 title、盘2 56 个 title，每个 title 恰好 1 轨；
旧的无菜单构建也完全一样，所以这不是菜单功能引入的）。

而 DVD 里「下一段 / 上一段」（下一章 / 上一章）的语义是
**在同一个 title 内前进到下一轨**。每个 title 只有一轨，播放器就无处可去，
只能退化成它自己的兜底行为 —— 表现为「按了没反应 / 跳回本首开头」。

## 修法

去掉 MLP 那两行条件，保留音频属性比较。于是一个音频组 = 一个 title，
title 内每首歌是一个 track（正是商业 DVD-Audio 盘的常规布局）：

    修前：title 1[轨1] title 2[轨2] title 3[轨3] ...   （盘2：56 title / 56 轨）
    修后：title 1[轨1 轨2 轨3 ... 轨56]               （盘2：1 title / 56 轨）

一次播放内会连续播完全组，播放器的「下一段」也就有了落点；
连带的收益是 AOB 内的间隙被消除，一张盘可以**一次连续播放到底**。

保留的属性比较仍然有意义：一个组内若中途中采样率/位深/声道数变化，
还是会正确地切成多个 title（本项目不会，分组已按属性做过）。

## 连带影响与验证

- **菜单不受影响**：菜单按钮是 `jump group G track K`，dvdauthor 侧
  `dvdvmy.y` 里 `TRACK_TOK NUM_TOK` 与 `TITLE_TOK NUM_TOK` 是同一个产生式
  （都 `|128`），VM 指令 `0x0A` 直接取 `i2-128` 当**曲目号**使用，
  与 title 粒度无关。（`dvdauthor-0.7.1/src/dvdcompile.c:994`）
- **播放封面（ASVS）预期保留**：静图机制是**按轨**工作的 ——
  ATSI 的静图记录是 `(图号, 轨号, onset)` 三元组，onset 相对各轨
  （每轨第一张图 → 0），且**只为有图的轨写记录**，没有记录的轨
  「沿用上一张」。ASVS 侧 `ntitlepics[组][title]` 会把 title 内各轨的图数
  累加，所以改成单 title 后只是把「28 条各 1 张」变成「1 条 28 张」，
  逐图扇区表（0x378）内容不变。
  但**这是推断，必须实盘验证**（PowerDVD 里逐首确认封面是否跟着变）。
- 上游那句「MLP 不允许无缝」若在**某些播放器**上成立，症状会是播放到
  轨边界出现卡顿/断音。实盘要留意。

## 连带改动：`ats.c` 必须「无条件按轨刷 pack」

只去掉 `amg2.c` 的分 title 条件会导致**段错误**（实测 `write_lpcm_header`
读 `info->mlp_layout[880418]` 越界）。因为 `ats.c` 里那个刷 pack 的块
**同时承担两件事**，而它原本挂在 `if (files[i].newtitle)` 上：

```c
if (i < ntracks)
  {
    if (files[i].newtitle)          // ← 原版每轨都成立（MLP 每轨自成 title）
      {
        write_pes_packet(fpout, &files[i-1], audio_buf, bytesinbuf, ...);
        ++pack;
        bytesinbuf = 0;
        pack_in_title = 0;          // ← 关键：每轨归零
        totpayload = 0;
      }
    files[i].first_sector = files[i-1].last_sector + 1;
```

两件事是：

1. **按轨对齐扇区**。`files[i].first_sector = files[i-1].last_sector + 1`
   只在「轨从 pack 边界开始」时才成立。不刷就会让下一轨从中途开始，
   读盘端 `get_ps1()` 取不到 pack 头 → **整首曲子被丢弃**
   （就是之前修过的那一类缺陷）。
2. **把 `pack_in_title` 归零**。MLP 的 `info->mlp_layout[]` 由
   `allocate_mlp_tracktable()` **按轨**分配，而
   `write_lpcm_header()` 用它算 PTS 偏移：

       frame_offset = info->mlp_layout[pack_in_title].pkt_pos - ...

   跨轨累加就会读到数组外面（实测 880418）→ 段错误。

所以改成**无条件按轨刷**。对 MLP 盘来说这与改动前的行为**完全一致**
（原版每轨都刷），只是标题结构从「每轨一个 title」变成「一组一个 title」，
因此 AOB 的字节内容不变，变的只有 ATSI / AMG 的标题表。

## 还原方法

本补丁改动两处（`amg2.c` 去掉 MLP 分标题条件，`ats.c` 改为无条件按轨刷），
把两处都改回原样即可；`build_dvda_author_mlp.sh` 的 `[1b]` 会把
`src/amg2.c` 与 `src/ats.c` 从原始源码还原，所以停用本脚本即可回到旧行为。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/amg2.c")
PATH_ATS = pathlib.Path("/root/dvda-author-mlp8/src/ats.c")

# ---- amg2.c：去掉「MLP 只成 title」 ----

OLD = """          // PATCH 13.11 on 12.06
          // PATCH MLP: TODO check this
          // Note: apparently MLP does not allow "gapless" same-audio characteristics titles, which
          // means that 3 following tracks with same audio specs will create 3 titles instead of 1 for
          // gapless PCM. TODO: check if this is software-dependent

          if (track)
            {
              if (command->files[group][track].samplerate     != command->files[group][track - 1].samplerate
                  || command->files[group][track].bitspersample != command->files[group][track - 1].bitspersample
                  || command->files[group][track].channels      != command->files[group][track - 1].channels
                  || command->files[group][track].cga           != command->files[group][track - 1].cga
                  || command->files[group][track].type          == AFMT_MLP
                  || command->files[group][track - 1].type      == AFMT_MLP)
                {
                  command->files[group][track].newtitle = 1;
                }
            }
          else
            command->files[group][track].newtitle = 1;
"""

NEW = """          /* PATCH 2026-09-21  原版在这里额外加了
                「本轨或前一轨是 MLP → 另起 title」，依据是「MLP 不允许无缝接轨」
                的猜测（见原注释）。后果是**每首歌都自成一个 title**，而 DVD 的
                「下一段 / 上一段」是在同一 title 内换轨 → 播放器无处可去，
                表现为按了没反应或跳回本首开头。

                去掉这两个 MLP 子句后，一个音频组 = 一个 title、title 内每首歌
                一个 track（商业 DVD-Audio 盘的常规布局），
                「下一段」就能逐轨前进，整组也能连续播放到底。

                保留音频属性比较：组内若中途中采样率/位深/声道数变化，
                仍会正确地切成多个 title。 */
          if (track)
            {
              if (command->files[group][track].samplerate     != command->files[group][track - 1].samplerate
                  || command->files[group][track].bitspersample != command->files[group][track - 1].bitspersample
                  || command->files[group][track].channels      != command->files[group][track - 1].channels
                  || command->files[group][track].cga           != command->files[group][track - 1].cga)
                {
                  command->files[group][track].newtitle = 1;
                }
            }
          else
            command->files[group][track].newtitle = 1;
"""

# 幂等性标记
MARK = "PATCH 2026-09-21  原版在这里额外加了"
MARK_ATS = "每个音轨边界都必须把上一轨的余量刷成一个独立 pack"

ATS_OLD = """              if (i < ntracks)
                {
                  /* If the current track is a different audio format, we must
                     start a new title. */

                  if (files[i].newtitle)
                    {
                      // Empty audio buffer

                      write_pes_packet(fpout,
                                       &files[i - 1],
                                       audio_buf,
                                       bytesinbuf,
                                       pack_in_title,
                                       start_of_file,
                                       globals);

                      ++pack;
                      bytesinbuf = 0;
                      pack_in_title = 0;
                      totpayload = 0;
                    }
"""

ATS_NEW = """              if (i < ntracks)
                {
                  /* 每个音轨边界都必须把上一轨的余量刷成一个独立 pack。

                     这件事原来挂在 `files[i].newtitle` 上 —— 原版每轨都自成一个
                     title，所以恰好每轨都刷。去掉 MLP 分 title 条件后必须改成
                     **无条件**，否则会出两个问题：

                       1. `pack_in_title` 跨轨累加，而 MLP 的
                          `info->mlp_layout[]` 是**按轨**分配的
                          （allocate_mlp_tracktable），
                          write_lpcm_header() 里
                          `mlp_layout[pack_in_title]` 会越界读 → 段错误；
                       2. `files[i].first_sector = files[i-1].last_sector + 1`
                          不再成立于 pack 边界，下一轨从扇区中途开始，
                          读盘端取不到 pack 头 → 整首被丢弃。 */
                  {
                    write_pes_packet(fpout,
                                     &files[i - 1],
                                     audio_buf,
                                     bytesinbuf,
                                     pack_in_title,
                                     start_of_file,
                                     globals);

                    ++pack;
                    bytesinbuf = 0;
                    pack_in_title = 0;
                    totpayload = 0;
                  }
"""

def patch_one(path, marker, old, new, what):
    """改一个文件；已经是新版就跳过。"""
    if not path.exists():
        print("[FAIL] 找不到 %s" % path)
        return False
    text = path.read_text(encoding="utf-8", errors="surrogateescape")
    if marker in text:
        print("[SKIP] %s 已经是新版（%s）" % (path.name, what))
        return True
    n = text.count(old)
    if n != 1:
        print("[FAIL] %s: %s 匹配 %d 次（应为 1；有多个副本必须查清再改）"
              % (path.name, what, n))
        return False
    path.write_text(text.replace(old, new, 1), encoding="utf-8",
                    errors="surrogateescape")
    print("[OK]   %s：%s" % (path.name, what))
    return True


def main():
    # 「两行 MLP 子句」必须只有一处，否则说明上游有重复副本
    # （这个坑踩过一次，见 TROUBLESHOOTING 16.24）。
    if PATH.exists():
        t = PATH.read_text(encoding="utf-8", errors="surrogateescape")
        if MARK not in t:
            n = t.count("|| command->files[group][track].type          == AFMT_MLP")
            if n != 1:
                print("[FAIL] 找到 %d 处 MLP 分 title 的条件，预期恰好 1 处"
                      % n)
                return 1

    if not patch_one(PATH, MARK, OLD, NEW,
                     "去掉 MLP 强制分 title（一个音频组 = 一个 title）"):
        return 1
    # ⚠️ 必须同时改 ats.c：那个刷 pack 的块挂在 newtitle 上，去掉分 title
    # 后会跨轨累加 pack_in_title → mlp_layout[] 越界 → 段错误。
    if not patch_one(PATH_ATS, MARK_ATS, ATS_OLD, ATS_NEW,
                     "按轨刷 pack 改为无条件"):
        return 1

    print("\n单 title 补丁完成（一个音频组 = 一个 title）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
