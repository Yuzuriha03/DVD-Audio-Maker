using System.Security.Cryptography;

namespace DvdaMaker.Preparation;

/// <summary>
/// 判断文件是否变化的低代价证据：长度、最后写入时间，以及首尾各 64 KiB 的哈希。
/// 这不是整文件哈希，用于在“重新完整解码”与“复用已有结论”之间做取舍。
/// </summary>
public sealed record FileIdentity(
    string Path,
    long Size,
    long LastWriteUtcTicks,
    string HeadHash,
    string TailHash);

public static class FileIdentityProbe
{
    public const int SampleBytes = 64 * 1024;

    public static FileIdentity? Compute(string path)
    {
        try
        {
            var info = new FileInfo(path);
            if (!info.Exists)
            {
                return null;
            }
            using var stream = File.OpenRead(path);
            var head = new byte[(int)Math.Min(SampleBytes, info.Length)];
            stream.ReadExactly(head);
            var tail = new byte[head.Length];
            stream.Seek(Math.Max(0, info.Length - tail.Length), SeekOrigin.Begin);
            stream.ReadExactly(tail);
            return new FileIdentity(
                path,
                info.Length,
                info.LastWriteTimeUtc.Ticks,
                Hash(head),
                Hash(tail));
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or ArgumentException)
        {
            return null;
        }
    }

    public static bool Matches(string path, FileIdentity expected)
    {
        var current = Compute(path);
        return current is not null &&
            current.Size == expected.Size &&
            current.LastWriteUtcTicks == expected.LastWriteUtcTicks &&
            string.Equals(current.HeadHash, expected.HeadHash, StringComparison.Ordinal) &&
            string.Equals(current.TailHash, expected.TailHash, StringComparison.Ordinal);
    }

    private static string Hash(ReadOnlySpan<byte> data) =>
        Convert.ToHexString(SHA256.HashData(data))[..32];
}
