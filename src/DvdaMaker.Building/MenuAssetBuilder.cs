using DvdaMaker.Configuration;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed class MenuAssetBuilder(
    DvdaOptions options,
    ProcessRunner runner,
    BuildLogWriter? log = null)
{
    private static readonly string[] RequiredMenuPrograms =
    [
        "dvdauthor", "spumux", "mpeg2enc", "mplex",
        "mp2enc",
    ];

    public async Task<MenuAssets> BuildAsync(
        DiscPlan disc,
        MenuPlan plan,
        CancellationToken cancellationToken = default)
    {
        var diagnostics = new List<BuildDiagnostic>();
        var directory = Path.Combine(options.MenuDirectory, $"disc{disc.Number}");
        var blankScreen = Path.Combine(directory, "blankscreen.png");

        var dataMenuDirectory = Path.Combine(options.AuthorSource, "menu");
        if (!Directory.Exists(dataMenuDirectory))
        {
            diagnostics.Add(Error(
                "MENU_DATA_MISSING",
                $"找不到 dvda-author 菜单素材目录: {dataMenuDirectory}"));
        }

        foreach (var program in RequiredMenuPrograms)
        {
            if (FindExecutable(program, options.MenuBinaryDirectory) is null)
            {
                diagnostics.Add(Error(
                    "MENU_TOOL_MISSING",
                    $"找不到菜单辅助程序 {program}；查找目录: {options.MenuBinaryDirectory}"));
            }
        }

        var imageMagick = FindExecutable("magick", options.MenuBinaryDirectory)
            ?? FindExecutable("convert", options.MenuBinaryDirectory);
        if (imageMagick is null)
        {
            diagnostics.Add(Error(
                "IMAGEMAGICK_MISSING",
                "内置图像组件缺失，请完整解压发布包并保留 image-native 目录。"));
        }
        var imageIdentify = FindImageMagickIdentify(options.MenuBinaryDirectory);
        if (imageIdentify is null)
        {
            diagnostics.Add(Error(
                "IMAGEMAGICK_IDENTIFY_MISSING",
                "内置图像组件缺失，无法验证菜单图片尺寸。请完整解压发布包。"));
        }

        if (diagnostics.Any(item => item.Severity == BuildDiagnosticSeverity.Error))
        {
            return EmptyAssets(plan, directory, blankScreen, diagnostics);
        }

        ResetDirectory(directory);
        var fontResolution = await new MenuFontResolver(runner, log).ResolveAsync(
            imageMagick!,
            options.MenuFont,
            options.MenuFontJapanese,
            options.MenuFontKorean,
            [plan.ScreenText],
            cancellationToken).ConfigureAwait(false);
        diagnostics.AddRange(fontResolution.Diagnostics);
        if (diagnostics.Any(item => item.Severity == BuildDiagnosticSeverity.Error))
        {
            return EmptyAssets(plan, directory, blankScreen, diagnostics);
        }

        var covers = FindAlbumCovers(plan.AlbumPages);
        var missingCovers = covers
            .Where(pair => pair.Value is null)
            .Select(pair => pair.Key)
            .Order(StringComparer.Ordinal)
            .ToArray();
        if (missingCovers.Length > 0)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "MENU_COVER_MISSING",
                $"{missingCovers.Length} 张专辑没有 cover.jpg/jpeg/png/webp，页面将使用黑色背景: " +
                string.Join("、", missingCovers.Take(3)) +
                (missingCovers.Length > 3 ? "…" : string.Empty)));
        }

        try
        {
            await RunImageMagickAsync(imageMagick!,
            [
                "-size", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}",
                "xc:none", "-depth", "8", "PNG32:" + blankScreen,
            ], cancellationToken).ConfigureAwait(false);
            await RequireFrameSizeAsync(
                imageIdentify!.Value,
                blankScreen,
                cancellationToken).ConfigureAwait(false);

            var backgrounds = new List<string>(plan.AlbumPageCount);
            for (var pageIndex = 0; pageIndex < plan.AlbumPages.Count; pageIndex++)
            {
                var page = plan.AlbumPages[pageIndex];
                var background = Path.Combine(directory, $"bg{pageIndex}.jpg");
                covers.TryGetValue(page.Album, out var cover);
                await MakeBackgroundAsync(
                    imageMagick!, cover, background,
                    cover is null ? 0 : options.MenuCoverDim,
                    cancellationToken).ConfigureAwait(false);
                await RequireFrameSizeAsync(
                    imageIdentify.Value,
                    background,
                    cancellationToken).ConfigureAwait(false);
                backgrounds.Add(background);
            }

            string? indexCoversFile = null;
            if (plan.IndexPages > 0)
            {
                indexCoversFile = Path.Combine(directory, "index_covers.txt");
                var indexCoverEntries = plan.IndexAlbums
                    .SelectMany(albums => albums)
                    .Select(album => new
                    {
                        Album = album,
                        Cover = covers.TryGetValue(album, out var cover) ? cover : null,
                    })
                    .ToArray();
                var missingIndexCovers = indexCoverEntries
                    .Where(entry => string.IsNullOrWhiteSpace(entry.Cover))
                    .Select(entry => entry.Album)
                    .ToArray();
                if (missingIndexCovers.Length > 0)
                {
                    diagnostics.Add(Error(
                        "MENU_INDEX_COVER_MISSING",
                        "一级菜单按位置解释封面清单，不能压缩缺项；缺少封面的专辑: " +
                        string.Join("、", missingIndexCovers.Take(5)) +
                        (missingIndexCovers.Length > 5 ? "…" : string.Empty)));
                }
                var indexCovers = indexCoverEntries
                    .Where(entry => !string.IsNullOrWhiteSpace(entry.Cover))
                    .Select(entry => entry.Cover!)
                    .ToArray();
                await File.WriteAllLinesAsync(indexCoversFile, indexCovers, cancellationToken)
                    .ConfigureAwait(false);
                var expectedIndexCovers = plan.IndexAlbums.Sum(albums => albums.Count);
                if (indexCovers.Length != expectedIndexCovers)
                {
                    diagnostics.Add(Error(
                        "MENU_INDEX_COVER_COUNT_MISMATCH",
                        $"索引封面清单 {indexCovers.Length} 项 != 索引格子 {expectedIndexCovers} 项。"));
                }
            }

            var stillPictures = new List<string>();
            if (options.MenuStillPictures)
            {
                var albumStillPaths = new Dictionary<string, string>(StringComparer.Ordinal);
                foreach (var page in plan.AlbumPages)
                {
                    if (albumStillPaths.ContainsKey(page.Album))
                    {
                        continue;
                    }
                    covers.TryGetValue(page.Album, out var cover);
                    if (cover is null)
                    {
                        albumStillPaths[page.Album] = string.Empty;
                        continue;
                    }

                    var still = Path.Combine(directory, $"still{albumStillPaths.Count}.jpg");
                    try
                    {
                        await MakeStillAsync(imageMagick!, cover, still, cancellationToken)
                            .ConfigureAwait(false);
                        await RequireFrameSizeAsync(
                            imageIdentify.Value,
                            still,
                            cancellationToken).ConfigureAwait(false);
                        albumStillPaths[page.Album] = still;
                    }
                    catch (InvalidOperationException exception)
                    {
                        diagnostics.Add(new BuildDiagnostic(
                            BuildDiagnosticSeverity.Warning,
                            "MENU_STILL_FAILED",
                            $"生成专辑 {page.Album} 的播放静图失败: {exception.Message}"));
                        albumStillPaths[page.Album] = string.Empty;
                    }
                }

                foreach (var page in plan.AlbumPages)
                {
                    stillPictures.AddRange(Enumerable.Repeat(
                        albumStillPaths[page.Album], page.TrackCount));
                }
            }

            ValidateAssets(plan, blankScreen, backgrounds, stillPictures, diagnostics);
            return new MenuAssets(
                plan,
                directory,
                blankScreen,
                backgrounds,
                stillPictures,
                indexCoversFile,
                options.AuthorSource,
                options.MenuBinaryDirectory,
                fontResolution.Font,
                fontResolution.JapaneseFont,
                fontResolution.KoreanFont,
                diagnostics);
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
            diagnostics.Add(Error("MENU_ASSET_FAILED", exception.Message));
            return EmptyAssets(plan, directory, blankScreen, diagnostics);
        }
    }

    private async Task MakeBackgroundAsync(
        string imageMagick,
        string? cover,
        string output,
        int dim,
        CancellationToken cancellationToken)
    {
        IReadOnlyList<string> arguments;
        if (cover is null)
        {
            arguments =
            [
                "-size", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}",
                "xc:black", "-quality", "88", output,
            ];
        }
        else
        {
            var values = new List<string>
            {
                cover,
                "-resize", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}^",
                "-gravity", "center",
                "-extent", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}",
            };
            if (dim > 0)
            {
                values.AddRange(["-brightness-contrast", $"-{Math.Clamp(dim, 0, 100)}x0"]);
            }
            values.AddRange(["-quality", "88", output]);
            arguments = values;
        }
        await RunImageMagickAsync(imageMagick, arguments, cancellationToken).ConfigureAwait(false);
    }

    private async Task MakeStillAsync(
        string imageMagick,
        string cover,
        string output,
        CancellationToken cancellationToken)
    {
        await RunImageMagickAsync(imageMagick,
        [
            cover,
            "-resize", "93.75%x100%!",
            "-resize", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}",
            "-background", "black", "-gravity", "center",
            "-extent", $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}",
            "-quality", "92", output,
        ], cancellationToken).ConfigureAwait(false);
    }

    private async Task RunImageMagickAsync(
        string executable,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken)
    {
        log?.WriteCommand(executable, arguments);
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = executable,
            Arguments = arguments,
            OnOutputLine = line => log?.WriteLine(line),
            OnErrorLine = line => log?.WriteLine(line),
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded)
        {
            var details = string.Join(Environment.NewLine,
                new[] { result.StandardOutput, result.StandardError }
                    .Where(value => !string.IsNullOrWhiteSpace(value)));
            throw new InvalidOperationException(
                $"ImageMagick 失败（退出码 {result.ExitCode}）" +
                (details.Length > 0 ? Environment.NewLine + details.Trim() : string.Empty));
        }
    }

    private async Task RequireFrameSizeAsync(
        (string Executable, IReadOnlyList<string> PrefixArguments) identify,
        string image,
        CancellationToken cancellationToken)
    {
        var arguments = new List<string>(identify.PrefixArguments)
        {
            "-format", "%w %h", image,
        };
        log?.WriteCommand(identify.Executable, arguments);
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = identify.Executable,
            Arguments = arguments,
            Timeout = TimeSpan.FromSeconds(30),
        }, cancellationToken).ConfigureAwait(false);
        var parts = result.StandardOutput.Split(
            [' ', '\r', '\n', '\t'],
            StringSplitOptions.RemoveEmptyEntries);
        if (!result.Succeeded || parts.Length != 2 ||
            !int.TryParse(parts[0], out var width) ||
            !int.TryParse(parts[1], out var height))
        {
            throw new InvalidOperationException($"无法读取菜单图片尺寸: {image}");
        }
        if (width != MenuPlanner.FrameWidth || height != MenuPlanner.FrameHeight)
        {
            throw new InvalidOperationException(
                $"{image} 尺寸为 {width}x{height}，应为 " +
                $"{MenuPlanner.FrameWidth}x{MenuPlanner.FrameHeight}");
        }
    }

    private static Dictionary<string, string?> FindAlbumCovers(
        IReadOnlyList<MenuPagePlan> pages)
    {
        var result = new Dictionary<string, string?>(StringComparer.Ordinal);
        foreach (var page in pages)
        {
            if (result.ContainsKey(page.Album))
            {
                continue;
            }
            var source = page.Tracks.FirstOrDefault()?.SourcePath;
            var albumDirectory = string.IsNullOrWhiteSpace(source)
                ? null
                : Path.GetDirectoryName(source);
            result[page.Album] = albumDirectory is null
                ? null
                : FindCover(albumDirectory);
        }
        return result;
    }

    private static string? FindCover(string directory)
    {
        foreach (var extension in new[] { "jpg", "jpeg", "png", "webp" })
        {
            var candidate = Path.Combine(directory, "cover." + extension);
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }
        return null;
    }

    private static void ValidateAssets(
        MenuPlan plan,
        string blankScreen,
        IReadOnlyList<string> backgrounds,
        IReadOnlyList<string> stillPictures,
        ICollection<BuildDiagnostic> diagnostics)
    {
        if (!File.Exists(blankScreen))
        {
            diagnostics.Add(Error("MENU_BLANKSCREEN_MISSING", "未生成菜单透明底图。"));
        }
        if (backgrounds.Count != plan.AlbumPageCount || backgrounds.Any(path => !File.Exists(path)))
        {
            diagnostics.Add(Error(
                "MENU_BACKGROUND_MISMATCH",
                $"菜单背景图数量或文件不完整: {backgrounds.Count}/{plan.AlbumPageCount}。"));
        }
        if (stillPictures.Count > 0 && stillPictures.Count != plan.TrackCount)
        {
            diagnostics.Add(Error(
                "MENU_STILL_MISMATCH",
                $"播放静图列表 {stillPictures.Count} 项 != 曲目数 {plan.TrackCount}。"));
        }
        if (plan.TotalPages != plan.IndexPages + plan.AlbumPageCount)
        {
            diagnostics.Add(Error(
                "MENU_PAGE_MISMATCH",
                "总菜单页数与索引页、专辑页之和不一致。"));
        }
        var segments = plan.ScreenText.Length == 0 || !plan.ScreenText.Contains('=')
            ? 0
            : plan.ScreenText[(plan.ScreenText.IndexOf('=') + 1)..].Count(character => character == ':') + 1;
        if (segments != plan.TotalPages)
        {
            diagnostics.Add(Error(
                "MENU_TEXT_PAGE_MISMATCH",
                $"screentext 段数 {segments} != 菜单页数 {plan.TotalPages}。"));
        }
    }

    private static MenuAssets EmptyAssets(
        MenuPlan plan,
        string directory,
        string blankScreen,
        IReadOnlyList<BuildDiagnostic> diagnostics) =>
        new(plan, directory, blankScreen, [], [], null,
            string.Empty, string.Empty, string.Empty, string.Empty,
            string.Empty, diagnostics);

    private static BuildDiagnostic Error(string code, string message) =>
        new(BuildDiagnosticSeverity.Error, code, message);

    private static void ResetDirectory(string path)
    {
        if (Directory.Exists(path))
        {
            Directory.Delete(path, recursive: true);
        }
        Directory.CreateDirectory(path);
    }

    private static string? FindExecutable(string name, string preferredDirectory)
    {
        if (BuiltinImages.IsImageTool(name)) return BuiltinImages.IsAvailable ? BuiltinImages.Tool(name) : null;
        var names = OperatingSystem.IsWindows() && Path.GetExtension(name).Length == 0
            ? new[] { name + ".exe", name + ".cmd", name + ".bat", name }
            : new[] { name };
        foreach (var candidateName in names)
        {
            var preferred = Path.Combine(preferredDirectory, candidateName);
            if (File.Exists(preferred))
            {
                return preferred;
            }
        }
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            foreach (var candidateName in names)
            {
                var candidate = Path.Combine(directory.Trim('"'), candidateName);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }
        }
        return null;
    }

    private static (string Executable, IReadOnlyList<string> PrefixArguments)?
        FindImageMagickIdentify(string preferredDirectory)
    {
        var magick = FindExecutable("magick", preferredDirectory);
        if (magick is not null)
        {
            return (magick, ["identify"]);
        }
        var identify = FindExecutable("identify", preferredDirectory);
        return identify is null ? null : (identify, []);
    }
}
