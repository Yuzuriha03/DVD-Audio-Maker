#ifndef DVDA_SESSION_IMPLEMENTATION
#define DVDA_SESSION_IMPLEMENTATION
#endif
#include "session.h"
#include <setjmp.h>
#include <stdarg.h>
#include <errno.h>

static HANDLE heap;
static jmp_buf failure;
static int consumed;
static DvdaReadRgba pixels;
typedef struct Resource { struct Resource *next; void *object; int kind; } Resource;
static Resource *resources;
static void own(void *object,int kind) {
    Resource *r=HeapAlloc(heap,0,sizeof(*r));
    if (!r) {
        if(kind==1)fclose(object);
        else if(kind==2)_close((int)(intptr_t)object);
        else if(kind==3)closedir(object);
        else if(kind==4)IUnknown_Release((IUnknown *)object);
        menu_exit(1);
    }
    r->object=object;r->kind=kind;r->next=resources;resources=r;
}
static void disown(void *object,int kind) {
    Resource **r=&resources;
    while (*r) { if ((*r)->object==object && (*r)->kind==kind) {
        Resource *old=*r;*r=old->next;HeapFree(heap,0,old);return;
    } r=&(*r)->next; }
}
_Noreturn void menu_exit(int status) { longjmp(failure,status ? status : 1); }
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
__declspec(dllexport) int dvda_menu_run(const DvdaMenuRequest *request) {
    volatile int result=1;
    if(consumed++ || !request || request->size!=sizeof(*request) || !request->xml || !request->output)return 1;
    heap=HeapCreate(0,0,0);if(!heap)return 1;
    pixels=request->read_rgba;
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
    while(resources) {
        Resource *r=resources;resources=r->next;
        if(r->kind==1 && fclose(r->object))result=1;
        else if(r->kind==2 && _close((int)(intptr_t)r->object))result=1;
        else if(r->kind==3)closedir(r->object);
        else if(r->kind==4)IUnknown_Release((IUnknown *)r->object);
    }
    HeapDestroy(heap);heap=NULL;pixels=NULL;
    return result;
}
