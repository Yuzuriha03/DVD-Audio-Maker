namespace DvdaMaker.Processes;

public static class CommandLineFormatter
{
    public static string Format(string fileName, IEnumerable<string> arguments) =>
        string.Join(' ', new[] { Quote(fileName) }.Concat(arguments.Select(Quote)));

    public static string Quote(string value)
    {
        if (value.Length > 0 && !value.Any(char.IsWhiteSpace) &&
            !value.Contains('"', StringComparison.Ordinal))
        {
            return value;
        }

        return "\"" + value.Replace("\\", "\\\\", StringComparison.Ordinal)
            .Replace("\"", "\\\"", StringComparison.Ordinal) + "\"";
    }
}
