#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""让 `-b/--background` 的「每页一张背景图」真正生效（自动菜单路径）。

`-b` 接受逗号分隔的背景 jpg 列表（一页一张），解析后存进
`img->backgroundpic[]`。但选项解析收尾处的复制循环把**每一页**都从
`backgroundpic[0]` 复制：

    copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic0.jpg", ...);
    for (u = 1; u < img->nmenus; u++)
        copy_file2dir_rename(img->backgroundpic[0], tempdir,
                             "bgpic<u>.jpg", ...);          // <- 又是 [0]

于是 tempdir 下 bgpic0..N.jpg 全是同一张，`-b` 形同虚设，
菜单所有页只能共用一个背景。

改法：第 u 页取 `backgroundpic[u]`；列表不够长或该项为空时回退到 [0]
（与原行为一致，不影响只用单张背景的旧用法）。

改完即可用 `-b` 给每页配不同背景 —— 例如「一页一张专辑封面」。
"""
import pathlib
import sys

PATH = pathlib.Path(
    "/root/dvda-author-mlp8/src/command_line_parsing.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0
TOTAL = 6


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


rep(
    """      if (img->nmenus > 1)
        for (u = 1; u < img->nmenus; u++)
          {
            char name[13];
            sprintf(name, "%s%d%s", "bgpic", u, ".jpg");
            copy_file2dir_rename(img->backgroundpic[0],
                                 globals->settings.tempdir, name, globals);
          }
""",
    """      /* 每页各取自己那张背景图。原式固定用 [0]，使 --background
         的逗号列表失效（所有页共用第 1 张）。 */
      if (img->nmenus > 1)
        for (u = 1; u < img->nmenus; u++)
          {
            char name[13];
            sprintf(name, "%s%d%s", "bgpic", u, ".jpg");
            const char *srcpic =
              (u < globals->backgroundpicsize && img->backgroundpic[u])
              ? img->backgroundpic[u] : img->backgroundpic[0];
            copy_file2dir_rename(srcpic, globals->settings.tempdir,
                                 name, globals);
          }
""",
    "每页背景图取 backgroundpic[u]",
)

# 2) 声明「给了 --background 列表」的标记
rep(
    """  int errmsg;
  bool allocate_files = false;
  bool logrefresh = false;
  bool refresh_tempdir = true;
""",
    """  int errmsg;
  bool allocate_files = false;
  bool logrefresh = false;
  bool refresh_tempdir = true;
  bool cli_background_list = false; // --background 是否给了「每页一张」的列表
""",
    "声明 cli_background_list",
)

# 3) case 'b' 里置位
rep(
    """        case 'b':
          str = strdup(optarg);

          if (img->backgroundmpg)
""",
    """        case 'b':
          str = strdup(optarg);
          cli_background_list = true;

          if (img->backgroundmpg)
""",
    "case 'b' 置位",
)

# 4) blankscreen 不再覆盖已给出的 --background 列表
rep(
    """  if (img->nmenus && img->blankscreen && globals->topmenu < NO_MENU)
    {
      if (globals->veryverbose)
        foutput("%s\\n", INF "Converting overlay .png blankscreen"
                " to .jg blankscreen for mpg authoring...");
""",
    """  /* --blankscreen 是「文字浮层底图」，但原式会把它转成 jpg 覆盖
     backgroundpic[0]（先 unlink 再写），把 --background 给的每页背景图
     全变成同一张 blankscreen。给了列表就不该覆盖。 */
  if (img->nmenus && img->blankscreen && globals->topmenu < NO_MENU
      && !cli_background_list)
    {
      if (globals->veryverbose)
        foutput("%s\\n", INF "Converting overlay .png blankscreen"
                " to .jg blankscreen for mpg authoring...");
""",
    "blankscreen 不再覆盖 --background",
)

# 5) case 'b' 补齐文件名时的堆越界写
rep(
    """          int backgroundpic_arraylength = 0;

          if ((backgroundpic_arraylength = arraylength(img->backgroundpic))
              < img->nmenus)
            {
              foutput("%s\\n", WAR "You did not give enough filenames,"
                      " completing with last one");

              for (u = 0; u + backgroundpic_arraylength < img->nmenus; ++u)
                copy_file(img->backgroundpic[backgroundpic_arraylength - 1],
                          img->backgroundpic[u + backgroundpic_arraylength],
                          globals);
            }
""",
    """          int backgroundpic_arraylength = 0;

          if ((backgroundpic_arraylength = arraylength(img->backgroundpic))
              < img->nmenus)
            {
              foutput("%s\\n", WAR "You did not give enough filenames,"
                      " completing with last one");

              /* fn_strtok 只分配了 arraylength+1 个槽，而原式写
                 [arraylength .. nmenus-1]：既堆越界（且 dst 指向未分配内存，
                 copy_file 会再往里 strcpy）。先扩到 nmenus+1 再补齐。 */
              img->backgroundpic = realloc(img->backgroundpic,
                                           (img->nmenus + 1) * sizeof(char *));
              if (img->backgroundpic == NULL)
                EXIT_ON_RUNTIME_ERROR_VERBOSE("backgroundpic realloc");

              for (u = backgroundpic_arraylength; u < (int) img->nmenus; ++u)
                img->backgroundpic[u] =
                  strdup(img->backgroundpic[backgroundpic_arraylength - 1]);

              img->backgroundpic[img->nmenus] = NULL;
              globals->backgroundpicsize = img->nmenus + 1;
            }
""",
    "case 'b' 补齐列表不再堆越界",
)

# 6) backgroundpicsize 与实际槽数对齐（否则 free2 越界 free）
rep(
    """  globals->imagepicsize = img->nmenus + 1;
  globals->highlightpicsize = img->nmenus + 1;
  globals->backgroundmpgsize = img->nmenus + 1;
  globals->selectpicsize = img->nmenus + 1;
}
""",
    """  globals->imagepicsize = img->nmenus + 1;
  globals->highlightpicsize = img->nmenus + 1;
  globals->backgroundmpgsize = img->nmenus + 1;
  globals->selectpicsize = img->nmenus + 1;

  /* 上面把 backgroundpic 重新分配成 nmenus+1 槽（只填了前 nmenus 个），
     但长度仍停在 --background 解析出的旧值（= 文件名数 + 1）。
     页数变小后它会大于实际槽数 —— free_memory 里的
     free2(backgroundpic) 按它遍历就会越界 free，
     实测报 “free(): invalid pointer” 而中止。必须与实际槽数对齐。 */
  globals->backgroundpicsize = img->nmenus + 1;
}
""",
    "backgroundpicsize 与实际槽数对齐",
)

if n_ok == TOTAL:
    PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("\n每页背景图修复完成，共 %d 项" % n_ok)
else:
    print("\n只应用了 %d/%d 项，**未写入文件**" % (n_ok, TOTAL))
    sys.exit(1)
