/* Link into dvda-author: image commands run inside this process, no helper EXE. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <wchar.h>
#include "../../menu-native/menu-api.h"

typedef int (__cdecl *ImageCommand)(const char *);
typedef int (__cdecl *ImageY4mWriter)(const char *, const char *, const char *, const char *);
static ImageCommand command_entry;
static ImageY4mWriter y4m_entry;
static DvdaReadRgba rgba_entry;
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
            rgba_entry=(DvdaReadRgba)(uintptr_t)GetProcAddress(image_module,"dvda_image_read_rgba");
        }
        if (!command_entry || !y4m_entry || !rgba_entry) {
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

static int menu_call(const wchar_t *name,const char *xml,const char *input,const char *output)
{
    wchar_t path[32768];
    if(load_image_runtime() || !GetModuleFileNameW(NULL,path,32768))return -1;
    wchar_t *last=wcsrchr(path,L'\\');if(!last)return -1;last[1]=0;
    if(wcslen(path)+wcslen(name)>=32768)return -1;
    wcscat(path,name);
    HMODULE module=LoadLibraryExW(path,NULL,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
    if(!module){fprintf(stderr,"[ERR] Cannot load menu library (%lu).\n",GetLastError());return -1;}
    DvdaMenuRun run=(DvdaMenuRun)(uintptr_t)GetProcAddress(module,"dvda_menu_run");
    DvdaMenuRequest request={sizeof(request),xml,input,output,rgba_entry};
    int result=run ? run(&request) : -1;
    FreeLibrary(module);
    return result;
}
int dvda_menu_subpictures(const char *xml,const char *input,const char *output)
{
    return menu_call(L"dvda-menu-spu.dll",xml,input,output);
}
int dvda_menu_navigation(const char *xml,const char *output)
{
    return menu_call(L"dvda-menu-nav.dll",xml,NULL,output);
}
