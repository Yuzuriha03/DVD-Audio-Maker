#if !HAVE_MENU_H
#define HAVE_MENU_H

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include "structures.h"

#if !defined HAVE_core_BUILD || !HAVE_core_BUILD

int  generate_background_mpg(pic* img, globalData*);
int prepare_overlay_img(char* text, int8_t group, pic *img, char* command, char* command2, int menu, char* albumcolor, globalData*);
int  launch_spumux(pic* img, globalData*);
int  launch_dvdauthor(globalData*);
int mogrify_img(char* text, int8_t group, int8_t track, pic *img, uint8_t maxnumtracks, char* command, char* command2,  int8_t offset, char* textcolor);
int  generate_menu_pics(command_t* , pic* img, uint8_t ngroups, uint8_t *numtracks, globalData*);
int  compute_menu_pages(pic* img, uint8_t ngroups, uint8_t *ntracks, globalData* globals);
int create_stillpic_directory(char* string, int32_t count, globalData*);
void create_activemenu(pic* img, globalData* globals);
int create_mpg(pic* img, uint16_t rank, char* mp2track, char* tempfile, globalData*);
uint16_t x(uint8_t group, uint8_t ngroups);
uint16_t y(uint8_t track, uint8_t maxnumtracks);
void initialize_binary_paths(char level, globalData*);
void menu_characteristics_coherence_test(pic* img, uint8_t ngroups, globalData*);
void compute_pointsize(pic* img, uint16_t maxtracklength, uint8_t maxnumtracks, globalData*);
/* 由码流自检得出 DVD 视频属性字节（0x43 = NTSC，0x53 = PAL）。
   见 docs/DVDA-AUTHOR-CHANGES.md。 */
uint8_t dvda_video_attr_of(const char* path, pic* image, globalData* globals);
/* ---- 一级菜单（专辑索引页）的缩略图网格 ----

   前 `img->index_pages` 页是专辑缩略图网格，点一下跳到该专辑的
   选曲页（`jump menu`）。格子几何必须三方一致：
     · menu.c   mogrify_thumb()      —— 画按钮区域（描边矩形）
     · xml.c    按钮 / spumux 坐标
     · menu_assets.py  make_index_page() —— 缩放略图 + 专辑名

       格子(col,row) = (col*CELL_W + INSET, INDEX_TOP + row*CELL_H + INSET)
       尺寸 = CELL_W-2*INSET x CELL_H-2*INSET

   三方不一致就会出现「点到的不是想选的那张」。

   ⚠️ 顶部 INDEX_TOP 是**留给大标题（光盘标题）的带子**：
   标题由 prepare_overlay_img() 在**每一页**画在 y≈28..54，
   网格从 0 开始就会压在缩略图上（实测过）。

   ⚠️ 网格占 INDEX_TOP..INDEX_TOP+INDEX_ROWS*INDEX_CELL_H（60..432），
   底部 144 px 是翻页箭头带 —— 索引页也要能翻页（专辑多于一页时
   没箭头就走不掉）。箭头用**绝对坐标**（下面的 INDEX_*_X0/Y0），
   不能走 compute_coordinates()：那里按「行」分行，而索引页的
   maxbuttons 是格子数，两者不是一回事（曾经因此拿到未初始化的
   坐标，spumux 直接报 "Button coordinates out of range" 并输出 0 字节）。 */
#define INDEX_COLS   4
#define INDEX_ROWS   3
#define INDEX_INSET  5
#define INDEX_TOP    60      /* 大标题带：0..59 留给标题墨迹 */

/* 格子尺寸是**显式常量**，不能写 norm_x/COLS、norm_y/ROWS ——
   后者会把画面均分（576/3 = 192），而网格只占一段。
   两边不一致时按钮区与缩略图就错位。 */
#define INDEX_CELL_W 180
#define INDEX_CELL_H 140     /* 60 + 3*140 = 480 = 网格底 */
#define INDEX_PER_PAGE (INDEX_COLS * INDEX_ROWS)

/* 索引页第 p 页、第 k 格的专辑，其内容页（菜单号 1-based）=
     index_pages + p * INDEX_PER_PAGE + k + 1
   menu_assets.py 的 INDEX_PER_PAGE 必须与此相同。 */

#define INDEX_ARROW_Y0   496      /* 箭头带：网格下方（480..576），垂直居中 */
#define INDEX_ARROW_Y1   552
#define INDEX_ARROW_W    160
#define INDEX_PREV_X0     40
#define INDEX_NEXT_X0    520

/* ---- 二级（选曲）页的「返回专辑索引」按钮 ----

   占底部**第三个**槽位（前两个是 Next / Previous），内容固定是
   `jump menu 1`（第一个菜单页 = 索引页 1）。

   ⚠️ 行号必须三处一致：
     · menu.c  generate_menu_pics()  —— 画文字（mogrify_img 的 track）
     · xml.c   dvdauthor XML         —— 出 `jump menu 1`
     · xml.c   spumux XML            —— 出按钮矩形
   `compute_coordinates()` 会把 y 填到 MENU_BUTTON_ROW，
   少填一行就会取到未初始化的坐标（曾经因此让 spumux 断言失败）。

   只在 `--index-pages > 0` 时出现 —— 没有索引页就无处可回；
   索引页自身也不画（那一页**就是**索引）。 */
#define MENU_BUTTON_ROW(img) ((img)->maxbuttons + 2)

/* ---- 翻页按钮的可用性（menu.c 与 xml.c 必须用同一套）----

   ⚠️ 索引页的 Next **只在索引页之间翻** —— 不能拿 Next 从索引页跳到
   专辑内容页（那是缩略图的活，而且跳过去会猜错用户想听哪张专辑）。
   所以**最后一个索引页没有 Next**。

   Previous 一律「不是第一页就有」。 */
#define DVDA_HAS_PREV(img, menu) ((unsigned) (menu) > 0)
#define DVDA_HAS_NEXT(img, menu)                                        \
  (((img)->page_ntracks                                                 \
    && (unsigned) (menu) < (unsigned) (img)->index_pages)               \
   ? ((unsigned) (menu) + 1 < (unsigned) (img)->index_pages)            \
   : ((unsigned) (menu) + 1 < (unsigned) (img)->nmenus))

/* ---- 文字描边（白色字身 + 一圈深色）的偏移（像素）----

   所有文字都画**两遍**：先在 (x+DX, y+DY) 画一遍深色的，再在 (x, y)
   画白字 —— 也就是索引页专辑名那一套（`menu_assets` 里用 `caption:`
   同样画两遍）。偏移 2 px，字号 25~30 时刚好，再大就糊。

   ⚠️ 这两遍**分属两层**，颜色不同：
     · 图像层 impic  → `bgcolor_pic`（黑）  = 未选中时看到的描边
     · 高亮层 hlpic  → `highlightcolor_pic`（红）= 选中时看到的描边
   spumux 把「选中态」的颜色取自 **hlpic 层**（见 subgen.c 里往 IFO 写
   ST_COLI 的那段：`findmasterpal(s, s->hlt.pal + ...)`），所以红色必须
   画在 hlpic 上，而不是靠 `_PALETTE` 换槽。 */
#define TEXT_SHADOW_DX 2
#define TEXT_SHADOW_DY 2

/* ---- 选中指示：行左侧的小三角箭头 ----

   ⚠️ 已放弃「选中时把描边变红」那套：描边是同一段文字偏移 2px，与字身
   重叠处的三色组合会多出一种（实测 5 种），而 spumux 每个按钮的调色板
   只有 4 项 → `pickbuttongroups()` 失败 → `ERR: Cannot pick button
   masks` → 菜单全丢。所以文字**两种状态下完全一样**（白字 + 黑描边），
   选中与否只靠这个箭头表示。

   箭头画在文字**左边**（x0 - GAP - W 起），与「字身 + 向右下偏移 2px 的
   描边」都不重叠 —— 这样三色组合恰好 4 种：透明 / 白字 / 黑描边 / 红箭头。

   位置：x0=45（ncolumns=1）时左顶点在 34，按钮矩形从 33 开始，正好在里面。 */
/* ---- 一级菜单（专辑索引页）的**画面**常量 ----
   画面由 C 现画（`dvda_make_index_pages()`），不再由外部脚本预先拼图 ——
   几何常量只在 `menu.h` 定义一份，`menu.c`（画）、`xml.c`（按钮矩形）
   都取它，不存在「两边不一致」的可能。

   画面 = 三层背景 + 每格 [封面 + 专辑名] + 缩略图描边：

     背景   对角渐变 + 与格子对齐的细网格 + 径向暗角
     格子   封面缩到 INDEX_THUMB 见方居中；下方 INDEX_LABEL_H 放专辑名
     名称   白字 + `caption:` 自动换行；字号按文字宽度估（见
            `index_label_pointsize()`）
     描边   单元格子最后单独一遍画（`-flatten` 之后列表只剩一张，
            `-draw` 才安全 —— 见 TROUBLESHOOTING 第 22 节） */
#define INDEX_THUMB          100   /* 缩略图边长（正方形，封面是 1:1） */
#define INDEX_THUMB_GAP        2   /* 缩略图与名称条的间隙 */
#define INDEX_LABEL_H         28   /* 名称条高度 */
#define INDEX_LABEL_W         (INDEX_CELL_W - 2 * INDEX_INSET)   /* 170 */
#define INDEX_LABEL_FONT_MAX  17   /* 名称字号上限（一行放不下就缩） */
#define INDEX_LABEL_FONT_MIN   9   /* 名称字号下限（再小看不清） */

/* 色彩用 `rgb(...)` 而不是 `#rrggbb` —— 前者在 shell 里只需引号包住，
   后者虽然也能过，但 `#` 在别处容易被当注释，改写时容易踩坑。
   ⚠️ **必须是彩色**：`-flatten` 的输出色彩空间看**第一张图**（= 背景），
   背景是灰度时封面的彩色会被全部丢掉（整页变黑白）。 */
#define INDEX_BG_FROM      "rgb(62,107,138)"    /* 左上：青蓝 */
#define INDEX_BG_TO        "rgb(34,22,48)"      /* 右下：深靛（暖紫调） */
#define INDEX_BG_GRID      "rgba(255,255,255,0.14)"
#define INDEX_BG_VIGNETTE  "#8090a0"            /* 暗角：只压亮度不改色相 */
#define INDEX_BORDER_COLOR "rgb(42,42,42)"
#define INDEX_BORDER_W     2

/* 生成全部索引页的画面（写 `<tempdir>/bgpic<p>.jpg`）。
   封面路径来自 `--index-covers`（扁平列表，页序 × 格子序），
   专辑名从 `--screentext` 的前 index_pages 段里取。
   返回 0 成功。 */
int dvda_make_index_pages(pic *img, globalData *globals);

#define TEXT_ARROW_W    7
#define TEXT_ARROW_H    6
#define TEXT_ARROW_GAP  4
#endif
#endif // HAVE_MENU_C
