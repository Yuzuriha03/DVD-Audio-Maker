using System.Buffers.Binary;
using System.Globalization;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

public sealed record AlacMagicCookie(
    int MaxSamplesPerFrame,
    int SampleSize,
    int Channels,
    int SampleRate,
    int MaxCodedFrameSize,
    int Offset);

public sealed record AlacPacket(double PresentationTime, int Size, long Position);

public sealed record AlacFramePatch(
    int PacketIndex,
    double PresentationTime,
    long Position,
    int Size,
    int SampleCount,
    long EndBit,
    int PreviousBits);

public sealed record AlacRepairResult(
    string OutputPath,
    AlacMagicCookie Cookie,
    IReadOnlyList<AlacFramePatch> Patches);

public sealed record AlacInspectionResult(
    AlacMagicCookie Cookie,
    IReadOnlyList<AlacFramePatch> Patches);

public sealed class AlacEndRepairer(ProcessRunner runner, string ffprobe)
{
    private const int HeaderBits = 23;
    private const int TypeEnd = 0b111;
    private const byte HasSizeMask = 0x10;
    private const byte ExtraBitsMask = 0x0C;
    private const byte UncompressedMask = 0x02;

    private static readonly HashSet<int> ValidSampleSizes = [16, 20, 24, 32];
    private static readonly HashSet<int> ValidSampleRates =
    [
        8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
        64_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
    ];

    public async Task<AlacInspectionResult> InspectAsync(
        string sourcePath,
        CancellationToken cancellationToken = default)
    {
        if (!IsSupportedExtension(sourcePath))
        {
            throw new InvalidDataException($"不支持的 ALAC 容器扩展名: {sourcePath}");
        }

        var data = await File.ReadAllBytesAsync(sourcePath, cancellationToken).ConfigureAwait(false);
        var cookie = ReadMagicCookie(data) ??
            throw new InvalidDataException("无法读取 ALAC magic cookie，可能不是 ALAC 文件");
        var packets = await ListPacketsAsync(sourcePath, cancellationToken).ConfigureAwait(false);
        return new AlacInspectionResult(cookie, FindBadFrames(data, cookie, packets));
    }

    public async Task<AlacRepairResult?> TryRepairAsync(
        string sourcePath,
        string outputDirectory,
        CancellationToken cancellationToken = default)
    {
        if (!IsSupportedExtension(sourcePath))
        {
            return null;
        }

        var data = await File.ReadAllBytesAsync(sourcePath, cancellationToken).ConfigureAwait(false);
        var cookie = ReadMagicCookie(data);
        if (cookie is null)
        {
            return null;
        }

        var packets = await ListPacketsAsync(sourcePath, cancellationToken).ConfigureAwait(false);
        var patches = FindBadFrames(data, cookie, packets);
        if (patches.Count == 0)
        {
            return null;
        }

        ApplyPatches(data, patches);
        Directory.CreateDirectory(outputDirectory);
        var outputPath = Path.Combine(outputDirectory, Path.GetFileName(sourcePath));
        await File.WriteAllBytesAsync(outputPath, data, cancellationToken).ConfigureAwait(false);
        return new AlacRepairResult(outputPath, cookie, patches);
    }

    public static AlacMagicCookie? ReadMagicCookie(ReadOnlySpan<byte> data)
    {
        ReadOnlySpan<byte> signature = "alac"u8;
        var searchOffset = 0;
        while (searchOffset <= data.Length - signature.Length)
        {
            var relative = data[searchOffset..].IndexOf(signature);
            if (relative < 0)
            {
                return null;
            }

            var signatureOffset = searchOffset + relative;
            foreach (var skip in new[] { 8, 12, 4 })
            {
                var offset = signatureOffset + skip;
                if (offset + 24 > data.Length)
                {
                    continue;
                }
                var cookie = ParseCookie(data.Slice(offset, 24), offset);
                if (cookie is not null)
                {
                    return cookie;
                }
            }
            searchOffset = signatureOffset + 1;
        }
        return null;
    }

    public static IReadOnlyList<AlacFramePatch> FindBadFrames(
        ReadOnlySpan<byte> data,
        AlacMagicCookie cookie,
        IReadOnlyList<AlacPacket> packets)
    {
        var patches = new List<AlacFramePatch>();
        for (var packetIndex = 0; packetIndex < packets.Count; packetIndex++)
        {
            var packet = packets[packetIndex];
            if (packet.Size < 4 || packet.Position < 0 ||
                packet.Position > data.Length - packet.Size)
            {
                continue;
            }

            var packetData = data.Slice(checked((int)packet.Position), packet.Size);
            var headerByte = packetData[2];
            if ((headerByte & UncompressedMask) == 0)
            {
                continue;
            }

            var hasSize = (headerByte & HasSizeMask) != 0;
            var sampleCount = hasSize && packetData.Length >= 7
                ? checked((int)ReadBits(packetData, 22, 32))
                : cookie.MaxSamplesPerFrame;
            if (sampleCount <= 0)
            {
                sampleCount = cookie.MaxSamplesPerFrame;
            }

            var endBit = HeaderBits + (long)sampleCount * cookie.Channels * cookie.SampleSize;
            if (endBit < 0 || endBit + 3 > (long)packet.Size * 8)
            {
                continue;
            }

            var current = checked((int)ReadBits(packetData, endBit, 3));
            if (current != TypeEnd)
            {
                patches.Add(new AlacFramePatch(
                    packetIndex,
                    packet.PresentationTime,
                    packet.Position,
                    packet.Size,
                    sampleCount,
                    endBit,
                    current));
            }
        }
        return patches;
    }

    public static void ApplyPatches(Span<byte> data, IReadOnlyList<AlacFramePatch> patches)
    {
        foreach (var patch in patches)
        {
            for (var bit = 0; bit < 3; bit++)
            {
                var absoluteBit = checked(patch.Position * 8 + patch.EndBit + bit);
                var byteIndex = checked((int)(absoluteBit / 8));
                var bitInByte = checked((int)(absoluteBit % 8));
                data[byteIndex] |= (byte)(1 << (7 - bitInByte));
            }
        }
    }

    private async Task<IReadOnlyList<AlacPacket>> ListPacketsAsync(
        string path,
        CancellationToken cancellationToken)
    {
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments =
            [
                "-v", "error",
                "-select_streams", "a:0",
                "-show_entries", "packet=pts_time,duration_time,size,pos",
                "-of", "csv=p=0",
                path,
            ],
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded)
        {
            throw new InvalidDataException(
                $"ffprobe 无法枚举 ALAC 包（退出码 {result.ExitCode}）: {path}");
        }

        var packets = new List<AlacPacket>();
        foreach (var rawLine in result.StandardOutput.Split('\n'))
        {
            var fields = rawLine.Trim().Split(',');
            if (fields.Length < 4 ||
                !int.TryParse(fields[2], NumberStyles.Integer, CultureInfo.InvariantCulture,
                    out var size) ||
                !long.TryParse(fields[3], NumberStyles.Integer, CultureInfo.InvariantCulture,
                    out var position))
            {
                continue;
            }
            var presentationTime = 0d;
            if (fields[0].Length > 0)
            {
                double.TryParse(fields[0], NumberStyles.Float, CultureInfo.InvariantCulture,
                    out presentationTime);
            }
            packets.Add(new AlacPacket(presentationTime, size, position));
        }
        return packets;
    }

    private static AlacMagicCookie? ParseCookie(ReadOnlySpan<byte> data, int offset)
    {
        var maxSamples = BinaryPrimitives.ReadInt32BigEndian(data[..4]);
        var sampleSize = data[5];
        var channels = data[9];
        var maxCodedFrameSize = BinaryPrimitives.ReadInt32BigEndian(data[12..16]);
        var sampleRate = BinaryPrimitives.ReadInt32BigEndian(data[20..24]);
        if (maxSamples is < 512 or > 16_384 ||
            !ValidSampleSizes.Contains(sampleSize) ||
            channels is < 1 or > 8 ||
            !ValidSampleRates.Contains(sampleRate))
        {
            return null;
        }
        return new AlacMagicCookie(
            maxSamples, sampleSize, channels, sampleRate, maxCodedFrameSize, offset);
    }

    private static ulong ReadBits(ReadOnlySpan<byte> data, long bitOffset, int count)
    {
        if (count is < 1 or > 64 || bitOffset < 0 || bitOffset + count > (long)data.Length * 8)
        {
            throw new ArgumentOutOfRangeException(nameof(bitOffset));
        }
        ulong value = 0;
        for (var index = 0; index < count; index++)
        {
            var currentBit = bitOffset + index;
            var byteIndex = checked((int)(currentBit / 8));
            var bitInByte = checked((int)(currentBit % 8));
            value = (value << 1) | (uint)((data[byteIndex] >> (7 - bitInByte)) & 1);
        }
        return value;
    }

    private static bool IsSupportedExtension(string path) =>
        Path.GetExtension(path).ToLowerInvariant() is ".m4a" or ".mp4" or ".alac";
}
