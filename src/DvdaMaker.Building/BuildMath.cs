using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public static class BuildMath
{
    public static long EstimateAobBytes(long mlpBytes) =>
        DvdaMaker.Processes.RustBridge.Mode != "managed"
            ? DvdaMaker.Processes.RustBridge.Run<long>("math.estimate_aob", mlpBytes,
                () => checked((long)Math.Ceiling(mlpBytes * ConfigDefaults.AobOverhead)))
            : checked((long)Math.Ceiling(mlpBytes * ConfigDefaults.AobOverhead));

    public static long DiscContentLimit(long discBytes) =>
        DvdaMaker.Processes.RustBridge.Mode != "managed"
            ? DvdaMaker.Processes.RustBridge.Run<long>("math.content_limit", discBytes,
                () => Math.Max(0, discBytes - ConfigDefaults.IsoSafety))
            : Math.Max(0, discBytes - ConfigDefaults.IsoSafety);
}
