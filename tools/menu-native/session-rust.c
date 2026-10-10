/* Only C failure-jump islands, varargs decoding and status promotion remain.
 * Rust hooks MUST return before menu_exit can be called. */
#ifndef DVDA_SESSION_IMPLEMENTATION
#define DVDA_SESSION_IMPLEMENTATION
#endif
#include "session.h"
#include <setjmp.h>
#include <stdarg.h>
#include <stdbool.h>
#include "compat.h"
#include "readxml.h"

bool parser_err=false,parser_acceptbody=false;
char *parser_body;
static jmp_buf *failure_target;
typedef int (*MenuCore)(const char *,const char *,int,int);
typedef void (*MenuReset)(void);
extern int menu_rust_run(const DvdaMenuRequest *,int,MenuCore,MenuReset,bool *,bool *,char **);
extern int menu_rust_malloc(size_t,void **);
extern int menu_rust_calloc(size_t,size_t,void **);
extern int menu_rust_realloc(void *,size_t,void **);
extern int menu_rust_free(void *);
extern int menu_rust_strndup(const char *,size_t,char **);
extern int menu_rust_fopen(const char *,const char *,void **);
extern int menu_rust_fclose(void *);
extern int menu_rust_open(const char *,int,int,int *);
extern int menu_rust_close(int);
extern int menu_rust_opendir(const char *,void **);
extern int menu_rust_readdir(void *,struct dirent **);
extern int menu_rust_closedir(void *);
extern int menu_rust_own_com(void *);
extern int menu_rust_release_com(void *);
extern int menu_rust_read_rgba(const char *,unsigned char *,size_t,unsigned *,unsigned *);
extern int menu_rust_body(const char *,char **,bool *);
extern int menu_rust_readxml(const char *,const struct elemdesc *,const struct elemattr *);
extern int menu_rust_boolean(const char *);

_Noreturn void menu_exit(int status) {
    longjmp(*failure_target,status ? status : 1);
}
_Noreturn void menu_abort(void) {
    fputs("ERR: menu core invariant failed\n",stderr);
    menu_exit(1);
}
void *menu_malloc(size_t size) {
    void *p=NULL;if(menu_rust_malloc(size,&p))menu_exit(1);return p;
}
void *menu_calloc(size_t n,size_t size) {
    void *p=NULL;if(menu_rust_calloc(n,size,&p))menu_exit(1);return p;
}
void *menu_realloc(void *p,size_t size) {
    void *q=NULL;if(menu_rust_realloc(p,size,&q))menu_exit(1);return q;
}
void menu_free(void *p) { if(menu_rust_free(p))menu_abort(); }
char *menu_strndup(const char *s,size_t n) {
    char *p=NULL;if(menu_rust_strndup(s,n,&p))menu_exit(1);return p;
}
char *menu_strdup(const char *s) { return menu_strndup(s,SIZE_MAX); }
FILE *menu_fopen(const char *path,const char *mode) {
    void *p=NULL;if(menu_rust_fopen(path,mode,&p))menu_exit(1);return p;
}
int menu_fclose(FILE *f) {
    int result=menu_rust_fclose(f);if(result)menu_exit(1);return result;
}
int menu_open(const char *path,int flags,...) {
    int mode=0,fd=-1;
    if(flags&O_CREAT){va_list a;va_start(a,flags);mode=va_arg(a,int);va_end(a);}
    if(menu_rust_open(path,flags,mode,&fd))menu_exit(1);return fd;
}
int menu_close(int fd) {
    int result=menu_rust_close(fd);if(result)menu_exit(1);return result;
}
DIR *menu_opendir(const char *path) {
    void *p=NULL;if(menu_rust_opendir(path,&p))menu_exit(1);return p;
}
struct dirent *menu_readdir(DIR *directory) {
    struct dirent *p=NULL;if(menu_rust_readdir(directory,&p))menu_exit(1);return p;
}
int menu_closedir(DIR *directory) { return menu_rust_closedir(directory); }
void menu_own_com(IUnknown *p) { if(menu_rust_own_com(p))menu_exit(1); }
void menu_release_com(IUnknown *p) { if(menu_rust_release_com(p))menu_exit(1); }
int menu_read_rgba(const char *p,unsigned char *b,size_t n,unsigned *w,unsigned *h) {
    return menu_rust_read_rgba(p,b,n,w,h);
}

#ifdef DVDA_SUBPICTURE
int menu_spu_core(const char *,int,int);
#else
int menu_nav_core(const char *,const char *);
#endif
#ifdef DVDA_DIRECT_LINK
void menu_vendor_reset(void);
#endif
/* This island catches exits from vendor frames, never from a Rust hook. */
static int menu_core_island(const char *xml,const char *output,int input,int outfd) {
    jmp_buf island;jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0) {
#ifdef DVDA_SUBPICTURE
        result=menu_spu_core(xml,input,outfd);
#else
        result=menu_nav_core(xml,output);
#endif
    }
    failure_target=previous;return result;
}
#ifdef DVDA_DIRECT_LINK
int dvda_menu_run(const DvdaMenuRequest *request) {
#else
__declspec(dllexport) int dvda_menu_run(const DvdaMenuRequest *request) {
#endif
    return menu_rust_run(request,
#ifdef DVDA_SUBPICTURE
        0,
#else
        1,
#endif
        menu_core_island,
#ifdef DVDA_DIRECT_LINK
        menu_vendor_reset,
#else
        NULL,
#endif
        &parser_err,&parser_acceptbody,&parser_body);
}
/* Rust XML dispatch enters vendor callbacks only inside a local C island. */
int menu_callback(parserfunc callback) {
    jmp_buf island;jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0){if(callback)callback();result=parser_err ? 1 : 0;}
    failure_target=previous;return result;
}
int menu_attribute(attrfunc callback,const char *value) {
    jmp_buf island;jmp_buf *previous=failure_target;
    volatile int result=1;failure_target=&island;
    if(setjmp(island)==0){if(callback)callback(value);result=parser_err ? 1 : 0;}
    failure_target=previous;return result;
}
int menu_body(const char *value) { return menu_rust_body(value,&parser_body,&parser_acceptbody); }
int menu_accepts_body(void) { return parser_acceptbody; }
int readxml(const char *path,const struct elemdesc *elems,const struct elemattr *attrs) {
    return menu_rust_readxml(path,elems,attrs);
}
bool xml_ison(const char *value,const char *attr) {
    int result=menu_rust_boolean(value);
    if(result<0){fprintf(stderr,"ERR: invalid boolean for %s\n",attr);menu_exit(1);}
    return result!=0;
}
