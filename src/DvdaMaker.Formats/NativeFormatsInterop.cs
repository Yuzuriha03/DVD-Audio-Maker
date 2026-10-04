using System.Runtime.InteropServices;

namespace DvdaMaker.Formats;

/// <summary>
/// Optional C17 implementation of the small, deterministic format helpers.
/// The managed implementations remain the compatibility fallback when the
/// development build has no native formats DLL.
/// </summary>
public static class NativeFormatsInterop
{
    private static readonly Lazy<Exports?> Native = new(Load, LazyThreadSafetyMode.ExecutionAndPublication);

    public static string LibraryPath
    {
        get
        {
            var configured = Environment.GetEnvironmentVariable("DVDA_FORMATS_NATIVE_DIR");
            if (!string.IsNullOrWhiteSpace(configured))
            {
                var path = Path.GetFullPath(configured);
                return path.EndsWith(".dll", StringComparison.OrdinalIgnoreCase)
                    ? path : Path.Combine(path, "dvda-formats.dll");
            }

            foreach (var candidate in new[]
            {
                Path.Combine(AppContext.BaseDirectory, "formats-native", "dvda-formats.dll"),
                Path.Combine(AppContext.BaseDirectory, "menu-bin", "dvda-formats.dll"),
                Path.Combine(AppContext.BaseDirectory, "dvda-formats.dll"),
            })
            {
                if (File.Exists(candidate)) return candidate;
            }
            return Path.Combine(AppContext.BaseDirectory, "formats-native", "dvda-formats.dll");
        }
    }

    private static bool Disabled => string.Equals(
        Environment.GetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE"), "1", StringComparison.Ordinal);

    private static Exports? Current => Disabled ? null : Native.Value;

    public static bool IsAvailable => Current is not null;

    public static bool TryInspectFile(string path, out NativeMlpInspection result)
    {
        result = default;
        var native = Current;
        if (native is null) return false;
        var pointer = Marshal.StringToCoTaskMemUTF8(Path.GetFullPath(path));
        try
        {
            if (native.InspectFile(pointer, out var value) != 0) return false;
            result = Convert(value);
            return true;
        }
        finally { Marshal.FreeCoTaskMem(pointer); }
    }

    public static bool TryInspect(ReadOnlySpan<byte> data, out NativeMlpInspection result)
    {
        result = default;
        var native = Current;
        if (native is null) return false;
        var bytes = data.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length == 0 ? 1 : bytes.Length);
        try
        {
            if (bytes.Length > 0) Marshal.Copy(bytes, 0, pointer, bytes.Length);
            if (native.InspectBuffer(pointer, (nuint)bytes.Length, out var value) != 0) return false;
            result = Convert(value);
            return true;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TryComparePcm(
        string sourcePath, string decodedPath, int bytesPerSampleFrame, int maxTrailingZeroFrames,
        out NativePcmComparison result)
    {
        result = default;
        var native = Current;
        if (native is null || bytesPerSampleFrame < 0 || maxTrailingZeroFrames < 0 ||
            (maxTrailingZeroFrames > 0 && bytesPerSampleFrame == 0)) return false;
        var source = Marshal.StringToCoTaskMemUTF8(Path.GetFullPath(sourcePath));
        var decoded = Marshal.StringToCoTaskMemUTF8(Path.GetFullPath(decodedPath));
        try
        {
            if (native.ComparePcm(source, decoded, (uint)bytesPerSampleFrame,
                (uint)maxTrailingZeroFrames, out var value) != 0) return false;
            result = new NativePcmComparison(value.Match != 0, value.ReasonCode,
                checked((long)value.SourceBytes), checked((long)value.DecodedBytes),
                checked((long)value.TrailingZeroBytes), checked((long)value.FirstMismatchOffset));
            return true;
        }
        finally
        {
            Marshal.FreeCoTaskMem(source);
            Marshal.FreeCoTaskMem(decoded);
        }
    }

    public static bool TryParsePts(ReadOnlySpan<byte> data, out long value)
    {
        value = 0;
        var native = Current;
        if (native is null || data.Length < 5) return false;
        var bytes = data[..Math.Min(data.Length, 5)].ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            return native.ParsePts(pointer, (nuint)bytes.Length, out value) == 0;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TrySampleRate(ReadOnlySpan<byte> majorSync, out int value)
    {
        value = 0;
        var native = Current;
        if (native is null) return false;
        var bytes = majorSync.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length == 0 ? 1 : bytes.Length);
        try
        {
            if (bytes.Length > 0) Marshal.Copy(bytes, 0, pointer, bytes.Length);
            var result = native.SampleRate(pointer, (nuint)bytes.Length, out var rate);
            if (result != 0) return false;
            value = rate;
            return true;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TryPeakBitrate(int sampleRate, out int value)
    {
        value = 0;
        var native = Current;
        if (native is null || sampleRate <= 0) return false;
        value = native.PeakBitrate(sampleRate);
        return value > 0;
    }

    public static bool TryChecksum16(ReadOnlySpan<byte> data, out ushort value)
    {
        value = 0;
        var native = Current;
        if (native is null || data.Length < 2) return false;
        var bytes = data.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            return native.Checksum16(pointer, (nuint)bytes.Length, out value) == 0;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TryChecksum8(ReadOnlySpan<byte> data, out byte value)
    {
        value = 0;
        var native = Current;
        if (native is null || data.Length < 1) return false;
        var bytes = data.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, pointer, bytes.Length);
            return native.Checksum8(pointer, (nuint)bytes.Length, out value) == 0;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TryCalculateParity(ReadOnlySpan<byte> data, out byte value)
    {
        value = 0;
        var native = Current;
        if (native is null) return false;
        var bytes = data.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length == 0 ? 1 : bytes.Length);
        try
        {
            if (bytes.Length > 0) Marshal.Copy(bytes, 0, pointer, bytes.Length);
            value = native.CalculateParity(pointer, (nuint)bytes.Length);
            return true;
        }
        finally { Marshal.FreeHGlobal(pointer); }
    }

    public static bool TryAlign(ReadOnlySpan<byte> data, out NativeMlpAlignment result)
    {
        result = default;
        var native = Current;
        if (native is null) return false;
        var bytes = data.ToArray();
        var pointer = Marshal.AllocHGlobal(bytes.Length == 0 ? 1 : bytes.Length);
        IntPtr aligned = IntPtr.Zero;
        try
        {
            if (bytes.Length > 0) Marshal.Copy(bytes, 0, pointer, bytes.Length);
            if (native.Align(pointer, (nuint)bytes.Length, out aligned, out var size, out var changes) != 0)
                return false;
            if (size > int.MaxValue) return false;
            var output = new byte[(int)size];
            if (output.Length > 0) Marshal.Copy(aligned, output, 0, output.Length);
            result = new NativeMlpAlignment(output, changes.PeakChanges, changes.ExtendedChanges,
                changes.ChecksumChanges, changes.InsertedEndOfStream != 0,
                changes.OldHeader < 0 ? null : checked((ushort)changes.OldHeader),
                changes.NewHeader < 0 ? null : checked((ushort)changes.NewHeader));
            return true;
        }
        finally
        {
            if (aligned != IntPtr.Zero) native.Free(aligned);
            Marshal.FreeHGlobal(pointer);
        }
    }

    private static NativeMlpInspection Convert(NativeMlpInspectionNative value) => new(
        checked((int)value.Size), checked((int)value.AccessUnitCount), checked((int)value.MajorSyncCount),
        value.MajorSyncInterval, value.MajorSyncErrorCount, value.AccessUnitParityErrorCount,
        value.SubstreamErrorCount, value.HasEndOfStream != 0, value.PeakBitrateRaw < 0 ? null : value.PeakBitrateRaw,
        value.ExtendedSubstreamInfo < 0 ? null : value.ExtendedSubstreamInfo,
        value.SampleRate < 0 ? null : value.SampleRate, value.IsValid != 0, value.ErrorCode);

    private static Exports? Load()
    {
        if (!OperatingSystem.IsWindows() || !Environment.Is64BitProcess) return null;
        var path = LibraryPath;
        if (!File.Exists(path)) return null;
        try
        {
            var handle = LoadLibraryExW(path, IntPtr.Zero, 0x100 | 0x800);
            if (handle == IntPtr.Zero) return null;
            return new Exports(
                Marshal.GetDelegateForFunctionPointer<InspectBufferDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_mlp_inspect_buffer")),
                Marshal.GetDelegateForFunctionPointer<InspectFileDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_mlp_inspect_file")),
                Marshal.GetDelegateForFunctionPointer<ComparePcmDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_pcm_compare_files")),
                Marshal.GetDelegateForFunctionPointer<ParsePtsDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_parse_pts")),
                Marshal.GetDelegateForFunctionPointer<SampleRateDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_sample_rate")),
                Marshal.GetDelegateForFunctionPointer<PeakBitrateDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_peak_bitrate_raw")),
                Marshal.GetDelegateForFunctionPointer<Checksum16Delegate>(NativeLibrary.GetExport(handle, "dvda_formats_checksum16")),
                Marshal.GetDelegateForFunctionPointer<Checksum8Delegate>(NativeLibrary.GetExport(handle, "dvda_formats_checksum8")),
                Marshal.GetDelegateForFunctionPointer<CalculateParityDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_calculate_parity")),
                Marshal.GetDelegateForFunctionPointer<AlignDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_mlp_align_buffer")),
                Marshal.GetDelegateForFunctionPointer<FreeDelegate>(NativeLibrary.GetExport(handle, "dvda_formats_free")));
        }
        catch (Exception exception) when (exception is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException or ArgumentException)
        {
            return null;
        }
    }

    public readonly record struct NativeMlpInspection(
        int Size, int AccessUnitCount, int MajorSyncCount, double MajorSyncInterval,
        int MajorSyncErrorCount, int AccessUnitParityErrorCount, int SubstreamErrorCount,
        bool HasEndOfStream, int? PeakBitrateRaw, int? ExtendedSubstreamInfo, int? SampleRate,
        bool IsValid, int ErrorCode);

    public readonly record struct NativePcmComparison(
        bool Match, int ReasonCode, long SourceBytes, long DecodedBytes,
        long TrailingZeroBytes, long FirstMismatchOffset);

    public readonly record struct NativeMlpAlignment(
        byte[] Data, int PeakChanges, int ExtendedChanges, int ChecksumChanges,
        bool InsertedEndOfStream, ushort? OldHeader, ushort? NewHeader);

    private sealed record Exports(
        InspectBufferDelegate InspectBuffer, InspectFileDelegate InspectFile,
        ComparePcmDelegate ComparePcm, ParsePtsDelegate ParsePts, SampleRateDelegate SampleRate,
        PeakBitrateDelegate PeakBitrate, Checksum16Delegate Checksum16, Checksum8Delegate Checksum8,
        CalculateParityDelegate CalculateParity, AlignDelegate Align, FreeDelegate Free);

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    private struct NativeMlpInspectionNative
    {
        public ulong Size;
        public uint AccessUnitCount;
        public uint MajorSyncCount;
        public double MajorSyncInterval;
        public int MajorSyncErrorCount;
        public int AccessUnitParityErrorCount;
        public int SubstreamErrorCount;
        public int HasEndOfStream;
        public int PeakBitrateRaw;
        public int ExtendedSubstreamInfo;
        public int SampleRate;
        public int IsValid;
        public int ErrorCode;
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    private struct NativePcmComparisonNative
    {
        public int Match;
        public int ReasonCode;
        public ulong SourceBytes;
        public ulong DecodedBytes;
        public ulong TrailingZeroBytes;
        public ulong FirstMismatchOffset;
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    private struct NativeMlpAlignmentNative
    {
        public int PeakChanges;
        public int ExtendedChanges;
        public int ChecksumChanges;
        public int InsertedEndOfStream;
        public int OldHeader;
        public int NewHeader;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int InspectBufferDelegate(IntPtr data, nuint size, out NativeMlpInspectionNative result);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int InspectFileDelegate(IntPtr path, out NativeMlpInspectionNative result);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int ComparePcmDelegate(IntPtr source, IntPtr decoded, uint frameBytes,
        uint maxTrailingZeroFrames, out NativePcmComparisonNative result);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int ParsePtsDelegate(IntPtr data, nuint size, out long value);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int SampleRateDelegate(IntPtr data, nuint size, out int value);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int PeakBitrateDelegate(int sampleRate);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int Checksum16Delegate(IntPtr data, nuint size, out ushort value);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int Checksum8Delegate(IntPtr data, nuint size, out byte value);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate byte CalculateParityDelegate(IntPtr data, nuint size);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int AlignDelegate(IntPtr data, nuint size, out IntPtr alignedData,
        out nuint alignedSize, out NativeMlpAlignmentNative alignment);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate void FreeDelegate(IntPtr pointer);

    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
}
