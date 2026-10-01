using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed class FfmpegMlpProvider
{
    private readonly DvdaOptions _options;
    private readonly ProcessRunner _runner;
    private readonly AudioParameterProbe _probe;

    public FfmpegMlpProvider(DvdaOptions options, ProcessRunner runner)
    {
        _options = options;
        _runner = runner;
        _probe = new AudioParameterProbe(runner, options.Ffprobe);
    }

    public async Task<MlpAcquisitionResult> AcquireAsync(
        IReadOnlyList<BuildTrack> tracks,
        CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(_options.MlpDirectory);
        var diagnostics = new List<BuildDiagnostic>();
        var cacheIndexPath = MlpCacheIndex.PathFor(_options.MlpDirectory);
        var cacheIndex = MlpCacheIndex.Load(cacheIndexPath);
        var encoderIdentity = await ToolIdentity.DescribeAsync(
            _runner, _options.Ffmpeg, cancellationToken).ConfigureAwait(false);

        // 逐轨编码彼此独立：按 DVDA_MLP_JOBS 做有界并发，默认 1 路（与原行为一致）。
        var jobs = Math.Max(1, Math.Min(_options.MlpJobs, Math.Max(1, tracks.Count)));
        if (jobs > 1)
        {
            Console.WriteLine($"[MLP] 并行编码 {jobs} 路（DVDA_MLP_JOBS={_options.MlpJobs}）");
        }
        var outcomes = new TrackOutcome?[tracks.Count];
        await Parallel.ForEachAsync(
            Enumerable.Range(0, tracks.Count),
            new ParallelOptions
            {
                MaxDegreeOfParallelism = jobs,
                CancellationToken = cancellationToken,
            },
            async (index, token) =>
            {
                outcomes[index] = await EncodeOneAsync(
                    tracks[index], encoderIdentity, cacheIndex, token).ConfigureAwait(false);
            }).ConfigureAwait(false);

        var output = new List<BuildTrack>(tracks.Count);
        var hits = 0;
        var rebuilt = 0;
        foreach (var outcome in outcomes)
        {
            if (outcome is null)
            {
                throw new InvalidOperationException("MLP 获取结果不完整。");
            }
            output.Add(outcome.Track);
            if (outcome.Diagnostic is not null)
            {
                diagnostics.Add(outcome.Diagnostic);
            }
            if (outcome.Hit)
            {
                hits++;
            }
            if (outcome.Rebuilt)
            {
                rebuilt++;
            }
            if (outcome.IndexKey is not null && outcome.Entry is not null)
            {
                cacheIndex.Record(outcome.IndexKey, outcome.Entry);
            }
        }

        try
        {
            cacheIndex.Save(cacheIndexPath);
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException)
        {
            Console.WriteLine($"[警告] MLP 缓存索引写入失败: {exception.Message}");
        }
        return new MlpAcquisitionResult(output, hits, rebuilt, diagnostics);
    }

    private sealed record TrackOutcome(
        BuildTrack Track,
        BuildDiagnostic? Diagnostic,
        bool Hit,
        bool Rebuilt,
        string? IndexKey,
        MlpCacheEntry? Entry);

    private async Task<TrackOutcome> EncodeOneAsync(
        BuildTrack track,
        string encoderIdentity,
        MlpCacheIndex cacheIndex,
        CancellationToken cancellationToken)
    {
        var cache = new FileInfo(track.MlpPath);
        var source = new FileInfo(track.SourcePath);
        if (!source.Exists)
        {
            return new TrackOutcome(track, new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "SOURCE_MISSING",
                $"源文件不存在: {track.SourcePath}"), false, false, null, null);
        }

        // 命中需要凭据：源文件身份 + 编码器身份 + 编码参数 + 输出文件身份。
        var sourceIdentity = FileIdentityProbe.Compute(track.SourcePath);
        var indexed = sourceIdentity is null
            ? null
            : cacheIndex.Match(
                track.MlpPath,
                sourceIdentity,
                encoderIdentity,
                track.Bits,
                track.ResampleTo,
                MlpCacheValidator.RequiredMajorSyncInterval);
        if (cache.Exists && cache.Length > 0 && indexed is not null &&
            MlpCacheValidator.IsValid(cache.FullName))
        {
            return new TrackOutcome(
                track with { MlpSize = cache.Length, MlpSource = "ffmpeg" },
                null, true, false, null, null);
        }

        if (cache.Exists)
        {
            cache.Delete();
        }
        var result = await _runner.RunAsync(new ProcessRequest
        {
            FileName = _options.Ffmpeg,
            Arguments = BuildArguments(track),
            OnOutputLine = Console.WriteLine,
            OnErrorLine = Console.Error.WriteLine,
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded || !File.Exists(track.MlpPath))
        {
            return new TrackOutcome(track, new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "MLP_ENCODE_FAILED",
                $"MLP 编码失败（退出码 {result.ExitCode}）: {track.Title}"), false, false, null, null);
        }

        var parameters = await _probe.ProbeAsync(track.MlpPath, cancellationToken)
            .ConfigureAwait(false);
        if (parameters.SampleRate != track.SampleRate || parameters.Bits != track.Bits)
        {
            File.Delete(track.MlpPath);
            return new TrackOutcome(track, new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "MLP_PARAMETERS_MISMATCH",
                $"{track.Title} 期望 {track.SampleRate}Hz/{track.Bits}bit，" +
                $"实际 {parameters.SampleRate}Hz/{parameters.Bits}bit。"), false, false, null, null);
        }

        AlignAndVerify(track.MlpPath);
        var entry = sourceIdentity is not null &&
            FileIdentityProbe.Compute(track.MlpPath) is { } outputIdentity
                ? new MlpCacheEntry
                {
                    Source = sourceIdentity,
                    Output = outputIdentity,
                    Encoder = encoderIdentity,
                    Bits = track.Bits,
                    ResampleTo = track.ResampleTo,
                    MaxInterval = MlpCacheValidator.RequiredMajorSyncInterval,
                }
                : null;
        return new TrackOutcome(
            track with
            {
                MlpSize = new FileInfo(track.MlpPath).Length,
                Channels = parameters.Channels,
                MlpSource = "ffmpeg",
            },
            null, false, true, track.MlpPath, entry);
    }

    public IReadOnlyList<string> BuildArguments(BuildTrack track)
    {
        var arguments = new List<string>
        {
            "-hide_banner", "-loglevel", "error", "-y",
            "-i", track.SourcePath,
        };
        if (track.ResampleTo is not null)
        {
            arguments.AddRange(["-af", $"aresample={track.ResampleTo}:resampler=soxr"]);
        }
        arguments.AddRange(
        [
            "-sample_fmt", SampleFormat(track.Bits),
            "-max_interval", MlpCacheValidator.RequiredMajorSyncInterval.ToString(),
            "-c:a", "mlp", "-strict", "-2", track.MlpPath,
        ]);
        return arguments;
    }

    public static string SampleFormat(int bits) => bits == 16 ? "s16p" : "s32p";

    private static void AlignAndVerify(string path)
    {
        var original = File.ReadAllBytes(path);
        var aligned = MlpStreamAligner.Align(original);
        if (!aligned.Data.AsSpan().SequenceEqual(original))
        {
            File.WriteAllBytes(path, aligned.Data);
        }
        var inspection = MlpStreamAligner.Inspect(aligned.Data);
        if (inspection.MajorSyncErrors.Count > 0 ||
            inspection.AccessUnitParityErrors.Count > 0 ||
            inspection.SubstreamErrors.Count > 0 ||
            !inspection.HasEndOfStream)
        {
            throw new InvalidDataException(
                $"MLP 对齐后自检失败: {Path.GetFileName(path)} " +
                $"(ms={inspection.MajorSyncErrors.Count}, " +
                $"au={inspection.AccessUnitParityErrors.Count}, " +
                $"sub={inspection.SubstreamErrors.Count}, eos={inspection.HasEndOfStream})");
        }
    }
}
