using DvdaMaker.Processes;
using System.Text;
using System.Text.Json;

namespace DvdaMaker.Configuration;

/// <summary>Portable GUI profile; importing an env never rewrites the source file.</summary>
public sealed class ProjectSettings
{
    public int Version { get; set; } = 1;
    public string Language { get; set; } = "auto";
    public Dictionary<string, string> Values { get; set; } = new(StringComparer.Ordinal);
    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DVD-Audio-Maker", "settings.json");

    public static ProjectSettings Defaults() => new()
    {
        Values = new Dictionary<string, string>(ConfigDefaults.Values, StringComparer.Ordinal),
    };

    public ProjectSettings Clone() => new() { Version = Version, Language = Language, Values = new(Values, StringComparer.Ordinal) };
    public DvdaOptions ToOptions() => DvdaOptions.FromValues(new Dictionary<string, string>(Values, StringComparer.Ordinal)
    {
        ["DVDA_FFMPEG"] = BuiltinMedia.Converter,
        ["DVDA_FFPROBE"] = BuiltinMedia.Probe,
    });

    public static ProjectSettings ImportEnv(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException("找不到配置文件。", path);
        var settings = Defaults();
        foreach (var pair in ConfigLoader.ParseFile(path)) settings.Values[pair.Key] = pair.Value;
        return settings;
    }

    public static ProjectSettings Load(string path)
        => ConfigurationFileInterop.Read("profile.load", new { Path = path, Defaults = ConfigDefaults.Values },
            path, () => LoadManaged(path));

    private static ProjectSettings LoadManaged(string path)
    {
        using var input = File.OpenRead(path);
        var settings = JsonSerializer.Deserialize<ProjectSettings>(input)
            ?? throw new InvalidDataException("配置方案为空。");
        if (settings.Version != 1 || settings.Values is null || settings.Values.Any(p => p.Value is null))
            throw new InvalidDataException("配置方案版本或内容无效。");
        var result = new ProjectSettings { Values = new(ConfigDefaults.ManagedValues, StringComparer.Ordinal) };
        result.Language = settings.Language;
        _ = DvdaMaker.Localization.L.Normalize(result.Language);
        foreach (var pair in settings.Values) result.Values[pair.Key] = pair.Value;
        return result;
    }

    public void Save(string path)
    {
        if (RustBridge.Mode == "managed") { SaveManaged(path); return; }
        if (!RustBridge.Enabled) throw new InvalidOperationException($"Unknown DVDA_RUST_MODE: {RustBridge.Mode}");
        // Mutations execute once; isolated tests compare the managed and Rust trees.
        _ = ConfigurationFileInterop.Unwrap(RustBridge.Invoke<ConfigurationFileInterop.Outcome<object?>>(
            "profile.save", new { Path = path, Profile = this }), path);
    }

    private void SaveManaged(string path)
    {
        path = Path.GetFullPath(path);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }), new UTF8Encoding(false));
            File.Move(temporary, path, overwrite: true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public IReadOnlyList<string> Validate(bool requireSource = true, bool requireEncoding = true)
    {
        var errors = new List<string>();
        var options = ToOptions();
        if (requireSource && !Directory.Exists(options.SourceDirectory)) errors.Add("请选择存在的音源目录。");
        if (string.IsNullOrWhiteSpace(options.BuildDirectory)) errors.Add("请选择构建缓存目录。");
        if (string.IsNullOrWhiteSpace(options.FinalDirectory)) errors.Add("请选择成品输出目录。");
        try
        {
            _ = options.MlpSource;
            if (options.MlpSource == "lpcm" && options.MlpSurcodeBits == 20)
                errors.Add("LPCM 编码请选择 16 或 24 位；20 位可使用 MLP 编码。");
            if (requireEncoding && options.MlpSource is ("surcode-batch" or "lpcm") && ExecutablePath.Resolve(options.Ffmpeg) is null)
                errors.Add("内置媒体组件缺失，请完整解压发布包；开发环境请准备 media-native 目录。");
            if (requireEncoding && options.MlpSource == "external" && !Directory.Exists(options.MlpExternalDirectory))
                errors.Add("导入外部 MLP 时必须选择存在的 MLP 目录。");
        }
        catch (ArgumentException exception) { errors.Add(exception.Message); }
        if (options.Get("DVDA_MLP_SOURCE").Trim().ToLowerInvariant() is ("surcode-batch" or "batch-surcode") && !string.IsNullOrWhiteSpace(options.MlpMetadataContext) && !File.Exists(options.MlpMetadataContext))
            errors.Add("指定的 MLP 元数据上下文文件不存在。");
        if (options.MlpSurcodeSampleRate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000))
            errors.Add("MLP 目标采样率无效。");
        if (options.MlpSurcodeBits is not (16 or 20 or 24)) errors.Add("MLP 目标位深必须为 16、20 或 24。");
        return errors;
    }

    public void ApplyBundledToolDefaults(string root)
    {
        var tools = Path.Combine(root, "menu-bin");
        if (File.Exists(Path.Combine(tools, "fonts", "DvdaNotoCJK-Regular.ttc")) &&
            File.Exists(Path.Combine(tools, "type-dvda-cjk.xml")))
        {
            foreach (var (key, region, fileName) in new[]
            {
                ("DVDA_MENU_FONT", "SC", "NotoSansCJKsc-Regular.otf"),
                ("DVDA_MENU_FONT_JP", "JP", "NotoSansCJKjp-Regular.otf"),
                ("DVDA_MENU_FONT_KR", "KR", "NotoSansCJKkr-Regular.otf"),
            })
                if (!Values.TryGetValue(key, out var value) || string.IsNullOrWhiteSpace(value) ||
                    value == ConfigDefaults.Values.GetValueOrDefault(key) ||
                    string.Equals(value.Replace('\\', '/'),
                        Path.Combine(tools, "fonts", fileName).Replace('\\', '/'),
                        StringComparison.OrdinalIgnoreCase))
                    Values[key] = "DVDA-Noto-Sans-CJK-" + region;
        }
        var candidates = new Dictionary<string, string>
        {
            ["DVDA_AUTHOR"] = Path.Combine(tools, "dvda-author-dev.exe"),
            ["DVDA_MENU_FONT"] = Path.Combine(tools, "fonts", "NotoSansCJKsc-Regular.otf"),
            ["DVDA_MENU_FONT_JP"] = Path.Combine(tools, "fonts", "NotoSansCJKjp-Regular.otf"),
            ["DVDA_MENU_FONT_KR"] = Path.Combine(tools, "fonts", "NotoSansCJKkr-Regular.otf"),
        };
        foreach (var pair in candidates)
            if (File.Exists(pair.Value) && (!Values.TryGetValue(pair.Key, out var value) ||
                string.IsNullOrWhiteSpace(value) || value == ConfigDefaults.Values.GetValueOrDefault(pair.Key) ||
                (BundledRuntime.Root != AppContext.BaseDirectory && BundledRuntime.IsCachedPath(value))))
                Values[pair.Key] = pair.Value;
        if (Directory.Exists(Path.Combine(root, "data", "menu")) &&
            (string.IsNullOrWhiteSpace(Values.GetValueOrDefault("DVDA_AUTHOR_SRC")) ||
                (BundledRuntime.Root != AppContext.BaseDirectory && BundledRuntime.IsCachedPath(Values.GetValueOrDefault("DVDA_AUTHOR_SRC")))))
            Values["DVDA_AUTHOR_SRC"] = Path.Combine(root, "data");
    }
}
