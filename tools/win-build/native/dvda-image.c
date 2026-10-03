/* Purpose-limited in-process ImageMagick bridge. Windows x64, no child tools. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <MagickWand/MagickWand.h>
#include <MagickWand/magick-cli.h>
#include <MagickWand/convert.h>
#include <MagickWand/identify.h>
#include <MagickWand/mogrify.h>
#include <MagickCore/client.h>
#include <MagickCore/monitor.h>
#include <MagickCore/type.h>

typedef void (__cdecl *Emit)(void *, int, const char *);
typedef int (__cdecl *Cancel)(void *);
typedef struct Call { Emit emit; Cancel cancel; void *state; } Call;
static INIT_ONCE once = INIT_ONCE_STATIC_INIT;
static CRITICAL_SECTION gate;
static _Thread_local Call *active;

static char *utf8(const wchar_t *text)
{
    int size = WideCharToMultiByte(CP_UTF8, 0, text, -1, NULL, 0, NULL, NULL);
    char *value = malloc(size);
    if (value) WideCharToMultiByte(CP_UTF8, 0, text, -1, value, size, NULL, NULL);
    return value;
}
static wchar_t *wide(const char *text)
{
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, NULL, 0);
    if (!size) return NULL;
    wchar_t *value = calloc(size, sizeof(wchar_t));
    if (value) MultiByteToWideChar(CP_UTF8, 0, text, -1, value, size);
    return value;
}
static void emit_text(Call *call, int stream, const char *text)
{
    if (call->emit) call->emit(call->state, stream, text ? text : "");
    else fputs(text ? text : "", stream == 2 ? stderr : stdout);
}
static void report(ExceptionType severity, const char *reason, const char *description)
{
    (void)severity;
    if (!active) return;
    emit_text(active, 2, reason); if (description && *description) { emit_text(active, 2, ": "); emit_text(active, 2, description); }
    emit_text(active, 2, "\n");
}
static BOOL CALLBACK initialize(PINIT_ONCE unused, PVOID data, PVOID *context)
{
    (void)unused; (void)data; (void)context;
    HMODULE module = NULL; wchar_t path[32768];
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        (LPCWSTR)(uintptr_t)&initialize, &module) || !GetModuleFileNameW(module, path, 32768)) return FALSE;
    char *filename = utf8(path); if (!filename) return FALSE;
    wchar_t *last = wcsrchr(path, L'\\'); if (!last) { free(filename); return FALSE; } *last = 0;
    SetEnvironmentVariableW(L"MAGICK_CONFIGURE_PATH", path);
    SetEnvironmentVariableW(L"MAGICK_THREAD_LIMIT", L"1");
    char *directory = utf8(path); if (!directory) { free(filename); return FALSE; }
    _putenv_s("MAGICK_CONFIGURE_PATH", directory); free(directory);
    _putenv_s("MAGICK_THREAD_LIMIT", "1");
    MagickCoreGenesis(filename, MagickFalse); free(filename);
    SetWarningHandler(report); SetErrorHandler(report); SetFatalErrorHandler(report);
    InitializeCriticalSection(&gate);
    return TRUE;
}
static MagickBooleanType monitor(const char *tag, const MagickOffsetType offset, const MagickSizeType span, void *opaque)
{
    (void)tag; (void)offset; (void)span;
    Call *call = opaque;
    return call->cancel && call->cancel(call->state) ? MagickFalse : MagickTrue;
}

static unsigned char clamp_byte(int value)
{
    if (value < 0) return 0;
    if (value > 255) return 255;
    return (unsigned char)value;
}

static unsigned char luma(unsigned int red, unsigned int green, unsigned int blue)
{
    return clamp_byte(((66 * (int)red + 129 * (int)green + 25 * (int)blue + 128) >> 8) + 16);
}

static unsigned char chroma_u(unsigned int red, unsigned int green, unsigned int blue)
{
    return clamp_byte(((-38 * (int)red - 74 * (int)green + 112 * (int)blue + 128) >> 8) + 128);
}

static unsigned char chroma_v(unsigned int red, unsigned int green, unsigned int blue)
{
    return clamp_byte(((112 * (int)red - 94 * (int)green - 18 * (int)blue + 128) >> 8) + 128);
}

/* Read a menu still in process for the native MPEG-2 encoder. */
__declspec(dllexport) int __cdecl dvda_image_write_y4m(const char *input, const char *output,
                                                       const char *frame_rate, const char *aspect)
{
    if (!input || !*input || !output || !*output || !frame_rate || !aspect ||
        (strcmp(frame_rate, "25") && strcmp(frame_rate, "30")) ||
        (strcmp(aspect, "1:1") && strcmp(aspect, "4:3") && strcmp(aspect, "16:9") && strcmp(aspect, "2.21:1")))
        return -1;
    if (!InitOnceExecuteOnce(&once, initialize, NULL, NULL)) return -1;

    while (!TryEnterCriticalSection(&gate)) Sleep(5);
    Call call = {NULL, NULL, NULL};
    active = &call;
    int result = -1;
    MagickWand *wand = NewMagickWand();
    unsigned char *rgb = NULL, *y_plane = NULL, *u_plane = NULL, *v_plane = NULL;
    wchar_t *output_path = NULL;
    FILE *file = NULL;
    size_t width = 0, height = 0, pixels = 0, chroma = 0;

    if (!wand || MagickReadImage(wand, input) == MagickFalse ||
        MagickTransformImageColorspace(wand, sRGBColorspace) == MagickFalse)
    {
        ExceptionType severity = UndefinedException;
        char *message = wand ? MagickGetException(wand, &severity) : NULL;
        report(severity, "Could not decode menu image", message);
        if (message) MagickRelinquishMemory(message);
        goto done;
    }
    width = MagickGetImageWidth(wand);
    height = MagickGetImageHeight(wand);
    if (!width || !height || (width & 1) || (height & 1) || width > 16384 || height > 16384 ||
        width > SIZE_MAX / height || width * height > SIZE_MAX / 3)
    {
        report(ErrorException, "Menu image must have nonzero even dimensions no larger than 16384 pixels", NULL);
        goto done;
    }
    pixels = width * height;
    chroma = pixels / 4;
    rgb = malloc(pixels * 3);
    y_plane = malloc(pixels);
    u_plane = malloc(chroma);
    v_plane = malloc(chroma);
    if (!rgb || !y_plane || !u_plane || !v_plane ||
        MagickExportImagePixels(wand, 0, 0, width, height, "RGB", CharPixel, rgb) == MagickFalse)
    {
        report(ErrorException, "Could not convert menu image pixels", NULL);
        goto done;
    }

    for (size_t row = 0; row < height; ++row)
        for (size_t column = 0; column < width; ++column)
        {
            const unsigned char *pixel = rgb + (row * width + column) * 3;
            y_plane[row * width + column] = luma(pixel[0], pixel[1], pixel[2]);
        }
    for (size_t row = 0; row < height; row += 2)
        for (size_t column = 0; column < width; column += 2)
        {
            unsigned int red = 0, green = 0, blue = 0;
            for (size_t dy = 0; dy < 2; ++dy)
                for (size_t dx = 0; dx < 2; ++dx)
                {
                    const unsigned char *pixel = rgb + ((row + dy) * width + column + dx) * 3;
                    red += pixel[0]; green += pixel[1]; blue += pixel[2];
                }
            red = (red + 2) / 4; green = (green + 2) / 4; blue = (blue + 2) / 4;
            size_t offset = (row / 2) * (width / 2) + column / 2;
            u_plane[offset] = chroma_u(red, green, blue);
            v_plane[offset] = chroma_v(red, green, blue);
        }

    output_path = wide(output);
    if (!output_path || !(file = _wfopen(output_path, L"wb")))
    {
        report(ErrorException, "Could not open YUV4MPEG2 output", output);
        goto done;
    }
    if (fprintf(file, "YUV4MPEG2 W%zu H%zu F%s Ip A%s C420jpeg\nFRAME\n",
                width, height, !strcmp(frame_rate,"30") ? "30000:1001" : "25:1", aspect) < 0 ||
        fwrite(y_plane, 1, pixels, file) != pixels ||
        fwrite(u_plane, 1, chroma, file) != chroma ||
        fwrite(v_plane, 1, chroma, file) != chroma || fflush(file) != 0 || ferror(file))
    {
        report(ErrorException, "Could not write YUV4MPEG2 menu frame", output);
        goto done;
    }
    result = 0;
done:
    if (file && fclose(file) != 0) result = -1;
    if (result != 0 && output_path) _wremove(output_path);
    if (wand) DestroyMagickWand(wand);
    free(rgb); free(y_plane); free(u_plane); free(v_plane); free(output_path);
    active = NULL;
    LeaveCriticalSection(&gate);
    return result;
}

/* Caller-owned RGBA buffer: no allocator crosses the DLL boundary. */
__declspec(dllexport) int __cdecl dvda_image_read_rgba(const char *input,
    unsigned char *rgba, size_t capacity, unsigned *width, unsigned *height)
{
    if(!input || !rgba || !width || !height || !InitOnceExecuteOnce(&once,initialize,NULL,NULL))return -1;
    EnterCriticalSection(&gate);
    MagickWand *wand=NewMagickWand();int result=-1;
    if(wand && MagickReadImage(wand,input)!=MagickFalse) {
        size_t w=MagickGetImageWidth(wand),h=MagickGetImageHeight(wand);
        if(w && h && w<=720 && h<=576 && w*h<=capacity/4 &&
           MagickExportImagePixels(wand,0,0,w,h,"RGBA",CharPixel,rgba)!=MagickFalse) {
            *width=(unsigned)w;*height=(unsigned)h;result=0;
        }
    }
    if(wand)DestroyMagickWand(wand);
    LeaveCriticalSection(&gate);return result;
}

/* argv includes a logical command name: magick, convert, identify or mogrify.
 * INFO output goes to per-request files, avoiding process-wide stdout redirection.
 * The DLL serializes ImageMagick's global configuration and legacy CLI state.
 */
__declspec(dllexport) int __cdecl dvda_image_run(int count, const char *const *arguments, Emit emit, Cancel cancel, void *state)
{
    Call call = {emit, cancel, state};
    if (count < 2 || count > 8192 || !arguments) return 2;
    if (!InitOnceExecuteOnce(&once, initialize, NULL, NULL)) return 2;
    while (!TryEnterCriticalSection(&gate)) {
        if (cancel && cancel(state)) return 130;
        Sleep(5);
    }
    active = &call;
    int result = 1, temporary_count = 0;
    char **argv = calloc((size_t)count + 1, sizeof(char *));
    wchar_t **temporaries = calloc(count, sizeof(wchar_t *));
    ExceptionInfo *exception = AcquireExceptionInfo();
    ImageInfo *info = AcquireImageInfo();
    char *metadata = NULL;
    if (!argv || !temporaries || !info || !exception) goto done;
    SetImageInfoProgressMonitor(info, monitor, &call);
    if (cancel && cancel(state)) { result = 130; goto done; }
    for (int i = 0; i < count; ++i) {
        if (!arguments[i]) goto done;
        argv[i] = _strdup(arguments[i]); if (!argv[i]) goto done;
        if (!_stricmp(argv[i], "info:") || !_stricmp(argv[i], "info:-")) {
            wchar_t folder[MAX_PATH + 1], path[MAX_PATH + 1];
            if (!GetTempPathW(MAX_PATH, folder) || !GetTempFileNameW(folder, L"dvi", 0, path)) goto done;
            temporaries[temporary_count++] = _wcsdup(path);
            char *name = utf8(path); if (!name) goto done;
            free(argv[i]); argv[i] = malloc(strlen(name) + 6);
            if (!argv[i]) { free(name); goto done; }
            sprintf(argv[i], "info:%s", name); free(name);
        }
    }
    if (count == 3 && !strcmp(argv[1], "-list") && !strcmp(argv[2], "font")) {
        size_t length = 0; char **fonts = GetTypeList("*", &length, exception);
        for (size_t i = 0; i < length; ++i) {
            emit_text(&call, 1, "Font: "); emit_text(&call, 1, fonts[i]); emit_text(&call, 1, "\n");
            RelinquishMagickMemory(fonts[i]);
        }
        RelinquishMagickMemory(fonts); result = exception->severity < ErrorException ? 0 : 1;
    } else {
        MagickCommand command = !strcmp(argv[0], "identify") ? IdentifyImageCommand :
            !strcmp(argv[0], "mogrify") ? MogrifyImageCommand :
            !strcmp(argv[0], "convert") ? ConvertImageCommand :
            !strcmp(argv[0], "magick") ? MagickImageCommand : NULL;
        if (!command) { emit_text(&call, 2, "Unsupported image command.\n"); goto done; }
        /* Convert's metadata parameter adds an unsolicited width,height,format
         * record. Only identify uses it as the command's actual stdout. */
        result = MagickCommandGenesis(info, command, count, argv,
            command == IdentifyImageCommand ? &metadata : NULL, exception) != MagickFalse && exception->severity < ErrorException ? 0 : 1;
        if (metadata) emit_text(&call, 1, metadata);
        for (int i = 0; i < temporary_count; ++i) {
            FILE *file = _wfopen(temporaries[i], L"rb"); if (!file) { result = 1; continue; }
            char text[4096]; size_t size;
            while ((size = fread(text, 1, sizeof(text)-1, file))) { text[size] = 0; emit_text(&call, 1, text); }
            if (ferror(file)) result = 1;
            fclose(file);
        }
    }
    if (exception->severity != UndefinedException) report(exception->severity, exception->reason, exception->description);
    if (cancel && cancel(state)) result = 130;
done:
    if (metadata) DestroyString(metadata);
    if (exception) DestroyExceptionInfo(exception);
    if (info) DestroyImageInfo(info);
    if (argv) { for (int i = 0; i < count; ++i) free(argv[i]); free(argv); }
    if (temporaries) { for (int i = 0; i < temporary_count; ++i) if (temporaries[i]) { DeleteFileW(temporaries[i]); free(temporaries[i]); } free(temporaries); }
    active = NULL; LeaveCriticalSection(&gate); return result;
}

/* For the native authoring program's existing UTF-8 argument builders.
 * Parse Windows quoting directly; never pass these strings to cmd.exe/system.
 */
__declspec(dllexport) int __cdecl dvda_image_command(const char *command)
{
    if (!command) return -1;
    /* CommandLineToArgvW treats leading whitespace as an empty argv[0].
     * The native index builder intentionally prefixes its command with spaces. */
    while (*command == ' ' || *command == '\t' || *command == '\r' || *command == '\n') ++command;
    wchar_t *line = wide(command); if (!line) return -1;
    int count = 0; wchar_t **parsed = CommandLineToArgvW(line, &count); free(line);
    if (!parsed || count < 2) { if (parsed) LocalFree(parsed); return -1; }
    char **args = calloc(count, sizeof(char *)); int result = -1;
    if (!args) { LocalFree(parsed); return -1; }
    for (int i = 0; i < count; ++i) if (!(args[i] = utf8(parsed[i]))) goto done;
    result = dvda_image_run(count, (const char *const *)args, NULL, NULL, NULL) == 0 ? 0 : -1;
done:
    for (int i = 0; i < count; ++i) free(args[i]); free(args); LocalFree(parsed); return result;
}
