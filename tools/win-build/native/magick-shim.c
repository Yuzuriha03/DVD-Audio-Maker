/* Keep legacy executable names while sharing the bundled ImageMagick core.
 * Build as an x64 Windows console executable with only Kernel32 imports.
 * Forward the argument tail verbatim: image expressions and quotes must survive.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#define CAPACITY 32768
static WCHAR executable[CAPACITY];
static WCHAR command[CAPACITY];
static STARTUPINFOW startup;
static PROCESS_INFORMATION child;
static JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits;

static void fail(const char *message, DWORD code)
{
    DWORD written;
    char digits[11];
    char reverse[10];
    unsigned count = 0, index;
    HANDLE error = GetStdHandle(STD_ERROR_HANDLE);
    WriteFile(error, message, (DWORD)lstrlenA(message), &written, NULL);
    do { reverse[count++] = (char)('0' + code % 10); code /= 10; } while (code);
    for (index = 0; index < count; ++index) digits[index] = reverse[count - index - 1];
    digits[count] = '\n';
    WriteFile(error, digits, count + 1, &written, NULL);
    ExitProcess(125);
}

static DWORD append(DWORD used, const WCHAR *text)
{
    while (*text)
    {
        if (used >= CAPACITY - 1) fail("ImageMagick argument list is too long; Windows error ", ERROR_BAD_LENGTH);
        command[used++] = *text++;
    }
    command[used] = 0;
    return used;
}

void WINAPI entry(void)
{
    DWORD size = GetModuleFileNameW(NULL, executable, CAPACITY);
    DWORD directory = 0, index, used, exit_code;
    BOOL quoted = FALSE;
    const WCHAR *mode;
    const WCHAR *tail = GetCommandLineW();
    HANDLE job;

    if (!size || size >= CAPACITY) fail("Cannot locate the ImageMagick forwarder; Windows error ", GetLastError());
    for (index = 0; index < size; ++index)
        if (executable[index] == L'\\' || executable[index] == L'/') directory = index + 1;
    if (lstrcmpiW(executable + directory, L"convert.exe") == 0) mode = L" convert ";
    else if (lstrcmpiW(executable + directory, L"mogrify.exe") == 0) mode = L" mogrify ";
    else fail("The forwarder must be named convert.exe or mogrify.exe; Windows error ", ERROR_INVALID_NAME);
    if (directory + 11 >= CAPACITY) fail("ImageMagick path is too long; Windows error ", ERROR_BAD_LENGTH);
    lstrcpyW(executable + directory, L"magick.exe");

    /* argv[0] follows the special Windows executable-name quoting rule. */
    while (*tail == L' ' || *tail == L'\t') ++tail;
    while (*tail)
    {
        if (*tail == L'"') quoted = !quoted;
        else if (!quoted && (*tail == L' ' || *tail == L'\t')) break;
        ++tail;
    }
    while (*tail == L' ' || *tail == L'\t') ++tail;
    used = append(0, L"\"");
    used = append(used, executable);
    used = append(used, L"\"");
    used = append(used, mode);
    append(used, tail);

    /* Closing this job kills descendants if the caller cancels the wrapper. */
    job = CreateJobObjectW(NULL, NULL);
    if (!job) fail("Cannot create the ImageMagick process job; Windows error ", GetLastError());
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (!SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof(limits)))
        fail("Cannot configure the ImageMagick process job; Windows error ", GetLastError());
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
    startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    if (!CreateProcessW(executable, command, NULL, NULL, TRUE,
            CREATE_SUSPENDED | CREATE_NO_WINDOW, NULL, NULL, &startup, &child))
        fail("Cannot launch bundled magick.exe; Windows error ", GetLastError());
    if (!AssignProcessToJobObject(job, child.hProcess))
    {
        DWORD error = GetLastError();
        TerminateProcess(child.hProcess, 125);
        fail("Cannot supervise the ImageMagick child; Windows error ", error);
    }
    if (ResumeThread(child.hThread) == (DWORD)-1)
        fail("Cannot resume the ImageMagick child; Windows error ", GetLastError());
    CloseHandle(child.hThread);
    if (WaitForSingleObject(child.hProcess, INFINITE) != WAIT_OBJECT_0 ||
        !GetExitCodeProcess(child.hProcess, &exit_code))
        fail("Cannot obtain the ImageMagick result; Windows error ", GetLastError());
    CloseHandle(child.hProcess);
    CloseHandle(job);
    ExitProcess(exit_code);
}
