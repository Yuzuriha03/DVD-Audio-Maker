using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace DvdaMaker.Processes;

/// <summary>Explicit Rust/compare modes never silently fall back to managed code.</summary>
public static class RustBridge
{
    private static readonly Lazy<Exports> Native = new(Load);
    private static readonly ConcurrentDictionary<string, long> Calls = new(StringComparer.Ordinal);
    public static string Mode => Environment.GetEnvironmentVariable("DVDA_RUST_MODE") ??
        (File.Exists(LibraryPath) ? "rust" : "managed");
    public static bool Enabled => Mode is "rust" or "compare";
    public static IReadOnlyDictionary<string, long> Snapshot() => new Dictionary<string, long>(Calls);
    public static string LibraryPath => Environment.GetEnvironmentVariable("DVDA_RUST_LIBRARY") is { Length: > 0 } path
        ? Path.GetFullPath(path) : Path.Combine(AppContext.BaseDirectory,"rust-native","dvda_host_ffi.dll");

    public static T Run<T>(string operation, object? request, Func<T> managed)
    {
        if (Mode == "managed") return managed();
        if (!Enabled) throw new InvalidOperationException($"Unknown DVDA_RUST_MODE: {Mode}");
        var actual = Invoke<T>(operation, request);
        if (Mode == "compare")
        {
            var expected = managed();
            if (!JsonNode.DeepEquals(JsonSerializer.SerializeToNode(expected), JsonSerializer.SerializeToNode(actual)))
                throw new InvalidDataException($"Rust compatibility mismatch in {operation}: expected {JsonSerializer.Serialize(expected)}, actual {JsonSerializer.Serialize(actual)}");
        }
        return actual;
    }

    public static T Invoke<T>(string operation, object? request)
    {
        var native=Native.Value;
        var op=Marshal.StringToCoTaskMemUTF8(operation);
        var input=Marshal.StringToCoTaskMemUTF8(JsonSerializer.Serialize(request));
        IntPtr output=IntPtr.Zero;
        try
        {
            output=native.Call(op,input);
            if (output==IntPtr.Zero) throw new InvalidOperationException("Rust returned a null response.");
            using var document=JsonDocument.Parse(Marshal.PtrToStringUTF8(output) ?? throw new InvalidDataException("Empty Rust response."));
            var root=document.RootElement;
            if (!root.GetProperty("ok").GetBoolean()) throw new InvalidDataException(root.GetProperty("error").GetString());
            var value=root.GetProperty("value").Deserialize<T>()!;
            Calls.AddOrUpdate(operation,1,(_,count)=>count+1);
            return value;
        }
        finally
        {
            if(output!=IntPtr.Zero) native.Free(output);
            Marshal.FreeCoTaskMem(op); Marshal.FreeCoTaskMem(input);
        }
    }

    public static unsafe long[] ScanAob(ReadOnlyMemory<byte> data, int stride, bool requirePack, Func<long[]> managed)
    {
        if (Mode == "managed") return managed();
        if (!Enabled) throw new InvalidOperationException($"Unknown DVDA_RUST_MODE: {Mode}");
        if (stride <= 0 || data.Length % stride != 0) throw new ArgumentOutOfRangeException(nameof(stride));
        var actual = new long[data.Length / stride];
        var native = Native.Value;
        fixed (byte* input = data.Span)
        fixed (long* output = actual)
        {
            var error = native.ScanAob((IntPtr)input, (nuint)data.Length, (nuint)stride,
                requirePack ? 1U : 0U, (IntPtr)output, (nuint)actual.Length);
            if (error != IntPtr.Zero)
            {
                try { throw new InvalidDataException(Marshal.PtrToStringUTF8(error)); }
                finally { native.Free(error); }
            }
        }
        Calls.AddOrUpdate("aob.scan_sectors", 1, (_, count) => count + 1);
        if (Mode == "compare")
        {
            var expected = managed();
            if (!actual.AsSpan().SequenceEqual(expected))
                throw new InvalidDataException("Rust compatibility mismatch in aob.scan_sectors.");
        }
        return actual;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct AobAuditState
    {
        public long Previous;
        public int SectorCount;
        public int MissingSector;
        public static AobAuditState Initial => new() { Previous = -1, MissingSector = -1 };
    }
    public readonly record struct AobAuditResult(AobAuditState State, int Drop);

    public static unsafe AobAuditResult ObserveAob(ReadOnlyMemory<byte> data, AobAuditState state,
        Func<AobAuditResult> managed)
    {
        if (Mode == "managed") return managed();
        if (!Enabled) throw new InvalidOperationException($"Unknown DVDA_RUST_MODE: {Mode}");
        var native = Native.Value;
        int drop;
        fixed (byte* input = data.Span)
        {
            var error = native.ObserveAob((IntPtr)input, (nuint)data.Length, ref state, out drop);
            if (error != IntPtr.Zero)
            {
                try { throw new InvalidDataException(Marshal.PtrToStringUTF8(error)); }
                finally { native.Free(error); }
            }
        }
        Calls.AddOrUpdate("aob.audit_observe", 1, (_, count) => count + 1);
        var actual = new AobAuditResult(state, drop);
        if (Mode == "compare" && actual != managed())
            throw new InvalidDataException("Rust compatibility mismatch in aob.audit_observe.");
        return actual;
    }

    private static Exports Load()
    {
        var library=NativeLibrary.Load(LibraryPath);
        try
        {
            var version=Marshal.GetDelegateForFunctionPointer<VersionDelegate>(NativeLibrary.GetExport(library,"dvda_rust_abi_version"));
            if(version()!=1) throw new InvalidDataException("Unsupported Rust ABI version.");
            return new Exports(
                Marshal.GetDelegateForFunctionPointer<CallDelegate>(NativeLibrary.GetExport(library,"dvda_rust_call")),
                Marshal.GetDelegateForFunctionPointer<FreeDelegate>(NativeLibrary.GetExport(library,"dvda_rust_free")),
                Marshal.GetDelegateForFunctionPointer<AobScanDelegate>(NativeLibrary.GetExport(library,"dvda_rust_aob_scan")),
                Marshal.GetDelegateForFunctionPointer<AobObserveDelegate>(NativeLibrary.GetExport(library,"dvda_rust_aob_observe")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_media_run")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_image_run")));
        }
        catch { NativeLibrary.Free(library); throw; }
    }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate uint VersionDelegate();
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr CallDelegate(IntPtr operation,IntPtr request);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void FreeDelegate(IntPtr pointer);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr AobScanDelegate(
        IntPtr data, nuint length, nuint stride, uint requirePack, IntPtr output, nuint capacity);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr AobObserveDelegate(
        IntPtr data, nuint length, ref AobAuditState state, out int dropIndex);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void MediaEmit(IntPtr state, int stream, IntPtr text);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int MediaCancel(IntPtr state);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr MediaRunDelegate(
        IntPtr request, MediaEmit emit, MediaCancel cancel, IntPtr state);
    private sealed record Exports(CallDelegate Call,FreeDelegate Free,AobScanDelegate ScanAob,AobObserveDelegate ObserveAob,
        MediaRunDelegate RunMedia, MediaRunDelegate RunImage);

    internal sealed record MediaFailure(string Kind, int? Code, string Message);
    internal sealed record MediaOutcome(int? ExitCode, MediaFailure? Failure);
    internal static int RunMedia(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, image: false);
    internal static int RunImage(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, image: true);
    private static int RunNativeJob(object request, Action<int, string> onText, CancellationToken token, bool image)
    {
        if (!Enabled) throw new InvalidOperationException($"Unknown DVDA_RUST_MODE: {Mode}");
        var native = Native.Value;
        var input = Marshal.StringToCoTaskMemUTF8(JsonSerializer.Serialize(request));
        IntPtr output = IntPtr.Zero;
        System.Runtime.ExceptionServices.ExceptionDispatchInfo? callbackFailure = null;
        MediaEmit emit = (_, stream, text) =>
        {
            try { onText(stream, Marshal.PtrToStringUTF8(text) ?? ""); }
            catch (Exception error) { callbackFailure ??= System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(error); }
        };
        MediaCancel cancel = _ => token.IsCancellationRequested || callbackFailure is not null ? 1 : 0;
        try
        {
            output = (image ? native.RunImage : native.RunMedia)(input, emit, cancel, IntPtr.Zero);
            GC.KeepAlive(emit); GC.KeepAlive(cancel);
            callbackFailure?.Throw();
            if (output == IntPtr.Zero) throw new InvalidDataException("Rust returned an empty media response.");
            using var response = JsonDocument.Parse(Marshal.PtrToStringUTF8(output)!);
            var root = response.RootElement;
            if (!root.GetProperty("ok").GetBoolean()) throw new InvalidDataException(root.GetProperty("error").GetString());
            var result = root.GetProperty("value").Deserialize<MediaOutcome>()!;
            Calls.AddOrUpdate(image ? "image.execute" : "media.execute", 1, (_, count) => count + 1);
            if (result.Failure is { } failure)
            {
                if (failure.Kind == "Cancelled") throw new OperationCanceledException(token);
                if (failure.Kind == "Timeout") throw new TimeoutException(image ? "内置图像处理超时。" : "内置媒体处理超时。");
                if (failure.Kind == "Argument") throw new ArgumentException(failure.Message);
                throw failure.Code switch
                {
                    2 => new FileNotFoundException(failure.Message),
                    3 => new DirectoryNotFoundException(failure.Message),
                    5 => new UnauthorizedAccessException(failure.Message),
                    int code => new IOException(failure.Message, unchecked((int)0x80070000) | code),
                    _ => new IOException(failure.Message),
                };
            }
            return result.ExitCode ?? throw new InvalidDataException("Missing media exit status.");
        }
        finally
        {
            if (output != IntPtr.Zero) native.Free(output);
            Marshal.FreeCoTaskMem(input);
        }
    }
}
