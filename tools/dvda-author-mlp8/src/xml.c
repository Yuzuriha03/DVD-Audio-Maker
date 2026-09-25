#ifdef HAVE_CONFIG_H
#include "config.h"
#endif
#if !defined HAVE_core_BUILD || !HAVE_core_BUILD
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include "structures.h"
#include "c_utils.h"
#include "commonvars.h"
#include "menu.h"
#include "launch_manager.h"
#include "winport.h"
#include "auxiliary.h"


extern uint16_t norm_x, norm_y, totntracks;


int  generate_amgm_xml(uint8_t ngroups, uint8_t *ntracks, pic *img, globalData *globals)
{

  errno = 0;
  uint8_t arrowbuttons = 1, buttons = 0, menu = 0, track = 0, groupcount = 0, group = 0;
  uint8_t menubuttons;

  // Writing XML code
  FILE *xmlfile;

  if (globals->xml == NULL)
    {
      char xmlfilepath[strlen(globals->settings.tempdir) + 8 + STRLEN_SEPARATOR];
      memset(xmlfilepath, 0, sizeof(xmlfilepath));
      sprintf(xmlfilepath, "%s"SEPARATOR"%s", globals->settings.tempdir, "xmltemp");
      globals->xml = strdup(xmlfilepath);
    }

  xmlfile = fopen(globals->xml, "wb");
  if (xmlfile == NULL)
    {
      EXIT_ON_RUNTIME_ERROR_VERBOSE("Could not open xmlfile")
    }

  fprintf(xmlfile, "%s%s%s%s%s\n",
          "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<dvdauthor jumppad=\"1\">\n\
 <amgm>\n\
   <menus>\n\
   <video format=\"", img->norm, "\" />\n\
   <audio format=\"", img->audioformat, "\" lang=\"en\" />");
  fprintf(xmlfile, "%s\n", "   <pgc>");

  if (globals->debugging) foutput("%s\n", img->hierarchical ? INF "Hierarchical menus" : "Non-hierarchical menus");

  do
    {
      if (img->hierarchical)
        {
          img->maxbuttons = (menu == 0) ? ngroups : Min(MAX_BUTTON_Y_NUMBER - 2, ntracks[groupcount]);
          img->resbuttons = 0;
        }

      arrowbuttons = (menu < img->nmenus - 1) + (menu > 0);
      menubuttons = (menu < img->nmenus - 1) ? img->maxbuttons : img->maxbuttons + img->resbuttons;

      foutput(INF "Menu: %d\n", menu);

      if (img->hierarchical)
        {
          if (menu == 0)
            {
              do
                {
                  groupcount++;
                  foutput(INF "Menu: %d\n", menu);
                  buttons++;
                  fprintf(xmlfile, "       %s%02d%s%d%s\n", "<button name=\"button", groupcount, "\">jump menu ", groupcount + 1, ";</button>");
                  foutput(INF "Menu: %d Button %d\n", menu, buttons);
                }
              while (groupcount < ngroups);
              groupcount = 0;
            }

          else if (groupcount < ngroups)
            {

              do
                {
                  buttons++;
                  track++;
                  fprintf(xmlfile, "       %s%02d%s%d%s%d%s\n", "<button name=\"button", buttons, "\">jump group ", groupcount + 1, " track ", track, ";</button>");
                  foutput(INF "Button: %d  Group: %d Track %d\n", buttons, group + 1, track);
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));

              if (track == ntracks[groupcount])
                {
                  groupcount++;
                  track = 0;
                }
            }

        }
      else if (img->page_ntracks)
        {
          /* 一页一个专辑：按钮是 `jump group G track K`，G/K 是「音频组 +
             组内曲目号」，与「页」无关，必须换算（compute_menu_pages）。 */
          if ((unsigned) menu < img->npages)
            {
              uint8_t  gp = img->page_group[menu];      /* 1-based */
              uint16_t t0 = img->page_t0[menu];         /* 0-based */

              if (menu < (unsigned) img->index_pages)
                {
                  /* ---- 一级菜单（专辑索引页）----
                     第 menu 页第 k 格 → 专辑号 a = menu*INDEX_PER_PAGE + k
                     → 该专辑的内容页（0-based）= index_pages + a
                     → 菜单号（1-based）= index_pages + menu*INDEX_PER_PAGE + k + 1。

                     这是**纯位置映射**，与 menu_assets.py 的
                     `--index-pages` 计算一致；越界（末页不足
                     INDEX_PER_PAGE 格、或专辑数不够）时跳过，不出按钮。 */
                  int ncells = (int) img->page_ntracks[menu];
                  uint16_t nmenus = img->nmenus;

                  for (track = 0; track < ncells; ++track)
                    {
                      uint16_t tgt = (uint16_t) (img->index_pages
                                                 + menu * INDEX_PER_PAGE
                                                 + track + 1);
                      if (tgt > nmenus) break;   /* 末页补位，无对应专辑 */

                      buttons++;
                      fprintf(xmlfile, "       %s%02d%s%d%s\n",
                              "<button name=\"button", buttons,
                              "\">jump menu ", tgt, ";</button>");
                      if (globals->debugging)
                        foutput(INF "Index button: %d -> menu %d\n",
                                buttons, tgt);
                    }

                  buttons = ncells;
                }
              else
                {
                  for (track = 0; track < img->page_ntracks[menu]; ++track)
                    {
                      buttons++;
                      fprintf(xmlfile, "       %s%02d%s%d%s%d%s\n", "<button name=\"button", buttons, "\">jump group ", gp, " track ", t0 + track + 1, ";</button>");
                      if (globals->debugging)
                        foutput(INF "Button: %d Group %d Track %d \n", buttons, gp, t0 + track + 1);
                    }

                  buttons = img->page_ntracks[menu];
                }
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
                  fprintf(xmlfile, "       %s%02d%s%d%s%d%s\n", "<button name=\"button", buttons, "\">jump group ", groupcount + 1, " track ", track, ";</button>");
                  foutput(INF "Button: %d Group %d Track %d \n", buttons, group + 1, track);
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
        }


      if ((img->nmenus > 1) && (menu < img->nmenus))
        {
          /* 与按钮位置（generate_spumux_xml）**逐一对应**：
             槽 1 = Next（末页改 Previous，即不再有 Next），槽 2 = Previous
             （仅中间页），槽 3 = Menu（仅二级页，返回专辑索引 1）。
             编号顺序也必须一致（Next → Previous → Menu）。
             ⚠️ 原 do-while 在曲目不满一页的末页会因 buttons 基数偏低而多转
             几轮，重复输出同一个 Previous —— 实测末页出现 6 个
             （button08..button13，全是 jump menu 7），而 spumux 只定义了
             1 个（button08）→ 两边按钮数 13 vs 8 不一致，dvdauthor 多建的
             按钮没有对应高亮区域，该页的 Previous 因此选不中 /
             回不到上一页。 */
          int jumps_emitted = 0;
          /* 槽 3：「返回专辑索引」。只在二级（选曲）页出现 ——
             没有索引页（--index-pages == 0）就没处可回；
             索引页自身也不画（那一页**就是**索引）。 */
          int has_menu_button = (img->index_pages > 0)
            && !(img->page_ntracks
                 && (menu < (unsigned) img->index_pages));
          /* 与 menu.c 同一套（宏在 menu.h）：索引页的 Next **只在索引页
             之间翻**，所以最后一个索引页没有 Next。两边的按钮编号顺序
             必须逐一对上，否则 dvdauthor 建出来的按钮没有高亮区域。 */
          int has_next = DVDA_HAS_NEXT(img, menu);
          int has_prev = DVDA_HAS_PREV(img, menu);
          /* 槽 1：有 Next 放 Next；只剩 Previous 时把它提上来占槽 1。 */
          int slot1 = has_next || has_prev;

          if (slot1)
            {
              buttons++;
              jumps_emitted++;
              fprintf(xmlfile, "       %s%02d%s%d%s\n", "<button name=\"button", buttons, "\">jump menu ", has_next ? menu + 2 : menu, ";</button>");
            }

          if (has_next && has_prev)
            {
              buttons++;
              jumps_emitted++;
              fprintf(xmlfile, "       %s%02d%s%d%s\n", "<button name=\"button", buttons, "\">jump menu ", menu, ";</button>");
            }

          /* 槽 3：「返回专辑索引」。第一个菜单页（menu 1）永远是索引页 1 ——
             当 `--index-pages` 为 0 时本按钮不出现，所以不会指向曲目页。 */
          if (has_menu_button)
            {
              buttons++;
              jumps_emitted++;
              fprintf(xmlfile, "       %s%02d%s%d%s\n", "<button name=\"button", buttons, "\">jump menu ", 1, ";</button>");
            }

          if (globals->debugging && (jumps_emitted != (int) arrowbuttons))
            foutput(WAR "Arrow jump count mismatch: %d emitted, %d expected\n",
                    jumps_emitted, (int) arrowbuttons);
          fprintf(xmlfile, "%s%s%s\n", "       <vob pause=\"inf\" file=\"", img->topmenu[menu], "\"/>\n\
   </pgc>\n");


          if (menu < img->nmenus - 1) fprintf(xmlfile, "%s\n", "   <pgc>");
        }
      else
        fprintf(xmlfile, "%s%s%s\n", "       <vob pause=\"inf\" file=\"", img->topmenu[menu], "\"/>\n\
   </pgc>\n");

      menu++;
      buttons = 0;
      group = 0;
    }
  while ((menu < img->nmenus) && (groupcount < ngroups));


  fprintf(xmlfile, "\
   </menus>\n\
 </amgm>\n\
</dvdauthor>\n");

  fclose(xmlfile);

  if (errno) foutput("%s\n", ERR "Could not generate Xml project file properly for generating DVD-Audio menu");
  else if (globals->debugging) foutput("%s\n", MSG_TAG "Xml dvdauthor project file was generated.");

  return (errno);
}






static inline void compute_coordinates(command_t *command, uint8_t ncol, uint16_t *x0, uint16_t *y0, uint16_t *x1, uint16_t *y1)
{

  int i, j;
  uint16_t delta = 0;

  delta = EVEN((norm_y - 60) / ((command->img->maxbuttons + 4) * 2));
  x1[0] = EVEN(x(1, ncol)) - 12;
  x0[0] = EMPIRICAL_X_SHIFT + 20 - 12;
  y0[0] = EVEN(y(1, command->img->maxbuttons + 4) - delta);
  y1[0] = EVEN(y(2, command->img->maxbuttons + 4) - delta);

  for (i = 1; i < ncol; ++i)
    {
      x1[i] = EVEN(x(i + 1, ncol)) - 12 ;
      x0[i] = x1[i - 1] ;
    }

  /* 填到 `maxbuttons + 2`：底部第三个槽位是「返回专辑索引」按钮
     （见 MENU_BUTTON_ROW）。少填一行就会取到未初始化的 y ——
     历史上正是这种未初始化坐标让 spumux 报 "Button coordinates out
     of range" 并输出 0 字节。 */
  for (j = 1; j < command->img->maxbuttons + 3; ++j)
    {
      y1[j] = EVEN(y(j + 2, command->img->maxbuttons + 4) - delta);
      y0[j] = y1[j - 1];
    }

}


int  generate_spumux_xml(command_t *command, uint8_t ngroups, uint8_t *ntracks, pic *img, globalData *globals)
{

  uint8_t buttons = 0, arrowbuttons, menubuttons, menu = 0, track = 0, group = 0, groupcount = 0, offset = 0;
  uint16_t x0[ngroups], y0[MAX_BUTTON_NUMBER], x1[ngroups], y1[MAX_BUTTON_NUMBER];
  errno = 0;
  FILE *spu_xmlfile = NULL;
  if (globals->debugging) foutput(MSG_TAG "Max ntracks: %d\n", command->img->maxbuttons);

  if (globals->spu_xml == NULL) globals->spu_xml = calloc(img->nmenus, sizeof(char *));
  if (globals->spu_xml == NULL) perror(ERR "spuxml\n");
  if (globals->debugging) foutput("%s\n", INF "Generating Xml project for spumux...");


  do
    {
      // Writing XML code

      if (globals->spu_xml[menu] == NULL)
        {
          char spu_xmlfilepath[strlen(globals->settings.tempdir) + 20];
          memset(spu_xmlfilepath, 0, sizeof(spu_xmlfilepath));
          sprintf(spu_xmlfilepath, "%s"SEPARATOR"%s%d%s", globals->settings.tempdir, "spu_xmltemp_", menu, ".xml");
          globals->spu_xml[menu] = strdup(spu_xmlfilepath);
        }

      if (!globals->nooutput) spu_xmlfile = fopen(globals->spu_xml[menu], "wb");

      /*  We take a basic picture of 720x576 and divide it into a maximum of 3 columns (max 3 groups) and 20 tracks per group
       *  Left/Right Border= 20, top border=56 pixels, bottom border=16 pix. Inter-column spacing=20 pixels, inter-line spacing=12 pixels
       *  Let G be the number of groups and T the maximum of the number of titles in all groups,
       *  button(g, t(g)) the button for track t(g) in group g,
       *  the width of each button will be (720-2*20-(G-1)*20)/G=700/G-20 i.e. 680, 330 or 213 for PAL
       *  more generally: (norm_x-2*20-(G-1)*20)/G
       *  the height is: (576-72-(T-1)*12)/T=516/T-12 i.e a minimum of 7 pixels
       *  more generally: (norm_y-72-(T-1)*12)/T
       *  the coordinates of this button will be (x0,y0,x1,y1)=(left x,top y,right x,bottom y) :
       *   button(g, t(g))=floor(20+ 700*(g-1)/G, 56 + 516*(t(g)-1)/T, 700*g/G, 56 + 516*t(g)/T) */

      if (globals->debugging)     foutput(INF "Creating spumux xml file %s for menu %d\n", globals->spu_xml[menu], menu);

      if (spu_xmlfile == NULL)   foutput(ERR "spumux xml file %s for menu %d could not be opened\n", globals->spu_xml[menu], menu);

      fprintf(spu_xmlfile, "%s%s%s%s%s%s%s%s%s%s%s%s",
              "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<subpictures>\n\
  <stream>\n\
    <spu ", (img->highlightpic[menu]) ? " highlight=\"" : "", (img->highlightpic[menu]) ? img->highlightpic[menu] : "", (img->highlightpic[menu]) ? "\"" : "", " force=\"yes\"", " start=\"00:00:00.00\"", (img->selectpic[menu]) ? " select=\"" : "", (img->selectpic[menu]) ? img->selectpic[menu] : "", (img->selectpic[menu]) ? "\"" : "", (img->imagepic[menu]) ? " image=\"" : "", (img->imagepic[menu]) ? img->imagepic[menu] : "", (img->imagepic[menu]) ? "\"" : "");

      fprintf(spu_xmlfile, "%s\n", ">");

      // We add group labels as non-buttons, so j->j+1 and maxnumttracsk->maxntracks+1

      if (img->hierarchical)
        {
          img->maxbuttons = (menu == 0) ? ngroups : Min(MAX_BUTTON_Y_NUMBER - 2, ntracks[groupcount]);
          img->resbuttons = 0;
        }

      arrowbuttons = (menu < img->nmenus - 1) + (menu > 0);
      menubuttons = (menu < img->nmenus - 1) ? img->maxbuttons : img->maxbuttons + img->resbuttons;
      compute_coordinates(command, img->ncolumns, x0, y0, x1, y1);

      if (img->hierarchical)
        {

          if (menu == 0)
            {
              do
                {
                  buttons++;
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[0], "\"", " y0=\"", y0[groupcount], "\"", " x1=\"", x1[0], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[groupcount], "\"/>");
                  groupcount++;
                }
              while (groupcount < ngroups);
              groupcount = 0;
            }
          else if (groupcount < ngroups)
            {
              do
                {
                  buttons++;
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[group], "\"", " y0=\"", y0[track], "\"", " x1=\"", x1[group], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[track], "\"/>");
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));

              if (track == ntracks[groupcount])
                {
                  groupcount++;
                  track = 0;
                }
            }
        }
      else if (img->page_ntracks)
        {
          /* 一页一个专辑：按钮矩形必须与 menu.c 的文字**同一套行距**。
             compute_coordinates() 是按 img->maxbuttons 分行，所以要先把它
             设成本页曲目数，再算坐标。 */
          if ((unsigned) menu < img->npages)
            {
              img->maxbuttons = img->page_ntracks[menu];

              if (menu < (unsigned) img->index_pages)
                {
                  /* 一级菜单（专辑索引页）：按钮是**网格矩形**，
                     不能用 compute_coordinates（那按行分行）。
                     坐标必须与 menu.c 的 mogrify_thumb()、
                     menu_assets.py 的 make_index_page() 完全一致。 */
                  for (track = 0; track < img->maxbuttons; ++track)
                    {
                      uint16_t bx0 = (uint16_t) ((track % INDEX_COLS)
                                                 * INDEX_CELL_W + INDEX_INSET);
                      uint16_t by0 = (uint16_t) (INDEX_TOP + (track / INDEX_COLS)
                                                 * INDEX_CELL_H + INDEX_INSET);
                      uint16_t bx1 = (uint16_t) (bx0 + INDEX_CELL_W
                                                 - 2 * INDEX_INSET);
                      uint16_t by1 = (uint16_t) (by0 + INDEX_CELL_H
                                                 - 2 * INDEX_INSET);
                      buttons++;
                      fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", bx0, "\"", " y0=\"", by0, "\"", " x1=\"", bx1, "\"", " name=\"button", buttons, "\"", " y1=\"", by1, "\"/>");
                    }

                  buttons = img->maxbuttons;
                }
              else
                {
                  compute_coordinates(command, img->ncolumns, x0, y0, x1, y1);

                  for (track = 0; track < img->maxbuttons; ++track)
                    {
                      buttons++;
                      fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[0], "\"", " y0=\"", y0[track], "\"", " x1=\"", x1[0], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[track], "\"/>");
                    }

                  buttons = img->maxbuttons;
                }
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
                  fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[group], "\"", " y0=\"", y0[track - offset], "\"", " x1=\"", x1[group], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[track - offset], "\"/>");
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
        }

      if (img->nmenus > 1)
        {
          /* 与 menu.c 的箭头文字一一对应（同一套槽位与页判定）：
             第 1 个槽放 Next（末页改放 Previous），中间页再补一个 Previous。
             ⚠️ 原 do-while 在曲目不满一页的末页会因 buttons 基数偏低而多转
             几轮，输出 5 个完全重叠的按钮。

             ⚠️ 索引页必须用**网格几何**：那几页不调 compute_coordinates()，
             x0[]/y0[]/x1[]/y1[] 全是未初始化的值 —— 曾经因此输出
             "Button coordinates out of range (720,576): (33,10944)-(708,35056)"，
             spumux 直接失败、topmenu 输出 0 字节。 */
          int is_index = (img->page_ntracks
                          && (menu < (unsigned) img->index_pages));
          int arrows_emitted = 0;
          int has_next = DVDA_HAS_NEXT(img, menu);
          int has_prev = DVDA_HAS_PREV(img, menu);
          int slot1 = has_next || has_prev;
          /* 槽 3：「返回专辑索引」。条件与 menu.c / dvdauthor XML 完全一致。 */
          int has_menu_button = (img->index_pages > 0) && !is_index;

          /* 槽 1：只有真的还有可翻的页才出按钮 —— 索引页的 Next 限定在
             索引页之间，所以最后一个索引页这个槽是空的。 */
          if (slot1)
            {
              buttons++;
              arrows_emitted++;
              if (is_index)
                fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n",
                        "       <button x0=\"",
                        (has_next ? INDEX_NEXT_X0 : INDEX_PREV_X0), "\"",
                        " y0=\"", INDEX_ARROW_Y0, "\"",
                        " x1=\"",
                        ((has_next ? INDEX_NEXT_X0 : INDEX_PREV_X0)
                         + INDEX_ARROW_W),
                        "\"", " name=\"button", buttons, "\"",
                        " y1=\"", INDEX_ARROW_Y1, "\"/>");
              else
                fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[img->ncolumns - 1], "\"", " y0=\"", y0[command->img->maxbuttons], "\"", " x1=\"", x1[img->ncolumns - 1], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[command->img->maxbuttons], "\"/>");
            }

          if (has_next && has_prev)
            {
              buttons++;
              arrows_emitted++;
              if (is_index)
                fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n",
                        "       <button x0=\"", INDEX_PREV_X0, "\"",
                        " y0=\"", INDEX_ARROW_Y0, "\"",
                        " x1=\"", (INDEX_PREV_X0 + INDEX_ARROW_W), "\"",
                        " name=\"button", buttons, "\"",
                        " y1=\"", INDEX_ARROW_Y1, "\"/>");
              else
                fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n", "       <button x0=\"", x0[img->ncolumns - 1], "\"", " y0=\"", y0[command->img->maxbuttons + 1], "\"", " x1=\"", x1[img->ncolumns - 1], "\"", " name=\"button", buttons, "\"", " y1=\"", y1[command->img->maxbuttons + 1], "\"/>");
            }

          /* 槽 3：索引页没有这个按钮（is_index 已排除），
             所以坐标一律用 compute_coordinates() 的 y0/y1
             —— 那一行由 MENU_BUTTON_ROW() 指定，两边必须一致。 */
          if (has_menu_button)
            {
              buttons++;
              arrows_emitted++;
              fprintf(spu_xmlfile, "%s%d%s%s%d%s%s%d%s%s%02d%s%s%d%s\n",
                      "       <button x0=\"", x0[img->ncolumns - 1], "\"",
                      " y0=\"", y0[MENU_BUTTON_ROW(img)], "\"",
                      " x1=\"", x1[img->ncolumns - 1], "\"",
                      " name=\"button", buttons, "\"",
                      " y1=\"", y1[MENU_BUTTON_ROW(img)], "\"/>");
            }

          if (globals->debugging && (arrows_emitted != (int) arrowbuttons))
            foutput(WAR "Arrow button count mismatch: %d emitted, %d expected\n",
                    arrows_emitted, (int) arrowbuttons);
        }
      menu++;
      buttons = 0;
      group = 0;

      fprintf(spu_xmlfile, "%s\n", "    </spu>\n\
  </stream>\n\
</subpictures>\n");

      fclose(spu_xmlfile);
    }
  while ((menu < img->nmenus) && (groupcount < ngroups));


  if (errno) foutput("%s\n", ERR "Could not generate spumux xml project file properly for generating DVD-Audio menu");
  else if (globals->debugging) foutput("%s\n", MSG_TAG "spumux xml dvdauthor project file was generated.");

  return (errno);
}
#endif
