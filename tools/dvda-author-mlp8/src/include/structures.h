#ifndef STRUCTURES_H_INCLUDED
#define STRUCTURES_H_INCLUDED


#include "stream_decoder.h"
#include "inttypes.h"
#include "commonvars.h"
#include "mlplayout.h"

typedef struct
{
    uint8_t  nextractgroup;
    uint8_t  extracttitleset[81];
    uint8_t  extracttrackintitleset[81][99];
} extractlist;

typedef struct
{
    uint8_t bitspersample;
    uint8_t channels;
    uint8_t cga;
    uint8_t type;
    uint32_t samplerate;
} audioformat_t;

typedef struct
{
    uint8_t  header_size;
    uint8_t* channel_header_size;
    uint8_t  type;
    uint8_t  bitspersample;
    uint8_t  channels;
    uint8_t  resample_bitspersample;
    uint8_t  resample_channels;
    uint8_t  cga;
    uint8_t  buf[1024*256];
    int8_t   downmix_table_rank;
    uint8_t  newtitle;
    uint8_t  contin_track;
    uint8_t  firstpackdecrement;
    uint8_t  firstpack_lpcm_headerquantity;
    uint8_t  midpack_lpcm_headerquantity;
    uint8_t  lastpack_lpcm_headerquantity;
    uint8_t  firstpack_pes_padding;
    uint8_t  midpack_pes_padding;
    uint8_t  samplesperframe;
    uint8_t  lastpack_audiopesheaderquantity;
    uint16_t sampleunitsize;
    uint16_t bytesperframe;
    uint16_t lpcm_payload;
    uint16_t firstpack_audiopesheaderquantity;
    uint16_t midpack_audiopesheaderquantity;
    bool     mergeflag;
    bool     dvdv_compliant;
    uint32_t samplerate;
    uint32_t resample_samplerate;
    uint32_t first_sector;
    uint32_t last_sector;
    uint32_t dw_channel_mask;
    uint32_t first_PTS;
    uint32_t PTS_length;
    uint32_t mlp_layout_size;
    uint32_t n;
    uint32_t eos;
    uint32_t bytesread;
    uint32_t *pts;
    uint32_t *dts;
    uint64_t *scr;
    uint64_t numsamples;
    uint64_t numbytes; // theoretical audio size
    uint64_t pcm_numbytes; // theoretical audio size
    uint64_t wav_numbytes; // wav audio size
    uint64_t file_size; // file size on disc
    uint64_t *channel_size; // channel size on disc
    uint64_t bytespersecond;
    FILE* fp;
    FILE** channel_fp;
    FLAC__StreamDecoder* flac;
    char    *filename;
    char    *out_filename;
    char    *mlp_filename;
    char    **given_channel;
    struct  MLP_LAYOUT *mlp_layout;
    /* 该轨在一个 title（PGC）时间轴上的起点偏移。MLP 的 pts[] 是按轨的
       （每轨从 PTS0 起算），合并成一个 title 后必须加上这个偏移才能让
       title 内的时间轴连续 —— 否则每个 cell 的 first_pts 都是 98，
       播放器按时间轴寻址任何一首都会落到第 1 首。
       见 src/ats.c 里 write_pes_packet 与按轨刷 pack 处的说明。 */
    uint32_t pts_shift;
} fileinfo_t;

typedef struct
{
    bool manual;
    bool active;
    uint8_t starteffect;
    uint8_t endeffect;
    uint8_t lag;
    uint16_t onset;
} stilloptions;

typedef struct
{
    bool refresh;
    bool loop;
    bool hierarchical;
    bool active;
    char** highlightpic;
    char** selectpic;
    char** imagepic;
    char** backgroundpic; // The background of the top menu, type is .jpg. There can be many.
    char* blankscreen;    // In principe blank for adding titles yet can have some background, type is .png
    char** backgroundmpg;
    char** backgroundcolors;
    char* activeheader;
    char** topmenu;
    char*** topmenu_slide;
    char* stillvob;
    char* tsvob;
    char*** soundtrack;
    char* audioformat;
    char* albumcolor;
    char* groupcolor;
    char* arrowcolor;

    char* textcolor_pic;
    char* bgcolor_pic;
    char* highlightcolor_pic;
    char* selectfgcolor_pic;

    char* activetextcolor_palette;
    char* activebgcolor_palette;
    char* activehighlightcolor_palette;
    char* activeselectfgcolor_palette;

    char* textcolor_palette;
    char* bgcolor_palette;
    char* highlightcolor_palette;
    char* selectfgcolor_palette;

    char* textfont;
    char* screentextchain;
    char* framerate;
    char* norm;
    char* aspect;
    char* aspectratio;
    uint8_t pointsize;
    uint8_t fontwidth;
    int8_t highlightformat;
    uint8_t h;
    uint8_t min;
    uint8_t sec;
    uint8_t action;
    uint8_t nmenus;
    uint8_t ncolumns;
    uint8_t maxbuttons;
    uint8_t resbuttons;
    uint16_t count;
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

    /* 一级菜单（专辑索引页）—— 对应命令行 `--index-pages N`。

       N > 0 时，**前 N 页是缩略图网格**，而不是曲目列表：
         · 这些页上的「曲目」其实是专辑格子（page_ntracks[p] = 格子数，
           page_group/page_t0 无意义、置 0）
         · 这些页**不画任何文字**（缩略图已经由菜单背景提供）
         · 第 p 页第 k 格的按钮是
           `jump menu (N + p*INDEX_PER_PAGE + k + 1)`
           —— 纯位置映射，见 menu.h / menu_assets.py 的 INDEX_* 常量

       其余页仍是「一页一个专辑」的曲目列表。

       ⚠️ 同样**必须追加在结构末尾**（pic 按位置初始化）。 */
    uint8_t  index_pages;

    /* 索引页每格的**封面图路径**，对应命令行 `--index-covers`。
       扁平列表、顺序 = 「页序 × 格子序」，即第 p 页第 k 格是
       indexcovers[p * INDEX_PER_PAGE + k]。
       个数由 `page_ntracks[p]` 决定（C 按它逐个取用）。

       索引页的画面由 C 现画（menu.c 的 `dvda_make_index_pages()`），
       这里只提供素材路径。

       ⚠️ 同样**必须追加在结构末尾**（pic 按位置初始化）。 */
    char    **indexcovers;
    int       indexcoverssize;

} pic;

typedef struct
{
  float Lf_l;
  float Lf_r;
  float Rf_l;
  float Rf_r;
  float C_l;
  float C_r;
  float S_l;
  float S_r;
  float Rs_l;
  float Rs_r;
  float LFE_l;
  float LFE_r;
  bool custom_table;

} downmix;

typedef struct
{
    uint16_t maxntracks;
    uint8_t ngroups;
    uint8_t n_g_groups;
    uint8_t nplaygroups;
    uint8_t *playtitleset;
    uint8_t nvideolinking_groups;
    uint8_t maximum_VTSI_rank;
    uint8_t *VTSI_rank;
    uint8_t *ntracks;
    char*   provider;
    pic*    img;
    fileinfo_t **files;
    char**  textable;
    downmix *db;
}command_t;

typedef struct
{
    uint8_t ngroups;
    uint8_t *ntracks;

} parse_t;


typedef struct
{
    uint16_t nlines;
    char **commandline;
} lexer_t;

typedef struct
{
    uint8_t samg;
    uint8_t amg;
    uint8_t asvs;
    uint8_t atsi[9];
    uint32_t stillvob;
    uint32_t topvob;

} sect;

#endif // STRUCTURES_H_INCLUDED
