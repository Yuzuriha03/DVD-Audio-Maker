#ifndef DVDA_MENU_API_H
#define DVDA_MENU_API_H
#include <stddef.h>
typedef int (*DvdaReadRgba)(const char *, unsigned char *, size_t, unsigned *, unsigned *);
/* One transaction per freshly loaded module. The author unloads the DLL after
 * this call, resetting all legacy algorithm globals. No process or stdio
 * redirection is involved. All owned memory and files are released first. */
typedef struct DvdaMenuRequest {
    unsigned size;
    const char *xml;
    const char *input;
    const char *output;
    DvdaReadRgba read_rgba;
} DvdaMenuRequest;
typedef int (*DvdaMenuRun)(const DvdaMenuRequest *);
#endif
