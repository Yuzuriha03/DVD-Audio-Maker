#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 `--stillpics`（每轨静图）与菜单共存时的段错误。

`pict` 是 menu.c 的**文件级** static 缓冲，在 `create_mpg()` 里按需分配，
分配判据是它自己的另一个 static（函数内）计数器 `s`：

    static unsigned long s;                 // 函数内 static
    ...
    if (s == 0)
      {
        s = MAX(strlen(globals->settings.stillpicdir) + 26,
                strlen(img->backgroundpic[rank]) + 1);
        pict  = calloc(s, sizeof(char *));
      }
    ...
    sprintf(pict, "%s" SEPARATOR "pic_%03u.jpg", globals->settings.stillpicdir, rank);

而 `generate_background_mpg()` 结束时会把 `pict` 置空：

    FREE(pict)        // { free(pict); pict = NULL; }

于是「先做菜单背景（ANIMATEDVIDEO）、再做静图背景（STILLPICS）」这条路径上，
第二次进入 create_mpg 时 `s != 0` 但 `pict == NULL`，
直接 `sprintf(NULL, ...)` → 段错误（实测）。

修法：把「pict 为空」也作为重新分配的判据。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/menu.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """  if (s == 0)
    {
      s = MAX(strlen(globals->settings.stillpicdir) + 26, strlen(img->backgroundpic[rank]) + 1);
      pict  = calloc(s, sizeof(char *));
    }
"""

NEW = """  /* pict 是文件级 static，且 generate_background_mpg() 末尾会 FREE(pict)
     把它置 NULL；而 s 是本函数 static、不会复位。于是第二次进入本函数
     （先菜单背景、再静图背景）时 s != 0 但 pict == NULL，
     下一句 sprintf 就写到 NULL —— 实测段错误。判据里补上 pict 是否为空。 */
  if ((s == 0) || (pict == NULL))
    {
      s = MAX(strlen(globals->settings.stillpicdir) + 26, strlen(img->backgroundpic[rank]) + 1);
      pict  = calloc(s, sizeof(char *));
    }
"""

if OLD not in text:
    if "pict == NULL" in text:
        print("[SKIP] 已应用过")
        sys.exit(0)
    print("[MISS] 未找到目标代码")
    sys.exit(1)

if text.count(OLD) != 1:
    print("[MISS] 匹配到 %d 处，需唯一" % text.count(OLD))
    sys.exit(1)

PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                errors="surrogateescape")
print("[OK] create_mpg 的 pict 重新分配判据已补 pict==NULL")
print("\n静图修复完成，共 1 项")
