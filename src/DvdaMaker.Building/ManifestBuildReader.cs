using System.Text.Json;
using System.Text.RegularExpressions;
using DvdaMaker.Preparation;

namespace DvdaMaker.Building;

public sealed partial class ManifestBuildReader
{
    public IReadOnlyList<BuildTrack> Read(
        string manifestPath,
        string mlpDirectory,
        Func<string, long>? fileSize = null)
    {
        fileSize ??= path => new FileInfo(path).Length;
        using var stream = File.OpenRead(manifestPath);
        var manifest = JsonSerializer.Deserialize<Dictionary<string, ManifestGroup>>(stream)
            ?? throw new InvalidDataException($"manifest 内容为空: {manifestPath}");

        var tracks = new List<BuildTrack>();
        foreach (var group in manifest.Values)
        {
            foreach (var item in group.Files)
            {
                var source = ToNativePath(item.Source);
                // Python 的精确规则是把路径分隔符替换为两个下划线。
                var mlp = Path.Combine(mlpDirectory, item.Name.Replace("/", "__") + ".mlp");
                tracks.Add(new BuildTrack
                {
                    Date = item.Date,
                    Track = item.Track,
                    Title = item.Title,
                    Album = string.IsNullOrEmpty(item.Album) ? item.Title : item.Album,
                    SampleRate = group.SampleRate,
                    Bits = group.Bits,
                    SourcePath = source,
                    ManifestName = item.Name,
                    Duration = item.Duration,
                    ResampleTo = item.ResampleTo,
                    SourceSize = fileSize(source),
                    MlpPath = mlp,
                    MlpSize = File.Exists(mlp) ? fileSize(mlp) : 0,
                });
            }
        }

        return tracks
            .OrderBy(track => track.Date, StringComparer.Ordinal)
            .ThenBy(track => TrackNumber(track.Track))
            .ThenBy(track => track.Title, StringComparer.Ordinal)
            .ToArray();
    }

    public static int TrackNumber(string? value)
    {
        var match = TrackNumberRegex().Match(value ?? string.Empty);
        return match.Success && int.TryParse(match.Groups[1].Value, out var number)
            ? number
            : 9999;
    }

    public static string ToNativePath(string path)
    {
        return path.Replace('/', Path.DirectorySeparatorChar)
            .Replace('\\', Path.DirectorySeparatorChar);
    }

    [GeneratedRegex(@"^\s*(\d+)")]
    private static partial Regex TrackNumberRegex();

}
