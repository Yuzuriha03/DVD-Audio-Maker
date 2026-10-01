using DvdaMaker.Processes;
using System.Text;
using System.Text.Json;

namespace DvdaMaker.Configuration;

/// <summary>Portable GUI profile; importing an env never rewrites the source file.</summary>
public sealed class ProjectSettings
{
    public int Version { get; set; } = 1;
    public Dictionary<string, string> Values { get; set; } = new(StringComparer.Ordinal);
    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DVD-Audio-Maker", "settings.json");

    public static ProjectSettings Defaults() => new()
    {
        Values = new Dictionary<string, string>(ConfigDefaults.Values, StringComparer.Ordinal),
    };

    public ProjectSettings Clone() => new() { Version = Version, Values = new(Values, StringComparer.Ordinal) };
    public DvdaOptions ToOptions() => DvdaOptions.FromValues(Values);

    public static ProjectSettings ImportEnv(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException("找不到配置文件。", path);
        var settings = Defaults();
        foreach (var pair in ConfigLoader.ParseFile(path)) settings.Values[pair.Key] = pair.Value;
        return settings;
    }

    public static ProjectSettings Load(string path)
    {
        using var input = File.OpenRead(path);
        var settings = JsonSerializer.Deserialize<ProjectSettings>(input)
            ?? throw new InvalidDataException("配置方案为空。");
        if (settings.Version != 1 || settings.Values is null || settings.Values.Any(p => p.Value is null))
            throw new InvalidDataException("配置方案版本或内容无效。");
        var result = Defaults();
        foreach (var pair in settings.Values) result.Values[pair.Key] = pair.Value;
        return result;
    }

    public void Save(string path)
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
            if (requireEncoding && options.MlpSource == "surcode-batch" && ExecutablePath.Resolve(options.Ffmpeg) is null)
                errors.Add("找不到 FFmpeg。请选择有效的 ffmpeg.exe 路径，或将它加入 PATH。");
            if (requireEncoding && options.MlpSource is "external" or "surcode" && !Directory.Exists(options.MlpExternalDirectory))
                errors.Add("导入外部 MLP 时必须选择存在的 MLP 目录。");
        }
        catch (ArgumentException exception) { errors.Add(exception.Message); }
        if (!string.IsNullOrWhiteSpace(options.MlpMetadataContext) && !File.Exists(options.MlpMetadataContext))
            errors.Add("指定的 MLP 元数据上下文文件不存在。");
        if (options.MlpSurcodeSampleRate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000))
            errors.Add("MLP 目标采样率无效。");
        if (options.MlpSurcodeBits is not (16 or 20 or 24)) errors.Add("MLP 目标位深必须为 16、20 或 24。");
        return errors;
    }

    public void ApplyBundledToolDefaults(string root)
    {
        var tools = Path.Combine(root, "menu-bin");
        var candidates = new Dictionary<string, string>
        {
            ["DVDA_AUTHOR"] = Path.Combine(tools, "dvda-author-dev.exe"),
            ["DVDA_MKISOFS"] = Path.Combine(tools, "mkisofs.exe"),
            ["DVDA_FFMPEG"] = Path.Combine(tools, "ffmpeg.exe"),
            ["DVDA_FFPROBE"] = Path.Combine(tools, "ffprobe.exe"),
            ["DVDA_METAFLAC"] = Path.Combine(tools, "metaflac.exe"),
            ["DVDA_MENU_FONT"] = Path.Combine(tools, "fonts", "NotoSansCJKsc-Regular.otf"),
            ["DVDA_MENU_FONT_JP"] = Path.Combine(tools, "fonts", "NotoSansCJKjp-Regular.otf"),
            ["DVDA_MENU_FONT_KR"] = Path.Combine(tools, "fonts", "NotoSansCJKkr-Regular.otf"),
        };
        foreach (var pair in candidates)
            if (File.Exists(pair.Value) && (!Values.TryGetValue(pair.Key, out var value) ||
                string.IsNullOrWhiteSpace(value) || value == ConfigDefaults.Values.GetValueOrDefault(pair.Key)))
                Values[pair.Key] = pair.Value;
        if (Directory.Exists(Path.Combine(root, "data", "menu")) &&
            string.IsNullOrWhiteSpace(Values.GetValueOrDefault("DVDA_AUTHOR_SRC")))
            Values["DVDA_AUTHOR_SRC"] = Path.Combine(root, "data");
    }
}
