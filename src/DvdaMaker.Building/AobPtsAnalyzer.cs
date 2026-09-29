using DvdaMaker.Formats.Mpeg;

namespace DvdaMaker.Building;

public sealed record AobPtsAnalysis(
    string Path,
    int SectorCount,
    int PtsSectorCount,
    long? FirstPts,
    long? LastPts,
    long MinimumStep,
    long MaximumStep,
    int NegativeSteps,
    int ZeroSteps,
    double MedianStep,
    int AbnormalSteps,
    double AbnormalRatio,
    IReadOnlyList<long> Steps,
    IReadOnlyList<VerificationIssue> Issues)
{
    public bool Succeeded => Issues.Count == 0;
    public double DurationSeconds => FirstPts is null || LastPts is null
        ? 0
        : (LastPts.Value - FirstPts.Value) / 90_000d;
}

public static class AobPtsAnalyzer
{
    private const int SectorSize = 2048;
    private static ReadOnlySpan<byte> PackHeader => [0, 0, 1, 0xBA];
    private static ReadOnlySpan<byte> PrivateStreamHeader => [0, 0, 1, 0xBD];

    public static AobPtsAnalysis Analyze(string path, int? maximumSectors = null) =>
        Analyze(File.ReadAllBytes(path), path, maximumSectors);

    public static AobPtsAnalysis Analyze(
        ReadOnlySpan<byte> data,
        string path = "<memory>",
        int? maximumSectors = null)
    {
        var sectorCount = data.Length / SectorSize;
        if (maximumSectors is > 0)
        {
            sectorCount = Math.Min(sectorCount, maximumSectors.Value);
        }

        var values = new List<long>();
        for (var sectorIndex = 0; sectorIndex < sectorCount; sectorIndex++)
        {
            var sector = data.Slice(sectorIndex * SectorSize, SectorSize);
            if (!sector[..4].SequenceEqual(PackHeader)) continue;
            var relative = sector.Slice(4, Math.Min(60, sector.Length - 4))
                .IndexOf(PrivateStreamHeader);
            if (relative < 0) continue;
            var marker = relative + 4;
            if (marker + 14 > sector.Length || (sector[marker + 7] & 0x80) == 0) continue;
            values.Add(PesTimestampParser.ParsePts(sector.Slice(marker + 9, 5)));
        }

        var issues = new List<VerificationIssue>();
        if (values.Count < 3)
        {
            issues.Add(new VerificationIssue(
                "PTS_TOO_FEW", $"有效 PTS 只有 {values.Count} 个，时间轴缺失"));
            return new AobPtsAnalysis(
                path, sectorCount, values.Count,
                values.Count == 0 ? null : values[0],
                values.Count == 0 ? null : values[^1],
                0, 0, 0, 0, 0, 0, 0, [], issues);
        }

        var steps = values.Zip(values.Skip(1), (left, right) => right - left).ToArray();
        var ordered = steps.Order().ToArray();
        var median = ordered.Length % 2 == 1
            ? ordered[ordered.Length / 2]
            : (ordered[ordered.Length / 2 - 1] + ordered[ordered.Length / 2]) / 2d;
        var abnormal = steps.Count(step => step <= 0 || step > median * 20);
        var ratio = abnormal * 100d / steps.Length;
        if (values[^1] == values[0])
        {
            issues.Add(new VerificationIssue(
                "PTS_NOT_ADVANCING", "首末 PTS 相同，时间轴没有推进"));
        }
        if (ratio > 1d)
        {
            issues.Add(new VerificationIssue(
                "PTS_ABNORMAL_RATIO",
                $"异常步长 {abnormal}/{steps.Length}，占比 {ratio:F3}% > 1%"));
        }

        return new AobPtsAnalysis(
            path,
            sectorCount,
            values.Count,
            values[0],
            values[^1],
            steps.Min(),
            steps.Max(),
            steps.Count(step => step < 0),
            steps.Count(step => step == 0),
            median,
            abnormal,
            ratio,
            steps,
            issues);
    }
}
