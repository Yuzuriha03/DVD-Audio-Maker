using System.Buffers.Binary;
using DvdaMaker.Formats.Mlp;

namespace DvdaMaker.Building;

public static class MlpCacheValidator
{
    private static ReadOnlySpan<byte> MajorSync => [0xF8, 0x72, 0x6F];
    private static ReadOnlySpan<byte> EndOfStream => [0xD2, 0x34, 0xD2, 0x34];

    public const int RequiredMajorSyncInterval = 8;

    // Original scheduling may balance restart spans; validate rather than rewrite its output.
    public static bool IsMlpEncoderValid(string path)
    {
        try
        {
            var inspection = MlpStreamAligner.Inspect(File.ReadAllBytes(path));
            return inspection.IsValid && inspection.HasEndOfStream;
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    public static bool IsValid(string path)
    {
        try
        {
            var info = new FileInfo(path);
            if (!info.Exists || info.Length < 4096)
            {
                return false;
            }

            using var stream = File.OpenRead(path);
            var head = new byte[Math.Min(128 * 1024, checked((int)info.Length))];
            stream.ReadExactly(head);
            stream.Seek(Math.Max(0, info.Length - 64), SeekOrigin.Begin);
            var tail = new byte[Math.Min(64, checked((int)info.Length))];
            stream.ReadExactly(tail);

            if (head.Length < 32 || !head.AsSpan(4, 3).SequenceEqual(MajorSync))
            {
                return false;
            }

            var interval = MajorSyncInterval(head);
            if (interval is not null && interval != RequiredMajorSyncInterval)
            {
                return false;
            }

            var major = head.AsSpan(4, MlpStreamAligner.MajorSyncSize);
            var sampleRate = MlpStreamAligner.SampleRateOf(major);
            if (sampleRate is null)
            {
                return false;
            }
            var peak = BinaryPrimitives.ReadUInt16BigEndian(major[14..16]) & 0x7FFF;
            if (peak != MlpStreamAligner.PeakBitrateRaw(sampleRate.Value) ||
                (major[16] & 3) != 1)
            {
                return false;
            }
            return tail.AsSpan().IndexOf(EndOfStream) >= 0;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    public static int? MajorSyncInterval(ReadOnlySpan<byte> head)
    {
        var position = 0;
        var accessUnit = 0;
        int? first = null;
        while (position + 4 <= head.Length)
        {
            var header = BinaryPrimitives.ReadUInt16BigEndian(head.Slice(position, 2));
            var length = (header & 0x0FFF) * 2;
            if (length < 4 || position + length > head.Length)
            {
                return null;
            }
            if (position + 7 <= head.Length &&
                head.Slice(position + 4, 3).SequenceEqual(MajorSync))
            {
                if (first is not null)
                {
                    return accessUnit - first.Value;
                }
                first = accessUnit;
            }
            position += length;
            accessUnit++;
        }
        return null;
    }
}
