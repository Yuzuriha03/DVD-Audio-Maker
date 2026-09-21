#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把选曲菜单改成「一页一个专辑」：大标题 = 光盘标题，小标题 = 专辑名。

## 原来的行为

菜单文字链 `--screentext` 的每一段对应一个**音频组**，而分页是
dvda-author 自己按「逐页填满 R 行」算的（R = ceil(总轨数/页数)）。
于是一页里会混进多张专辑的曲子，只能给每首曲子加「专辑名 | 」前缀来区分。

## 目标

    大标题（每页都有） = 光盘标题
    小标题（该页顶部） = 专辑名
    一页只放一个专辑；专辑换了就自动换页
    每页背景 = 该专辑的封面（素材侧改，见 menu_assets.py）

## 为什么能这么做

`menu.c` 在 `ncolumns == 1` 时的画页循环是

    do {
      mogrify_img(grouptext[groupcount][0], ...);            // 小标题
      offset = track;
      do { mogrify_img(tracktext[groupcount][track], ...); track++; }
      while ((buttons < menubuttons) && (track < ntracks[groupcount]));
      if (track == ntracks[groupcount]) { group++; groupcount++; track = 0; }
      else break;                                            // 本段没画完 → 换页
    } while ((group < img->ncolumns) && (groupcount < ngroups));

也就是**一次只画一个文字段，段内曲子画完才换页**。所以只要让
`--screentext` 每段 = 一个专辑，页数就等于专辑数，新专辑自动换页。
`albumtext`（第一个 `=` 之前）由 `prepare_overlay_img()` 画在**每一页**顶部，
正好当大标题。

## 需要改的三件事

1. **页与音频组解耦**。按钮是 `jump group G track K`，G/K 是
   「音频组 + 组内曲目号」，与「页」无关。一页一个专辑后页号 != 组号，
   必须把每页换算回 (组, 组内首轨)。新增 `compute_menu_pages()` 做这件事：
   解析 `--screentext` 得到每页曲目数，再按音频组的轨数把它切成
   「页 → (组, 组内首轨)」。**任何一步对不上就整体放弃**（返回 0），
   调用方退回旧行为 —— 宁可保持可用的旧排版，也不要画出错位的按钮。

2. **每页行数要按该页实际曲目数取**。原式 `maxbuttons` 是
   `ceil(总轨数/页数)` 一个全局值，各专辑曲目数不同就必然有页画不下或
   留空。行数同时决定文字行距（`mogrify_img` 的 `maxnumtracks`）与
   按钮矩形（`compute_coordinates` 用 `img->maxbuttons`），所以两边都用
   同一个按页取值即可保持一致。

3. **`ntracks[]` 的语义改为「每页曲目数」**。`menu.c`/`xml.c` 里的
   `ntracks[k]`（k 是文字段号）本来就该是「第 k 段的曲目数」，
   只是原先段 == 音频组才恰好一致。现在由 `amg2.c` 传入按页的数组。

## 为什么必须校验而不是「大致对上」

按钮错位**不会报错**：只是点一首放成另一首。所以所有前提
（页数 == `--nmenus`、页曲目数之和 == 音频组轨数之和、每页不跨组、
每页不超过屏幕行数）都显式检查，任一不满足就整体回退。

## 回退

停用本脚本（并从 build 脚本的补丁列表里去掉）即可回到旧排版；
`[1b]` 会把 menu.c / xml.c / amg2.c / structures.h 从原始源码还原。
"""
import pathlib
import re
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "compute_menu_pages"

# ---------------------------------------------------------------- 1. structures.h
S_OLD = """    uint16_t count;
    uint16_t* npics;
    uint16_t* topmenu_nslides;
    uint32_t* stillpicvobsize;
    uint32_t* menuvobsize;
    stilloptions** options;

} pic;"""

S_NEW = """    uint16_t count;
    uint16_t* npics;
    uint16_t* topmenu_nslides;
    uint32_t* stillpicvobsize;
    uint32_t* menuvobsize;
    stilloptions** options;

    /* 按页的菜单布局（「一页一个专辑」，见 menu.c 的 compute_menu_pages）。
       page_ntracks[p] = 第 p 页的曲目数；
       page_group[p]   = 第 p 页所属音频组号（1-based，给 jump group 用）；
       page_t0[p]      = 第 p 页首轨在该音频组内的序号（0-based，给 jump track 用）。
       任一为 NULL 时退回「按音频组分页」的旧行为。

       ⚠️ 必须**追加在结构末尾**：dvda-author.c 里 pic 是按位置初始化的
       （`pic img0 = {1, 0, 0, ...}`），插在中间会让后面所有字段错位。 */
    uint8_t* page_ntracks;
    uint8_t* page_group;
    uint16_t* page_t0;
    uint8_t  npages;

} pic;"""

# ---------------------------------------------------------------- 2. menu.h
H_OLD = """int  generate_menu_pics(command_t* , pic* img, uint8_t ngroups, uint8_t *numtracks, globalData*);"""

H_NEW = """int  generate_menu_pics(command_t* , pic* img, uint8_t ngroups, uint8_t *numtracks, globalData*);
int  compute_menu_pages(pic* img, uint8_t ngroups, uint8_t *ntracks, globalData* globals);"""

# ---------------------------------------------------------------- 3. menu.c
function_text = r'''
/* ---------------------------------------------------------------------------
   把「页」映射到「音频组 + 组内首轨」。

   --screentext 的每段对应一页，段内曲目数就是该页的曲目数；音频组的轨数
   由调用方给出。两者按顺序切分：每页必须完整落在某一个音频组内
   （素材侧保证专辑不跨组，而一页 = 一个专辑）。全部对得上才启用按页布局。

   返回 1 = 已填好 img->page_*（按页布局可用）；0 = 不可用，调用方退回旧行为。
--------------------------------------------------------------------------- */
int compute_menu_pages(pic *img, uint8_t ngroups, uint8_t *ntracks,
                       globalData *globals)
{
  if (img->page_ntracks) return 1;          /* 已经算过 */
  if (!img->screentextchain || !img->screentextchain[0]) return 0;
  if ((img->nmenus == 0) || (ngroups == 0)) return 0;

  /* ⚠️ 不能改 img->screentextchain：generate_menu_pics() 会把第一个 '='
     改成 '\0'，那是**就地修改**。这里用副本解析，免得依赖调用顺序。 */
  char *s = strdup(img->screentextchain);
  if (s == NULL) return 0;

  char *eq = strchr(s, '=');
  if (eq == NULL)
    {
      FREE(s);
      return 0;
    }

  uint8_t pc[256];                          /* 每页曲目数（nmenus 是 uint8_t） */
  int np = 0, total = 0;
  char *p = eq + 1;

  while (*p && (np < 256))
    {
      char *colon = strchr(p, ':');
      char *segend = colon ? colon : (p + strlen(p));

      /* 段格式：小标题'='曲名1,曲名2,...；有 '=' 且后面非空才算有曲目 */
      char *g = (char *) memchr(p, '=', (size_t) (segend - p));
      int n = 0;
      if (g && (g + 1 < segend))
        {
          n = 1;
          for (char *q = g + 1; q < segend; ++q)
            if (*q == ',') ++n;
        }
      if ((n < 1) || (n > (int) (MAX_BUTTON_Y_NUMBER - 2)))
        {
          FREE(s);
          return 0;
        }
      pc[np++] = (uint8_t) n;
      total += n;

      if (!colon) break;
      p = colon + 1;
    }

  int total_audio = 0;
  for (int g = 0; g < ngroups; ++g) total_audio += ntracks[g];

  if ((np == 0) || (np != (int) img->nmenus) || (total != total_audio))
    {
      if (globals->debugging)
        foutput(WAR "Menu page structure mismatch: %d page(s)/%d track(s) "
                "in --screentext against %d menu(s)/%d track(s) in %d audio "
                "group(s); keeping group-based layout\n",
                np, total, (int) img->nmenus, total_audio, (int) ngroups);
      FREE(s);
      return 0;
    }

  uint8_t  *pnt = (uint8_t *) calloc((size_t) np, sizeof(uint8_t));
  uint8_t  *pgrp = (uint8_t *) calloc((size_t) np, sizeof(uint8_t));
  uint16_t *pt0 = (uint16_t *) calloc((size_t) np, sizeof(uint16_t));
  if (!pnt || !pgrp || !pt0)
    {
      if (pnt) FREE(pnt);
      if (pgrp) FREE(pgrp);
      if (pt0) FREE(pt0);
      FREE(s);
      return 0;
    }

  int g = 0, acc = 0, ok = 1;
  for (int q = 0; q < np; ++q)
    {
      /* 一页跨了两个音频组 → 无法用「单个 jump group」表示，放弃 */
      if ((g >= ngroups) || (acc + pc[q] > ntracks[g]))
        {
          ok = 0;
          break;
        }
      pnt[q] = pc[q];
      pgrp[q] = (uint8_t) (g + 1);          /* 1-based */
      pt0[q] = (uint16_t) acc;
      acc += pc[q];
      if (acc == ntracks[g]) { ++g; acc = 0; }
    }
  if (ok && (g != ngroups)) ok = 0;

  if (!ok)
    {
      if (globals->debugging)
        foutput("%s", WAR "Menu pages do not line up with audio groups "
                "(a page spans two groups); keeping group-based layout\n");
      FREE(pnt);
      FREE(pgrp);
      FREE(pt0);
      FREE(s);
      return 0;
    }

  img->page_ntracks = pnt;
  img->page_group = pgrp;
  img->page_t0 = pt0;
  img->npages = (uint8_t) np;

  /* 一页一列：x() 与 compute_coordinates() 都以 ncolumns 均分宽度，
     一页一个专辑就必须占满整宽。 */
  img->ncolumns = 1;

  if (globals->debugging)
    {
      foutput(MSG_TAG "One album per menu: %d page(s)\n", np);
      for (int q = 0; q < np; ++q)
        foutput(MSG_TAG "  page %d: %d track(s), audio group %d, track %d\n",
                q + 1, pnt[q], pgrp[q], pt0[q] + 1);
    }

  FREE(s);
  return 1;
}

'''

M_OLD = """      else
        {
          do
            {

              mogrify_img(grouptext[groupcount][0], group, -1, img, img->maxbuttons, command1, command2, 0, img->groupcolor);
              offset = track;

              do
                {

                  buttons++;
                  mogrify_img(tracktext[groupcount][track], group, track, img, img->maxbuttons, command1, command2, offset, img->textcolor_pic);
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));


              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                  offset = 0;
                }
              else
                break;  // changing menus without completing the liste of tracks in the same group
            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

M_NEW = """      else if (img->page_ntracks)
        {
          /* 一页一个专辑（见 compute_menu_pages）：
             文字段号 == 页号，行数取本页实际曲目数。
             行距（mogrify_img 的 maxnumtracks）与按钮矩形（xml.c 里
             compute_coordinates 用的 img->maxbuttons）是同一个值，所以
             两边必然对齐；x 一律取第 0 列，让文字占满整宽。 */
          if ((unsigned) menu < img->npages)
            {
              img->maxbuttons = img->page_ntracks[menu];

              mogrify_img(grouptext[menu][0], 0, -1, img, img->maxbuttons,
                          command1, command2, 0, img->groupcolor);

              for (track = 0; track < img->maxbuttons; ++track)
                mogrify_img(tracktext[menu][track], 0, track, img,
                            img->maxbuttons, command1, command2, 0,
                            img->textcolor_pic);

              buttons = img->maxbuttons;
            }
        }
      else
        {
          do
            {

              mogrify_img(grouptext[groupcount][0], group, -1, img, img->maxbuttons, command1, command2, 0, img->groupcolor);
              offset = track;

              do
                {

                  buttons++;
                  mogrify_img(tracktext[groupcount][track], group, track, img, img->maxbuttons, command1, command2, offset, img->textcolor_pic);
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));


              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                  offset = 0;
                }
              else
                break;  // changing menus without completing the liste of tracks in the same group
            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

# ---------------------------------------------------------------- 4. xml.c spumux
X1_OLD = """      else
        {

          do
            {
              offset = track;
              do
                {
                  buttons++;
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\\n", "       <button x0=\\"", x0[group], "\\"", " y0=\\"", y0[track - offset], "\\"", " x1=\\"", x1[group], "\\"", " name=\\"button", buttons, "\\"", " y1=\\"", y1[track - offset], "\\"/>");
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));

              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                  offset = 0;
                }
              else
                {
                  break;  // changing menus without completing the liste of tracks in the same group
                }
            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

X1_NEW = """      else if (img->page_ntracks)
        {
          /* 一页一个专辑：按钮矩形必须与 menu.c 的文字**同一套行距**。
             compute_coordinates() 是按 img->maxbuttons 分行，所以要先把它
             设成本页曲目数，再算坐标。 */
          if ((unsigned) menu < img->npages)
            {
              img->maxbuttons = img->page_ntracks[menu];
              compute_coordinates(command, img->ncolumns, x0, y0, x1, y1);

              for (track = 0; track < img->maxbuttons; ++track)
                {
                  buttons++;
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\\n", "       <button x0=\\"", x0[0], "\\"", " y0=\\"", y0[track], "\\"", " x1=\\"", x1[0], "\\"", " name=\\"button", buttons, "\\"", " y1=\\"", y1[track], "\\"/>");
                }

              buttons = img->maxbuttons;
            }
        }
      else
        {

          do
            {
              offset = track;
              do
                {
                  buttons++;
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\\n", "       <button x0=\\"", x0[group], "\\"", " y0=\\"", y0[track - offset], "\\"", " x1=\\"", x1[group], "\\"", " name=\\"button", buttons, "\\"", " y1=\\"", y1[track - offset], "\\"/>");
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));

              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                  offset = 0;
                }
              else
                {
                  break;  // changing menus without completing the liste of tracks in the same group
                }
            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

# ---------------------------------------------------------------- 5. xml.c amgm
X2_OLD = """      else
        {
          do
            {
              do
                {
                  buttons++;
                  track++;
                  fprintf(xmlfile, "       %s%02d%s%d%s%d%s\\n", "<button name=\\"button", buttons, "\\">jump group ", groupcount + 1, " track ", track, ";</button>");
                  foutput(INF "Button: %d Group %d Track %d \\n", buttons, group + 1, track);
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));



              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                }
              else
                {
                  break;  // changing menus without completing the liste of tracks in the same group
                }

            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

X2_NEW = """      else if (img->page_ntracks)
        {
          /* 一页一个专辑：按钮是 `jump group G track K`，G/K 是「音频组 +
             组内曲目号」，与「页」无关，必须换算（compute_menu_pages）。 */
          if ((unsigned) menu < img->npages)
            {
              uint8_t  gp = img->page_group[menu];      /* 1-based */
              uint16_t t0 = img->page_t0[menu];         /* 0-based */

              for (track = 0; track < img->page_ntracks[menu]; ++track)
                {
                  buttons++;
                  fprintf(xmlfile, "       %s%02d%s%d%s%d%s\\n", "<button name=\\"button", buttons, "\\">jump group ", gp, " track ", t0 + track + 1, ";</button>");
                  if (globals->debugging)
                    foutput(INF "Button: %d Group %d Track %d \\n", buttons, gp, t0 + track + 1);
                }

              buttons = img->page_ntracks[menu];
            }
        }
      else
        {
          do
            {
              do
                {
                  buttons++;
                  track++;
                  fprintf(xmlfile, "       %s%02d%s%d%s%d%s\\n", "<button name=\\"button", buttons, "\\">jump group ", groupcount + 1, " track ", track, ";</button>");
                  foutput(INF "Button: %d Group %d Track %d \\n", buttons, group + 1, track);
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));



              if (track == ntracks[groupcount])
                {
                  group++;
                  groupcount++;
                  track = 0;
                }
              else
                {
                  break;  // changing menus without completing the liste of tracks in the same group
                }

            }
          while ((group < img->ncolumns) && (groupcount < ngroups));
        }"""

# ---------------------------------------------------------------- 6. amg2.c
A_OLD = """  img->action = ANIMATEDVIDEO;

  switch (globals->topmenu)"""

A_NEW = """  img->action = ANIMATEDVIDEO;

  /* 「一页一个专辑」：先算出「每页曲目数」与「页 -> (音频组, 组内首轨)」。
     成功时菜单每页一个专辑（文字段 == 页），失败则整条退回旧的按组排版。 */
  int by_page = compute_menu_pages(img, ngroups, ntracks, globals);
  uint8_t  m_ngroups = by_page ? img->npages : ngroups;
  uint8_t *m_ntracks = by_page ? img->page_ntracks : ntracks;

  switch (globals->topmenu)"""

A_OLD2 = """      generate_menu_pics(command, img, ngroups, ntracks, globals);"""
A_NEW2 = """      generate_menu_pics(command, img, m_ngroups, m_ntracks, globals);"""

A_OLD3 = """      errno = generate_spumux_xml(command, ngroups, ntracks, img, globals);"""
A_NEW3 = """      errno = generate_spumux_xml(command, m_ngroups, m_ntracks, img, globals);"""

A_OLD4 = """          errno = generate_amgm_xml(ngroups, ntracks, img, globals);"""
A_NEW4 = """          errno = generate_amgm_xml(m_ngroups, m_ntracks, img, globals);"""


# ------------------------------------------------- 7. main 栈数组容量（必须）
# main 里的 tab1/tab2 是**局部栈数组**，而 menu.c 会直接往它们里面写
# `globals->grouptextsize[k] = ...`。原先「文字段 == 音频组」最多 9 个；
# 一页一个专辑后文字段 == 页数（实测 17 / 28），写 tab1[16] 就会冲掉
# main 的栈 canary —— 症状是运行结束时突然
# `*** stack smashing detected ***: terminated`（实测，见 TROUBLESHOOTING）。
CV_OLD = """#define DEFAULT_POINTSIZE  25"""

CV_NEW = """/* 菜单文字段（= 菜单页）数量的上限。
   ⚠️ dvda-author.c 里 main 用**局部栈数组**保存每段的 grouptextsize /
   tracktextsize，而 menu.c 会直接往里面写（`globals->grouptextsize[k] = ...`）。
   段数超过数组容量就是越界写栈 —— 症状是运行结束时突然报
   "*** stack smashing detected ***"。取 256 是因为 --nmenus 是 uint8_t。 */
#define MENU_TEXT_GROUP_MAX 256

#define DEFAULT_POINTSIZE  25"""

D_OLD = """  uint32_t tab1[9] = {0};
  tab1[0] = 1;
  uint32_t tab2[9] = {0};
  tab2[0] = 1;"""

D_NEW = """  /* grouptextsize / tracktextsize：menu.c 按**文字段号**直接写这两个数组
     （一段 = 一个菜单页），容量必须容纳最大页数，否则越界写栈。
     见 commonvars.h 的 MENU_TEXT_GROUP_MAX。 */
  uint32_t tab1[MENU_TEXT_GROUP_MAX] = {0};
  tab1[0] = 1;
  uint32_t tab2[MENU_TEXT_GROUP_MAX] = {0};
  tab2[0] = 1;"""

MC_DIM_OLD = """      dim = (ndef > (int) ngroups) ? ndef : (int) ngroups;"""

MC_DIM_NEW = """      dim = (ndef > (int) ngroups) ? ndef : (int) ngroups;

      /* globals->grouptextsize / tracktextsize 指向 dvda-author.c 里 main 的
         栈数组，容量 = MENU_TEXT_GROUP_MAX。段数超了必须报错，
         绝不能越界写栈（那会表现为 "stack smashing detected"）。 */
      if (dim > MENU_TEXT_GROUP_MAX)
        EXIT_ON_RUNTIME_ERROR_VERBOSE("Too many menu text groups")
"""


def apply(path, pairs):
    """逐条替换；要求每条 OLD 恰好出现一次（多了说明上游有重复副本，必须查清）。"""
    f = SRC / path
    if not f.exists():
        print("[FAIL] 找不到 %s" % f)
        return False
    text = f.read_text(encoding="utf-8", errors="surrogateescape")
    for old, new, what in pairs:
        n = text.count(old)
        if n != 1:
            print("[FAIL] %s: %s 匹配 %d 次（应为 1）" % (path, what, n))
            return False
        text = text.replace(old, new, 1)
        print("[OK]   %s: %s" % (path, what))
    f.write_text(text, encoding="utf-8", errors="surrogateescape")
    return True


def main():
    # 幂等：structures.h 里出现标记就认为已打过
    sh = SRC / "include/structures.h"
    if sh.exists() and MARK in sh.read_text(encoding="utf-8",
                                            errors="surrogateescape"):
        print("[SKIP] 已应用过（structures.h 含 %s）" % MARK)
        return 0

    if not apply("include/structures.h",
                 [(S_OLD, S_NEW, "pic 结构追加按页字段")]):
        return 1
    if not apply("include/menu.h",
                 [(H_OLD, H_NEW, "声明 compute_menu_pages")]):
        return 1
    if not apply("include/commonvars.h",
                 [(CV_OLD, CV_NEW, "定义 MENU_TEXT_GROUP_MAX")]):
        return 1
    if not apply("dvda-author.c",
                 [(D_OLD, D_NEW, "main 的 grouptextsize/tracktextsize 扩到 "
                                 "MENU_TEXT_GROUP_MAX（否则越界写栈）")]):
        return 1

    # menu.c：先插入函数定义（放在 generate_menu_pics 之前），再改循环
    mc = SRC / "menu.c"
    text = mc.read_text(encoding="utf-8", errors="surrogateescape")
    anchor = "int generate_menu_pics(command_t *command, pic *img, uint8_t ngroups, uint8_t *ntracks,  globalData *globals)"
    if text.count(anchor) != 1:
        print("[FAIL] menu.c: 找不到 generate_menu_pics 定义")
        return 1
    text = text.replace(anchor, function_text.lstrip("\n") + anchor, 1)
    for old, new, what in ((M_OLD, M_NEW, "按页排版"),
                           (MC_DIM_OLD, MC_DIM_NEW, "文字段数上限守卫")):
        if text.count(old) != 1:
            print("[FAIL] menu.c: %s 匹配 %d 次（应为 1）" % (what, text.count(old)))
            return 1
        text = text.replace(old, new, 1)
    mc.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("[OK]   menu.c: 插入 compute_menu_pages、按页排版、段数上限守卫")

    if not apply("xml.c",
                 [(X1_OLD, X1_NEW, "按钮矩形按页分行"),
                  (X2_OLD, X2_NEW, "按钮跳转换算回音频组/组内曲目号")]):
        return 1
    if not apply("amg2.c",
                 [(A_OLD, A_NEW, "调用 compute_menu_pages 并传按页数组"),
                  (A_OLD2, A_NEW2, "generate_menu_pics 传按页数组"),
                  (A_OLD3, A_NEW3, "generate_spumux_xml 传按页数组"),
                  (A_OLD4, A_NEW4, "generate_amgm_xml 传按页数组")]):
        return 1

    print("\n「一页一个专辑」补丁完成")
    return 0


if __name__ == "__main__":
    sys.exit(main())
