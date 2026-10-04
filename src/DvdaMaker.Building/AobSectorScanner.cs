using DvdaMaker.Processes;

namespace DvdaMaker.Building;

internal static class AobSectorScanner
{
    internal const long MissingPts = -1;
    internal const long ShortSector = -2;

    internal static long[] Scan(ReadOnlyMemory<byte> data, int stride, bool requirePack) =>
        RustBridge.ScanAob(data, stride, requirePack, () => ScanManaged(data.Span, stride, requirePack));

    internal static long Audit(ReadOnlyMemory<byte> data) => data.IsEmpty
        ? ShortSector : Scan(data, data.Length, false)[0];

    internal static RustBridge.AobAuditResult Observe(ReadOnlyMemory<byte> data, RustBridge.AobAuditState state) =>
        RustBridge.ObserveAob(data, state, () => ObserveManaged(data, state));

    private static RustBridge.AobAuditResult ObserveManaged(ReadOnlyMemory<byte> data, RustBridge.AobAuditState state)
    {
        if (state.MissingSector >= 0) return new(state, -1);
        var index = state.SectorCount++;
        var current = ParseManaged(data.Span, false);
        var drop = -1;
        if (current == MissingPts) state.MissingSector = index;
        else if (current >= 0)
        {
            if (state.Previous >= 0 && current < state.Previous) drop = index;
            state.Previous = current;
        }
        return new(state, drop);
    }

    internal static long[] ScanManaged(ReadOnlySpan<byte> data, int stride, bool requirePack)
    {
        var result = new long[data.Length / stride];
        for (var index = 0; index < result.Length; index++)
            result[index] = ParseManaged(data.Slice(index * stride, stride), requirePack);
        return result;
    }

    // Independent reference: do not call the native parser from compare mode.
    private static long ParseManaged(ReadOnlySpan<byte> sector, bool requirePack)
    {
        if (sector.Length < 64) return ShortSector;
        ReadOnlySpan<byte> pack = [0, 0, 1, 0xBA];
        ReadOnlySpan<byte> privateStream = [0, 0, 1, 0xBD];
        if (requirePack && !sector[..4].SequenceEqual(pack)) return MissingPts;
        var relative = sector[4..64].IndexOf(privateStream);
        if (relative < 0) return MissingPts;
        var marker = relative + 4;
        if (marker + 14 > sector.Length || (sector[marker + 7] & 0x80) == 0) return MissingPts;
        var pts = sector.Slice(marker + 9, 5);
        return (((long)pts[0] >> 1) & 7) << 30
            | (((long)pts[1] << 8 | pts[2]) >> 1) << 15
            | ((long)pts[3] << 8 | pts[4]) >> 1;
    }
}
