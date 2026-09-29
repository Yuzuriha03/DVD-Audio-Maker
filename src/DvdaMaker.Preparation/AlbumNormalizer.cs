using System.Text.RegularExpressions;

namespace DvdaMaker.Preparation;

public static partial class AlbumNormalizer
{
    public static void Apply(IReadOnlyList<AudioTrackMetadata> tracks)
    {
        foreach (var album in tracks.GroupBy(track =>
                     string.IsNullOrEmpty(track.Album) ? track.Title : track.Album))
        {
            var albumTracks = album.ToArray();
            if (albumTracks.Length < 2)
            {
                continue;
            }

            var combinations = albumTracks
                .GroupBy(track => (track.SampleRate, track.Bits))
                .ToArray();
            if (combinations.Length == 1)
            {
                continue;
            }

            var majorityRate = albumTracks
                .GroupBy(track => track.SampleRate)
                .OrderByDescending(group => group.Count())
                .First().Key;
            var majorityBits = albumTracks
                .Where(track => track.SampleRate == majorityRate)
                .GroupBy(track => track.Bits)
                .OrderByDescending(group => group.Count())
                .First().Key;

            foreach (var track in albumTracks)
            {
                if ((track.SampleRate, track.Bits) == (majorityRate, majorityBits))
                {
                    continue;
                }
                track.SampleRate = majorityRate;
                track.Bits = majorityBits;
                track.ResampleTo = majorityRate;
            }
        }
    }

    public static int TrackNumber(string value)
    {
        var match = TrackNumberRegex().Match(value ?? string.Empty);
        return match.Success && int.TryParse(match.Groups[1].Value, out var result)
            ? result
            : 9999;
    }

    public static string SafeBaseName(string path) =>
        InvalidFileNameRegex().Replace(System.IO.Path.GetFileNameWithoutExtension(path), "_");

    [GeneratedRegex(@"^\s*(\d+)")]
    private static partial Regex TrackNumberRegex();

    [GeneratedRegex("[<>:\"/\\\\|?*]")]
    private static partial Regex InvalidFileNameRegex();
}
