using System.Text;
using System.Text.RegularExpressions;
using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public static partial class MenuPlanner
{
    private sealed record RustSanitizeResult(string Value, bool Changed);
    public const int FrameWidth = 720;
    public const int FrameHeight = 576;
    public const int IndexColumns = 4;
    public const int IndexRows = 3;
    public const int IndexPerPage = IndexColumns * IndexRows;
    public const int MaximumMenuRows = 24;
    public const int MinimumFontSize = 7;
    public const int MaximumFontSize = 30;
    public const int TextBudgetPixels = 660;

    public static MenuPlan Create(DiscPlan disc, DvdaOptions options)
    {
        var tracks = disc.Groups.SelectMany(group => group.Tracks).ToArray();
        var albums = tracks.Select(AlbumOf).ToArray();
        var rowCap = Math.Min(options.MenuTracksPerPage, MaximumMenuRows);
        var pages = CreateAlbumPages(tracks, albums, rowCap);
        var maximumRows = Math.Max(1, pages.Select(page => page.TrackCount).DefaultIfEmpty(1).Max());
        var fontSize = ComputeFontSize(maximumRows);

        var indexPages = pages.Count == 0
            ? 0
            : (pages.Count + IndexPerPage - 1) / IndexPerPage;
        if (indexPages > 0 && pages.Count < options.MenuIndexMinimumAlbums)
        {
            indexPages = 0;
        }

        var indexAlbums = Enumerable.Range(0, indexPages)
            .Select(page => (IReadOnlyList<string>)pages
                .Skip(page * IndexPerPage)
                .Take(IndexPerPage)
                .Select(item => item.Album)
                .ToArray())
            .ToArray();

        var sanitized = new List<string>();
        var albumLabels = BuildAlbumLabels(pages.Select(page => page.Album));
        var chunks = new List<string>();
        var displayedTexts = new List<string>();

        foreach (var indexAlbumPage in indexAlbums)
        {
            var names = indexAlbumPage.Select(album =>
                Truncate(SanitizeAndRecord(albumLabels[album], sanitized), fontSize)).ToArray();
            chunks.Add("选择专辑=" + string.Join(',', names));
        }

        foreach (var page in pages)
        {
            var album = albumLabels[page.Album] + (page.Continuation ? "（续）" : string.Empty);
            album = SanitizeAndRecord(album, sanitized);
            var titles = page.Tracks.Select(track =>
                Truncate(SanitizeAndRecord(
                    string.IsNullOrWhiteSpace(track.Title)
                        ? Path.GetFileName(track.SourcePath)
                        : track.Title,
                    sanitized), fontSize)).ToArray();
            chunks.Add(album + "=" + string.Join(',', titles));
            displayedTexts.Add(album);
            displayedTexts.AddRange(titles);
        }

        var title = Sanitize(options.Title).Value;
        displayedTexts.Add(title);
        var screenText = title + "=" + string.Join(':', chunks);
        return new MenuPlan(
            indexPages + pages.Count,
            indexPages,
            maximumRows,
            fontSize,
            ComputeFontWidth(displayedTexts),
            screenText,
            pages,
            indexAlbums,
            sanitized.Distinct(StringComparer.Ordinal).ToArray());
    }

    public static IReadOnlyList<MenuPagePlan> CreateAlbumPages(
        IReadOnlyList<BuildTrack> tracks,
        IReadOnlyList<string> albums,
        int rowCap)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
            return DvdaMaker.Processes.RustBridge.Run<IReadOnlyList<MenuPagePlan>>("menu.pages",
                new { Tracks = tracks, Albums = albums, RowCap = rowCap },
                () => CreateAlbumPagesManaged(tracks, albums, rowCap));
        return CreateAlbumPagesManaged(tracks, albums, rowCap);
    }

    private static IReadOnlyList<MenuPagePlan> CreateAlbumPagesManaged(
        IReadOnlyList<BuildTrack> tracks,
        IReadOnlyList<string> albums,
        int rowCap)
    {
        if (tracks.Count != albums.Count)
        {
            throw new ArgumentException("曲目与专辑列表长度必须一致。");
        }
        if (rowCap < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(rowCap));
        }

        var pages = new List<MenuPagePlan>();
        var blockStart = 0;
        while (blockStart < tracks.Count)
        {
            var album = albums[blockStart];
            var blockEnd = blockStart + 1;
            while (blockEnd < tracks.Count && albums[blockEnd] == album)
            {
                blockEnd++;
            }

            var pageStart = blockStart;
            var continuation = false;
            while (pageStart < blockEnd)
            {
                var pageEnd = Math.Min(pageStart + rowCap, blockEnd);
                pages.Add(new MenuPagePlan(
                    album,
                    pageStart,
                    pageEnd,
                    continuation,
                    tracks.Skip(pageStart).Take(pageEnd - pageStart).ToArray()));
                continuation = true;
                pageStart = pageEnd;
            }
            blockStart = blockEnd;
        }
        return pages;
    }

    public static int ComputeFontSize(int rows)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
            return DvdaMaker.Processes.RustBridge.Run<int>("menu.font_size", rows, () => ComputeFontSizeManaged(rows));
        return ComputeFontSizeManaged(rows);
    }

    private static int ComputeFontSizeManaged(int rows)
    {
        var span = rows + 4;
        var labelHeight = (FrameHeight - 56 - 40 - span * 12) / span;
        var spacing = labelHeight + 12;
        return Math.Clamp(spacing - 10, MinimumFontSize, MaximumFontSize);
    }

    public static int ComputeFontWidth(IEnumerable<string> texts)
    {
        var values = texts.ToArray();
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
            return DvdaMaker.Processes.RustBridge.Run<int>("menu.font_width", values, () => ComputeFontWidthManaged(values));
        return ComputeFontWidthManaged(values);
    }

    private static int ComputeFontWidthManaged(IEnumerable<string> texts)
    {
        var wide = 0;
        var narrow = 0;
        var bytes = 0;
        foreach (var character in texts.SelectMany(text => text))
        {
            if (IsWide(character))
            {
                wide++;
                bytes += 3;
            }
            else
            {
                narrow++;
                bytes++;
            }
        }
        if (bytes == 0) return 5;
        return Math.Clamp((int)Math.Round(10d * (wide + 0.5 * narrow) / bytes), 1, 10);
    }

    public static (string Value, bool Changed) Sanitize(string value)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
        {
            var result = DvdaMaker.Processes.RustBridge.Run<RustSanitizeResult>(
                "menu.sanitize", value, () =>
                {
                    var managed = SanitizeManaged(value);
                    return new RustSanitizeResult(managed.Value, managed.Changed);
                });
            return (result.Value, result.Changed);
        }
        return SanitizeManaged(value);
    }

    private static (string Value, bool Changed) SanitizeManaged(string value)
    {
        if (value.IndexOfAny([',', ':', '=']) < 0)
        {
            return (value, false);
        }
        var wide = value.Any(IsWide);
        var builder = new StringBuilder(value.Length);
        foreach (var character in value)
        {
            builder.Append(character switch
            {
                ',' => wide ? '，' : '·',
                ':' => wide ? '：' : '·',
                '=' => wide ? '＝' : '·',
                _ => character,
            });
        }
        return (builder.ToString(), true);
    }

    public static string Truncate(string value, int fontSize, int budget = TextBudgetPixels)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
            return DvdaMaker.Processes.RustBridge.Run<string>("menu.truncate",
                new { Value = value, FontSize = fontSize, Budget = budget },
                () => TruncateManaged(value, fontSize, budget));
        return TruncateManaged(value, fontSize, budget);
    }

    private static string TruncateManaged(string value, int fontSize, int budget)
    {
        if (TextPixels(value, fontSize) <= budget) return value;
        var output = value;
        while (output.Length > 0 && TextPixels(output + "~", fontSize) > budget)
        {
            output = output[..^1];
        }
        return output.Length == 0 ? string.Empty : output.TrimEnd() + "~";
    }

    public static string ShortAlbum(string value, int maximumCharacters = 24)
    {
        if (DvdaMaker.Processes.RustBridge.Mode != "managed")
            return DvdaMaker.Processes.RustBridge.Run<string>("menu.short_album",
                new { Value = value, MaximumCharacters = maximumCharacters },
                () => ShortAlbumManaged(value, maximumCharacters));
        return ShortAlbumManaged(value, maximumCharacters);
    }

    private static string ShortAlbumManaged(string value, int maximumCharacters)
    {
        var output = AlbumSuffixPattern().Split(value, 2)[0].Trim().TrimEnd('-', '–', '—').Trim();
        if (output.Length == 0) output = value;
        return output.Length > maximumCharacters ? output[..maximumCharacters] : output;
    }

    public static string NormalizeFontPath(string value) =>
        DvdaMaker.Processes.RustBridge.Mode != "managed"
            ? DvdaMaker.Processes.RustBridge.Run<string>("menu.normalize_path", value,
                () => OperatingSystem.IsWindows() ? value.Replace('\\', '/') : value)
            : OperatingSystem.IsWindows() ? value.Replace('\\', '/') : value;

    public static string AlbumOf(BuildTrack track)
    {
        var source = track.SourcePath.Replace('\\', '/').TrimEnd('/');
        var slash = source.LastIndexOf('/');
        if (slash > 0)
        {
            var parent = source[..slash].TrimEnd('/');
            var parentSlash = parent.LastIndexOf('/');
            var name = parent[(parentSlash + 1)..];
            if (name.Length > 0) return name;
        }
        return track.Album;
    }

    private static Dictionary<string, string> BuildAlbumLabels(IEnumerable<string> albums)
    {
        var distinct = albums.Distinct(StringComparer.Ordinal).ToArray();
        var shortGroups = distinct.GroupBy(album => ShortAlbum(album), StringComparer.Ordinal)
            .ToDictionary(group => group.Key, group => group.ToArray(), StringComparer.Ordinal);
        var output = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var album in distinct)
        {
            var shortName = ShortAlbum(album);
            if (shortGroups[shortName].Length == 1)
            {
                output[album] = shortName;
                continue;
            }
            var qualifier = BracketContents().Matches(album)
                .Select(match => match.Groups[1].Value.Trim())
                .FirstOrDefault(value => !GenericGameQualifier().IsMatch(value));
            output[album] = qualifier is { Length: > 0 }
                ? $"{shortName} [{qualifier}]"
                : shortName;
        }
        return output;
    }

    private static string SanitizeAndRecord(string value, ICollection<string> changed)
    {
        var result = Sanitize(value);
        if (result.Changed) changed.Add(value);
        return result.Value;
    }

    private static int TextPixels(string value, int fontSize)
    {
        var wide = value.Count(IsWide);
        var narrow = value.Length - wide;
        return (int)(wide * fontSize + narrow * fontSize * 0.55);
    }

    private static bool IsWide(char character)
    {
        var value = character;
        return value is >= '\u2E80' and <= '\u9FFF' || value >= '\uFF00';
    }

    [GeneratedRegex(@"[\(\[（【]")]
    private static partial Regex AlbumSuffixPattern();

    [GeneratedRegex(@"[\(\[（【]([^\)\]）】]*)[\)\]）】]")]
    private static partial Regex BracketContents();

    [GeneratedRegex(@"^\s*游戏[《<]")]
    private static partial Regex GenericGameQualifier();
}
