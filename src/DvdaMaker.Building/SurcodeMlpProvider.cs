using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

namespace DvdaMaker.Building;

public sealed class SurcodeMlpProvider
{
    private readonly DvdaOptions _options;
    private readonly ProcessRunner _runner;

    public SurcodeMlpProvider(DvdaOptions options, ProcessRunner runner)
    {
        _options = options;
        _runner = runner;
    }

    public async Task<MlpAcquisitionResult> AcquireAsync(
        IReadOnlyList<BuildTrack> tracks,
        CancellationToken cancellationToken = default)
    {
        var diagnostics = new List<BuildDiagnostic>();
        if (!OperatingSystem.IsWindows())
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "SURCODE_WINDOWS_REQUIRED",
                "内置 SurCode 编码分支只能在 Windows 上运行。"));
            return new MlpAcquisitionResult(tracks, 0, 0, diagnostics);
        }

        if (string.IsNullOrWhiteSpace(_options.MlpSurcodeExecutable) ||
            !File.Exists(_options.MlpSurcodeExecutable))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "SURCODE_EXECUTABLE_MISSING",
                $"surcode-batch 模式需要通过 DVDA_MLP_SURCODE_EXE 指定有效的 surcodemlp.exe: {_options.MlpSurcodeExecutable}"));
        }
        if (string.IsNullOrWhiteSpace(_options.MlpEac3toExecutable) ||
            !File.Exists(_options.MlpEac3toExecutable))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "EAC3TO_EXECUTABLE_MISSING",
                $"surcode-batch 模式需要通过 DVDA_MLP_EAC3TO_EXE 指定有效的 eac3to.exe: {_options.MlpEac3toExecutable}"));
        }
        if (diagnostics.Count > 0)
        {
            return new MlpAcquisitionResult(tracks, 0, 0, diagnostics);
        }

        var outputRoot = ManifestBuildReader.ToNativePath(_options.MlpExternalDirectory);
        var sourceRoot = ManifestBuildReader.ToNativePath(_options.SourceDirectory);
        if (string.IsNullOrWhiteSpace(outputRoot))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "SURCODE_OUTPUT_DIRECTORY_MISSING",
                "surcode-batch 模式需要设置 DVDA_MLP_EXTERNAL_DIR 作为 MLP 缓存目录。"));
            return new MlpAcquisitionResult(tracks, 0, 0, diagnostics);
        }
        Directory.CreateDirectory(outputRoot);

        var pending = new List<PendingTrack>();
        var hits = 0;
        foreach (var track in tracks)
        {
            if (!File.Exists(track.SourcePath))
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "SOURCE_MISSING",
                    $"源文件不存在: {track.SourcePath}"));
                continue;
            }

            var destination = DestinationPath(track.SourcePath, sourceRoot, outputRoot);
            var mlp = new FileInfo(destination);
            var source = new FileInfo(track.SourcePath);
            if (mlp.Exists && mlp.Length > 0 &&
                mlp.LastWriteTimeUtc >= source.LastWriteTimeUtc &&
                MlpCacheValidator.IsValid(destination))
            {
                hits++;
                continue;
            }
            pending.Add(new PendingTrack(track, destination));
        }

        var rebuilt = 0;
        var verifiedOutputs = new Dictionary<string, FileIdentity>(StringComparer.OrdinalIgnoreCase);
        var defaultWorkRoot = Path.Combine(_options.BuildDirectory, "surcode-batch");
        var tempRoot = string.IsNullOrWhiteSpace(_options.MlpBatchTempDirectory)
            ? Path.Combine(defaultWorkRoot, "temp")
            : ManifestBuildReader.ToNativePath(_options.MlpBatchTempDirectory);
        var stageRoot = string.IsNullOrWhiteSpace(_options.MlpBatchOutputDirectory)
            ? Path.Combine(defaultWorkRoot, "output")
            : ManifestBuildReader.ToNativePath(_options.MlpBatchOutputDirectory);
        tempRoot = NormalizeBatchPath(tempRoot);
        stageRoot = NormalizeBatchPath(stageRoot);
        Directory.CreateDirectory(tempRoot);
        Directory.CreateDirectory(stageRoot);
        if (pending.Count > 0)
        {
            var token = Guid.NewGuid().ToString("N");
            var tempDirectory = Path.Combine(tempRoot, token);
            var stageDirectory = Path.Combine(stageRoot, token);
            Directory.CreateDirectory(tempDirectory);
            Directory.CreateDirectory(stageDirectory);
            try
            {
                var job = BuildJob(tempDirectory, stageDirectory, pending);
                Console.WriteLine($"[SurCode] 进程内提交 {pending.Count} 个 FLAC 给编码类库");
                Console.WriteLine($"[SurCode] 临时目录: {tempDirectory}");
                Console.WriteLine($"[SurCode] MLP 输出目录: {stageDirectory}");
                var succeeded = false;
                try
                {
                    await new SurcodeBatchEncoder(_runner)
                        .RunAsync(job, cancellationToken).ConfigureAwait(false);
                    succeeded = true;
                }
                catch (Exception exception) when (exception is not OperationCanceledException)
                {
                    diagnostics.Add(new BuildDiagnostic(
                        BuildDiagnosticSeverity.Error,
                        "SURCODE_TOOL_FAILED",
                        $"内置 SurCode 编码类库失败（单批 {pending.Count} 首）：{exception.Message}"));
                }
                if (succeeded)
                {
                    for (var index = 0; index < pending.Count; index++)
                    {
                        var item = pending[index];
                        var staged = Path.Combine(stageDirectory, WorkName(index) + ".mlp");
                        if (!File.Exists(staged) || new FileInfo(staged).Length == 0)
                        {
                            diagnostics.Add(new BuildDiagnostic(
                                BuildDiagnosticSeverity.Error,
                                "SURCODE_OUTPUT_MISSING",
                                $"内置 SurCode 编码工具未生成 MLP: {item.Track.Title}"));
                            continue;
                        }
                        try
                        {
                            var inspection = MlpStreamAligner.Inspect(
                                await File.ReadAllBytesAsync(staged, cancellationToken).ConfigureAwait(false));
                            if (!inspection.IsValid || !inspection.HasEndOfStream)
                            {
                                diagnostics.Add(new BuildDiagnostic(
                                    BuildDiagnosticSeverity.Error,
                                    "SURCODE_OUTPUT_INVALID",
                                    $"SurCode MLP 结构校验失败: {item.Track.Title}"));
                                continue;
                            }
                        }
                        catch (InvalidDataException exception)
                        {
                            diagnostics.Add(new BuildDiagnostic(
                                BuildDiagnosticSeverity.Error,
                                "SURCODE_OUTPUT_INVALID",
                                $"SurCode MLP 无法解析: {item.Track.Title}（{exception.Message}）"));
                            continue;
                        }

                        Directory.CreateDirectory(Path.GetDirectoryName(item.Destination)!);
                        File.Move(staged, item.Destination, overwrite: true);
                        var identity = FileIdentityProbe.Compute(item.Destination);
                        if (identity is not null)
                        {
                            verifiedOutputs[Path.GetFullPath(item.Destination)] = identity;
                        }
                        rebuilt++;
                    }
                }
            }
            finally
            {
                TryDeleteDirectory(tempDirectory);
                TryDeleteDirectory(stageDirectory);
            }
        }

        var external = await new ExternalMlpProvider(_options, _runner)
            .AcquireAsync(tracks, verifiedOutputs, cancellationToken).ConfigureAwait(false);
        var normalized = external.Tracks.Select(track =>
            track.MlpSource == "external" ? track with { MlpSource = "surcode-batch" } : track).ToArray();
        return new MlpAcquisitionResult(
            normalized,
            hits,
            rebuilt,
            diagnostics.Concat(external.Diagnostics).ToArray());
    }

    public SurcodeEncodingJob BuildJob(
        string tempDirectory,
        string outputDirectory,
        IReadOnlyList<PendingTrack> pending)
    {
        return new SurcodeEncodingJob
        {
            SurcodeExecutable = NormalizeBatchPath(_options.MlpSurcodeExecutable),
            Eac3toExecutable = NormalizeBatchPath(_options.MlpEac3toExecutable),
            TemporaryDirectory = NormalizeBatchPath(tempDirectory),
            OutputDirectory = NormalizeBatchPath(outputDirectory),
            SampleRate = _options.MlpSurcodeSampleRate,
            Bits = _options.MlpSurcodeBits,
            Tracks = pending.Select((item, index) => new SurcodeEncodingTrack
            {
                SourcePath = NormalizeBatchPath(item.Track.SourcePath),
                WorkName = WorkName(index),
                DisplayName = item.Track.Title,
                DurationSeconds = item.Track.Duration,
                SourceSampleRate = item.Track.SourceSampleRate ?? item.Track.SampleRate,
                SourceBits = item.Track.SourceBits ?? item.Track.Bits,
            }).ToArray(),
        };
    }

    private static string NormalizeBatchPath(string path)
    {
        if (!OperatingSystem.IsWindows()) return path;
        return Path.GetFullPath(path.Replace('/', '\\'));
    }

    public static string DestinationPath(string sourcePath, string sourceRoot, string outputRoot)
    {
        var prefix = sourceRoot.TrimEnd('/', '\\') + Path.DirectorySeparatorChar;
        var relative = sourcePath.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
            ? Path.GetRelativePath(sourceRoot, sourcePath)
            : Path.GetFileName(sourcePath);
        return Path.Combine(outputRoot, Path.ChangeExtension(relative, ".mlp"));
    }

    private static string WorkName(int index) => $"__surcode_{index + 1:D4}";

    private static void TryDeleteDirectory(string path)
    {
        try
        {
            if (Directory.Exists(path)) Directory.Delete(path, recursive: true);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
        }
    }

    public sealed record PendingTrack(BuildTrack Track, string Destination);
}
