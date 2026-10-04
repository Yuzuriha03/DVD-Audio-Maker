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

    private static Exports Load()
    {
        var library=NativeLibrary.Load(LibraryPath);
        try
        {
            var version=Marshal.GetDelegateForFunctionPointer<VersionDelegate>(NativeLibrary.GetExport(library,"dvda_rust_abi_version"));
            if(version()!=1) throw new InvalidDataException("Unsupported Rust ABI version.");
            return new Exports(
                Marshal.GetDelegateForFunctionPointer<CallDelegate>(NativeLibrary.GetExport(library,"dvda_rust_call")),
                Marshal.GetDelegateForFunctionPointer<FreeDelegate>(NativeLibrary.GetExport(library,"dvda_rust_free")));
        }
        catch { NativeLibrary.Free(library); throw; }
    }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate uint VersionDelegate();
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate IntPtr CallDelegate(IntPtr operation,IntPtr request);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void FreeDelegate(IntPtr pointer);
    private sealed record Exports(CallDelegate Call,FreeDelegate Free);
}
