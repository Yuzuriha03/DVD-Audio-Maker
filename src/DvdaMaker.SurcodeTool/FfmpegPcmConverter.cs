using System.Globalization;
using DvdaMaker.Processes;

namespace DvdaMaker.SurcodeTool;

/// <summary>Prepare integer PCM only. The DLL remains the sole MLP encoder.</summary>
public static class FfmpegPcmConverter
{
    // Bump when conversion semantics change, even if the FFmpeg binary does not.
    public const string Policy = "ffmpeg-pcm-v1-swr-no-dither-20bit-nearest";

    public static IReadOnlyList<string> Arguments(string source, string output, int rate, int bits)
    {
        if (rate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000) || bits is not (16 or 20 or 24))
            throw new ArgumentException("PCM 目标格式必须符合 DVD-Audio 采样率及 16/20/24 位要求。");
        var format = bits switch { 16 => "s16", 20 => "dblp", _ => "s32" };
        var filter = $"aresample={rate}:resampler=swr:osf={format}:dither_method=none";
        // Double precision preserves all 24-bit integers exactly. Rounding is idempotent
        // for existing 20-bit PCM; c=same preserves the layout and channel order.
        if (bits == 20)
            filter += ",aeval=exprs='clip(floor(val(ch)*524288+0.5),-524288,524287)/524288':c=same,aformat=sample_fmts=s32";
        return ["-nostdin", "-hide_banner", "-loglevel", "level+warning", "-xerror", "-nostats", "-n",
            "-i", source, "-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "-1",
            "-af", filter, "-c:a", bits == 16 ? "pcm_s16le" : "pcm_s24le",
            "-rf64", "never", "-progress", "pipe:1", "-f", "wav", output];
    }

    public static async Task ConvertAsync(ProcessRunner runner, string executable, SurcodeEncodingTrack track,
        string output, int rate, int bits, CancellationToken token)
    {
        var previous = -1;
        Console.WriteLine($"[PCM] 正在准备音源：{track.DisplayName}");
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = executable,
            Arguments = Arguments(track.SourcePath, output, rate, bits),
            Timeout = TimeSpan.FromSeconds(Math.Max(300, track.DurationSeconds * 2 + 120)),
            CaptureOutput = false,
            OnOutputLine = line =>
            {
                if (track.DurationSeconds > 0 && line.StartsWith("out_time_us=", StringComparison.Ordinal) &&
                    long.TryParse(line.AsSpan(12), NumberStyles.Integer, CultureInfo.InvariantCulture, out var microseconds))
                {
                    var percent = (int)Math.Clamp(microseconds / (track.DurationSeconds * 10000), 0, 99);
                    if (percent != previous)
                    {
                        previous = percent;
                        Console.WriteLine($"[PCM] {track.DisplayName}：{percent}%");
                    }
                }
            },
            OnErrorLine = line => Console.Error.WriteLine($"[FFmpeg PCM] {line}"),
        }, token).ConfigureAwait(false);
        if (!result.Succeeded)
            throw new InvalidOperationException($"音源转换失败：{track.DisplayName}（FFmpeg 退出码 {result.ExitCode}）。{result.StandardError.Trim()}");
        Console.WriteLine($"[PCM] 音源准备完成：{track.DisplayName}");
    }
}
