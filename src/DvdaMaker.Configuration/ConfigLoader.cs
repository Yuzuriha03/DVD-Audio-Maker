using System.Text.RegularExpressions;

namespace DvdaMaker.Configuration;

public sealed partial class ConfigLoader
{
    private readonly IReadOnlyDictionary<string, string?> _environment;

    public ConfigLoader(IReadOnlyDictionary<string, string?>? environment = null)
    {
        _environment = environment ?? ReadEnvironment();
    }

    public DvdaOptions Load(string? explicitPath = null, string? workingDirectory = null)
    {
        var path = FindConfigPath(explicitPath, workingDirectory);
        var fileValues = ParseFile(path);
        return new DvdaOptions(path, fileValues, _environment);
    }

    public string? FindConfigPath(string? explicitPath = null, string? workingDirectory = null)
    {
        if (!string.IsNullOrEmpty(explicitPath))
        {
            return explicitPath;
        }

        if (_environment.TryGetValue("DVDA_CONFIG", out var configured) &&
            !string.IsNullOrEmpty(configured))
        {
            return configured;
        }

        var candidates = new[]
        {
            Path.Combine(AppContext.BaseDirectory, "config.env"),
            Path.Combine(workingDirectory ?? Environment.CurrentDirectory, "config.env"),
        };
        return candidates.FirstOrDefault(File.Exists);
    }

    public static IReadOnlyDictionary<string, string> ParseFile(string? path)
    {
        if (string.IsNullOrEmpty(path) || !File.Exists(path)) return ParseFileManaged(path);
        return DvdaMaker.Processes.RustBridge.Run<IReadOnlyDictionary<string,string>>(
            "config.parse", File.ReadAllText(path), () => ParseFileManaged(path));
    }

    private static IReadOnlyDictionary<string, string> ParseFileManaged(string? path)
    {
        var values = new Dictionary<string, string>(StringComparer.Ordinal);
        if (string.IsNullOrEmpty(path) || !File.Exists(path))
        {
            return values;
        }

        foreach (var raw in File.ReadLines(path))
        {
            var trimmed = raw.Trim();
            if (trimmed.Length == 0 || trimmed.StartsWith('#'))
            {
                continue;
            }

            var match = AssignmentRegex().Match(raw);
            if (!match.Success)
            {
                continue;
            }

            var key = match.Groups[1].Value;
            var value = match.Groups[2].Value.Trim();
            var quoted = value.Length >= 2 &&
                         value[0] == value[^1] &&
                         (value[0] == '\'' || value[0] == '"');
            if (!quoted)
            {
                var spaceComment = value.IndexOf(" #", StringComparison.Ordinal);
                var tabComment = value.IndexOf("\t#", StringComparison.Ordinal);
                var comment = spaceComment < 0 ? tabComment :
                    tabComment < 0 ? spaceComment : Math.Min(spaceComment, tabComment);
                if (comment >= 0)
                {
                    value = value[..comment].TrimEnd();
                }
            }

            value = Unquote(value.Trim());
            if (LooksLikePath(value))
            {
                value = value.Replace('\\', '/');
            }
            values[key] = value;
        }

        return values;
    }

    private static string Unquote(string value) =>
        value.Length >= 2 && value[0] == value[^1] &&
        (value[0] == '\'' || value[0] == '"')
            ? value[1..^1]
            : value;

    private static bool LooksLikePath(string value) =>
        value.Length > 0 &&
        (value.StartsWith('/') ||
         value.StartsWith("\\\\", StringComparison.Ordinal) ||
         (value.Length >= 3 && char.IsAsciiLetter(value[0]) && value[1] == ':' &&
          (value[2] == '\\' || value[2] == '/')) ||
         value.Contains('\\'));

    private static IReadOnlyDictionary<string, string?> ReadEnvironment()
    {
        var result = new Dictionary<string, string?>(StringComparer.Ordinal);
        foreach (System.Collections.DictionaryEntry item in Environment.GetEnvironmentVariables())
        {
            result[(string)item.Key] = item.Value?.ToString();
        }
        return result;
    }

    [GeneratedRegex("^\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*=\\s*(.*?)\\s*$")]
    private static partial Regex AssignmentRegex();
}
