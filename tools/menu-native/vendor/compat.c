#include "config.h"

#include "compat.h"

#include <limits.h>
#include <ctype.h>
#include <fcntl.h>
#include <errno.h>
#include "menu-assert.h"
#include <locale.h>
/* MinGW 没有 <langinfo.h>（它属于 POSIX）。它只被下面的 nl_langinfo(CODESET)
   用到，而那里已经有 Windows 分支，故这里守卫掉。 */
#ifndef _WIN32
#include <langinfo.h>
#endif

#ifdef _WIN32
/* 见 compat.h 里的说明：MinGW 的 mkdir() 只接一个参数、且没有 fsync()。 */
int da_mkdir(const char * path, int mode)
{
  (void) mode;   /* Windows 下新目录继承父目录的 ACL，没有权限位可指定 */
  /* The caller and XML use UTF-8; the Rust boundary also maps errors to errno
     so the vendor can continue accepting an existing output directory. */
  extern int menu_rust_mkdir(const char *);
  return menu_rust_mkdir(path);
}

int da_fsync(int fd)
{
  return _commit(fd);
}
#endif

/*
    Useful string stuff
*/


unsigned int strtounsigned
  (
    const char * s,
    const char * what /* description of what I'm trying to convert, for error message */
  )
  /* parses s as an unsigned decimal integer, returning its value. Aborts the
    program on error. */
  {
    char * s_end;
    unsigned long result;
    errno = 0;
    result = strtoul(s, &s_end, 10);
    if (errno == 0)
      {
        if (*s_end != '\0')
          {
            errno = EINVAL;
          }
        else if (result > UINT_MAX)
          {
            errno = ERANGE;
          }
      } /*if*/
    if (errno != 0)
      {
        fprintf(stderr, "ERR:  %d converting %s \"%s\" -- %s\n", errno, what, s, strerror(errno));
        exit(1);
      } /*if*/
    return result;
  } /*strtounsigned*/


#ifndef HAVE_STRNDUP
char * strndup
  (
    const char * s,
    size_t n
  )
  {
    char * result;
    size_t l = strlen(s);
    if (l > n)
      {
        l = n;
      } /*if*/
    result = malloc(l + 1);
    memcpy(result, s, l);
    result[l] = 0;
    return
        result;
  } /*strndup*/
#endif /*HAVE_STRNDUP*/

char * str_extract_until
  (
    const char ** src,
    const char * delim
  )
  /* scans *src, looking for the first occurrence of a character in delim. Returns
    a copy of the prior part of *src if found, and updates *src to point after the
    delimiter character; else returns a copy of the whole of *src, and sets *src
    to NULL. Returns NULL iff *src is NULL. */
  {
    char * result = NULL; /* to begin with */
    if (*src != NULL)
      {
        const size_t pos = strcspn(*src, delim);
        if (pos < strlen(*src))
          {
            result = strndup(*src, pos);
            *src = *src + pos + strspn(*src + pos, delim);
          }
        else
          {
            result = strdup(*src);
            *src = NULL;
          } /*if*/
      } /*if*/
    return
        result;
  } /*str_extract_until*/

/*
    locale stuff
*/

#ifdef HAVE_ICONV

const char *
    default_charset;
static char
    default_charset_buf[32]; /* big enough? */
static iconv_t
    from_locale = ICONV_NULL;

#endif /*HAVE_ICONV*/



#if HAVE_ICONV && LOCALIZE_FILENAMES

static iconv_t
    to_locale = ICONV_NULL;

char * localize_filename(const char * pathname)
  /* converts a filename from UTF-8 to localized encoding. */
  {
    char * result;
    size_t inlen, outlen;
    char * resultnext;
    if (to_locale == ICONV_NULL)
      {
        fprintf(stderr, "INFO: Converting filenames to %s\n", default_charset);
        to_locale = iconv_open(default_charset, "UTF-8");
        if (to_locale == ICONV_NULL)
          {
            fprintf(stderr, "ERR:  Cannot convert from UTF-8 to charset \"%s\"\n", default_charset);
            exit(1);
          } /*if*/
      } /*if*/
    inlen = strlen(pathname);
    outlen = inlen * 5; /* should be enough? */
    result = malloc(outlen);
    resultnext = result;
    if (iconv(to_locale, (char **)&pathname, &inlen, &resultnext, &outlen) < 0)
      {
        fprintf
          (
            stderr,
            "ERR:  Error %d -- %s encoding pathname \"%s\"\n",
            errno, strerror(errno), pathname
          );
        exit(1);
      } /*if*/
    assert(outlen != 0); /* there will be room for ... */
    *resultnext++ = 0; /* ... terminating null */
    result = realloc(result, resultnext - result); /* free unneeded memory */
    return result;
  } /*localize_filename*/

#endif

/*
    vfiles
*/

struct vfile varied_open
  (
    const char * fname,
    int mode, /* either O_RDONLY or O_WRONLY, nothing more */
    const char * what /* description of what I'm trying to open, for error message */
  )
  {
    struct vfile vf;
    if(!fname || !*fname || !strcmp(fname,"-") || fname[0]=='|' || fname[strlen(fname)-1]=='|' || fname[0]=='&') {
        fprintf(stderr,"ERR: menu input must be a file path\n");exit(1);
    }
    vf.ftype=VFTYPE_FILE;
    vf.h=fopen(fname,mode==O_RDONLY ? "rb" : "wb");
    if (vf.h == NULL)
      {
        fprintf(stderr, "ERR:  %d opening %s \"%s\" -- %s\n", errno, what, fname, strerror(errno));
        exit(1);
      } /*if*/
    vf.mode = mode;
    return vf;
  } /*varied_open*/

void varied_close(struct vfile vf)
  {
    if (vf.mode == O_WRONLY)
      {
      /* check for errors on final write before closing */
        if (fflush(vf.h) != 0)
          {
            fprintf(stderr, "ERR:  %d on fflush -- %s\n", errno, strerror(errno));
            exit(1);
          } /*if*/
      } /*if*/
    switch (vf.ftype)
      {
    case VFTYPE_FILE:
        fclose(vf.h);
    break;
    case VFTYPE_PIPE:
        exit(1);
    break;
    case VFTYPE_REDIR:
    default:
      /* nothing to do */
    break;
      } /*switch*/
  } /*varied_close*/


