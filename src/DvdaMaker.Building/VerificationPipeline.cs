using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;
using DvdaMaker.Configuration;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

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

        var identifyPrefix = BuiltinImages.IsBuiltin(imageMagick) || Path.GetFileNameWithoutExtension(imageMagick)
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
        => VerifyTimelineCore(collectAuditObservations: false).Timeline;

    public (VerificationSectionResult Timeline,
        IReadOnlyDictionary<(string IsoPath, int Group), DiscVerifier.AuditPtsObservation> Observations)
        VerifyTimelineWithAuditObservations()
        => VerifyTimelineCore(collectAuditObservations: true);

    private (VerificationSectionResult Timeline,
        IReadOnlyDictionary<(string IsoPath, int Group), DiscVerifier.AuditPtsObservation> Observations)
        VerifyTimelineCore(bool collectAuditObservations)
    {
        var observations = new Dictionary<(string IsoPath, int Group), DiscVerifier.AuditPtsObservation>();
        var isoFiles = FindIsoFiles();
        if (isoFiles.Length == 0)
        {
            return (new VerificationSectionResult(
                "timeline",
                [new VerificationIssue("NO_ISO", "未找到成品 ISO")]), observations);
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
                    var observation = collectAuditObservations
                        ? new DiscVerifier.AuditPtsObservation() : null;
                    Action<ReadOnlyMemory<byte>>? observeSector = null;
                    if (observation is not null)
                    {
                        observeSector = observation.Observe;
                    }
                    var analysis = AobPtsAnalyzer.AnalyzeChunks(
                        ReadAobChunks(iso, entries),
                        $"{Path.GetFileName(isoPath)} 组 {group}", null,
                        observeSector);
                    if (observation is not null)
                    {
                        observations.Add((isoPath, group), observation);
                    }
                    issues.AddRange(analysis.Issues);
                }
            }
            return (new VerificationSectionResult("timeline", issues), observations);
        }
        catch (Exception exception) when (exception is Iso9660Exception or IOException)
        {
            // 任一组读取失败时，已收集的观察值不再代表完整的一轮扫描。
            // 审计回退到自身的读取路径，保持原有诊断语义。
            observations.Clear();
            return (new VerificationSectionResult(
                "timeline",
                [new VerificationIssue("TIMELINE_UNAVAILABLE", exception.Message)]), observations);
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
        if (index is null || index.Discs.Count == 0 || index.Discs.Any(disc => disc.Tracks.Count == 0))
            return new VerificationSectionResult("lossless",
                [indexIssue ?? new VerificationIssue("LOSSLESS_SOURCE_MISSING", "正式索引不包含可校验的音轨。")], true);

        var issues = new List<VerificationIssue>();
        var work = Path.Combine(options.BuildDirectory, "verify-tmp", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(work);
        try
        {
            foreach (var disc in index.Discs)
            {
                cancellationToken.ThrowIfCancellationRequested();
                var isoPath = Path.Combine(options.FinalDirectory, disc.IsoName);
                try
                {
                    using var iso = new Iso9660Reader(isoPath);
                    var entries = iso.ListDirectory("AUDIO_TS");
                    foreach (var group in disc.Groups)
                    {
                        var segments = entries.Where(entry => Regex.IsMatch(entry.Name,
                            $@"^ATS_{group.Number:00}_\d+\.AOB$", RegexOptions.IgnoreCase))
                            .OrderBy(entry => int.Parse(entry.Name.Split('_')[2].Split('.')[0], CultureInfo.InvariantCulture))
                            .ToArray();
                        if (segments.Length == 0 || segments.Any(entry => entry.Size % iso.SectorSize != 0))
                            throw new InvalidDataException($"第 {disc.Number} 盘组 {group.Number} 的 AOB 缺失或扇区不完整。");
                        NativeDiscVerifier.Verify(options.MenuBinaryDirectory,
                            group.Tracks.Select(track => track.MlpPath).ToArray(), ReadAobChunks(iso, segments), cancellationToken,
                            group.Tracks.All(track => track.MlpSource == "lpcm")
                                ? group.Tracks.Select(track => (byte)(track.LpcmTitleEnd ? 1 : 0)).ToArray() : null);
                        Console.WriteLine($"[校验] 第 {disc.Number} 盘组 {group.Number}：{group.Tracks.Count} 轨成品音频全部字节一致。");
                    }
                }
                catch (Exception error) when (error is Iso9660Exception or IOException or InvalidDataException or
                    InvalidOperationException or DllNotFoundException or EntryPointNotFoundException or BadImageFormatException)
                {
                    issues.Add(new VerificationIssue("ISO_AUDIO_MISMATCH", $"第 {disc.Number} 盘：{error.Message}"));
                }

                for (var number = 0; number < disc.Tracks.Count; number++)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var track = disc.Tracks[number];
                    var label = $"第 {disc.Number} 盘第 {number + 1} 轨（{Path.GetFileName(track.SourcePath)}）";
                    var folder = Path.Combine(work, $"disc{disc.Number}-track{number + 1}");
                    Directory.CreateDirectory(folder);
                    try
                    {
                        if (!File.Exists(track.SourcePath) || !File.Exists(track.MlpPath))
                            throw new IOException("缺少源音频或编码 MLP，无法完成逐轨校验。");
                        var sourceRaw = Path.Combine(folder, "source.raw");
                        var decodedRaw = Path.Combine(folder, "decoded.raw");
                        var source = track.SourcePath;
                        if (track.MlpSource is "surcode-batch" or "lpcm")
                        {
                            // Reuse the exact conversion policy used before encoding. This
                            // regenerates target PCM; no encoded stream is changed.
                            var converted = Path.Combine(folder, "converted.wav");
                            var result = await RunAsync(options.Ffmpeg,
                                FfmpegPcmConverter.Arguments(source, converted, track.SampleRate, track.Bits),
                                cancellationToken).ConfigureAwait(false);
                            if (!result.Succeeded) throw new InvalidDataException("无法重建编码输入 PCM：" + result.StandardError);
                            source = Path.Combine(folder, "input.wav");
                            SurcodePcmWav.Normalize(converted, source, track.SampleRate, track.Bits, cancellationToken);
                            File.Delete(converted);
                        }
                        else if (track.ExternalResampled || track.ResampleTo is not null)
                            throw new InvalidDataException("旧外部 MLP 的转换策略未知；不能用采样数相近代替 PCM 一致性校验。");
                        var sourceDecode = await RunAsync(options.Ffmpeg,
                            ["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i", source, "-f", "s24le", sourceRaw],
                            cancellationToken).ConfigureAwait(false);
                        var mlpDecode = await RunAsync(options.Ffmpeg,
                            ["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i", track.MlpPath, "-f", "s24le", decodedRaw],
                            cancellationToken).ConfigureAwait(false);
                        if (!sourceDecode.Succeeded || !mlpDecode.Succeeded)
                            throw new InvalidDataException("内置媒体组件无法解码目标 PCM 或 MLP。");
                        var encodedByBuiltin = track.MlpSource == "surcode-batch";
                        var comparison = encodedByBuiltin && track.Channels > 0 && track.SampleRate > 1000
                            ? PcmComparer.Compare(sourceRaw, decodedRaw, checked(3 * track.Channels), (track.SampleRate - 1) / 1000)
                            : PcmComparer.Compare(sourceRaw, decodedRaw);
                        if (!comparison.Match) throw new InvalidDataException(comparison.Reason);
                        Console.WriteLine($"[校验] {label}：完整目标 PCM 一致。");
                    }
                    catch (Exception error) when (error is IOException or InvalidDataException or
                        InvalidOperationException or ArgumentException)
                    {
                        issues.Add(new VerificationIssue("TRACK_PCM_MISMATCH", $"{label}：{error.Message}"));
                    }
                    finally { Directory.Delete(folder, recursive: true); }
                }
            }
        }
        finally { if (Directory.Exists(work)) Directory.Delete(work, recursive: true); }
        return new VerificationSectionResult("lossless", issues);
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
        if (BuiltinImages.IsImageTool(name)) return BuiltinImages.IsAvailable ? BuiltinImages.Tool(name) : null;
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
                                : groupElement.GetProperty("sr").GetInt32(),
                            detail.ValueKind == JsonValueKind.Object &&
                            detail.TryGetProperty("ch", out var channels) &&
                            channels.ValueKind == JsonValueKind.Number
                                ? channels.GetInt32()
                                : 0,
                            detail.ValueKind == JsonValueKind.Object && detail.TryGetProperty("bits", out var bits)
                                ? bits.GetInt32() : groupElement.GetProperty("bits").GetInt32(),
                            detail.ValueKind == JsonValueKind.Object && detail.TryGetProperty("lpcm_title_end", out var titleEnd)
                                && titleEnd.ValueKind == JsonValueKind.True));
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
        int SampleRate,
        int Channels,
        int Bits,
        bool LpcmTitleEnd);
}
