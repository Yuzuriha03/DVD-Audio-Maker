/* Forced include for migrated C modules only; never for third-party DLLs. */
#ifndef DVDA_MENU_SESSION_H
#define DVDA_MENU_SESSION_H
#include "config.h"
#define WIN32_LEAN_AND_MEAN
#define COBJMACROS
#include <windows.h>
#include <objbase.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>
#include <stdint.h>
#include <unistd.h>
#include <io.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <dirent.h>
#include <assert.h>
#include "menu-api.h"
/* Rust readdir uses this MinGW ABI. These checks emit no production code. */
_Static_assert(sizeof(struct dirent)==268,"Rust dirent size mismatch");
_Static_assert(_Alignof(struct dirent)==4,"Rust dirent alignment mismatch");
_Static_assert(offsetof(struct dirent,d_ino)==0,"Rust dirent inode mismatch");
_Static_assert(offsetof(struct dirent,d_reclen)==4,"Rust dirent record length mismatch");
_Static_assert(offsetof(struct dirent,d_namlen)==6,"Rust dirent name length mismatch");
_Static_assert(offsetof(struct dirent,d_name)==8,"Rust dirent name mismatch");
_Static_assert(sizeof(((struct dirent *)0)->d_name)==260,"Rust dirent name capacity mismatch");
void *menu_malloc(size_t);
void *menu_calloc(size_t,size_t);
void *menu_realloc(void *,size_t);
void menu_free(void *);
char *menu_strdup(const char *);
char *menu_strndup(const char *,size_t);
FILE *menu_fopen(const char *,const char *);
int menu_fclose(FILE *);
int menu_open(const char *,int,...);
int menu_close(int);
DIR *menu_opendir(const char *);
struct dirent *menu_readdir(DIR *);
int menu_closedir(DIR *);
void menu_own_com(IUnknown *);
void menu_release_com(IUnknown *);
int menu_read_rgba(const char *,unsigned char *,size_t,unsigned *,unsigned *);
_Noreturn void menu_exit(int);
_Noreturn void menu_abort(void);
#ifndef DVDA_SESSION_IMPLEMENTATION
#define malloc menu_malloc
#define calloc menu_calloc
#define realloc menu_realloc
#define free menu_free
#define strdup menu_strdup
#define strndup menu_strndup
#define fopen menu_fopen
#define fclose menu_fclose
#define open menu_open
#define close menu_close
#define opendir menu_opendir
#define readdir menu_readdir
#define closedir menu_closedir
#define exit menu_exit
#define abort menu_abort
#undef assert
#define assert(c) ((c) ? (void)0 : menu_abort())
#endif
#endif
