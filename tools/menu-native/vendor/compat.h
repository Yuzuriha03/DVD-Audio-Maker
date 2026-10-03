// basic headers
#ifndef _GNU_SOURCE
#define _GNU_SOURCE /* really just for strndup */
#endif

#ifdef HAVE_STDBOOL_H
# include <stdbool.h>
#else
# ifndef HAVE__BOOL
#  ifdef __cplusplus
typedef bool _Bool;
#  else
#   define _Bool signed char
#  endif
# endif
# define bool _Bool
# define false 0
# define true 1
# define __bool_true_false_are_defined 1
#endif

#include <stdio.h>

#ifdef HAVE_STDLIB_H
#include <stdlib.h>
#endif

#ifdef HAVE_STDINT_H
#include <stdint.h>
#endif

#ifdef HAVE_STRINGS_H
#include <strings.h>
#endif

/* ---- bzero() / bcopy()（POSIX 的早期内存函数）----

   ⚠️ MinGW **没有**这两个（string.h / strings.h 里都没有，实测），而
   subreader.c 用了 4 次 bzero()。语义与 memset/memmove 完全等价：
        bzero(p, n)      == memset(p, 0, n)
        bcopy(src,dst,n) == memmove(dst, src, n)   ← 注意参数顺序相反！
   （bcopy 的 dest/src 是反的，容易写错，所以这里一并提供。）

   ⚠️ 用宏是安全的：MinGW 里不存在这两个函数的**声明**，不会出现
   `macro requires 2 arguments` 那种把声明一起改写的问题
   （那个坑参见下面 da_mkdir 的说明）。 */
#ifdef _WIN32
#  include <string.h>
#  define bzero(p, n)        memset((p), 0, (n))
#  define bcopy(src, dst, n) memmove((dst), (src), (n))
#endif

#ifdef HAVE_STRING_H
#include <string.h>
#endif

#ifdef HAVE_UNISTD_H
#include <unistd.h>
#endif

#ifdef HAVE_INTTYPES_H
#include <inttypes.h>
#endif

#ifdef HAVE_MEMORY_H
#include <memory.h>
#endif

#ifdef HAVE_SYS_STAT_H
#include <sys/stat.h>
#endif

#ifdef HAVE_SYS_TYPES_H
#include <sys/types.h>
#endif

#ifdef HAVE_GETOPT_H
#include <getopt.h>
#endif

#ifdef HAVE_IO_H
#include <io.h>
#endif

#ifdef HAVE_ICONV
#include <iconv.h>
#endif

// this doesn't really belong here, but it was easiest
#ifdef HAVE_MAGICK
#define BUILDSPEC_MAGICK " imagemagick"
#else
#ifdef HAVE_GMAGICK
#define BUILDSPEC_MAGICK " graphicsmagick"
#else
#define BUILDSPEC_MAGICK ""
#endif
#endif

#ifdef HAVE_GETOPT_LONG
#define BUILDSPEC_GETOPT " gnugetopt"
#else
#define BUILDSPEC_GETOPT ""
#endif

#ifdef HAVE_ICONV
#define BUILDSPEC_ICONV " iconv"
#else
#define BUILDSPEC_ICONV ""
#endif

#ifdef HAVE_FREETYPE
#define BUILDSPEC_FREETYPE " freetype"
#else
#define BUILDSPEC_FREETYPE ""
#endif

#ifdef HAVE_FRIBIDI
#define BUILDSPEC_FRIBIDI " fribidi"
#else
#define BUILDSPEC_FRIBIDI ""
#endif

#ifdef HAVE_FONTCONFIG
#define BUILDSPEC_FONTCONFIG " fontconfig"
#else
#define BUILDSPEC_FONTCONFIG ""
#endif

#define BUILDSPEC BUILDSPEC_GETOPT BUILDSPEC_MAGICK BUILDSPEC_ICONV BUILDSPEC_FREETYPE BUILDSPEC_FRIBIDI BUILDSPEC_FONTCONFIG

#ifdef HAVE_ICONV

#define ICONV_NULL ((iconv_t)-1)
extern const char * default_charset;
  /* the name of the default character set to use, depending on user's locale settings */

#endif /*HAVE_ICONV*/

void strconcat
  (
    char * dest,
    size_t maxdestlen,
    const char * src
  );
  /* appends null-terminated src onto dest, ensuring length of contents
    of latter (including terminating null) do not exceed maxdestlen. */

unsigned int strtounsigned
  (
    const char * s,
    const char * what /* description of what I'm trying to convert, for error message */
  );
  /* parses s as an unsigned decimal integer, returning its value. Aborts the
    program on error. */

int strtosigned
  (
    const char * s,
    const char * what /* description of what I'm trying to convert, for error message */
  );
  /* parses s as a signed decimal integer, returning its value. Aborts the
    program on error. */

#ifndef HAVE_STRNDUP
char * strndup
  (
    const char * s,
    size_t n
  );
#endif

char * str_extract_until
  (
    const char ** src,
    const char * delim
  );
  /* scans *src, looking for the first occurrence of a character in delim. Returns
    a copy of the prior part of *src if found, and updates *src to point after the
    delimiter character; else returns a copy of the whole of *src, and sets *src
    to NULL. Returns NULL iff *src is NULL. */

void init_locale();
  /* does locale initialization and initializes default_charset. */

char * locale_decode
  (
    const char * localestr
  );
  /* allocates and returns a string containing the UTF-8 representation of
    localestr interpreted according to the user's locale settings. */

#if !HAVE_DECL_O_BINARY
#define O_BINARY 0
#endif

#if defined(HAVE_SETMODE) && HAVE_DECL_O_BINARY
#define win32_setmode setmode
#else
#define win32_setmode(x,y)
#endif

/* ---- Windows/MinGW 缺失的 POSIX 接口 ----

   ⚠️ MinGW 的 mkdir() 来自 <io.h>，**只接一个参数**（Windows 下没有权限位
   这个概念，新目录继承父目录的 ACL），而代码里按 POSIX 习惯写成
   `mkdir(path, 0777)`，直接编译失败（"too many arguments"）。
   fsync() 在 MinGW 里根本不存在，对应的是 _commit()（声明在 <io.h>）。

   ⚠️ 这里**不能**用 `#define mkdir(path,mode) mkdir(path)` 这种宏 ——
   <io.h> 里的**函数声明** `int mkdir(const char *)` 只带一个参数，
   会被那个两参宏当成「参数不够的调用」而报错
   （实测：`macro 'mkdir' requires 2 arguments, but only 1 given`）。
   既然这些头文件是在本文件之后才被 include 的，宏无论如何都会踩到。
   所以改用**垫片函数**（与前面 win32_setmode / HAVE_STRNDUP 的做法一致）。
   下面每个名字在 Windows 上是函数、在别处是宏，调用点写法完全一致。 */
#ifdef _WIN32
int da_mkdir(const char * path, int mode);
int da_fsync(int fd);
#else
#  define da_mkdir(path, mode) mkdir(path, mode)
#  define da_fsync(fd)         fsync(fd)
#endif

/* ---- 字节序函数（subgen.c / spuunmux.c 的 MPEG 包头用）----

   ⚠️ MinGW **没有** <netinet/in.h>，htonl/ntohl/htons/ntohs 定义在
   <winsock2.h> 里，但包含那个头会给可执行文件引入 -lws2_32 依赖
   （实测：不加该库链接报 `undefined reference to __imp_htonl`）。
   这四个函数的语义就是「主机序 ↔ 网络序（大端）」的字节交换，
   而 x86-64 / ARM64 都是小端，直接用 GCC 内建即可 —— **零依赖**。

   ⚠️ 必须放在本头文件（而不是 subgen.c）里：subgen.c 与 spuunmux.c 都用到
   它，而两者都先 include 本文件、再 include <netinet/in.h>，顺序刚好。 */
#ifdef _WIN32
#  define htonl(x) __builtin_bswap32((uint32_t)(x))
#  define ntohl(x) __builtin_bswap32((uint32_t)(x))
#  define htons(x) __builtin_bswap16((uint16_t)(x))
#  define ntohs(x) __builtin_bswap16((uint16_t)(x))
#endif

#define PACKAGE_HEADER(x) PACKAGE_NAME "::" x ", version " PACKAGE_VERSION ".\nBuild options:" BUILDSPEC "\nSend bug reports to <" PACKAGE_BUGREPORT ">\n\n"

#ifndef HAVE_FT2BUILD_H
#define FT_FREETYPE_H <freetype/freetype.h>
#define FT_GLYPH_H <freetype/ftglyph.h>
#endif

enum {VF_NONE=0,VF_NTSC=1,VF_PAL=2}; /* values for videodesc.vformat in da-internal.h as well as other uses */

typedef struct
  {
    unsigned char r, g, b, a;
  } colorspec;

#if HAVE_ICONV && LOCALIZE_FILENAMES

char * localize_filename(const char * pathname);
  /* converts a filename from UTF-8 to localized encoding. */

#else
#    define localize_filename(pathname) (strdup(pathname))
#endif

/* values for vfile.ftype */
#define VFTYPE_FILE 0 /* an actual file I opened */
#define VFTYPE_PIPE 1 /* an actual pipe I opened to/from a child process */
#define VFTYPE_REDIR 2 /* a redirection to/from another already-opened file */
struct vfile /* for keeping track of files opened by varied_open */
  {
    FILE * h; /* do your I/O to/from this */
    int ftype, mode; /* for use by varied_close */
  } /*vfile*/;

struct vfile varied_open
  (
    const char * fname,
    int mode, /* either O_RDONLY or O_WRONLY, nothing more */
    const char * what /* description of what I'm trying to open, for error message */
  );
  /* opens the file fname, which can be an ordinary file name or take one of the
    following special forms:
        "-" -- refers to standard input (if mode is O_RDONLY) or output (if O_WRONLY)
        "&n" -- (n integer) refers to the already-opened file handle n
        "cmd|" -- spawns cmd as a subprocess and reads from its standard output
        "|cmd" -- spawns cmd as a subprocess and writes to its standard input.

    Will abort the process on any errors.
  */

void varied_close(struct vfile vf);
  /* closes a file previously opened by varied_open. */

colorspec parse_color
  (
    const char * colorstr,
    const char * what /* additional explanatory text for error message */
  );
  /* parses colorstr and returns the resulting colour. Will abort the process
    on any errors. */
