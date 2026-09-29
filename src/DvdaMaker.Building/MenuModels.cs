namespace DvdaMaker.Building;

public sealed record MenuPagePlan(
    string Album,
    int StartTrackIndex,
    int EndTrackIndex,
    bool Continuation,
    IReadOnlyList<BuildTrack> Tracks)
{
    public int TrackCount => EndTrackIndex - StartTrackIndex;
}

public sealed record MenuAssets(
    MenuPlan Plan,
    string Directory,
    string BlankScreen,
    IReadOnlyList<string> Backgrounds,
    IReadOnlyList<string> StillPictures,
    string? IndexCoversFile,
    string DataDirectory,
    string BinaryDirectory,
    string Font,
    string JapaneseFont,
    string KoreanFont,
    IReadOnlyList<BuildDiagnostic> Diagnostics)
{
    public bool HasErrors => Diagnostics.Any(item =>
        item.Severity == BuildDiagnosticSeverity.Error);
}

public sealed record MenuPlan(
    int TotalPages,
    int IndexPages,
    int MaximumRows,
    int FontSize,
    int FontWidth,
    string ScreenText,
    IReadOnlyList<MenuPagePlan> AlbumPages,
    IReadOnlyList<IReadOnlyList<string>> IndexAlbums,
    IReadOnlyList<string> SanitizedValues)
{
    public int AlbumPageCount => AlbumPages.Count;
    public int TrackCount => AlbumPages.Sum(page => page.TrackCount);

    public IReadOnlyList<string> BuildAuthorArguments(
        string blankScreen,
        IReadOnlyList<string> backgrounds,
        string dataDirectory,
        string binaryDirectory,
        string font,
        string japaneseFont,
        string koreanFont,
        string? indexCoversFile,
        IReadOnlyList<string> stillPictures,
        char stillPictureSeparator)
    {
        if (backgrounds.Count != AlbumPageCount)
        {
            throw new InvalidOperationException(
                $"菜单背景图 {backgrounds.Count} 张 != 专辑页 {AlbumPageCount} 页。");
        }

        var arguments = new List<string>
        {
            "--topmenu",
            $"--nmenus={TotalPages}",
            "--blankscreen", blankScreen,
            "--background", string.Join(',', backgrounds),
            "--screentext", ScreenText,
            "--bindir", binaryDirectory,
            "--datadir", dataDirectory,
        };
        if (IndexPages > 0)
        {
            arguments.AddRange(["--index-pages", IndexPages.ToString()]);
            if (!string.IsNullOrWhiteSpace(indexCoversFile))
            {
                arguments.AddRange(["--index-covers", indexCoversFile]);
            }
        }
        if (!string.IsNullOrWhiteSpace(font))
        {
            arguments.AddRange(["--fontname", MenuPlanner.NormalizeFontPath(font)]);
        }
        if (!string.IsNullOrWhiteSpace(japaneseFont))
        {
            arguments.AddRange(["--fontname-jp", MenuPlanner.NormalizeFontPath(japaneseFont)]);
        }
        if (!string.IsNullOrWhiteSpace(koreanFont))
        {
            arguments.AddRange(["--fontname-kr", MenuPlanner.NormalizeFontPath(koreanFont)]);
        }
        arguments.AddRange(["--fontsize", FontSize.ToString(), "--fontwidth", FontWidth.ToString()]);
        if (stillPictures.Any(path => path.Length > 0))
        {
            arguments.AddRange(["--stillpics", string.Join(stillPictureSeparator, stillPictures)]);
        }
        return arguments;
    }
}
