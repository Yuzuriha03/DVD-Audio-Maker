using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record MenuFontResolution(
    string Font,
    string JapaneseFont,
    string KoreanFont,
    IReadOnlySet<string> MissingScripts,
    IReadOnlyList<BuildDiagnostic> Diagnostics);

public sealed partial class MenuFontResolver(
    ProcessRunner runner,
    BuildLogWriter? log = null)
{
    private static readonly (string Script, string Probe)[] ScriptProbes =
    [
        ("ASCII", "Ag("),
        ("汉字", "中文"),
        ("平假名", "あいう"),
        ("片假名", "アイウ"),
        ("韩文", "한글"),
        ("CJK标点", "、《》"),
    ];

    private static readonly string[] FontCandidates =
    [
        "Noto-Sans-CJK-SC",
        "Noto-Sans-CJK-JP",
        "Noto-Sans-CJK-KR",
        "Noto-Sans-CJK-TC",
        "NotoSansCJK-Regular",
        "Source-Han-Sans-CN",
        "WenQuanYi-Micro-Hei",
        "WenQuanYi-Zen-Hei",
        "AR-PL-UMing-CN",
        "Droid-Sans-Fallback",
        "DejaVu-Sans",
    ];

    public async Task<MenuFontResolution> ResolveAsync(
        string imageMagick,
        string preferred,
        string preferredJapanese,
        string preferredKorean,
        IEnumerable<string> texts,
        CancellationToken cancellationToken = default)
    {
        var diagnostics = new List<BuildDiagnostic>();
        var required = NeededScripts(texts);
        var availableFonts = await ListFontsAsync(imageMagick, cancellationToken)
            .ConfigureAwait(false);

        preferred = MenuPlanner.NormalizeFontPath(preferred);
        if (preferred.EndsWith(".ttc", StringComparison.OrdinalIgnoreCase))
        {
            diagnostics.Add(Warning(
                "MENU_FONT_COLLECTION",
                "主字体是 TTC 集合；ImageMagick 按文件路径通常只使用 face 0，" +
                "Noto Sans CJK 可能因此把中文显示为日文字形。建议使用独立 SC OTF。"));
        }

        var selected = string.Empty;
        IReadOnlySet<string> missing = required;
        if (preferred.Length > 0 && FontExists(preferred, availableFonts))
        {
            var coverage = await CoverageAsync(
                imageMagick, preferred, required, cancellationToken).ConfigureAwait(false);
            var preferredMissing = required.Except(coverage, StringComparer.Ordinal).ToHashSet();
            if (preferredMissing.Count == 0)
            {
                selected = preferred;
                missing = preferredMissing;
            }
            else
            {
                diagnostics.Add(Warning(
                    "MENU_FONT_INCOMPLETE",
                    $"字体 {preferred} 实际画不出 {string.Join('、', preferredMissing.Order())}，" +
                    "正在寻找候选字体。"));
            }
        }
        else if (preferred.Length > 0)
        {
            diagnostics.Add(Warning(
                "MENU_FONT_UNAVAILABLE",
                $"字体不可用或名称含空格，正在寻找候选字体: {preferred}"));
        }

        if (selected.Length == 0 && required.Count > 0)
        {
            var candidates = FontCandidates
                .Concat(availableFonts.Order(StringComparer.OrdinalIgnoreCase))
                .Where(font => CjkFontHint().IsMatch(font))
                .Distinct(StringComparer.OrdinalIgnoreCase);
            var best = string.Empty;
            IReadOnlySet<string> bestMissing = required;
            var bestScore = -1;
            foreach (var candidate in candidates)
            {
                if (!FontExists(candidate, availableFonts)) continue;
                var coverage = await CoverageAsync(
                    imageMagick, candidate, required, cancellationToken).ConfigureAwait(false);
                var candidateMissing = required.Except(coverage, StringComparer.Ordinal).ToHashSet();
                if (candidateMissing.Count == 0)
                {
                    selected = candidate;
                    missing = candidateMissing;
                    break;
                }
                var score = required.Count - candidateMissing.Count;
                if (score <= bestScore) continue;
                best = candidate;
                bestMissing = candidateMissing;
                bestScore = score;
            }
            if (selected.Length == 0)
            {
                selected = best;
                missing = bestMissing;
            }
        }
        else if (required.Count == 0)
        {
            selected = preferred;
            missing = new HashSet<string>();
        }

        if (selected.Length == 0)
        {
            diagnostics.Add(Error(
                "MENU_FONT_MISSING",
                "找不到能渲染菜单文字的字体；建议安装 fonts-noto-cjk。"));
        }
        else if (missing.Count > 0)
        {
            diagnostics.Add(Error(
                "MENU_FONT_GLYPHS_MISSING",
                $"字体 {selected} 仍无法渲染: {string.Join('、', missing.Order())}。"));
        }

        var japanese = await ResolveRegionalFaceAsync(
            imageMagick, selected, preferredJapanese, "jp", "日文",
            availableFonts, diagnostics, cancellationToken).ConfigureAwait(false);
        var korean = await ResolveRegionalFaceAsync(
            imageMagick, selected, preferredKorean, "kr", "韩文",
            availableFonts, diagnostics, cancellationToken).ConfigureAwait(false);

        return new MenuFontResolution(
            selected, japanese, korean, missing, diagnostics);
    }

    public static IReadOnlySet<string> NeededScripts(IEnumerable<string> texts)
    {
        var output = new HashSet<string>(StringComparer.Ordinal);
        foreach (var character in texts.SelectMany(text => text ?? string.Empty))
        {
            if (ScriptOf(character) is { } script) output.Add(script);
        }
        return output;
    }

    public static string? ScriptOf(char character)
    {
        if (char.IsWhiteSpace(character)) return null;
        var value = (int)character;
        if (value < 0x80) return "ASCII";
        if (value is >= 0x3040 and <= 0x309F) return "平假名";
        if (value is >= 0x30A0 and <= 0x30FF) return "片假名";
        if (value is >= 0xAC00 and <= 0xD7AF or >= 0x1100 and <= 0x11FF) return "韩文";
        if (value is >= 0x3400 and <= 0x4DBF or >= 0x4E00 and <= 0x9FFF) return "汉字";
        if (value is >= 0x3000 and <= 0x303F or >= 0xFF00 and <= 0xFFEF) return "CJK标点";
        return null;
    }

    public static string DeriveRegionalFace(
        string font,
        string tag,
        Func<string, bool> exists)
    {
        if (string.IsNullOrWhiteSpace(font)) return string.Empty;
        if (!font.Contains('/') && !font.Contains('\\'))
        {
            var match = FontFamilyFace().Match(font);
            if (!match.Success) return string.Empty;
            var candidate = match.Groups[1].Value + "-" + tag.ToUpperInvariant() +
                            font[(match.Index + match.Length)..];
            return exists(candidate) ? candidate : string.Empty;
        }

        var normalized = font.Replace('\\', '/');
        var slash = normalized.LastIndexOf('/');
        var directory = slash >= 0 ? normalized[..slash] : string.Empty;
        var fileName = slash >= 0 ? normalized[(slash + 1)..] : normalized;
        var fileMatch = FontFileFace().Match(fileName);
        if (!fileMatch.Success) return string.Empty;
        var candidateName = fileMatch.Groups[1].Value + tag + fileMatch.Groups[3].Value;
        var candidatePath = directory.Length == 0 ? candidateName : directory + "/" + candidateName;
        return exists(candidatePath) ? MenuPlanner.NormalizeFontPath(candidatePath) : string.Empty;
    }

    private async Task<string> ResolveRegionalFaceAsync(
        string imageMagick,
        string mainFont,
        string configured,
        string tag,
        string label,
        IReadOnlySet<string> availableFonts,
        ICollection<BuildDiagnostic> diagnostics,
        CancellationToken cancellationToken)
    {
        var candidate = configured.Length > 0
            ? MenuPlanner.NormalizeFontPath(configured)
            : DeriveRegionalFace(mainFont, tag, value => FontExists(value, availableFonts));
        if (candidate.Length == 0) return string.Empty;
        if (!FontExists(candidate, availableFonts))
        {
            diagnostics.Add(Warning(
                "MENU_REGIONAL_FONT_UNAVAILABLE",
                $"{label}字体不可用，继续使用主字体: {candidate}"));
            return string.Empty;
        }
        var script = tag == "jp" ? "平假名" : "韩文";
        var coverage = await CoverageAsync(
            imageMagick,
            candidate,
            new HashSet<string>(StringComparer.Ordinal) { script },
            cancellationToken).ConfigureAwait(false);
        if (!coverage.Contains(script))
        {
            diagnostics.Add(Warning(
                "MENU_REGIONAL_FONT_INCOMPLETE",
                $"{label}字体无法实际渲染{script}，继续使用主字体: {candidate}"));
            return string.Empty;
        }
        return candidate;
    }

    private async Task<IReadOnlySet<string>> CoverageAsync(
        string imageMagick,
        string font,
        IReadOnlySet<string> required,
        CancellationToken cancellationToken)
    {
        var output = new HashSet<string>(StringComparer.Ordinal);
        foreach (var (script, probe) in ScriptProbes)
        {
            if (!required.Contains(script)) continue;
            if (await ReadInkAsync(imageMagick, font, probe, cancellationToken).ConfigureAwait(false) > 0)
            {
                output.Add(script);
            }
        }
        return output;
    }

    private async Task<double> ReadInkAsync(
        string imageMagick,
        string font,
        string probe,
        CancellationToken cancellationToken)
    {
        string? temporary = null;
        try
        {
            var annotation = probe;
            if (OperatingSystem.IsWindows())
            {
                temporary = Path.Combine(
                    Path.GetTempPath(), "dvda-font-probe-" + Guid.NewGuid().ToString("N") + ".txt");
                await File.WriteAllTextAsync(
                    temporary, probe, new UTF8Encoding(false), cancellationToken).ConfigureAwait(false);
                annotation = "@" + temporary.Replace('\\', '/');
            }
            var arguments = new List<string>
            {
                "-size", "160x48", "xc:none", "-font", MenuPlanner.NormalizeFontPath(font),
                "-pointsize", "20", "-fill", "white", "-annotate", "+2+32", annotation,
                "-format", "%[fx:mean.a]", "info:",
            };
            log?.WriteCommand(imageMagick, arguments);
            var result = await runner.RunAsync(new ProcessRequest
            {
                FileName = imageMagick,
                Arguments = arguments,
            }, cancellationToken).ConfigureAwait(false);
            return result.Succeeded && double.TryParse(
                result.StandardOutput.Trim(), NumberStyles.Float,
                CultureInfo.InvariantCulture, out var value)
                ? value
                : 0;
        }
        finally
        {
            if (temporary is not null)
            {
                try { File.Delete(temporary); } catch (IOException) { }
            }
        }
    }

    private async Task<IReadOnlySet<string>> ListFontsAsync(
        string imageMagick,
        CancellationToken cancellationToken)
    {
        var arguments = new[] { "-list", "font" };
        log?.WriteCommand(imageMagick, arguments);
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = imageMagick,
            Arguments = arguments,
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded) return new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        return result.StandardOutput.Split('\n')
            .Select(line => line.Trim())
            .Where(line => line.StartsWith("Font:", StringComparison.Ordinal))
            .Select(line => line[5..].Trim())
            .Where(line => line.Length > 0)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
    }

    private static bool FontExists(string font, IReadOnlySet<string> availableFonts)
    {
        if (string.IsNullOrWhiteSpace(font) || font.Any(char.IsWhiteSpace)) return false;
        var normalized = MenuPlanner.NormalizeFontPath(font);
        if (File.Exists(normalized)) return true;
        var target = NormalizeName(normalized);
        return availableFonts.Any(value =>
        {
            var candidate = NormalizeName(value);
            return candidate == target ||
                   target.Length > 0 && (candidate.Contains(target) || target.Contains(candidate));
        });
    }

    private static string NormalizeName(string value) =>
        NonAlphanumeric().Replace(value.ToLowerInvariant(), string.Empty);

    private static BuildDiagnostic Warning(string code, string message) =>
        new(BuildDiagnosticSeverity.Warning, code, message);

    private static BuildDiagnostic Error(string code, string message) =>
        new(BuildDiagnosticSeverity.Error, code, message);

    [GeneratedRegex("cjk|hei|ming|song|kai|wqy|wenquan|arphic|ar-?pl|droid|han|zen|uming|ukai|noto", RegexOptions.IgnoreCase)]
    private static partial Regex CjkFontHint();

    [GeneratedRegex(@"(Noto-Sans-CJK)-(SC|JP|KR|TC|HK)", RegexOptions.IgnoreCase)]
    private static partial Regex FontFamilyFace();

    [GeneratedRegex(@"^(NotoSansCJK)(sc|jp|kr|tc|hk)(-.*)$", RegexOptions.IgnoreCase)]
    private static partial Regex FontFileFace();

    [GeneratedRegex("[^a-z0-9]")]
    private static partial Regex NonAlphanumeric();
}
