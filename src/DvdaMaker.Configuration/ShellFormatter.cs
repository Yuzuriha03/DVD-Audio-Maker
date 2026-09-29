namespace DvdaMaker.Configuration;

public static class ShellFormatter
{
    public static string Assignment(string key, string value) =>
        $"{key}='{value.Replace("'", "'\\''", StringComparison.Ordinal)}'";
}
