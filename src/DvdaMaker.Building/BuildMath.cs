using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public static class BuildMath
{
    public static long EstimateAobBytes(long mlpBytes) =>
        checked((long)Math.Ceiling(mlpBytes * ConfigDefaults.AobOverhead));

    public static long DiscContentLimit(long discBytes) =>
        Math.Max(0, discBytes - ConfigDefaults.IsoSafety);
}
