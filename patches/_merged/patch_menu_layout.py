#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""菜单排版改用「每页容量」，修掉 >34 轨的组必崩的栈溢出。

`command->maxntracks` 被赋成**整组最大轨数**（amg2.c:165
`command->maxntracks = MAX(track, command->maxntracks)`），
而菜单排版把它当成「一页有几行」用：

  xml.c  compute_coordinates()：
      uint16_t y0[MAX_BUTTON_NUMBER], y1[MAX_BUTTON_NUMBER];   // 36
      for (j = 1; j < command->maxntracks + 2; ++j)
          y1[j] = ...; y0[j] = ...;
      随后还用到 y0[command->maxntracks]、y0[command->maxntracks + 1]

`MAX_BUTTON_NUMBER` 是 36，所以 maxntracks >= 35 时就越界写栈数组 ——
实测 40 轨的组直接 `*** stack smashing detected ***` 中止。
（盘1 的组1 有 65 轨，必然中招；没有菜单时不走这段代码，所以以前没暴露。）

正确的「行数」应当是每页的按钮数 `img->maxbuttons`
（= Min(32, ceil(总轨数 / 页数))，见 patch_menu_paging.py），
它既受屏幕上限约束（<=32 < 34，不会越界），
又是排版真正需要的行数 —— 而且必须与 spumux 按钮坐标一致，
否则文字与按钮会错位（menu.c 画文字、xml.c 画按钮，两边共用这个值）。

本补丁把 menu.c / xml.c 里**全部** `command->maxntracks` 换成
`img->maxbuttons`。两处已核实 `img` 与 `command->img` 是同一对象：
  · amg2.c:82  `#define img command->img`
  · 两个函数都由 amg2.c 以 `img == command->img` 调用
所有这些出现位置都在菜单排版路径内，没有别的语义。
"""
import pathlib
import sys

FILES = [
    # (路径, 替换成什么)
    # menu.c 里这些函数都有 img 参数
    (pathlib.Path("/root/dvda-author-mlp8/src/menu.c"),
     "img->maxbuttons"),
    # xml.c 的 compute_coordinates() 只有 command（无 img），统一走 command->img
    (pathlib.Path("/root/dvda-author-mlp8/src/xml.c"),
     "command->img->maxbuttons"),
]
OLD = "command->maxntracks"

total = 0
for p, NEW in FILES:
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        sys.exit(1)
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    n = t.count(OLD)
    if n == 0:
        # 幂等：已替换过就跳过（不要再做任何"还原"，那会误伤别处新增的
        # img->maxbuttons，例如分页补丁写入的赋值行）
        print("[SKIP] %s 已无 %s（应已应用）" % (p.name, OLD))
        continue
    p.write_text(t.replace(OLD, NEW), encoding="utf-8", errors="surrogateescape")
    print("[OK] %s：%d 处 %s -> %s" % (p.name, n, OLD, NEW))
    total += n

if total:
    print("\n菜单排版修复完成，共替换 %d 处" % total)
else:
    print("\n未替换任何内容")
    sys.exit(1)
