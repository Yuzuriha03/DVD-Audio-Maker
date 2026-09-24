#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""按菜单页数撑大 AMG 缓冲 —— 页数多时 `uint8_t amg[sectors->amg * 2048]` 会溢出。

`launch_manager.c` 里

    sectors.amg = SIZE_AMG + (globals->text ? 8 : 0)
                + (globals->topmenu <= TS_VOB_TYPE);

`SIZE_AMG` 是常量 3，于是 `sectors.amg` 恒为 4（开菜单时）。
而 `amg2.c` 的 `create_amg()` 里

    uint8_t amg[sectors->amg * 2048];        // 4 * 2048 = 8192 字节

要写**随页数增长**的菜单表（`menusector` 分支，从 `amg[0x1820]` 起）：

    页索引   : 8 * (nmenus - 1)
    每页条目 : 0x13A 字节，共 nmenus 份
    → 需要 0x1820 + 8*(nmenus-1) + nmenus*0x13A 字节

| 页数 | 需要 | 缓冲 | 结果 |
|---|---|---|---|
| 1 | ~6490 | 8192 | ✔ |
| 4 | ~7456 | 8192 | ✔（40 轨测试盘通过的页数） |
| **9** | **~9066** | 8192 | **✗ 越界约 900 字节** |

越界写的是**栈**（VLA），所以崩点离真正原因很远：实测一路跑到
`amg2.c` 的 `create_amg()` 深处才 `.data` 段错误，而前面的菜单编码、
spumux、dvdauthor 全都正常完成 —— 极难定位。

本补丁按页数把 `sectors.amg` 撑到够用。`sectors.amg` 同时决定
`sizeofamg = sizeof(amg)` 与写盘长度、以及各处扇区指针
（`2*sectors->amg + ...`、`sectors->amg - 1`、`menusector * sectors->amg`），
所以它会自动跟着调整，盘上 IFO 也跟着变大，是自洽的。
"""
import pathlib
import sys

PATH = pathlib.Path(
    "/root/dvda-author-mlp8/src/launch_manager.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """  // Late initialization
  sectors.amg = SIZE_AMG + (globals->text ? 8 : 0) + (globals->topmenu <= TS_VOB_TYPE);
"""

NEW = """  // Late initialization
  sectors.amg = SIZE_AMG + (globals->text ? 8 : 0) + (globals->topmenu <= TS_VOB_TYPE);

  /* 菜单页数多时把 AMG 撑够。amg2.c 的 create_amg() 用
     `uint8_t amg[sectors->amg * 2048]` 这个 VLA，而菜单表（从 amg[0x1820]
     起，每页 0x13A 字节 + 8*(nmenus-1) 页索引）随页数增长 ——
     固定 4 扇区（8192 字节）在 9 页时需 ~9066 字节，越界写栈。
     越界后崩点离原因很远（跑完菜单编码/spumux/dvdauthor 才在
     create_amg() 里段错误），故必须在这里撑够。
     nmenus 在命令行解析阶段（menu_characteristics_coherence_test）已定型。 */
  if (globals->topmenu <= TS_VOB_TYPE && img->nmenus > 1)
    {
      uint32_t need = 0x1820 + 8 * (img->nmenus - 1) + img->nmenus * 0x13A;
      uint32_t need_sectors = (need + 2047) / 2048;
      if (need_sectors > sectors.amg)
        {
          if (globals->debugging)
            foutput("%s%d%s%u\\n", MSG_TAG "AMG buffer grown for ",
                    img->nmenus, " menu screens (sectors): ", need_sectors);
          sectors.amg = need_sectors;
        }
    }
"""

# 幂等判据必须先看「新内容是否已就位」：
# OLD 是 NEW 的**前缀**（NEW = OLD + 增长块），所以应用之后 OLD 依然能匹配 ——
# 用 `if OLD not in text` 作守卫会每次再插一份（实测曾因此叠出 3 份）。
if "AMG buffer grown for" in text:
    print("[SKIP] 已应用过")
    sys.exit(0)

if OLD not in text:
    print("[MISS] 未找到 sectors.amg 初始化")
    sys.exit(1)

if text.count(OLD) != 1:
    print("[MISS] 匹配到 %d 处，需唯一" % text.count(OLD))
    sys.exit(1)

PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                errors="surrogateescape")
print("[OK] sectors.amg 按菜单页数撑大")
print("\nAMG 缓冲补丁完成，共 1 项")
