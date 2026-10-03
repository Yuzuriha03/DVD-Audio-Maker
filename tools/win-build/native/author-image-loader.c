/* Link into dvda-author: image commands run inside this process, no helper EXE. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <wchar.h>

typedef int (__cdecl *ImageCommand)(const char *);
typedef int (__cdecl *ImageY4mWriter)(const char *, const char *, const char *, const char *);
static ImageCommand command_entry;
static ImageY4mWriter y4m_entry;
static HMODULE image_module;

static int load_image_runtime(void)
{
    if (!image_module) {
        wchar_t path[32768];
        if (!GetModuleFileNameW(NULL, path, 32768)) return -1;
        wchar_t *last = wcsrchr(path,L'\\'); if (!last) return -1; *last=0;
        last = wcsrchr(path,L'\\'); if (!last) return -1; *last=0;
        if (wcslen(path)+32 >= 32768) return -1;
        wcscat(path,L"\\image-native\\dvda-image.dll");
        image_module = LoadLibraryExW(path,NULL,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (image_module) {
            command_entry=(ImageCommand)(uintptr_t)GetProcAddress(image_module,"dvda_image_command");
            y4m_entry=(ImageY4mWriter)(uintptr_t)GetProcAddress(image_module,"dvda_image_write_y4m");
        }
        if (!command_entry || !y4m_entry) {
            fprintf(stderr,"[ERR] Cannot load bundled x64 image runtime (Windows error %lu).\n",GetLastError());
            return -1;
        }
    }
    return 0;
}

int dvda_image_command(const char *arguments)
{
    if (load_image_runtime() != 0) return -1;
    return command_entry(arguments);
}

int dvda_image_write_y4m(const char *input, const char *output, const char *frame_rate, const char *aspect)
{
    if (load_image_runtime() != 0) return -1;
    return y4m_entry(input, output, frame_rate, aspect);
}
