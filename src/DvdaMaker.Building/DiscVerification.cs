using System.Buffers.Binary;
using System.Text.RegularExpressions;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Formats.Mpeg;

namespace DvdaMaker.Building;

public sealed record VerificationIssue(string Code, string Message);

public sealed record DiscVerificationResult(
    string IsoPath,
    int TrackCount,
    IReadOnlyList<VerificationIssue> Issues,
    bool Unavailable = false)
{
    public bool Succeeded => !Unavailable && Issues.Count == 0;
}

public sealed class DiscVerifier
{
    private const int SectorSize = 2048;
    private static readonly byte[] PackHeader = [0, 0, 1, 0xBA];
    private static readonly Regex TrackRowPattern = new(
        "^\\s*(?<group>\\d+)\\s+(?<title>\\d+)/\\d+\\s+(?<track>\\d+)\\s+(?<first>\\d+)\\s+(?<last>\\d+)\\s+(?<pts>\\d+)\\s+(?<length>\\d+)\\s+\\d+\\s*$",
        RegexOptions.Compiled | RegexOptions.Multiline);
    private static readonly Regex AnsiPattern = new(
        "\\x1b\\[[0-9;]*[A-Za-z]",
        RegexOptions.Compiled);
    private static readonly Regex OutputArgumentPattern = new(
        "(?:^|\\s)-o\\s+(?:\"(?<double>[^\"]+)\"|'(?<single>[^']+)'|(?<bare>\\S+))",
        RegexOptions.Compiled);

    public IReadOnlyList<DiscVerificationResult> QuickCheck(
        string isoDirectory,
        string? manifestPath = null,
        string? buildLogPath = null)
    {
        var expected = ReadManifestTrackCount(manifestPath);
        var logIssues = ReadPaddingIssues(buildLogPath);
        var files = Directory.Exists(isoDirectory)
            ? Directory.EnumerateFiles(isoDirectory, "*.iso").OrderBy(path => path).ToArray()
            : [];
        if (files.Length == 0)
        {
            return [new DiscVerificationResult(
                isoDirectory, 0, [new VerificationIssue("NO_ISO", $"未找到 ISO: {isoDirectory}")])];
        }

        var results = new List<DiscVerificationResult>();
        foreach (var iso in files)
        {
            var issues = new List<VerificationIssue>(logIssues);
            var count = 0;
            var maximumStillReference = 0;
            try
            {
                using var reader = new Iso9660Reader(iso);
                var entries = reader.ListDirectory("AUDIO_TS");
                var ifos = entries
                    .Where(entry => Regex.IsMatch(entry.Name, "^ATS_\\d+_0\\.IFO$",
                        RegexOptions.IgnoreCase))
                    .OrderBy(entry => entry.Name, StringComparer.OrdinalIgnoreCase)
                    .ToArray();
                if (ifos.Length == 0)
                {
                    issues.Add(new VerificationIssue("AUDIO_IFO_MISSING", "AUDIO_TS 下没有 ATS_xx_0.IFO"));
                }

                foreach (var ifo in ifos)
                {
                    var data = reader.ReadFile($"AUDIO_TS/{ifo.Name}");
                    var group = ParseGroup(data, ifo.Name);
                    count += group.TrackCount;
                    maximumStillReference = Math.Max(maximumStillReference, group.MaximumStillReference);
                    issues.AddRange(group.Issues);

                    foreach (var start in group.TrackStarts)
                    {
                        var sector = ReadGroupSector(reader, start, groupNumber: ParseGroupNumber(ifo.Name));
                        if (sector.Length < PackHeader.Length ||
                            !sector.AsSpan(0, PackHeader.Length).SequenceEqual(PackHeader))
                        {
                            issues.Add(new VerificationIssue(
                                "TRACK_NOT_PACK", $"{ifo.Name} 的轨道首扇区 {start} 不是 pack 头"));
                        }
                    }
                }

                if (maximumStillReference > 0)
                {
                    var audioSv = entries.FirstOrDefault(entry =>
                        entry.Name.Equals("AUDIO_SV.IFO", StringComparison.OrdinalIgnoreCase));
                    if (audioSv is not null)
                    {
                        var data = reader.ReadFile($"AUDIO_TS/{audioSv.Name}");
                        if (data.Length >= 0xE)
                        {
                            var recordCount = ReadU16(data, 0xC);
                            if (maximumStillReference > recordCount)
                            {
                                issues.Add(new VerificationIssue(
                                    "STILL_REFERENCE_OUT_OF_RANGE",
                                    $"静图引用号 {maximumStillReference} > AUDIO_SV.IFO 记录数 {recordCount}"));
                            }
                        }
                    }
                }
            }
            catch (Iso9660Exception exception)
            {
                issues.Add(new VerificationIssue("ISO_READ_FAILED", exception.Message));
            }

            results.Add(new DiscVerificationResult(iso, count, issues));
        }

        var total = results.Sum(result => result.TrackCount);
        if (expected is not null && total != expected && results.Count > 0)
        {
            var issue = new VerificationIssue(
                "TRACK_COUNT_MISMATCH",
                $"全部 ISO 的 IFO 声明轨数 {total} != manifest 曲目数 {expected}");
            results[^1] = results[^1] with
            {
                Issues = results[^1].Issues.Concat([issue]).ToArray(),
            };
        }
        return results;
    }

    public IReadOnlyList<DiscVerificationResult> Audit(
        string isoDirectory,
        string buildLogPath,
        string? manifestPath = null,
        bool allowLogFallback = true)
    {
        var selectedLog = allowLogFallback
            ? SelectLatestBuildLog(buildLogPath)
            : File.Exists(buildLogPath) ? buildLogPath : null;
        var results = QuickCheck(isoDirectory, manifestPath, selectedLog)
            .Select(result => result with { Issues = result.Issues.ToArray() })
            .ToArray();
        if (selectedLog is null)
        {
            return MarkUnavailable(results, "BUILD_LOG_MISSING", "未找到可用于审计的构建日志");
        }

        var log = ParseAuditLog(selectedLog);
        var rows = log.Rows;
        if (rows.Count == 0)
        {
            return MarkUnavailable(
                results, "TRACK_TABLE_MISSING", "构建日志中未解析到 dvda-author 轨道表");
        }
        if (log.Commands.Count == 0)
        {
            return MarkUnavailable(
                results, "DVDA_COMMAND_MISSING", "构建日志中未解析到 dvda-author 命令行");
        }

        var rowIndex = 0;
        var layouts = new List<DiscLayout>();
        foreach (var command in log.Commands)
        {
            var groups = new List<IReadOnlyList<TrackRow>>();
            for (var groupIndex = 0; groupIndex < command.GroupCount && rowIndex < rows.Count; groupIndex++)
            {
                var groupNumber = rows[rowIndex].Group;
                var groupRows = new List<TrackRow>();
                while (rowIndex < rows.Count && rows[rowIndex].Group == groupNumber)
                {
                    groupRows.Add(rows[rowIndex++]);
                }
                groups.Add(groupRows);
            }
            layouts.Add(new DiscLayout(command.DiscTag, groups));
        }

        for (var resultIndex = 0; resultIndex < results.Length; resultIndex++)
        {
            var result = results[resultIndex];
            var issues = result.Issues.ToList();
            var discNumber = ParseDiscNumberFromIso(result.IsoPath);
            var layout = discNumber is null
                ? layouts.ElementAtOrDefault(resultIndex)
                : layouts.FirstOrDefault(item => ParseTrailingNumber(item.DiscTag) == discNumber) ??
                  layouts.ElementAtOrDefault(resultIndex);
            if (layout is null)
            {
                issues.Add(new VerificationIssue(
                    "DISC_LOG_MAPPING_MISSING", "无法把 ISO 映射到构建日志中的 dvda-author 命令"));
                results[resultIndex] = result with { Issues = issues, Unavailable = true };
                continue;
            }

            var rowsForDisc = layout.Groups.SelectMany(group => group).ToArray();
            if (rowsForDisc.Length != result.TrackCount)
            {
                issues.Add(new VerificationIssue(
                    "DISC_TRACK_MAPPING_MISMATCH",
                    $"日志映射 {rowsForDisc.Length} 轨 != ISO IFO 声明 {result.TrackCount} 轨"));
            }
            foreach (var group in rowsForDisc.GroupBy(row => row.Group))
            {
                var groupRows = group.OrderBy(row => row.First).ToArray();
                for (var i = 1; i < groupRows.Length; i++)
                {
                    if (groupRows[i].First != groupRows[i - 1].Last + 1)
                    {
                        issues.Add(new VerificationIssue(
                            "TRACK_GAP", $"组 {group.Key} 轨道表存在扇区断点"));
                    }
                }
                if (groupRows.Length > 0)
                {
                    var aobSectors = CountGroupAobSectors(result.IsoPath, group.Key);
                    var declared = groupRows.Max(row => row.Last) + 1;
                    if (aobSectors != declared)
                    {
                        issues.Add(new VerificationIssue(
                            "AOB_SECTOR_MISMATCH",
                            $"组 {group.Key} AOB 扇区 {aobSectors} != 轨道表 {declared}"));
                    }
                    ValidatePts(result.IsoPath, group.Key, groupRows, issues);
                }
            }
            results[resultIndex] = result with { Issues = issues };
        }
        return results;
    }

    private static GroupParseResult
        ParseGroup(byte[] data, string name)
    {
        var issues = new List<VerificationIssue>();
        var starts = new List<int>();
        if (data.Length < 212)
        {
            return new GroupParseResult(
                0, starts, 0, [new VerificationIssue("IFO_SHORT", $"{name} 文件过短")]);
        }
        var pointer = checked((int)ReadU32(data, 204)) * SectorSize;
        if (pointer < 0 || pointer + 8 > data.Length)
        {
            return new GroupParseResult(
                0, starts, 0, [new VerificationIssue("PGC_OUT_OF_RANGE", $"{name} 的 PGCI 指针越界")]);
        }
        var pgcLength = checked((int)ReadU32(data, pointer + 4)) + 1;
        if (pgcLength < 8 || pointer + pgcLength > data.Length)
        {
            return new GroupParseResult(
                0, starts, 0, [new VerificationIssue("PGC_OUT_OF_RANGE", $"{name} 的 PGC 越界")]);
        }
        var pgc = data.AsSpan(pointer, pgcLength);
        var titles = ReadU16(pgc, 0);
        var trackCount = 0;
        var maximumStillReference = 0;
        for (var title = 0; title < titles; title++)
        {
            var titleOffset = checked((int)ReadU32(pgc, 8 + title * 8 + 4));
            if (titleOffset + 16 > pgc.Length)
            {
                issues.Add(new VerificationIssue("TITLE_OUT_OF_RANGE", $"{name} 的 title {title + 1} 越界"));
                continue;
            }
            var tracks = pgc[titleOffset + 2];
            trackCount += tracks;
            if (tracks > 1)
            {
                var timestamps = new List<(uint First, uint Length)>();
                for (var track = 0; track < tracks; track++)
                {
                    var timestampOffset = titleOffset + 16 + track * 20;
                    if (timestampOffset + 20 > pgc.Length)
                    {
                        issues.Add(new VerificationIssue(
                            "CELL_TIMESTAMP_OUT_OF_RANGE",
                            $"{name} 的 title {title + 1} cell 时间戳表越界"));
                        break;
                    }
                    timestamps.Add((
                        ReadU32(pgc, timestampOffset + 6),
                        ReadU32(pgc, timestampOffset + 10)));
                }
                if (timestamps.Count == tracks)
                {
                    if (timestamps.Zip(timestamps.Skip(1), (left, right) => left.First < right.First)
                        .Any(increasing => !increasing))
                    {
                        issues.Add(new VerificationIssue(
                            "PGC_TIMELINE_NOT_INCREASING",
                            $"{name} 的 title {title + 1} cell first_pts 非严格递增"));
                    }
                    var titleLength = ReadU32(pgc, titleOffset + 4);
                    var finalEnd = (long)timestamps[^1].First + timestamps[^1].Length;
                    if (Math.Abs(finalEnd - titleLength) > 90_000)
                    {
                        issues.Add(new VerificationIssue(
                            "PGC_TIMELINE_LENGTH_MISMATCH",
                            $"{name} 的 title {title + 1} 末 cell 结束 {finalEnd} 与标题长度 {titleLength} 相差超过 1 秒"));
                    }
                }
            }

            var pictureTable = ReadU16(pgc, titleOffset + 14);
            if (pictureTable != 0)
            {
                for (var track = 0; track < tracks; track++)
                {
                    var pictureOffset = titleOffset + pictureTable + track * 6;
                    if (pictureOffset + 6 > pgc.Length) break;
                    maximumStillReference = Math.Max(maximumStillReference, pgc[pictureOffset]);
                }
            }
            var sectorTable = titleOffset + ReadU16(pgc, titleOffset + 12);
            for (var track = 0; track < tracks; track++)
            {
                var entry = sectorTable + track * 12;
                if (entry + 12 > pgc.Length)
                {
                    issues.Add(new VerificationIssue("CELL_OUT_OF_RANGE", $"{name} 的 cell 表越界"));
                    break;
                }
                starts.Add(checked((int)ReadU32(pgc, entry + 4)));
            }
        }
        return new GroupParseResult(trackCount, starts, maximumStillReference, issues);
    }

    private static int CountGroupAobSectors(string isoPath, int group)
    {
        using var reader = new Iso9660Reader(isoPath);
        return reader.ListDirectory("AUDIO_TS")
            .Where(entry => Regex.IsMatch(entry.Name, $"^ATS_{group:00}_\\d+\\.AOB$",
                RegexOptions.IgnoreCase))
            .Sum(entry => (int)(entry.Size / SectorSize));
    }

    private static byte[] ReadGroupSector(Iso9660Reader reader, int groupSector, int groupNumber)
    {
        var offset = 0;
        foreach (var entry in reader.ListDirectory("AUDIO_TS")
                     .Where(entry => Regex.IsMatch(entry.Name, $"^ATS_{groupNumber:00}_\\d+\\.AOB$",
                         RegexOptions.IgnoreCase))
                     .OrderBy(entry => entry.Name, StringComparer.OrdinalIgnoreCase))
        {
            var sectors = (int)((entry.Size + SectorSize - 1) / SectorSize);
            if (groupSector < offset + sectors)
            {
                return reader.ReadSectors(entry.LogicalBlockAddress + groupSector - offset, 1);
            }
            offset += sectors;
        }
        return [];
    }

    private static int ParseGroupNumber(string name) =>
        int.TryParse(name.AsSpan(4, 2), out var value) ? value : 0;

    private static void ValidatePts(
        string isoPath,
        int group,
        IReadOnlyList<TrackRow> rows,
        ICollection<VerificationIssue> issues)
    {
        using var reader = new Iso9660Reader(isoPath);
        var aobs = reader.ListDirectory("AUDIO_TS")
            .Where(entry => Regex.IsMatch(entry.Name, $"^ATS_{group:00}_\\d+\\.AOB$",
                RegexOptions.IgnoreCase))
            .OrderBy(entry => entry.Name, StringComparer.OrdinalIgnoreCase)
            .SelectMany(entry => reader.ReadFile($"AUDIO_TS/{entry.Name}")
                .Chunk(SectorSize)
                .SelectMany(chunk => chunk.Length == SectorSize
                    ? new[] { chunk }
                    : Array.Empty<byte[]>()))
            .ToArray();
        var previous = -1L;
        var drops = new HashSet<int>();
        for (var sectorIndex = 0; sectorIndex < aobs.Length; sectorIndex++)
        {
            var sector = aobs[sectorIndex];
            var relativeMarker = sector.AsSpan(4, Math.Min(60, sector.Length - 4))
                .IndexOf(new byte[] { 0, 0, 1, 0xBD });
            var marker = relativeMarker < 0 ? -1 : relativeMarker + 4;
            if (marker < 0 || marker + 14 > sector.Length || (sector[marker + 7] & 0x80) == 0)
            {
                issues.Add(new VerificationIssue("PTS_MISSING", $"组 {group} 存在缺少 PTS 的扇区"));
                break;
            }
            var current = PesTimestampParser.ParsePts(sector.AsSpan(marker + 9, 5));
            if (previous >= 0 && current < previous)
            {
                drops.Add(sectorIndex);
                var boundary = rows.Any(row => row.First == sectorIndex);
                if (!boundary)
                {
                    issues.Add(new VerificationIssue(
                        "PTS_DROP_OFF_BOUNDARY", $"组 {group} PTS 在非轨道边界扇区 {sectorIndex} 下降"));
                }
            }
            previous = current;
        }

        var previousTitle = -1;
        foreach (var row in rows)
        {
            if (row.Title != previousTitle)
            {
                if (row.First != 0 && row.First < aobs.Length && !drops.Contains(row.First))
                {
                    issues.Add(new VerificationIssue(
                        "PTS_RESET_MISSING",
                        $"组 {group} title {row.Title} 起点扇区 {row.First} 应出现 PTS 下降但未下降"));
                }
                previousTitle = row.Title;
            }
        }
    }

    internal static AuditLogData ParseAuditLog(string path)
    {
        var text = AnsiPattern.Replace(File.ReadAllText(path), string.Empty);
        var rows = TrackRowPattern.Matches(text)
            .Select(match => new TrackRow(
                int.Parse(match.Groups["group"].Value),
                int.Parse(match.Groups["title"].Value),
                int.Parse(match.Groups["track"].Value),
                int.Parse(match.Groups["first"].Value),
                int.Parse(match.Groups["last"].Value),
                long.Parse(match.Groups["pts"].Value),
                long.Parse(match.Groups["length"].Value)))
            .ToArray();
        var commands = new List<DiscCommand>();
        foreach (var line in text.Split('\n'))
        {
            var trimmed = line.TrimEnd('\r');
            if (!trimmed.StartsWith("+ ", StringComparison.Ordinal)) continue;
            var groupCount = Regex.Matches(trimmed, "(?:^|\\s)-g(?:\\s|$)").Count;
            if (groupCount == 0) continue;
            var output = OutputArgumentPattern.Match(trimmed);
            if (!output.Success) continue;
            var outputPath = output.Groups["double"].Success ? output.Groups["double"].Value
                : output.Groups["single"].Success ? output.Groups["single"].Value
                : output.Groups["bare"].Value;
            var discTag = Path.GetFileName(outputPath.TrimEnd('/', '\\'));
            commands.Add(new DiscCommand(discTag, groupCount));
        }
        return new AuditLogData(rows, commands);
    }

    internal static string? SelectLatestBuildLog(string requestedPath)
    {
        var directory = Path.GetDirectoryName(requestedPath);
        var candidates = new[]
        {
            requestedPath,
            directory is null ? null : Path.Combine(directory, "build.log"),
            directory is null ? null : Path.Combine(directory, "rebuild-final.log"),
            directory is null ? null : Path.Combine(directory, "finalrebuild.log"),
        };
        return candidates
            .Where(path => path is not null && File.Exists(path))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .OrderByDescending(path => File.GetLastWriteTimeUtc(path!))
            .FirstOrDefault();
    }

    private static DiscVerificationResult[] MarkUnavailable(
        IEnumerable<DiscVerificationResult> results,
        string code,
        string message) => results.Select(result => result with
        {
            Issues = result.Issues.Concat([new VerificationIssue(code, message)]).ToArray(),
            Unavailable = true,
        }).ToArray();

    private static int? ParseDiscNumberFromIso(string path)
    {
        var match = Regex.Match(Path.GetFileName(path), "_(\\d+)\\.iso$", RegexOptions.IgnoreCase);
        return match.Success ? int.Parse(match.Groups[1].Value) : null;
    }

    private static int? ParseTrailingNumber(string value)
    {
        var match = Regex.Match(value, "(\\d+)$");
        return match.Success ? int.Parse(match.Groups[1].Value) : null;
    }

    private static IReadOnlyList<VerificationIssue> ReadPaddingIssues(string? path) =>
        path is not null && File.Exists(path) && AnsiPattern.Replace(
            File.ReadAllText(path), string.Empty).Contains(
            "pes_padding length must be higher", StringComparison.OrdinalIgnoreCase)
            ? [new VerificationIssue("PES_PADDING_FAILED", "构建日志包含 pack 补齐失败")]
            : [];

    private static int? ReadManifestTrackCount(string? path)
    {
        if (path is null || !File.Exists(path)) return null;
        using var document = System.Text.Json.JsonDocument.Parse(File.ReadAllText(path));
        return document.RootElement.EnumerateObject()
            .Where(property => property.Value.TryGetProperty("files", out _))
            .Sum(property => property.Value.GetProperty("files").GetArrayLength());
    }

    private static ushort ReadU16(ReadOnlySpan<byte> data, int offset) =>
        BinaryPrimitives.ReadUInt16BigEndian(data[offset..]);

    private static uint ReadU32(ReadOnlySpan<byte> data, int offset) =>
        BinaryPrimitives.ReadUInt32BigEndian(data[offset..]);

    internal sealed record TrackRow(int Group, int Title, int Track, int First, int Last, long Pts, long Length);
    internal sealed record DiscCommand(string DiscTag, int GroupCount);
    private sealed record DiscLayout(string DiscTag, IReadOnlyList<IReadOnlyList<TrackRow>> Groups);
    internal sealed record AuditLogData(IReadOnlyList<TrackRow> Rows, IReadOnlyList<DiscCommand> Commands);
    private sealed record GroupParseResult(
        int TrackCount,
        IReadOnlyList<int> TrackStarts,
        int MaximumStillReference,
        IReadOnlyList<VerificationIssue> Issues);
}
