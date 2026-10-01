using DvdaMaker.Processes;

namespace DvdaMaker.SurcodeTool;

/// <summary>Compatibility name for the MLP core batch pipeline; never automates SurCode.</summary>
public sealed class SurcodeBatchEncoder(ProcessRunner runner)
{
    public async Task RunAsync(SurcodeEncodingJob job, CancellationToken cancellationToken)
    {
        var ffmpeg = ExecutablePath.Resolve(job.FfmpegExecutable)
            ?? throw new FileNotFoundException("找不到 FFmpeg，请设置 DVDA_FFMPEG 或将 ffmpeg.exe 加入 PATH。", job.FfmpegExecutable);
        Validate(job);
        Directory.CreateDirectory(job.TemporaryDirectory);
        Directory.CreateDirectory(job.OutputDirectory);
        _ = MlpEncoder.ExtractLibrary();
        await Parallel.ForEachAsync(job.Tracks, new ParallelOptions
        {
            MaxDegreeOfParallelism = Math.Clamp(job.Jobs, 1, 16),
            CancellationToken = cancellationToken,
        }, async (track, token) =>
        {
            var folder = Path.Combine(job.TemporaryDirectory, Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(folder);
            try
            {
                Console.WriteLine($"[MLP] {track.DisplayName}");
                var input = Path.Combine(folder, "decoded.wav");
                await FfmpegPcmConverter.ConvertAsync(runner, ffmpeg, track, input,
                    job.SampleRate, job.Bits, token).ConfigureAwait(false);
                var prepared = Path.Combine(folder, "input.wav");
                SurcodePcmWav.Normalize(input, prepared, job.SampleRate, job.Bits, token);
                File.Delete(input);
                await MlpEncoder.EncodeAsync( prepared,
                    Path.Combine(job.OutputDirectory, track.WorkName + ".mlp"), job.MetadataContext,
                    TimeSpan.FromSeconds(Math.Max(300, track.DurationSeconds * 3 + 120)), token).ConfigureAwait(false);
            }
            finally
            {
                // This GUID directory is owned exclusively by this track invocation.
                Directory.Delete(folder, recursive: true);
            }
        }).ConfigureAwait(false);
    }

    private static void Validate(SurcodeEncodingJob job)
    {
        if (!string.IsNullOrEmpty(job.MetadataContext) && !File.Exists(job.MetadataContext))
            throw new FileNotFoundException("找不到显式 MLP 元数据上下文。", job.MetadataContext);
        if (job.SampleRate is not (44_100 or 48_000 or 88_200 or 96_000 or 176_400 or 192_000))
            throw new InvalidDataException($"不支持的目标采样率: {job.SampleRate}");
        if (job.Bits is not (16 or 20 or 24)) throw new InvalidDataException($"不支持的目标位深: {job.Bits}");
        if (job.Tracks.Count == 0) throw new InvalidDataException("编码任务不包含任何音轨。");
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var track in job.Tracks)
        {
            if (!File.Exists(track.SourcePath)) throw new FileNotFoundException("找不到待编码音源。", track.SourcePath);
            if (track.WorkName.Length == 0 || track.WorkName is "." or ".." || !names.Add(track.WorkName) ||
                track.WorkName.Any(c => !(char.IsAsciiLetterOrDigit(c) || c is '_' or '-')))
                throw new InvalidDataException($"工作文件名必须是唯一的安全 ASCII 名称: {track.WorkName}");
            if (!double.IsFinite(track.DurationSeconds) || track.DurationSeconds < 0)
                throw new InvalidDataException("无效的音轨时长。");
            if (File.Exists(Path.Combine(job.OutputDirectory, track.WorkName + ".mlp")))
                throw new IOException("拒绝覆盖已有的暂存 MLP 文件。");
        }
    }
}
