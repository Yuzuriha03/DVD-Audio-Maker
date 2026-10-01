using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed class ExternalMlpProvider
{
    private readonly DvdaOptions _options;
    private readonly AudioParameterProbe _probe;

    public ExternalMlpProvider(DvdaOptions options, ProcessRunner runner)
    {
        _options = options;
        _probe = new AudioParameterProbe(runner, options.Ffprobe);
    }

    public async Task<MlpAcquisitionResult> AcquireAsync(
        IReadOnlyList<BuildTrack> tracks,
        CancellationToken cancellationToken = default)
        => await AcquireAsync(tracks, null, cancellationToken).ConfigureAwait(false);

    internal async Task<MlpAcquisitionResult> AcquireAsync(
        IReadOnlyList<BuildTrack> tracks,
        IReadOnlyDictionary<string, FileIdentity>? verifiedOutputs,
        CancellationToken cancellationToken = default)
    {
        var diagnostics = new List<BuildDiagnostic>();
        var externalRoot = ManifestBuildReader.ToNativePath(_options.MlpExternalDirectory);
        var sourceRoot = ManifestBuildReader.ToNativePath(_options.SourceDirectory);
        if (string.IsNullOrWhiteSpace(externalRoot) || !Directory.Exists(externalRoot))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "EXTERNAL_MLP_DIRECTORY_INVALID",
                $"外部 MLP 目录无效: {_options.MlpExternalDirectory}"));
            return new MlpAcquisitionResult(tracks, 0, 0, diagnostics);
        }

        var lookup = BuildBasenameLookup(externalRoot);
        foreach (var ambiguous in lookup.AmbiguousNames)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "EXTERNAL_MLP_BASENAME_AMBIGUOUS",
                $"外部 MLP 文件名重复，无法安全回退匹配: {ambiguous}"));
        }
        var output = new List<BuildTrack>(tracks.Count);
        var channelCounts = new Dictionary<int, int>();
        foreach (var track in tracks)
        {
            var hit = Resolve(track.SourcePath, sourceRoot, externalRoot, lookup.Index);
            if (hit is null)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "EXTERNAL_MLP_MISSING",
                    $"找不到外部 MLP: {track.Title}"));
                output.Add(track);
                continue;
            }
            if (new FileInfo(hit).Length == 0)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "EXTERNAL_MLP_EMPTY",
                    $"外部 MLP 是空文件: {hit}"));
                output.Add(track);
                continue;
            }

            var alreadyVerified = verifiedOutputs is not null &&
                verifiedOutputs.TryGetValue(Path.GetFullPath(hit), out var identity) &&
                FileIdentityProbe.Matches(hit, identity);
            try
            {
                if (!alreadyVerified)
                {
                    var inspection = MlpStreamAligner.Inspect(await File.ReadAllBytesAsync(
                        hit, cancellationToken).ConfigureAwait(false));
                    if (!inspection.IsValid || !inspection.HasEndOfStream)
                    {
                        diagnostics.Add(new BuildDiagnostic(
                            BuildDiagnosticSeverity.Error,
                            "EXTERNAL_MLP_INVALID",
                            $"外部 MLP 结构校验失败: {hit}"));
                        output.Add(track);
                        continue;
                    }
                }
            }
            catch (InvalidDataException exception)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "EXTERNAL_MLP_INVALID",
                    $"外部 MLP 无法解析: {hit}（{exception.Message}）"));
                output.Add(track);
                continue;
            }

            var mlp = await _probe.ProbeAsync(hit, cancellationToken).ConfigureAwait(false);
            var source = await _probe.ProbeAsync(track.SourcePath, cancellationToken)
                .ConfigureAwait(false);
            if (mlp.SampleRate <= 0 || mlp.Bits <= 0)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "EXTERNAL_MLP_PROBE_FAILED",
                    $"无法探测外部 MLP 参数: {hit}"));
                output.Add(track);
                continue;
            }

            channelCounts[mlp.Channels] = channelCounts.GetValueOrDefault(mlp.Channels) + 1;
            var resampled = source.SampleRate > 0 && source.SampleRate != mlp.SampleRate;
            var rebitded = source.Bits > 0 && source.Bits != mlp.Bits;
            output.Add(track with
            {
                SampleRate = mlp.SampleRate,
                Bits = mlp.Bits,
                Channels = mlp.Channels,
                SourceSampleRate = source.SampleRate,
                SourceBits = source.Bits,
                ResampleTo = resampled ? mlp.SampleRate : null,
                MlpPath = hit,
                MlpSize = new FileInfo(hit).Length,
                MlpSource = "external",
                ExternalResampled = resampled,
                ExternalRebitded = rebitded,
                ParametersChanged = resampled || rebitded,
            });
        }

        if (channelCounts.Count > 1)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "EXTERNAL_MLP_CHANNELS_MIXED",
                "外部 MLP 声道数不一致: " + string.Join(", ",
                    channelCounts.OrderBy(pair => pair.Key)
                        .Select(pair => $"{pair.Key} 声道 × {pair.Value}"))));
        }
        return new MlpAcquisitionResult(output, 0, 0, diagnostics);
    }

    public static string? Resolve(
        string sourcePath,
        string sourceRoot,
        string externalRoot,
        IReadOnlyDictionary<string, string> basenameIndex)
    {
        var relative = sourcePath.StartsWith(sourceRoot.TrimEnd('/', '\\') + Path.DirectorySeparatorChar,
                StringComparison.OrdinalIgnoreCase)
            ? Path.GetRelativePath(sourceRoot, sourcePath)
            : Path.GetFileName(sourcePath);
        var mirror = Path.Combine(externalRoot, Path.ChangeExtension(relative, ".mlp"));
        if (File.Exists(mirror))
        {
            return mirror;
        }
        return basenameIndex.GetValueOrDefault(Path.GetFileNameWithoutExtension(sourcePath));
    }

    public static IReadOnlyDictionary<string, string> BuildBasenameIndex(string root)
        => BuildBasenameLookup(root).Index;

    public static (IReadOnlyDictionary<string, string> Index, IReadOnlyList<string> AmbiguousNames)
        BuildBasenameLookup(string root)
    {
        var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var ambiguous = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var path in Directory.EnumerateFiles(root, "*.mlp", SearchOption.AllDirectories))
        {
            var name = Path.GetFileNameWithoutExtension(path);
            if (!result.TryAdd(name, path))
            {
                ambiguous.Add(name);
                result.Remove(name);
            }
        }
        return (result, ambiguous.Order(StringComparer.OrdinalIgnoreCase).ToArray());
    }
}
