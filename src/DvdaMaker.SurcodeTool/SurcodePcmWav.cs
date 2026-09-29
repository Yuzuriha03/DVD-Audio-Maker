using System.Text;

namespace DvdaMaker.SurcodeTool;

public static class SurcodePcmWav
{
    private static readonly string[] ChannelSuffixes =
        [".L.wav", ".R.wav", ".C.wav", ".LFE.wav", ".SL.wav", ".SR.wav", ".S.wav"];

    public static void UpconvertProducedFiles(string directory, string baseName, int targetBits)
    {
        foreach (var suffix in ChannelSuffixes)
        {
            var path = Path.Combine(directory, baseName + suffix);
            if (!File.Exists(path))
            {
                continue;
            }

            var layout = ReadLayout(path);
            var sourceBits = layout.ValidBits > 0 ? layout.ValidBits : layout.ContainerBits;
            if (sourceBits >= targetBits)
            {
                continue;
            }

            var temporary = path + ".up.wav";
            File.Delete(temporary);
            ConvertBitDepth(path, temporary, targetBits, layout);
            File.Move(temporary, path, overwrite: true);
        }
    }

    public static WavLayout ReadLayout(string path)
    {
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        Span<byte> header = stackalloc byte[12];
        if (stream.Read(header) != header.Length ||
            Encoding.ASCII.GetString(header[..4]) != "RIFF" ||
            Encoding.ASCII.GetString(header[8..]) != "WAVE")
        {
            throw new InvalidDataException($"不是有效的 WAV 文件: {path}");
        }

        var containerBits = 0;
        var validBits = 0;
        var channels = 0;
        var sampleRate = 0;
        long dataOffset = -1;
        long dataSize = 0;
        Span<byte> chunkHeader = stackalloc byte[8];
        while (stream.Position + chunkHeader.Length <= stream.Length)
        {
            if (stream.Read(chunkHeader) != chunkHeader.Length)
            {
                break;
            }
            var chunkId = Encoding.ASCII.GetString(chunkHeader[..4]);
            var chunkSize = BitConverter.ToInt32(chunkHeader[4..]);
            if (chunkSize < 0)
            {
                break;
            }

            if (chunkId == "fmt ")
            {
                if (chunkSize is < 16 or > 4096)
                {
                    throw new InvalidDataException($"WAV fmt 块无效: {path}");
                }
                var format = new byte[chunkSize];
                stream.ReadExactly(format);
                var formatTag = BitConverter.ToUInt16(format, 0);
                channels = BitConverter.ToUInt16(format, 2);
                sampleRate = BitConverter.ToInt32(format, 4);
                containerBits = BitConverter.ToUInt16(format, 14);
                validBits = containerBits;
                if (formatTag == 0xFFFE && format.Length >= 20)
                {
                    var declaredValidBits = BitConverter.ToUInt16(format, 18);
                    if (declaredValidBits > 0)
                    {
                        validBits = declaredValidBits;
                    }
                }
                if ((chunkSize & 1) != 0)
                {
                    stream.Seek(1, SeekOrigin.Current);
                }
                continue;
            }

            if (chunkId == "data")
            {
                dataOffset = stream.Position;
                dataSize = Math.Min(chunkSize, stream.Length - dataOffset);
                break;
            }

            stream.Seek(chunkSize + (chunkSize & 1), SeekOrigin.Current);
        }

        if (containerBits <= 0 || channels <= 0 || sampleRate <= 0 || dataOffset < 0)
        {
            throw new InvalidDataException($"WAV 头不完整: {path}");
        }
        return new WavLayout(
            containerBits,
            validBits,
            channels,
            sampleRate,
            dataOffset,
            dataSize,
            containerBits / 8);
    }

    private static void ConvertBitDepth(
        string sourcePath,
        string destinationPath,
        int targetBits,
        WavLayout layout)
    {
        var sourceBits = layout.ValidBits > 0 ? layout.ValidBits : layout.ContainerBits;
        var outputBits = targetBits <= 16 ? 16 : 24;
        if (outputBits < sourceBits)
        {
            throw new InvalidOperationException(
                $"拒绝在内部升位步骤中降低位深: {sourceBits} -> {outputBits} bit");
        }
        if (layout.BytesPerSample is <= 0 or > 4)
        {
            throw new InvalidDataException(
                $"不支持的 WAV 样本宽度: {layout.BytesPerSample} bytes");
        }

        var outputBytesPerSample = outputBits / 8;
        var sourceFrameBytes = layout.Channels * layout.BytesPerSample;
        var outputBlockAlign = layout.Channels * outputBytesPerSample;
        var frames = layout.DataSize / sourceFrameBytes;
        var outputDataSize = frames * outputBlockAlign;
        if (outputDataSize > int.MaxValue)
        {
            throw new InvalidDataException("WAV 数据超过 RIFF 32 位长度限制。");
        }

        using var input = new FileStream(sourcePath, FileMode.Open, FileAccess.Read, FileShare.Read);
        using var output = new BinaryWriter(new FileStream(destinationPath, FileMode.Create));
        output.Write(Encoding.ASCII.GetBytes("RIFF"));
        output.Write(checked((int)(36 + outputDataSize)));
        output.Write(Encoding.ASCII.GetBytes("WAVEfmt "));
        output.Write(16);
        output.Write((short)1);
        output.Write(checked((short)layout.Channels));
        output.Write(layout.SampleRate);
        output.Write(checked(layout.SampleRate * outputBlockAlign));
        output.Write(checked((short)outputBlockAlign));
        output.Write(checked((short)outputBits));
        output.Write(Encoding.ASCII.GetBytes("data"));
        output.Write(checked((int)outputDataSize));

        input.Seek(layout.DataOffset, SeekOrigin.Begin);
        const int bufferFrames = 65_536;
        var inputBuffer = new byte[bufferFrames * sourceFrameBytes];
        var outputBuffer = new byte[bufferFrames * outputBlockAlign];
        var remaining = layout.DataSize;
        var shift = outputBits - sourceBits;
        while (remaining > 0)
        {
            var wanted = checked((int)Math.Min(inputBuffer.Length, remaining));
            wanted -= wanted % sourceFrameBytes;
            if (wanted <= 0)
            {
                break;
            }
            input.ReadExactly(inputBuffer.AsSpan(0, wanted));

            var frameCount = wanted / sourceFrameBytes;
            var outputPosition = 0;
            for (var frame = 0; frame < frameCount; frame++)
            {
                for (var channel = 0; channel < layout.Channels; channel++)
                {
                    var offset = (frame * layout.Channels + channel) * layout.BytesPerSample;
                    var value = ReadSignedSample(inputBuffer, offset, layout.BytesPerSample);
                    var scaled = shift == 0 ? value : value << shift;
                    outputBuffer[outputPosition++] = (byte)(scaled & 0xFF);
                    outputBuffer[outputPosition++] = (byte)((scaled >> 8) & 0xFF);
                    if (outputBytesPerSample == 3)
                    {
                        outputBuffer[outputPosition++] = (byte)((scaled >> 16) & 0xFF);
                    }
                }
            }
            output.Write(outputBuffer, 0, outputPosition);
            remaining -= wanted;
        }
    }

    private static int ReadSignedSample(byte[] buffer, int offset, int bytesPerSample) =>
        bytesPerSample switch
        {
            1 => buffer[offset] - 128,
            2 => (short)(buffer[offset] | buffer[offset + 1] << 8),
            3 => SignExtend24(
                buffer[offset] | buffer[offset + 1] << 8 | buffer[offset + 2] << 16),
            4 => buffer[offset] | buffer[offset + 1] << 8 |
                buffer[offset + 2] << 16 | buffer[offset + 3] << 24,
            _ => throw new InvalidDataException("不支持的 PCM 样本宽度。"),
        };

    private static int SignExtend24(int value) =>
        value >= 0x800000 ? value - 0x1000000 : value;

    public sealed record WavLayout(
        int ContainerBits,
        int ValidBits,
        int Channels,
        int SampleRate,
        long DataOffset,
        long DataSize,
        int BytesPerSample);
}
