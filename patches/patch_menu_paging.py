#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修复菜单分页：让「每页按钮数」真的是每页容量，而不是把总数除以页数。

原式（menu.c 两处）：
    img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
    img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;

语义应当是：maxbuttons = 每页能画几个按钮，resbuttons = 末页额外多出的几个。
但原式先把总数**截到 32** 再**除以页数**，于是无论 nmenus 设多大，
所有页加起来最多只有 32 个按钮 —— 89 首的盘只有前 32 首能进菜单，
其余全部点不到（且不报错）。

对照：xml.c 分层分支里 `maxbuttons = Min(..., ntracks[groupcount])` 才是
正确用法（一页一个组的容量）—— 可见原意就是「每页容量」，
非分层分支写错了。

改法：
    maxbuttons = Min(32, ceil(totntracks / nmenus))   // 每页容量
    resbuttons = 0                                     // 末页由循环自然截断

配合调用方传 `-6 --nmenus=ceil(总轨数/每页目标数)`，即可覆盖全部曲目。
（menu_characteristics_coherence_test 会由 nmenus 反推 ncolumns，
 故 nmenus=组数时 ncolumns 恰为 1 —— 单列长列表。）
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/menu.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0

NEW_BODY = """      /* 每页容量：总数按页数均分（向上取整），再受屏幕上限约束。
         原式先截 32 再除页数，导致所有页合计恒 <=32 个按钮。 */
      img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2,
                            (totntracks + img->nmenus - 1) / img->nmenus);
      img->resbuttons = 0;
"""


def rep(old, new, label):
    global text, n_ok
    if old not in text:
        print("[MISS] %s" % label)
        return False
    if text.count(old) != 1:
        print("[MISS] %s（匹配到 %d 处，需唯一）" % (label, text.count(old)))
        return False
    text = text.replace(old, new, 1)
    print("[OK] %s" % label)
    n_ok += 1
    return True


# 处 0：页数护栏只对分层菜单成立
rep(
    """          if ((img->ncolumns)*ngroups < img->nmenus - 1)
""",
    """          /* 该上限只对「分层菜单」成立（首页列组 + 每组一页）。
             非分层菜单的页数只受按钮总数约束，套用此式会把用户给的
             --nmenus 静默压成 ngroups*ncolumns+1（实测 3 组时 8 -> 4）。 */
          if (img->hierarchical && ((img->ncolumns)*ngroups < img->nmenus - 1))
""",
    "页数护栏仅限分层菜单",
)

# 处 1：menu_characteristics_coherence_test() —— 默认值设定（分层判断之后）
rep(
    """              img->nmenus = ngroups * img->ncolumns + 1;
            }

        }

      img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
      img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;
""",
    """              img->nmenus = ngroups * img->ncolumns + 1;
            }

        }

""" + NEW_BODY,
    "分页修正 #1（menu_characteristics_coherence_test）",
)

# 处 2：generate_menu_pics() —— 非分层分支
rep(
    """  int dim = 0, k, j;
  char **grouparray = NULL, **basemotif = NULL, *albumtext = NULL, * **tracktext = NULL, * **grouptext = NULL;

  if (!img->hierarchical)
    {
      img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
      img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;
    }
""",
    """  int dim = 0, k, j;
  char **grouparray = NULL, **basemotif = NULL, *albumtext = NULL, * **tracktext = NULL, * **grouptext = NULL;

  if (!img->hierarchical)
    {
""" + NEW_BODY + """    }
""",
    "分页修正 #2（generate_menu_pics）",
)

if n_ok == 3:
    PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("\n菜单分页修复完成，共 %d 项" % n_ok)
else:
    print("\n未全部应用，**未写入文件**（避免半成品）")
    sys.exit(1)