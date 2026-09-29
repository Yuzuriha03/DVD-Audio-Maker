using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;
using DvdaMaker.Configuration;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record CapacityVerificationResult(
    string IsoPath,
    long Size,
    long Limit,
    string VolumeIdentifier,
    IReadOnlyList<string> RootEntries,
    IReadOnlyList<VerificationIssue> Issues)
{
    public bool Succeeded => Issues.Count == 0;
}

public sealed record VerificationSectionResult(
    string Name,
    IReadOnlyList<VerificationIssue> Issues,
    bool Unavailable = false)
{
    public bool Succeeded => !Unavailable && Issues.Count == 0;
}

public sealed class VerificationPipeline(DvdaOptions options, ProcessRunner? runner = null)
{
    private readonly ProcessRunner _runner = runner ?? new ProcessRunner();

    public IReadOnlyList<CapacityVerificationResult> VerifyCapacity()
    {
        var files = FindIsoFiles();
        if (files.Length == 0)
        {
            return
            [
                new CapacityVerificationResult(
                    options.FinalDirectory, 0, options.DiscBytes, string.Empty, [],
                    [new VerificationIssue("NO_ISO", $"未找到 ISO: {options.FinalDirectory}")]),
            ];
        }

        var results = new List<CapacityVerificationResult>();
        foreach (var path in files)
        {
            var issues = new List<VerificationIssue>();
            var size = new FileInfo(path).Length;
            if (size > options.DiscBytes)
            {
                issues.Add(new VerificationIssue(
                    "ISO_CAPACITY_EXCEEDED",
                    $"ISO 超出单盘上限 {size - options.DiscBytes:N0} 字节"));
            }

            try
            {
                using var iso = new Iso9660Reader(path);
                var root = iso.ListDirectory().Select(entry => entry.Name).ToArray();
                if (!root.Contains("AUDIO_TS", StringComparer.OrdinalIgnoreCase))
                {
                    issues.Add(new VerificationIssue(
                        "AUDIO_TS_MISSING", "ISO 根目录缺少 AUDIO_TS"));
                }
                var expectedDisc = ParseDiscNumber(path);
                if (expectedDisc is not null)
                {
                    var expectedVolume = options.VolumeId(expectedDisc.Value);
                    if (!string.Equals(
                            iso.VolumeIdentifier.Trim(), expectedVolume.Trim(),
                            StringComparison.Ordinal))
                    {
                        issues.Add(new VerificationIssue(
                            "VOLUME_ID_MISMATCH",
                            $"卷标 {iso.VolumeIdentifier} != 期望 {expectedVolume}"));
                    }
                }
                results.Add(new CapacityVerificationResult(
                    path, size, options.DiscBytes, iso.VolumeIdentifier, root, issues));
            }
            catch (Iso9660Exception exception)
            {
                issues.Add(new VerificationIssue("ISO_READ_FAILED", exception.Message));
                results.Add(new CapacityVerificationResult(
                    path, size, options.DiscBytes, string.Empty, [], issues));
            }
        }
        return results;
    }

    public async Task<IReadOnlyList<MenuVerificationResult>> VerifyMenuAsync(
        CancellationToken cancellationToken = default)
    {
        if (!options.MenuEnabled)
        {
            return [];
        }

        var index = ReadIndex(out var indexIssue);
        if (index is null)
        {
                return
            [
                new MenuVerificationResult(
                    options.MlpIndexPath,
                    [indexIssue ?? new VerificationIssue(
                        "MLP_INDEX_INVALID", "无法读取 mlp_index.json")],
                    0, 0,
                    Unavailable: true),
            ];
        }

        var verifier = new MenuDiscVerifier();
        var results = new List<MenuVerificationResult>();
        foreach (var disc in index.Discs)
        {
            var isoPath = Path.Combine(options.FinalDirectory, disc.IsoName);
            if (!File.Exists(isoPath))
            {
                results.Add(new MenuVerificationResult(
                    isoPath,
                    [new VerificationIssue("ISO_MISSING", $"找不到 {isoPath}")],
                    0, 0));
                continue;
            }

            var albumPages = CountAlbumPages(
                disc.Tracks.Select(track => AlbumDirectoryName(track.SourcePath)).ToArray(),
                Math.Min(options.MenuTracksPerPage, MenuPlanner.MaximumMenuRows));
            var indexPages = albumPages == 0
                ? 0
                : (albumPages + MenuPlanner.IndexPerPage - 1) / MenuPlanner.IndexPerPage;
            if (indexPages > 0 && albumPages < options.MenuIndexMinimumAlbums)
            {
                indexPages = 0;
            }
            var expectation = CreateMenuExpectation(disc);
            var result = verifier.Verify(isoPath, expectation);
            var visual = await VerifyMenuVisualAsync(
                isoPath, expectation.Pages, expectation.IndexPages,
                expectation.Albums, cancellationToken).ConfigureAwait(false);
            results.Add(result with
            {
                Issues = result.Issues.Concat(visual.Issues).ToArray(),
                Unavailable = result.Unavailable ||
                    visual.Unavailable && result.Issues.Count == 0,
            });
        }
        return results;
    }

    public async Task<MenuVerificationResult> VerifyMenuIsoAsync(
        string isoPath,
        CancellationToken cancellationToken = default)
    {
        var fullPath = Path.GetFullPath(isoPath);
        if (!File.Exists(fullPath))
        {
            return new MenuVerificationResult(
                fullPath,
                [new VerificationIssue("ISO_MISSING", $"找不到 {fullPath}")],
                0, 0);
        }

        MenuVerificationExpectation? expectation = null;
        var index = ReadIndex(options.MlpIndexPath, out _);
        if (index is not null)
        {
            var disc = index.Discs.FirstOrDefault(item =>
                string.Equals(item.IsoName, Path.GetFileName(fullPath),
                    StringComparison.OrdinalIgnoreCase));
            if (disc is not null)
            {
                expectation = CreateMenuExpectation(disc);
            }
        }

        var result = new MenuDiscVerifier().Verify(fullPath, expectation);
        var menuPages = expectation?.Pages ?? Math.Max(1, result.MenuPages);
        var visual = await VerifyMenuVisualAsync(
            fullPath,
            menuPages,
            expectation?.IndexPages ?? 0,
            expectation?.Albums ?? 0,
            cancellationToken).ConfigureAwait(false);
        return result with
        {
            Issues = result.Issues.Concat(visual.Issues).ToArray(),
            Unavailable = result.Unavailable ||
                visual.Unavailable && result.Issues.Count == 0,
        };
    }

    private async Task<MenuVisualVerificationResult> VerifyMenuVisualAsync(
        string isoPath,
        int menuPages,
        int indexPages,
        int albumCount,
        CancellationToken cancellationToken)
    {
        var imageMagick = FindExecutable("magick", options.MenuBinaryDirectory)
            ?? FindExecutable("identify", options.MenuBinaryDirectory);
        if (imageMagick is null)
        {
            return new MenuVisualVerificationResult(
                [new VerificationIssue("MENU_VISUAL_UNAVAILABLE", "找不到 ImageMagick，无法抽帧检查菜单画面。")],
                true);
        }

        var identifyPrefix = Path.GetFileNameWithoutExtension(imageMagick)
            .Equals("magick", StringComparison.OrdinalIgnoreCase)
            ? (IReadOnlyList<string>)["identify"]
            : [];
        return await new MenuVisualVerifier(_runner).VerifyAsync(
            isoPath,
            options.Ffmpeg,
            imageMagick,
            identifyPrefix,
                menuPages,
                indexPages,
                albumCount,
            cancellationToken).ConfigureAwait(false);
    }

    public VerificationSectionResult VerifyTimeline()
    {
        var isoFiles = FindIsoFiles();
        if (isoFiles.Length == 0)
        {
            return new VerificationSectionResult(
                "timeline",
                [new VerificationIssue("NO_ISO", "未找到成品 ISO")]);
        }

        var issues = new List<VerificationIssue>();
        try
        {
            foreach (var isoPath in isoFiles)
            {
                using var iso = new Iso9660Reader(isoPath);
                var groups = iso.ListDirectory("AUDIO_TS")
                    .Select(entry => Regex.Match(
                        entry.Name,
                        @"^ATS_(\d{2})_\d+\.AOB$",
                        RegexOptions.IgnoreCase))
                    .Where(match => match.Success)
                    .Select(match => int.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture))
                    .Distinct()
                    .Order()
                    .ToArray();
                if (groups.Length == 0)
                {
                    issues.Add(new VerificationIssue(
                        "AOB_MISSING", $"{Path.GetFileName(isoPath)} 没有 ATS_xx_n.AOB"));
                    continue;
                }

                foreach (var group in groups)
                {
                    var entries = iso.ListDirectory("AUDIO_TS")
                        .Where(entry => Regex.IsMatch(
                            entry.Name,
                            $@"^ATS_{group:00}_\d+\.AOB$",
                            RegexOptions.IgnoreCase))
                        .OrderBy(entry => entry.Name, StringComparer.OrdinalIgnoreCase)
                        .ToArray();
                    var analysis = AobPtsAnalyzer.AnalyzeChunks(
                        ReadAobChunks(iso, entries),
                        $"{Path.GetFileName(isoPath)} 组 {group}");
                    issues.AddRange(analysis.Issues);
                }
            }
            return new VerificationSectionResult("timeline", issues);
        }
        catch (Exception exception) when (exception is Iso9660Exception or IOException)
        {
            return new VerificationSectionResult(
                "timeline",
                [new VerificationIssue("TIMELINE_UNAVAILABLE", exception.Message)]);
        }
    }

    internal static AobPtsAnalysis AnalyzeAobGroup(
        IReadOnlyList<(string Path, byte[] Data)> segments,
        string label)
        => AobPtsAnalyzer.AnalyzeChunks(
            segments.Select(segment => (ReadOnlyMemory<byte>)segment.Data), label);

    private static IEnumerable<ReadOnlyMemory<byte>> ReadAobChunks(
        Iso9660Reader iso,
        IReadOnlyList<IsoDirectoryEntry> entries)
    {
        const int sectorsPerChunk = 4096;
        foreach (var entry in entries)
        {
            var lba = iso.GetDataLogicalBlockAddress($"AUDIO_TS/{entry.Name}");
            var remaining = (long)entry.Size / iso.SectorSize;
            var offset = 0L;
            while (remaining > 0)
            {
                var count = (int)Math.Min(sectorsPerChunk, remaining);
                yield return iso.ReadSectors(lba + offset, count);
                offset += count;
                remaining -= count;
            }
        }
    }

    public async Task<VerificationSectionResult> VerifyLosslessAsync(
        CancellationToken cancellationToken = default)
    {
        var index = ReadIndex(out var indexIssue);
        var firstTrack = index?.Discs.FirstOrDefault()?.Groups.FirstOrDefault()?.Tracks.FirstOrDefault();
        if (firstTrack is null || string.IsNullOrWhiteSpace(firstTrack.MlpPath) ||
            !File.Exists(firstTrack.MlpPath))
        {
            return new VerificationSectionResult(
                "lossless",
                [indexIssue ?? new VerificationIssue(
                    "LOSSLESS_SOURCE_MISSING",
                    "无法从 mlp_index.json 定位第 1 盘、组 1、第 1 轨 MLP")],
                Unavailable: indexIssue is not null);
        }

        var issues = new List<VerificationIssue>();
        var work = Path.Combine(options.BuildDirectory, "verify-tmp", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(work);
        try
        {
            if (firstTrack.MlpSource is "external" or "surcode-batch" && firstTrack.ExternalResampled)
            {
                var decoded = await ReadDecodedSampleCountAsync(
                    firstTrack.MlpPath, cancellationToken).ConfigureAwait(false);
                var expectedSamples = checked((long)Math.Round(
                    firstTrack.Duration * firstTrack.SampleRate));
                var tolerance = firstTrack.SampleRate / 20;
                if (!decoded.ProcessSucceeded || decoded.SampleCount is null ||
                    decoded.DecodeErrors > 0 ||
                    Math.Abs(decoded.SampleCount.Value - expectedSamples) > tolerance)
                {
                    issues.Add(new VerificationIssue(
                        "MLP_SAMPLE_COUNT_MISMATCH",
                        $"MLP 解码采样数 {decoded.SampleCount?.ToString() ?? "未知"}，" +
                        $"期望 {expectedSamples}（容差 {tolerance}），" +
                        $"解码错误 {decoded.DecodeErrors}，退出成功 {decoded.ProcessSucceeded}"));
                }
            }
            else
            {
                var sourceRaw = Path.Combine(work, "source.raw");
                var decodedRaw = Path.Combine(work, "decoded.raw");
                var sourceArguments = new List<string>
                {
                    "-hide_banner", "-nostdin", "-loglevel", "error", "-y",
                    "-i", firstTrack.SourcePath,
                };
                if (firstTrack.ResampleTo is not null)
                {
                    sourceArguments.AddRange([
                        "-af", $"aresample={firstTrack.ResampleTo}:resampler=soxr",
                    ]);
                }
                sourceArguments.AddRange(["-f", "s24le", sourceRaw]);
                var sourceDecode = await RunAsync(
                    options.Ffmpeg, sourceArguments, cancellationToken).ConfigureAwait(false);
                var mlpDecode = await RunAsync(
                    options.Ffmpeg,
                    [
                        "-hide_banner", "-nostdin", "-loglevel", "error", "-y",
                        "-i", firstTrack.MlpPath, "-f", "s24le", decodedRaw,
                    ], cancellationToken).ConfigureAwait(false);
                if (!sourceDecode.Succeeded || !mlpDecode.Succeeded)
                {
                    issues.Add(new VerificationIssue(
                        "PCM_DECODE_FAILED", "ffmpeg 无法解码源音频或 MLP"));
                }
                else if (!FilesEqualPrefix(sourceRaw, decodedRaw))
                {
                    issues.Add(new VerificationIssue(
                        "SOURCE_PCM_MISMATCH", "MLP 解码 PCM 与源音频不一致"));
                }
            }

            var firstIso = FindIsoFiles().FirstOrDefault();
            if (firstIso is null)
            {
                issues.Add(new VerificationIssue("NO_ISO", "未找到第 1 张成品 ISO"));
            }
            else
            {
                var aobPath = Path.Combine(work, "ATS_01_1.AOB");
                using (var iso = new Iso9660Reader(firstIso))
                {
                    if (!iso.Extract("AUDIO_TS/ATS_01_1.AOB", aobPath))
                    {
                        issues.Add(new VerificationIssue(
                            "AOB_EXTRACT_FAILED", "无法从 ISO 提取 ATS_01_1.AOB"));
                    }
                }

                if (File.Exists(aobPath))
                {
                    var extractDirectory = Path.Combine(work, "aob-extract");
                    Directory.CreateDirectory(extractDirectory);
                    await RunAsync(
                        options.DvdaAuthor,
                        ["--aob-extract", aobPath, "-o", extractDirectory, "-W", "-P0", "-n"],
                        cancellationToken).ConfigureAwait(false);
                    var extracted = Directory.EnumerateFiles(
                            extractDirectory, "track_01_title_01.mlp", SearchOption.AllDirectories)
                        .FirstOrDefault();
                    if (extracted is null || !FilesEqual(firstTrack.MlpPath, extracted))
                    {
                        issues.Add(new VerificationIssue(
                            "ISO_MLP_MISMATCH", "成品 ISO 内提取出的首轨 MLP 与源 MLP 不一致"));
                    }
                }
            }
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
            issues.Add(new VerificationIssue("LOSSLESS_FAILED", exception.Message));
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
        return new VerificationSectionResult("lossless", issues);
    }

    private async Task<DecodedSampleResult> ReadDecodedSampleCountAsync(
        string path,
        CancellationToken cancellationToken)
    {
        var result = await RunAsync(
            options.Ffmpeg,
            [
                "-hide_banner", "-nostdin", "-v", "info", "-i", path,
                "-af", "astats=metadata=1", "-f", "null", "-",
            ], cancellationToken).ConfigureAwait(false);
        var text = result.StandardOutput + Environment.NewLine + result.StandardError;
        var match = Regex.Match(text, @"Number of samples:\s*(\d+)");
        long? sampleCount = match.Success && long.TryParse(
            match.Groups[1].Value, NumberStyles.Integer, CultureInfo.InvariantCulture, out var value)
            ? value
            : null;
        var decodeErrors = text.Split('\n').Count(line =>
            line.Contains("Error submitting", StringComparison.OrdinalIgnoreCase) ||
            line.Contains("invalid element", StringComparison.OrdinalIgnoreCase) ||
            line.Contains("not implemented", StringComparison.OrdinalIgnoreCase) ||
            line.Contains("Error while decoding", StringComparison.OrdinalIgnoreCase));
        return new DecodedSampleResult(sampleCount, decodeErrors, result.Succeeded);
    }

    private Task<ProcessResult> RunAsync(
        string executable,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken) => _runner.RunAsync(new ProcessRequest
    {
        FileName = executable,
        Arguments = arguments,
    }, cancellationToken);

    private string[] FindIsoFiles()
    {
        if (!Directory.Exists(options.FinalDirectory)) return [];
        return Directory.EnumerateFiles(options.FinalDirectory, "*.iso")
            .Where(path => DiscVerifier.MatchesCurrentIsoName(path, options.IsoPrefix))
            .OrderBy(path => ParseDiscNumber(path) ?? int.MaxValue)
            .ThenBy(path => path, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static string? FindExecutable(string name, string preferredDirectory)
    {
        var names = OperatingSystem.IsWindows() && Path.GetExtension(name).Length == 0
            ? new[] { name + ".exe", name + ".cmd", name + ".bat", name }
            : new[] { name };
        foreach (var candidateName in names)
        {
            var preferred = Path.Combine(preferredDirectory, candidateName);
            if (File.Exists(preferred)) return preferred;
        }
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            foreach (var candidateName in names)
            {
                var candidate = Path.Combine(directory.Trim('"'), candidateName);
                if (File.Exists(candidate)) return candidate;
            }
        }
        return null;
    }

    private VerificationIndex? ReadIndex(out VerificationIssue? issue) =>
        ReadIndex(options.MlpIndexPath, out issue);

    private static VerificationIndex? ReadIndex(
        string indexPath,
        out VerificationIssue? issue)
    {
        issue = ValidateIndexFile(indexPath);
        if (issue is not null) return null;
        try
        {
            using var document = JsonDocument.Parse(File.ReadAllText(indexPath));
            if (!document.RootElement.TryGetProperty("__discs__", out var discsElement))
            {
                issue = new VerificationIssue(
                    "MLP_INDEX_INVALID", "mlp_index.json 缺少 __discs__");
                return null;
            }
            var discs = new List<VerificationDisc>();
            foreach (var discElement in discsElement.EnumerateArray())
            {
                var groups = new List<VerificationGroup>();
                foreach (var groupElement in discElement.GetProperty("groups").EnumerateArray())
                {
                    var tracks = new List<VerificationTrack>();
                    foreach (var trackElement in groupElement.GetProperty("tracks").EnumerateArray())
                    {
                        var mlp = trackElement.GetProperty("mlp").GetString() ?? string.Empty;
                        var source = trackElement.GetProperty("src").GetString() ?? string.Empty;
                        var detail = document.RootElement.TryGetProperty(mlp, out var detailElement)
                            ? detailElement
                            : default;
                        tracks.Add(new VerificationTrack(
                            mlp,
                            source,
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("resample_to", out var resample) &&
                            resample.ValueKind == JsonValueKind.Number
                                ? resample.GetInt32()
                                : null,
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("mlp_source", out var sourceKind)
                                ? sourceKind.GetString() ?? "ffmpeg"
                                : "ffmpeg",
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("ext_resampled", out var externalResampled) &&
                            externalResampled.ValueKind == JsonValueKind.True,
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("dur", out var duration)
                                ? duration.GetDouble()
                                : 0,
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("sr", out var sampleRate)
                                ? sampleRate.GetInt32()
                                : groupElement.GetProperty("sr").GetInt32()));
                    }
                    groups.Add(new VerificationGroup(
                        groupElement.GetProperty("group").GetInt32(), tracks));
                }
                discs.Add(new VerificationDisc(
                    discElement.GetProperty("disc").GetInt32(),
                    discElement.GetProperty("iso").GetString() ?? string.Empty,
                    groups));
            }
            return new VerificationIndex(discs);
        }
        catch (Exception exception) when (
            exception is JsonException or IOException or InvalidOperationException or
                         KeyNotFoundException or FormatException or OverflowException)
        {
            issue = new VerificationIssue(
                "MLP_INDEX_INVALID", $"无法读取 mlp_index.json: {exception.Message}");
            return null;
        }
    }

    internal static VerificationIssue? ValidateIndexFile(string path)
    {
        if (!File.Exists(path))
        {
            return new VerificationIssue("MLP_INDEX_MISSING", $"找不到正式索引: {path}");
        }

        try
        {
            using var document = JsonDocument.Parse(File.ReadAllText(path));
            if (document.RootElement.TryGetProperty("__meta__", out var metadata) &&
                metadata.ValueKind == JsonValueKind.Object &&
                metadata.TryGetProperty("dry_run", out var dryRun) &&
                dryRun.ValueKind == JsonValueKind.True)
            {
                return new VerificationIssue(
                    "MLP_INDEX_DRY_RUN",
                    "当前索引来自 dry-run，不能作为现有成品 ISO 的校验依据");
            }
            return null;
        }
        catch (Exception exception) when (exception is JsonException or IOException)
        {
            return new VerificationIssue(
                "MLP_INDEX_INVALID", $"无法读取 mlp_index.json: {exception.Message}");
        }
    }

    private static int CountAlbumPages(IReadOnlyList<string> albums, int rowsPerPage)
    {
        var pages = 0;
        for (var index = 0; index < albums.Count;)
        {
            var end = index + 1;
            while (end < albums.Count && albums[end] == albums[index]) end++;
            pages += (end - index + rowsPerPage - 1) / rowsPerPage;
            index = end;
        }
        return pages;
    }

    private MenuVerificationExpectation CreateMenuExpectation(VerificationDisc disc)
    {
        var albums = disc.Tracks
            .Select(track => AlbumDirectoryName(track.SourcePath))
            .ToArray();
        var albumPages = CountAlbumPages(
            albums,
            Math.Min(options.MenuTracksPerPage, MenuPlanner.MaximumMenuRows));
        var indexPages = albumPages == 0
            ? 0
            : (albumPages + MenuPlanner.IndexPerPage - 1) / MenuPlanner.IndexPerPage;
        if (indexPages > 0 && albumPages < options.MenuIndexMinimumAlbums)
        {
            indexPages = 0;
        }
        return new MenuVerificationExpectation(
            albumPages + indexPages,
            disc.Tracks.Count,
            options.MenuStillPictures ? disc.Tracks.Count : 0,
            albumPages,
            indexPages);
    }

    private static string AlbumDirectoryName(string sourcePath)
    {
        var normalized = sourcePath.Replace('\\', '/').TrimEnd('/');
        var slash = normalized.LastIndexOf('/');
        if (slash <= 0) return string.Empty;
        var parent = normalized[..slash].TrimEnd('/');
        var parentSlash = parent.LastIndexOf('/');
        return parent[(parentSlash + 1)..];
    }

    private static int? ParseDiscNumber(string path)
    {
        var match = Regex.Match(Path.GetFileName(path), @"_(\d+)\.iso$", RegexOptions.IgnoreCase);
        return match.Success ? int.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture) : null;
    }

    private static bool FilesEqualPrefix(string sourcePath, string decodedPath)
    {
        var sourceLength = new FileInfo(sourcePath).Length;
        if (new FileInfo(decodedPath).Length < sourceLength) return false;
        using var source = File.OpenRead(sourcePath);
        using var decoded = File.OpenRead(decodedPath);
        return StreamsEqual(source, decoded, sourceLength);
    }

    private static bool FilesEqual(string leftPath, string rightPath)
    {
        var length = new FileInfo(leftPath).Length;
        if (new FileInfo(rightPath).Length != length) return false;
        using var left = File.OpenRead(leftPath);
        using var right = File.OpenRead(rightPath);
        return StreamsEqual(left, right, length);
    }

    private static bool StreamsEqual(Stream left, Stream right, long length)
    {
        var leftBuffer = new byte[128 * 1024];
        var rightBuffer = new byte[leftBuffer.Length];
        long readTotal = 0;
        while (readTotal < length)
        {
            var wanted = (int)Math.Min(leftBuffer.Length, length - readTotal);
            var leftRead = left.Read(leftBuffer, 0, wanted);
            var rightRead = right.Read(rightBuffer, 0, wanted);
            if (leftRead != rightRead || leftRead == 0) return false;
            if (!leftBuffer.AsSpan(0, leftRead).SequenceEqual(rightBuffer.AsSpan(0, rightRead)))
            {
                return false;
            }
            readTotal += leftRead;
        }
        return true;
    }

    private sealed record VerificationIndex(IReadOnlyList<VerificationDisc> Discs);
    private sealed record VerificationDisc(
        int Number,
        string IsoName,
        IReadOnlyList<VerificationGroup> Groups)
    {
        public IReadOnlyList<VerificationTrack> Tracks => Groups.SelectMany(group => group.Tracks).ToArray();
    }
    private sealed record VerificationGroup(
        int Number,
        IReadOnlyList<VerificationTrack> Tracks);
    private sealed record VerificationTrack(
        string MlpPath,
        string SourcePath,
        int? ResampleTo,
        string MlpSource,
        bool ExternalResampled,
        double Duration,
        int SampleRate);
    private sealed record DecodedSampleResult(
        long? SampleCount,
        int DecodeErrors,
        bool ProcessSucceeded);
}
