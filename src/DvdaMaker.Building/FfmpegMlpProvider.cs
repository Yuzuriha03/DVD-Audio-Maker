using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
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
        var output = new List<BuildTrack>(tracks.Count);
        var diagnostics = new List<BuildDiagnostic>();
        var hits = 0;
        var rebuilt = 0;

        foreach (var track in tracks)
        {
            var cache = new FileInfo(track.MlpPath);
            var source = new FileInfo(track.SourcePath);
            if (!source.Exists)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "SOURCE_MISSING",
                    $"源文件不存在: {track.SourcePath}"));
                output.Add(track);
                continue;
            }

            if (cache.Exists && cache.Length > 0 &&
                cache.LastWriteTimeUtc >= source.LastWriteTimeUtc &&
                MlpCacheValidator.IsValid(cache.FullName))
            {
                hits++;
                output.Add(track with { MlpSize = cache.Length, MlpSource = "ffmpeg" });
                continue;
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
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "MLP_ENCODE_FAILED",
                    $"MLP 编码失败（退出码 {result.ExitCode}）: {track.Title}"));
                output.Add(track);
                continue;
            }

            var parameters = await _probe.ProbeAsync(track.MlpPath, cancellationToken)
                .ConfigureAwait(false);
            if (parameters.SampleRate != track.SampleRate || parameters.Bits != track.Bits)
            {
                File.Delete(track.MlpPath);
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "MLP_PARAMETERS_MISMATCH",
                    $"{track.Title} 期望 {track.SampleRate}Hz/{track.Bits}bit，" +
                    $"实际 {parameters.SampleRate}Hz/{parameters.Bits}bit。"));
                output.Add(track);
                continue;
            }

            AlignAndVerify(track.MlpPath);
            rebuilt++;
            output.Add(track with
            {
                MlpSize = new FileInfo(track.MlpPath).Length,
                Channels = parameters.Channels,
                MlpSource = "ffmpeg",
            });
        }
        return new MlpAcquisitionResult(output, hits, rebuilt, diagnostics);
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
