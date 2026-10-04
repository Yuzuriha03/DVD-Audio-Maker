using DvdaMaker.Configuration;
using DvdaMaker.SurcodeTool;

namespace DvdaMaker.Building;

public sealed class BuildPlanService(DvdaOptions options)
{
    public BuildPlan Create()
    {
        if (!File.Exists(options.ManifestPath))
        {
            throw new FileNotFoundException("找不到 manifest.json，请先运行 prepare。", options.ManifestPath);
        }
        var tracks = ApplyDiagnosticAlbumLimit(
            new ManifestBuildReader().Read(options.ManifestPath, options.MlpDirectory),
            options.DiagnosticAlbumLimit);
        if (options.MlpSource == "lpcm")
            tracks = tracks.Select(track =>
            {
                var path = LpcmProvider.CachePath(options.BuildDirectory, track.SourcePath);
                var wave = File.Exists(path) ? SurcodePcmWav.ReadLayout(path) : null;
                if (wave is not null) LpcmProvider.ValidateLayout(wave);
                var matches = wave is not null && wave.SampleRate == options.MlpSurcodeSampleRate && wave.ValidBits == options.MlpSurcodeBits;
                return track with { MlpSource = "lpcm", MlpPath = path, MlpSize = matches ? new FileInfo(path).Length : 0,
                    SampleRate = options.MlpSurcodeSampleRate, Bits = options.MlpSurcodeBits,
                    Channels = wave?.Channels, ChannelMask = wave?.ChannelMask };
            }).ToArray();
        return new DiscPlanner().Plan(
            tracks,
            options.DiscBytes,
            options.MaxDiscs,
            options.GroupTrackLimit);
    }

    internal static IReadOnlyList<BuildTrack> ApplyDiagnosticAlbumLimit(
        IReadOnlyList<BuildTrack> tracks,
        int? albumLimit)
    {
        if (albumLimit is null) return tracks;
        var retainedAlbums = DiscPlanner.AggregateAlbums(tracks)
            .Take(albumLimit.Value)
            .Select(album => album.Name)
            .ToHashSet(StringComparer.Ordinal);
        return tracks.Where(track => retainedAlbums.Contains(track.Album)).ToArray();
    }

    public static void Print(BuildPlan plan, DvdaOptions options, TextWriter writer)
    {
        writer.WriteLine($"总曲目 {plan.Tracks.Count} 首，共 {DiscPlanner.AggregateAlbums(plan.Tracks).Count} 张专辑");
        writer.WriteLine($"=== 分盘结果 ({plan.Discs.Count} 张) ===");
        foreach (var disc in plan.Discs)
        {
            writer.WriteLine(
                $"  第 {disc.Number} 盘: {disc.Tracks.Count} 首, 音频 {disc.MlpBytes / 1024d / 1024 / 1024:F2} GiB " +
                $"-> 估AOB {disc.EstimatedAobBytes / 1024d / 1024 / 1024:F2} GiB  卷标 \"{options.VolumeId(disc.Number)}\"");
            foreach (var group in disc.Groups)
            {
                writer.WriteLine($"    第 {group.Number} 组 {group.SampleRate}/{group.Bits}: {group.Tracks.Count} 首");
            }
        }
        foreach (var diagnostic in plan.Diagnostics)
        {
            writer.WriteLine($"[{diagnostic.Severity}] {diagnostic.Code}: {diagnostic.Message}");
        }
    }
}
