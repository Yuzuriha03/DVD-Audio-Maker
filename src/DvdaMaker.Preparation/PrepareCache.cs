using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace DvdaMaker.Preparation;

/// <summary>ffprobe 探测到的原始参数，用于免去重复探测。</summary>
public sealed record AudioProbeFacts
{
    [JsonPropertyName("sr")] public int SampleRate { get; init; }
    [JsonPropertyName("bits")] public int Bits { get; init; }
    [JsonPropertyName("ch")] public int Channels { get; init; }
    [JsonPropertyName("date")] public string Date { get; init; } = string.Empty;
    [JsonPropertyName("track")] public string Track { get; init; } = string.Empty;
    [JsonPropertyName("title")] public string Title { get; init; } = string.Empty;
    [JsonPropertyName("album")] public string Album { get; init; } = string.Empty;
    [JsonPropertyName("dur")] public double Duration { get; init; }
}

/// <summary>一次通过的解码校验结论。只有完全通过（无错误、采样数吻合）才会记录。</summary>
public sealed record PrepareValidationFacts
{
    [JsonPropertyName("sr")] public int SampleRate { get; init; }
    [JsonPropertyName("bits")] public int Bits { get; init; }
    [JsonPropertyName("resample_to")] public int? ResampleTo { get; init; }
    [JsonPropertyName("expected")] public long? ExpectedSamples { get; init; }
    [JsonPropertyName("decoded")] public long DecodedSamples { get; init; }
    [JsonPropertyName("patches")] public IReadOnlyList<AlacFramePatch>? Patches { get; init; }
    [JsonPropertyName("repaired_file")] public FileIdentity? RepairedFile { get; init; }
}

public sealed record PrepareCacheEntry
{
    [JsonPropertyName("identity")] public FileIdentity Identity { get; init; } = new("", 0, 0, "", "");
    [JsonPropertyName("probe")] public AudioProbeFacts Probe { get; init; } = new();
    [JsonPropertyName("validation")] public PrepareValidationFacts Validation { get; init; } = new();
}

/// <summary>
/// prepare 阶段的可复现缓存：路径 -&gt; 探测结果 + 已通过的解码校验结论。
/// 只有文件身份（长度/时间/首尾哈希）完全一致时才复用，源文件变化或校验未通过一律重新校验。
/// </summary>
public sealed class PrepareCache
{
    public const string FileName = "prepare-cache.json";

    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private readonly Dictionary<string, PrepareCacheEntry> _entries;

    private PrepareCache(Dictionary<string, PrepareCacheEntry> entries) => _entries = entries;

    public int Count => _entries.Count;

    public static PrepareCache Load(string path)
    {
        try
        {
            if (!File.Exists(path))
            {
                return Empty();
            }
            using var stream = File.OpenRead(path);
            var entries = JsonSerializer.Deserialize<Dictionary<string, PrepareCacheEntry>>(
                stream, SerializerOptions);
            return entries is null
                ? Empty()
                : new PrepareCache(new Dictionary<string, PrepareCacheEntry>(
                    entries, StringComparer.OrdinalIgnoreCase));
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or JsonException or
                         NotSupportedException or ArgumentException)
        {
            Console.WriteLine($"[警告] 准备缓存无法读取，将全部重新校验: {exception.Message}");
            return Empty();
        }
    }

    public PrepareCacheEntry? Match(string sourcePath) =>
        _entries.TryGetValue(sourcePath, out var entry) &&
        FileIdentityProbe.Matches(sourcePath, entry.Identity)
            ? entry
            : null;

    public void Record(string sourcePath, PrepareCacheEntry entry) =>
        _entries[sourcePath] = entry;

    public void Save(string path)
    {
        var directory = Path.GetDirectoryName(path);
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

    private static PrepareCache Empty() =>
        new(new Dictionary<string, PrepareCacheEntry>(StringComparer.OrdinalIgnoreCase));
}
