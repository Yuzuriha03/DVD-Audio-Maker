using System.Text;

namespace DvdaMaker.Processes;

public sealed record ProcessRequest
{
    public required string FileName { get; init; }

    public IReadOnlyList<string> Arguments { get; init; } = [];

    public string? WorkingDirectory { get; init; }

    public IReadOnlyDictionary<string, string?> Environment { get; init; } =
        new Dictionary<string, string?>(StringComparer.Ordinal);

    public Encoding OutputEncoding { get; init; } = new UTF8Encoding(false, false);

    public Encoding ErrorEncoding { get; init; } = new UTF8Encoding(false, false);

    public bool CaptureOutput { get; init; } = true;

    public bool CaptureError { get; init; } = true;

    public bool ThrowOnNonZeroExitCode { get; init; }

    public TimeSpan? Timeout { get; init; }

    public Action<string>? OnOutputLine { get; init; }

    public Action<string>? OnErrorLine { get; init; }
}
