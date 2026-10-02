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
