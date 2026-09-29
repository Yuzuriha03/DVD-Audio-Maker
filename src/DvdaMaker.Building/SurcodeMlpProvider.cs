using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Processes;

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
                "SurCode Batch MLP Encoder 分支只能在 Windows 上运行。"));
            return new MlpAcquisitionResult(tracks, 0, 0, diagnostics);
        }

        var executable = ResolveBatchEncoder(_options.MlpBatchEncoder);
        if (executable is null)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "SURCODE_BATCH_ENCODER_MISSING",
                $"找不到 Batch-MLP-Encoder-3: {_options.MlpBatchEncoder}"));
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
            var duplicateNames = pending
                .GroupBy(item => Path.GetFileNameWithoutExtension(item.Track.SourcePath),
                    StringComparer.OrdinalIgnoreCase)
                .Where(group => group.Count() > 1)
                .Select(group => group.Key)
                .ToArray();
            if (duplicateNames.Length > 0)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Error,
                    "SURCODE_INPUT_NAME_AMBIGUOUS",
                    "全部 FLAC 单批传入时存在重复文件名，Batch MLP Encoder 的平面输出目录无法区分: " +
                    string.Join(", ", duplicateNames)));
            }
            else
            {
                var token = Guid.NewGuid().ToString("N");
                var tempDirectory = Path.Combine(tempRoot, token);
                var stageDirectory = Path.Combine(stageRoot, token);
                Directory.CreateDirectory(tempDirectory);
                Directory.CreateDirectory(stageDirectory);
                try
                {
                    Console.WriteLine($"[SurCode Batch] 单次传入 {pending.Count} 个 FLAC");
                    Console.WriteLine($"[SurCode Batch] 临时目录: {tempDirectory}");
                    Console.WriteLine($"[SurCode Batch] MLP 输出目录: {stageDirectory}");
                    var result = await _runner.RunAsync(new ProcessRequest
                    {
                        FileName = executable,
                        WorkingDirectory = Path.GetDirectoryName(executable),
                        Arguments = BuildArguments(
                            tempDirectory,
                            stageDirectory,
                            pending.Select(item => item.Track.SourcePath).ToArray()),
                        OnOutputLine = Console.WriteLine,
                        OnErrorLine = Console.Error.WriteLine,
                    }, cancellationToken).ConfigureAwait(false);
                    if (!result.Succeeded)
                    {
                        diagnostics.Add(new BuildDiagnostic(
                            BuildDiagnosticSeverity.Error,
                            "SURCODE_BATCH_FAILED",
                            $"Batch MLP Encoder 退出码 {result.ExitCode}（单批 {pending.Count} 首）"));
                    }
                    else
                    {
                        foreach (var item in pending)
                        {
                            var staged = Path.Combine(
                                stageDirectory,
                                Path.GetFileNameWithoutExtension(item.Track.SourcePath) + ".mlp");
                            if (!File.Exists(staged) || new FileInfo(staged).Length == 0)
                            {
                                diagnostics.Add(new BuildDiagnostic(
                                    BuildDiagnosticSeverity.Error,
                                    "SURCODE_OUTPUT_MISSING",
                                    $"Batch MLP Encoder 未生成 MLP: {item.Track.Title}"));
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
        }

        var external = await new ExternalMlpProvider(_options, _runner)
            .AcquireAsync(tracks, cancellationToken).ConfigureAwait(false);
        var normalized = external.Tracks.Select(track =>
            track.MlpSource == "external" ? track with { MlpSource = "surcode-batch" } : track).ToArray();
        return new MlpAcquisitionResult(
            normalized,
            hits,
            rebuilt,
            diagnostics.Concat(external.Diagnostics).ToArray());
    }

    public IReadOnlyList<string> BuildArguments(
        string tempDirectory,
        string outputDirectory,
        IReadOnlyList<string> sourceFiles)
    {
        var arguments = new List<string>
        {
            "--batch",
            "--temp", NormalizeBatchPath(tempDirectory),
            "--output", NormalizeBatchPath(outputDirectory),
            "--sample-rate", _options.MlpSurcodeSampleRate.ToString(),
            "--bits", _options.MlpSurcodeBits.ToString(),
        };
        arguments.AddRange(["--surcode", NormalizeBatchPath(_options.MlpSurcodeExecutable)]);
        arguments.AddRange(["--eac3to", NormalizeBatchPath(_options.MlpEac3toExecutable)]);
        arguments.Add("--");
        arguments.AddRange(sourceFiles.Select(NormalizeBatchPath));
        return arguments;
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

    public static string? ResolveBatchEncoder(string configuredPath)
    {
        if (File.Exists(configuredPath)) return Path.GetFullPath(configuredPath);
        if (!Directory.Exists(configuredPath)) return null;
        var candidates = new[]
        {
            Path.Combine(configuredPath, "bin", "Release", "net10.0-windows", "win-x86",
                "Batch-MLP-Encoder-3.exe"),
            Path.Combine(configuredPath, "bin", "Release", "Batch-MLP-Encoder-3.exe"),
            Path.Combine(configuredPath, "Batch-MLP-Encoder-3.exe"),
        };
        return candidates.FirstOrDefault(File.Exists);
    }

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

    private sealed record PendingTrack(BuildTrack Track, string Destination);
}
