using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

/// <summary>
/// MLP 编码核心产出的 MLP 缓存凭据。
/// 只有源文件身份、编码器身份与编码参数都一致，且 MLP 文件本身未被改动时才复用。
/// </summary>
public sealed record MlpCacheEntry
{
    [JsonPropertyName("source")] public FileIdentity? Source { get; init; }
    [JsonPropertyName("output")] public FileIdentity? Output { get; init; }
    [JsonPropertyName("encoder")] public string Encoder { get; init; } = string.Empty;
    [JsonPropertyName("bits")] public int Bits { get; init; }
    [JsonPropertyName("resample_to")] public int? ResampleTo { get; init; }
    [JsonPropertyName("max_interval")] public int MaxInterval { get; init; }
}

public sealed class MlpCacheIndex
{
    public const string FileName = "mlp-cache.json";

    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private readonly Dictionary<string, MlpCacheEntry> _entries;

    private MlpCacheIndex(Dictionary<string, MlpCacheEntry> entries) => _entries = entries;

    public int Count => _entries.Count;

    public static string PathFor(string mlpDirectory) =>
        System.IO.Path.Combine(mlpDirectory, FileName);

    public static MlpCacheIndex Load(string path)
    {
        try
        {
            if (!File.Exists(path))
            {
                return Empty();
            }
            using var stream = File.OpenRead(path);
            var entries = JsonSerializer.Deserialize<Dictionary<string, MlpCacheEntry>>(
                stream, SerializerOptions);
            return entries is null
                ? Empty()
                : new MlpCacheIndex(new Dictionary<string, MlpCacheEntry>(
                    entries, StringComparer.OrdinalIgnoreCase));
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or JsonException or
                         NotSupportedException or ArgumentException)
        {
            Console.WriteLine($"[警告] MLP 缓存索引无法读取，将重新编码: {exception.Message}");
            return Empty();
        }
    }

    /// <summary>
    /// 命中要求：索引项与源身份/编码器/参数完全一致，且磁盘上的 MLP 与记录一致。
    /// </summary>
    public MlpCacheEntry? Match(
        string mlpPath,
        FileIdentity sourceIdentity,
        string encoder,
        int bits,
        int? resampleTo,
        int maxInterval)
    {
        if (!_entries.TryGetValue(mlpPath, out var entry) ||
            entry.Source is null || entry.Output is null)
        {
            return null;
        }
        if (!string.Equals(entry.Encoder, encoder, StringComparison.Ordinal) ||
            entry.Bits != bits ||
            entry.ResampleTo != resampleTo ||
            entry.MaxInterval != maxInterval)
        {
            return null;
        }
        if (!SameIdentity(entry.Source, sourceIdentity))
        {
            return null;
        }
        return FileIdentityProbe.Matches(mlpPath, entry.Output) ? entry : null;
    }

    public void Record(string mlpPath, MlpCacheEntry entry) => _entries[mlpPath] = entry;

    public void Save(string path)
    {
        var directory = System.IO.Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(directory))
        {
            Directory.CreateDirectory(directory);
        }
        var temporary = path + ".tmp";
        File.WriteAllText(
            temporary,
            JsonSerializer.Serialize(_entries, SerializerOptions) + Environment.NewLine,
            new UTF8Encoding(false));
        File.Move(temporary, path, overwrite: true);
    }

    private static bool SameIdentity(FileIdentity left, FileIdentity right) =>
        left.Size == right.Size &&
        left.LastWriteUtcTicks == right.LastWriteUtcTicks &&
        string.Equals(left.HeadHash, right.HeadHash, StringComparison.Ordinal) &&
        string.Equals(left.TailHash, right.TailHash, StringComparison.Ordinal);

    private static MlpCacheIndex Empty() =>
        new(new Dictionary<string, MlpCacheEntry>(StringComparer.OrdinalIgnoreCase));
}

/// <summary>编码器身份：优先用可执行文件本身的大小与时间，否则退化为版本首行。</summary>
internal static class ToolIdentity
{
    public static async Task<string> DescribeAsync(
        ProcessRunner runner,
        string executable,
        CancellationToken cancellationToken)
    {
        var name = Path.GetFileName(executable);
        try
        {
            var info = new FileInfo(executable);
            if (info.Exists)
            {
                return $"{name}|{info.Length}|{info.LastWriteTimeUtc.Ticks}";
            }
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or ArgumentException)
        {
        }

        try
        {
            var result = await runner.RunAsync(new ProcessRequest
            {
                FileName = executable,
                Arguments = ["-version"],
            }, cancellationToken).ConfigureAwait(false);
            if (result.Succeeded)
            {
                var line = result.StandardOutput
                    .Split('\n', StringSplitOptions.TrimEntries)
                    .FirstOrDefault(value => value.Length > 0);
                if (!string.IsNullOrEmpty(line))
                {
                    return $"{name}|{line}";
                }
            }
        }
        catch (Exception exception) when (
            exception is InvalidOperationException or IOException or UnauthorizedAccessException)
        {
        }

        return name;
    }
}
