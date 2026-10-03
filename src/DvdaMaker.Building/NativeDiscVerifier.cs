using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

namespace DvdaMaker.Building;

/// <summary>Read-only C PES parser; compares full ordered MLP streams.</summary>
public static class NativeDiscVerifier
{
    public static void Verify(string directory, IReadOnlyList<string> sources,
        IEnumerable<ReadOnlyMemory<byte>> chunks, CancellationToken token = default)
    {
        var library = NativeLibrary.Load(Path.Combine(directory, "dvda-disc-verify.dll"));
        var strings = new List<IntPtr>();
        IntPtr array = IntPtr.Zero;
        try
        {
            var entry = Marshal.GetDelegateForFunctionPointer<VerifyDelegate>(
                NativeLibrary.GetExport(library, "dvda_verify_mlp_payload"));
            foreach (var source in sources) strings.Add(Marshal.StringToCoTaskMemUTF8(source));
            array = Marshal.AllocCoTaskMem(checked(strings.Count * IntPtr.Size));
            Marshal.Copy(strings.ToArray(), 0, array, strings.Count);
            using var iterator = chunks.GetEnumerator();
            ReadOnlyMemory<byte> current = default;
            ExceptionDispatchInfo? failure = null;
            ReadChunk read = (_, buffer, capacity) =>
            {
                try
                {
                    token.ThrowIfCancellationRequested();
                    while (current.IsEmpty)
                    {
                        if (!iterator.MoveNext()) return 0;
                        current = iterator.Current;
                    }
                    var count = Math.Min(current.Length, checked((int)capacity));
                    Marshal.Copy(current[..count].ToArray(), 0, buffer, count);
                    current = current[count..];
                    return count;
                }
                catch (Exception error) { failure = ExceptionDispatchInfo.Capture(error); return -1; }
            };
            var status = entry(array, (uint)strings.Count, read, IntPtr.Zero, out var result);
            failure?.Throw();
            token.ThrowIfCancellationRequested();
            if (status != 0)
                throw new InvalidDataException($"ISO MLP 字节校验失败：轨道 {result.Track + 1}，" +
                    $"偏移 {result.Offset}，AOB 扇区 {result.Sectors}，状态 {status}。");
        }
        finally
        {
            foreach (var text in strings) Marshal.FreeCoTaskMem(text);
            if (array != IntPtr.Zero) Marshal.FreeCoTaskMem(array);
            NativeLibrary.Free(library);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Result { public int Code, Track; public ulong Offset, Sectors, Bytes; }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int ReadChunk(IntPtr state, IntPtr buffer, uint capacity);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int VerifyDelegate(IntPtr paths, uint count, ReadChunk read, IntPtr state, out Result result);
}
