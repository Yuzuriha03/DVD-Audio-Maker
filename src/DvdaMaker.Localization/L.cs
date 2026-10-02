using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace DvdaMaker.Localization;

/// <summary>Presentation-only localization. File names, configuration values and encoder inputs stay unchanged.</summary>
public static class L
{
    private static readonly Lazy<Catalog> Messages = new(() => new Catalog());
    private static string _language = "zh-CN";
    public static string Language => _language;
    public static bool IsEnglish => _language == "en";

    public static string Normalize(string? language) => language?.Trim().ToLowerInvariant() switch
    {
        null or "" or "auto" => CultureInfo.InstalledUICulture.TwoLetterISOLanguageName == "zh" ? "zh-CN" : "en",
        "zh" or "zh-cn" or "zh-hans" or "中文" => "zh-CN",
        "en" or "en-us" or "en-gb" or "english" => "en",
        _ => throw new ArgumentException("Language must be auto, en, or zh-CN."),
    };

    public static void SetLanguage(string? language) => _language = Normalize(language);

    public static string? TakeLanguage(List<string> arguments)
    {
        string? result = null;
        for (var i = 0; i < arguments.Count;)
        {
            if (arguments[i] != "--language") { i++; continue; }
            if (result is not null || i + 1 == arguments.Count || arguments[i + 1].StartsWith("--", StringComparison.Ordinal))
                throw new ArgumentException("Usage: --language auto|en|zh-CN (specify once).");
            result = Normalize(arguments[i + 1]);
            arguments.RemoveRange(i, 2);
        }
        return result;
    }

    public static string T(string? text) => text is null ? "" : !IsEnglish ? text : Messages.Value.Translate(text, 0);
    public static void LocalizeConsole()
    {
        Console.SetOut(TextWriter.Synchronized(new LocalizedWriter(Console.Out)));
        Console.SetError(TextWriter.Synchronized(new LocalizedWriter(Console.Error)));
    }

    public static IReadOnlyList<(string Source, string English, int Parameters)> Entries =>
        Messages.Value.All.Select(e => (e.Source, e.English, e.Parameters)).ToArray();

    private sealed class Catalog
    {
        public Entry[] All { get; }
        private readonly Dictionary<string, string> _exact;
        private readonly Entry[] _templates;
        public Catalog()
        {
            using var input = typeof(L).Assembly.GetManifestResourceStream("DvdaMaker.Localization.Messages.en.json")
                ?? throw new InvalidOperationException("The English language resource is missing.");
            All = JsonSerializer.Deserialize<Entry[]>(input) ?? [];
            _exact = All.Where(e => e.Parameters == 0).ToDictionary(e => Unescape(e.Source), e => Unescape(e.English), StringComparer.Ordinal);
            _templates = All.Where(e => e.Parameters > 0)
                .OrderByDescending(e => Placeholder.Replace(e.Source, "").Length).ThenBy(e => e.Parameters).ToArray();
        }
        public string Translate(string text, int depth)
        {
            if (depth > 6 || !text.Any(c => c is >= '\u4e00' and <= '\u9fff')) return text;
            if (_exact.TryGetValue(text, out var translated)) return translated;
            foreach (var entry in _templates)
            {
                if (entry.Prefix.Length > 0 && !text.StartsWith(entry.Prefix, StringComparison.Ordinal)) continue;
                var match = entry.Pattern.Match(text);
                if (!match.Success) continue;
                var values = Enumerable.Range(0, entry.Parameters).Select(i =>
                    entry.Nested.Contains(i) ? Translate(match.Groups["v" + i].Value, depth + 1) : match.Groups["v" + i].Value).Cast<object>().ToArray();
                return string.Format(CultureInfo.InvariantCulture, entry.English, values);
            }
            if (text.Contains('\n')) return string.Join("\n", text.Split('\n').Select(line => Translate(line, depth + 1)));
            var prefix = Regex.Match(text, @"\A(?<prefix>(?:\[[A-Z][A-Za-z -]*\]\s*|[A-Z][A-Z0-9_]+:\s*))(?<body>.+)\z", RegexOptions.Singleline | RegexOptions.CultureInvariant);
            if (prefix.Success) return prefix.Groups["prefix"].Value + Translate(prefix.Groups["body"].Value, depth + 1);
            return text;
        }
    }

    private static readonly Regex Placeholder = new(@"(?<!\{)\{(\d+)\}(?!\})", RegexOptions.CultureInvariant);
    private static string Unescape(string text) => text.Replace("{{", "{", StringComparison.Ordinal).Replace("}}", "}", StringComparison.Ordinal);
    private sealed class Entry
    {
        public required string Source { get; init; }
        public required string English { get; init; }
        public int Parameters { get; init; }
        public int[] Nested { get; init; } = [];
        private Regex? _pattern;
        private string? _prefix;
        public string Prefix => _prefix ??= Unescape(Source[..Placeholder.Match(Source).Index]);
        public Regex Pattern => _pattern ??= CreatePattern();
        private Regex CreatePattern()
        {
            var result = new StringBuilder(@"\A"); var offset = 0;
            foreach (Match match in Placeholder.Matches(Source))
            {
                result.Append(Regex.Escape(Unescape(Source[offset..match.Index])));
                result.Append("(?<v").Append(match.Groups[1].Value).Append(@">.*?)");
                offset = match.Index + match.Length;
            }
            result.Append(Regex.Escape(Unescape(Source[offset..]))).Append(@"\z");
            return new Regex(result.ToString(), RegexOptions.Singleline | RegexOptions.CultureInvariant | RegexOptions.NonBacktracking, TimeSpan.FromMilliseconds(100));
        }
    }
}

internal sealed class LocalizedWriter(TextWriter target) : TextWriter
{
    private readonly StringBuilder _line = new();
    public override Encoding Encoding => target.Encoding;
    public override void Write(char value)
    {
        if (value == '\n') { target.WriteLine(L.T(_line.ToString().TrimEnd('\r'))); _line.Clear(); }
        else _line.Append(value);
    }
    public override void Write(string? value) { if (value is not null) foreach (var c in value) Write(c); }
    public override void WriteLine(string? value) { Write(value); Write('\n'); }
    public override void Flush() { if (_line.Length > 0) { target.Write(L.T(_line.ToString())); _line.Clear(); } target.Flush(); }
    protected override void Dispose(bool disposing) { if (disposing) Flush(); base.Dispose(disposing); }
}
