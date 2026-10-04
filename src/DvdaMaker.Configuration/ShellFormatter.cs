namespace DvdaMaker.Configuration;

public static class ShellFormatter
{
    public static string Assignment(string key, string value) =>
        DvdaMaker.Processes.RustBridge.Run<string>("shell.assignment", new { Key = key, Value = value },
            () => $"{key}='{value.Replace("'", "'\\''", StringComparison.Ordinal)}'");
}
