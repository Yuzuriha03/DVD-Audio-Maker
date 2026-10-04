using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using DvdaMaker.Configuration;

namespace DvdaMaker.Preparation;

/// <summary>
/// Records the inputs used to produce a successful preparation manifest. The
/// desktop workflow can therefore avoid running metadata probes and decode checks
/// twice when the user clicks “检查音源” and then “开始制作”.
/// </summary>
public sealed record PreparationSnapshot(
    int Version,
    string SourceDirectory,
    string ConfigurationFingerprint,
    IReadOnlyList<PreparationSnapshotSource> Sources,
    FileIdentity Manifest);

public sealed record PreparationSnapshotSource(
    string RelativePath,
    FileIdentity Identity);

public static class PreparationSnapshotStore
{
    private const int CurrentVersion = 1;
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNameCaseInsensitive = true,
    };

    public static IReadOnlyList<string> EnumerateSources(DvdaOptions options)
    {
        if (!Directory.Exists(options.SourceDirectory))
        {
            return [];
        }
        return Directory.EnumerateFiles(options.SourceDirectory, "*", SearchOption.AllDirectories)
            .Where(path => path.EndsWith(".flac", StringComparison.OrdinalIgnoreCase) ||
                           path.EndsWith(".m4a", StringComparison.OrdinalIgnoreCase))
            .Order(StringComparer.Ordinal)
            .ToArray();
    }

    public static bool TryReuse(
        DvdaOptions options,
        out PreparationSnapshot? snapshot,
        out string reason)
    {
        snapshot = null;
        reason = string.Empty;
        var loaded = Load(options.PrepareSnapshotPath);
        if (loaded is null)
        {
            reason = "未找到可复用的准备快照";
            return false;
        }
        if (loaded.Sources is null || loaded.Manifest is null)
        {
            reason = "准备快照内容不完整";
            return false;
        }
        if (loaded.Version != CurrentVersion)
        {
            reason = "准备快照版本已变化";
            return false;
        }
        if (!string.Equals(loaded.SourceDirectory, NormalizePath(options.SourceDirectory),
                StringComparison.OrdinalIgnoreCase))
        {
            reason = "音源目录已变化";
            return false;
        }
        if (!string.Equals(loaded.ConfigurationFingerprint, ConfigurationFingerprint(options),
                StringComparison.Ordinal))
        {
            reason = "音源检查设置已变化";
            return false;
        }
        if (!FileIdentityProbe.Matches(options.ManifestPath, loaded.Manifest))
        {
            reason = "准备清单已变化或不存在";
            return false;
        }

        var sources = EnumerateSources(options);
        if (sources.Count != loaded.Sources.Count)
        {
            reason = "音源文件数量已变化";
            return false;
        }
        var expected = new Dictionary<string, FileIdentity>(StringComparer.OrdinalIgnoreCase);
        foreach (var source in loaded.Sources)
        {
            if (!expected.TryAdd(source.RelativePath, source.Identity))
            {
                reason = "准备快照包含重复的音源路径";
                return false;
            }
        }
        foreach (var source in sources)
        {
            var relative = RelativePath(options.SourceDirectory, source);
            if (!expected.TryGetValue(relative, out var identity) ||
                !FileIdentityProbe.Matches(source, identity))
            {
                reason = $"音源文件已变化: {Path.GetFileName(source)}";
                return false;
            }
        }

        snapshot = loaded;
        reason = $"复用已确认的 {loaded.Sources.Count} 个音源检查结果";
        return true;
    }

    public static bool Save(DvdaOptions options, IReadOnlyList<string> sourcePaths)
    {
        var manifest = FileIdentityProbe.Compute(options.ManifestPath);
        if (manifest is null)
        {
            return false;
        }
        var sources = new List<PreparationSnapshotSource>(sourcePaths.Count);
        foreach (var path in sourcePaths)
        {
            var identity = FileIdentityProbe.Compute(path);
            if (identity is null)
            {
                return false;
            }
            sources.Add(new PreparationSnapshotSource(
                RelativePath(options.SourceDirectory, path),
                identity));
        }

        var snapshot = new PreparationSnapshot(
            CurrentVersion,
            NormalizePath(options.SourceDirectory),
            ConfigurationFingerprint(options),
            sources.OrderBy(source => source.RelativePath, StringComparer.Ordinal).ToArray(),
            manifest);
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(options.PrepareSnapshotPath)!);
            var temporary = options.PrepareSnapshotPath + ".tmp";
            File.WriteAllText(temporary, JsonSerializer.Serialize(snapshot, JsonOptions) + Environment.NewLine,
                new UTF8Encoding(false));
            File.Move(temporary, options.PrepareSnapshotPath, overwrite: true);
            return true;
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or JsonException)
        {
            TryDelete(options.PrepareSnapshotPath + ".tmp");
            return false;
        }
    }

    public static void Invalidate(DvdaOptions options) => TryDelete(options.PrepareSnapshotPath);

    public static string ConfigurationFingerprint(DvdaOptions options)
    {
        var values = new[]
        {
            NormalizePath(options.SourceDirectory),
            NormalizePath(options.Ffmpeg),
            NormalizePath(options.Ffprobe),
            NormalizePath(options.AlacFixDirectory),
            options.MlpSource,
            options.PrepareCacheEnabled ? "cache:on" : "cache:off",
            options.LossErrorSeconds.ToString("R", System.Globalization.CultureInfo.InvariantCulture),
            options.LossWarningSeconds.ToString("R", System.Globalization.CultureInfo.InvariantCulture),
        };
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(string.Join("\n", values))));
    }

    private static PreparationSnapshot? Load(string path)
    {
        try
        {
            if (!File.Exists(path))
            {
                return null;
            }
            return JsonSerializer.Deserialize<PreparationSnapshot>(
                File.ReadAllText(path), JsonOptions);
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or JsonException)
        {
            return null;
        }
    }

    private static string RelativePath(string root, string path) =>
        Path.GetRelativePath(NormalizePath(root), NormalizePath(path)).Replace('\\', '/');

    private static string NormalizePath(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return string.Empty;
        }
        try
        {
            return Path.GetFullPath(path);
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException)
        {
            return path.Trim();
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
