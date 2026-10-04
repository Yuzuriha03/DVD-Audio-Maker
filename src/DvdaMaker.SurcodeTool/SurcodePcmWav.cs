using System.Buffers.Binary;

namespace DvdaMaker.SurcodeTool;

/// <summary>Integer WAVE validation and lossless storage/valid-bit normalization.</summary>
public static class SurcodePcmWav
{
    private static ReadOnlySpan<byte> PcmGuid => [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
    public sealed record WavLayout(int ContainerBits, int ValidBits, int Channels, int SampleRate,
        long DataOffset, long DataSize, int BytesPerSample, uint ChannelMask);

    public static WavLayout ReadLayout(string path)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
        {
            return DvdaMaker.Processes.RustBridge.Run<WavLayout>("wav.layout", path,
                () => ReadLayoutManaged(path));
        }
        return ReadLayoutManaged(path);
    }

    private static WavLayout ReadLayoutManaged(string path)
    {
        using var stream = File.OpenRead(path);
        Span<byte> head = stackalloc byte[12]; stream.ReadExactly(head);
        if (!head[..4].SequenceEqual("RIFF"u8) || !head[8..].SequenceEqual("WAVE"u8))
            throw new InvalidDataException("编码输入必须是整数 RIFF/WAVE PCM。");
        var end = checked((long)BinaryPrimitives.ReadUInt32LittleEndian(head[4..]) + 8);
        if (end > stream.Length || end < 12) throw new InvalidDataException("WAVE 长度字段无效或文件截断。");
        int bits = 0, valid = 0, channels = 0, rate = 0, align = 0; uint mask = 0;
        long data = -1, size = 0; Span<byte> chunk = stackalloc byte[8];
        while (stream.Position + 8 <= end)
        {
            stream.ReadExactly(chunk);
            var length = (long)BinaryPrimitives.ReadUInt32LittleEndian(chunk[4..]);
            var next = checked(stream.Position + length + (length & 1));
            // eac3to omits the RIFF alignment byte after an odd final data chunk.
            // Accept only that terminal case; actual PCM truncation still fails.
            if ((length & 1) != 0 && chunk[..4].SequenceEqual("data"u8) &&
                stream.Position + length == end && end == stream.Length) next = end;
            if (next > end) throw new InvalidDataException("WAVE 块超出文件边界。");
            if (chunk[..4].SequenceEqual("fmt "u8))
            {
                if (bits != 0 || length is < 16 or > 4096) throw new InvalidDataException("WAVE fmt 块无效。");
                var fmt = new byte[(int)length]; stream.ReadExactly(fmt);
                var tag = BinaryPrimitives.ReadUInt16LittleEndian(fmt);
                channels = BinaryPrimitives.ReadUInt16LittleEndian(fmt.AsSpan(2));
                rate = checked((int)BinaryPrimitives.ReadUInt32LittleEndian(fmt.AsSpan(4)));
                align = BinaryPrimitives.ReadUInt16LittleEndian(fmt.AsSpan(12));
                bits = BinaryPrimitives.ReadUInt16LittleEndian(fmt.AsSpan(14)); valid = bits;
                if (tag == 0xfffe)
                {
                    if (length < 40 || BinaryPrimitives.ReadUInt16LittleEndian(fmt.AsSpan(16)) < 22 ||
                        !fmt.AsSpan(24, 16).SequenceEqual(PcmGuid)) throw new InvalidDataException("WAVE 扩展格式不是整数 PCM。");
                    valid = BinaryPrimitives.ReadUInt16LittleEndian(fmt.AsSpan(18));
                    mask = BinaryPrimitives.ReadUInt32LittleEndian(fmt.AsSpan(20));
                    if (valid == 0) valid = bits;
                }
                else if (tag != 1) throw new InvalidDataException("拒绝浮点或压缩 WAVE 输入。");
            }
            else if (chunk[..4].SequenceEqual("data"u8))
            {
                if (data >= 0) throw new InvalidDataException("重复的 WAVE data 块。");
                data = stream.Position; size = length;
            }
            stream.Position = next;
        }
        if (stream.Position != end || bits is not (16 or 24 or 32) || valid < 1 || valid > bits ||
            channels is < 1 or > 6 || align != channels * (bits / 8) || data < 0 || size <= 0 || size % align != 0 ||
            rate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000) || (rate > 96000 && channels > 2))
            throw new InvalidDataException("WAVE PCM 参数或帧边界不符合 DVD-Audio 格式。");
        if (mask == 0) mask = channels switch { 1 => 4u, 2 => 3u, 3 => 7u, 4 => 0x33u, 5 => 0x37u, 6 => 0x3fu, _ => 0u };
        if (System.Numerics.BitOperations.PopCount(mask) != channels) throw new InvalidDataException("WAVE 声道掩码与声道数不符。");
        return new(bits, valid, channels, rate, data, size, bits / 8, mask);
    }

    public static void Normalize(string source, string destination, int rate, int bits, CancellationToken token = default)
    {
        if (DvdaMaker.Processes.RustBridge.Mode == "rust")
        {
            token.ThrowIfCancellationRequested();
            _ = DvdaMaker.Processes.RustBridge.Invoke<NormalizeResult>("wav.normalize",
                new { Source = Path.GetFullPath(source), Destination = Path.GetFullPath(destination), Rate = rate, Bits = bits });
            token.ThrowIfCancellationRequested();
            return;
        }
        if (DvdaMaker.Processes.RustBridge.Mode == "compare")
        {
            token.ThrowIfCancellationRequested();
            var rustDestination = destination + ".rust-" + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                _ = DvdaMaker.Processes.RustBridge.Invoke<NormalizeResult>("wav.normalize",
                    new { Source = Path.GetFullPath(source), Destination = Path.GetFullPath(rustDestination), Rate = rate, Bits = bits });
                NormalizeManaged(source, destination, rate, bits, token);
                var managed = File.ReadAllBytes(destination);
                var rust = File.ReadAllBytes(rustDestination);
                if (!managed.AsSpan().SequenceEqual(rust))
                {
                    throw new InvalidDataException($"Rust WAV normalize mismatch at byte {FirstMismatch(managed, rust)}.");
                }
                return;
            }
            finally
            {
                if (File.Exists(rustDestination)) File.Delete(rustDestination);
            }
        }
        NormalizeManaged(source, destination, rate, bits, token);
    }

    private sealed record NormalizeResult(long WrittenBytes);

    private static int FirstMismatch(ReadOnlySpan<byte> left, ReadOnlySpan<byte> right)
    {
        var common = Math.Min(left.Length, right.Length);
        for (var index = 0; index < common; index++)
        {
            if (left[index] != right[index]) return index;
        }
        return common;
    }

    private static void NormalizeManaged(string source, string destination, int rate, int bits, CancellationToken token)
    {
        if (bits is not (16 or 20 or 24)) throw new InvalidDataException("目标位深必须是 16/20/24。");
        var layout = ReadLayout(source);
        if (layout.SampleRate != rate) throw new InvalidDataException("PCM 输出采样率与任务不符。");
        // WAVE sources may label DVD surround Ls/Rs as SIDE_LEFT/RIGHT. With no
        // separate rear pair these name the same two speakers, in the same order.
        var channelMask = layout.ChannelMask;
        if ((channelMask & 0x600) != 0)
        {
            if ((channelMask & 0x30) != 0 || (channelMask & 0x100) != 0)
                throw new InvalidDataException("不能把同时存在的侧置、后置或后中置声道合并为 DVD 环绕声道。");
            channelMask = (channelMask & ~0x600u) | ((channelMask & 0x600u) >> 5);
        }
        var outputWidth = bits == 16 ? 2 : 3;
        var sourceFrame = layout.Channels * layout.BytesPerSample;
        var frames = layout.DataSize / sourceFrame;
        var outputSize = checked(frames * layout.Channels * outputWidth);
        if (outputSize + 60 + (outputSize & 1) > uint.MaxValue) throw new InvalidDataException("PCM 超出 RIFF 4 GiB 限制；请拆分过长音轨。");
        using var input = File.OpenRead(source);
        var created = false;
        try
        {
            using (var output = new BinaryWriter(new FileStream(destination, FileMode.CreateNew, FileAccess.Write)))
            {
                created = true;
                output.Write("RIFF"u8); output.Write((uint)(60 + outputSize + (outputSize & 1))); output.Write("WAVEfmt "u8);
                output.Write(40u); output.Write((ushort)0xfffe); output.Write((ushort)layout.Channels); output.Write(rate);
                output.Write(checked(rate * layout.Channels * outputWidth)); output.Write((ushort)(layout.Channels * outputWidth));
                output.Write((ushort)(outputWidth * 8)); output.Write((ushort)22); output.Write((ushort)bits);
                output.Write(channelMask); output.Write(PcmGuid); output.Write("data"u8); output.Write((uint)outputSize);
                input.Position = layout.DataOffset;
                var buffer = new byte[8192 * sourceFrame]; var converted = new byte[8192 * layout.Channels * outputWidth];
                var remaining = layout.DataSize;
                while (remaining > 0)
                {
                    token.ThrowIfCancellationRequested();
                    var wanted = (int)Math.Min(buffer.Length, remaining); input.ReadExactly(buffer.AsSpan(0, wanted));
                    var pos = 0;
                    for (var at = 0; at < wanted; at += layout.BytesPerSample)
                    {
                        long sample = layout.BytesPerSample switch
                        {
                            2 => (long)BinaryPrimitives.ReadInt16LittleEndian(buffer.AsSpan(at)) << 8,
                            3 => (int)((uint)(buffer[at] | buffer[at + 1] << 8 | buffer[at + 2] << 16) << 8) >> 8,
                            4 => BinaryPrimitives.ReadInt32LittleEndian(buffer.AsSpan(at)),
                            _ => throw new InvalidDataException("无效的 PCM 宽度。"),
                        };
                        if (layout.BytesPerSample == 4)
                        {
                            if ((sample & 255) != 0) throw new InvalidDataException("32 位存储包含超过 24 位的有效 PCM。");
                            sample >>= 8;
                        }
                        if ((sample & ((1L << (24 - bits)) - 1)) != 0)
                            throw new InvalidDataException("PCM 有效精度高于目标位深；拒绝静默截断，请检查音源转换设置。");
                        if (bits == 16) sample >>= 8;
                        converted[pos++] = (byte)sample; converted[pos++] = (byte)(sample >> 8);
                        if (outputWidth == 3) converted[pos++] = (byte)(sample >> 16);
                    }
                    output.Write(converted, 0, pos); remaining -= wanted;
                }
                if ((outputSize & 1) != 0) output.Write((byte)0);
            }
        }
        catch { if (created) File.Delete(destination); throw; }
    }
}
