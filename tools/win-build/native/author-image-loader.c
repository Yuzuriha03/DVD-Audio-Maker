/* Link into dvda-author: image commands run inside this process, no helper EXE. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <wchar.h>

typedef int (__cdecl *ImageCommand)(const char *);
static ImageCommand command_entry;

int dvda_image_command(const char *arguments)
{
    if (!command_entry) {
        wchar_t path[32768];
        if (!GetModuleFileNameW(NULL, path, 32768)) return -1;
        wchar_t *last = wcsrchr(path,L'\\'); if (!last) return -1; *last=0;
        last = wcsrchr(path,L'\\'); if (!last) return -1; *last=0;
        if (wcslen(path)+32 >= 32768) return -1;
        wcscat(path,L"\\image-native\\dvda-image.dll");
        HMODULE module = LoadLibraryExW(path,NULL,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (module) command_entry=(ImageCommand)(uintptr_t)GetProcAddress(module,"dvda_image_command");
        if (!command_entry) {
            fprintf(stderr,"[ERR] Cannot load bundled x64 image runtime (Windows error %lu).\n",GetLastError());
            return -1;
        }
    }
    return command_entry(arguments);
}
