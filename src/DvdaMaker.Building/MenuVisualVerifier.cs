using System.Globalization;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record MenuVisualVerificationResult(
    IReadOnlyList<VerificationIssue> Issues,
    bool Unavailable = false)
{
    public bool Succeeded => !Unavailable && Issues.Count == 0;
}

public sealed class MenuVisualVerifier(ProcessRunner runner)
{
    private const int SectorSize = 2048;
    private const int FrameWidth = MenuPlanner.FrameWidth;
    private const int FrameHeight = MenuPlanner.FrameHeight;
    private const int IndexColumns = MenuPlanner.IndexColumns;
    private const int IndexRows = MenuPlanner.IndexRows;
    private const int IndexPerPage = MenuPlanner.IndexPerPage;
    private const int IndexCellWidth = FrameWidth / IndexColumns;
    private const int IndexCellHeight = 140;
    private const int IndexTop = 60;
    private const int IndexInset = 5;
    private const int IndexThumb = 100;
    private const int IndexThumbGap = 2;
    private const int IndexLabelHeight = 28;

    public async Task<MenuVisualVerificationResult> VerifyAsync(
        string isoPath,
        string ffmpeg,
        string imageMagick,
        IReadOnlyList<string> identifyPrefixArguments,
        int menuPages,
        int indexPages,
        int albumCount,
        CancellationToken cancellationToken = default)
    {
        var issues = new List<VerificationIssue>();
        var work = Path.Combine(Path.GetTempPath(), "dvda-menu-verify-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(work);
        try
        {
            var vob = Path.Combine(work, "AUDIO_TS.VOB");
            byte[] amg;
            using (var iso = new Formats.Iso9660.Iso9660Reader(isoPath))
            {
                if (!iso.Extract("AUDIO_TS/AUDIO_TS.VOB", vob))
                {
                    return Unavailable("无法从 ISO 提取 AUDIO_TS.VOB，无法进行菜单画面校验。");
                }
                amg = iso.ReadFile("AUDIO_TS/AUDIO_TS.IFO");
            }

            var pageCount = Math.Max(1, menuPages);
            IReadOnlyList<(uint Start, uint End)> ranges;
            try
            {
                ranges = MenuDiscVerifier.ReadMenuCellRanges(amg, pageCount);
            }
            catch (Exception exception) when (
                exception is InvalidDataException or ArgumentOutOfRangeException or OverflowException)
            {
                return Unavailable($"无法定位菜单页画面范围: {exception.Message}");
            }

            for (var pageIndex = 0; pageIndex < pageCount; pageIndex++)
            {
                var pageVob = pageCount == 1
                    ? vob
                    : Path.Combine(work, $"menu-page-{pageIndex + 1}.vob");
                if (pageCount > 1)
                {
                    await ExtractCellRangeAsync(
                        vob, pageVob, ranges[pageIndex], cancellationToken).ConfigureAwait(false);
                }

                var frame = Path.Combine(work, $"menu-frame-{pageIndex + 1}.png");
                var extraction = await runner.RunAsync(new ProcessRequest
                {
                    FileName = ffmpeg,
                    Arguments = ["-v", "error", "-y", "-i", pageVob, "-frames:v", "1", frame],
                }, cancellationToken).ConfigureAwait(false);
                if (!extraction.Succeeded || !File.Exists(frame))
                {
                    return Unavailable(
                        $"无法抽取菜单第 {pageIndex + 1} 页首帧（ffmpeg 退出码 {extraction.ExitCode}）。");
                }

                var expected = pageIndex < indexPages
                    ? ExpectedIndexCells(albumCount, pageIndex) : 0;
                var useBatch = expected > 0 && identifyPrefixArguments.Count == 1 &&
                    identifyPrefixArguments[0].Equals("identify", StringComparison.OrdinalIgnoreCase);
                var batch = useBatch
                    ? await ReadIndexPageBatchAsync(imageMagick, frame, expected, cancellationToken)
                        .ConfigureAwait(false)
                    : null;
                var frameStats = useBatch
                    ? batch?.FrameStats
                    : await ReadStatsAsync(
                        imageMagick, identifyPrefixArguments, frame,
                        "%[fx:mean] %[fx:standard_deviation] %k",
                        cancellationToken).ConfigureAwait(false);
                if (frameStats is null || frameStats.Length < 3)
                {
                    return Unavailable($"ImageMagick 无法读取菜单第 {pageIndex + 1} 页统计。");
                }
                var mean = frameStats[0] * 255;
                var standardDeviation = frameStats[1] * 255;
                var colors = frameStats[2];
                if (IsFrameNearSolid(standardDeviation, colors))
                {
                    issues.Add(new VerificationIssue(
                        "MENU_FRAME_NEAR_SOLID",
                        $"菜单第 {pageIndex + 1} 页接近纯色（均值 {mean:F0}，" +
                        $"标准差 {standardDeviation:F0}，颜色 {colors:F0}），背景图可能未生效。"));
                }

                if (pageIndex < indexPages)
                {
                    if (useBatch && batch is not null)
                    {
                        var (background, thumbnail, label) = ParseIndexBatchOutput(
                            batch.Output, expected, true);
                        AddIndexIssue(issues, "MENU_INDEX_BACKGROUND_INVALID", pageIndex,
                            "格子角落发白，疑似画布布局错误", background);
                        AddIndexIssue(issues, "MENU_INDEX_THUMBNAIL_MISSING", pageIndex,
                            "缩略图为空", thumbnail);
                        AddIndexIssue(issues, "MENU_INDEX_LABEL_MISSING", pageIndex,
                            "专辑名未检测到亮字", label);
                    }
                    else if (!useBatch)
                    {
                        await VerifyIndexPageAsync(
                            imageMagick, identifyPrefixArguments, frame, pageIndex, expected,
                            issues, cancellationToken).ConfigureAwait(false);
                    }
                }
            }

            return new MenuVisualVerificationResult(issues);
        }
        catch (Formats.Iso9660.Iso9660Exception exception)
        {
            return Unavailable($"读取 ISO 菜单 VOB 失败: {exception.Message}");
        }
        catch (IOException exception)
        {
            return Unavailable($"菜单视觉校验文件操作失败: {exception.Message}");
        }
        finally
        {
            try
            {
                if (Directory.Exists(work)) Directory.Delete(work, recursive: true);
            }
            catch (IOException)
            {
            }
        }
    }

    internal static bool IsFrameNearSolid(double standardDeviation, double colors) =>
        RustBridge.Run<bool>("menu.visual_near_solid", new
        {
            StandardDeviation = standardDeviation,
            Colors = colors,
        }, () => IsFrameNearSolidManaged(standardDeviation, colors));

    private static bool IsFrameNearSolidManaged(double standardDeviation, double colors) =>
        colors <= 50 || standardDeviation <= 1;

    internal static bool IsIndexBackgroundInvalid(double mean) =>
        RustBridge.Run<bool>("menu.visual_background_invalid", mean,
            () => IsIndexBackgroundInvalidManaged(mean));

    private static bool IsIndexBackgroundInvalidManaged(double mean) => mean > 160;

    internal static bool IsIndexThumbnailMissing(double mean) =>
        RustBridge.Run<bool>("menu.visual_thumbnail_missing", mean,
            () => IsIndexThumbnailMissingManaged(mean));

    private static bool IsIndexThumbnailMissingManaged(double mean) => mean <= 3;

    internal static bool IsIndexLabelMissing(double maximum, double mean) =>
        RustBridge.Run<bool>("menu.visual_label_missing", new { Maximum = maximum, Mean = mean },
            () => IsIndexLabelMissingManaged(maximum, mean));

    private static bool IsIndexLabelMissingManaged(double maximum, double mean) =>
        maximum <= 200 || mean > 200;

    internal static int ExpectedIndexCells(int albumCount, int pageIndex) =>
        RustBridge.Run<int>("menu.visual_expected_index_cells", new
        {
            AlbumCount = albumCount,
            PageIndex = pageIndex,
            PerPage = IndexPerPage,
        }, () => ExpectedIndexCellsManaged(albumCount, pageIndex));

    private static int ExpectedIndexCellsManaged(int albumCount, int pageIndex) =>
        Math.Clamp(albumCount - pageIndex * IndexPerPage, 0, IndexPerPage);

    private async Task VerifyIndexPageAsync(
        string imageMagick,
        IReadOnlyList<string> identifyPrefixArguments,
        string frame,
        int pageIndex,
        int expected,
        ICollection<VerificationIssue> issues,
        CancellationToken cancellationToken)
    {
        if (expected <= 0) return;
        var badBackground = new List<int>();
        var badThumbnail = new List<int>();
        var badLabel = new List<int>();
        var labelWidth = IndexCellWidth - 2 * IndexInset;
        var thumbnailX = IndexInset + (labelWidth - IndexThumb) / 2;
        for (var index = 0; index < expected; index++)
        {
            var offsetX = (index % IndexColumns) * IndexCellWidth;
            var offsetY = IndexTop + (index / IndexColumns) * IndexCellHeight;
            var background = await ReadCropAsync(
                imageMagick, identifyPrefixArguments, frame,
                "%[fx:mean*255]", offsetX + 1, offsetY + 1, 3, 3,
                cancellationToken).ConfigureAwait(false);
            if (background is null || IsIndexBackgroundInvalid(background.Value))
            {
                badBackground.Add(index + 1);
            }

            var thumbnail = await ReadCropAsync(
                imageMagick, identifyPrefixArguments, frame,
                "%[fx:mean*255]", offsetX + thumbnailX, offsetY + IndexInset,
                IndexThumb, IndexThumb, cancellationToken).ConfigureAwait(false);
            if (thumbnail is null || IsIndexThumbnailMissing(thumbnail.Value))
            {
                badThumbnail.Add(index + 1);
            }

            var labelY = offsetY + IndexInset + IndexThumb + IndexThumbGap;
            var labelMaximum = await ReadCropAsync(
                imageMagick, identifyPrefixArguments, frame,
                "%[fx:maxima*255]", offsetX + IndexInset, labelY,
                labelWidth, IndexLabelHeight, cancellationToken).ConfigureAwait(false);
            var labelMean = await ReadCropAsync(
                imageMagick, identifyPrefixArguments, frame,
                "%[fx:mean*255]", offsetX + IndexInset, labelY,
                labelWidth, IndexLabelHeight, cancellationToken).ConfigureAwait(false);
            if (labelMaximum is null || labelMean is null ||
                IsIndexLabelMissing(labelMaximum.Value, labelMean.Value))
            {
                badLabel.Add(index + 1);
            }
        }

        AddIndexIssue(issues, "MENU_INDEX_BACKGROUND_INVALID", pageIndex,
            "格子角落发白，疑似画布布局错误", badBackground);
        AddIndexIssue(issues, "MENU_INDEX_THUMBNAIL_MISSING", pageIndex,
            "缩略图为空", badThumbnail);
        AddIndexIssue(issues, "MENU_INDEX_LABEL_MISSING", pageIndex,
            "专辑名未检测到亮字", badLabel);
    }

    internal sealed record IndexPageBatch(double[] FrameStats, string Output);

    internal async Task<IndexPageBatch?> ReadIndexPageBatchAsync(
        string imageMagick,
        string frame,
        int expected,
        CancellationToken cancellationToken)
    {
        // 这里使用 magick 的图像栈，而不是其 identify 子命令。
        var arguments = new List<string>
        {
            frame, "-format", "F|%[fx:mean]|%[fx:standard_deviation]|%k\\n",
            "-write", "info:",
        };
        var labelWidth = IndexCellWidth - 2 * IndexInset;
        var thumbnailX = IndexInset + (labelWidth - IndexThumb) / 2;
        for (var index = 0; index < expected; index++)
        {
            var offsetX = (index % IndexColumns) * IndexCellWidth;
            var offsetY = IndexTop + (index / IndexColumns) * IndexCellHeight;
            var labelY = offsetY + IndexInset + IndexThumb + IndexThumbGap;
            arguments.AddRange(["(", "+clone", "-crop", $"3x3+{offsetX + 1}+{offsetY + 1}",
                "+repage", "-format", $"B|{index + 1}|%[fx:mean*255]\\n", "-write", "info:", ")", "-delete", "-1"]);
            arguments.AddRange(["(", "+clone", "-crop", $"{IndexThumb}x{IndexThumb}+{offsetX + thumbnailX}+{offsetY + IndexInset}",
                "+repage", "-format", $"T|{index + 1}|%[fx:mean*255]\\n", "-write", "info:", ")", "-delete", "-1"]);
            arguments.AddRange(["(", "+clone", "-crop", $"{labelWidth}x{IndexLabelHeight}+{offsetX + IndexInset}+{labelY}",
                "+repage", "-format", $"L|{index + 1}|%[fx:maxima*255]|%[fx:mean*255]\\n", "-write", "info:", ")", "-delete", "-1"]);
        }
        arguments.Add("null:");
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = imageMagick,
            Arguments = arguments,
            Timeout = TimeSpan.FromSeconds(120),
        }, cancellationToken).ConfigureAwait(false);
        var frameStats = result.Succeeded ? ParseBatchFrameStats(result.StandardOutput) : null;
        return frameStats is null ? null : new IndexPageBatch(frameStats, result.StandardOutput);
    }

    internal static double[]? ParseBatchFrameStats(string output) =>
        RustBridge.Run<double[]?>("menu.parse_batch_frame_stats", output,
            () => ParseBatchFrameStatsManaged(output));

    private static double[]? ParseBatchFrameStatsManaged(string output)
    {
        var lines = output.Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Select(line => line.TrimEnd('\r'))
            .Where(line => line.StartsWith("F|", StringComparison.Ordinal))
            .ToArray();
        if (lines.Length != 1) return null;
        var fields = lines[0].Split('|');
        return fields.Length == 4 && TryParse(fields[1], out var mean) &&
            TryParse(fields[2], out var deviation) && TryParse(fields[3], out var colors)
            ? [mean, deviation, colors]
            : null;
    }

    private sealed record RustIndexBatchResult(
        IReadOnlyList<int> Background,
        IReadOnlyList<int> Thumbnail,
        IReadOnlyList<int> Label);

    internal static (IReadOnlyList<int> Background, IReadOnlyList<int> Thumbnail,
        IReadOnlyList<int> Label) ParseIndexBatchOutput(string output, int expected, bool succeeded)
    {
        var result = RustBridge.Run<RustIndexBatchResult>("menu.parse_index_batch",
            new { Output = output, Expected = expected, Succeeded = succeeded },
            () => ParseIndexBatchOutputManaged(output, expected, succeeded));
        return (result.Background, result.Thumbnail, result.Label);
    }

    private static RustIndexBatchResult ParseIndexBatchOutputManaged(string output, int expected, bool succeeded)
    {
        var records = new Dictionary<(string Kind, int Index), string[]>();
        var duplicates = new HashSet<(string Kind, int Index)>();
        foreach (var line in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var fields = line.TrimEnd('\r').Split('|');
            if (fields.Length < 3 || fields[0] is not ("B" or "T" or "L") ||
                !int.TryParse(fields[1], NumberStyles.None, CultureInfo.InvariantCulture,
                    out var index) || index < 1 || index > expected)
            {
                continue;
            }
            var key = (fields[0], index);
            if (!records.TryAdd(key, fields)) duplicates.Add(key);
        }

        bool Valid(string kind, int index, int count, out double first, out double second)
        {
            first = second = 0;
            var key = (kind, index);
            return succeeded && !duplicates.Contains(key) &&
                records.TryGetValue(key, out var fields) && fields.Length == count &&
                TryParse(fields[2], out first) &&
                (count == 3 || TryParse(fields[3], out second));
        }

        var background = new List<int>();
        var thumbnail = new List<int>();
        var label = new List<int>();
        for (var index = 1; index <= expected; index++)
        {
            if (!Valid("B", index, 3, out var value, out _) ||
                IsIndexBackgroundInvalidManaged(value)) background.Add(index);
            if (!Valid("T", index, 3, out value, out _) ||
                IsIndexThumbnailMissingManaged(value)) thumbnail.Add(index);
            if (!Valid("L", index, 4, out var maximum, out var mean) ||
                IsIndexLabelMissingManaged(maximum, mean)) label.Add(index);
        }
        return new RustIndexBatchResult(background, thumbnail, label);
    }

    private static bool TryParse(string value, out double result) =>
        double.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out result) &&
        double.IsFinite(result);

    private static void AddIndexIssue(
        ICollection<VerificationIssue> issues,
        string code,
        int pageIndex,
        string message,
        IReadOnlyCollection<int> cells)
    {
        if (cells.Count == 0) return;
        issues.Add(new VerificationIssue(
            code,
            $"索引第 {pageIndex + 1} 页{message}，页内格号: {string.Join(", ", cells)}"));
    }

    private static async Task ExtractCellRangeAsync(
        string source,
        string destination,
        (uint Start, uint End) range,
        CancellationToken cancellationToken)
    {
        var offset = checked((long)range.Start * SectorSize);
        var length = checked(((long)range.End - range.Start + 1) * SectorSize);
        await using var input = new FileStream(
            source, FileMode.Open, FileAccess.Read, FileShare.Read,
            128 * 1024, FileOptions.Asynchronous | FileOptions.SequentialScan);
        await using var output = new FileStream(
            destination, FileMode.Create, FileAccess.Write, FileShare.None,
            128 * 1024, FileOptions.Asynchronous | FileOptions.SequentialScan);
        input.Position = offset;
        var buffer = new byte[128 * 1024];
        while (length > 0)
        {
            var wanted = (int)Math.Min(buffer.Length, length);
            var read = await input.ReadAsync(buffer.AsMemory(0, wanted), cancellationToken)
                .ConfigureAwait(false);
            if (read == 0)
            {
                throw new InvalidDataException("菜单 VOB cell 范围超出文件结尾");
            }
            await output.WriteAsync(buffer.AsMemory(0, read), cancellationToken)
                .ConfigureAwait(false);
            length -= read;
        }
    }

    private async Task<double[]?> ReadStatsAsync(
        string executable,
        IReadOnlyList<string> prefix,
        string image,
        string format,
        CancellationToken cancellationToken)
    {
        var arguments = new List<string>(prefix) { "-format", format, image };
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = executable,
            Arguments = arguments,
            Timeout = TimeSpan.FromSeconds(30),
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded) return null;
        var values = result.StandardOutput.Split(
            [' ', '\r', '\n', '\t'], StringSplitOptions.RemoveEmptyEntries);
        return values.Select(value => double.TryParse(
                value, NumberStyles.Float, CultureInfo.InvariantCulture, out var parsed)
                ? (double?)parsed
                : null)
            .TakeWhile(value => value is not null)
            .Select(value => value!.Value)
            .ToArray();
    }

    private Task<double?> ReadCropAsync(
        string executable,
        IReadOnlyList<string> prefix,
        string image,
        string format,
        int x,
        int y,
        int width,
        int height,
        CancellationToken cancellationToken) => ReadCropCoreAsync(
            executable, prefix, image, format, x, y, width, height, cancellationToken);

    private async Task<double?> ReadCropCoreAsync(
        string executable,
        IReadOnlyList<string> prefix,
        string image,
        string format,
        int x,
        int y,
        int width,
        int height,
        CancellationToken cancellationToken)
    {
        var arguments = new List<string>(prefix)
        {
            "-crop", $"{width}x{height}+{x}+{y}",
            "-format", format,
            image,
        };
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = executable,
            Arguments = arguments,
            Timeout = TimeSpan.FromSeconds(30),
        }, cancellationToken).ConfigureAwait(false);
        return result.Succeeded && double.TryParse(
            result.StandardOutput.Trim(), NumberStyles.Float,
            CultureInfo.InvariantCulture, out var value)
            ? value
            : null;
    }

    private static MenuVisualVerificationResult Unavailable(string message) =>
        new([new VerificationIssue("MENU_VISUAL_UNAVAILABLE", message)], true);
}
