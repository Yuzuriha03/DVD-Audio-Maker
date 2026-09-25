
//#undef __STRICT_ANSI__

#if !defined HAVE_core_BUILD || !HAVE_core_BUILD

#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <errno.h>
#include <math.h>
#include <sys/types.h>
#ifndef __WIN32__
#include <sys/wait.h>
#include <unistd.h>
#include <fcntl.h>
#endif
#include <sys/stat.h>
#include "structures.h"
#include "c_utils.h"
#include "launch_manager.h"
#include "winport.h"
#include "auxiliary.h"
#include "amg.h"
#include "menu.h"
#include "commonvars.h"



// Automated top-menu generation using patched dvdauthor
// We authorize only maximal resolution form input pics (ie: 720x576, pal/secam or 720x480, ntsc)

uint16_t norm_x = PAL_X, norm_y = PAL_Y; // TODO: adjust for ntsc #define NTSC_Y 480
extern uint16_t totntracks;


void menu_characteristics_coherence_test(pic *img, uint8_t ngroups, globalData *globals)
{
  if (img->active)
    {
      if (globals->topmenu == NO_MENU)
        globals->topmenu = TEMPORARY_AUTOMATIC_MENU; // you need to create a TS_VOB at least temporarily
      if (img->nmenus > 1)
        {
          foutput("%s", WAR "Active menus can only be used with simple menus for version "VERSION"\n       Using img->nmenus=1...\n");
          img->nmenus = 1;
        }
      if (img->hierarchical)
        {
          foutput("%s", WAR "Active menus cannot be used with hierarchical menus for version "VERSION"\n       Choosing hierarchical menus...\n");
          img->active = 0;
          img->hierarchical = 1;
        }
    }

  // default values must be set even if globals->topmenu = NO_MENU

  if (ngroups)
    {

      if (img->nmenus == 0)
        {
          if (img->ncolumns == 0) img->ncolumns = DEFAULT_MENU_NCOLUMNS; // just in case, not to divide by zero, yet should not arise unless...
          if (img->hierarchical) img->nmenus = ngroups + 1; // list of groups and one menu per group only (--> limitation to be indicated)
          else

            img->nmenus = ngroups / img->ncolumns + (ngroups % img->ncolumns > 0); // number of columns cannot be higher than img->ncolumns; adjusting number of menus to ensure this.
          if (globals->topmenu != NO_MENU) foutput(MSG_TAG "With %d columns, number of menus will be %d\n", img->ncolumns, img->nmenus);
        }
      else
        {
          if ((img->hierarchical) && (img->nmenus == 1))
            {
              foutput("%s", WAR "Hierarchical menus should have at least two screens...\n       Incrementing value for --nmenus=1->2\n");
              img->nmenus++;
            }

          img->ncolumns = ngroups / (img->nmenus - img->hierarchical) + (ngroups % (img->nmenus - img->hierarchical) > 0);

          /* 该上限只对「分层菜单」成立（首页列组 + 每组一页）。
             非分层菜单的页数只受按钮总数约束，套用此式会把用户给的
             --nmenus 静默压成 ngroups*ncolumns+1（实测 3 组时 8 -> 4）。 */
          if (img->hierarchical && ((img->ncolumns)*ngroups < img->nmenus - 1))
            {
              foutput(WAR "Hierarchical menus should have at most %d*%d+1=%d menus...\n       Resetting value for --nmenus=%d\n", img->ncolumns, ngroups, img->ncolumns * ngroups + 1, img->ncolumns * ngroups + 1);
              img->nmenus = ngroups * img->ncolumns + 1;
            }

        }

      /* 每页容量：总数按页数均分（向上取整），再受屏幕上限约束。
         原式先截 32 再除页数，导致所有页合计恒 <=32 个按钮。 */
      img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2,
                            (totntracks + img->nmenus - 1) / img->nmenus);
      img->resbuttons = 0;
    }


}



/* patches AUDIO_TS.VOB into an active-menu type AUDIO_SV.VOB at minor processing cost */

void create_activemenu(pic *img, globalData *globals)
{
  if (img->tsvob == NULL) EXIT_ON_RUNTIME_ERROR_VERBOSE("No matrix AUDIO_TS.VOB available for generating active menus.")

    uint8_t j;
  uint64_t i;
  uint64_t activeheadersize = 0;

  char *activeheader = copy_file2dir(img->activeheader, globals->settings.tempdir, globals);
  activeheadersize = stat_file_size(activeheader);

  FILE *activeheaderfile = NULL;

  if (!globals->nooutput)
    activeheaderfile = fopen(activeheader, "rb");

  /* processing */

  foutput("%s\n", INF "Using already created top menus.\n");

  uint64_t tsvobsize = 0;
  tsvobsize = stat_file_size(img->tsvob);
  if (tsvobsize <= activeheadersize)
    {
      perror(ERR "AUDIO_TS.VOB is too small.\n");
      clean_exit(EXIT_FAILURE, globals) ;
    }
  uint8_t tsvobpt[tsvobsize];
  memset(tsvobpt, 0, tsvobsize);

  FILE *tsvobfile = fopen(img->tsvob, "rb");

  if (!globals->nooutput && fread(tsvobpt, activeheadersize, 1, activeheaderfile) == 0) perror(ERR "fread [active menu authoring, stage 1]");

  if (-1 == fseek(tsvobfile, ACTIVEHEADER_INSERTOFFSET, SEEK_SET)) perror(ERR "fseek [active menu authoring, stage 2]");

  if (fread(tsvobpt + activeheadersize + 32, 0x314 - ACTIVEHEADER_INSERTOFFSET, 1, tsvobfile) == 0) perror(ERR "fread [active menu authoring, stage 3]");
//nlinks=1;
  i = activeheadersize;

  tsvobpt[i] = 0x10;
  tsvobpt[i + 3] = (uint8_t) totntracks; // A max of 256 links ?
  tsvobpt[i + 4] = (uint8_t) totntracks;
  uint32_copy(&tsvobpt[i + 8],   0x32FFDD00); //uint32_copy(&tsvobpt[i+8],   0x0010B0B0);
  uint32_copy(&tsvobpt[i + 12],   0x34FFDD00); //uint32_copy(&tsvobpt[i+12],   0x0020B090);

  i = 0x314;
  uint32_copy(&tsvobpt[i], 0x01BE04E8);
  i += 4;
  while (i < 0x800)
    {
      tsvobpt[i] = 0xFF;
      i++;
    }
  if (-1 == fseek(tsvobfile, 0x800, SEEK_SET)) perror(ERR "fseek [active menu authoring, stage 4]");
  if (fread(tsvobpt + 0x800, tsvobsize - 0x800, 1, tsvobfile) == 0) perror(ERR "fread, stage 1, create_activemenu");


  /* writing */
  if (img->stillvob == NULL)
    {
      img->stillvob = strdup(img->tsvob);
      if (img->stillvob)
        {
          img->stillvob[strlen(img->stillvob) - 5] = 'V';
          img->stillvob[strlen(img->stillvob) - 6] = 'S';
        }
      else
        {
          perror(ERR " stillvob string allocation.\n");
          return;
        }
    }


  FILE *svvobfile;
  if (!globals->nooutput)
    {
      svvobfile = fopen(img->stillvob, "wb");
      if (svvobfile == NULL)
        EXIT_ON_RUNTIME_ERROR_VERBOSE("Cannot open AUDIO_SV.VOB for generating active menus.")

        foutput("\n"DBG "Creating active menu: will patch AUDIO_TS.VOB into AUDIO_SV.VOB=%s\n\n", img->stillvob);

      for (j = 0; j < totntracks; j++)
        fwrite(tsvobpt, tsvobsize, 1, svvobfile);
      fclose(svvobfile);
    }

  free(activeheader);

  if (globals->topmenu == TEMPORARY_AUTOMATIC_MENU)
    {
      unlink(img->tsvob);
      img->tsvob = NULL;
    }

  return;
}

char *mp2enc = NULL;
char *jpeg2yuv = NULL;
char *mpeg2enc = NULL;
char *mplex = NULL;
char *mogrify = NULL;
char *dvdauthor = NULL;
char *spumux = NULL;
char *convert = NULL;
char *mpeg2dec = NULL;
char *pgmtoy4m = NULL;
static char *curl = NULL;
char *extract_ac3 = NULL;
char *ac3dec = NULL;

void initialize_binary_paths(char level, globalData *globals)
{
  ///   saves ressources by ensuring this is done just once  ///
  static uint16_t count1, count2, count3, count4, count5, count6, count7;
  switch (level)
    {

    case CREATE_EXTRACT_AC3:
      if (!count7)
        {
          extract_ac3 = create_binary_path(extract_ac3, EXTRACT_AC3, SEPARATOR EXTRACT_AC3_BASENAME, globals);
          ac3dec      = create_binary_path(ac3dec, AC3DEC, SEPARATOR AC3DEC_BASENAME, globals);
          ++count7;
        }
      break;

    case CREATE_MJPEGTOOLS:
      if (!count1)
        {
          // if installed with autotools, if bindir overrides then use override, otherwise use config.h value;
          // if not installed with autotools, then use command line value or last-resort hard-code set defaults and test for result

          mp2enc   = create_binary_path(mp2enc, MP2ENC, SEPARATOR MP2ENC_BASENAME, globals);
          jpeg2yuv = create_binary_path(jpeg2yuv, JPEG2YUV, SEPARATOR JPEG2YUV_BASENAME, globals);
          mpeg2enc = create_binary_path(mpeg2enc, MPEG2ENC, SEPARATOR MPEG2ENC_BASENAME, globals);
          mplex    = create_binary_path(mplex, MPLEX, SEPARATOR MPLEX_BASENAME, globals);
          pgmtoy4m = create_binary_path(pgmtoy4m, MPLEX, SEPARATOR MPLEX_BASENAME, globals);
          ++count1;
        }
      break;

    case CREATE_SPUMUX:
      if (!count2)
        {
          spumux = create_binary_path(spumux, SPUMUX, SEPARATOR SPUMUX_BASENAME, globals);
          ++count2;
        }
      break;

    case CREATE_DVDAUTHOR:
      if (!count3)
        {
          dvdauthor = create_binary_path(dvdauthor, DVDAUTHOR, SEPARATOR DVDAUTHOR_BASENAME, globals);
          count3++;
        }
      break;

    case CREATE_IMAGEMAGICK:
      if (!count4)
        {
          mogrify = create_binary_path(mogrify, MOGRIFY, SEPARATOR MOGRIFY_BASENAME, globals);
          convert = create_binary_path(convert, CONVERT, SEPARATOR CONVERT_BASENAME, globals);
          count4++;
        }
      break;


    case CREATE_MPEG2DEC:
      if (!count5)
        {
          mpeg2dec = create_binary_path(mpeg2dec, MPEG2DEC, SEPARATOR MPEG2DEC_BASENAME, globals);
          count5++;
        }
      break;

    case CREATE_CURL:
      if (!count6)
        {
          curl = create_binary_path(curl, CURL, SEPARATOR CURL_BASENAME, globals);
          count6++;
        }
      break;

    case FREE_MEMORY:
      if (count1)
        {
          free((char *) mp2enc);
          mp2enc = NULL;
          free((char *) jpeg2yuv);
          jpeg2yuv = NULL;
          free((char *) mpeg2enc);
          mpeg2enc = NULL;
          free((char *) mplex);
          mplex = NULL;
        }
      if (count2)
        {
          free((char *) spumux);
          spumux = NULL;
        }
      if (count3)
        {
          free((char *) dvdauthor);
          dvdauthor = NULL;
        }
      if (count4)
        {
          free((char *) mogrify);
          mogrify = NULL;
          free((char *) convert);
          convert = NULL;
        }
      if (count5)
        {
          free((char *) mpeg2dec);
          mpeg2dec = NULL;
        }
      if (count6)
        {
          free((char *) curl);
          curl = NULL;
        }
      if (count7)
        {
          free((char *) ac3dec);
          free((char *) extract_ac3);
          ac3dec = NULL;
          extract_ac3 = NULL;
        }
      break;
    }
}

static char *pict;

/* 程序结束码独占扇区（对齐商业盘）：见 docs/DVDA-AUTHOR-CHANGES.md。
   mplex 把 00 00 01 B9 写在本段最后一个扇区的最后 4 字节；商业盘则是让
   结束码独占一个扇区、其后补 0xFF 到扇区边界。这里把前者改写成后者。 */
static int dvda_pad_program_end(const char *path)
{
  FILE *f = fopen(path, "rb+");
  unsigned char tail[4];
  unsigned char *blk;
  long sz;

  if (f == NULL) return -1;
  if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return -1; }
  sz = ftell(f);
  if (sz < 2048 || (sz % 2048) != 0) { fclose(f); return -1; }

  if (fseek(f, sz - 4, SEEK_SET) != 0) { fclose(f); return -1; }
  if (fread(tail, 1, 4, f) != 4) { fclose(f); return -1; }

  /* 已经是商业盘形态（结束码在扇区开头）或本段没有结束码 → 不动 */
  if (!(tail[0] == 0x00 && tail[1] == 0x00
        && tail[2] == 0x01 && tail[3] == 0xB9))
    {
      fclose(f);
      return 0;
    }

  {
    unsigned char ff[4] = {0xFF, 0xFF, 0xFF, 0xFF};
    if (fseek(f, sz - 4, SEEK_SET) != 0) { fclose(f); return -1; }
    if (fwrite(ff, 1, 4, f) != 4) { fclose(f); return -1; }
  }

  blk = (unsigned char *) malloc(2048);
  if (blk == NULL) { fclose(f); return -1; }
  memset(blk, 0xFF, 2048);
  blk[0] = 0x00; blk[1] = 0x00; blk[2] = 0x01; blk[3] = 0xB9;

  if (fseek(f, 0, SEEK_END) != 0) { fclose(f); free(blk); return -1; }
  if (fwrite(blk, 1, 2048, f) != 2048) { fclose(f); free(blk); return -1; }
  free(blk);
  fclose(f);
  return 0;
}

/* 静图导航扇区改写成商业盘形态。

   mplex 的 `-f 8` 写出的是**空壳**导航包（其手册原文：includes empty
   versions of the peculiar VOBU start sectors）：PCI 之后紧跟一个 DSI，
   而 DSI 里「前后 VOBU 扇区指针」全是 0 —— 播放器据此寻址会跳回 VOB 开头。
   四张盘实测：巴赫 / 李娜 / Enigma 三张商业盘的导航扇区**都没有 DSI**，
   且盘内恒定（同一张盘上第 1 张与第 2 张逐字节相同）；只有本工程带空 DSI。

   每张静图的 mpg 都以自己的导航扇区开头，故此处按扇区原地改写：
   保留首 14 字节 pack 头（与商业盘逐字节相同），其后换成固定的
       系统头(21B) + PCI(751B) + 填充包(1262B)
   合计恒为 2048 字节，扇区数不变 —— 因此 ASVS 偏移表、每图大小、
   其余任何表都不用动。

   商业盘 PCI 里只有 6 个非零字节（Enigma 更只剩 1 个）→ 播放器并不读 PCI
   的内容，只要导航扇区「存在且不含误导性的 DSI」。

   判据是「0x400 处是否为 DSI 起始码」，故本函数幂等。 */
static int dvda_rewrite_nav_sector(const char *path)
{
  static const unsigned char SYS_HDR[15] = {
    0x80, 0xC4, 0xE1, 0x00, 0x61, 0x7F, 0xB9, 0xE0,
    0xE8, 0xBD, 0xE0, 0x34, 0xBF, 0xE0, 0x01
  };
  unsigned char blk[2048];
  FILE *f;

  if (path == NULL) return -1;

  f = fopen(path, "rb+");
  if (f == NULL) return -1;

  if (fread(blk, 1, sizeof blk, f) != sizeof blk)
    {
      fclose(f);
      return 0;                 /* 不足一扇区，不该发生 */
    }

  /* 已经是商业盘形态（0x400 落在全 0xFF 的填充区）→ 不动 */
  if (memcmp(blk + 0x400, "\x00\x00\x01\xBF", 4) != 0)
    {
      fclose(f);
      return 0;
    }

  /* 系统头：长 15 */
  blk[14] = 0x00; blk[15] = 0x00; blk[16] = 0x01; blk[17] = 0xBB;
  blk[18] = 0x00; blk[19] = 0x0F;
  memcpy(blk + 20, SYS_HDR, sizeof SYS_HDR);

  /* PCI 包：数据 745 字节，只有 6 个非零字节 */
  blk[35] = 0x00; blk[36] = 0x00; blk[37] = 0x01; blk[38] = 0xBF;
  blk[39] = 0x02; blk[40] = 0xE9;
  memset(blk + 41, 0x00, 745);
  blk[41] = 0x02;
  blk[46] = 0x8C; blk[47] = 0xA0;
  memset(blk + 48, 0xFF, 8);

  /* 填充包：数据 1256 字节，补满整扇区 */
  blk[786] = 0x00; blk[787] = 0x00; blk[788] = 0x01; blk[789] = 0xBE;
  blk[790] = 0x04; blk[791] = 0xE8;
  memset(blk + 792, 0xFF, 1256);

  if (fseek(f, 0, SEEK_SET) != 0
      || fwrite(blk, 1, sizeof blk, f) != sizeof blk)
    {
      fclose(f);
      return -1;
    }

  fclose(f);
  return 1;
}

int create_mpg(pic *img, uint16_t rank, char *mp2track, char *tempfile, globalData *globals)
{
  errno = 0;
  static unsigned long s;

  /* pict 是文件级 static，且 generate_background_mpg() 末尾会 FREE(pict)
     把它置 NULL；而 s 是本函数 static、不会复位。于是第二次进入本函数
     （先菜单背景、再静图背景）时 s != 0 但 pict == NULL，
     下一句 sprintf 就写到 NULL —— 实测段错误。判据里补上 pict 是否为空。 */
  if ((s == 0) || (pict == NULL))
    {
      s = MAX(strlen(globals->settings.stillpicdir) + 26, strlen(img->backgroundpic[rank]) + 1);
      pict  = calloc(s, sizeof(char *));
    }

  // Important memory fix here
  // On SOME Unix platforms (e.g. Fedora 10 vs. Gentoo 201906 or Ubuntu 1904), not all, passing variable-size arrays to fork
  // does not work at RUNTIME. Memory should be allocated on heap.
  // buffer overflow potential issue to be checked here. Widening buffer as first unsatisfactory step. TOD: fix this.



  free(img->backgroundmpg[rank]);

  img->backgroundmpg[rank] = calloc(1 + strlen(globals->settings.tempdir) + 17 + 3 + 4 + 1, sizeof(char));

  if (img->action == STILLPICS)
    {
      if (globals->debugging) foutput("%s%u\n", INF "Creating still picture #", rank + 1);

      sprintf(img->backgroundmpg[rank], "%s" SEPARATOR "%s%u%s", globals->settings.tempdir, "background_still_", rank, ".mpg");

      sprintf(pict, "%s" SEPARATOR "pic_%03u.jpg", globals->settings.stillpicdir, rank);  // here stillpic[0] is a subdir.

      if (globals->debugging)
        {
          foutput("%s%d%s\n", DBG "Created still picture path #:", rank + 1, pict);
        }
    }
  else if (img->action == ANIMATEDVIDEO)
    {
      if (globals->debugging) foutput(INF "Creating animated menu rank #%u out of %s\n", rank + 1, img->backgroundpic[rank]);
      sprintf(img->backgroundmpg[rank], "%s" SEPARATOR "%s%u%s", globals->settings.tempdir, "background_movie_", rank, ".mpg");
      strcpy(pict, img->backgroundpic[rank]);
      if (img->backgroundcolors)
        {
          if (globals->veryverbose) foutput("%s\n", INF "Colorizing background jpg files prior to multiplexing...");
          char command[500];

          mogrify = create_binary_path(mogrify, MOGRIFY, SEPARATOR MOGRIFY_BASENAME, globals);

          snprintf(command, 500, "%s -fill \"rgb(%s)\" -colorize 66%% %s", mogrify, img->backgroundcolors[rank], img->backgroundpic[rank]);

          if (globals->debugging) foutput(INF "Launching mogrify to colorize menu: %d with command line %s\n", rank, command);
          if (system(win32quote(command)) == -1) EXIT_ON_RUNTIME_ERROR_VERBOSE("System command failed")
            fflush(NULL);
        }
    }

  initialize_binary_paths(CREATE_MJPEGTOOLS, globals);

  char norm[2];
  norm[0] = img->norm[0];
  norm[1] = 0;

  char *argsmp2enc[] = {MP2ENC_BASENAME, "-o", mp2track, NULL};
  char *argsjpeg2yuv[] = {JPEG2YUV_BASENAME, "-f", img->framerate, "-I", "p", "-n", "1", "-j", pict, "-A", img->aspectratio, NULL};
  char *argsmpeg2enc[] = {MPEG2ENC_BASENAME,  "-f", "8", "-n", norm,  "-o", tempfile, "-a", img->aspect, NULL};
  const char *argsmplex[] = {MPLEX_BASENAME, "-f", "8",  "-o", img->backgroundmpg[rank], tempfile, mp2track, NULL};

  //////////////////////////

  if (img->action == ANIMATEDVIDEO)
    {
      if (globals->debugging) foutput("%s\n", INF "Running mp2enc...");

      char soundtrack[strlen(globals->settings.tempdir) + 11];
      sprintf(soundtrack, "%s"SEPARATOR"%s", globals->settings.tempdir, "soundtrack");
      if (file_exists(soundtrack)) unlink(soundtrack);
      errno = 0;
      change_directory(globals->settings.datadir, globals);
      copy_file(img->soundtrack[0][0], soundtrack, globals);
      change_directory(globals->settings.workdir, globals);

      // using freopen to redirect is safer here
#ifndef _WIN32

      int pid1;
      switch (pid1 = fork())
        {
        case -1:
          foutput("%s\n", ERR "Could not launch "MP2ENC);
          break;

        case 0:


          if (NULL == freopen(soundtrack, "rb", stdin))
            {
              perror(ERR "freopen");
              clean_exit(EXIT_FAILURE, globals);
            }

          dup2(STDOUT_FILENO, STDERR_FILENO);

          if (errno) perror(MP2ENC);
          execv(mp2enc, (char *const *)argsmp2enc);
          foutput("%s\n", ERR "Runtime failure in mp2enc child process");
          return errno;

          break;

        default:
          waitpid(pid1, NULL, 0);
        }
#else
      const char *s = get_command_line(argsmp2enc, globals);
      uint16_t size = strlen(s);
      char cml[strlen(mp2enc) + size + 3 + strlen(img->soundtrack[0][0]) + 1 + 1 + 2];
      sprintf(cml, "%s %s < %s", mp2enc, s, win32quote(img->soundtrack[0][0]));
      foutput("%s %s\n", INF "Launching: ", cml);
      free((char *) s);
      system(win32quote(cml));
#endif
    }

#ifndef _WIN32


  sync();
  int pid2;
  char c;
  int tube[2];
  int tubeerr[2];
  int tubeerr2[2];

  // Two extra tubes are in order to redirect jpeg2yuv and mpeg2enc stdout messages and realign them with overall stdout messages, otherwise they fall out of sync
  // with one another and dvda-author messages.

  if (pipe(tube) || pipe(tubeerr) || pipe(tubeerr2))
    {
      perror(ERR "Pipe");
      return errno;
    }

  if (globals->debugging)
    {
      foutput("%s %s ...\n", INF "Running ", jpeg2yuv);
    }

  if (globals->veryverbose)
    {
      foutput("%s %s ...\n", INF "Then piping to ...", mpeg2enc);
    }

  // Owing to the piping of the stdout streams (necessary for coherence of output) existence checks must be tightened up.
  // System will freeze should an input file not exit, as mjpegtools to not always exit on system error. This may cause a loop in the piping of jpeg2yuv to mpeg2enc
  // Tight system error strategy in order here
  errno = 0;

  FILE *f = fopen(pict, "rb");
  foutput("opening: %s\n", pict);
  if ((errno) || (f == NULL))
    {
      if (img->action == ANIMATEDVIDEO)
        {
          foutput(ERR "menu input files: background pic: %s", pict);
          perror("background");
        }
      else
        {
          foutput(ERR "still pic: %s", pict);
        }
      clean_exit(EXIT_FAILURE, globals);
    }
  fclose(f);
  errno = 0;


//    if (mp2track)
//    {
//        FILE* f=fopen(mp2track, "rb");
//
//        if ((errno) || (f == NULL))
//        {
//            perror(ERR "menu input files: mp2 track");
//            globals->topmenu=NO_MENU;
//
//            return(errno);
//        }
//        fclose(f);
//    }


  switch (fork())
    {
    case -1:
      fprintf(stderr, "%s\n", ERR "Could not launch jpeg2yuv");
      break;

    case 0:

      close(tube[0]);
      close(tubeerr[0]);
      dup2(tube[1], STDOUT_FILENO);
      // Piping stdout is required here as STDOUT is not a possible duplicate for stdout
      dup2(tubeerr[1], STDERR_FILENO);
      execv(jpeg2yuv, (char *const *) argsjpeg2yuv);
      fprintf(stderr, "%s\n", ERR "Runtime failure in jpeg2yuv child process");
      perror("menu1");

      return errno;


    default:
      close(tube[1]);
      close(tubeerr[1]);
      dup2(tube[0], STDIN_FILENO);
      if (globals->debugging) foutput("%s\n", INF "Piping to mpeg2enc...");

      switch (pid2 = fork())
        {
        case -1:
          foutput("%s\n", ERR "Could not launch mpeg2enc");
          break;

        case 0:
          // This looks like an extra complication as it could be considered to simply use dup2(STDOUT_FILENO, stdout_FILENO) without further piping
          // However this would reverse the order of jpeg2yuv and mpeg2enc stdout messages, the latter comming first,
          // which is not desirable as jpeg2yuv is piped into mpeg2enc. Hereby we are realigning these msg streams, which even in bash piping are intermingled,
          // making it hard to read/use.
          close(tubeerr2[0]);
          close(STDOUT_FILENO);
          dup2(tubeerr2[1], STDERR_FILENO);
          // End of comment
          execv(mpeg2enc, (char *const *)argsmpeg2enc);
          foutput("%s\n", ERR "Runtime failure in mpeg2enc parent process");
          perror("menu2");
          return errno;

        default:
          waitpid(pid2, NULL, 0);
          dup2(tubeerr[0], STDIN_FILENO);

          while (read(tubeerr[0], &c, 1) == 1) foutput("%c", c);
          close(tubeerr[0]);
          close(tubeerr2[1]);
          dup2(tubeerr2[0], STDIN_FILENO);

          while (read(tubeerr2[0], &c, 1) == 1) foutput("%c", c);
          close(tubeerr2[0]);
          if (globals->debugging) foutput("%s\n", INF "Running mplex...");
          run(mplex, argsmplex, WAIT, FORK, globals);
        }
      close(tube[0]);
    }

#else

  char *mpeg2enccl = get_command_line(argsmpeg2enc, globals);
  char *jpeg2yuvcl = get_command_line(argsjpeg2yuv, globals);

// This is unsatisfactory yet will do for porting purposes.

  const char *mplexcl = get_command_line(argsmplex, globals);

  char cml2[strlen(jpeg2yuv) + 1 + strlen(jpeg2yuvcl) + 3 + strlen(mpeg2enc) + 1 + strlen(mpeg2enccl) + 1];

  sprintf(cml2, "%s %s | %s %s", jpeg2yuv, jpeg2yuvcl, mpeg2enc, mpeg2enccl);

  system(win32quote(cml2));

  char cml3[strlen(mplex) + 1 + strlen(mplexcl) + 1];

  sprintf(cml3, "%s %s", mplex, mplexcl);
  system(win32quote(cml3));

  free((char *) jpeg2yuvcl);
  free((char *) mpeg2enccl);
  free((char *) mplexcl);
#endif

  return errno;
}



/* ── MPEG-2 码流自检：制式（NTSC/PAL）与逐行/隔行 ──────────────────────

   见 docs/DVDA-AUTHOR-CHANGES.md。

   dvda-author 原先把视频属性写死成 PAL（0x53），并照抄 mpeg2enc 的
   progressive_sequence=0。这两项都能从**实际产出的码流**里读出来，
   故此处改为探测后如实填写，不再依赖硬编码。 */

typedef struct
{
  int      seq_ok;          /* 找到序列头 */
  int      ext_ok;          /* 找到序列扩展 */
  int      pce_ok;          /* 找到图像编码扩展 */
  int      ntsc;            /* 1 = 525/60 (NTSC)，0 = 625/50 (PAL/SECAM) */
  int      width, height;   /* 序列头里的画面尺寸 */
  int      aspect;          /* aspect_ratio_information */
  int      frame_rate;      /* frame_rate_code */
  uint32_t bit_rate_kbps;   /* 序列头声明的码率 */
  int      progressive;     /* 序列扩展的 progressive_sequence */
  int      progressive_pic; /* 图像编码扩展：整帧 + frame_pred_frame_dct
                               + progressive_frame */
  long     ext_at;          /* 序列扩展起始码在文件内的偏移 */
} dvda_mpeg2_t;

/* 序列头/序列扩展/图像编码扩展都在文件开头，读前 64 KB 足够 */
#define DVDA_PROBE_BYTES 65536

static int dvda_mpeg2_probe(const char *path, dvda_mpeg2_t *p)
{
  unsigned char buf[DVDA_PROBE_BYTES];
  FILE *f;
  size_t n, i;
  long seq = -1, ext = -1, pce = -1;

  memset(p, 0, sizeof *p);
  if (path == NULL) return 0;

  f = fopen(path, "rb");
  if (f == NULL) return 0;
  n = fread(buf, 1, sizeof buf, f);
  fclose(f);
  if (n < 16) return 0;

  for (i = 0; i + 4 <= n; ++i)
    {
      unsigned id;

      if (buf[i] != 0x00 || buf[i + 1] != 0x00 || buf[i + 2] != 0x01)
        continue;

      if (buf[i + 3] == 0xB3 && seq < 0 && i + 12 <= n)
        {
          seq = (long) i;
          continue;
        }

      if (buf[i + 3] != 0xB5 || i + 10 > n) continue;

      id = buf[i + 4] >> 4;
      if (id == 1 && ext < 0)      ext = (long) i;  /* sequence_extension */
      else if (id == 8 && pce < 0) pce = (long) i;  /* picture_coding_ext */

      if (seq >= 0 && ext >= 0 && pce >= 0) break;
    }

  if (seq >= 0)
    {
      const unsigned char *b = buf + seq + 4;
      int by_size = -1;

      p->seq_ok     = 1;
      p->width      = (b[0] << 4) | (b[1] >> 4);
      p->height     = ((b[1] & 0x0F) << 8) | b[2];
      p->aspect     = b[3] >> 4;
      p->frame_rate = b[3] & 0x0F;
      /* bit_rate_value 单位 400 bps */
      p->bit_rate_kbps = (uint32_t)((((unsigned long) b[4] << 10)
                                     | ((unsigned long) b[5] << 2)
                                     | (b[6] >> 6)) * 400UL / 1000UL);

      if (p->height == 480 || p->height == 240)      by_size = 1; /* 525/60 */
      else if (p->height == 576 || p->height == 288) by_size = 0; /* 625/50 */

      /* frame_rate_code: 1=23.976 2=24 3=25 4=29.97 5=30 6=50 7=59.94 8=60 */
      if (p->frame_rate == 3 || p->frame_rate == 6)      p->ntsc = 0;
      else if (p->frame_rate >= 1 && p->frame_rate <= 8) p->ntsc = 1;
      else                                               p->ntsc = (by_size == 1);

      /* 画面高度是显示制式的直接判据；与帧率冲突时以它为准 */
      if (by_size >= 0 && by_size != p->ntsc) p->ntsc = by_size;
    }

  if (ext >= 0)
    {
      p->ext_ok      = 1;
      p->ext_at      = ext;
      p->progressive = (buf[ext + 5] >> 3) & 1;
    }

  if (pce >= 0)
    {
      const unsigned char *b = buf + pce + 4;
      int structure = (b[2] >> 6) & 3;   /* 3 = 整帧 */
      int fpfd      = (b[3] >> 6) & 1;   /* frame_pred_frame_dct */
      int pf        = (b[4] >> 7) & 1;   /* progressive_frame */

      p->pce_ok          = 1;
      p->progressive_pic = (structure == 3 && fpfd && pf);
    }

  return p->seq_ok;
}

/* DVD 视频属性字节：0x43 = 525/60 (NTSC)，0x53 = 625/50 (PAL)。
   取自码流自检；探测不到时退回 --norm 的设置（上游默认 pal）并告警。 */
uint8_t dvda_video_attr_of(const char *path, pic *image, globalData *globals)
{
  dvda_mpeg2_t p;
  int ntsc;

  if (dvda_mpeg2_probe(path, &p) && p.seq_ok)
    {
      ntsc = p.ntsc;
      foutput(MSG_TAG "视频属性自检: %s\n"
              "       %dx%d, %s, 帧率码 %d, 声明 %u kbps, 图像编码 %s\n",
              (path != NULL) ? path : "(none)",
              p.width, p.height, ntsc ? "NTSC" : "PAL",
              p.frame_rate, (unsigned) p.bit_rate_kbps,
              p.pce_ok ? (p.progressive_pic ? "逐行" : "隔行") : "未检出");
    }
  else
    {
      ntsc = (image != NULL && image->norm != NULL
              && strcmp(image->norm, "ntsc") == 0);
      foutput(WAR "视频属性自检: %s 读不到序列头，退回 --norm=%s → %s\n",
              (path != NULL) ? path : "(none)",
              (image != NULL && image->norm != NULL) ? image->norm : "pal",
              ntsc ? "NTSC" : "PAL");
    }

  return (uint8_t)(ntsc ? 0x43 : 0x53);
}

/* 序列扩展的 progressive_sequence：mpeg2enc 无条件写 0，但它产出的每张静图
   实际就是单个逐行 I 帧（图像编码扩展写明 整帧 + frame_pred_frame_dct
   + progressive_frame）。此处按实际内容写回 1；实际是隔行则一个字节都不动。

   必须排在 stat_file_size() 之前（同 patch_still_end_code.py），
   否则 ASVS 的扇区指针与实物不符。 */
static unsigned long dvda_prog_seen = 0, dvda_prog_fixed = 0;

static int dvda_fix_sequence_progressive(const char *path, globalData *globals)
{
  dvda_mpeg2_t p;
  FILE *f;
  unsigned char b;

  ++dvda_prog_seen;

  if (!dvda_mpeg2_probe(path, &p) || !p.ext_ok || !p.pce_ok) return 0;

  if (dvda_prog_seen == 1)   /* 所有静图共用一套编码参数，报一次即可 */
    foutput(MSG_TAG "静图码流自检: %dx%d %s, 序列扩展=%s, 图像编码=%s\n",
            p.width, p.height, p.ntsc ? "NTSC" : "PAL",
            p.progressive ? "逐行" : "隔行",
            p.progressive_pic ? "逐行" : "隔行");

  if (p.progressive || !p.progressive_pic) return 0;   /* 已如实描述 */

  f = fopen(path, "rb+");
  if (f == NULL) return -1;

  if (fseek(f, p.ext_at + 5, SEEK_SET) != 0 || fread(&b, 1, 1, f) != 1)
    {
      fclose(f);
      return -1;
    }

  b = (unsigned char)(b | 0x08);   /* progressive_sequence = 1 */

  if (fseek(f, p.ext_at + 5, SEEK_SET) != 0 || fwrite(&b, 1, 1, f) != 1)
    {
      fclose(f);
      return -1;
    }

  fclose(f);
  ++dvda_prog_fixed;
  return 1;
}

static void dvda_report_sequence_progressive(globalData *globals)
{
  if (dvda_prog_seen == 0) return;

  foutput(MSG_TAG "静图 progressive_sequence 自检: %lu/%lu 张按实际画面写回 1，"
          "其余保持原值\n", dvda_prog_fixed, dvda_prog_seen);
}

int generate_background_mpg(pic *img, globalData *globals)
{
  uint16_t rank = 0;
  char tempfile[CHAR_BUFSIZ * 10];
  char *mp2track;
  errno = 0;

  if (strcmp(img->norm, "ntsc") == 0) norm_y = NTSC_Y; //   x  value is the same as for PAL (720)
  memset(tempfile, '0', sizeof(tempfile));
  sprintf(tempfile, "%s"SEPARATOR"%s", globals->settings.tempdir, "temp.m2v");

  mp2track = (img->action == ANIMATEDVIDEO) ? calloc(CHAR_BUFSIZ, sizeof(char)) : NULL;
  if (mp2track)
    sprintf(mp2track, "%s"SEPARATOR"%s", globals->settings.tempdir, "mp2track.mp2");

  if (img->backgroundmpg == NULL) foutput("%s", MSG_TAG "backgroundmpg will be allocated.\n");

  if (globals->debugging) foutput(INF "Launching mjpegtools to create background mpg with nmenus=%d\n", img->nmenus);

  /* now authoring AUDIO_TS.VOB */
  rank = 0;

  if (img->action == ANIMATEDVIDEO)
    {
      FREE(img->backgroundmpg);
      img->backgroundmpg = calloc(img->nmenus, sizeof(char *));

      while (rank < img->nmenus)
        {
          create_mpg(img, rank, mp2track, tempfile, globals);
          fflush(NULL);
          rank++;
        }
      globals->backgroundmpgsize = img->nmenus;
    }
  rank = 0;

  if (img->action == STILLPICS)
    {
      FREE(img->backgroundmpg);
      img->backgroundmpg = calloc(img->count, sizeof(char *));
      globals->backgroundmpgsize = img->count;
      if (img->backgroundmpg)
        while (rank < img->count)
          {
            create_mpg(img, rank, mp2track, tempfile, globals);
            /* 结束码独占扇区（对齐商业盘）。必须在 stat_file_size 之前做，
               否则 stillpicvobsize 与 ASVS 表里的扇区指针会与实物不符。 */
            dvda_pad_program_end(img->backgroundmpg[rank]);
            /* 导航扇区去掉 mplex 的空 DSI（对齐商业盘）。等长改写，
               故对 stillpicvobsize 无影响。 */
            dvda_rewrite_nav_sector(img->backgroundmpg[rank]);
            /* 序列扩展的 progressive_sequence 按实际画面写回。
               同样必须在 stat_file_size 之前。 */
            dvda_fix_sequence_progressive(img->backgroundmpg[rank], globals);
            img->stillpicvobsize[rank] = (uint32_t)(stat_file_size(img->backgroundmpg[rank]) / 0x800);
            if (img->stillpicvobsize[rank] > 1024) foutput("%s", WAR "Size of slideshow in excess of the 2MB track limit... some stillpics may not be displayed.\n");
            if (rank) cat_file(img->backgroundmpg[rank], img->backgroundmpg[0], globals);
            ++rank;
          }
      dvda_report_sequence_progressive(globals);
      // The first backgroundmpg file is the one that is used to create AUDIO_SV.VOB in amg2.c
    }

  FREE(mp2track)

  FREE(pict)

  if ((globals->debugging) && (!errno))
    foutput("%s\n", INF "MPG background authoring OK.");
  return errno;

}


int launch_spumux(pic *img, globalData *globals)
{
  // hush up spumux on stdout if non-verbose mode selected

  //sprintf(spumuxcommand, "%s%s%s%s%s%s%s", "spumux -v 0 ", globals->spu_xml, " < ", img->backgroundmpg, (globals->debugging)? "" : " 2>null ", " 1> ", img->topmenu);

  if (globals->debugging) foutput("%s\n", INF "Launching spumux to create buttons");
  int menu = 0;


  initialize_binary_paths(CREATE_SPUMUX, globals);


  while (menu < img->nmenus)
    {
      if (globals->debugging) foutput(INF "Creating menu %d from Xml file %s\n", menu + 1, globals->spu_xml[menu]);
      const char *argsspumux[] = {SPUMUX_BASENAME, "-v", "2", globals->spu_xml[menu], NULL};

      // This is to hush up dvdauthor's stdout messages, which interfere out of sequential order with main application stdout messages
      // and anyway could not be logged by  -l;
      // with normal verbosity, stdout messages end up in a tube's dead end, otherwise they are retrieved at the other end on stdout.
      errno = 0;
#ifndef __WIN32__

      int firsttubeerr[2];
      if (pipe(firsttubeerr) == -1)
        perror(ERR "Pipe issue with spumux (firsttubeerr[2])");
      char c;


      switch (fork())
        {

        case -1:
          foutput("%s\n", ERR "Could not launch spumux");
          break;

        case 0:
          close(firsttubeerr[0]);
          dup2(firsttubeerr[1], STDERR_FILENO);

          fprintf(stderr, INF "command line: %s %s %s %s %s", spumux, argsspumux[1], argsspumux[2], argsspumux[3], argsspumux[4]);
          if (freopen(img->backgroundmpg[menu], "rb", stdin) == NULL)
            {
              fprintf(stderr, "%s", ERR "freopen (stdin)\n");
              fprintf(stderr, "img->backgroundmpg[%d]=%s errno=%d: %s", menu, img->backgroundmpg[menu], errno, strerror(errno));
              return errno;
            }
          if (freopen(img->topmenu[menu], "wb", stdout) == NULL)
            {
              fprintf(stderr, "%s\n", ERR "freopen (stdout)");
              fprintf(stderr, "img->backgroundmpg[%d]=%s errno=%d: %s", menu, img->topmenu[menu], errno, strerror(errno));
              return errno;
            }

          execv(spumux, (char *const *) argsspumux);
          return errno;


        default:

          close(firsttubeerr[1]);
          dup2(firsttubeerr[0], STDIN_FILENO);
          wait(NULL);

          while (read(firsttubeerr[0], &c, 1) == 1) foutput("%c", c);

          if (errno)
            {
              foutput("%s\n", ERR "Runtime failure in spumux child process");
              perror(ERR "spumux");
              return errno;
            }
          close(firsttubeerr[0]);
        }

#else

      char *s = get_command_line(argsspumux, globals);
      uint16_t size = strlen(s);
      char cml[strlen(spumux) + 1 + size + 3 + strlen(img->backgroundmpg[menu]) + 2 + 3 + strlen(img->topmenu[menu]) + 2 + 1];
      sprintf(cml, "%s %s < %s > %s", spumux, s, win32quote(img->backgroundmpg[menu]), win32quote(img->topmenu[menu]));
      system(win32quote(cml));
      free((char *) s);

#endif



      menu++;
    }


  return errno;
}



int launch_dvdauthor(globalData *globals)
{

  initialize_binary_paths(2, globals);

  errno = 0;

  if (globals->debugging) foutput("%s\n", INF "Launching dvdauthor to add virtual machine commands to top menu");

  const char *args[] = {dvdauthor, "-o", globals->settings.outdir, "-x", globals->xml, NULL};

  run(dvdauthor, (const char **)args, WAIT, FORK, globals);

#ifndef _WIN32
  sync();
#endif

  return errno;
}



uint16_t x(uint8_t group, uint8_t ngroups)
{
  return  Min(norm_x, (20 + ((norm_x - 20) * group) / ngroups + EMPIRICAL_X_SHIFT));
}

// text is within button (i,j) with left-justified spacing of 10 pixels wrt left border
uint16_t y(uint8_t track, uint8_t maxnumtracks)
{

  int labelheight = (norm_y - 56 - 40 - maxnumtracks * 12) / maxnumtracks;
  int y_top = 56 + track * (labelheight + 12) + labelheight / 2 ;

  return y_top;

}


/* 把「同一段文字，偏移 (TEXT_SHADOW_DX, TEXT_SHADOW_DY)，颜色 color」
   追加到 dest（一条给 mogrify 的命令串）。

   为什么要分层画：子画面整幅只有 4 个调色板项，而 spumux 的 s->pal[]
   是从**图像层各像素自己的颜色**取的 —— 同一层里再现第二种颜色要么被
   合并、要么直接报 "Too many colors in base picture"。
   所以「白字 + 阴影」只能是两层两个颜色：
     · 图像层 impic ← 未选中时显示 → 阴影用 bgcolor_pic（黑）
     · 高亮层 hlpic ← 选中时显示   → 阴影用 highlightcolor_pic（红）
   详见 docs/TROUBLESHOOTING.md 第 23 节。

   ⚠️ 追加的片段**末尾必须留空格**：调用方随后 strcat 的是输出文件路径，
   少了空格两者会粘成一个参数，mogrify 就把结果打到 stdout 并返回 0
   （静默什么都不画，见第 22 节）。 */
static void append_shadow(char *dest, pic *img, const char *text,
                          uint16_t x, uint16_t y, int pointsize,
                          const char *color)
{
  char *q = quote((char *) color);
  char str[2 * CHAR_BUFSIZ];

  /* ⚠️ `-stroke none` 不能省！`mogrify_thumb()` 会把 `-stroke "rgb(红色)"`
     留在参数流里（它是描边矩形），而这个设置**会一直生效到被改掉为止**。
     `-draw "text"` 是同时用 `-fill` 和 `-stroke` 画的（IM 对文本也描边），
     于是后面的文字全被 6px 宽的红色描边糊住 —— 实测整整排查了一轮：
     图上看到的「红字」其实是「黑字 + 红描边」。 */
  snprintf(str, sizeof(str),
           " -stroke none -fill \"rgb(%s)\" -font %s -pointsize %d"
           " -draw \"text %u,%u '%s'\" ",
           q, img->textfont, pointsize,
           (unsigned) (x + TEXT_SHADOW_DX), (unsigned) (y + TEXT_SHADOW_DY),
           text);
  free(q);
  strcat(dest, str);
}


int prepare_overlay_img(char *text, int8_t group, pic *img, char *command, char *command2, int menu, char *albumcolor, globalData *globals)
{  initialize_binary_paths(3, globals);

  int size = strlen(globals->settings.tempdir) + 11;
  char picture_save[size];
  sprintf(picture_save, "%s"SEPARATOR"%s", globals->settings.tempdir, "svpic.png");

  if (file_exists(picture_save)) unlink(picture_save);
  errno = 0;
  change_directory(globals->settings.datadir, globals);
  if (img->blankscreen)
    copy_file(img->blankscreen, picture_save, globals);
  change_directory(globals->settings.workdir, globals);

  if ((group == -1) && (text)) // album text
    {
      uint16_t x0 = EVEN(x((group > 0 ? group : 0), img->ncolumns)) ;
      char *q = quote(picture_save);
      snprintf(command, 2 * CHAR_BUFSIZ, "%s %s %s %s %s \"rgb(%s)\" %s %s %s %d %s %s %d%c%d %c%s%s %s", mogrify,
               "+antialias", "-stroke", "none", "-fill", albumcolor, "-font", img->textfont, "-pointsize", DEFAULT_POINTSIZE,
               "-draw", " \"text ", x0, ',', ALBUM_TEXT_Y0,  '\'', text, "\'\"", q);
      free(q);
      if (globals->debugging) foutput("%s%s\n", INF "Launching mogrify (title) with command line: ", command);
      if (system(win32quote(command)) == -1) EXIT_ON_RUNTIME_ERROR_VERBOSE("System command failed")
        fflush(NULL);
    }

  if ((img->imagepic[menu] == NULL) || (img->highlightpic[menu] == NULL) || (img->imagepic[menu] == NULL))
    {
      EXIT_ON_RUNTIME_ERROR_VERBOSE("pic pathnames");
      return -1;
    }


  copy_file(picture_save, img->imagepic[menu], globals);
  if (globals->debugging) foutput(INF "copying %s to %s for menu #%d\n", picture_save, img->imagepic[menu], menu);
  copy_file(picture_save, img->highlightpic[menu], globals);
  copy_file(picture_save, img->selectpic[menu], globals);

  errno = 0;
  snprintf(command, CHAR_BUFSIZ, "%s %s", mogrify, "+antialias");
  snprintf(command2, CHAR_BUFSIZ, "%s %s", mogrify, "+antialias");

  /* ---- 标题阴影 ----
     标题（光盘标题）烘在 `svpic.png` 里、被复制到**三层**，所以它在三层
     的像素一模一样 —— 想在它旁边加个**另一种颜色**的阴影，只能额外往
     高亮层画一遍（同一层不能有两种颜色，见 mogrify_img 的说明）。

     ⚠️ 必须**追加到 `command`（= command1，作用于 hlpic）**，
     不能直接在这里改 `img->highlightpic[menu]` 这个文件：
     `generate_menu_pics()` 紧接着就会
         copy_file(img->imagepic[menu], img->highlightpic[menu], globals);
     把 hlpic 用 impic 整个覆盖掉（实测标题阴影就是这样消失的）。

     也要注意放在上面那次 `snprintf(command, ...)` **之后** —— 那句是重置。 */
  if ((group == -1) && text && img->highlightcolor_pic)
    {
      uint16_t xs = EVEN(x((group > 0 ? group : 0), img->ncolumns));

      /* 描边画两遍、分两层（见 menu.h 的 TEXT_SHADOW_DX 说明）：
           command2 → impic  黑 = 未选中
           command  → hlpic  红 = 选中   （command 在这里就是 command1） */
      append_shadow(command2, img, text, xs, ALBUM_TEXT_Y0, DEFAULT_POINTSIZE,
                    img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
      append_shadow(command, img, text, xs, ALBUM_TEXT_Y0, DEFAULT_POINTSIZE,
                    img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
    }

  return errno;
}


int mogrify_img(char *text, int8_t group, int8_t track, pic *img, uint8_t maxnumtracks, char *command, char *command2,  int8_t offset, char *textcolor)
{
  errno = 0;
  uint16_t x0, y0;

  x0 = EVEN(x((group > 0) ? group : 0, img->ncolumns)) ;
  y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));

// In automatic mode, we underline presupposing -font Courier with approx 1 letter of font 10 =6 pix in width, otherwise sepcify fontwidth

  char *str, *str2;
  str = (char *) calloc(10 * CHAR_BUFSIZ, 1);
  if (str == NULL) perror(ERR "mogrify, string");
  str2 = (char *) calloc(10 * CHAR_BUFSIZ, 1);
  if (str2 == NULL) perror(ERR "mogrify, string 2");

// +antialias is crucial for dvdauthor, otherwise button masks will not be properly detected.
  int16_t deltax0 = 0, deltax1 = 0, deltay0 = 0, deltay1 = 0;

  if (img->highlightformat == UNDERLINE)
    {
      deltax0 = 0;
      deltay0 = (img->pointsize < 12) ? 2 : 4;
      deltax1 = EVEN((img->fontwidth * img->pointsize * strlen(text)) / 10);
      deltay1 = deltay0 + 2;
    }
  else if (img->highlightformat == PRECEDE)
    {
      deltax0 = -12;
      deltay0 = -8;
      deltax1 = -4;
      deltay1 = 0;
    }
  if (img->highlightformat == BUTTON)
    {

      deltax0 = -4;
      deltay0 = -4 - EVEN((img->fontwidth * img->pointsize) / 5);
      deltax1 = EVEN((img->fontwidth * img->pointsize * strlen(text)) / 10) + 4;
      deltay1 = 4;
    }


  /* ---- 选中指示：行左侧的小三角箭头（画进 command = hlpic）----
     取代原来的「下划线／按钮框」矩形 —— 下划线是横贯文字的，与字身和
     描边必然重叠，选中态就没法用另一种颜色表示（调色板只有 4 项）。
     箭头放在文字左边，与文字完全不相交，所以能独立占一个调色板项。 */
  if (track != -1)
    {
      int pts = (int) floor(img->pointsize * (1 - (track == -1) * 0.2));
      int spy = y0 - pts / 2;                       /* 文字的垂直中心 */
      int ax  = (int) x0 - TEXT_ARROW_GAP - TEXT_ARROW_W;
      char *q = quote(img->highlightcolor_pic);

      snprintf(str, 10 * CHAR_BUFSIZ,
               " %s %s %s \"rgb(%s)\" %s %s %d,%d %d,%d %d,%d%s ",
               "-stroke", "none", "-fill", q, "-draw", " \"polygon ",
               ax, spy,
               ax + TEXT_ARROW_W, spy - TEXT_ARROW_H,
               ax + TEXT_ARROW_W, spy + TEXT_ARROW_H, "\"");
      free(q);
    }

  strcat(command, str);

  /* ---- 文字描边：两层各画一遍（颜色不同）----
     command2 → impic 用黑（**未选中**时看到）
     command  → hlpic 用红（**选中**时看到）
     spumux 把「选中态」的颜色取自 hlt 层（subgen.c 写 ST_COLI 时用
     `s->hlt.pal`），所以红必须画在 hlpic 上。详见 menu.h 的 TEXT_SHADOW_DX。 */
  {
    int pts = (int) floor(img->pointsize * (1 - (track == -1) * 0.2));
    append_shadow(command2, img, text, x0, y0, pts,
                  img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
    append_shadow(command, img, text, x0, y0, pts,
                  img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
  }

  snprintf(str2, 10 * CHAR_BUFSIZ, " %s %s %s \"rgb(%s)\" %s %s %s %d %s %s %d%c%d %s%s%s ",
           "-stroke", "none",
           "-fill", textcolor, "-font", img->textfont, "-pointsize", (int) floor(img->pointsize * (1 - (track == -1) * 0.2)),
           "-draw", " \"text ", x0, ',', y0, "\'", text, "\'\"");

  strcat(command2, str2);

  /* 文字**无条件**也画进 command（= hlpic）。

     上游只在 `highlightformat == BUTTON` 时才这样（注释说「因为文字会被
     覆盖」）。我们不这样不行：描边也是画在 hlpic 上的，而描边是同一段
     文字偏移 2px —— 它与字身的笔画必然重叠。若 hlpic 里**只有描边没有
     字身**，重叠处的三色组合会多出一种：
         img=白(字身) hlt=红(描边) sel=白
     加上
         img=透明 hlt=透明 sel=透明   （背景）
         img=白     hlt=透明 sel=白   （字身）
         img=黑     hlt=红   sel=黑   （描边）
         img=透明   hlt=红   sel=透明 （下划线）
     共 **5 种**，而 spumux 每个按钮的调色板只有 4 项
     （subgen-image.c 的 checkcolor：`if (p->numpal == 4) return false;`）
     → `pickbuttongroups()` 全部失败 → `ERR: Cannot pick button masks`
     → `assert(useimg)` 中止，菜单全丢（实测 85 次报错、VOB 少了 15 万字节）。

     把字身也画进 hlpic 后，重叠处的 hlt 就是白（字身覆盖了红描边），
     组合正好回到 4 种。屏幕上「选中」时看的是 hlpic：白字 + 红描边 + 红下划线。 */
  strcat(command, str2);


  if (errno) perror(ERR "mogrify");
  FREE(str)
  FREE(str2)
  return errno;
}


void compute_pointsize(pic *img, uint16_t maxtracklength, uint8_t maxnumtracks, globalData *globals)
{
  if (img->pointsize == 0)
    {
      uint8_t wide = (((norm_x - 4 * EMPIRICAL_X_SHIFT - 2 * 20) - (img->ncolumns - 1) * 20) * 10) / (img->ncolumns * img->fontwidth * maxtracklength);
      uint8_t delta = (y(1, maxnumtracks) - y(0, maxnumtracks)) * 3 / 4;
      uint8_t height = (uint8_t)((delta * 5) / img->fontwidth);

      img->pointsize = Min(wide, height);
    }
  img->pointsize = MAX(MIN_POINTSIZE, img->pointsize);
  img->pointsize = Min(img->pointsize, MAX_POINTSIZE);
}

/* The following function tests presence of characters with pixels likely to intersect underlining motifs thereby causing spumux to crash
   and to avoid this switches ----highlightformat to -1 (little squares) */

void test_underline(char *text, pic *img, globalData *globals)
{

  int j, s = strlen(text);

  for (j = 0; j < s; j++)
    if ((text[j] == 'g') || (text[j] == 'j') || (text[j] == 'p') || (text[j] == 'q') || (text[j] == 'y'))
      {
        if (globals->debugging)
          foutput(INF "Switching to little squares rather than underlining motifs for highlight\n       as %c could cut underlines\n", text[j]);
        img->highlightformat = -1;
      }

}


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

  uint8_t pc[256];                          /* 每页条目数（nmenus 是 uint8_t） */
  int np = 0, total = 0;                    /* total = 仅**专辑页**的曲目数 */
  char *p = eq + 1;

  /* 前 img->index_pages 段是「专辑索引页」：段内的条目是**专辑格子**，
     不是曲目，所以不计入 total（total 要与音频总轨数对上）。 */
  const int nidx = (int) img->index_pages;

  while (*p && (np < 256))
    {
      char *colon = strchr(p, ':');
      char *segend = colon ? colon : (p + strlen(p));

      /* 段格式：小标题'='条目1,条目2,...；有 '=' 且后面非空才算有条目 */
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
      pc[np] = (uint8_t) n;
      if (np >= nidx) total += n;          /* 索引页的格子不计入曲目总数 */
      ++np;

      if (!colon) break;
      p = colon + 1;
    }

  int total_audio = 0;
  for (int g = 0; g < ngroups; ++g) total_audio += ntracks[g];


  if ((np == 0) || (np != (int) img->nmenus) || (total != total_audio)
      || (nidx > np))
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

  /* 索引页：格子数 = 该段的条目数；group/t0 无意义（置 0）。
     xml.c 对这几页输出 `jump menu (nidx + p*INDEX_PER_PAGE + k + 1)`。 */
  for (int q = 0; q < nidx; ++q)
    {
      pnt[q] = pc[q];
      pgrp[q] = 1;
      pt0[q] = 0;
    }

  int g = 0, acc = 0, ok = 1;
  for (int q = nidx; q < np; ++q)
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

/* 专辑索引页：给第 cell 个缩略图格子画**按钮区域**。

   画在 highlight 层（command1），用**描边**矩形而不是填充矩形 ——
   填色会把缩略图整块盖住；描边只勾一个框，播放器按高亮状态给框上色。
   颜色仍用 img->highlightcolor_pic（与曲目下划线同一套调色板机制）。

   坐标与 menu_assets.py 的 make_index_page() 严格一致，见 INDEX_* 常量。 */
static int mogrify_thumb(uint8_t cell, pic *img, char *command,
                         globalData *globals)
{
  uint16_t x0 = (uint16_t) ((cell % INDEX_COLS) * INDEX_CELL_W + INDEX_INSET);
  uint16_t y0 = (uint16_t) (INDEX_TOP + (cell / INDEX_COLS) * INDEX_CELL_H
                            + INDEX_INSET);
  uint16_t x1 = (uint16_t) (x0 + INDEX_CELL_W - 2 * INDEX_INSET);
  uint16_t y1 = (uint16_t) (y0 + INDEX_CELL_H - 2 * INDEX_INSET);
  char *q = quote(img->highlightcolor_pic);
  char str[512];

  /* 末尾**必须有空格**：调用方接着 strcat 的是输出文件路径，少了这个
     空格两者会粘成一个参数

       … -draw "rectangle 545,293 715,427""/path/hlpic0.png"

     → mogrify 拿不到输出文件，把结果打到 stdout、**返回 0**，
       于是静默什么都不画（查了很久）。mogrify_img() 的格式串末尾
       也有这个空格，跟它保持一致。 */
  snprintf(str, sizeof(str),
           " -fill none -stroke \"rgb(%s)\" -strokewidth 6"
           " -draw \"rectangle %u,%u %u,%u\" ",
           q, x0, y0, x1, y1);
  free(q);
  strcat(command, str);
  return errno;
}


/* 专辑索引页的翻页箭头：**绝对坐标**绘制。

   不能走 mogrify_img()：它的 y 由 y(track, maxnumtracks) 按「行」算，
   而索引页的 maxbuttons 是格子数 —— 算出来的位置跑到画面外。
   矩形必须与 xml.c 输出的箭头按钮一致（INDEX_*_X0/Y0/W）。 */
static int mogrify_arrow_abs(const char *text, uint16_t x0, pic *img,
                             char *command2, char *command1,
                             const char *color)
{
  char str[512];
  char *q = quote((char *) color);
  char *qh = quote(img->highlightcolor_pic);
  int tw = (img->fontwidth * img->pointsize * (int) strlen(text)) / 10;
  uint16_t cx = (uint16_t) (x0 + ((INDEX_ARROW_W > tw)
                                  ? (INDEX_ARROW_W - tw) / 2 : 0));
  uint16_t cy = (uint16_t) (INDEX_ARROW_Y0
                            + (INDEX_ARROW_Y1 - INDEX_ARROW_Y0) / 2
                            + img->pointsize / 2);

  /* 末尾空格的原因同 mogrify_thumb() —— 后面紧跟的是输出文件路径。 */
  snprintf(str, sizeof(str),
           " -stroke none -fill \"rgb(%s)\" -font %s -pointsize %d"
           " -draw \"text %u,%u '%s'\" ",
           q, img->textfont, (int) img->pointsize,
           (unsigned) cx, (unsigned) cy, text);
  strcat(command2, str);

  /* 描边两层各一遍：command2 = impic（黑，未选中）、
     command1 = hlpic（红，选中）。见 menu.h 的 TEXT_SHADOW_DX。 */
  append_shadow(command2, img, text, cx, cy, (int) img->pointsize,
                img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
  append_shadow(command1, img, text, cx, cy, (int) img->pointsize,
                img->bgcolor_pic ? img->bgcolor_pic : DEFAULT_BGCOLOR_PIC);
  free(q);
  free(qh);
  return errno;
}


/* ===========================================================================
   一级菜单（专辑索引页）的画面 —— 由 C 现画
   ===========================================================================

   以前这一页的画面是外部脚本用 ImageMagick 拼好、通过 `--background` 传进来
   的。那样几何常量存在两份（menu.h 与脚本各一份），改一处忘另一处就会
   「点到的不是想选的那张」。现在整块搬进 C：素材（封面路径 + 专辑名）由
   命令行给，画面在这里拼。

   画面 = 三层背景 + 每格 [封面 + 专辑名] + 缩略图描边：

     背景   对角渐变（左上亮、右下暗，给画面一个方向感）
            + 与 4x3 格子对齐的细网格（让背景和版式有关系）
            + 径向暗角（压住四角，视线收拢到中间）
     格子   封面缩到 INDEX_THUMB 见方居中；下方 INDEX_LABEL_H 放专辑名
     名称   白字 + `caption:` 自动换行（字号按文字宽度估，见下）
     描边   每个格子描一圈深灰，把封面从背景里「托」出来

   ⚠️ 三条踩过的坑都体现在下面的命令里：

   1. **`-repage` 必须写在 `( )` 内部**。它是**算子**，不加括号会对列表里
      每一张生效（包括开头那张底色），结果底色被挪到最后一格、画布露出
      `-flatten` 的默认白底 —— 整页只剩右下角一张图。
   2. **描边要单独一遍画在 `-flatten` 之后**。`-draw` 会作用到列表里的
      每一张，拼在一起画时边框会被后续叠加顺序盖掉、或落到某张小图自己的
      坐标系里（实测取到纯黑）。
   3. **背景必须是彩色**。`-flatten` 的输出色彩空间看**第一张图**，
      背景是灰度时封面的彩色会被全部丢掉（整页变黑白，实测 `%k` 只剩 256）。
      所以显式 `-colorspace sRGB -type TrueColor`。
   =========================================================================== */

/* 命令串拼接器：长度动态增长，参数按需要加 shell 引号。
   固定 2 KB 装不下 —— 12 个格子各带一条封面路径和一段专辑名，
   实测单页命令串 8~12 KB。 */
struct cmdstr
{
  char  *buf;
  size_t cap;
  size_t len;
};

static void cs_grow(struct cmdstr *c, size_t extra)
{
  if (c->len + extra + 1 <= c->cap) return;
  size_t cap = c->cap ? c->cap : 4096;
  while (cap < c->len + extra + 1) cap *= 2;
  char *b = realloc(c->buf, cap);
  if (!b)
    {
      perror(ERR "Out of memory building the album index page command line");
      exit(EXIT_FAILURE);
    }
  c->buf = b;
  c->cap = cap;
}

static void cs_raw(struct cmdstr *c, const char *s)
{
  size_t n = strlen(s);
  cs_grow(c, n);
  memcpy(c->buf + c->len, s, n + 1);
  c->len += n;
}

static void cs_fmt(struct cmdstr *c, const char *fmt, ...)
{
  char tmp[1024];
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(tmp, sizeof(tmp), fmt, ap);
  va_end(ap);
  cs_raw(c, tmp);
}

/* 加**一个**参数，自动加 shell 引号（路径和专辑名都可能带空格/中文）。 */
static void cs_arg(struct cmdstr *c, const char *s)
{
  char *q = quote(s);
  cs_raw(c, " ");
  cs_raw(c, q);
  free(q);
}

/* 加一个已经写好的开关（如 `-flatten`），不加引号。 */
static void cs_opt(struct cmdstr *c, const char *s)
{
  cs_raw(c, " ");
  cs_raw(c, s);
}

/* 专辑名的「宽度」，单位 = 0.1 个全角字宽。

   为什么不复用全局的 `img->fontwidth`：那是按**全部**文字（曲名 + 专辑名）
   算出来的平均值，用它估单个专辑名会偏。这里逐字符数更准：
   全角（汉字/假名/韩文）= 10，ASCII ≈ 5（半个全角宽）。
   UTF-8 首字节即可判断：<0x80 单字节、0xC0 段双字节、0xE0 段三字节。 */
static int index_label_units(const char *s)
{
  int u = 0;
  const unsigned char *p = (const unsigned char *) s;

  while (*p)
    {
      if (*p < 0x80)                { u += 5;  p += 1; }
      else if ((*p & 0xE0) == 0xC0) { u += 10; p += 2; }
      else if ((*p & 0xF0) == 0xE0) { u += 10; p += 3; }
      else                          { u += 10; p += 4; }
    }
  return u ? u : 10;
}

/* 专辑名的字号：让「units/10 个全角宽 × 字号」不超 INDEX_LABEL_W。
   `caption:` 自己也会换行，所以估偏了只是行数变多、字被缩小居中，
   不会溢出格子。 */
static int index_label_pointsize(const char *label)
{
  int size = (10 * INDEX_LABEL_W) / index_label_units(label);

  if (size > INDEX_LABEL_FONT_MAX) size = INDEX_LABEL_FONT_MAX;
  if (size < INDEX_LABEL_FONT_MIN) size = INDEX_LABEL_FONT_MIN;
  return size;
}

/* 取第 `page` 个索引页第 `cell` 格的专辑名（malloc，调用方 free）。

   `--screentext` 的格式是 `光盘标题=段1:段2:...`，索引页的段形如
   `选择专辑=名字1,名字2,...`（那段的「标题」不会显示，只是让段格式合法）。

   为什么在这里自己解析：`generate_menu_pics()` 里有完整的 screentext 解析，
   但它在**后台**晚于背景图生成 —— 而背景图正是这里要写的。 */
static char *index_label_of(pic *img, int page, int cell)
{
  if (!img->screentextchain) return NULL;

  const char *p = strchr(img->screentextchain, '=');   /* 跳过光盘标题 */
  if (!p) return NULL;
  ++p;

  for (int s = 0; s < page; ++s)                        /* 走到第 page 段 */
    {
      p = strchr(p, ':');
      if (!p) return NULL;
      ++p;
    }

  const char *segend = strchr(p, ':');
  const char *seq    = strchr(p, '=');                  /* 段内 `标签=列表` */
  if (seq && (!segend || seq < segend)) p = seq + 1;

  for (int k = 0; k < cell; ++k)                        /* 走到第 cell 个 */
    {
      const char *comma = strchr(p, ',');
      if (!comma || (segend && comma > segend)) return NULL;
      p = comma + 1;
    }

  const char *end = p;
  while (*end && *end != ',' && (!segend || end < segend)) ++end;
  if (end == p) return NULL;

  size_t n = (size_t) (end - p);
  char *out = calloc(n + 1, 1);
  if (out) memcpy(out, p, n);
  return out;
}

/* 执行拼好的 convert 命令。

   ⚠️ 必须检查**退出码**，不能只判 `system(...) == -1` —— 后者只在 fork
   失败时为真，命令本身失败（参数错、文件读不到）时返回的是
   `状态 << 8`，会被当成成功。上游到处只判 -1，所以我们以前出过
   「convert 静默失败、菜单缺图但没人报错」的情况。 */
static int run_convert(struct cmdstr *courier, globalData *globals)
{
  if (globals->debugging)
    foutput(INF "Launching convert (album index page) with command line: %s\n",
            courier->buf);

  int rc = system(win32quote(courier->buf));
  if (rc == -1 || !WIFEXITED(rc) || WEXITSTATUS(rc) != 0)
    {
      fprintf(stderr, "%s", ERR "convert failed while authoring an album "
              "index page\n");
      fprintf(stderr, "       command: %s\n", courier->buf);
      fprintf(stderr, "       exit status: %d\n", rc);
      clean_exit(EXIT_FAILURE, globals);
    }
  return 0;
}

int dvda_make_index_pages(pic *img, globalData *globals)
{
  if (!img->index_pages) return 0;
  if (!img->page_ntracks)
    EXIT_ON_RUNTIME_ERROR_VERBOSE("Index pages need the per-page layout"
                                  " (page_ntracks)")

  char *convert = NULL;
  convert = create_binary_path(convert, CONVERT, SEPARATOR CONVERT_BASENAME,
                               globals);
  if (!convert) EXIT_ON_RUNTIME_ERROR_VERBOSE("No convert binary")

  for (int page = 0; page < (int) img->index_pages; ++page)
    {
      int ncells = (int) img->page_ntracks[page];
      if (ncells > INDEX_PER_PAGE) ncells = INDEX_PER_PAGE;
      if (ncells <= 0) continue;

      char out[CHAR_BUFSIZ];
      snprintf(out, sizeof(out), "%s%s%s%d%s", globals->settings.tempdir,
               SEPARATOR, "bgpic", page, ".jpg");

      struct cmdstr c = { 0 };
      cs_arg(&c, convert);
      cs_opt(&c, "+antialias");
      cs_opt(&c, "-size");
      cs_fmt(&c, " %dx%d", norm_x, norm_y);

      /* ---- 1) 背景：渐变 + 网格 + 暗角，整体包成一个图层 ---- */
      cs_arg(&c, "(");
      /* ⚠️ 必须走 cs_arg（加引号）：`rgb(...)` 里的括号是 shell 的语法
         字符，不加引号直接就是 "syntax error near unexpected token `('"。
         实测就是这么炸的 —— 而 system() 的返回值当时没检查，只看到
         「背景图不存在」。 */
      {
        char xc[128];
        snprintf(xc, sizeof(xc), "xc:%s", INDEX_BG_FROM);
        cs_arg(&c, xc);
      }
      cs_arg(&c, "-sparse-color");
      cs_arg(&c, "bilinear");
      {
        char stops[256];
        snprintf(stops, sizeof(stops), "0,0 %s %d,%d %s",
                 INDEX_BG_FROM, norm_x - 1, norm_y - 1, INDEX_BG_TO);
        cs_arg(&c, stops);
      }
      cs_arg(&c, "-stroke");
      cs_arg(&c, "none");
      cs_arg(&c, "-fill");
      cs_arg(&c, INDEX_BG_GRID);
      {
        char grid[1024];
        int  g = 0;
        for (int col = 1; col < INDEX_COLS; ++col)
          g += snprintf(grid + g, sizeof(grid) - g, "rectangle %d,0 %d,%d ",
                        col * INDEX_CELL_W, col * INDEX_CELL_W, norm_y - 1);
        for (int row = 0; row <= INDEX_ROWS; ++row)
          {
            int y = INDEX_TOP + row * INDEX_CELL_H;
            g += snprintf(grid + g, sizeof(grid) - g, "rectangle 0,%d %d,%d ",
                          y, norm_x - 1, y);
          }
        cs_arg(&c, "-draw");
        cs_arg(&c, grid);
      }
      cs_arg(&c, "(");
      cs_opt(&c, "-size");
      cs_fmt(&c, " %dx%d", norm_x, norm_y);
      {
        char vig[64];
        snprintf(vig, sizeof(vig), "radial-gradient:#ffffff-%s",
                 INDEX_BG_VIGNETTE);
        cs_arg(&c, vig);
      }
      cs_arg(&c, ")");
      cs_opt(&c, "-compose");
      cs_opt(&c, "multiply");
      cs_opt(&c, "-composite");
      /* ⚠️ `-compose` 与 `-stroke`/`-fill` 一样是**粘住的**：不复位的话
         后面的 `-flatten` 也会按 multiply 合成 —— 整页被压暗、封面与背景
         相乘（实测整页均值从 ~60 掉到 44、封面从 ~116 掉到 ~40）。 */
      cs_opt(&c, "-compose");
      cs_opt(&c, "over");
      cs_opt(&c, "-colorspace");
      cs_opt(&c, "sRGB");
      cs_opt(&c, "-type");
      cs_opt(&c, "TrueColor");
      cs_arg(&c, ")");

      /* ---- 2) 每格：封面 + 专辑名 ----
         ⚠️ 封面的下标是**全局连续**的：`--index-covers` 是扁平列表，
         顺序为「页序 × 格子序」。每页从 0 重来的话第 2 页会重复第 1 页
         的封面（实测两页画面完全相同）。 */
      int coverbase = page * INDEX_PER_PAGE;
      for (int cell = 0; cell < ncells; ++cell)
        {
          int ox = (cell % INDEX_COLS) * INDEX_CELL_W;
          int oy = INDEX_TOP + (cell / INDEX_COLS) * INDEX_CELL_H;
          int tx = ox + INDEX_INSET + (INDEX_LABEL_W - INDEX_THUMB) / 2;
          int ty = oy + INDEX_INSET;

          int cidx = coverbase + cell;
          if (img->indexcovers && cidx < img->indexcoverssize
              && img->indexcovers[cidx])
            {
              cs_arg(&c, "(");
              cs_arg(&c, img->indexcovers[cidx]);
              cs_arg(&c, "-resize");
              cs_fmt(&c, " %dx%d^", INDEX_THUMB, INDEX_THUMB);
              cs_opt(&c, "-gravity");
              cs_opt(&c, "center");
              cs_arg(&c, "-extent");
              cs_fmt(&c, " %dx%d", INDEX_THUMB, INDEX_THUMB);
              cs_arg(&c, "-repage");           /* ← 必须在 `)` 内！ */
              cs_fmt(&c, " +%d+%d", tx, ty);
              cs_arg(&c, ")");
            }

          char *label = index_label_of(img, page, cell);
          if (label)
            {
              char cap[CHAR_BUFSIZ];
              int  lx = ox + INDEX_INSET;
              int  ly = oy + INDEX_INSET + INDEX_THUMB + INDEX_THUMB_GAP;
              snprintf(cap, sizeof(cap), "caption:%s", label);

              cs_arg(&c, "(");
              cs_opt(&c, "-background");
              cs_opt(&c, "none");
              cs_opt(&c, "-stroke");
              cs_opt(&c, "none");              /* mogrify_thumb 会留下 -stroke */
              cs_opt(&c, "-fill");
              cs_opt(&c, "white");
              cs_opt(&c, "-font");
              cs_arg(&c, img->textfont);
              cs_opt(&c, "-pointsize");
              cs_fmt(&c, " %d", index_label_pointsize(label));
              cs_opt(&c, "-size");
              cs_fmt(&c, " %dx%d", INDEX_LABEL_W, INDEX_LABEL_H);
              cs_opt(&c, "-gravity");
              cs_opt(&c, "center");
              cs_arg(&c, cap);
              cs_arg(&c, "-repage");           /* ← 必须在 `)` 内！ */
              cs_fmt(&c, " +%d+%d", lx, ly);
              cs_arg(&c, ")");
              free(label);
            }
        }

      /* ---- 3) 合成 + 缩略图描边（必须单独一遍，见文件头注释）---- */
      cs_opt(&c, "-flatten");
      cs_arg(&c, "-stroke");
      cs_arg(&c, INDEX_BORDER_COLOR);
      cs_opt(&c, "-strokewidth");
      cs_fmt(&c, " %d", INDEX_BORDER_W);
      cs_arg(&c, "-fill");
      cs_arg(&c, "none");
      {
        char rect[2048];
        int  r = 0;
        for (int cell = 0; cell < ncells; ++cell)
          {
            int ox = (cell % INDEX_COLS) * INDEX_CELL_W;
            int oy = INDEX_TOP + (cell / INDEX_COLS) * INDEX_CELL_H;
            int tx = ox + INDEX_INSET + (INDEX_LABEL_W - INDEX_THUMB) / 2;
            int ty = oy + INDEX_INSET;
            r += snprintf(rect + r, sizeof(rect) - r,
                          "rectangle %d,%d %d,%d ", tx, ty,
                          tx + INDEX_THUMB - 1, ty + INDEX_THUMB - 1);
          }
        cs_arg(&c, "-draw");
        cs_arg(&c, rect);
      }
      cs_opt(&c, "-colorspace");
      cs_opt(&c, "sRGB");
      cs_opt(&c, "-type");
      cs_opt(&c, "TrueColor");
      cs_opt(&c, "-quality");
      cs_opt(&c, "92");
      cs_arg(&c, out);

      run_convert(&c, globals);
      free(c.buf);
    }

  free(convert);
  return 0;
}


int generate_menu_pics(command_t *command, pic *img, uint8_t ngroups, uint8_t *ntracks,  globalData *globals)
{
  if ((!img->refresh) || (!img->nmenus))   return 0;
  errno = 0;
  FILE *f;
  uint8_t group = 0, track = 0, buttons = 0, menu = 0, arrowbuttons = 1, groupcount = 0, menubuttons;
  uint16_t maxtracklength = 0;
  int dim = 0, k, j;
  char **grouparray = NULL, **basemotif = NULL, *albumtext = NULL, * **tracktext = NULL, * **grouptext = NULL;

  if (!img->hierarchical)
    {
      /* 每页容量：总数按页数均分（向上取整），再受屏幕上限约束。
         原式先截 32 再除页数，导致所有页合计恒 <=32 个按钮。 */
      img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2,
                            (totntracks + img->nmenus - 1) / img->nmenus);
      img->resbuttons = 0;
    }

  if (img->screentextchain)
    {
      /* ⚠️ 这里**不能**用 fn_strtok(..., count, cutloop, ...)：
         cutloop 的 `static int32_t loop` 会在调用之间**泄漏** ——
         `++loop; if (count > loop) return 1; else { loop = 0; return 0; }`
         当某次调用的子串数**少于** count 时循环自然结束，loop **不归零**；
         下一次调用就会在**第一个子串上 break**，而 fn_strtok 在 break 时
         把 array[0] 置成**哨兵 NULL**。
         实测后果：`--screentext` 的段0 只有一个 '='（如
         `ALBUM==曲1,曲2`）→ 子串数 1 < count=2 → loop 留下 1
         → 段1 的 grouptext[1][0] 变成 NULL
         → 画组标题时 mogrify_img(strlen(NULL)) 段错误。
         所以改成**手工按第一个分隔符切分**，不依赖任何 static 状态。 */

      /* size 只用于「按像素预算截断曲名」，绝不能同时当 fn_strtok 的输出参数
         （那会被覆写成子串个数，把曲名截成几个字节）。 */
      uint32_t size;
      size = (uint16_t)((norm_x - 40 - 20 * (img->ncolumns - 1)) / img->ncolumns);

      char *psep = strchr(img->screentextchain, '=');
      char *groupspec;
      if (psep)
        {
          *psep = '\0';
          albumtext = strdup(img->screentextchain);
          groupspec = psep + 1;
        }
      else
        {
          albumtext = strdup(img->screentextchain);
          groupspec = NULL;
        }
      if (!albumtext || !albumtext[0])
        {
          free(albumtext);
          albumtext = strdup(DEFAULT_ALBUM_HEADER);
        }

      /* ':' 分组的解析用 f=NULL（没有计数器回调），无 static 泄漏风险 */
      grouparray = groupspec ? fn_strtok(groupspec, ':', grouparray,
                                         &dim, 0, NULL, NULL, globals)
                             : NULL;

      /* dim 是槽位数（含末尾 NULL 哨兵），实际定义的组数要用 arraylength。
         后面画页面时按 dim 索引 grouptext[]/tracktext[]（含释放循环），
         所以 dim 至少要 >= ngroups。 */
      int ndef = grouparray ? arraylength(grouparray) : 0;
      if (ndef < (int) ngroups)
        foutput(WAR "Screen text defines only %d group(s) but the disc has "
                "%d：missing groups will get blank labels.\n",
                ndef, (int) ngroups);
      dim = (ndef > (int) ngroups) ? ndef : (int) ngroups;

      /* globals->grouptextsize / tracktextsize 指向 dvda-author.c 里 main 的
         栈数组，容量 = MENU_TEXT_GROUP_MAX。段数超了必须报错，
         绝不能越界写栈（那会表现为 "stack smashing detected"）。 */
      if (dim > MENU_TEXT_GROUP_MAX)
        EXIT_ON_RUNTIME_ERROR_VERBOSE("Too many menu text groups")


      tracktext = calloc(dim + 1, sizeof(char **));
      if (tracktext == NULL) perror(ERR "Track text allocation");
      grouptext = calloc(dim + 1, sizeof(char **));
      if (grouptext == NULL) perror(ERR "Group text allocation");

      for (k = 0; k < ndef && k < dim; k++)
        {
          char *q = strchr(grouparray[k], '=');
          char *trackspec;
          if (q)
            {
              *q = '\0';              /* 组标题到此为止 */
              trackspec = q + 1;       /* 之后是曲名列表 */
            }
          else
            {
              /* 没有 '=' → 整段只是组标题，轨名留空。
                 （与文档一致：组标题在 '=' 之前） */
              trackspec = NULL;
            }

          grouptext[k] = (char **) calloc(2, sizeof(char *));
          grouptext[k][0] = strdup(grouparray[k]);
          globals->grouptextsize[k] = 2;

          if (trackspec)
            tracktext[k] = fn_strtok(trackspec, ',', tracktext[k],
                                     &globals->tracktextsize[k], 0,
                                     NULL, NULL, globals);

          free(grouparray[k]);
        }
      free(grouparray);
      grouparray = NULL;

      /* 统一补齐：menu.c 画文字时按 ntracks[组] 索引 tracktext[组][轨]，
         条数不足就会读到 NULL 哨兵 → strlen(NULL) 段错误。
         **与文字内容无关，只与条数有关**，故这里无条件补够。 */
      for (k = 0; k < dim; k++)
        {
          if (!grouptext[k])
            {
              grouptext[k] = (char **) calloc(2, sizeof(char *));
              grouptext[k][0] = strdup("");
              globals->grouptextsize[k] = 2;
            }

          int need = (k < (int) ngroups) ? (int) ntracks[k] : 0;
          int have = tracktext[k] ? arraylength(tracktext[k]) : 0;
          if (have >= need) continue;

          char **grown = (char **) realloc(tracktext[k],
                                           (need + 1) * sizeof(char *));
          if (!grown) { perror(ERR "tracktext realloc"); continue; }
          tracktext[k] = grown;
          for (int i = have; i < need; ++i) tracktext[k][i] = strdup("");
          tracktext[k][need] = NULL;
          globals->tracktextsize[k] = need + 1;
        }

      free(grouparray);

      do
        {
          if (img->hierarchical) test_underline(grouptext[group][0], img, globals);
          do
            {
              maxtracklength = MAX(maxtracklength, strlen(tracktext[group][track]));
              if (strlen(tracktext[group][track]) > size) tracktext[group][track][size] = '\0';
              test_underline(tracktext[group][track], img, globals);
              track++;

            }
          while (track < ntracks[group]);
          group++;
          track = 0;

        }
      while (group < Min(img->ncolumns * img->nmenus, ngroups));
    }
  else
    {
      albumtext = strdup(DEFAULT_ALBUM_HEADER);
      grouptext = (char ***)calloc(ngroups, sizeof(char **));
      tracktext = (char ***)calloc(ngroups, sizeof(char **));
      dim = ngroups;
      globals->grouptextsize = calloc(dim, sizeof(int));
      globals->tracktextsize = calloc(dim, sizeof(int));
      for (k = 0; k < dim; k++)
        {
          grouptext[k] = calloc(2, sizeof(char **));

          globals->grouptextsize[k] = 2;
          grouptext[k][0] = calloc(strlen(DEFAULT_GROUP_HEADER_UPPERCASE) + 2, sizeof(char));
          sprintf(grouptext[k][0], "%s%d", DEFAULT_GROUP_HEADER_UPPERCASE, k + 1);
          tracktext[k] = calloc(ntracks[k] + 1, sizeof(char **));
          globals->tracktextsize[k] = ntracks[k] + 1;
          for (j = 0; j < ntracks[k]; j++)
            {
              tracktext[k][j] = calloc(strlen(DEFAULT_TRACK_HEADER) + 3, sizeof(char));
              sprintf(tracktext[k][j], "%s%d", DEFAULT_TRACK_HEADER, j + 1);
            }
          tracktext[k][ntracks[k]] = NULL;
          grouptext[k][1] = NULL;
        }


    }

  track = group = 0;
  int8_t offset = 0;

  do
    {

      if ((f = fopen(img->imagepic[menu], "rb")) == NULL) img->refresh = 1;
      else fclose(f);
      if ((f = fopen(img->highlightpic[menu], "rb")) == NULL) img->refresh = 1;
      else fclose(f);
      if ((f = fopen(img->selectpic[menu], "rb")) == NULL) img->refresh = 1;
      else fclose(f);

      char *command1 = calloc(50 * CHAR_BUFSIZ, 1);
      char *command2 = calloc(50 * CHAR_BUFSIZ, 1);

      char picture_save[CHAR_BUFSIZ + 14];
      sprintf(picture_save, "%s/%s%d", globals->settings.tempdir, "svpic", menu);

      if (globals->debugging)  foutput("%s\n", INF "Authoring top menu streams...");

      if (img->hierarchical)
        {
          img->maxbuttons = (menu == 0) ? ngroups : Min(MAX_BUTTON_Y_NUMBER - 2, ntracks[groupcount]);
          img->resbuttons = 0;
        }


      /* 二级（选曲）页底部第三个槽位：「返回专辑索引」（jump menu 1）。
         只在存在索引页时出现 —— 没有索引页就无处可回；索引页自身也不画
         （那一页**就是**索引）。行号见 MENU_BUTTON_ROW()。 */
      int has_menu_button = (img->index_pages > 0)
        && !(img->page_ntracks && (menu < (unsigned) img->index_pages));

      /* Next / Previous 的可用性。索引页的 Next **只在索引页之间翻**，
         所以**最后一个索引页没有 Next**（去专辑靠缩略图）。
         宏在 menu.h，xml.c 用同一套 —— 两边数量必须一致。 */
      int  has_next = DVDA_HAS_NEXT(img, menu);
      int  has_prev = DVDA_HAS_PREV(img, menu);

      arrowbuttons = has_next + has_prev + has_menu_button;
      menubuttons = (menu < img->nmenus - 1) ? img->maxbuttons : img->maxbuttons + img->resbuttons;

      buttons = 0;


      compute_pointsize(img, 10, img->maxbuttons, globals);

      prepare_overlay_img(albumtext, -1, img, command1, command2, menu, img->albumcolor, globals);
      //free(albumtext);  // segfault here under linux for unknown reasons. TO: fix it.

      if (img->hierarchical)
        {
          if (menu == 0)
            {
              do
                {


                  /* Vicious issue here: use DEFAULT_GROUP_HEADER such that the underline for highlighting does not cut a letter.
                                 With lower-case "group", this happens as the underline cuts the 'p'. Two ways out: underline lower or use another label/use uppercase
                                 Note: This issue was tested to cause spumux crash */
                  mogrify_img(grouptext[groupcount][0], 0, groupcount, img, img->maxbuttons, command1, command2, 0, img->textcolor_pic);
                  groupcount++;
                  buttons++;

                }
              while (groupcount < ngroups);
              groupcount = 0;
            }

          else if (groupcount < ngroups)
            {
              mogrify_img(grouptext[groupcount][0], 0, -1, img, img->maxbuttons, command1, command2, 0, img->groupcolor);
              offset = track;

              do
                {
                  buttons++;
                  mogrify_img(tracktext[groupcount][track], 0, track, img, img->maxbuttons, command1, command2, offset, img->textcolor_pic);
                  track++;
                }
              while ((buttons < menubuttons) && (track < ntracks[groupcount]));


              if (track == ntracks[groupcount])
                {
                  groupcount++;
                  track = 0;
                  offset = 0;
                }
            }
        }
      else if (img->page_ntracks)
        {
          /* 一页一个专辑（见 compute_menu_pages）：
             文字段号 == 页号，行数取本页实际曲目数。
             行距（mogrify_img 的 maxnumtracks）与按钮矩形（xml.c 里
             compute_coordinates 用的 img->maxbuttons）是同一个值，所以
             两边必然对齐；x 一律取第 0 列，让文字占满整宽。 */
          if ((unsigned) menu < img->npages)
            {
              img->maxbuttons = img->page_ntracks[menu];

              if (menu < (unsigned) img->index_pages)
                {
                  /* ---- 一级菜单（专辑索引页）----
                     缩略图由菜单背景提供，这里**不画任何文字**；
                     逐格画按钮区域（描边矩形），再在底部箭头带画翻页文字。 */
                  for (track = 0; track < img->maxbuttons; ++track)
                    mogrify_thumb(track, img, command1, globals);

                  if (has_next || has_prev)
                    {
                      /* 与下面通用箭头同一套槽位判定。
                         has_next 已按「索引页只在自己几页之间翻」算过 ——
                         最后一个索引页没有 Next（也不会接到专辑页）。 */
                      mogrify_arrow_abs(has_next ? DEFAULT_NEXT
                                                 : DEFAULT_PREVIOUS,
                                        has_next ? INDEX_NEXT_X0
                                                 : INDEX_PREV_X0,
                                        img, command2, command1,
                                        img->arrowcolor);

                      if (has_next && has_prev)
                        mogrify_arrow_abs(DEFAULT_PREVIOUS, INDEX_PREV_X0,
                                          img, command2, command1,
                                          img->arrowcolor);
                    }

                  buttons = img->maxbuttons;
                }
              else
                {
                  mogrify_img(grouptext[menu][0], 0, -1, img, img->maxbuttons,
                              command1, command2, 0, img->groupcolor);

                  for (track = 0; track < img->maxbuttons; ++track)
                    mogrify_img(tracktext[menu][track], 0, track, img,
                                img->maxbuttons, command1, command2, 0,
                                img->textcolor_pic);

                  buttons = img->maxbuttons;
                }
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
        }


      if ((img->nmenus > 1) && (menu < img->nmenus)
          && !(img->page_ntracks && (menu < (unsigned) img->index_pages)))
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

          /* 槽 1：有 Next 放 Next；没有 Next（末页）但有 Previous 就把
             Previous 提上来 —— 否则槽 1 空着、看着像少了个按钮。 */
          if (has_next || has_prev)
            {
              buttons++;
              arrows_drawn++;
              mogrify_img(has_next ? DEFAULT_NEXT : DEFAULT_PREVIOUS,
                          img->ncolumns - 1, img->maxbuttons, img,
                          img->maxbuttons, command1, command2, 0,
                          img->arrowcolor);
            }

          if (has_next && has_prev)
            {
              buttons++;
              arrows_drawn++;
              mogrify_img(DEFAULT_PREVIOUS, img->ncolumns - 1,
                          img->maxbuttons + 1, img, img->maxbuttons,
                          command1, command2, 0, img->arrowcolor);
            }

          /* 第三个槽位：「返回专辑索引」。槽位顺序必须与 xml.c 里
             两个 XML 的按钮编号顺序一致（Next → Previous → Menu）。 */
          if (has_menu_button)
            {
              buttons++;
              arrows_drawn++;
              mogrify_img(DEFAULT_MENU_BUTTON, img->ncolumns - 1,
                          MENU_BUTTON_ROW(img), img, img->maxbuttons,
                          command1, command2, 0, img->arrowcolor);
            }

          if (globals->debugging && (arrows_drawn != (int) arrowbuttons))
            foutput(WAR "Arrow count mismatch: %d drawn, %d expected\n",
                    arrows_drawn, (int) arrowbuttons);
        }

      char *q = quote(img->imagepic[menu]);
      strcat(command2, q);
      free(q);
      if (globals->veryverbose) foutput(INF "Menu: %d/%d, groupcount: %d/%d.\n       Launching mogrify (image) with command line: %s\n", menu, img->nmenus, groupcount, ngroups, command2);
      if (system(win32quote(command2)) == -1) EXIT_ON_RUNTIME_ERROR_VERBOSE("System command failed");
      free(command2);
      command2 = NULL;
      copy_file(img->imagepic[menu], img->highlightpic[menu], globals);
      q = quote(img->highlightpic[menu]);
      strcat(command1, q);
      free(q);
      if (globals->veryverbose) foutput(INF "Menu: %d/%d, groupcount: %d/%d.\n       Launching mogrify (highlight) with command line: %s\n", menu, img->nmenus, groupcount, ngroups, command1);
      if (system(win32quote(command1)) == -1) EXIT_ON_RUNTIME_ERROR_VERBOSE("System command failed");
      free(command1);
      command1 = NULL;
      char command3[500];
      q  = quote(img->selectfgcolor_pic);
      char *q2 = quote(img->textcolor_pic);
      char *q3 = quote(img->imagepic[menu]);
      char *q4 = quote(img->selectpic[menu]);
      snprintf(command3, sizeof(command3), "%s %s \"rgb(%s)\"  %s \"rgb(%s)\" %s %s", convert, "-fill", q, "-opaque", q2, q3, q4);
      free(q);
      free(q2);
      free(q3);
      free(q4);
      if (globals->veryverbose) foutput(INF "Menu: %d/%d, groupcount: %d/%d.\n       Launching convert (select) with command line: %s\n", menu, img->nmenus, groupcount, ngroups, command3);
      if (system(win32quote(command3)) == -1) EXIT_ON_RUNTIME_ERROR_VERBOSE("System command failed");

      menu++;
      group = 0;


    }
  while ((menu < img->nmenus) && (groupcount < dim));

  for (group = 0; group < dim; ++group)
    {
      for (uint32_t i = 0; i < globals->grouptextsize[group]; ++i)
        free(grouptext[group][i]);
      for (uint32_t i = 0; i < globals->tracktextsize[group]; ++i)
        free(tracktext[group][i]);
      free(grouptext[group]);
      free(tracktext[group]);
    }

  free(grouptext);
  free(tracktext);

  if (img->screentextchain && basemotif)
    {
      free(basemotif[0]);
      free(basemotif[1]);
      free(basemotif);
    }

  if (globals->debugging)
    if (!errno)
      foutput("%s\n", MSG_TAG "Top menu pictures were authored.");

  return errno;
}


int create_stillpic_directory(char *string, int32_t count, globalData *globals)
{
  if (!string)
    {
      fprintf(stderr, ERR "Null string input for stillpic in create_stillpic_directory, with count=%d\n", count);
      exit(-1);
    }

  static int32_t  k;
  change_directory(globals->settings.stillpicdir, globals);
  if (k == count)
    {
      if (globals->debugging) foutput(WAR "Too many pics, only %d sound track%s skipping others...\n", count, (count == 1) ? "," : "s,");

      change_directory(globals->settings.workdir, globals);
      return 0;
    }

  if (*string == '\0')
    {
      if (globals->debugging) foutput(INF "Jumping one track for picture rank = %d\n", k);

      change_directory(globals->settings.workdir, globals);
      return 1;
    }

#ifndef __WIN32__
  struct stat buf;

  if (stat(string, &buf) == -1)
    {
      fprintf(stderr, ERR "create_stillpic_directory: could not stat file %s\n", string);
      exit(-1);
    }
  if (S_IFDIR & buf.st_mode)
    {
      if (globals->debugging) foutput(INF "Directory %s will be parsed for still pics\n", string);
      globals->settings.stillpicdir = strdup(string);

      change_directory(globals->settings.workdir, globals);
      return 0;
    }
  if (S_IFREG & buf.st_mode)
    {
#endif
      char dest[strlen(globals->settings.tempdir) + 13];
      sprintf(dest, "%s"SEPARATOR"pic_%03d.jpg", globals->settings.tempdir, k);
      if (globals->debugging) fprintf(stderr, DBG "Picture %s will be copied to temporary directory as %s.\n", string, dest);

      copy_file(string, dest, globals);

      ++k;

      change_directory(globals->settings.workdir, globals);
      return 1;
#ifndef __WIN32__
    }
#endif

  change_directory(globals->settings.workdir, globals);
  return 0;

}
#endif














