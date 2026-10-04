using System.Security.Cryptography;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using DvdaMaker.Configuration;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record DiscResumeEntry
{
    [JsonPropertyName("signature")] public string Signature { get; init; } = string.Empty;
    [JsonPropertyName("iso")] public FileIdentity? Iso { get; init; }
    [JsonPropertyName("created")] public string Created { get; init; } = string.Empty;
}

/// <summary>
/// 逐盘续跑凭据。只有签名（源/MLP 身份 + 工具身份 + 影响输出的配置）一致，
/// 且暂存 ISO 与记录身份一致时才复用；否则丢弃该盘的暂存产物重新出盘。
/// 它只影响“是否需要重新出盘”，最终仍由整套事务发布保证索引与 ISO 一致。
/// </summary>
public sealed class DiscResumeStore
{
    public const string DirectoryName = "publish-staging";
    public const string IndexFileName = "resume.json";

    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    private readonly Dictionary<int, DiscResumeEntry> _entries;
    private readonly string _directory;
    private readonly string _indexPath;

    private DiscResumeStore(string directory, Dictionary<int, DiscResumeEntry> entries)
    {
        _directory = directory;
        _indexPath = Path.Combine(directory, IndexFileName);
        _entries = entries;
    }

    public static string DirectoryFor(DvdaOptions options) =>
        Path.Combine(options.BuildDirectory, DirectoryName);

    public static DiscResumeStore Load(string directory)
    {
        var indexPath = Path.Combine(directory, IndexFileName);
        try
        {
            if (!File.Exists(indexPath))
            {
                return new DiscResumeStore(directory, []);
            }
            using var stream = File.OpenRead(indexPath);
            var entries = JsonSerializer.Deserialize<Dictionary<int, DiscResumeEntry>>(
                stream, SerializerOptions);
            return new DiscResumeStore(directory, entries is null ? [] : new Dictionary<int, DiscResumeEntry>(entries));
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or JsonException or
                         NotSupportedException or ArgumentException)
        {
            Console.WriteLine($"[警告] 续跑记录无法读取，将重新出盘: {exception.Message}");
            return new DiscResumeStore(directory, []);
        }
    }

    public int Count => _entries.Count;

    public string IsoPath(string isoName) => Path.Combine(_directory, isoName);

    public DiscResumeEntry? TryReuse(int discNumber, string signature, string isoName)
    {
        if (!_entries.TryGetValue(discNumber, out var entry) ||
            entry.Iso is null)
        {
            return null;
        }
        var signatureMatches = RustBridge.Run<bool>("resume.signature_match", new
        {
            Entry = entry,
            Signature = signature,
        }, () => string.Equals(entry.Signature, signature, StringComparison.Ordinal));
        if (!signatureMatches)
        {
            return null;
        }
        return FileIdentityProbe.Matches(IsoPath(isoName), entry.Iso) ? entry : null;
    }

    public void Record(int discNumber, string signature, string isoName)
    {
        var identity = FileIdentityProbe.Compute(IsoPath(isoName));
        if (identity is null)
        {
            _entries.Remove(discNumber);
            return;
        }
        _entries[discNumber] = new DiscResumeEntry
        {
            Signature = signature,
            Iso = identity,
            Created = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss"),
        };
    }

    /// <summary>签名不符或产物缺失时，丢弃该盘的暂存产物与记录。</summary>
    public void Discard(int discNumber, string isoName)
    {
        _entries.Remove(discNumber);
        try
        {
            var path = IsoPath(isoName);
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException)
        {
        }
    }

    public void Save()
    {
        Directory.CreateDirectory(_directory);
        var temporary = _indexPath + ".tmp";
        File.WriteAllText(
            temporary,
            JsonSerializer.Serialize(_entries, SerializerOptions) + Environment.NewLine,
            new UTF8Encoding(false));
        File.Move(temporary, _indexPath, overwrite: true);
    }
}

public static class DiscSignature
{
    public static async Task<string> ComputeAsync(
        DvdaOptions options,
        DiscPlan disc,
        ProcessRunner runner,
        CancellationToken cancellationToken = default)
    {
        var builder = new StringBuilder();
        builder.Append("disc=").Append(disc.Number).Append('\n');
        builder.Append("volid=").Append(options.VolumeId(disc.Number)).Append('\n');
        builder.Append("iso=").Append(options.IsoName(disc.Number)).Append('\n');
        builder.Append("title=").Append(options.Title).Append('\n');
        builder.Append("title_mode=").Append(options.DiagnosticTitleMode).Append('\n');
        builder.Append("group_limit=").Append(options.GroupTrackLimit).Append('\n');
        builder.Append("author=").Append(options.DvdaAuthor).Append('|')
            .Append(await ToolIdentity.DescribeAsync(runner, options.DvdaAuthor, cancellationToken)
                .ConfigureAwait(false))
            .Append('\n');
        builder.Append("iso_writer=in-process-c-v2\n");
        // The author's dynamically loaded algorithms can change independently
        // of its executable. Never resume an ISO made with different DLLs.
        if (Directory.Exists(options.MenuBinaryDirectory))
        {
            foreach (var file in Directory.EnumerateFiles(options.MenuBinaryDirectory, "*.dll")
                .Order(StringComparer.Ordinal))
            {
                cancellationToken.ThrowIfCancellationRequested();
                using var stream = File.OpenRead(file);
                builder.Append("author_library=").Append(Path.GetFileName(file)).Append('|')
                    .Append(Convert.ToHexString(SHA256.HashData(stream))).Append('\n');
            }
        }
        builder.Append("menu=").Append(options.MenuEnabled).Append('|')
            .Append(options.MenuTracksPerPage).Append('|')
            .Append(options.MenuIndexMinimumAlbums).Append('|')
            .Append(options.MenuStillPictures).Append('|')
            .Append(options.MenuCoverDim).Append('|')
            .Append(options.MenuFont).Append('|')
            .Append(options.MenuFontJapanese).Append('|')
            .Append(options.MenuFontKorean).Append('\n');
        if (options.MenuEnabled && BuiltinImages.IsAvailable)
        {
            // Rebuild staged menus when their in-process renderer/configuration changes.
            foreach (var file in Directory.EnumerateFiles(Path.GetDirectoryName(BuiltinImages.LibraryPath)!)
                .Where(path => Path.GetExtension(path) is ".dll" or ".xml").Order(StringComparer.Ordinal))
            {
                using var stream = File.OpenRead(file);
                builder.Append("image=").Append(Path.GetFileName(file)).Append('|')
                    .Append(Convert.ToHexString(SHA256.HashData(stream))).Append('\n');
            }
        }

        foreach (var track in disc.Tracks)
        {
            var identity = FileIdentityProbe.Compute(track.MlpPath);
            builder.Append("track=").Append(track.Title).Append('|')
                .Append(track.ManifestName).Append('|')
                .Append(track.SampleRate).Append('|')
                .Append(track.Bits).Append('|')
                .Append(track.MlpSource).Append('|')
                .Append(track.Channels).Append('|')
                .Append(track.ChannelMask).Append('|')
                .Append(track.MlpPath).Append('|')
                .Append(track.MlpSize).Append('|');
            if (identity is null)
            {
                builder.Append("missing");
            }
            else
            {
                builder.Append(identity.Size).Append('|')
                    .Append(identity.LastWriteUtcTicks).Append('|')
                    .Append(identity.HeadHash).Append('|')
                    .Append(identity.TailHash);
            }
            builder.Append('\n');
        }

        return Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(builder.ToString())));
    }
}
