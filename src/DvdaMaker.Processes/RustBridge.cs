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
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_image_run")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_encoder_run")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_process_run")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_pcm_run")),
                Marshal.GetDelegateForFunctionPointer<MediaRunDelegate>(NativeLibrary.GetExport(library,"dvda_rust_batch_run")));
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
        MediaRunDelegate RunMedia, MediaRunDelegate RunImage, MediaRunDelegate RunEncoder, MediaRunDelegate RunProcess, MediaRunDelegate RunPcm, MediaRunDelegate RunBatch);

    internal sealed record MediaFailure(string Kind, int? Code, string Message);
    internal sealed record MediaOutcome(int? ExitCode, MediaFailure? Failure,
        string StandardOutput = "", string StandardError = "", double DurationSeconds = 0);
    internal static int RunMedia(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, "media.execute").ExitCode!.Value;
    internal static int RunImage(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, "image.execute").ExitCode!.Value;
    public static int RunEncoder(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, "encoder.execute").ExitCode!.Value;
    public static void NormalizePcm(object request, CancellationToken token)
        => RunNativeJob(request, (_, _) => { }, token, "pcm.normalize");
    public static void RunBatch(object request, Action<int, string> onText, CancellationToken token)
        => RunNativeJob(request, onText, token, "batch.encode");
    internal static ProcessResult RunProcess(ProcessRequest request, CancellationToken token)
    {
        MediaOutcome result;
        try
        {
            result = RunNativeJob(new { request.FileName, request.Arguments, request.WorkingDirectory,
                Environment = request.Environment.Select(p => new object?[] {p.Key, p.Value}).ToArray(),
                OutputCodePage = request.OutputEncoding.CodePage, ErrorCodePage = request.ErrorEncoding.CodePage,
                request.CaptureOutput, request.CaptureError,
                TimeoutMillis = request.Timeout is { } time && time != Timeout.InfiniteTimeSpan ? (long?)Math.Max(0, time.TotalMilliseconds) : null },
                (stream, json) => { var line = JsonSerializer.Deserialize<string>(json)!;
                    if (stream == 1) request.OnOutputLine?.Invoke(line); else request.OnErrorLine?.Invoke(line); }, token, "process.execute");
        }
        catch (TimeoutException)
        { throw new TimeoutException($"外部程序运行超时: {CommandLineFormatter.Format(request.FileName, request.Arguments)}"); }
        var value = new ProcessResult(request.FileName, request.Arguments, result.ExitCode!.Value,
            result.StandardOutput, result.StandardError, TimeSpan.FromSeconds(result.DurationSeconds));
        if (request.ThrowOnNonZeroExitCode && !value.Succeeded) throw new ProcessExecutionException(value);
        return value;
    }
    public static void WriteEncoderMetadata(object request)
    {
        var result = Invoke<MediaOutcome>("encoder.write_metadata", request);
        ThrowJobFailure(result, "encoder.write_metadata", CancellationToken.None);
    }
    private static MediaOutcome RunNativeJob(object request, Action<int, string> onText, CancellationToken token, string operation)
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
            var run = operation switch { "image.execute" => native.RunImage, "encoder.execute" => native.RunEncoder,
                "process.execute" => native.RunProcess, "pcm.normalize" => native.RunPcm,
                "batch.encode" => native.RunBatch, _ => native.RunMedia };
            output = run(input, emit, cancel, IntPtr.Zero);
            GC.KeepAlive(emit); GC.KeepAlive(cancel);
            callbackFailure?.Throw();
            if (output == IntPtr.Zero) throw new InvalidDataException("Rust returned an empty media response.");
            using var response = JsonDocument.Parse(Marshal.PtrToStringUTF8(output)!);
            var root = response.RootElement;
            if (!root.GetProperty("ok").GetBoolean()) throw new InvalidDataException(root.GetProperty("error").GetString());
            var result = root.GetProperty("value").Deserialize<MediaOutcome>()!;
            Calls.AddOrUpdate(operation, 1, (_, count) => count + 1);
            ThrowJobFailure(result, operation, token);
            if (result.ExitCode is null) throw new InvalidDataException("Missing native job exit status.");
            return result;
        }
        finally
        {
            if (output != IntPtr.Zero) native.Free(output);
            Marshal.FreeCoTaskMem(input);
        }
    }

    private static void ThrowJobFailure(MediaOutcome result, string operation, CancellationToken token)
    {
        if (result.Failure is not { } failure) return;
        if (failure.Kind == "Cancelled") throw new OperationCanceledException(token);
        if (failure.Kind == "Timeout") throw new TimeoutException(operation switch
        { "image.execute" => "内置图像处理超时。", "encoder.execute" => "MLP 编码器 DLL 编码超时，已取消并清理临时输出。", _ => "内置媒体处理超时。" });
        if (failure.Kind == "Argument") throw new ArgumentException(failure.Message);
        if (failure.Kind == "ArgumentOutOfRange") throw new ArgumentOutOfRangeException(null, failure.Message);
        if (failure.Kind == "InvalidData") throw new InvalidDataException(failure.Message);
        if (failure.Kind == "EndOfStream") throw new EndOfStreamException(failure.Message);
        if (failure.Kind == "Overflow") throw new OverflowException(failure.Message);
        if (failure.Kind == "InvalidOperation") throw new InvalidOperationException(failure.Message);
        throw failure.Code switch
        {
            2 => new FileNotFoundException(failure.Message),
            3 => new DirectoryNotFoundException(failure.Message),
            5 => new UnauthorizedAccessException(failure.Message),
            int code => new IOException(failure.Message, unchecked((int)0x80070000) | code),
            _ => new IOException(failure.Message),
        };
    }
}
