using DvdaMaker.Formats.Mpeg;
using DvdaMaker.Processes;

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
        => AnalyzeChunks([data.ToArray()], path, maximumSectors);

    public static AobPtsAnalysis AnalyzeChunks(
        IEnumerable<ReadOnlyMemory<byte>> chunks,
        string path = "<memory>",
        int? maximumSectors = null)
        => AnalyzeChunks(chunks, path, maximumSectors, null);

    internal static AobPtsAnalysis AnalyzeChunks(
        IEnumerable<ReadOnlyMemory<byte>> chunks,
        string path,
        int? maximumSectors,
        Action<ReadOnlyMemory<byte>>? observeSector)
    {
        var values = new List<long>();
        var sectorCount = 0;
        foreach (var chunkMemory in chunks)
        {
            var chunk = chunkMemory.Span;
            var chunkSectors = chunk.Length / SectorSize;
            for (var index = 0; index < chunkSectors; index++)
            {
                if (maximumSectors is > 0 && sectorCount >= maximumSectors.Value) break;
                var sector = chunk.Slice(index * SectorSize, SectorSize);
                sectorCount++;
                observeSector?.Invoke(chunkMemory.Slice(index * SectorSize, SectorSize));
                if (!sector[..4].SequenceEqual(PackHeader)) continue;
                var relative = sector.Slice(4, Math.Min(60, sector.Length - 4))
                    .IndexOf(PrivateStreamHeader);
                if (relative < 0) continue;
                var marker = relative + 4;
                if (marker + 14 > sector.Length || (sector[marker + 7] & 0x80) == 0) continue;
                values.Add(PesTimestampParser.ParsePts(sector.Slice(marker + 9, 5)));
            }
            if (maximumSectors is > 0 && sectorCount >= maximumSectors.Value) break;
        }

        var statistics = ComputeStatistics(values);
        var issues = BuildIssues(statistics, values.Count);
        return new AobPtsAnalysis(
            path,
            sectorCount,
            values.Count,
            statistics.FirstPts,
            statistics.LastPts,
            statistics.MinimumStep,
            statistics.MaximumStep,
            statistics.NegativeSteps,
            statistics.ZeroSteps,
            statistics.MedianStep,
            statistics.AbnormalSteps,
            statistics.AbnormalRatio,
            statistics.Steps,
            issues);
    }

    private sealed record PtsStatistics(
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
        IReadOnlyList<string> IssueCodes);

    private static PtsStatistics ComputeStatistics(IReadOnlyList<long> values) =>
        RustBridge.Run<PtsStatistics>("aob.pts_statistics", new { Values = values },
            () => ComputeStatisticsManaged(values));

    private static PtsStatistics ComputeStatisticsManaged(IReadOnlyList<long> values)
    {
        var first = values.Count == 0 ? (long?)null : values[0];
        var last = values.Count == 0 ? (long?)null : values[^1];
        if (values.Count < 3)
        {
            return new PtsStatistics(first, last, 0, 0, 0, 0, 0, 0, 0, [], ["PTS_TOO_FEW"]);
        }

        var steps = values.Zip(values.Skip(1), (left, right) => right - left).ToArray();
        var ordered = steps.Order().ToArray();
        var median = ordered.Length % 2 == 1
            ? ordered[ordered.Length / 2]
            : (ordered[ordered.Length / 2 - 1] + ordered[ordered.Length / 2]) / 2d;
        var abnormal = steps.Count(step => step <= 0 || step > median * 20);
        var ratio = abnormal * 100d / steps.Length;
        var codes = new List<string>();
        if (last == first) codes.Add("PTS_NOT_ADVANCING");
        if (ratio > 1d) codes.Add("PTS_ABNORMAL_RATIO");
        return new PtsStatistics(
            first, last, steps.Min(), steps.Max(), steps.Count(step => step < 0),
            steps.Count(step => step == 0), median, abnormal, ratio, steps, codes);
    }

    private static List<VerificationIssue> BuildIssues(PtsStatistics statistics, int valueCount)
    {
        var issues = new List<VerificationIssue>();
        foreach (var code in statistics.IssueCodes)
        {
            issues.Add(code switch
            {
                "PTS_TOO_FEW" => new VerificationIssue(code, $"有效 PTS 只有 {valueCount} 个，时间轴缺失"),
                "PTS_NOT_ADVANCING" => new VerificationIssue(code, "首末 PTS 相同，时间轴没有推进"),
                "PTS_ABNORMAL_RATIO" => new VerificationIssue(code, $"异常步长 {statistics.AbnormalSteps}/{statistics.Steps.Count}，占比 {statistics.AbnormalRatio:F3}% > 1%"),
                _ => new VerificationIssue(code, code),
            });
        }
        return issues;
    }
}
