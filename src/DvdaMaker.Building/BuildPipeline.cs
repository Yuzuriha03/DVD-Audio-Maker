using DvdaMaker.Configuration;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed class BuildPipeline(DvdaOptions options, ProcessRunner? processRunner = null)
{
    private readonly ProcessRunner _runner = processRunner ?? new ProcessRunner();

    public async Task<BuildPipelineResult> RunAsync(
        bool dryRun,
        CancellationToken cancellationToken = default)
    {
        if (!File.Exists(options.ManifestPath))
        {
            throw new FileNotFoundException("找不到 manifest.json，请先运行 prepare。",
                options.ManifestPath);
        }

        using var log = new BuildLogWriter(options, dryRun);

        var initial = BuildPlanService.ApplyDiagnosticAlbumLimit(
            new ManifestBuildReader().Read(options.ManifestPath, options.MlpDirectory),
            options.DiagnosticAlbumLimit);
        if (options.DiagnosticAlbumLimit is not null)
        {
            log.WriteLine($"[诊断] DVDA_ALBUM_LIMIT={options.DiagnosticAlbumLimit}: " +
                $"仅处理 {DiscPlanner.AggregateAlbums(initial).Count} 张专辑 / {initial.Count} 轨");
        }
        MlpAcquisitionResult acquisition = options.MlpSource switch
        {
            "external" or "surcode" => await new ExternalMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
            "surcode-batch" => await new SurcodeMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
            _ => await new FfmpegMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
        };

        var plan = new DiscPlanner().Plan(
            acquisition.Tracks,
            options.DiscBytes,
            options.MaxDiscs,
            options.GroupTrackLimit);
        if (acquisition.Diagnostics.Count > 0)
        {
            plan = plan with
            {
                Diagnostics = acquisition.Diagnostics.Concat(plan.Diagnostics).ToArray(),
            };
        }

        if (plan.HasErrors)
        {
            throw new InvalidOperationException(
                "MLP 获取或构建规划包含错误；未写入索引，也未执行出盘。" );
        }

        if (dryRun)
        {
            var dryRunIndexPath = DryRunIndexPath(options.MlpIndexPath);
            MlpIndexWriter.Write(dryRunIndexPath, plan, options, dryRun: true);
            return new BuildPipelineResult(plan, acquisition, dryRunIndexPath, []);
        }

        var pendingIndexPath = PendingIndexPath(options.MlpIndexPath);
        MlpIndexWriter.Write(pendingIndexPath, plan, options, dryRun: false);
        var discResults = new List<DiscBuildResult>(plan.Discs.Count);
        var executor = new DiscBuildExecutor(options, _runner, log);
        var stagingDirectory = Path.Combine(
            options.BuildDirectory, "publish-staging", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stagingDirectory);
        var indexPublished = false;
        try
        {
            foreach (var disc in plan.Discs)
            {
                var result = await executor.BuildAsync(
                    disc,
                    stagingDirectory,
                    copyToWindows: false,
                    cancellationToken).ConfigureAwait(false);
                discResults.Add(result);
                if (!result.Succeeded)
                {
                    break;
                }
            }

            if (discResults.Count == plan.Discs.Count &&
                discResults.All(result => result.Succeeded))
            {
                try
                {
                    var finalPaths = DiscPublisher.PublishSet(
                        discResults.Select(result => (
                            result.PublishedIsoPath,
                            options.IsoName(result.DiscNumber))).ToArray(),
                        options.FinalDirectory,
                        pendingIndexPath,
                        options.MlpIndexPath);
                    for (var index = 0; index < discResults.Count; index++)
                    {
                        discResults[index] = discResults[index] with
                        {
                            PublishedIsoPath = finalPaths[index],
                        };
                    }
                    indexPublished = true;
                    await CopyPublishedIsosToWindowsAsync(
                        discResults, log, cancellationToken).ConfigureAwait(false);
                }
                catch (Exception exception) when (
                    exception is IOException or UnauthorizedAccessException or InvalidDataException)
                {
                    var last = discResults[^1];
                    discResults[^1] = last with
                    {
                        Diagnostics = last.Diagnostics.Concat([
                            new BuildDiagnostic(
                                BuildDiagnosticSeverity.Error,
                                "FINAL_PUBLICATION_FAILED",
                                $"正式 ISO 集合与索引发布失败，旧成品已回滚: {exception.Message}"),
                        ]).ToArray(),
                    };
                }
            }
            return new BuildPipelineResult(
                plan,
                acquisition,
                indexPublished ? options.MlpIndexPath : string.Empty,
                discResults);
        }
        finally
        {
            if (!indexPublished)
            {
                DeletePendingIndex(pendingIndexPath);
            }
            TryDeleteDirectory(stagingDirectory);
        }
    }

    internal static string DryRunIndexPath(string formalIndexPath) =>
        WithSuffix(formalIndexPath, "-dryrun");

    internal static string PendingIndexPath(string formalIndexPath) =>
        WithSuffix(formalIndexPath, ".pending");

    private async Task CopyPublishedIsosToWindowsAsync(
        IList<DiscBuildResult> discResults,
        BuildLogWriter log,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(options.WindowsDestination)) return;
        var copier = new WindowsIsoCopier(_runner);
        for (var index = 0; index < discResults.Count; index++)
        {
            var copy = await copier.CopyAsync(
                discResults[index].PublishedIsoPath,
                options.WindowsDestination,
                cancellationToken: cancellationToken).ConfigureAwait(false);
            if (copy.Succeeded)
            {
                log.WriteLine($"[Windows copy] {copy.Message}");
                continue;
            }
            var current = discResults[index];
            discResults[index] = current with
            {
                Diagnostics = current.Diagnostics.Concat([
                    new BuildDiagnostic(
                        BuildDiagnosticSeverity.Warning,
                        "WINDOWS_ISO_COPY_FAILED",
                        $"Windows 侧复制失败；ISO 仍保留在 {current.PublishedIsoPath}。" +
                        $"{Environment.NewLine}{copy.Message}"),
                ]).ToArray(),
            };
        }
    }

    private static string WithSuffix(string path, string suffix)
    {
        var directory = Path.GetDirectoryName(path) ?? string.Empty;
        var extension = Path.GetExtension(path);
        var name = Path.GetFileNameWithoutExtension(path) + suffix + extension;
        return Path.Combine(directory, name);
    }

    private static void DeletePendingIndex(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
        }
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
}
