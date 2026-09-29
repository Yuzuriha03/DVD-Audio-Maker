using System.Text.Json.Serialization;

namespace DvdaMaker.Preparation;

public sealed record AudioTrackMetadata
{
    public required string Path { get; init; }
    public int SampleRate { get; set; }
    public int Bits { get; set; }
    public int Channels { get; init; }
    public string Date { get; init; } = string.Empty;
    public string Track { get; init; } = string.Empty;
    public string Title { get; init; } = string.Empty;
    public string Album { get; init; } = string.Empty;
    public double Duration { get; init; }
    public int SourceSampleRate { get; set; }
    public int SourceBits { get; set; }
    public int? ResampleTo { get; set; }
}

public sealed record AppliedAudioRepair(
    string OriginalPath,
    string RepairedPath,
    IReadOnlyList<AlacFramePatch> Patches);

public sealed record DecodeCheckResult(
    long? Samples,
    int ErrorCount,
    IReadOnlyList<string> ErrorLines,
    int ExitCode);

public sealed record ValidationIssue(
    string Level,
    string Title,
    string Path,
    string Reason,
    IReadOnlyList<string> Detail);

public sealed record ManifestTrack
{
    [JsonPropertyName("n")]
    public required int Number { get; init; }

    [JsonPropertyName("src")]
    public required string Source { get; init; }

    [JsonPropertyName("name")]
    public required string Name { get; init; }

    [JsonPropertyName("title")]
    public required string Title { get; init; }

    [JsonPropertyName("date")]
    public required string Date { get; init; }

    [JsonPropertyName("track")]
    public required string Track { get; init; }

    [JsonPropertyName("album")]
    public required string Album { get; init; }

    [JsonPropertyName("dur")]
    public required double Duration { get; init; }

    [JsonPropertyName("resample_to")]
    public int? ResampleTo { get; init; }

    [JsonPropertyName("repaired")]
    public int Repaired { get; init; }

    [JsonPropertyName("repair_detail")]
    public IReadOnlyList<string>? RepairDetail { get; init; }

    [JsonPropertyName("orig_src")]
    public string? OriginalSource { get; init; }
}

public sealed record ManifestGroup
{
    [JsonPropertyName("sr")]
    public required int SampleRate { get; init; }

    [JsonPropertyName("bits")]
    public required int Bits { get; init; }

    [JsonPropertyName("count")]
    public required int Count { get; init; }

    [JsonPropertyName("files")]
    public required IReadOnlyList<ManifestTrack> Files { get; init; }
}

public sealed record PreparationResult(
    IReadOnlyDictionary<string, ManifestGroup> Manifest,
    IReadOnlyList<ValidationIssue> Issues,
    int CheckedTracks,
    IReadOnlyList<AppliedAudioRepair> Repairs)
{
    public int FailureCount => Issues.Count(issue => issue.Level == "FAIL");
    public int WarningCount => Issues.Count(issue => issue.Level == "WARN");
}
