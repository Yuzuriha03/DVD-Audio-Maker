using System.Reflection;
using DvdaMaker.Packaging;

namespace DvdaMaker.Processes;

public static class BundledRuntime
{
    public static string Root { get; private set; } = AppContext.BaseDirectory;
    public static string CacheDirectory => Path.GetFullPath(
        Environment.GetEnvironmentVariable("DVDA_BUNDLE_CACHE_ROOT") is { Length: > 0 } configured
            ? configured : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "DVD-Audio-Maker", "runtime"));

    public static void Initialize(Assembly application)
    {
        using var index = application.GetManifestResourceStream(RuntimeArchive.IndexResource);
        if (index is null) return; // Normal Debug and directory publishing.
        using var bytes = new MemoryStream();
        index.CopyTo(bytes);
        Root = RuntimeArchive.Extract(bytes.ToArray(), () => application.GetManifestResourceStream(RuntimeArchive.DataResource)
            ?? throw new InvalidDataException("Missing bundled application components."), CacheDirectory);
    }

    public static bool IsCachedPath(string? path) => !string.IsNullOrWhiteSpace(path) &&
        Path.GetFullPath(path).StartsWith(CacheDirectory.TrimEnd(Path.DirectorySeparatorChar) +
            Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);
}
