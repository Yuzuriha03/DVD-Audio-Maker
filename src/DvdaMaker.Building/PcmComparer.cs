namespace DvdaMaker.Building;

public sealed record PcmComparison(
    bool Match,
    long SourceBytes,
    long DecodedBytes,
    string? Reason,
    long TrailingZeroBytes = 0);

/// <summary>
/// 逐字节比较两段已解码的 PCM。默认要求等长；调用方可以明确允许
/// 有界、完整采样帧构成的零值尾部填充。
/// </summary>
public static class PcmComparer
{
    private const int BufferSize = 128 * 1024;

    public static PcmComparison Compare(
        string sourcePath, string decodedPath,
        int bytesPerSampleFrame = 0, int maxTrailingZeroFrames = 0)
    {
        if (bytesPerSampleFrame < 0 || maxTrailingZeroFrames < 0 ||
            (maxTrailingZeroFrames > 0 && bytesPerSampleFrame == 0))
        {
            throw new ArgumentOutOfRangeException(nameof(maxTrailingZeroFrames));
        }
        var sourceBytes = new FileInfo(sourcePath).Length;
        var decodedBytes = new FileInfo(decodedPath).Length;
        var extraBytes = decodedBytes - sourceBytes;
        var paddingAllowed = maxTrailingZeroFrames > 0 &&
            extraBytes > 0 && sourceBytes % bytesPerSampleFrame == 0 &&
            extraBytes % bytesPerSampleFrame == 0 &&
            extraBytes <= (long)bytesPerSampleFrame * maxTrailingZeroFrames;
        if (extraBytes != 0 && !paddingAllowed)
        {
            return new PcmComparison(false, sourceBytes, decodedBytes,
                $"解码 PCM {decodedBytes:N0} 字节 != 源 PCM {sourceBytes:N0} 字节" +
                $"（相差 {decodedBytes - sourceBytes:N0} 字节）");
        }

        using var source = File.OpenRead(sourcePath);
        using var decoded = File.OpenRead(decodedPath);
        var reason = FirstDifference(source, decoded, sourceBytes);
        if (reason is null && paddingAllowed)
        {
            decoded.Position = sourceBytes;
            var remaining = extraBytes;
            var buffer = new byte[BufferSize];
            while (remaining > 0)
            {
                var read = decoded.Read(buffer, 0, (int)Math.Min(buffer.Length, remaining));
                if (read == 0 || buffer.AsSpan(0, read).IndexOfAnyExcept((byte)0) >= 0)
                {
                    reason = "MLP 解码 PCM 的尾部填充包含非零字节或读取不完整";
                    break;
                }
                remaining -= read;
            }
            if (reason is null)
            {
                return new PcmComparison(true, sourceBytes, decodedBytes, null, extraBytes);
            }
        }
        return new PcmComparison(reason is null, sourceBytes, decodedBytes, reason);
    }

    private static string? FirstDifference(Stream left, Stream right, long length)
    {
        var leftBuffer = new byte[BufferSize];
        var rightBuffer = new byte[BufferSize];
        long readTotal = 0;
        while (readTotal < length)
        {
            var wanted = (int)Math.Min(BufferSize, length - readTotal);
            var leftRead = left.Read(leftBuffer, 0, wanted);
            var rightRead = right.Read(rightBuffer, 0, wanted);
            if (leftRead != rightRead || leftRead == 0)
            {
                return $"读取长度不一致（偏移 {readTotal:N0} 字节处）";
            }
            if (!leftBuffer.AsSpan(0, leftRead).SequenceEqual(rightBuffer.AsSpan(0, rightRead)))
            {
                var offset = readTotal + FirstMismatch(leftBuffer, rightBuffer, leftRead);
                return $"PCM 内容不一致（首个不同字节偏移 {offset:N0}）";
            }
            readTotal += leftRead;
        }
        return null;
    }

    private static int FirstMismatch(byte[] left, byte[] right, int count)
    {
        for (var index = 0; index < count; index++)
        {
            if (left[index] != right[index])
            {
                return index;
            }
        }
        return 0;
    }
}
