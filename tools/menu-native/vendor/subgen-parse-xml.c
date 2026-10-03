/*
    Parsing of spumux XML control files
*/
/*
 * Copyright (C) 2003 Scott Smith (trckjunky@users.sourceforge.net)
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or (at
 * your option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307
 * USA
 */

#include "config.h"

#include "compat.h"

#include "menu-assert.h"
#include <ctype.h>

#include "subglobals.h"

#include "subgen.h"
#include "readxml.h"


static void printtime(char *b,int t)
{
    sprintf(b,"%d:%02d:%02d.%03d",
            (t/90/1000/60/60),
            (t/90/1000/60)%60,
            (t/90/1000)%60,
            (t/90)%1000);
}

static unsigned int parsetime(const char *t)
  /* parses a time as [[hh:]mm:]ss[.cc], returning the value in 90kHz clock units. */
  {
    bool tf = true; /* haven't seen decimal point yet */
    int rt = 0; /* accumulation of all componetns except last */
    int n = 0; /* value of last copmonent accumulated here */
    int nd = 0; /* multiplier for next digit of n */
    while (*t)
      {
        if (isdigit(*t))
          {
            if (nd < 10000)
              {
                n = n * 10 + t[0] - '0';
                nd *= 10;
              } /*if*/
          }
        else if (*t == ':')
          {
            assert(tf);
            rt = rt * 60 + n;
            n = 0;
            nd = 1;
          }
        else if (*t == '.' || *t == ',')
          {
          /* on to fractions of a second */
            assert(tf);
            rt = rt * 60 + n;
            n = 0;
            nd = 1;
            tf = false;
          } /*if*/
        t++;
      } /*while*/
    if (tf)
        return (rt * 60 + n) * 90000;
    else
        return rt * 90000 + 90000 * n / nd;
  } /*parsetime*/

static bool
    had_stream = false, /* whether I've seen <stream> */
    had_spu = false, /* whether I've seen <spu> */
    had_textsub = false; /* whether I've seen <textsub> */
static stinfo *curspu = 0; /* current <spu> directive collected here */
static button *curbutton=0;
static char * filename = 0;

static void stream_begin()
  {
    if (had_stream)
      {
        fprintf(stderr, "ERR:  Only one stream is currently allowed.\n");
        exit(1);
      } /*if*/
    had_stream = true;
  } /*stream_begin*/

static void stream_video_format(const char *v)
  {
    if (!strcasecmp(v, "NTSC"))
      {
        default_video_format = VF_NTSC;
      }
    else if (!strcasecmp(v, "PAL"))
      {
        default_video_format = VF_PAL;
      }
    else
      {
        fprintf(stderr, "ERR:  unrecognized video format \"%s\"\n", v);
        exit(1);
      } /*if*/
  } /*stream_video_format*/

static void spu_begin()
  {
    if (had_textsub)
      {
        fprintf(stderr, "ERR:  cannot have both <textsub> and <spu>\n");
        exit(1);
      } /*if*/
    curspu = malloc(sizeof(stinfo));
    memset(curspu, 0, sizeof(stinfo));
    had_spu = true;
  }

static void spu_image(const char *v)        { curspu->img.fname=localize_filename(v); }
static void spu_highlight(const char *v)    { curspu->hlt.fname=localize_filename(v); }
static void spu_select(const char *v)       { curspu->sel.fname=localize_filename(v); }
static void spu_start(const char *v)        { curspu->spts         = parsetime(v); }
static void spu_end(const char *v)          { curspu->sd           = parsetime(v); }
static void spu_outlinewidth(const char *v) { curspu->outlinewidth = strtounsigned(v, "spu outlinewidth");      }
static void spu_xoffset(const char *v)      { curspu->x0 = strtounsigned(v, "spu xoffset");                }
static void spu_yoffset(const char *v)      { curspu->y0 = strtounsigned(v, "spu yoffset");                }

static void spu_force(const char *v)
{
    curspu->forced = xml_ison(v, "spu force");
}

static void spu_transparent(const char *v)
{
    curspu->transparentc = parse_color(v, "transparency");
}

static void spu_autooutline(const char *v)
{
    if (!strcmp(v, "infer"))
        curspu->autooutline = true;
    else
      {
        fprintf(stderr, "ERR:  Unknown autooutline type %s\n", v);
        exit(1);
      } /*if*/
}

static void spu_autoorder(const char *v)
{
    if (!strcmp(v, "rows"))
        curspu->autoorder = false;
    else if (!strcmp(v, "columns"))
        curspu->autoorder = true;
    else
      {
        fprintf(stderr, "ERR:  Unknown autoorder type %s\n", v);
        exit(1);
      } /*if*/
}

static void spu_complete()
{
    if (!curspu->sd) /* no end time specified */
        curspu->sd = -1; /* default to indefinite */
    else
      {
        if (curspu->sd <= curspu->spts)
          {
            char stime[50], etime[50];
            printtime(stime, curspu->spts);
            printtime(etime, curspu->sd);
            fprintf(stderr, "ERR:  sub has end (%s)<=start (%s), skipping\n", etime, stime);
            nr_subtitles_skipped++;
            return;
          } /*if*/
        curspu->sd -= curspu->spts;
      } /*if*/
    spus = realloc(spus, (numspus + 1) * sizeof(stinfo *));
    spus[numspus++] = curspu;
    curspu = 0;
}

static void button_begin()
{
    curspu->buttons = realloc(curspu->buttons, (curspu->numbuttons + 1) * sizeof(button));
    curbutton = &curspu->buttons[curspu->numbuttons++];
    memset(curbutton, 0, sizeof(button));
    curbutton->r.x0 = -1;
    curbutton->r.y0 = -1;
    curbutton->r.x1 = -1;
    curbutton->r.y1 = -1;
}

static void action_begin()
{
    button_begin();
    curbutton->autoaction = true;
}

static void button_label(const char *v) { curbutton->name  = strdup(v); }
static void button_up(const char *v)    { curbutton->up    = strdup(v); }
static void button_down(const char *v)  { curbutton->down  = strdup(v); }
static void button_left(const char *v)  { curbutton->left  = strdup(v); }
static void button_right(const char *v) { curbutton->right = strdup(v); }
static void button_x0(const char *v)    { curbutton->r.x0  = strtounsigned(v, "button x0");   }
static void button_y0(const char *v)    { curbutton->r.y0  = strtounsigned(v, "button y0");   }
static void button_x1(const char *v)    { curbutton->r.x1  = strtounsigned(v, "button x1");   }
static void button_y1(const char *v)    { curbutton->r.y1  = strtounsigned(v, "button y1");   }

enum { /* parse states */
    SPU_BEGIN=0, /* initial state must be 0 */
    SPU_ROOT, /* expect <stream> */
    SPU_STREAM, /* expect <spu> or <textsub> */
    SPU_SPU, /* within <spu>, expect <button> or <action> */
    SPU_NOSUB /* not expecting subtags */
};

static struct elemdesc spu_elems[]={
    {"subpictures",SPU_BEGIN,SPU_ROOT,0,0},
    {"stream",SPU_ROOT,SPU_STREAM,stream_begin,0},
    {"spu",SPU_STREAM,SPU_SPU,spu_begin,spu_complete},
    {"button",SPU_SPU,SPU_NOSUB,button_begin,0},
    {"action",SPU_SPU,SPU_NOSUB,action_begin,0},
    {0,0,0,0,0}
};

static struct elemattr spu_attrs[]={
    {"subpictures","format",stream_video_format},
    {"spu","image",spu_image},
    {"spu","highlight",spu_highlight},
    {"spu","select",spu_select},
    {"spu","start",spu_start},
    {"spu","end",spu_end},
    {"spu","transparent",spu_transparent},
    {"spu","autooutline",spu_autooutline},
    {"spu","outlinewidth",spu_outlinewidth},
    {"spu","autoorder",spu_autoorder},
    {"spu","force",spu_force},
    {"spu","xoffset",spu_xoffset},
    {"spu","yoffset",spu_yoffset},
    {"button","name",button_label},
    {"button","up",button_up},
    {"button","down",button_down},
    {"button","left",button_left},
    {"button","right",button_right},
    {"button","x0",button_x0},
    {"button","y0",button_y0},
    {"button","x1",button_x1},
    {"button","y1",button_y1},
    {"action","name",button_label},
    {"action","up",button_up},
    {"action","down",button_down},
    {"action","left",button_left},
    {"action","right",button_right},
    {"action","x0",button_x0},
    {"action","y0",button_y0},
    {"action","x1",button_x1},
    {"action","y1",button_y1},
    {0,0,0}
};

int spumux_parse(const char *fname)
{
   return readxml(fname,spu_elems,spu_attrs);
}
