using System.Text.RegularExpressions;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

public sealed partial class DecodeValidator(ProcessRunner processRunner, string ffmpeg)
{
    private sealed record ErrorScan(int Count, IReadOnlyList<string> Lines);

    public static readonly string[] ErrorKeywords =
    [
        "error submitting packet to decoder",
        "invalid element",
        "error while decoding",
        "invalid data found",
        "channel element",
        "crc mismatch",
        "corrupt",
        "not implemented",
        "not yet implemented",
    ];

    public async Task<DecodeCheckResult> CheckAsync(
        string path,
        int? resampleTo,
        CancellationToken cancellationToken = default)
    {
        var filters = new List<string>();
        if (resampleTo is not null)
        {
            filters.Add($"aresample={resampleTo}:resampler=soxr");
        }
        filters.Add("astats=metadata=1");

        var result = await processRunner.RunAsync(new ProcessRequest
        {
            FileName = ffmpeg,
            Arguments =
            [
                "-hide_banner", "-nostdin", "-v", "info", "-y",
                "-i", path,
                "-af", string.Join(',', filters),
                "-f", "null", "-",
            ],
        }, cancellationToken).ConfigureAwait(false);

        var scan = RustBridge.Run<ErrorScan>("decode.scan", result.StandardError,
            () =>
            {
                var managed = ScanErrorsManaged(result.StandardError);
                return new ErrorScan(managed.Count, managed.Lines);
            });
        var errorCount = scan.Count;
        var errorLines = scan.Lines;
        var match = SamplesRegex().Match(result.StandardError);
        long? samples = match.Success && long.TryParse(match.Groups[1].Value, out var value)
            ? value
            : null;
        if (!result.Succeeded && errorCount == 0)
        {
            errorCount = 1;
            errorLines = [$"ffmpeg 退出码 {result.ExitCode}"];
        }
        return new DecodeCheckResult(samples, errorCount, errorLines, result.ExitCode);
    }

    public static (int Count, IReadOnlyList<string> Lines) ScanErrors(string stderr)
    {
        var scan = RustBridge.Run<ErrorScan>("decode.scan", stderr,
            () =>
            {
                var managed = ScanErrorsManaged(stderr);
                return new ErrorScan(managed.Count, managed.Lines);
            });
        return (scan.Count, scan.Lines);
    }

    private static (int Count, IReadOnlyList<string> Lines) ScanErrorsManaged(string stderr)
    {
        var count = 0;
        var examples = new List<string>(3);
        foreach (var line in stderr.Split('\n'))
        {
            if (!ErrorKeywords.Any(keyword =>
                    line.Contains(keyword, StringComparison.OrdinalIgnoreCase)))
            {
                continue;
            }
            count++;
            if (examples.Count < 3)
            {
                examples.Add(line.Trim());
            }
        }
        return (count, examples);
    }

    [GeneratedRegex(@"Number of samples:\s*(\d+)")]
    private static partial Regex SamplesRegex();
}
