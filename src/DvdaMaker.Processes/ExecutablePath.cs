namespace DvdaMaker.Processes;

/// <summary>Resolve once so validation, execution and cache hashing use the same executable.</summary>
public static class ExecutablePath
{
    public static string? Resolve(string configured)
    {
        var value = configured.Trim().Trim('"');
        if (value.Length == 0) return null;
        if (BuiltinMedia.IsBuiltin(value)) return BuiltinMedia.IsAvailable ? value : null;
        if (BuiltinImages.IsBuiltin(value)) return BuiltinImages.IsAvailable ? value : null;
        var names = OperatingSystem.IsWindows() && Path.GetExtension(value).Length == 0
            ? new[] { value + ".exe", value } : new[] { value };
        if (Path.IsPathRooted(value) || value.Contains('/') || value.Contains('\\'))
            return names.Where(File.Exists).Select(Path.GetFullPath).FirstOrDefault();
        // An explicit directory is required for local tools; bare names search PATH.
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator))
        {
            var folder = directory.Trim().Trim('"');
            if (folder.Length == 0) continue;
            foreach (var name in names)
            {
                var candidate = Path.Combine(folder, name);
                if (File.Exists(candidate)) return Path.GetFullPath(candidate);
            }
        }
        return null;
    }
}
