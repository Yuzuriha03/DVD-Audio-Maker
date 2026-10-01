using System.Buffers.Binary;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace DvdaMaker.SurcodeTool;

/// <summary>In-process bridge to the pinned x64 encoder. No encoder process or output rewriting.</summary>
public static class MlpEncoder
{
    public const string BinarySha256 = "1eaa3a212a8a703d416874583688c46532fec393266a63ea402253e2f29905e4";
    public const string MetadataPolicy = "empty-auxiliary-tlv-v1";
    private static readonly Lazy<EncodeDelegate> Encoder = new(LoadEncoder);

    public static string ExtractLibrary()
    {
        if (!OperatingSystem.IsWindows() || !Environment.Is64BitProcess)
            throw new PlatformNotSupportedException("MLP DLL 需要 Windows x64 主程序，请使用 win-x64 发布包。");
        var folder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "DVD-Audio-Maker", "native", BinarySha256);
        Directory.CreateDirectory(folder);
        var path = Path.Combine(folder, "mlp_encoder.dll");
        if (File.Exists(path)) { Verify(path); return path; }
        var temporary = Path.Combine(folder, Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            using (var input = typeof(MlpEncoder).Assembly.GetManifestResourceStream("DvdaMaker.MlpEncoder.Library")
                ?? throw new InvalidOperationException("发布包缺少MLP DLL。"))
            using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                input.CopyTo(output);
            Verify(temporary);
            try { File.Move(temporary, path); }
            catch (IOException) when (File.Exists(path)) { Verify(path); }
            return path;
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    private static void Verify(string path)
    {
        using var stream = File.OpenRead(path);
        if (!Convert.ToHexString(SHA256.HashData(stream)).Equals(BinarySha256, StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("MLP DLL 哈希不符，请清理损坏的原生核心缓存后重试。");
    }

    private static EncodeDelegate LoadEncoder()
    {
        if (Marshal.SizeOf<NativeConfig>() != 48 || Marshal.SizeOf<NativeStamp>() != 32 || Marshal.SizeOf<NativeResult>() != 224)
            throw new InvalidOperationException("MLP DLL ABI 布局不匹配。");
        var library = NativeLibrary.Load(ExtractLibrary()); // Keep loaded for process lifetime / concurrent calls.
        var version = Marshal.GetDelegateForFunctionPointer<VersionDelegate>(NativeLibrary.GetExport(library, "mlp_encoder_abi_version"));
        if (version() != 1) { NativeLibrary.Free(library); throw new InvalidOperationException("不支持的 MLP DLL ABI。"); }
        return Marshal.GetDelegateForFunctionPointer<EncodeDelegate>(NativeLibrary.GetExport(library, "mlp_encode_stream_layout"));
    }

    public static void WriteMetadata(string path, long frames, int sampleRate)
    {
        var units = AccessUnits(frames, sampleRate);
        using var writer = new BinaryWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write));
        writer.Write("MSCTX001"u8); writer.Write(units); writer.Write(1u);
        writer.Write(0UL); writer.Write(4u); writer.Write(new byte[] { 0, 0, 0x40, 0 });
    }

    private static ulong AccessUnits(long frames, int rate)
    {
        if (frames <= 0 || rate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000))
            throw new InvalidDataException("无效的 PCM 帧数或采样率。");
        var block = 40 * (rate / (rate % 44100 == 0 ? 44100 : 48000));
        return checked((ulong)((frames + block - 1) / block));
    }

    public static Task EncodeAsync(string wave, string destination, string metadataContext,
        TimeSpan timeout, CancellationToken cancellationToken) => Task.Run(() =>
    {
        using var timed = new CancellationTokenSource(timeout);
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, timed.Token);
        try { Encode(wave, destination, metadataContext, linked.Token); }
        catch (OperationCanceledException) when (timed.IsCancellationRequested && !cancellationToken.IsCancellationRequested)
        { throw new TimeoutException("MLP DLL 编码超时，已取消并清理临时输出。"); }
    }, cancellationToken);

    private static void Encode(string wave, string destination, string context, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        var layout = SurcodePcmWav.ReadLayout(wave);
        var frames = layout.DataSize / (layout.BytesPerSample * layout.Channels);
        var assignment = Assignment(layout.ChannelMask);
        var encode = Encoder.Value;
        using var metadata = new Metadata(context, AccessUnits(frames, layout.SampleRate));
        if (File.Exists(destination)) throw new IOException("拒绝覆盖已有的 MLP 输出文件。");
        var temporary = destination + "." + Guid.NewGuid().ToString("N") + ".partial";
        var created = false;
        try
        {
            using (var input = File.OpenRead(wave))
            using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                created = true;
                input.Position = layout.DataOffset;
                var remaining = frames;
                var raw = new byte[8192 * layout.Channels * layout.BytesPerSample];
                var pcm = new int[8192 * layout.Channels];
                var encoded = new byte[8190];
                ExceptionDispatchInfo? failure = null;
                ReadDelegate read = (IntPtr _, IntPtr samples, nuint capacity, out nuint count) =>
                {
                    count = 0;
                    try
                    {
                        token.ThrowIfCancellationRequested();
                        var take = (int)Math.Min(Math.Min((ulong)capacity, 8192UL), (ulong)remaining);
                        var values = take * layout.Channels;
                        input.ReadExactly(raw.AsSpan(0, values * layout.BytesPerSample));
                        for (var i = 0; i < values; i++)
                        {
                            var at = i * layout.BytesPerSample;
                            pcm[i] = layout.BytesPerSample switch
                            {
                                2 => BinaryPrimitives.ReadInt16LittleEndian(raw.AsSpan(at)) << 8,
                                3 => (int)((uint)(raw[at] | raw[at + 1] << 8 | raw[at + 2] << 16) << 8) >> 8,
                                4 => BinaryPrimitives.ReadInt32LittleEndian(raw.AsSpan(at)) >> 8,
                                _ => throw new InvalidDataException("不支持的 PCM 存储格式。"),
                            };
                            if (layout.BytesPerSample == 4 && raw[at] != 0)
                                throw new InvalidDataException("32 位 PCM 包含无法无损表示的有效低位。");
                        }
                        Marshal.Copy(pcm, 0, samples, values);
                        remaining -= take; count = (nuint)take;
                        return 0;
                    }
                    catch (Exception exception) { failure = ExceptionDispatchInfo.Capture(exception); return 1; }
                };
                WriteDelegate write = (_, bytes, count) =>
                {
                    try
                    {
                        token.ThrowIfCancellationRequested();
                        var size = checked((int)count);
                        if (size > encoded.Length) throw new InvalidDataException("MLP DLL 输出块超过 ABI 上限。");
                        Marshal.Copy(bytes, encoded, 0, size); output.Write(encoded, 0, size); return 0;
                    }
                    catch (Exception exception) { failure = ExceptionDispatchInfo.Capture(exception); return 1; }
                };
                var config = new NativeConfig
                {
                    StructSize = 48,
                    AbiVersion = 1,
                    SampleRate = (uint)layout.SampleRate,
                    Bits = (uint)layout.ValidBits,
                    Channels = (uint)layout.Channels,
                    Frames = (ulong)frames,
                    Metadata = metadata.Pointer,
                    MetadataCount = (nuint)metadata.Count,
                };
                var status = encode(ref config, assignment, read, IntPtr.Zero, write, IntPtr.Zero, out var result);
                GC.KeepAlive(read); GC.KeepAlive(write);
                failure?.Throw(); token.ThrowIfCancellationRequested();
                if (status != 0 || result.Status != 0 || result.InputFrames != (ulong)frames ||
                    result.OutputBytes != (ulong)output.Length || output.Length == 0)
                    throw new InvalidOperationException($"MLP DLL 编码失败（{status}）：{result.Error}");
                Console.WriteLine($"[MLP DLL] {frames:N0} 帧 / {layout.SampleRate} Hz / {layout.ValidBits} bit / {layout.Channels} 声道，{output.Length:N0} 字节");
            }
            token.ThrowIfCancellationRequested();
            File.Move(temporary, destination, overwrite: false);
        }
        finally { if (created && File.Exists(temporary)) File.Delete(temporary); }
    }

    private static uint Assignment(uint mask) => mask switch
    {
        4 => 0,
        3 => 1,
        0x103 => 2,
        0x33 => 3,
        0x0b => 4,
        0x10b => 5,
        0x3b => 6,
        7 => 7,
        0x107 => 8,
        0x37 => 9,
        0x0f => 10,
        0x10f => 11,
        0x3f => 12,
        _ => throw new InvalidDataException($"不支持的 DVD-Audio 声道掩码：0x{mask:X}。"),
    };

    private sealed class Metadata : IDisposable
    {
        private readonly List<IntPtr> _packets = [];
        public IntPtr Pointer { get; private set; }
        public int Count { get; private set; }
        public Metadata(string path, ulong units)
        {
            try
            {
                var records = new List<(ulong Start, byte[] Packet, uint Valid)>();
                if (string.IsNullOrWhiteSpace(path)) records.Add((0, [0, 0, 0x40, 0], 0));
                else
                {
                    using var reader = new BinaryReader(File.OpenRead(path));
                    var magic = Encoding.ASCII.GetString(reader.ReadBytes(8));
                    if (magic is not ("MSCTX001" or "MSCTX002") || reader.ReadUInt64() != units)
                        throw new InvalidDataException("MLP 元数据上下文格式或 AU 总数与输入不符。");
                    var count = reader.ReadUInt32();
                    if (count is 0 or > 4096) throw new InvalidDataException("元数据记录数量无效。");
                    long total = 0;
                    for (var i = 0; i < count; i++)
                    {
                        var start = reader.ReadUInt64(); var size = reader.ReadUInt32();
                        var valid = magic == "MSCTX002" ? reader.ReadUInt32() : 0;
                        total += size;
                        if (size is < 4 or > 65539 || total > 16777216)
                            throw new InvalidDataException("元数据记录长度无效。");
                        var packet = reader.ReadBytes((int)size);
                        if (packet.Length != size) throw new InvalidDataException("元数据上下文已截断。");
                        records.Add((start, packet, valid));
                    }
                    if (reader.BaseStream.Position != reader.BaseStream.Length)
                        throw new InvalidDataException("元数据上下文包含多余数据。");
                }
                Count = records.Count; Pointer = Marshal.AllocHGlobal(checked(32 * Count));
                for (var i = 0; i < Count; i++)
                {
                    var r = records[i]; var packet = Marshal.AllocHGlobal(r.Packet.Length); _packets.Add(packet);
                    Marshal.Copy(r.Packet, 0, packet, r.Packet.Length);
                    Marshal.StructureToPtr(new NativeStamp { Start = r.Start, Size = (uint)r.Packet.Length, Packet = packet, ValidBits = r.Valid },
                        IntPtr.Add(Pointer, 32 * i), false);
                }
            }
            catch { Dispose(); throw; }
        }
        public void Dispose()
        {
            foreach (var packet in _packets) Marshal.FreeHGlobal(packet); _packets.Clear();
            if (Pointer != IntPtr.Zero) Marshal.FreeHGlobal(Pointer); Pointer = IntPtr.Zero;
        }
    }

    [StructLayout(LayoutKind.Sequential, Pack = 8)]
    private struct NativeConfig
    {
        public uint StructSize, AbiVersion, SampleRate, Bits, Channels, RestartInterval;
        public ulong Frames;
        public IntPtr Metadata;
        public nuint MetadataCount;
    }
    [StructLayout(LayoutKind.Sequential, Pack = 8)]
    private struct NativeStamp { public ulong Start; public uint Size; public IntPtr Packet; public uint ValidBits; }
    [StructLayout(LayoutKind.Sequential, Pack = 8, CharSet = CharSet.Ansi)]
    private struct NativeResult
    {
        public int Status; public uint AccessUnits; public ulong InputFrames, EncodedFrames, OutputBytes;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 192)] public string Error;
    }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate uint VersionDelegate();
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int ReadDelegate(IntPtr state, IntPtr pcm, nuint capacity, out nuint frames);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int WriteDelegate(IntPtr state, IntPtr bytes, nuint count);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int EncodeDelegate(ref NativeConfig config, uint assignment,
        ReadDelegate read, IntPtr input, WriteDelegate write, IntPtr output, out NativeResult result);
}
