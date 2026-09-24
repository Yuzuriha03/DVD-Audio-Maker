#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 AMG 菜单 table 里每个菜单的 cell 结束地址用错大小（翻页跳转失效）。

## 症状

菜单里「Next / Previous」翻页按钮按了无效、回不到上一页（**且不止一页**），
而**选曲按钮正常**。

这个「只有翻页坏、选曲不坏」的不对称正是线索：
- 选曲按钮走 `jump group G track K` → 经 ATSI 定位到音频区，与菜单表无关
- 翻页按钮走 `jump menu N` → 由播放器查 **AMG IFO 的菜单 PGC 表**，而那张表是
  `amg2.c` **手写**的（逆向出来的结构）

## 根因

`amg2.c` 的菜单表里，每个菜单的 cell 结束地址这么算：

    if (j > 1) menuvobsize_sum += img->menuvobsize[j - 2] - 1;   // 累加「前面」各页
    uint32_check(&amg[i], menuvobsize_sum + img->menuvobsize[img->nmenus - 1] - 1 - 1);
                                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 恒为**最后一页**的大小

`menuvobsize_sum` 是前面各页的累计，但加上去的是 `menuvobsize[nmenus-1]` ——
**最后一页的大小**，而不是当前这一页。于是：

- `nmenus == 1` 时它恰好就是当前页 → 正确（这是它一直没被发现的原因）
- `nmenus > 1` 时，第 1..n-1 页的结束地址全错

而且**错误会被各页大小相近掩盖**：本项目 8 页的 VOB 分别是
37/40/37/43/39/39/41/39 扇区，算出

    正确： [35, 74, 110, 152, 190, 228, 268, 306]
    实际： [37, 73, 112, 148, 190, 228, 266, 306]
                                  ^^^  ^^^            ^^^
                                  巧合相同

⇒ **第 5、6、8 页恰好正确，其余 5 页错误**，与「不止一页有问题」完全吻合。

在成品 IFO 里核对（`AUDIO_TS.IFO` 的菜单表，8 个地址等间距出现）：

    代码写出的值 37/73/112/148/190/228/266/306   ← 全部在 IFO 里找到
    正确值       35/74/110/152/268               ← 一个都不存在

## 修复

把 `img->menuvobsize[img->nmenus - 1]` 换成 `img->menuvobsize[j - 1]`
（当前页），其余算式不动 —— `nmenus == 1` 时行为完全不变。

修正后的算式等价于「累计到当前页为止的有效扇区数 - 1」：

    期望 = Σ_{i<j} (sizes[i] - 1) - 1

（减 1 是 dvdauthor 处理 topmenu 时会丢一个数据扇区，原注释已说明）

起始地址一侧不需要改：cell j 的 start 用的就是 `menuvobsize_sum`，
而 cell j-1 的 end + 1 恰好等于它 —— 两边自洽。

顺带加两条自检（`globals->debugging` 时打印），以及 `verify_menu.py` 里的
**cell 连续性校验**：`start_j == end_{j-1} + 1` 且 `start <= end`。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/amg2.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """          if (j > 1)
            {
              menuvobsize_sum += img->menuvobsize[j - 2] - 1;
              uint32_check(&amg[i], menuvobsize_sum); // in the course of processing dvdauthor over topmenus, one data sector added by spumux is lost
              i += 8; // 4 bytes of padding
              uint32_check(&amg[i], menuvobsize_sum);  // repeat
              i += 4;
            }
          else i += 12;
          uint32_check(&amg[i], menuvobsize_sum + img->menuvobsize[img->nmenus - 1] - 1 - 1);
          i += 4;
"""

NEW = """          if (j > 1)
            {
              menuvobsize_sum += img->menuvobsize[j - 2] - 1;
              uint32_check(&amg[i], menuvobsize_sum); // in the course of processing dvdauthor over topmenus, one data sector added by spumux is lost
              i += 8; // 4 bytes of padding
              uint32_check(&amg[i], menuvobsize_sum);  // repeat
              i += 4;
            }
          else i += 12;
          /* cell 结束地址：必须用**当前这一页**的大小（j-1），
             原式写的是 menuvobsize[nmenus-1]（恒为最后一页），
             只有 nmenus == 1 时才恰好正确 —— 多页时第 1..n-1 页的结束地址全错，
             表现为「翻页按钮点了没反应 / 回不到上一页」，
             而各页 VOB 大小接近时错误会在某些页上相互抵消（实测 8 页里 5 页错）。
             起始地址一侧不用改：cell j 的 start = menuvobsize_sum
             = cell j-1 的 end + 1，两边自洽。 */
          uint32_t cell_end = menuvobsize_sum + img->menuvobsize[j - 1] - 1 - 1;
          uint32_check(&amg[i], cell_end);
          if (globals->debugging)
            foutput("%s%d%s%u%s%u%s%u%s", MSG_TAG "AMG menu cell ", j,
                    ": start=", menuvobsize_sum, " end=", cell_end,
                    " (vob size=", img->menuvobsize[j - 1], " sectors)\\n");
          i += 4;
"""

# ⚠️ amg2.c 里有**两份**这段代码，分别属于：
#   · create_topmenu()  —— 写的是 dvdauthor/spumux 跑之前的**占位** IFO
#   · create_amg()      —— 写的是**最终** IFO（落在盘上的那份）
# 两者只差 `uint32_check` vs `uint32_copy`，所以第一版补丁只匹配到前者、
# 修了「看起来对但盘上不用」的那一处，成品 IFO 里的地址依旧是旧值。
# 两份都要修，且以 create_amg 的那份为主。
OLD2 = OLD.replace("uint32_check", "uint32_copy")
NEW2 = NEW.replace("uint32_check", "uint32_copy")

if OLD not in text and "cell_end" in text and OLD2 not in text:
    print("[SKIP] 已应用过")
    sys.exit(0)

applied = 0
for old, new, who in ((OLD, NEW, "create_topmenu（占位 IFO）"),
                      (OLD2, NEW2, "create_amg（最终 IFO）")):
    if old not in text:
        if "cell_end" in text and text.count("cell_end") > applied:
            print("[SKIP] %s 已应用过" % who)
            applied += 1
            continue
        print("[MISS] 未找到 %s 的 cell 地址代码" % who)
        sys.exit(1)
    text = text.replace(old, new, 1)
    print("[OK] %s 的 cell 结束地址改用当前页大小" % who)
    applied += 1

PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("[OK] 调试模式会打印每个菜单的 start/end")
print("\nAMG 菜单 cell 地址修复完成（%d 处）" % applied)
