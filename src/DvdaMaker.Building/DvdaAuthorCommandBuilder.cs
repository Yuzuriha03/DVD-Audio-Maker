using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public static class DvdaAuthorCommandBuilder
{
    public static IReadOnlyList<string> BuildArguments(
        DiscPlan disc,
        string outputDirectory,
        string temporaryDirectory,
        MenuAssets? menuAssets = null,
        string titleMode = "album",
        string? isoPath = null,
        string? isoVolume = null)
    {
        var arguments = new List<string>();
        var normalizedMode = NormalizeTitleMode(titleMode);
        var singleTitle = normalizedMode == "one";
        var tracksPerTitle = int.TryParse(normalizedMode, out var parsed) ? parsed : (int?)null;
        var tracksSeen = 0;
        foreach (var group in disc.Groups)
        {
            arguments.Add("-g");
            string? previousAlbum = null;
            foreach (var track in group.Tracks)
            {
                var needsTitleBoundary = !singleTitle &&
                    (tracksPerTitle is not null
                        ? tracksSeen > 0 && tracksSeen % tracksPerTitle.Value == 0
                        : previousAlbum is not null &&
                          !string.Equals(previousAlbum, track.Album, StringComparison.Ordinal));
                if (needsTitleBoundary)
                {
                    arguments.Add("-z");
                }
                arguments.Add(track.MlpPath);
                previousAlbum = track.Album;
                tracksSeen++;
            }
        }
        arguments.AddRange(["-o", outputDirectory, "-D", temporaryDirectory, "-W", "-P0", "-n"]);
        if (menuAssets is not null)
        {
            arguments.AddRange(menuAssets.Plan.BuildAuthorArguments(
                menuAssets.BlankScreen,
                menuAssets.Backgrounds,
                menuAssets.DataDirectory,
                menuAssets.BinaryDirectory,
                menuAssets.Font,
                menuAssets.JapaneseFont,
                menuAssets.KoreanFont,
                menuAssets.IndexCoversFile,
                menuAssets.StillPictures,
                OperatingSystem.IsWindows() ? ';' : ':'));
        }
        if (!string.IsNullOrWhiteSpace(isoPath))
        {
            // dvda-author owns the in-process C ISO writer. Use an attached
            // optional argument because getopt does not consume a separate
            // token for --iso when its argument is optional.
            arguments.Add("--iso=" + isoPath);
            if (!string.IsNullOrWhiteSpace(isoVolume))
            {
                arguments.Add("--iso-volume");
                arguments.Add(isoVolume);
            }
        }
        return arguments;
    }

    internal static string NormalizeTitleMode(string? value)
    {
        var normalized = (value ?? "album").Trim().ToLowerInvariant();
        if (normalized is "album" or "one") return normalized;
        return int.TryParse(normalized, out var count) && count > 0
            ? count.ToString(System.Globalization.CultureInfo.InvariantCulture)
            : "album";
    }

}
