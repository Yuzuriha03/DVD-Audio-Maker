#ifndef DVDA_MSVC_UNISTD_H
#define DVDA_MSVC_UNISTD_H
#include <io.h>
#include <direct.h>
#include <process.h>
#include <sys/types.h>
#define read _read
#define write _write
#define lseek _lseeki64
#define unlink _unlink
#define rmdir _rmdir
#define fileno _fileno
#define isatty _isatty
#define setmode _setmode
#define fdopen _fdopen
#define fseeko _fseeki64
#define ftello _ftelli64
#endif
