namespace DvdaMaker.SurcodeTool;

public sealed record SurcodeEncodingJob
{
    public required string SurcodeExecutable { get; init; }

    public required string Eac3toExecutable { get; init; }

    public required string TemporaryDirectory { get; init; }

    public required string OutputDirectory { get; init; }

    public required int SampleRate { get; init; }

    public required int Bits { get; init; }

    public IReadOnlyList<SurcodeEncodingTrack> Tracks { get; init; } = [];
}

public sealed record SurcodeEncodingTrack
{
    public required string SourcePath { get; init; }

    public required string WorkName { get; init; }

    public required string DisplayName { get; init; }

    public required double DurationSeconds { get; init; }

    public required int SourceSampleRate { get; init; }

    public required int SourceBits { get; init; }
}
