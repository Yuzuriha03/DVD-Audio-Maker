namespace DvdaMaker.Building;

public sealed record BuildTrack
{
    public required string Date { get; init; }
    public required string Track { get; init; }
    public required string Title { get; init; }
    public required string Album { get; init; }
    public required int SampleRate { get; init; }
    public required int Bits { get; init; }
    public required string SourcePath { get; init; }
    public required string ManifestName { get; init; }
    public required double Duration { get; init; }
    public int? ResampleTo { get; init; }
    public required long SourceSize { get; init; }
    public required string MlpPath { get; init; }
    public required long MlpSize { get; init; }
    public int? Channels { get; init; }
    public int? SourceSampleRate { get; init; }
    public int? SourceBits { get; init; }
    public string MlpSource { get; init; } = "ffmpeg";
    public bool ExternalResampled { get; init; }
    public bool ExternalRebitded { get; init; }
    public bool ParametersChanged { get; init; }
}

public sealed record AlbumPlan(
    string Name,
    IReadOnlyList<BuildTrack> Tracks)
{
    public long MlpBytes => Tracks.Sum(track => track.MlpSize);
}

public sealed record AudioGroupPlan(
    int Number,
    int SampleRate,
    int Bits,
    IReadOnlyList<BuildTrack> Tracks);

public sealed record DiscPlan(
    int Number,
    IReadOnlyList<AlbumPlan> Albums,
    IReadOnlyList<AudioGroupPlan> Groups)
{
    public IReadOnlyList<BuildTrack> Tracks => Albums.SelectMany(album => album.Tracks).ToArray();
    public long MlpBytes => Albums.Sum(album => album.MlpBytes);
    public long EstimatedAobBytes => BuildMath.EstimateAobBytes(MlpBytes);
}

public enum BuildDiagnosticSeverity
{
    Information,
    Warning,
    Error,
}

public sealed record BuildDiagnostic(
    BuildDiagnosticSeverity Severity,
    string Code,
    string Message);

public sealed record BuildPlan(
    IReadOnlyList<BuildTrack> Tracks,
    IReadOnlyList<DiscPlan> Discs,
    IReadOnlyList<BuildDiagnostic> Diagnostics,
    long DiscContentLimitBytes)
{
    public bool HasErrors => Diagnostics.Any(item => item.Severity == BuildDiagnosticSeverity.Error);
}

public sealed record MlpAcquisitionResult(
    IReadOnlyList<BuildTrack> Tracks,
    int CacheHits,
    int CacheRebuilt,
    IReadOnlyList<BuildDiagnostic> Diagnostics)
{
    public bool HasErrors => Diagnostics.Any(item => item.Severity == BuildDiagnosticSeverity.Error);
}

public sealed record BuildPipelineResult(
    BuildPlan Plan,
    MlpAcquisitionResult Acquisition,
    string IndexPath,
    IReadOnlyList<DiscBuildResult> DiscResults);

public sealed record DiscBuildResult(
    int DiscNumber,
    string IntermediateIsoPath,
    string PublishedIsoPath,
    long IsoSize,
    IReadOnlyList<BuildDiagnostic> Diagnostics)
{
    public bool Succeeded => Diagnostics.All(item => item.Severity != BuildDiagnosticSeverity.Error);
}
