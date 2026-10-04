using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public sealed record VolumeSpaceRequirement(
    string Root,
    string Purpose,
    long RequiredBytes,
    long? AvailableBytes)
{
    public bool? IsSufficient =>
        AvailableBytes is null ? null : AvailableBytes.Value >= RequiredBytes;
}

/// <summary>
/// 按卷估算构建过程的磁盘占用并给出预警。
/// 数字是保守估计（同卷的多个用途会被相加），只用于提前发现问题，
/// 不是精确的写入量预测；最终依据仍是 <see cref="DiscBuildExecutor"/> 的实际产物大小。
/// </summary>
public static class DiskSpacePlanner
{
    /// <summary>菜单素材与 author 输出的固定余量。</summary>
    public const long MenuSafetyBytes = 32L * 1024 * 1024;

    public static IReadOnlyList<VolumeSpaceRequirement> Estimate(
        DvdaOptions options,
        IReadOnlyList<BuildTrack> tracks,
        IReadOnlyList<DiscPlan>? discs,
        Func<string, long?>? freeSpace = null)
    {
        var available = freeSpace ?? AvailableBytes;
        var items = new List<(string Root, string Purpose, long Bytes)>();

        if (options.MlpSource == "surcode-batch")
        {
            // MLP 尚未生成时，用源文件大小作为编码输出的粗略下界。
            var missingMlpBytes = tracks
                .Where(track => track.MlpSize <= 0)
                .Sum(track => Math.Max(0, track.SourceSize));
            if (missingMlpBytes > 0)
            {
                items.Add((Root(options.MlpExternalDirectory), "MLP 编码输出", missingMlpBytes));
            }
        }

        if (options.MlpSource == "lpcm" && discs is null)
        {
            var pcmBytes = tracks.Sum(track => checked((long)Math.Ceiling(Math.Max(0, track.Duration) *
                options.MlpSurcodeSampleRate * (options.MlpSurcodeBits / 8) * (track.Channels ?? 6)) + 128));
            items.Add((Root(options.BuildDirectory), "LPCM 音频缓存与转换临时文件", checked(pcmBytes * 3)));
        }

        if (discs is { Count: > 0 })
        {
            var isoBytes = discs.Sum(disc => disc.EstimatedAobBytes);
            var intermediate = isoBytes * (options.KeepIntermediate ? 2 : 1) +
                (options.MenuEnabled ? MenuSafetyBytes : 0);
            items.Add((Root(options.OutputRoot), "author 中间产物与暂存 ISO", intermediate));
            items.Add((Root(options.FinalDirectory), "成品 ISO 集合", isoBytes));
        }

        return items
            .GroupBy(item => item.Root, StringComparer.OrdinalIgnoreCase)
            .OrderBy(group => group.Key, StringComparer.OrdinalIgnoreCase)
            .Select(group => new VolumeSpaceRequirement(
                group.Key,
                string.Join(" + ", group.Select(item => item.Purpose).Distinct(StringComparer.Ordinal)),
                group.Sum(item => item.Bytes),
                TryAvailable(available, group.Key)))
            .ToArray();
    }

    public static IReadOnlyList<BuildDiagnostic> Evaluate(
        IReadOnlyList<VolumeSpaceRequirement> requirements)
    {
        var diagnostics = new List<BuildDiagnostic>();
        foreach (var requirement in requirements)
        {
            if (requirement.IsSufficient is not false)
            {
                continue;
            }
            var available = requirement.AvailableBytes!.Value;
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "DISK_SPACE_LOW",
                $"{requirement.Root} 上“{requirement.Purpose}”预计需要 " +
                $"{requirement.RequiredBytes:N0} 字节，但仅剩 {available:N0} 字节" +
                $"（约缺 {requirement.RequiredBytes - available:N0} 字节）。估算为保守值，仅供参考。"));
        }
        return diagnostics;
    }

    public static string Describe(IReadOnlyList<VolumeSpaceRequirement> requirements)
    {
        if (requirements.Count == 0)
        {
            return "无需额外估算的写入阶段。";
        }
        return string.Join("；", requirements.Select(requirement =>
            $"{requirement.Root} “{requirement.Purpose}” 需要 {requirement.RequiredBytes:N0} B" +
            (requirement.AvailableBytes is null
                ? "，可用未知"
                : $"，可用 {requirement.AvailableBytes.Value:N0} B")));
    }

    public static long? AvailableBytes(string root)
    {
        try
        {
            if (string.IsNullOrWhiteSpace(root) || root == "?")
            {
                return null;
            }
            var drive = new DriveInfo(root);
            return drive.IsReady ? drive.AvailableFreeSpace : null;
        }
        catch (Exception exception) when (
            exception is ArgumentException or IOException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    private static long? TryAvailable(Func<string, long?> freeSpace, string root)
    {
        try
        {
            return freeSpace(root);
        }
        catch (Exception exception) when (
            exception is ArgumentException or IOException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    private static string Root(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return "?";
        }
        try
        {
            return Path.GetPathRoot(Path.GetFullPath(path)) ?? "?";
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException)
        {
            return "?";
        }
    }
}
