#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修菜单翻页箭头：文字错位（第 2 页起跑到页面顶部）+ 末页重复画 5 次。

## 缺陷一：箭头文字从第 2 页起全部错位

`menu.c` 画箭头文字时把 `offset` 传进了 `mogrify_img()`：

    mogrify_img(arrowstring, img->ncolumns - 1, img->maxbuttons,
                img, img->maxbuttons, command1, command2, offset, img->arrowcolor);
                                        ^^^^^^ track            ^^^^^^ offset

而 `mogrify_img()` 里是

    y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));

`offset` 的语义是「**本页首轨的全局序号**」，它是给曲名用的（让每页第 1 首
都画在第 1 行）。但箭头用的是**固定绝对行号** `img->maxbuttons` /
`img->maxbuttons + 1`（页面底部的箭头槽，与 `xml.c` 里输出的按钮坐标同源），
不该再减 offset。

于是：

| 页 | offset | `y(track+1-offset)` | 结果 |
|---|---|---|---|
| 第 1 页 | 0（初始值） | y(13) | 底部 ✔ |
| 第 2 页起 | >0 | `12+1-offset` | **顶部第 1、2 行** ✗ |
| 末页 | 0（走完整个组时被复位） | y(13) | 底部 ✔ |

实测（56 轨 5 页，逐页提取文字墨迹的行区间）：

```
页 0  墨迹 … 408-430, 441-455      ← 441-455 是底部的 Next
页 1  墨迹 … 378-400, 408-429      ← 没有 441-455！箭头被画到了第 1、2 行
页 4  墨迹 … 288-309, 441-455      ← 底部 Previous
```

也就是说**第 2 页到倒数第 2 页**，箭头文字压在最先两首曲名上，
而底部的按钮位置没有文字 —— 看起来就是「翻页后不显示 Previous/Next」。

修法：箭头调用一律传 `offset = 0`（用绝对行号定位）。

## 缺陷二：末页把同一个箭头重复画 5 次

原来是 `do { ... } while (buttons < menubuttons + arrowbuttons);`。
`buttons` 进入这个循环前是**本页已画的按钮数**，而 `menubuttons` 仍是
「满页容量」：曲目正好填满时两者正好接上，但**末页只有 8 首**（56 - 4×12）
时 `buttons = 8`、目标是 `12 + 1 = 13` → 循环 5 轮，把同一个 Previous
重复画在同一位置。

`xml.c` 里同构的循环会输出 5 个**完全重叠**的按钮，实测：

```
页 4   button09..button13 全部 y0=440..470   ← 5 个重叠按钮
```

修法：改成单次判断 —— 不该有重复。

## 附带清理

去掉 `char arrowstring[9]` + `strcpy`（`DEFAULT_PREVIOUS` 是 8 字符 + NUL = 9，
刚好塞满这个缓冲，本来就贴着边界）；直接把字面量传给 `mogrify_img()`
（该函数已有直接传字面量的用法）。

`menu.c` 与 `xml.c` 这两段必须**保持一致**（一个画文字、一个输出按钮），
所以本补丁同时改两处。
"""
import pathlib
import sys

FILES = {
    # (起点锚点, 结束锚点) —— 两文件的形状不同：
    #   menu.c: `if (...) do { ... } while (...);`  ← if 不带花括号
    #   xml.c : `if (...) { do { ... } while (...); }`  ← if 带花括号
    # 结束锚点必须把属于该 if 块的花括号一起吃掉，否则会多出一个 `}`。
    "/root/dvda-author-mlp8/src/menu.c": (
        "      if ((img->nmenus > 1) && (menu < img->nmenus))",
        "        while (buttons < menubuttons + arrowbuttons);",
    ),
    "/root/dvda-author-mlp8/src/xml.c": (
        "      if (img->nmenus > 1)",
        "          while (buttons < menubuttons + arrowbuttons);\n        }",
    ),
}

MENU_NEW = '''      if ((img->nmenus > 1) && (menu < img->nmenus))
        {
          /* 箭头文字必须用**绝对行号**定位，与 xml.c 输出的按钮坐标一一对应：
             两者都取 img->maxbuttons / img->maxbuttons + 1 两行 ——
             也就是页面底部的两个箭头槽。
             ⚠️ 原式把 offset 传了进来，而 offset 是「本页首轨的全局序号」，
             mogrify_img() 里算的是 y(track + 1 - offset)。于是第 2 页起
             算到的是**页面顶部**：箭头压在第 1、2 行的曲名上，而底部真正的
             按钮位置没有文字。（第 1 页与末页的 offset 恰为 0，所以正常。）
             另外原来的 do-while 在曲目不满一页的末页会因 buttons 基数偏低
             而多转几轮，把同一个箭头重复画 5 次。 */
          int arrows_drawn = 0;

          buttons++;
          arrows_drawn++;
          mogrify_img((menu == img->nmenus - 1) ? DEFAULT_PREVIOUS : DEFAULT_NEXT,
                      img->ncolumns - 1, img->maxbuttons, img,
                      img->maxbuttons, command1, command2, 0, img->arrowcolor);

          if ((menu) && (menu < img->nmenus - 1))
            {
              buttons++;
              arrows_drawn++;
              mogrify_img(DEFAULT_PREVIOUS, img->ncolumns - 1,
                          img->maxbuttons + 1, img, img->maxbuttons,
                          command1, command2, 0, img->arrowcolor);
            }

          if (globals->debugging && (arrows_drawn != (int) arrowbuttons))
            foutput(WAR "Arrow count mismatch: %d drawn, %d expected\\n",
                    arrows_drawn, (int) arrowbuttons);
        }'''

XML_NEW = '''      if (img->nmenus > 1)
        {
          /* 与 menu.c 的箭头文字一一对应（同一套槽位与页判定）：
             第 1 个槽放 Next（末页改放 Previous），中间页再补一个 Previous。
             ⚠️ 原 do-while 在曲目不满一页的末页会因 buttons 基数偏低而多转
             几轮，输出 5 个完全重叠的按钮。 */
          int arrows_emitted = 0;

          buttons++;
          arrows_emitted++;
          fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\\n", "       <button x0=\\"", x0[img->ncolumns - 1], "\\"", " y0=\\"", y0[command->img->maxbuttons], "\\"", " x1=\\"", x1[img->ncolumns - 1], "\\"", " name=\\"button", buttons, "\\"", " y1=\\"", y1[command->img->maxbuttons], "\\"/>");

          if ((menu) && (menu < img->nmenus - 1))
            {
              buttons++;
              arrows_emitted++;
              fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\\n", "       <button x0=\\"", x0[img->ncolumns - 1], "\\"", " y0=\\"", y0[command->img->maxbuttons + 1], "\\"", " x1=\\"", x1[img->ncolumns - 1], "\\"", " name=\\"button", buttons, "\\"", " y1=\\"", y1[command->img->maxbuttons + 1], "\\"/>");
            }

          if (globals->debugging && (arrows_emitted != (int) arrowbuttons))
            foutput(WAR "Arrow button count mismatch: %d emitted, %d expected\\n",
                    arrows_emitted, (int) arrowbuttons);
        }'''

ok = 0
for path, (start_marker, end_marker) in FILES.items():
    p = pathlib.Path(path)
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        sys.exit(1)
    text = p.read_text(encoding="utf-8", errors="surrogateescape")

    if start_marker not in text:
        if "arrows_drawn" in text or "arrows_emitted" in text:
            print("[SKIP] %s 已应用过" % p.name)
            ok += 1
            continue
        print("[MISS] %s 里找不到箭头块起点" % p.name)
        sys.exit(1)
    if text.count(start_marker) != 1:
        print("[MISS] %s 的起点锚点出现 %d 次，需唯一"
              % (p.name, text.count(start_marker)))
        sys.exit(1)

    i = text.index(start_marker)
    j = text.find(end_marker, i)
    if j < 0:
        print("[MISS] %s 里在箭头块之后找不到结束锚点" % p.name)
        sys.exit(1)

    new_block = MENU_NEW if p.name == "menu.c" else XML_NEW
    # 自检：替换后这一段的括号必须平衡，否则会写出无法编译的代码
    if new_block.count("{") != new_block.count("}"):
        print("[FAIL] %s 的新代码块花括号不平衡" % p.name)
        sys.exit(1)

    text = text[:i] + new_block + text[j + len(end_marker):]
    p.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("[OK] %s：箭头块已重写（offset=0 + 去掉重复循环）" % p.name)
    ok += 1

print("\n菜单箭头修复完成，共 %d 个文件" % ok)
