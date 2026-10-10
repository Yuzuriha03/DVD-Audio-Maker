#ifndef DVDA_SESSION_IMPLEMENTATION
#define DVDA_SESSION_IMPLEMENTATION
#endif
#include "session.h"
#include <setjmp.h>
#include <stdarg.h>
#include <errno.h>
#include <stdbool.h>
extern bool parser_err,parser_acceptbody;
extern char *parser_body;

static HANDLE heap;
static jmp_buf failure;
#ifndef DVDA_DIRECT_LINK
static int consumed;
#else
void menu_vendor_reset(void);
#endif
static DvdaReadRgba pixels;
extern int menu_rust_own(void *,int);
extern void menu_rust_disown(void *,int);
extern int menu_rust_take(void **,int *);
static jmp_buf *failure_target;
static void own(void *object,int kind) {
    if(menu_rust_own(object,kind)) {
        if(kind==1)fclose(object);
        else if(kind==2)_close((int)(intptr_t)object);
        else if(kind==3)closedir(object);
        else if(kind==4)IUnknown_Release((IUnknown *)object);
        menu_exit(1);
    }
}
static void disown(void *object,int kind) { menu_rust_disown(object,kind); }
_Noreturn void menu_exit(int status) { longjmp(*failure_target,status ? status : 1); }
_Noreturn void menu_abort(void) { fputs("ERR: menu core invariant failed\n",stderr);menu_exit(1); }
void *menu_malloc(size_t size) {
    void *p=HeapAlloc(heap,0,size ? size : 1);if(!p)menu_exit(1);return p;
}
void *menu_calloc(size_t n,size_t size) {
    if(size && n>SIZE_MAX/size)menu_exit(1);
    void *p=menu_malloc(n*size);memset(p,0,n*size);return p;
}
void menu_free(void *p) { if(p && !HeapFree(heap,0,p))menu_abort(); }
void *menu_realloc(void *p,size_t size) {
    if(!p)return menu_malloc(size);
    if(!size){menu_free(p);return NULL;}
    void *q=HeapReAlloc(heap,0,p,size);if(!q)menu_exit(1);return q;
}
char *menu_strndup(const char *s,size_t n) {
    size_t len=strnlen(s,n);char *p=menu_malloc(len+1);memcpy(p,s,len);p[len]=0;return p;
}
char *menu_strdup(const char *s) { return menu_strndup(s,strlen(s)); }
static wchar_t *wide(const char *s) {
    if(!s)menu_exit(1);
    int n=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s,-1,NULL,0);
    if(!n)menu_exit(1);
    wchar_t *p=menu_malloc((size_t)n*sizeof(*p));
    MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s,-1,p,n);return p;
}
FILE *menu_fopen(const char *path,const char *mode) {
    wchar_t *p=wide(path),*m=wide(mode);FILE *f=_wfopen(p,m);
    menu_free(p);menu_free(m);if(f)own(f,1);return f;
}
int menu_fclose(FILE *f) {
    int result=fclose(f);disown(f,1);if(result)menu_exit(1);return result;
}
int menu_open(const char *path,int flags,...) {
    int mode=0;if(flags&O_CREAT){va_list a;va_start(a,flags);mode=va_arg(a,int);va_end(a);}
    wchar_t *p=wide(path);int fd=_wopen(p,flags|O_BINARY,mode);menu_free(p);
    if(fd>=0)own((void *)(intptr_t)fd,2);return fd;
}
int menu_close(int fd) {
    int result=_close(fd);disown((void *)(intptr_t)fd,2);if(result)menu_exit(1);return result;
}
DIR *menu_opendir(const char *path) { DIR *p=opendir(path);if(p)own(p,3);return p; }
int menu_closedir(DIR *p) { int result=closedir(p);disown(p,3);return result; }
void menu_own_com(IUnknown *p) { own(p,4); }
void menu_release_com(IUnknown *p) { if(p){IUnknown_Release(p);disown(p,4);} }
int menu_read_rgba(const char *p,unsigned char *b,size_t n,unsigned *w,unsigned *h) {
    return pixels ? pixels(p,b,n,w,h) : -1;
}
/* Configuration is supplied by the caller/XML, never ~/.dvdauthorrc. */
int get_video_format(void) { return 2; }
char *get_outputdir(void) { return NULL; }
#ifdef DVDA_SUBPICTURE
int menu_spu_core(const char *,int,int);
#else
int menu_nav_core(const char *,const char *);
#endif
#ifdef DVDA_DIRECT_LINK
int dvda_menu_run(const DvdaMenuRequest *request) {
#else
__declspec(dllexport) int dvda_menu_run(const DvdaMenuRequest *request) {
#endif
    volatile int result=1;
#ifndef DVDA_DIRECT_LINK
    if(consumed++)return 1;
#endif
    if(!request || request->size!=sizeof(*request) || !request->xml || !request->output)return 1;
#ifdef DVDA_DIRECT_LINK
    menu_vendor_reset();
#endif
    heap=HeapCreate(0,0,0);if(!heap)return 1;
    pixels=request->read_rgba;
    failure_target=&failure;
    if(setjmp(failure)==0) {
#ifdef DVDA_SUBPICTURE
        int input=menu_open(request->input,O_RDONLY);
        if(input<0)menu_exit(1);
        struct _stat64 info;
        if(_fstat64(input,&info) || info.st_size<=0 || info.st_size%2048)menu_exit(1);
        int output=menu_open(request->output,O_WRONLY|O_CREAT|O_TRUNC,_S_IREAD|_S_IWRITE);
        if(output<0)menu_exit(1);
        result=menu_spu_core(request->xml,input,output);
#else
        result=menu_nav_core(request->xml,request->output);
#endif
    }
    void *object;int kind;
    while(menu_rust_take(&object,&kind)) {
        if(kind==1 && fclose(object))result=1;
        else if(kind==2 && _close((int)(intptr_t)object))result=1;
        else if(kind==3)closedir(object);
        else if(kind==4)IUnknown_Release((IUnknown *)object);
    }
    HeapDestroy(heap);heap=NULL;pixels=NULL;failure_target=NULL;
    parser_err=false;parser_acceptbody=false;parser_body=NULL;
#ifdef DVDA_DIRECT_LINK
    menu_vendor_reset();
#endif
    return result;
}


#include "compat.h"
#include "readxml.h"
bool parser_err=false,parser_acceptbody=false;
char *parser_body;
extern int menu_rust_readxml(const char *,const struct elemdesc *,const struct elemattr *);
extern int menu_rust_boolean(const char *);
/* Each island owns its jump target. The vendor stack unwinds only through C,
 * returning a status to Rust; the outer transaction target is never crossed. */
int menu_callback(parserfunc callback) {
    jmp_buf island; jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0) { if(callback)callback();result=parser_err ? 1 : 0; }
    failure_target=previous;return result;
}
int menu_attribute(attrfunc callback,const char *value) {
    jmp_buf island; jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0) { if(callback)callback(value);result=parser_err ? 1 : 0; }
    failure_target=previous;return result;
}
int menu_body(const char *value) {
    jmp_buf island; jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0) {
        menu_free(parser_body);parser_body=NULL;
        if(value)parser_body=menu_strdup(value);else parser_acceptbody=false;
        result=0;
    }
    failure_target=previous;return result;
}
int menu_accepts_body(void) { return parser_acceptbody; }
int readxml(const char *path,const struct elemdesc *elems,const struct elemattr *attrs) {
    return menu_rust_readxml(path,elems,attrs);
}
bool xml_ison(const char *value,const char *attr) {
    int result=menu_rust_boolean(value);
    if(result<0) { fprintf(stderr,"ERR: invalid boolean for %s\n",attr);menu_exit(1); }
    return result!=0;
}
