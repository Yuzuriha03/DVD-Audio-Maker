/*
    dvdauthor mainline, interpretation of command-line options and parsing of
    dvdauthor XML control files
*/
/*
 * Copyright (C) 2002 Scott Smith (trckjunky@users.sourceforge.net)
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
#include <fcntl.h>

#include "conffile.h"
#include "dvdauthor.h"
#include "readxml.h"
#include "rgb.h"

int default_video_format = VF_NONE;

/* common parsing bits for both command line and XML file */

#define RGB2YCrCb(R,G,B) ((((int)RGB2Y(R,G,B))<<16)|(((int)RGB2Cr(R,G,B))<<8)|(((int)RGB2Cb(R,G,B))))

extern _Bool videoedit, audioedit;


static int readdvdauthorxml(const char *xmlfile,const char *fb);

static enum
  {
    chapters_neither, /* neither of following seen yet */
    chapters_cells, /* vob chapters specified via cells (and possibly also "chapters" attribute) */
    chapters_chapters, /* only via "chapters" attribute */
  }
    hadchapter = chapters_neither; /* info about last VOB in current PGC */
static int
    pauselen=0;
static bool
    writeoutput = true;
static char
    *cur_chapters = 0;








static void parseentries(struct pgc *p, vtypes ismenuf, const char *b)
  {
    char *v;
    while (NULL != (v = str_extract_until(&b, ", ")))
      {
        pgc_add_entry(p, ismenuf, v);
        free(v);
      } /*while*/
  } /*parseentries*/

static double parsechapter(const char *s)
{
    double total=0,field=0;
    int i;

    for( i=0; s[i]; i++ ) {
        if(!strchr(s+i,':')) {
            char *last;
            field=strtod(s+i,&last);
            if( *last ) {
                fprintf(stderr,"ERR:  Cannot parse chapter timestamp '%s'\n",s);
                exit(1);
            }
            break;
        } else if(isdigit(s[i]))
            field=field*10+s[i]-'0';
        else if(s[i]==':') {
            total=total*60+field;
            field=0;
        } else {
            fprintf(stderr,"ERR:  Cannot parse chapter timestamp '%s'\n",s);
            exit(1);
        }
    }
    return total*60+field;
}

static void parsechapters(const char *o, struct source *src, int pauselen)
  /* parses a chapter spec and adds corresponding cell definitions. */
  {
    char *s;
    double last = 0;
    cell_chapter_types lastchap = CELL_NEITHER;
    while (NULL != (s = str_extract_until(&o, ", ")))
      {
        const double total = parsechapter(s);
        if (total > last)
          {
            source_add_cell(src, last, total, lastchap, 0, 0);
            last = total;
            lastchap = CELL_CHAPTER_PROGRAM;
          }
        else if (total == last)
            lastchap = CELL_CHAPTER_PROGRAM;
      /* else report error? user specified chapter times in wrong order? */
        free(s);
      } /*while*/
    source_add_cell(src, last, -1, lastchap, pauselen, 0);
  } /*parsechapters*/

static void readpalette(struct pgc *p,const char *fname)
{
    int i,rgbf;
    struct vfile h;

    h=varied_open(fname, O_RDONLY, "palette file");
    
    /* write out colors, the hex is the 0yuv combined in one integer 00yyuuvv */
    i=strlen(fname);
    rgbf=( i>=4 && !strcasecmp(fname+i-4,".rgb") );
    for( i=0; i<16; i++ ) {
        int pcolor;
        fscanf(h.h, "%x", &pcolor);
        if( rgbf ) {
            const int r=(pcolor>>16)&255,
                g=(pcolor>>8)&255,
                b=(pcolor)&255;
            pcolor=RGB2YCrCb(r,g,b);
        }
        pgc_set_color(p,i,pcolor);
    }

#if 0
    int b;
    unsigned char groups[24];

    memset(groups,0,24);
    i=0;
    while(fscanf(h.h,"%x",&b) == 1 && i<24 ) {
        groups[i++]=b>>24;
        groups[i++]=b>>16;
        groups[i++]=b>>8;
        groups[i++]=b;
        if( !(i&7) )
            pgc_set_buttongroup(p,(i-8)/8,groups+(i-8));
    }
#endif
    varied_close(h);
}


/* bits for authoring via command line */
#define NOXML                                                                          \
    if (xmlfile)                                                                       \
      {                                                                                \
        fprintf                                                                        \
          (                                                                            \
            stderr,                                                                    \
            "ERR:  Cannot use command line options after specifying XML config file\n" \
          );                                                                           \
        return 1;                                                                      \
      } /*if*/
#define MAINDEF                                                               \
            if (!usedtocflag)                                                 \
              {                                                               \
                fprintf(stderr, "ERR:  Must first specify -t, -m, or -x.\n"); \
                return 1;                                                     \
              } /*if*/                                                        \
            if (istoc && istitle)                                             \
              {                                                               \
                fprintf(stderr, "ERR:  TOC cannot have titles\n");            \
                return 1;                                                     \
              } /*if*/                                                        \
            if (!va[istitle])                                                          \
              {                                                               \
                va[istitle] = pgcgroup_new((int)istoc + 1 - (int)istitle); \
              } /*if*/

#define MAINDEFPGC                                                   \
            MAINDEF                                                  \
            if (!curpgc)                                             \
                curpgc = pgc_new();

#define MAINDEFVOB                                                   \
            MAINDEFPGC                                               \
            if (!curvob)                                             \
                curvob = source_new();

#define FLUSHPGC                                                     \
            if (curpgc)                                              \
              {                                                      \
                pgcgroup_add_pgc(va[istitle], curpgc);               \
                curpgc = 0;                                          \
              } /*if*/

/* authoring via XML file */

enum {
    DA_BEGIN=0,
    DA_ROOT,
    DA_SET,
    DA_PGCGROUP,
    DA_PGC,
    DA_VOB,
    DA_SUBP,
    DA_NOSUB
};

static struct pgcgroup
    *titles=0, /* titles saved here (only one set allowed) */
    *curgroup=0; /* current menus or titles */
static struct menugroup
    *mg=0, /* current menu group (for titleset or vmgm) */
    *vmgmmenus=0,  /* vmgm menu group saved here on completion */
    *amgmmenus=0;
static struct pgc *curpgc=0,*fpc=0;
static struct source *curvob=0;
static const char
    *fbase = 0, /* output directory name */
    *buttonname = 0; /* name of button currently being defined */
static vtypes
    ismenuf = VTYPE_VTS; /* type of current pgcgroup structure being parsed */
static bool
    istoc = false, /* true for vmgm, false for titleset */
    hadtoc = false, /* set to true when vmgm seen */
    hadaudiotoc=false;
static int
    setvideo=0, /* to keep count of <video> tags */
    setaudio=0, /* to keep count of <audio> tags */
    setsubpicture=0, /* to keep count of <subpicture> tags */
    subpmode=DA_NOSUB,
      /* for determining context of a <subpicture> tag:
        DA_NOSUB -- invalid
        DA_PGC -- inside a <pgc> tag (no "lang" attribute allowed)
        DA_PGCGROUP -- inside a <menus> or <titles> tag, must come before the <pgc> tags
      */
    subpstreamid=-1; /* id attribute of current <stream> saved here */
static char *subpstreammode=0; /* mode attribute of current <stream> saved here */
static enum
  {
    vob_has_neither, /* neither of following seen yet */
    vob_has_chapters_pause, /* vob has "chapters" or "pause" attribute */
    vob_has_cells, /* vob has <cell> subtags */
  }
    vobbasic; /* info about each VOB */
static cell_chapter_types
    cell_chapter;
static double
    cell_starttime,
    cell_endtime;
static char
    menulang[3];

static void set_video_attr(int attr,const char *s)
{
    if (ismenuf != VTYPE_VTS)
        menugroup_set_video_attr(mg, attr, s);
    else
        pgcgroup_set_video_attr(titles, attr, s);
}

static void set_audio_attr(int attr,const char *s,int ch)
{
    if (ismenuf != VTYPE_VTS)
        menugroup_set_audio_attr(mg,attr,s,ch);
    else
        pgcgroup_set_audio_attr(titles,attr,s,ch);
}

static void set_subpic_attr(int attr,const char *s,int ch)
{
    if (ismenuf != VTYPE_VTS)
        menugroup_set_subpic_attr(mg,attr,s,ch);
    else
        pgcgroup_set_subpic_attr(titles,attr,s,ch);
}

static void set_subpic_stream(int ch,const char *m,int id)
{
    if (ismenuf != VTYPE_VTS)
        menugroup_set_subpic_stream(mg,ch,m,id);
    else
        pgcgroup_set_subpic_stream(titles,ch,m,id);
}

static int parse_pause(const char *f)
{
    if (!strcmp(f,"inf"))
        return 255;
    else
        return strtounsigned(f, "pause time"); /* should check it's in [0 .. 254] */
}

static void dvdauthor_outputdir(const char *s)
  {
    if (!fbase)
      {
        fbase = localize_filename(s);
      } /*if*/
  }

static void dvdauthor_jumppad(const char *s)
{
    if(xml_ison(s, "jumppad"))
        dvdauthor_enable_jumppad();
}

static void dvdauthor_allgprm(const char *s)
{
    if (xml_ison(s, "allgprm"))
        dvdauthor_enable_allgprm();
}

static void dvdauthor_video_format
  (
    const char * s
  )
  {
    if (!strcmp(s, "ntsc") || !strcmp(s, "NTSC"))
      {
        default_video_format = VF_NTSC;
      }
    else if (!strcmp(s, "pal") || !strcmp(s, "PAL"))
      {
        default_video_format = VF_PAL;
      }
    else
      {
        fprintf(stderr, "ERR:  Unrecognized video format \"%s\"\n", s);
        parser_err = true;
      } /*if*/
  } /*dvdauthor_video_format*/

static void getfbase()
  {
    if (!writeoutput)
        fbase = 0;
    else if (!fbase)
      {
        fprintf(stderr, "ERR:  Must specify working directory\n");
        parser_err = true;
      } /*if*/
  } /*getfbase*/

static void dvdauthor_end()
/* called on </dvdauthor> end tag, generates the VMGM if specified.
  This needs to be done after all the titles, so it can include
  information about them. */
  {
    if (hadtoc)
      {
        dvdauthor_vmgm_gen(fpc, vmgmmenus, fbase);
        pgc_free(fpc);
        menugroup_free(vmgmmenus);
        fpc = 0;
        vmgmmenus = 0;
      } /*if*/

    if( hadaudiotoc ) {
         dvdauthor_amgm_gen(amgmmenus,fbase);
         if( fpc )
             pgc_free(fpc);
         if( amgmmenus )
             menugroup_free(amgmmenus);
         fpc=0;
         amgmmenus=0;
     }
  } /*dvdauthor_end*/



// DVDA-AUTHOR PATCH



// END OF DVDA-AUTHOR PATCH






static void amgm_start()
{
    if( hadaudiotoc ) {
        fprintf(stderr,"ERR:  Can only define one AMGM\n");
        parser_err=1;
        return;
    }
    mg=menugroup_new();
    audioedit=true;
    videoedit=false;
    istoc=1;
    hadaudiotoc=1;
}

static void amgm_end()
{
    getfbase();
    // we put off compilation of amgm until after the audio groups
    amgmmenus=mg;
    mg=0;
}

static void fpc_start()
{
    if( !istoc ) {
        fprintf(stderr,"ERR:  Can only specify <fpc> under <vmgm>\n");
        parser_err = true;
        return;
    }
    if( fpc ) {
        fprintf(stderr,"ERR:  Already defined <fpc>\n");
        parser_err = true;
        return;
    }
    fpc=pgc_new();
    parser_acceptbody = true;
}

static void fpc_end()
{
    pgc_set_pre(fpc,parser_body);
}

static void pgcgroup_start()
  /* common part of both <menus> and <titles> start. */
  {
    setvideo = 0;
    setaudio = -1;
    setsubpicture = -1;
    subpmode = DA_PGCGROUP;
  } /*pgcgroup_start*/


static void menus_start()
  /* called on a <menus> tag. */
  {
    ismenuf = (istoc ? VTYPE_VMGM : VTYPE_VTSM);
    curgroup = pgcgroup_new(ismenuf);
    pgcgroup_start();
    strcpy(menulang, "en"); /* default. */
      /* fixme: would be nice to make this a preference setting, perhaps default to locale? */
      /* fixme: no check for buffer overflow! */
  }

static void menus_lang(const char *lang)
  {
    strcpy(menulang, lang);
      /* fixme: no check for buffer overflow! */
  } /*menus_lang*/

static void menus_end()
  {
    menugroup_add_pgcgroup(mg, menulang, curgroup);
    curgroup = 0;
  } /*menus_end*/

static void video_start()
  /* called on a <video> tag. */
  {
    if (setvideo)
      {
        fprintf(stderr, "ERR:  Already defined video characteristics for this PGC group\n");
          /* only one video stream allowed */
        parser_err = true;
      }
    else
        setvideo = 1;
  }

static void video_format(const char *c)
  {
    set_video_attr(VIDEO_FORMAT, c);
  }

static void video_aspect(const char *c)
  {
    set_video_attr(VIDEO_ASPECT, c);
  }

static void video_resolution(const char *c)
  {
    set_video_attr(VIDEO_RESOLUTION, c);
  }

static void video_widescreen(const char *c)
  {
    set_video_attr(VIDEO_WIDESCREEN, c);
  }

static void video_caption(const char *c)
  {
    set_video_attr(VIDEO_CAPTION, c);
  }

static void audio_start()
  /* called on an <audio> tag. */
  {
    setaudio++;
    if (setaudio >= 8)
      {
        fprintf(stderr, "ERR:  Attempting to define too many audio streams for this PGC group\n");
        parser_err = true;
      } /*if*/
  }

static void audio_format(const char *c)
  {
    set_audio_attr(AUDIO_FORMAT, c, setaudio);
  }

static void audio_quant(const char *c)
  {
    set_audio_attr(AUDIO_QUANT, c, setaudio);
  }

static void audio_dolby(const char *c)
  {
    set_audio_attr(AUDIO_DOLBY, c, setaudio);
  }

static void audio_lang(const char *c)
  {
    set_audio_attr(AUDIO_LANG, c, setaudio);
  }

static void audio_samplerate(const char *c)
  {
    set_audio_attr(AUDIO_SAMPLERATE, c, setaudio);
  }

static void audio_content(const char *c)
  {
    set_audio_attr(AUDIO_CONTENT, c, setaudio);
  } /*audio_content*/

static void audio_channels(const char *c)
  {
    char ch[4];
    if (strlen(c) == 1)
      {
        ch[0] = c[0];
        ch[1] = 'c';
        ch[2] = 'h';
        ch[3] = 0;
        c = ch;
      } /*if*/
    set_audio_attr(AUDIO_CHANNELS, ch, setaudio);
  }

static void subattr_group_start()
  /* called for a <subpicture> tag immediately within a pgcgroup (<menus> or <titles>). */
 {
    if (subpmode != DA_PGCGROUP)
      {
        fprintf(stderr, "ERR:  Define all the subpictures before defining PGCs\n");
        parser_err = true;
        return;
      } /*if*/
    setsubpicture++;
    if (setsubpicture >= 32)
      {
        fprintf(stderr, "ERR:  Attempting to define too many subpicture streams for this PGC group\n");
        parser_err = true;
      } /*if*/
  } /*subattr_group_start*/

static void subattr_pgc_start()
  /* called for a <subpicture> tag immediately within a <pgc>. */
  {
    setsubpicture++;
    if (setsubpicture >= 32)
      {
        fprintf(stderr, "ERR:  Attempting to define too many subpicture streams for this PGC group\n");
        parser_err = true;
      } /*if*/
  } /*subattr_pgc_start*/

static void subattr_lang(const char *c)
  {
    if (subpmode == DA_PGCGROUP)
        set_subpic_attr(SPU_LANG, c, setsubpicture);
    else
      {
        fprintf
          (
            stderr,
            "ERR:  Cannot set subpicture language within a pgc; do it within titles or menus\n"
          );
        parser_err = true;
      } /*if*/
  } /*subattr_lang*/

static void subattr_content(const char * c)
  {
    if (subpmode == DA_PGCGROUP)
      {
        set_subpic_attr(SPU_CONTENT, c, setsubpicture);
      }
    else
      {
        fprintf
          (
            stderr,
            "ERR:  Cannot set subpicture content type within a pgc; do it within titles or menus\n"
          );
        parser_err = true;
      } /*if*/
  } /*subattr_content*/

static void stream_start()
  /* called on a <stream> tag. */
  {
    subpstreamid = -1;
    subpstreammode = 0;
  } /*stream_start*/

static void substream_id(const char *c)
  /* saves the id attribute for the current <stream> tag. */
  {
    subpstreamid = strtounsigned(c, "subpicture stream id");
    if (subpstreamid < 0 || subpstreamid >= 32)
      {
        fprintf(stderr, "ERR:  Subpicture stream id must be 0-31: '%s'.\n", c);
        parser_err = true;
      } /*if*/
  } /*substream_id*/

static void substream_mode(const char *c)
  /* saves the mode attribute for the current <stream> tag. */
  {
    subpstreammode = strdup(c); /* won't leak, because I won't be called more than once */
  }

static void stream_end()
  {
    if (subpstreamid == -1 || !subpstreammode)
      {
        fprintf(stderr, "ERR:  Must define the mode and id for the stream.\n");
        parser_err = true;
      }
    else if (subpmode == DA_PGCGROUP)
      {
        set_subpic_stream(setsubpicture, subpstreammode, subpstreamid);
      }
    else
        pgc_set_subpic_stream(curpgc, setsubpicture, subpstreammode, subpstreamid);
    free(subpstreammode);
  } /*stream_end*/

static void pgc_start()
{
    curpgc = pgc_new();
    hadchapter = chapters_neither; /* reset for new menu/title */
    setsubpicture = -1;
    subpmode = DA_PGC;
}

static void pgc_entry(const char *e)
{
    // xml attributes can only be defined once, so entry="foo" entry="bar" won't work
    // instead, use parseentries...
    parseentries(curpgc, ismenuf, e);
}

static void pgc_palette(const char *p)
{
    readpalette(curpgc,p);
}

static void pgc_pause(const char *c)
{
    pgc_set_stilltime(curpgc,parse_pause(c));
}

static void pgc_end()
{
    pgcgroup_add_pgc(curgroup,curpgc);
    curpgc=0;
    subpmode=DA_NOSUB;
}

static void pre_start()
{
    parser_acceptbody = true;
}

static void pre_end()
{
    pgc_set_pre(curpgc,parser_body);
}

static void post_start()
{
    parser_acceptbody = true;
}

static void post_end()
{
    pgc_set_post(curpgc,parser_body);
}

static void vob_start()
{
    curvob = source_new();
    pauselen = 0;
    vobbasic = vob_has_neither; /* to begin with */
  /* but note hadchapter keeps its value from previous VOB in this PGC
    if no chapters attribute or cells are seen */
    cell_endtime = 0;
}

static void vob_file(const char *f)
{
    f = localize_filename(f);
    source_set_filename(curvob, f);
}

static void vob_chapters(const char *c)
  {
    vobbasic = vob_has_chapters_pause;
    hadchapter = chapters_chapters;
    cur_chapters = strdup(c); /* won't leak, because I won't be called more than once per <vob> */
  }

static void vob_pause(const char *c)
  {
    vobbasic = vob_has_chapters_pause;
    pauselen = parse_pause(c);
  }

static void vob_end()
  {
    if (vobbasic != vob_has_cells) /* = vob_has_chapters_pause or vob_has_neither */
      {
        if (hadchapter == chapters_chapters) /* vob has chapters attribute */
          {
            parsechapters(cur_chapters, curvob, pauselen);
            hadchapter = chapters_cells;
              /* subsequent <vob>s in this PGC shall be only cells, unless
                they have their own "chapter" attribute */
          }
        else /* hadchapter = chapters_neither or chapters_cells */
            source_add_cell
              (
                /*source =*/ curvob,
                /*starttime =*/ 0,
                /*endtime =*/ -1,
                /*chap =*/
                    hadchapter == chapters_neither ?
                        CELL_CHAPTER_PROGRAM /* first <vob> in <pgc> */
                    :
                        CELL_NEITHER, /* subsequent <vob>s in <pgc> */
                /*pause =*/ pauselen,
                /*cmd =*/ 0
              );
              /* default to single chapter/cell for entire source file */
      } /*if*/
    pgc_add_source(curpgc, curvob);
    free(cur_chapters);
    cur_chapters = 0;
    curvob = 0;
  } /*vob_end*/

static void cell_start()
  {
    parser_acceptbody = true; /* collect cell commands */
    assert(vobbasic != vob_has_chapters_pause);
      /* no "chapters" or "pause" attribute on containing <vob> allowed */
    vobbasic = vob_has_cells;
    cell_starttime = cell_endtime; /* new cell starts by default where previous one ends */
    cell_endtime = -1;
    cell_chapter = CELL_NEITHER; /* to begin with */
    pauselen = 0;
    hadchapter = chapters_cells;
  }

static void cell_parsestart(const char *f)
  {
    cell_starttime = parsechapter(f);
  }

static void cell_parseend(const char *f)
  {
    cell_endtime = parsechapter(f);
  }

static void cell_parsechapter(const char *f)
  {
    if (xml_ison(f, "chapter"))
        cell_chapter = CELL_CHAPTER_PROGRAM;
  }

static void cell_parseprogram(const char *f)
  {
    if (xml_ison(f, "program") && cell_chapter != CELL_CHAPTER_PROGRAM)
        cell_chapter = CELL_PROGRAM;
  }

static void cell_pauselen(const char *f)
  {
    pauselen=parse_pause(f);
  }

static void cell_end()
  {
    assert (cell_starttime >= 0);
    source_add_cell(curvob, cell_starttime, cell_endtime, cell_chapter, pauselen, parser_body);
    pauselen = 0;
  }

static void button_start()
{
    parser_acceptbody = true;
    buttonname=0;
}

static void button_name(const char *f)
{
    buttonname=strdup(f);
}

static void button_end()
{
    pgc_add_button(curpgc,buttonname,parser_body);
    if(buttonname) free((char *)buttonname);
}

static struct elemdesc elems[]={
    {"dvdauthor", DA_BEGIN,   DA_ROOT,    0,               dvdauthor_end},
     // DVDA-AUTHOR PATCH
     {"amgm",      DA_ROOT,    DA_SET,     amgm_start,      amgm_end},
    //
    {"fpc",       DA_SET,     DA_NOSUB,   fpc_start,       fpc_end},
    {"menus",     DA_SET,     DA_PGCGROUP,menus_start,     menus_end},
    {"video",     DA_PGCGROUP,DA_NOSUB,   video_start,     0},
    {"audio",     DA_PGCGROUP,DA_NOSUB,   audio_start,     0},
    {"subpicture",DA_PGCGROUP,DA_SUBP,    subattr_group_start, 0},
    {"pgc",       DA_PGCGROUP,DA_PGC,     pgc_start,       pgc_end},
    {"pre",       DA_PGC,     DA_NOSUB,   pre_start,       pre_end},
    {"post",      DA_PGC,     DA_NOSUB,   post_start,      post_end},
    {"button",    DA_PGC,     DA_NOSUB,   button_start,    button_end},
    {"vob",       DA_PGC,     DA_VOB,     vob_start,       vob_end},
    {"subpicture",DA_PGC,     DA_SUBP,    subattr_pgc_start, 0},
    {"cell",      DA_VOB,     DA_NOSUB,   cell_start,      cell_end},
    {"stream",    DA_SUBP,    DA_NOSUB,   stream_start,    stream_end},
    {0,0,0,0,0}
};

static struct elemattr attrs[]={
    {"dvdauthor","dest",dvdauthor_outputdir},
    {"dvdauthor","jumppad",dvdauthor_jumppad},
    {"dvdauthor","allgprm",dvdauthor_allgprm},
    {"dvdauthor","format",dvdauthor_video_format},

    {"menus","lang",menus_lang},

    {"vob","file",vob_file},
    {"vob","chapters",vob_chapters},
    {"vob","pause",vob_pause},

    {"cell","start",cell_parsestart},
    {"cell","end",cell_parseend},
    {"cell","chapter",cell_parsechapter},
    {"cell","program",cell_parseprogram},
    {"cell","pause",cell_pauselen},

    {"button","name",button_name},

    {"video","format",video_format},
    {"video","aspect",video_aspect},
    {"video","resolution",video_resolution},
    {"video","widescreen",video_widescreen},
    {"video","caption",video_caption},

    {"audio","format",audio_format},
    {"audio","quant",audio_quant},
    {"audio","dolby",audio_dolby},
    {"audio","lang",audio_lang},
    {"audio","channels",audio_channels},
    {"audio","samplerate",audio_samplerate},
    {"audio","content",audio_content},

    {"subpicture","lang",subattr_lang},
    {"subpicture","content",subattr_content},
    {"stream","mode",substream_mode},
    {"stream","id",substream_id},

    {"pgc","entry",pgc_entry},
    {"pgc","palette",pgc_palette},
    {"pgc","pause",pgc_pause},
    {0,0,0}
};

static int readdvdauthorxml(const char *xmlfile, const char *fb)
{
    fbase = fb;
    if (!fbase)
      {
        fbase = get_outputdir();
      } /*if*/
    return readxml(xmlfile, elems, attrs);
}

int menu_nav_core(const char *xml,const char *output)
{
    default_video_format=VF_PAL;
    return readdvdauthorxml(xml,output);
}
