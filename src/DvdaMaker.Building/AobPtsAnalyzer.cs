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
    private sealed record FileScan(int SectorCount, IReadOnlyList<long> Values);
    private sealed record FileScanOutcome(FileScan? Scan, int? ErrorCode);

    public static AobPtsAnalysis Analyze(string path, int? maximumSectors = null)
    {
        var result = RustBridge.Run<FileScanOutcome>("aob.scan_file", new { Path = path, MaximumSectors = maximumSectors },
            () => ScanFileManaged(path, maximumSectors));
        if (result.Scan is { } scan) return BuildAnalysis(path, scan.SectorCount, scan.Values);
        var code = result.ErrorCode ?? 87;
        var message = new System.ComponentModel.Win32Exception(code).Message + ": " + path;
        throw code switch
        {
            2 => new FileNotFoundException(message, path),
            3 => new DirectoryNotFoundException(message),
            5 => new UnauthorizedAccessException(message),
            _ => new IOException(message, unchecked((int)0x80070000) | code),
        };
    }

    private static FileScanOutcome ScanFileManaged(string path, int? maximumSectors)
    {
        try
        {
            using var input = File.OpenRead(path);
            var buffer = new byte[128 * 1024];
            var values = new List<long>();
            var count = 0;
            var limit = maximumSectors is > 0 ? maximumSectors.Value : int.MaxValue;
            while (count < limit)
            {
                var requested = (int)Math.Min(buffer.Length, (long)(limit - count) * SectorSize);
                var read = input.ReadAtLeast(buffer.AsSpan(0, requested), requested, throwOnEndOfStream: false);
                var complete = read / SectorSize;
                values.AddRange(AobSectorScanner.ScanManaged(buffer.AsSpan(0, complete * SectorSize), SectorSize, true)
                    .Where(pts => pts >= 0));
                count += complete;
                if (read < requested) break;
            }
            return new(new FileScan(count, values), null);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return new(null, error.HResult & 0xffff);
        }
    }

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
            var chunkSectors = chunkMemory.Length / SectorSize;
            if (maximumSectors is > 0) chunkSectors = Math.Min(chunkSectors, maximumSectors.Value - sectorCount);
            var timestamps = AobSectorScanner.Scan(chunkMemory[..(chunkSectors * SectorSize)], SectorSize, true);
            for (var index = 0; index < chunkSectors; index++)
            {
                sectorCount++;
                observeSector?.Invoke(chunkMemory.Slice(index * SectorSize, SectorSize));
                if (timestamps[index] >= 0) values.Add(timestamps[index]);
            }
            if (maximumSectors is > 0 && sectorCount >= maximumSectors.Value) break;
        }

        return BuildAnalysis(path, sectorCount, values);
    }

    private static AobPtsAnalysis BuildAnalysis(string path, int sectorCount, IReadOnlyList<long> values)
    {
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
