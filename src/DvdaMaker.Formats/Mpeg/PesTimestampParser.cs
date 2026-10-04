namespace DvdaMaker.Formats.Mpeg;

public static class PesTimestampParser
{
    public static long ParsePts(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length < 5)
        {
            throw new ArgumentException("PTS 至少需要 5 字节。", nameof(bytes));
        }

        if (NativeFormatsInterop.TryParsePts(bytes, out var nativeValue))
        {
            return nativeValue;
        }

        var high = ((uint)bytes[0] >> 1) & 0x07U;
        var middle = (((uint)bytes[1] << 8) | bytes[2]) >> 1;
        var low = (((uint)bytes[3] << 8) | bytes[4]) >> 1;
        var value = ((ulong)high << 30) | ((ulong)middle << 15) | low;
        return checked((long)value);
    }
}
