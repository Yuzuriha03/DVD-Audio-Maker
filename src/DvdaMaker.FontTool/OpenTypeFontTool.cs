using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;

namespace DvdaMaker.FontTool;

public sealed record ExtractedFontFace(
    int SourceFaceIndex,
    string FamilyName,
    string PostScriptName,
    string OutputPath);

public sealed record FontFaceInspection(
    string FamilyName,
    string PostScriptName,
    bool HasHan,
    bool HasKana,
    bool HasHangul,
    bool HasLatin,
    bool ChecksumValid);

public static class OpenTypeFontTool
{
    private const uint TrueTypeCollectionTag = 0x74746366;
    private const uint ChecksumMagic = 0xB1B0AFBA;

    private static readonly FaceTarget[] NotoCjkTargets =
    [
        new("Noto Sans CJK SC", "NotoSansCJKsc-Regular.otf"),
        new("Noto Sans CJK JP", "NotoSansCJKjp-Regular.otf"),
        new("Noto Sans CJK KR", "NotoSansCJKkr-Regular.otf"),
    ];

    /// <summary>Shares identical SFNT tables while retaining every regional face and glyph.</summary>
    public static void PackNotoCjkFaces(string sourceDirectory, string destinationPath)
    {
        var sources = NotoCjkTargets.Select(target =>
        {
            var path = Path.Combine(sourceDirectory, target.FileName);
            VerifyFace(path, target.FamilyName);
            var data = File.ReadAllBytes(path);
            return (Data: data, Face: ParseFace(data, 0));
        }).ToArray();
        var directories = new int[sources.Length];
        var length = 12 + sources.Length * 4;
        for (var index = 0; index < sources.Length; index++)
        {
            directories[index] = length;
            length = checked(length + 12 + sources[index].Face.Tables.Count * 16);
        }

        var shared = new Dictionary<string, (int Offset, byte[] Data)>(StringComparer.Ordinal);
        var offsets = new List<int[]>();
        foreach (var source in sources)
        {
            var faceOffsets = new int[source.Face.Tables.Count];
            for (var index = 0; index < source.Face.Tables.Count; index++)
            {
                var table = source.Face.Tables[index];
                var bytes = source.Data.AsSpan(table.Offset, table.Length);
                var key = table.Tag + Convert.ToHexString(SHA256.HashData(bytes));
                if (!shared.TryGetValue(key, out var entry))
                {
                    entry = (length, bytes.ToArray());
                    shared.Add(key, entry);
                    length = checked(length + Align4(bytes.Length));
                }
                else if (!bytes.SequenceEqual(entry.Data))
                    throw new InvalidDataException("字体表哈希冲突，不能合并。");
                faceOffsets[index] = entry.Offset;
            }
            offsets.Add(faceOffsets);
        }

        var output = new byte[length];
        WriteU32(output, 0, TrueTypeCollectionTag);
        WriteU32(output, 4, 0x00010000);
        WriteU32(output, 8, checked((uint)sources.Length));
        for (var index = 0; index < sources.Length; index++)
        {
            var source = sources[index];
            var directory = directories[index];
            WriteU32(output, 12 + index * 4, checked((uint)directory));
            source.Data.AsSpan(0, 12).CopyTo(output.AsSpan(directory));
            for (var tableIndex = 0; tableIndex < source.Face.Tables.Count; tableIndex++)
            {
                var sourceRecord = 12 + tableIndex * 16;
                var record = directory + sourceRecord;
                source.Data.AsSpan(sourceRecord, 16).CopyTo(output.AsSpan(record));
                WriteU32(output, record + 8, checked((uint)offsets[index][tableIndex]));
            }
        }
        foreach (var entry in shared.Values) entry.Data.CopyTo(output, entry.Offset);

        var roundTrip = ReadFaces(output);
        for (var index = 0; index < sources.Length; index++)
            if (!BuildStandaloneFace(sources[index].Data, sources[index].Face).AsSpan()
                    .SequenceEqual(BuildStandaloneFace(output, roundTrip[index])))
                throw new InvalidDataException("合并字体后区域 face 内容不一致。");
        var fullPath = Path.GetFullPath(destinationPath);
        Directory.CreateDirectory(Path.GetDirectoryName(fullPath)!);
        var temporary = fullPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllBytes(temporary, output);
            File.Move(temporary, fullPath, overwrite: true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public static void VerifyNotoCjkCollection(string path)
    {
        var data = File.ReadAllBytes(path);
        var faces = ReadFaces(data);
        if (faces.Count != NotoCjkTargets.Length)
            throw new InvalidDataException("菜单字体集合必须包含 SC、JP、KR 三个 face。");
        for (var index = 0; index < faces.Count; index++)
        {
            var face = faces[index];
            if (face.FamilyName != NotoCjkTargets[index].FamilyName ||
                !new uint[] { 0x6C49, 0x3042, 0xAC00, 0x0041 }.All(code => HasCodePoint(data, face, code)))
                throw new InvalidDataException($"菜单字体集合 face[{index}] 的区域或字符覆盖不正确。");
            _ = BuildStandaloneFace(data, face);
        }
    }

    public static IReadOnlyList<ExtractedFontFace> ExtractNotoCjkFaces(
        string sourcePath,
        string outputDirectory)
    {
        var source = File.ReadAllBytes(sourcePath);
        var faces = ReadFaces(source);
        if (faces.Count < 2)
        {
            throw new InvalidDataException("输入不是包含多个 face 的 TTC 字体集合。");
        }

        Directory.CreateDirectory(outputDirectory);
        var results = new List<ExtractedFontFace>(NotoCjkTargets.Length);
        foreach (var target in NotoCjkTargets)
        {
            var matches = faces
                .Select((face, index) => (Face: face, Index: index))
                .Where(item => string.Equals(
                    item.Face.FamilyName, target.FamilyName, StringComparison.Ordinal))
                .ToArray();
            if (matches.Length != 1)
            {
                throw new InvalidDataException(
                    $"family='{target.FamilyName}' 应恰好出现一次，实际 {matches.Length} 次；" +
                    "可用 family: " + string.Join(", ", faces.Select(face => face.FamilyName)));
            }

            var destination = Path.Combine(outputDirectory, target.FileName);
            WriteFaceAtomically(source, matches[0].Face, destination);
            var inspection = VerifyFace(destination, target.FamilyName);
            results.Add(new ExtractedFontFace(
                matches[0].Index,
                inspection.FamilyName,
                inspection.PostScriptName,
                destination));
        }
        return results;
    }

    public static FontFaceInspection InspectFace(string path)
    {
        var data = File.ReadAllBytes(path);
        if (data.Length >= 4 && ReadU32(data, 0) == TrueTypeCollectionTag)
        {
            throw new InvalidDataException("期望单 face 字体，实际仍是 TTC 字体集合。");
        }

        var face = ParseFace(data, 0);
        return new FontFaceInspection(
            face.FamilyName,
            face.PostScriptName,
            HasCodePoint(data, face, 0x6C49),
            HasCodePoint(data, face, 0x3042),
            HasCodePoint(data, face, 0xAC00),
            HasCodePoint(data, face, 0x0041),
            ComputeChecksum(data) == ChecksumMagic);
    }

    public static FontFaceInspection VerifyFace(string path, string expectedFamily)
    {
        var inspection = InspectFace(path);
        if (!string.Equals(inspection.FamilyName, expectedFamily, StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                $"family 应为 '{expectedFamily}'，实为 '{inspection.FamilyName}'。");
        }
        var missing = new List<string>();
        if (!inspection.HasHan) missing.Add("汉字");
        if (!inspection.HasKana) missing.Add("假名");
        if (!inspection.HasHangul) missing.Add("谚文");
        if (!inspection.HasLatin) missing.Add("拉丁");
        if (missing.Count > 0)
        {
            throw new InvalidDataException("字体缺少字符集: " + string.Join(", ", missing));
        }
        if (!inspection.ChecksumValid)
        {
            throw new InvalidDataException("OpenType 全字体校验和无效。");
        }
        return inspection;
    }

    private static IReadOnlyList<SfntFace> ReadFaces(byte[] data)
    {
        EnsureRange(data, 0, 4, "字体头");
        if (ReadU32(data, 0) != TrueTypeCollectionTag)
        {
            return [ParseFace(data, 0)];
        }

        EnsureRange(data, 0, 12, "TTC 头");
        var count = CheckedInt(ReadU32(data, 8), "TTC face 数");
        if (count <= 0 || count > 256)
        {
            throw new InvalidDataException($"TTC face 数无效: {count}");
        }
        EnsureRange(data, 12, checked(count * 4), "TTC face 偏移表");
        var faces = new List<SfntFace>(count);
        for (var index = 0; index < count; index++)
        {
            faces.Add(ParseFace(data, CheckedInt(ReadU32(data, 12 + index * 4), "face 偏移")));
        }
        return faces;
    }

    private static SfntFace ParseFace(byte[] data, int offset)
    {
        EnsureRange(data, offset, 12, "SFNT 头");
        var scalerType = ReadU32(data, offset);
        if (scalerType is not (0x00010000 or 0x4F54544F or 0x74727565 or 0x74797031))
        {
            throw new InvalidDataException($"不支持的 SFNT scaler type: 0x{scalerType:X8}");
        }
        var tableCount = ReadU16(data, offset + 4);
        if (tableCount == 0 || tableCount > 256)
        {
            throw new InvalidDataException($"SFNT 表数量无效: {tableCount}");
        }
        EnsureRange(data, offset + 12, checked(tableCount * 16), "SFNT 表目录");

        var tables = new List<SfntTable>(tableCount);
        var tags = new HashSet<string>(StringComparer.Ordinal);
        for (var index = 0; index < tableCount; index++)
        {
            var record = offset + 12 + index * 16;
            var tag = Encoding.ASCII.GetString(data, record, 4);
            var tableOffset = CheckedInt(ReadU32(data, record + 8), $"{tag} 偏移");
            var length = CheckedInt(ReadU32(data, record + 12), $"{tag} 长度");
            EnsureRange(data, tableOffset, length, $"{tag} 表");
            if (!tags.Add(tag))
            {
                throw new InvalidDataException($"SFNT 表目录包含重复 tag: {tag}");
            }
            tables.Add(new SfntTable(tag, tableOffset, length));
        }

        var temporary = new SfntFace(scalerType, tables, string.Empty, string.Empty);
        var name = GetRequiredTable(temporary, "name");
        var names = ReadNames(data, name);
        return temporary with
        {
            FamilyName = SelectName(names, 1),
            PostScriptName = SelectName(names, 6),
        };
    }

    private static IReadOnlyList<NameValue> ReadNames(byte[] data, SfntTable table)
    {
        EnsureRange(data, table.Offset, Math.Min(table.Length, 6), "name 表头");
        if (table.Length < 6) throw new InvalidDataException("name 表过短。");
        var count = ReadU16(data, table.Offset + 2);
        var strings = ReadU16(data, table.Offset + 4);
        EnsureRange(data, table.Offset + 6, checked(count * 12), "name 记录");
        var result = new List<NameValue>(count);
        for (var index = 0; index < count; index++)
        {
            var record = table.Offset + 6 + index * 12;
            var platform = ReadU16(data, record);
            var encoding = ReadU16(data, record + 2);
            var language = ReadU16(data, record + 4);
            var nameId = ReadU16(data, record + 6);
            var length = ReadU16(data, record + 8);
            var relativeOffset = ReadU16(data, record + 10);
            var valueOffset = checked(table.Offset + strings + relativeOffset);
            EnsureRange(data, valueOffset, length, "name 字符串");
            string value;
            if (platform is 0 or 3)
            {
                if ((length & 1) != 0) continue;
                value = Encoding.BigEndianUnicode.GetString(data, valueOffset, length);
            }
            else
            {
                value = Encoding.Latin1.GetString(data, valueOffset, length);
            }
            value = value.Trim('\0', ' ');
            if (value.Length > 0)
            {
                result.Add(new NameValue(platform, encoding, language, nameId, value));
            }
        }
        return result;
    }

    private static string SelectName(IReadOnlyList<NameValue> names, int nameId)
    {
        var selected = names
            .Where(item => item.NameId == nameId)
            .OrderByDescending(NameScore)
            .Select(item => item.Value)
            .FirstOrDefault();
        if (selected is null)
        {
            throw new InvalidDataException($"name 表缺少 name ID {nameId}。");
        }
        return selected;
    }

    private static int NameScore(NameValue item) => item.Platform switch
    {
        3 when item.Language == 0x0409 => 400,
        0 => 300,
        3 => 200,
        _ => 100,
    };

    private static bool HasCodePoint(byte[] data, SfntFace face, uint codePoint)
    {
        var cmap = GetRequiredTable(face, "cmap");
        if (cmap.Length < 4) throw new InvalidDataException("cmap 表过短。");
        var count = ReadU16(data, cmap.Offset + 2);
        EnsureRange(data, cmap.Offset + 4, checked(count * 8), "cmap 编码记录");
        var subtables = new List<(int Offset, ushort Format)>();
        for (var index = 0; index < count; index++)
        {
            var record = cmap.Offset + 4 + index * 8;
            var relative = CheckedInt(ReadU32(data, record + 4), "cmap 子表偏移");
            var offset = checked(cmap.Offset + relative);
            EnsureRange(data, offset, 2, "cmap 子表");
            subtables.Add((offset, ReadU16(data, offset)));
        }
        foreach (var subtable in subtables.OrderByDescending(item => item.Format == 12))
        {
            if (subtable.Format == 12 && Format12Contains(data, subtable.Offset, codePoint))
                return true;
            if (subtable.Format == 4 && codePoint <= ushort.MaxValue &&
                Format4Contains(data, subtable.Offset, (ushort)codePoint))
                return true;
        }
        return false;
    }

    private static bool Format12Contains(byte[] data, int offset, uint codePoint)
    {
        EnsureRange(data, offset, 16, "cmap format 12");
        var length = CheckedInt(ReadU32(data, offset + 4), "cmap format 12 长度");
        EnsureRange(data, offset, length, "cmap format 12");
        var groups = CheckedInt(ReadU32(data, offset + 12), "cmap format 12 group 数");
        if (16L + groups * 12L > length) throw new InvalidDataException("cmap format 12 group 表越界。");
        var low = 0;
        var high = groups - 1;
        while (low <= high)
        {
            var middle = low + (high - low) / 2;
            var group = offset + 16 + middle * 12;
            var start = ReadU32(data, group);
            var end = ReadU32(data, group + 4);
            if (codePoint < start) high = middle - 1;
            else if (codePoint > end) low = middle + 1;
            else return ReadU32(data, group + 8) + codePoint - start != 0;
        }
        return false;
    }

    private static bool Format4Contains(byte[] data, int offset, ushort codePoint)
    {
        EnsureRange(data, offset, 14, "cmap format 4");
        var length = ReadU16(data, offset + 2);
        EnsureRange(data, offset, length, "cmap format 4");
        var segmentCount = ReadU16(data, offset + 6) / 2;
        if (segmentCount == 0) return false;
        var endCodes = offset + 14;
        var startCodes = endCodes + segmentCount * 2 + 2;
        var deltas = startCodes + segmentCount * 2;
        var rangeOffsets = deltas + segmentCount * 2;
        if (rangeOffsets + segmentCount * 2 > offset + length)
            throw new InvalidDataException("cmap format 4 segment 表越界。");
        for (var index = 0; index < segmentCount; index++)
        {
            var end = ReadU16(data, endCodes + index * 2);
            if (codePoint > end) continue;
            var start = ReadU16(data, startCodes + index * 2);
            if (codePoint < start) return false;
            var delta = ReadI16(data, deltas + index * 2);
            var rangeOffsetPosition = rangeOffsets + index * 2;
            var rangeOffset = ReadU16(data, rangeOffsetPosition);
            if (rangeOffset == 0)
            {
                return (ushort)(codePoint + delta) != 0;
            }
            var glyphPosition = checked(rangeOffsetPosition + rangeOffset + (codePoint - start) * 2);
            if (glyphPosition + 2 > offset + length) return false;
            var glyph = ReadU16(data, glyphPosition);
            return glyph != 0 && (ushort)(glyph + delta) != 0;
        }
        return false;
    }

    private static void WriteFaceAtomically(byte[] source, SfntFace face, string destination)
    {
        var output = BuildStandaloneFace(source, face);
        var directory = Path.GetDirectoryName(Path.GetFullPath(destination))!;
        Directory.CreateDirectory(directory);
        var temporary = destination + ".tmp-" + Guid.NewGuid().ToString("N");
        try
        {
            File.WriteAllBytes(temporary, output);
            File.Move(temporary, destination, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporary)) File.Delete(temporary);
        }
    }

    private static byte[] BuildStandaloneFace(byte[] source, SfntFace face)
    {
        var tableCount = face.Tables.Count;
        var directoryLength = checked(12 + tableCount * 16);
        var outputLength = directoryLength;
        foreach (var table in face.Tables)
        {
            outputLength = Align4(outputLength);
            outputLength = checked(outputLength + Align4(table.Length));
        }
        var output = new byte[outputLength];
        WriteU32(output, 0, face.ScalerType);
        WriteU16(output, 4, checked((ushort)tableCount));
        var maximumPower = HighestPowerOfTwo(tableCount);
        WriteU16(output, 6, checked((ushort)(maximumPower * 16)));
        WriteU16(output, 8, checked((ushort)Log2(maximumPower)));
        WriteU16(output, 10, checked((ushort)(tableCount * 16 - maximumPower * 16)));

        var outputOffset = directoryLength;
        int? headOffset = null;
        for (var index = 0; index < tableCount; index++)
        {
            var table = face.Tables[index];
            outputOffset = Align4(outputOffset);
            var tableData = source.AsSpan(table.Offset, table.Length).ToArray();
            if (table.Tag == "head")
            {
                if (tableData.Length < 12) throw new InvalidDataException("head 表过短。");
                tableData.AsSpan(8, 4).Clear();
                headOffset = outputOffset;
            }
            tableData.CopyTo(output, outputOffset);
            var record = 12 + index * 16;
            Encoding.ASCII.GetBytes(table.Tag).CopyTo(output, record);
            WriteU32(output, record + 4, ComputeChecksum(tableData));
            WriteU32(output, record + 8, checked((uint)outputOffset));
            WriteU32(output, record + 12, checked((uint)table.Length));
            outputOffset = checked(outputOffset + Align4(table.Length));
        }
        if (headOffset is null) throw new InvalidDataException("字体缺少 head 表。");
        var adjustment = unchecked(ChecksumMagic - ComputeChecksum(output));
        WriteU32(output, headOffset.Value + 8, adjustment);
        if (ComputeChecksum(output) != ChecksumMagic)
            throw new InvalidDataException("无法生成有效的 OpenType checkSumAdjustment。");
        return output;
    }

    private static SfntTable GetRequiredTable(SfntFace face, string tag) =>
        face.Tables.FirstOrDefault(table => table.Tag == tag)
        ?? throw new InvalidDataException($"字体缺少 {tag} 表。");

    private static uint ComputeChecksum(ReadOnlySpan<byte> data)
    {
        uint sum = 0;
        Span<byte> word = stackalloc byte[4];
        for (var offset = 0; offset < data.Length; offset += 4)
        {
            word.Clear();
            var length = Math.Min(4, data.Length - offset);
            data.Slice(offset, length).CopyTo(word);
            sum = unchecked(sum + BinaryPrimitives.ReadUInt32BigEndian(word));
        }
        return sum;
    }

    private static int HighestPowerOfTwo(int value)
    {
        var result = 1;
        while (result <= value / 2) result *= 2;
        return result;
    }

    private static int Log2(int value)
    {
        var result = 0;
        while (value > 1)
        {
            value /= 2;
            result++;
        }
        return result;
    }

    private static int Align4(int value) => checked((value + 3) & ~3);

    private static int CheckedInt(uint value, string label) => value <= int.MaxValue
        ? (int)value
        : throw new InvalidDataException($"{label} 超出支持范围: {value}");

    private static void EnsureRange(byte[] data, int offset, int length, string label)
    {
        if (offset < 0 || length < 0 || offset > data.Length - length)
            throw new InvalidDataException($"{label} 越界: offset={offset}, length={length}");
    }

    private static ushort ReadU16(byte[] data, int offset) =>
        BinaryPrimitives.ReadUInt16BigEndian(data.AsSpan(offset, 2));

    private static short ReadI16(byte[] data, int offset) =>
        BinaryPrimitives.ReadInt16BigEndian(data.AsSpan(offset, 2));

    private static uint ReadU32(byte[] data, int offset) =>
        BinaryPrimitives.ReadUInt32BigEndian(data.AsSpan(offset, 4));

    private static void WriteU16(byte[] data, int offset, ushort value) =>
        BinaryPrimitives.WriteUInt16BigEndian(data.AsSpan(offset, 2), value);

    private static void WriteU32(byte[] data, int offset, uint value) =>
        BinaryPrimitives.WriteUInt32BigEndian(data.AsSpan(offset, 4), value);

    private sealed record FaceTarget(string FamilyName, string FileName);
    private sealed record SfntTable(string Tag, int Offset, int Length);
    private sealed record SfntFace(
        uint ScalerType,
        IReadOnlyList<SfntTable> Tables,
        string FamilyName,
        string PostScriptName);
    private sealed record NameValue(
        int Platform,
        int Encoding,
        int Language,
        int NameId,
        string Value);
}
