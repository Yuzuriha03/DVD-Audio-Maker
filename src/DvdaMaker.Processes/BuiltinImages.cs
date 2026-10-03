using System.Diagnostics;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Text;

namespace DvdaMaker.Processes;

/// <summary>Image operations shared by the GUI and native authoring library.</summary>
public static class BuiltinImages
{
    private const string Prefix = "builtin:image:";
    private static readonly Lazy<RunDelegate> Entry = new(Load);
    public static string LibraryPath => Path.Combine(
        Environment.GetEnvironmentVariable("DVDA_IMAGE_NATIVE_DIR") is { Length: > 0 } directory
            ? Path.GetFullPath(directory) : Path.Combine(BundledRuntime.Root, "image-native"), "dvda-image.dll");
    public static bool IsAvailable => File.Exists(LibraryPath);
    public static bool IsBuiltin(string name) => name.StartsWith(Prefix, StringComparison.Ordinal);
    public static bool IsImageTool(string name) => name is "magick" or "convert" or "identify" or "mogrify";
    public static string Tool(string name) => IsImageTool(name) ? Prefix + name : throw new ArgumentException("Unknown image operation.");

    private static RunDelegate Load()
    {
        if (!OperatingSystem.IsWindows() || !Environment.Is64BitProcess)
            throw new PlatformNotSupportedException("内置图像处理需要 Windows x64。");
        if (!IsAvailable) throw new FileNotFoundException("内置图像组件缺失，请完整解压发布包。", LibraryPath);
        var module = LoadLibraryExW(LibraryPath, IntPtr.Zero, 0x100 | 0x800);
        if (module == IntPtr.Zero) throw new InvalidOperationException($"无法加载内置图像组件（Windows 错误 {Marshal.GetLastWin32Error()}）。");
        return Marshal.GetDelegateForFunctionPointer<RunDelegate>(NativeLibrary.GetExport(module, "dvda_image_run"));
    }

    public static async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken cancellation)
    {
        using var timeout = request.Timeout is { } duration ? new CancellationTokenSource(duration) : null;
        using var linked = timeout is null ? CancellationTokenSource.CreateLinkedTokenSource(cancellation)
            : CancellationTokenSource.CreateLinkedTokenSource(cancellation, timeout.Token);
        try { return await Task.Run(() => Execute(request, linked.Token), CancellationToken.None).ConfigureAwait(false); }
        catch (OperationCanceledException) when (timeout?.IsCancellationRequested == true && !cancellation.IsCancellationRequested)
        { throw new TimeoutException("内置图像处理超时。"); }
    }

    private static ProcessResult Execute(ProcessRequest request, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        var started = Stopwatch.GetTimestamp();
        var mode = request.FileName[Prefix.Length..];
        var arguments = request.Arguments.ToList();
        if (mode == "magick" && arguments.Count > 0 && IsImageTool(arguments[0])) { mode = arguments[0]; arguments.RemoveAt(0); }
        if (!IsImageTool(mode)) throw new NotSupportedException("Unsupported built-in image operation.");
        string? temporary = null, destination = null;
        var output = new StringBuilder(); var error = new StringBuilder();
        var allocations = new List<IntPtr>();
        ExceptionDispatchInfo? callbackError = null;
        try
        {
            // Application convert requests have one output. Keep an existing output
            // intact if decoding, cancellation or writing fails. Native authoring
            // uses its own isolated temporary menu directory for in-place mogrify.
            if (mode is "magick" or "convert" && arguments.Count > 0 && !arguments.Contains("-list") &&
                arguments[^1] is not ("info:" or "info:-" or "null:"))
            {
                var path = arguments[^1]; var format = "";
                if (path.StartsWith("PNG32:", StringComparison.OrdinalIgnoreCase)) { format = path[..6]; path = path[6..]; }
                destination = Path.GetFullPath(path, request.WorkingDirectory ?? Environment.CurrentDirectory);
                temporary = Path.Combine(Path.GetDirectoryName(destination)!, ".dvda-image-" + Guid.NewGuid().ToString("N") + Path.GetExtension(destination));
                arguments[^1] = format + temporary;
            }
            arguments.Insert(0, mode);
            var pointers = arguments.Select(value => { var pointer = Marshal.StringToCoTaskMemUTF8(value); allocations.Add(pointer); return pointer; }).ToArray();
            var array = Marshal.AllocCoTaskMem(pointers.Length * IntPtr.Size); allocations.Add(array); Marshal.Copy(pointers, 0, array, pointers.Length);
            Emit emit = (_, stream, pointer) =>
            {
                try
                {
                    var text = Marshal.PtrToStringUTF8(pointer) ?? "";
                    if (stream == 2) error.Append(text); else output.Append(text);
                }
                catch (Exception exception) { callbackError ??= ExceptionDispatchInfo.Capture(exception); }
            };
            Cancel cancel = _ => token.IsCancellationRequested || callbackError is not null ? 1 : 0;
            var status = Entry.Value(pointers.Length, array, emit, cancel, IntPtr.Zero);
            GC.KeepAlive(emit); GC.KeepAlive(cancel);
            callbackError?.Throw(); token.ThrowIfCancellationRequested();
            if (status == 0 && temporary is not null) { File.Move(temporary, destination!, true); temporary = null; }
            foreach (var line in output.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries)) request.OnOutputLine?.Invoke(line.TrimEnd('\r'));
            foreach (var line in error.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries)) request.OnErrorLine?.Invoke(line.TrimEnd('\r'));
            var result = new ProcessResult(request.FileName, request.Arguments, status, request.CaptureOutput ? output.ToString() : "",
                request.CaptureError ? error.ToString() : "", Stopwatch.GetElapsedTime(started));
            if (request.ThrowOnNonZeroExitCode && !result.Succeeded) throw new ProcessExecutionException(result);
            return result;
        }
        finally
        {
            foreach (var pointer in allocations) Marshal.FreeCoTaskMem(pointer);
            if (temporary is not null && File.Exists(temporary)) File.Delete(temporary);
        }
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int RunDelegate(int count, IntPtr args, Emit emit, Cancel cancel, IntPtr state);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void Emit(IntPtr state, int stream, IntPtr text);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int Cancel(IntPtr state);
    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)] private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
}
