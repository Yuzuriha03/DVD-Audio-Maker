using System.Buffers.Binary;
using DvdaMaker.Formats.Iso9660;

namespace DvdaMaker.Building;

public sealed record MenuVerificationExpectation(
    int Pages,
    int Tracks,
    int StillPictures,
    int Albums,
    int IndexPages);

public sealed record MenuVerificationResult(
    string IsoPath,
    IReadOnlyList<VerificationIssue> Issues,
    int MenuPages,
    int StillPictures,
    bool Unavailable = false)
{
    public bool Succeeded => !Unavailable && Issues.Count == 0;
}

public sealed class MenuDiscVerifier
{
    public const int MaximumAsvsSectors = 4096;

    private const int SectorSize = 2048;
    private const int AmgLanguageUnitOffset = 0x1810;
    private const int AmgFirstPgcPointer = 0x181C;
    private const int AmgPgcIndexOffset = 0x1820;
    private const int PgcStride = 0x132;
    private const int PgcNextMenu = 0x09C;
    private const int PgcPreviousMenu = 0x09E;
    private const int PgcCellStart = 0x11E;
    private const int PgcCellStartCopy = 0x126;
    private const int PgcCellEnd = 0x12A;

    private const int AsvsTitleCount = 0x0C;
    private const int AsvsLastSector = 0x14;
    private const int AsvsTable = 0x60;
    private const int AsvsEntryLength = 8;

    public MenuVerificationResult Verify(
        string isoPath,
        MenuVerificationExpectation? expectation = null)
    {
        var issues = new List<VerificationIssue>();
        try
        {
            using var reader = new Iso9660Reader(isoPath);
            var entries = reader.ListDirectory("AUDIO_TS");
            var names = entries.Select(entry => entry.Name)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);
            var requiredFiles = new List<string> { "AUDIO_TS.IFO", "AUDIO_TS.VOB" };
            if (expectation is null || expectation.StillPictures > 0)
            {
                requiredFiles.AddRange(["AUDIO_SV.IFO", "AUDIO_SV.VOB"]);
            }
            foreach (var required in requiredFiles)
            {
                if (!names.Contains(required))
                {
                    issues.Add(new VerificationIssue(
                        "MENU_FILE_MISSING", $"AUDIO_TS 下缺少 {required}"));
                }
            }

            if (!names.Contains("AUDIO_TS.IFO"))
            {
                return new MenuVerificationResult(isoPath, issues, 0, 0);
            }

            var amg = reader.ReadFile("AUDIO_TS/AUDIO_TS.IFO");
            var menuPages = ReadMenuCount(amg, issues);
            if (expectation is not null && menuPages != expectation.Pages)
            {
                issues.Add(new VerificationIssue(
                    "MENU_PAGE_COUNT_MISMATCH",
                    $"AMG 声明 {menuPages} 页，规划为 {expectation.Pages} 页"));
            }

            ValidateAmgCapacity(amg, menuPages, issues);
            if (menuPages > 1 && names.Contains("AUDIO_TS.VOB"))
            {
                var menuVob = entries.First(entry =>
                    entry.Name.Equals("AUDIO_TS.VOB", StringComparison.OrdinalIgnoreCase));
                ValidateMenuCellChain(amg, menuPages, menuVob.Size, issues);
            }

            var stillPictures = 0;
            if (names.Contains("AUDIO_SV.IFO") && names.Contains("AUDIO_SV.VOB"))
            {
                var asvs = reader.ReadFile("AUDIO_TS/AUDIO_SV.IFO");
                var stillVob = entries.First(entry =>
                    entry.Name.Equals("AUDIO_SV.VOB", StringComparison.OrdinalIgnoreCase));
                stillPictures = ValidateAsvs(asvs, stillVob.Size, expectation, issues);
            }
            else if (expectation is { StillPictures: > 0 })
            {
                issues.Add(new VerificationIssue(
                    "ASVS_MISSING", "规划包含播放静图，但成品缺少 ASVS 文件"));
            }

            return new MenuVerificationResult(isoPath, issues, menuPages, stillPictures);
        }
        catch (Exception exception) when (exception is Iso9660Exception or IOException)
        {
            issues.Add(new VerificationIssue("MENU_ISO_READ_FAILED", exception.Message));
            return new MenuVerificationResult(isoPath, issues, 0, 0, true);
        }
        catch (Exception exception) when (
            exception is InvalidDataException or InvalidOperationException or
                         ArgumentOutOfRangeException or OverflowException)
        {
            issues.Add(new VerificationIssue("MENU_STRUCTURE_INVALID", exception.Message));
            return new MenuVerificationResult(isoPath, issues, 0, 0);
        }
    }

    internal static int ReadMenuCount(
        ReadOnlySpan<byte> data,
        ICollection<VerificationIssue> issues)
    {
        if (data.Length < AmgLanguageUnitOffset + 2)
        {
            issues.Add(new VerificationIssue(
                "AMG_TOO_SHORT", $"AUDIO_TS.IFO 只有 {data.Length} 字节，读不到菜单表"));
            return 0;
        }
        return ReadU16(data, AmgLanguageUnitOffset);
    }

    internal static void ValidateAmgCapacity(
        ReadOnlySpan<byte> data,
        int menuPages,
        ICollection<VerificationIssue> issues)
    {
        var count = Math.Max(1, menuPages);
        var required = 0x1820 + 8 * Math.Max(0, count - 1) + count * 0x13A;
        if (data.Length < required)
        {
            issues.Add(new VerificationIssue(
                "AMG_TABLE_OUT_OF_RANGE",
                $"AUDIO_TS.IFO 只有 {data.Length} 字节，菜单表至少需要 {required} 字节"));
        }
    }

    internal static void ValidateMenuCellChain(
        ReadOnlySpan<byte> data,
        int menuPages,
        long vobBytes,
        ICollection<VerificationIssue> issues)
    {
        if (menuPages <= 1) return;
        if (data.Length < AmgPgcIndexOffset + (menuPages - 1) * 8)
        {
            issues.Add(new VerificationIssue("AMG_PGC_INDEX_SHORT", "AMG 菜单 PGC 索引被截断"));
            return;
        }

        var bases = new List<int>(menuPages)
        {
            checked(AmgLanguageUnitOffset + (int)ReadU32(data, AmgFirstPgcPointer)),
        };
        for (var index = 0; index < menuPages - 1; index++)
        {
            bases.Add(checked(AmgLanguageUnitOffset +
                (int)ReadU32(data, AmgPgcIndexOffset + index * 8 + 4)));
        }

        if (bases.Distinct().Count() != menuPages || !bases.SequenceEqual(bases.Order()))
        {
            issues.Add(new VerificationIssue(
                "AMG_PGC_ORDER_INVALID", "AMG 菜单 PGC 地址不是唯一递增序列"));
            return;
        }
        for (var index = 1; index < bases.Count; index++)
        {
            if (bases[index] - bases[index - 1] != PgcStride)
            {
                issues.Add(new VerificationIssue(
                    "AMG_PGC_STRIDE_MISMATCH",
                    $"第 {index}/{index + 1} 页 PGC 间距 " +
                    $"0x{bases[index] - bases[index - 1]:X} != 0x{PgcStride:X}"));
                return;
            }
        }
        if (bases[^1] + PgcCellEnd + 4 > data.Length)
        {
            issues.Add(new VerificationIssue(
                "AMG_PGC_OUT_OF_RANGE", "AMG 菜单 PGC 表超出 AUDIO_TS.IFO"));
            return;
        }

        var totalSectors = (vobBytes + SectorSize - 1) / SectorSize;
        long span = 0;
        uint? previousEnd = null;
        for (var index = 0; index < bases.Count; index++)
        {
            var page = index + 1;
            var baseOffset = bases[index];
            var start = page == 1 ? 0u : ReadU32(data, baseOffset + PgcCellStart);
            var startCopy = page == 1 ? 0u : ReadU32(data, baseOffset + PgcCellStartCopy);
            var end = ReadU32(data, baseOffset + PgcCellEnd);
            var next = ReadU16(data, baseOffset + PgcNextMenu);
            var previous = ReadU16(data, baseOffset + PgcPreviousMenu);

            if (start != startCopy)
            {
                issues.Add(new VerificationIssue(
                    "AMG_CELL_START_COPY_MISMATCH",
                    $"第 {page} 页 cell 起始地址副本 {start}/{startCopy} 不一致"));
            }
            if (previousEnd is not null && start != previousEnd.Value + 1)
            {
                issues.Add(new VerificationIssue(
                    "AMG_CELL_CHAIN_BROKEN",
                    $"第 {page} 页从 {start} 开始，应为 {previousEnd.Value + 1}"));
            }
            if (end < start)
            {
                issues.Add(new VerificationIssue(
                    "AMG_CELL_RANGE_INVALID", $"第 {page} 页 cell 结束 {end} 小于起始 {start}"));
            }
            else
            {
                span += end - start + 1L;
            }
            if (page < menuPages && next != page + 1)
            {
                issues.Add(new VerificationIssue(
                    "AMG_NEXT_MENU_INVALID", $"第 {page} 页 Next={next}，应为 {page + 1}"));
            }
            if (page == menuPages && next != 0)
            {
                issues.Add(new VerificationIssue(
                    "AMG_LAST_NEXT_MENU_INVALID", $"末页 Next={next}，应为 0"));
            }
            if (page > 1 && previous != page - 1)
            {
                issues.Add(new VerificationIssue(
                    "AMG_PREVIOUS_MENU_INVALID",
                    $"第 {page} 页 Previous={previous}，应为 {page - 1}"));
            }
            previousEnd = end;
        }

        if (previousEnd is not null && previousEnd.Value != totalSectors - 1)
        {
            issues.Add(new VerificationIssue(
                "AMG_LAST_CELL_END_MISMATCH",
                $"末页结束于 {previousEnd.Value}，VOB 末扇区为 {totalSectors - 1}"));
        }
        if (span != totalSectors)
        {
            issues.Add(new VerificationIssue(
                "AMG_CELL_SPAN_MISMATCH",
                $"各页 cell 跨度之和 {span} != AUDIO_TS.VOB 扇区数 {totalSectors}"));
        }
    }

    internal static IReadOnlyList<(uint Start, uint End)> ReadMenuCellRanges(
        ReadOnlySpan<byte> data,
        int menuPages)
    {
        if (menuPages <= 0) return [];
        if (data.Length < AmgPgcIndexOffset + Math.Max(0, menuPages - 1) * 8)
        {
            throw new InvalidDataException("AMG 菜单 PGC 索引被截断");
        }

        var bases = new List<int>(menuPages)
        {
            checked(AmgLanguageUnitOffset + (int)ReadU32(data, AmgFirstPgcPointer)),
        };
        for (var index = 0; index < menuPages - 1; index++)
        {
            bases.Add(checked(AmgLanguageUnitOffset +
                (int)ReadU32(data, AmgPgcIndexOffset + index * 8 + 4)));
        }
        if (bases.Distinct().Count() != menuPages || !bases.SequenceEqual(bases.Order()))
        {
            throw new InvalidDataException("AMG 菜单 PGC 地址不是唯一递增序列");
        }
        if (bases[^1] + PgcCellEnd + 4 > data.Length)
        {
            throw new InvalidDataException("AMG 菜单 PGC 表超出 AUDIO_TS.IFO");
        }

        var ranges = new List<(uint Start, uint End)>(menuPages);
        for (var index = 0; index < bases.Count; index++)
        {
            var start = index == 0 ? 0u : ReadU32(data, bases[index] + PgcCellStart);
            var end = ReadU32(data, bases[index] + PgcCellEnd);
            if (end < start)
            {
                throw new InvalidDataException($"第 {index + 1} 页 cell 范围无效: {start}-{end}");
            }
            ranges.Add((start, end));
        }
        return ranges;
    }

    internal static int ValidateAsvs(
        ReadOnlySpan<byte> data,
        long vobBytes,
        MenuVerificationExpectation? expectation,
        ICollection<VerificationIssue> issues)
    {
        if (data.Length < AsvsTable)
        {
            issues.Add(new VerificationIssue("ASVS_TOO_SHORT", "AUDIO_SV.IFO 太短"));
            return 0;
        }

        var records = ReadU16(data, AsvsTitleCount);
        var sectors = (vobBytes + SectorSize - 1) / SectorSize;
        var lastSector = ReadU32(data, AsvsLastSector);
        if (lastSector != sectors - 1)
        {
            issues.Add(new VerificationIssue(
                "ASVS_SECTOR_COUNT_MISMATCH",
                $"IFO 记录 {lastSector + 1} 扇区，AUDIO_SV.VOB 实际 {sectors} 扇区"));
        }
        if (sectors > MaximumAsvsSectors)
        {
            issues.Add(new VerificationIssue(
                "ASVS_TOO_LARGE",
                $"AUDIO_SV.VOB 有 {sectors} 扇区，超过报警线 {MaximumAsvsSectors}"));
        }

        var pictures = 0;
        for (var index = 0; index < records; index++)
        {
            var offset = AsvsTable + index * AsvsEntryLength;
            if (offset + AsvsEntryLength > data.Length)
            {
                issues.Add(new VerificationIssue(
                    "ASVS_TABLE_TRUNCATED", $"ASVS 声明 {records} 条记录，但第 {index + 1} 条越界"));
                break;
            }
            var count = data[offset];
            var startPicture = ReadU16(data, offset + 2);
            var startSector = ReadU32(data, offset + 4);
            if (count == 0)
            {
                issues.Add(new VerificationIssue(
                    "ASVS_EMPTY_RECORD", $"第 {index + 1} 条静图记录的图数为 0"));
            }
            if (startPicture != pictures + 1)
            {
                issues.Add(new VerificationIssue(
                    "ASVS_PICTURE_SEQUENCE_BROKEN",
                    $"第 {index + 1} 条起始图号 {startPicture}，应为 {pictures + 1}"));
            }
            if ((long)startSector * SectorSize >= vobBytes)
            {
                issues.Add(new VerificationIssue(
                    "ASVS_SECTOR_OUT_OF_RANGE",
                    $"第 {index + 1} 条起始扇区 {startSector} 超出 AUDIO_SV.VOB"));
            }
            pictures += count;
        }

        if (expectation is not null && pictures != expectation.StillPictures)
        {
            issues.Add(new VerificationIssue(
                "ASVS_PICTURE_COUNT_MISMATCH",
                $"静图总数 {pictures} != 规划数 {expectation.StillPictures}"));
        }
        return pictures;
    }

    private static ushort ReadU16(ReadOnlySpan<byte> data, int offset) =>
        BinaryPrimitives.ReadUInt16BigEndian(data[offset..]);

    private static uint ReadU32(ReadOnlySpan<byte> data, int offset) =>
        BinaryPrimitives.ReadUInt32BigEndian(data[offset..]);
}
