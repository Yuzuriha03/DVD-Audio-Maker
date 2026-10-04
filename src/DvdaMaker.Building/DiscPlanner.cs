namespace DvdaMaker.Building;

public sealed class DiscPlanner
{
    public BuildPlan Plan(
        IReadOnlyList<BuildTrack> tracks,
        long discBytes,
        int maxDiscs,
        int groupTrackLimit)
    {
        var diagnostics = new List<BuildDiagnostic>();
        foreach (var track in tracks.Where(track => track.MlpSize <= 0))
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Error,
                "MLP_MISSING",
                $"找不到或无法读取 MLP: {track.MlpPath}（{track.Title}）"));
        }

        var albums = AggregateAlbums(tracks);
        var limit = BuildMath.DiscContentLimit(discBytes);
        var discAlbums = SplitDiscs(albums, limit, diagnostics);
        var discs = discAlbums.Select((items, index) => new DiscPlan(
            index + 1,
            items,
            GroupByParameters(items, groupTrackLimit, diagnostics))).ToArray();

        if (maxDiscs > 0 && discs.Length > maxDiscs)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "DISC_COUNT_EXCEEDED",
                $"按容量需 {discs.Length} 张盘，超出期望的 {maxDiscs} 张。"));
        }

        return new BuildPlan(tracks, discs, diagnostics, limit);
    }

    public static IReadOnlyList<AlbumPlan> AggregateAlbums(IReadOnlyList<BuildTrack> tracks)
    {
        var order = new List<string>();
        var albums = new Dictionary<string, List<BuildTrack>>(StringComparer.Ordinal);
        foreach (var track in tracks)
        {
            if (!albums.TryGetValue(track.Album, out var albumTracks))
            {
                albumTracks = [];
                albums.Add(track.Album, albumTracks);
                order.Add(track.Album);
            }
            albumTracks.Add(track);
        }
        return order.Select(name => new AlbumPlan(name, albums[name])).ToArray();
    }

    private static IReadOnlyList<IReadOnlyList<AlbumPlan>> SplitDiscs(
        IReadOnlyList<AlbumPlan> albums,
        long limit,
        ICollection<BuildDiagnostic> diagnostics)
    {
        var result = new List<IReadOnlyList<AlbumPlan>>();
        var current = new List<AlbumPlan>();
        long currentMlpBytes = 0;
        foreach (var album in albums)
        {
            if (current.Count > 0 &&
                BuildMath.EstimateAobBytes(currentMlpBytes + album.MlpBytes) > limit)
            {
                result.Add(current.ToArray());
                current = [];
                currentMlpBytes = 0;
            }
            current.Add(album);
            currentMlpBytes += album.MlpBytes;
        }
        if (current.Count > 0)
        {
            result.Add(current.ToArray());
        }

        for (var index = 0; index < result.Count; index++)
        {
            var disc = result[index];
            var estimate = BuildMath.EstimateAobBytes(disc.Sum(album => album.MlpBytes));
            if (disc.Count == 1 && estimate > limit)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Warning,
                    "ALBUM_EXCEEDS_DISC",
                    $"第 {index + 1} 盘仅含专辑“{disc[0].Name}”，估算 AOB 已超过容量上限；专辑不会被拆分。"));
            }
        }
        return result;
    }

    private static IReadOnlyList<AudioGroupPlan> GroupByParameters(
        IReadOnlyList<AlbumPlan> albums,
        int groupTrackLimit,
        ICollection<BuildDiagnostic> diagnostics)
    {
        var keyOrder = new List<(int SampleRate, int Bits, string Codec, int Channels)>();
        var buckets = new Dictionary<(int SampleRate, int Bits, string Codec, int Channels), List<BuildTrack>>();
        foreach (var track in albums.SelectMany(album => album.Tracks))
        {
            var key = (track.SampleRate, track.Bits, track.MlpSource == "lpcm" ? "lpcm" : "mlp",
                track.MlpSource == "lpcm" ? track.Channels ?? 0 : 0);
            if (!buckets.TryGetValue(key, out var bucket))
            {
                bucket = [];
                buckets.Add(key, bucket);
                keyOrder.Add(key);
            }
            bucket.Add(track);
        }

        var groups = new List<AudioGroupPlan>();
        foreach (var key in keyOrder)
        {
            var chunks = SplitAtAlbumBoundaries(buckets[key], groupTrackLimit);
            foreach (var chunk in chunks)
            {
                if (chunk.Count > groupTrackLimit)
                {
                    diagnostics.Add(new BuildDiagnostic(
                        BuildDiagnosticSeverity.Warning,
                        "ALBUM_EXCEEDS_GROUP_LIMIT",
                        $"专辑“{chunk[0].Album}”在 {key.SampleRate}Hz/{key.Bits}bit 组中有 {chunk.Count} 轨，超过组轨上限 {groupTrackLimit}；为保持专辑完整未拆分。"));
                }
                groups.Add(new AudioGroupPlan(
                    groups.Count + 1,
                    key.SampleRate,
                    key.Bits,
                    chunk));
            }
        }
        return groups;
    }

    private static IReadOnlyList<IReadOnlyList<BuildTrack>> SplitAtAlbumBoundaries(
        IReadOnlyList<BuildTrack> tracks,
        int groupTrackLimit)
    {
        var result = new List<IReadOnlyList<BuildTrack>>();
        var current = new List<BuildTrack>();
        string? currentAlbum = null;
        foreach (var track in tracks)
        {
            if (current.Count > 0 &&
                !string.Equals(track.Album, currentAlbum, StringComparison.Ordinal) &&
                current.Count >= groupTrackLimit)
            {
                result.Add(current.ToArray());
                current = [];
            }
            current.Add(track);
            currentAlbum = track.Album;
        }
        if (current.Count > 0)
        {
            result.Add(current.ToArray());
        }
        return result;
    }
}
