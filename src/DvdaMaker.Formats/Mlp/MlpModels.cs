namespace DvdaMaker.Formats.Mlp;

public sealed record MlpAccessUnit(int Offset, int Length, bool HasMajorSync);

public sealed record MlpMajorSyncError(int Offset, string Message);

public sealed record MlpAuParityError(int Offset, int Expected, int Actual);

public sealed record MlpInspection(
    int Size,
    int AccessUnitCount,
    int MajorSyncCount,
    double MajorSyncInterval,
    IReadOnlyList<MlpMajorSyncError> MajorSyncErrors,
    IReadOnlyList<MlpAuParityError> AccessUnitParityErrors,
    IReadOnlyList<int> SubstreamErrors,
    bool HasEndOfStream,
    int? PeakBitrateRaw,
    int? ExtendedSubstreamInfo,
    int? SampleRate)
{
    public bool IsValid =>
        AccessUnitCount > 0 &&
        MajorSyncCount > 0 &&
        SampleRate is > 0 &&
        PeakBitrateRaw == MlpStreamAligner.PeakBitrateRaw(SampleRate.Value) &&
        ExtendedSubstreamInfo == 1 &&
        HasEndOfStream &&
        MajorSyncErrors.Count == 0 &&
        AccessUnitParityErrors.Count == 0 &&
        SubstreamErrors.Count == 0;
}

public sealed record MlpAlignmentChanges(
    int PeakBitrateChanges,
    int ExtendedSubstreamInfoChanges,
    int ChecksumChanges,
    bool AddedEndOfStream,
    ushort? PreviousAccessUnitHeader,
    ushort? NewAccessUnitHeader);

public sealed record MlpAlignmentResult(byte[] Data, MlpAlignmentChanges Changes);
