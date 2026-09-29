using System.Runtime.InteropServices;
using System.Text;

namespace DvdaMaker.SurcodeTool;

public static class SurcodeSsfWriter
{
    private static readonly byte[] Header =
    [
        7, 83, 117, 114, 99, 111, 100, 101, 3, 49, 46, 48, 32, 32, 32, 32, 32,
        32, 32, 32, 1, 32, 32, 32, 1, 32, 32, 32, 1, 32, 32, 32, 32, 32, 32, 32,
        32, 32, 32, 32, 1, 32, 32, 32, 1, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
        32, 32, 32, 32, 32, 15, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 1,
        32, 32, 32, 27, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
        32, 32, 32, 32, 32, 7, 32, 32, 32, 7, 32, 32, 32,
    ];

    private static readonly Encoding Ansi = CreateAnsiEncoding();

    public static string Write(
        string baseName,
        string wavDirectory,
        string ssfDirectory,
        string mlpDirectory)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(baseName);
        Directory.CreateDirectory(ssfDirectory);
        Directory.CreateDirectory(mlpDirectory);

        var left = ExistingPath(wavDirectory, baseName, ".L.wav");
        var right = ExistingPath(wavDirectory, baseName, ".R.wav");
        var center = ExistingPath(wavDirectory, baseName, ".C.wav");
        var surroundLeft = ExistingPath(wavDirectory, baseName, ".SL.wav");
        var surroundRight = ExistingPath(wavDirectory, baseName, ".SR.wav");
        var lowFrequency = ExistingPath(wavDirectory, baseName, ".LFE.wav");
        var monoSurround = ExistingPath(wavDirectory, baseName, ".S.wav");
        var mode = DetermineMode(
            left is not null,
            right is not null,
            center is not null,
            surroundLeft is not null,
            surroundRight is not null,
            lowFrequency is not null,
            monoSurround is not null);

        var fields = new[]
        {
            EncodeOptional("左声道 WAV", left),
            EncodeOptional("右声道 WAV", right),
            EncodeOptional("环绕左声道 WAV", surroundLeft ?? monoSurround),
            EncodeOptional("环绕右声道 WAV", surroundRight),
            EncodeOptional("中置声道 WAV", center),
            EncodeOptional("低频声道 WAV", lowFrequency),
            EncodeRequired("WAV 临时目录", Path.TrimEndingDirectorySeparator(wavDirectory) + '\\'),
            EncodeRequired("MLP 输出目录", Path.TrimEndingDirectorySeparator(mlpDirectory) + '\\'),
            EncodeRequired("MLP 输出文件", Path.Combine(mlpDirectory, baseName + ".mlp")),
        };

        var path = Path.Combine(ssfDirectory, baseName + ".ssf");
        using var writer = new BinaryWriter(new FileStream(path, FileMode.Create, FileAccess.Write));
        writer.Write(Header);
        foreach (var field in fields)
        {
            writer.Write(checked((byte)field.Length));
            writer.Write(field);
        }
        writer.Write(mode);
        writer.Write(new byte[] { 0, 0, 0 });
        return path;
    }

    public static int AnsiByteCount(string value) => Ansi.GetByteCount(value);

    private static string? ExistingPath(string directory, string baseName, string suffix)
    {
        var path = Path.Combine(directory, baseName + suffix);
        return File.Exists(path) ? path : null;
    }

    private static byte[] EncodeOptional(string fieldName, string? value) =>
        value is null ? [0] : EncodeRequired(fieldName, value);

    private static byte[] EncodeRequired(string fieldName, string value)
    {
        var bytes = Ansi.GetBytes(value);
        if (bytes.Length > byte.MaxValue)
        {
            throw new InvalidDataException(
                $"{fieldName} 的 ANSI 路径长度为 {bytes.Length} 字节，" +
                "超过 SurCode SSF 格式的 255 字节上限。请使用更短的目录。");
        }
        return bytes;
    }

    private static byte DetermineMode(
        bool left,
        bool right,
        bool center,
        bool surroundLeft,
        bool surroundRight,
        bool lowFrequency,
        bool monoSurround)
    {
        if (center && !(left || right || lowFrequency || surroundLeft || surroundRight)) return 0;
        if (left && right && !(lowFrequency || surroundLeft || surroundRight || center)) return 1;
        if (left && right && monoSurround && !(center || lowFrequency || surroundLeft || surroundRight)) return 2;
        if (left && right && surroundLeft && surroundRight && !(center || lowFrequency)) return 3;
        if (left && right && lowFrequency && !(center || surroundLeft || surroundRight)) return 4;
        if (left && right && lowFrequency && monoSurround && !center && !surroundLeft && !surroundRight) return 5;
        if (left && right && lowFrequency && surroundLeft && surroundRight && !center) return 6;
        if (center && left && right && !(lowFrequency || surroundLeft || surroundRight)) return 7;
        if (center && left && right && monoSurround && !(lowFrequency || surroundLeft || surroundRight)) return 8;
        if (left && right && center && surroundLeft && surroundRight && !lowFrequency) return 9;
        if (left && right && center && lowFrequency && !(surroundLeft || surroundRight)) return 10;
        if (left && right && center && lowFrequency && monoSurround && !(surroundLeft || surroundRight)) return 11;
        if (left && right && center && lowFrequency && surroundLeft && surroundRight) return 12;
        if (left && right && center && monoSurround && !(lowFrequency || surroundLeft || surroundRight)) return 13;
        throw new InvalidDataException("eac3to 生成的声道组合不受 SurCode SSF 支持。");
    }

    private static Encoding CreateAnsiEncoding()
    {
        if (!OperatingSystem.IsWindows())
        {
            return Encoding.Latin1;
        }
        Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
        return Encoding.GetEncoding(checked((int)GetAcp()));
    }

    [DllImport("kernel32.dll", EntryPoint = "GetACP")]
    private static extern uint GetAcp();
}
