using DvdaMaker.Configuration;
using DvdaMaker.Processes;
using System.Diagnostics;

namespace DvdaMaker.Building;

public sealed class BuildPipeline(DvdaOptions options, ProcessRunner? processRunner = null)
{
    private readonly ProcessRunner _runner = processRunner ?? new ProcessRunner();

    public async Task<BuildPipelineResult> RunAsync(
        bool dryRun,
        bool disableResume = false,
        CancellationToken cancellationToken = default)
    {
        if (!File.Exists(options.ManifestPath))
        {
            throw new FileNotFoundException("找不到 manifest.json，请先运行 prepare。",
                options.ManifestPath);
        }

        using var log = new BuildLogWriter(options, dryRun);
        var pipelineStarted = Stopwatch.GetTimestamp();

        var initial = BuildPlanService.ApplyDiagnosticAlbumLimit(
            new ManifestBuildReader().Read(options.ManifestPath, options.MlpDirectory),
            options.DiagnosticAlbumLimit);
        if (options.DiagnosticAlbumLimit is not null)
        {
            log.WriteLine($"[诊断] DVDA_ALBUM_LIMIT={options.DiagnosticAlbumLimit}: " +
                $"仅处理 {DiscPlanner.AggregateAlbums(initial).Count} 张专辑 / {initial.Count} 轨");
        }
        var acquisitionSpace = DiskSpacePlanner.Estimate(options, initial, discs: null);
        log.WriteLine($"[空间] MLP 获取阶段: {DiskSpacePlanner.Describe(acquisitionSpace)}");
        foreach (var diagnostic in DiskSpacePlanner.Evaluate(acquisitionSpace))
        {
            log.WriteLine($"[警告] {diagnostic.Code}: {diagnostic.Message}");
            Console.Error.WriteLine($"[警告] {diagnostic.Message}");
        }
        var acquisitionStarted = Stopwatch.GetTimestamp();
        MlpAcquisitionResult acquisition = options.MlpSource switch
        {
            "external" or "surcode" => await new ExternalMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
            "surcode-batch" => await new SurcodeMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
            _ => await new FfmpegMlpProvider(options, _runner)
                .AcquireAsync(initial, cancellationToken).ConfigureAwait(false),
        };
        log.WriteLine($"[耗时] MLP 获取: {Stopwatch.GetElapsedTime(acquisitionStarted)}");

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

        var publicationSpace = DiskSpacePlanner.Estimate(options, acquisition.Tracks, plan.Discs);
        log.WriteLine($"[空间] 出盘阶段: {DiskSpacePlanner.Describe(publicationSpace)}");
        var spaceDiagnostics = DiskSpacePlanner.Evaluate(publicationSpace);
        if (spaceDiagnostics.Count > 0)
        {
            plan = plan with
            {
                Diagnostics = plan.Diagnostics.Concat(spaceDiagnostics).ToArray(),
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
            log.WriteLine($"[汇总] 预演: {plan.Discs.Count} 盘 / {plan.Tracks.Count} 轨, " +
                $"MLP {plan.Tracks.Sum(track => track.MlpSize):N0} B, " +
                $"缓存复用 {acquisition.CacheHits} / 重新编码 {acquisition.CacheRebuilt}");
            log.WriteLine($"[耗时] build 预演总计: {Stopwatch.GetElapsedTime(pipelineStarted)}");
            return new BuildPipelineResult(plan, acquisition, dryRunIndexPath, []);
        }

        var pendingIndexPath = PendingIndexPath(options.MlpIndexPath);
        MlpIndexWriter.Write(pendingIndexPath, plan, options, dryRun: false);
        var discResults = new List<DiscBuildResult>(plan.Discs.Count);
        var executor = new DiscBuildExecutor(options, _runner, log);
        // 续跑要求暂存 ISO 留在固定目录：保留中间产物时 ISO 直接落成品目录，无法续跑。
        var canResume = !disableResume && options.ResumeEnabled && !options.KeepIntermediate;
        var resumeStore = canResume ? DiscResumeStore.Load(DiscResumeStore.DirectoryFor(options)) : null;
        var stagingDirectory = canResume
            ? DiscResumeStore.DirectoryFor(options)
            : Path.Combine(options.BuildDirectory, "publish-staging", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stagingDirectory);
        if (resumeStore is not null)
        {
            log.WriteLine($"[续跑] 已启用逐盘续跑，暂存目录: {stagingDirectory}，" +
                $"已有记录 {resumeStore.Count} 盘");
        }
        var indexPublished = false;
        var resumed = 0;
        try
        {
            foreach (var disc in plan.Discs)
            {
                var signature = resumeStore is null
                    ? null
                    : await DiscSignature.ComputeAsync(
                        options, disc, _runner, cancellationToken).ConfigureAwait(false);
                var reusable = signature is null
                    ? null
                    : resumeStore!.TryReuse(
                        disc.Number, signature, options.IsoName(disc.Number));
                DiscBuildResult result;
                if (reusable is not null)
                {
                    var stagedPath = resumeStore!.IsoPath(options.IsoName(disc.Number));
                    resumed++;
                    log.WriteLine($"[恢复] 第 {disc.Number} 盘签名一致，" +
                        $"复用暂存 ISO（{reusable.Iso!.Size:N0} B）: {stagedPath}");
                    result = new DiscBuildResult(
                        disc.Number, stagedPath, stagedPath, reusable.Iso.Size, []);
                }
                else
                {
                    resumeStore?.Discard(disc.Number, options.IsoName(disc.Number));
                    result = await executor.BuildAsync(
                        disc,
                        stagingDirectory,
                        cancellationToken,
                        stageForTransactionalPublication: true).ConfigureAwait(false);
                    if (result.Succeeded && resumeStore is not null && signature is not null)
                    {
                        try
                        {
                            resumeStore.Record(disc.Number, signature, options.IsoName(disc.Number));
                            resumeStore.Save();
                        }
                        catch (Exception exception) when (
                            exception is IOException or UnauthorizedAccessException)
                        {
                            log.WriteLine($"[警告] 续跑记录写入失败: {exception.Message}");
                        }
                    }
                }
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
                    var publicationStarted = Stopwatch.GetTimestamp();
                    var finalPaths = DiscPublisher.PublishSet(
                        discResults.Select(result => (
                            result.PublishedIsoPath,
                            options.IsoName(result.DiscNumber))).ToArray(),
                        options.FinalDirectory,
                        pendingIndexPath,
                        options.MlpIndexPath,
                        moveStagedIsos: true);
                    log.WriteLine($"[耗时] 正式 ISO 集合与索引发布: {Stopwatch.GetElapsedTime(publicationStarted)}");
                    for (var index = 0; index < discResults.Count; index++)
                    {
                        discResults[index] = discResults[index] with
                        {
                            PublishedIsoPath = finalPaths[index],
                        };
                    }
                    indexPublished = true;
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
            var published = discResults.Where(result => result.Succeeded).ToArray();
            log.WriteLine(
                $"[汇总] 成功 {published.Length}/{plan.Discs.Count} 盘 / {plan.Tracks.Count} 轨, " +
                $"MLP {plan.Tracks.Sum(track => track.MlpSize):N0} B, " +
                $"ISO {published.Sum(result => result.IsoSize):N0} B, " +
                $"缓存复用 {acquisition.CacheHits} / 重新编码 {acquisition.CacheRebuilt}, " +
                $"续跑复用 {resumed} 盘, " +
                $"索引已发布: {(indexPublished ? "是" : "否")}");
            log.WriteLine($"[耗时] build 总计: {Stopwatch.GetElapsedTime(pipelineStarted)}");
            if (!indexPublished)
            {
                DeletePendingIndex(pendingIndexPath);
            }
            if (indexPublished || !Directory.EnumerateFileSystemEntries(stagingDirectory).Any())
            {
                TryDeleteDirectory(stagingDirectory);
            }
            else
            {
                log.WriteLine(
                    $"[警告] 构建或正式发布失败，保留暂存 ISO 与续跑记录供排查/续跑: {stagingDirectory}");
            }
        }
    }

    internal static string DryRunIndexPath(string formalIndexPath) =>
        WithSuffix(formalIndexPath, "-dryrun");

    internal static string PendingIndexPath(string formalIndexPath) =>
        WithSuffix(formalIndexPath, ".pending");

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
