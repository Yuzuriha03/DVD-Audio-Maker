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

本补丁改动多处（`amg2.c` 去掉 MLP 分标题条件；`ats.c` 改为无条件按轨刷 +
title 内 PTS 连续；`atsi2.c` 每轨都标记为曲目起点；`structures.h` 加字段），
把各处改回原样即可；`build_dvda_author_mlp.sh` 的 `[1b]` 会把这些文件从原始
源码还原，所以停用本脚本即可回到旧行为。

## 各处的验证状态

| 改动 | 状态 |
|---|---|
| `amg2.c` 一个 title | ✅ 已验证（`numtitles=1`、`tracks=N`） |
| `ats.c` 按轨刷 pack | ✅ 已验证（不刷会段错误） |
| `ats.c` title 内 PTS 连续 | ✅ 已验证（cell `first_pts` 递增，AOB 无回落） |
| `atsi2.c` 每轨标为曲目起点 | ⚠️ **实验性，待真机验证** |

### 关于 `atsi2.c` 的那个标记（0xC000）

它在时间戳记录首字节，dvda-author 只给 `t == 0`（本 title 第 1 轨）加。
原版「每轨一个 title」时每轨都是 t==0，所以**每轨都带**；合并成一个 title 后
只有第 1 轨带（实测盘2：cell1 = `0xC010`，cell2..56 = `0x0010`）。

实测现象支持「这个位是曲目起点」：PowerDVD 显示「0/56」→ 按下一曲变
「1/56」但**音频不动**（也就是它认得出 56 个条目、知道下一首是谁，但只认得出
**一个起点**，于是永远落到第一个起点 = 第 1 首）。把该位补到每一轨后，
每轨都成为合法起点 —— **需在真机上确认「下一曲」是否恢复**。

若无效，说明它只对 `t == 0` 有意义，把 `x |= 0xc000;` 换回
`if (t == 0) x |= 0xc000;` 即可（其余改动保留）。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/amg2.c")
PATH_ATS = pathlib.Path("/root/dvda-author-mlp8/src/ats.c")
PATH_ATSI = pathlib.Path("/root/dvda-author-mlp8/src/atsi2.c")
PATH_SH = pathlib.Path("/root/dvda-author-mlp8/src/include/structures.h")

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
MARK_ATSI = "0xC000 = 「这里是一个曲目的起点」"

# ---- atsi2.c：把「曲目起点」标记打到**每一轨**上 ----
#
# 每个 cell 的时间戳记录首字节是「轨类型」：dvda-author 只给 **t == 0**
# （本 title 的第 1 轨）加上 0xC000。原版「每轨一个 title」时每轨都是 t == 0，
# 所以**每一轨**都带这个标记；合并成一个 title 后只有第 1 轨带，其余变成
# 0x0000 —— 实测盘2：cell1 = 0xC010，cell2..56 = 0x0010。
# 如果这个位表示「曲目起点」，播放器就会变成「认得出 56 个条目（靠 indexes
# 与时间戳表条数），但只认得出一个起点」——「下一曲」永远落到第一个起点，
# 也就是第 1 首（实测现象：曲目号会从 0 变到 1，但音频不移动）。
X_OLD = """          x = (x * 8) << 8;

          if (t == 0)
            {
              x |= 0xc000;
            }
"""

X_NEW = """          x = (x * 8) << 8;

          /* 0xC000 = 「这里是一个曲目的起点」。

             dvda-author 原来只给 t == 0（本 title 的第 1 轨）加这个位 ——
             而原版是「每轨一个 title」，于是**每一轨**都是某个 title 的第 1 轨。
             合并成一个 title 后只有第 1 轨带这个位，其余 55 轨变成 0x0000：
             播放器认得出有多少个条目（indexes / 时间戳表条数），却只认得出
             **一个起点**，于是「下一曲」永远落到第一个起点 = 第 1 首
             （实测：曲目号会变，但音频不移动）。

             故对所有轨都打上该标记，恢复到合并前的语义。 */
          x |= 0xc000;
"""

# ---- fileinfo_t 追加 pts_shift ----
FI_OLD = """    char    **given_channel;
    struct  MLP_LAYOUT *mlp_layout;
} fileinfo_t;"""

FI_NEW = """    char    **given_channel;
    struct  MLP_LAYOUT *mlp_layout;
    /* 该轨在一个 title（PGC）时间轴上的起点偏移。MLP 的 pts[] 是按轨的
       （每轨从 PTS0 起算），合并成一个 title 后必须加上这个偏移才能让
       title 内的时间轴连续 —— 否则每个 cell 的 first_pts 都是 98，
       播放器按时间轴寻址任何一首都会落到第 1 首。
       见 src/ats.c 里 write_pes_packet 与按轨刷 pack 处的说明。 */
    uint32_t pts_shift;
} fileinfo_t;"""

# ---- create_ats 的变量声明 ----
DECL_OLD = """  uint8_t audio_buf[AUDIO_BUFFER_SIZE];
  uint64_t pack_in_title = 0;"""

DECL_NEW = """  uint8_t audio_buf[AUDIO_BUFFER_SIZE];
  uint64_t pack_in_title = 0;
  /* 一个 title 内的 PTS 累计偏移（见下方按轨刷 pack 处与 write_pes_packet）。 */
  uint32_t pts_shift = 0;"""

# ---- write_pes_packet 的 PTS/DTS/SCR 平移 ----
PTSOLD = """      mlp_flag = 0x80;
      if (pack_in_title == 0) cumbytes = 0;
      PTS = calc_PTS(info, pack_in_title, &cumbytes, globals);
      SCR = calc_SCR(info, pack_in_title);
    }

  static bool wait_for_next_pack;"""

PTSNEW = """      mlp_flag = 0x80;
      if (pack_in_title == 0) cumbytes = 0;
      PTS = calc_PTS(info, pack_in_title, &cumbytes, globals);
      SCR = calc_SCR(info, pack_in_title);
    }

  /* 把本轨平移到它所属 title（PGC）的时间轴上。

     MLP 的 pts[]/dts[]/scr[] 是按轨算的，每轨都从 PTS0（实测 98）起算。
     原版每轨自成一个 title，所以每轨一条时间轴、没问题；合并成一个 title 后
     必须整体平移，否则 PGC 的时间轴会断成 N 段各自从 98 开始：
       · 每个 cell 的 first_pts 都等于 98（first_PTS 就是在这里取的首个 PTS）
       · 播放器按时间轴寻址任何一首都会落到 PTS 98 = 第 1 首
       · 症状：不管哪首按「下一曲」都跳回曲目 1
     SCR 与 PTS 同源（27MHz / 90kHz = 300），按同比例平移以保持一致。 */
  PTS += info->pts_shift;
  DTS += info->pts_shift;
  SCR += (uint64_t) info->pts_shift * 300;

  static bool wait_for_next_pack;"""

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

                  /* 累计到下一轨的 PTS 偏移。

                     MLP 的 pts[]/dts[]/scr[] 是**按轨**的（每轨从 PTS0 = 98
                     起算、轨内单调）。原版每轨自成一个 title，每轨一条时间轴，
                     这没问题；现在整组是一个 title，而 DVD 里一个 title（PGC）
                     只有**一根时间轴**，必须把后续轨整体平移，否则 56 个 cell
                     的 first_pts 全是 98 —— 播放器按时间轴寻址任何一首都会落到
                     PTS 98 处，也就是**第 1 首**。
                     症状：不管哪首按「下一曲」都跳回曲目 1。

                     偏移量用上一轨声明的时长 PTS_length（与 AMG 里的 title
                     长度同源），小幅间隔（实测 < 1000 ticks，< 11 ms）无害。 */
                  pts_shift += files[i - 1].PTS_length;
                  files[i].pts_shift = pts_shift;
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


def apply(path, pairs):
    """逐条替换；要求每条 OLD 恰好出现一次（多个副本必须查清再改）。"""
    text = path.read_text(encoding="utf-8", errors="surrogateescape")
    for old, new, what in pairs:
        n = text.count(old)
        if n != 1:
            print("[FAIL] %s: %s 匹配 %d 次（应为 1）" % (path.name, what, n))
            return False
        text = text.replace(old, new, 1)
    path.write_text(text, encoding="utf-8", errors="surrogateescape")
    for _o, _n, what in pairs:
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
    # ⚠️ 必须同时改 ats.c：那个刷 pack 的块挂在 newtitle 上，去掉分 title 后
    # 会跨轨累加 pack_in_title → mlp_layout[] 越界 → 段错误；
    # 而且 MLP 的 pts[] 是按轨的，必须加上累计偏移才能让一个 title 内时间轴连续。
    if not apply(PATH_ATS, [
            (ATS_OLD, ATS_NEW, "按轨刷 pack 改为无条件，并累计 title 内的 PTS 偏移"),
            (DECL_OLD, DECL_NEW, "create_ats 声明 pts_shift"),
            (PTSOLD, PTSNEW, "write_pes_packet 把 PTS/DTS/SCR 平移到 title 时间轴"),
    ]):
        return 1
    if not apply(PATH_SH, [(FI_OLD, FI_NEW, "fileinfo_t 追加 pts_shift")]):
        return 1
    if not apply(PATH_ATSI,
                 [(X_OLD, X_NEW, "每一轨都标记为曲目起点（0xC000）")]):
        return 1

    print("\n单 title 补丁完成（一个音频组 = 一个 title，时间轴连续，每轨可寻址）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
