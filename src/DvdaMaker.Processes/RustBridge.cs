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
                Marshal.GetDelegateForFunctionPointer<AobObserveDelegate>(NativeLibrary.GetExport(library,"dvda_rust_aob_observe")));
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
    private sealed record Exports(CallDelegate Call,FreeDelegate Free,AobScanDelegate ScanAob,AobObserveDelegate ObserveAob);
}
