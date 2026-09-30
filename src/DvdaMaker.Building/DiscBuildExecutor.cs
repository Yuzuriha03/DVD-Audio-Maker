using DvdaMaker.Configuration;
using DvdaMaker.Processes;
using System.Diagnostics;

namespace DvdaMaker.Building;

public sealed class DiscBuildExecutor(
    DvdaOptions options,
    ProcessRunner runner,
    BuildLogWriter log)
{
    public async Task<DiscBuildResult> BuildAsync(
        DiscPlan disc,
        string? publishDirectory = null,
        CancellationToken cancellationToken = default,
        bool stageForTransactionalPublication = false)
    {
        var diagnostics = new List<BuildDiagnostic>();
        var tag = $"disc{disc.Number}";
        var output = Path.Combine(options.OutputRoot, tag);
        var temporary = Path.Combine(options.TemporaryRoot, tag);
        var iso = Path.Combine(options.IsoDirectory, tag + ".iso");

        ResetDirectory(output);
        ResetDirectory(temporary);
        Directory.CreateDirectory(options.IsoDirectory);
        if (File.Exists(iso))
        {
            File.Delete(iso);
        }

        MenuAssets? menuAssets = null;
        if (options.MenuEnabled)
        {
            var menuPlan = MenuPlanner.Create(disc, options);
            menuAssets = await new MenuAssetBuilder(options, runner, log)
                .BuildAsync(disc, menuPlan, cancellationToken).ConfigureAwait(false);
            diagnostics.AddRange(menuAssets.Diagnostics);
            if (menuAssets.HasErrors)
            {
                return new DiscBuildResult(disc.Number, iso, string.Empty, 0, diagnostics);
            }
        }

        var authorArguments = DvdaAuthorCommandBuilder.BuildArguments(
            disc, output, temporary, menuAssets, options.DiagnosticTitleMode);
        if (options.DiagnosticTitleMode != "album")
        {
            log.WriteLine($"[诊断] DVDA_TITLE_MODE={options.DiagnosticTitleMode}；" +
                "非 album 模式可能导致 ASVS/title 结构与商业盘不一致。");
        }
        log.WriteCommand(options.DvdaAuthor, authorArguments);
        var author = await runner.RunAsync(new ProcessRequest
        {
            FileName = options.DvdaAuthor,
            Arguments = authorArguments,
            OnOutputLine = line =>
            {
                Console.WriteLine(line);
                log.WriteLine(line);
            },
            OnErrorLine = line =>
            {
                Console.Error.WriteLine(line);
                log.WriteLine(line);
            },
        }, cancellationToken).ConfigureAwait(false);
        log.WriteLine($"[耗时] 第 {disc.Number} 盘 dvda-author: {author.Duration}");
        if (!author.Succeeded)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "DVDA_AUTHOR_FAILED",
                $"dvda-author 生成第 {disc.Number} 盘失败（退出码 {author.ExitCode}）。"));
            return new DiscBuildResult(disc.Number, iso, string.Empty, 0, diagnostics);
        }
        var audioTs = Path.Combine(output, "AUDIO_TS");
        if (!Directory.Exists(audioTs))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "AUDIO_TS_MISSING",
                $"dvda-author 未生成第 {disc.Number} 盘的 AUDIO_TS。"));
            return new DiscBuildResult(disc.Number, iso, string.Empty, 0, diagnostics);
        }

        if (menuAssets is not null)
        {
            var menuDiagnostics = await new MenuBuildVerifier(runner, log)
                .VerifyAsync(audioTs, temporary, menuAssets, cancellationToken)
                .ConfigureAwait(false);
            diagnostics.AddRange(menuDiagnostics);
            if (menuDiagnostics.Any(item =>
                    item.Severity == BuildDiagnosticSeverity.Error))
            {
                return new DiscBuildResult(disc.Number, iso, string.Empty, 0, diagnostics);
            }
        }

        var mkisofsArguments = DvdaAuthorCommandBuilder.BuildMkisofsArguments(
            options, disc, iso, output);
        log.WriteCommand(options.Mkisofs, mkisofsArguments);
        var mkisofs = await runner.RunAsync(new ProcessRequest
        {
            FileName = options.Mkisofs,
            Arguments = mkisofsArguments,
            OnOutputLine = line =>
            {
                Console.WriteLine(line);
                log.WriteLine(line);
            },
            OnErrorLine = line =>
            {
                Console.Error.WriteLine(line);
                log.WriteLine(line);
            },
        }, cancellationToken).ConfigureAwait(false);
        log.WriteLine($"[耗时] 第 {disc.Number} 盘 mkisofs: {mkisofs.Duration}");
        if (!mkisofs.Succeeded || !File.Exists(iso) || new FileInfo(iso).Length == 0)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "MKISOFS_FAILED",
                $"mkisofs 打包第 {disc.Number} 盘失败（退出码 {mkisofs.ExitCode}）。"));
            return new DiscBuildResult(disc.Number, iso, string.Empty, 0, diagnostics);
        }
        var isoSize = new FileInfo(iso).Length;
        if (isoSize > options.DiscBytes)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "ISO_CAPACITY_EXCEEDED",
                $"第 {disc.Number} 盘 ISO 超出配置容量 {isoSize - options.DiscBytes:N0} 字节。"));
            return new DiscBuildResult(disc.Number, iso, string.Empty, isoSize, diagnostics);
        }

        var publicationStarted = Stopwatch.GetTimestamp();
        var published = stageForTransactionalPublication && !options.KeepIntermediate
            ? DiscPublisher.StageIso(
                iso, publishDirectory ?? options.FinalDirectory, options.IsoName(disc.Number))
            : DiscPublisher.Publish(
                iso, publishDirectory ?? options.FinalDirectory, options.IsoName(disc.Number));
        log.WriteLine($"[耗时] 第 {disc.Number} 盘 ISO 暂存: {Stopwatch.GetElapsedTime(publicationStarted)}");
        if (published.Diagnostic is not null)
        {
            diagnostics.Add(published.Diagnostic);
        }

        if (!options.KeepTemporary)
        {
            TryDeleteDirectory(temporary, diagnostics, "TEMP_CLEANUP_FAILED");
        }
        if (!options.KeepIntermediate)
        {
            var publishedLength = new FileInfo(published.Path).Length;
            if (publishedLength == isoSize)
            {
                TryDeleteFile(iso, diagnostics, "ISO_CLEANUP_FAILED");
                TryDeleteDirectory(output, diagnostics, "OUTPUT_CLEANUP_FAILED");
            }
        }

        return new DiscBuildResult(disc.Number, iso, published.Path, isoSize, diagnostics);
    }

    private static void ResetDirectory(string path)
    {
        if (Directory.Exists(path))
        {
            Directory.Delete(path, recursive: true);
        }
        Directory.CreateDirectory(path);
    }

    private static void TryDeleteDirectory(
        string path,
        ICollection<BuildDiagnostic> diagnostics,
        string code)
    {
        try
        {
            if (Directory.Exists(path))
            {
                Directory.Delete(path, recursive: true);
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning, code, $"清理目录失败 {path}: {exception.Message}"));
        }
    }

    private static void TryDeleteFile(
        string path,
        ICollection<BuildDiagnostic> diagnostics,
        string code)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning, code, $"清理文件失败 {path}: {exception.Message}"));
        }
    }
}
