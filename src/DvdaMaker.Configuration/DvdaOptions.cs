using System.Globalization;
using System.Text.RegularExpressions;

namespace DvdaMaker.Configuration;

public sealed partial class DvdaOptions
{
    private readonly IReadOnlyDictionary<string, string> _fileValues;
    private readonly IReadOnlyDictionary<string, string?> _environment;

    internal DvdaOptions(
        string? configPath,
        IReadOnlyDictionary<string, string> fileValues,
        IReadOnlyDictionary<string, string?> environment)
    {
        ConfigPath = configPath;
        _fileValues = fileValues;
        _environment = environment;
    }

    /// <summary>Creates a snapshot from explicit application settings without ambient environment overrides.</summary>
    public static DvdaOptions FromValues(IReadOnlyDictionary<string, string> values) =>
        new(null, new Dictionary<string, string>(values, StringComparer.Ordinal),
            new Dictionary<string, string?>());

    public string? ConfigPath { get; }

    public string Get(string key, string? fallback = null)
    {
        if (_environment.TryGetValue(key, out var environmentValue) &&
            !string.IsNullOrEmpty(environmentValue))
        {
            return environmentValue;
        }
        if (_fileValues.TryGetValue(key, out var fileValue) && fileValue.Length > 0)
        {
            return fileValue;
        }
        return ConfigDefaults.Values.TryGetValue(key, out var defaultValue)
            ? defaultValue
            : fallback ?? string.Empty;
    }

    public string SourceDirectory => TrimSlash(Get("DVDA_SRC"));
    public string FinalDirectory => TrimSlash(Get("DVDA_FINAL_DIR"));
    public string BuildDirectory => TrimSlash(Get("DVDA_BUILD_DIR"));
    public string Title => Get("DVDA_TITLE");
    public string IsoPrefix => Get("DVDA_ISO_PREFIX") is { Length: > 0 } prefix
        ? prefix
        : DerivePrefix(Title);
    public string ManifestPath => UnderBuild("manifest.json");
    public string ReportPath => UnderBuild("decode_report.txt");
    public string OutputRoot => UnderBuild("out");
    public string TemporaryRoot => UnderBuild("tmp");
    public string IsoDirectory => UnderBuild("iso");
    public string MlpDirectory => UnderBuild("mlp");
    public string MlpIndexPath => UnderBuild("mlp_index.json");
    public string AlacFixDirectory => UnderBuild("alacfix");
    public string MenuDirectory => UnderBuild("menu");
    public string BuildLogPath => UnderBuild("build.log");
    public string PrepareCachePath => UnderBuild("prepare-cache.json");
    public bool PrepareCacheEnabled => Get("DVDA_PREPARE_CACHE", "on").Trim().ToLowerInvariant() is not
        ("off" or "0" or "false" or "no");
    public string DvdaAuthor => Get("DVDA_AUTHOR");
    public string Mkisofs => Get("DVDA_MKISOFS");
    public string Ffmpeg => Get("DVDA_FFMPEG");
    public string Ffprobe => Get("DVDA_FFPROBE");
    public string AuthorSource => TrimSlash(Get("DVDA_AUTHOR_SRC"));
    public string MenuBinaryDirectory => CombinePortable(
        ParentDirectoryPortable(AuthorSource),
        "menu-bin");
    public int MaxDiscs => GetInt("DVDA_MAX_DISCS") ?? 0;
    public int GroupTrackLimit => Math.Clamp(
        GetInt("DVDA_GROUP_TRACK_LIMIT") ?? ConfigDefaults.GroupTrackHardLimit,
        1,
        ConfigDefaults.GroupTrackHardLimit);
    public long DiscBytes => GetLong("DVDA_DISC_BYTES") is { } value && value != 0
        ? value
        : ConfigDefaults.Dvd5Bytes;
    public string MlpSource => Get("DVDA_MLP_SOURCE").Trim().ToLowerInvariant() switch
    {
        "external" => "external",
        "surcode" => "surcode",
        "surcode-batch" or "batch-surcode" => "surcode-batch",
        "ffmpeg" => throw new ArgumentException("FFmpeg MLP 编码分支已移除，请将 DVDA_MLP_SOURCE 改为 surcode-batch 或 external。"),
        _ => throw new ArgumentException("DVDA_MLP_SOURCE 仅支持 surcode-batch、external 或 surcode（外部文件）。"),
    };
    public string MlpExternalDirectory => TrimSlash(Get("DVDA_MLP_EXTERNAL_DIR").Trim()) is { Length: > 0 } directory
        ? directory : MlpSource == "surcode-batch" ? MlpDirectory : string.Empty;
    public string MlpBatchTempDirectory => TrimSlash(Get("DVDA_MLP_BATCH_TEMP_DIR").Trim());
    public string MlpBatchOutputDirectory => TrimSlash(Get("DVDA_MLP_BATCH_OUTPUT_DIR").Trim());
    public string MlpMetadataContext => Get("DVDA_MLP_METADATA_CONTEXT").Trim();
    public string MlpEac3toExecutable => Get("DVDA_MLP_EAC3TO_EXE").Trim();
    public int MlpSurcodeSampleRate => GetInt("DVDA_MLP_SURCODE_SAMPLE_RATE") ?? 48_000;
    public int MlpSurcodeBits => GetInt("DVDA_MLP_SURCODE_BITS") ?? 24;
    public int MlpJobs => Math.Clamp(GetInt("DVDA_MLP_JOBS") ?? 1, 1, 16);
    public bool MenuEnabled => GetBool("DVDA_MENU");
    public int MenuTracksPerPage => Math.Clamp(
        GetInt("DVDA_MENU_TRACKS_PER_PAGE") ?? 12,
        1,
        32);
    public int MenuIndexMinimumAlbums => Math.Max(
        0,
        GetInt("DVDA_MENU_INDEX_MIN_ALBUMS") ?? 4);
    public bool MenuStillPictures => GetBool("DVDA_MENU_STILLPICS");
    public int MenuCoverDim => Math.Clamp(
        GetInt("DVDA_MENU_COVER_DIM") ?? 35,
        0,
        100);
    public string MenuFont => Get("DVDA_MENU_FONT").Trim();
    public string MenuFontJapanese => Get("DVDA_MENU_FONT_JP").Trim();
    public string MenuFontKorean => Get("DVDA_MENU_FONT_KR").Trim();
    public bool KeepTemporary => GetBool("DVDA_KEEP_TMP");
    public bool KeepIntermediate => GetBool("DVDA_KEEP_INTERMEDIATE");
    public bool ResumeEnabled => Get("DVDA_RESUME", "on").Trim().ToLowerInvariant() is not
        ("off" or "0" or "false" or "no");
    public double LossErrorSeconds => GetDouble("DVDA_LOSS_ERROR_S") ?? 0.05;
    public double LossWarningSeconds => GetDouble("DVDA_LOSS_WARN_S") ?? 0.005;
    public int? DiagnosticAlbumLimit
    {
        get
        {
            var value = GetInt("DVDA_ALBUM_LIMIT");
            return value is > 0 ? value : null;
        }
    }
    public string DiagnosticTitleMode
    {
        get
        {
            var value = Get("DVDA_TITLE_MODE", "album").Trim().ToLowerInvariant();
            if (value is "album" or "one") return value;
            return int.TryParse(value, out var count) && count > 0
                ? count.ToString(CultureInfo.InvariantCulture)
                : "album";
        }
    }

    public string VolumeId(int index) => $"{Title} {index}";
    public string IsoName(int index) => $"{IsoPrefix}_{index}.iso";

    public IReadOnlyList<string> MissingRequiredValues() =>
        new[] { "DVDA_SRC", "DVDA_FINAL_DIR" }
            .Where(key => Get(key).Length == 0)
            .ToArray();

    public IReadOnlyList<string> EffectiveKeys() =>
        ConfigDefaults.Values.Keys.Concat(_fileValues.Keys)
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .ToArray();

    public string ValueSource(string key)
    {
        if (_environment.TryGetValue(key, out var environmentValue) &&
            !string.IsNullOrEmpty(environmentValue))
        {
            return "环境变量";
        }
        if (_fileValues.TryGetValue(key, out var fileValue) && fileValue.Length > 0)
        {
            return Path.GetFileName(ConfigPath ?? "config.env");
        }
        return "默认值";
    }

    public bool HasEnvironmentOverrides(IEnumerable<string>? keys = null) =>
        (keys ?? EffectiveKeys()).Any(key =>
            _environment.TryGetValue(key, out var value) && !string.IsNullOrEmpty(value));

    public IReadOnlyList<KeyValuePair<string, string>> ToShellPairs() =>
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["DVDA_SRC"] = SourceDirectory,
            ["DVDA_FINAL_DIR"] = FinalDirectory,
            ["DVDA_BUILD_DIR"] = BuildDirectory,
            ["DVDA_TITLE"] = Title,
            ["DVDA_ISO_PREFIX"] = IsoPrefix,
            ["DVDA_MANIFEST"] = ManifestPath,
            ["DVDA_REPORT"] = ReportPath,
            ["DVDA_OUT_ROOT"] = OutputRoot,
            ["DVDA_TMP_ROOT"] = TemporaryRoot,
            ["DVDA_ISO_DIR"] = IsoDirectory,
            ["DVDA_MLP_DIR"] = MlpDirectory,
            ["DVDA_MLP_INDEX"] = MlpIndexPath,
            ["DVDA_ALAC_FIX_DIR"] = AlacFixDirectory,
            ["DVDA_BUILD_LOG"] = BuildLogPath,
            ["DVDA_AUTHOR"] = DvdaAuthor,
            ["DVDA_MKISOFS"] = Mkisofs,
            ["DVDA_FFMPEG"] = Ffmpeg,
            ["DVDA_FFPROBE"] = Ffprobe,
            ["DVDA_AUTHOR_SRC"] = AuthorSource,
            ["DVDA_MAX_DISCS"] = MaxDiscs.ToString(CultureInfo.InvariantCulture),
            ["DVDA_GROUP_TRACK_LIMIT"] = GroupTrackLimit.ToString(CultureInfo.InvariantCulture),
            ["DVDA_DISC_BYTES"] = DiscBytes.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MLP_SOURCE"] = MlpSource,
            ["DVDA_MLP_EXTERNAL_DIR"] = MlpExternalDirectory,
            ["DVDA_LOSS_ERROR_S"] = LossErrorSeconds.ToString(CultureInfo.InvariantCulture),
            ["DVDA_LOSS_WARN_S"] = LossWarningSeconds.ToString(CultureInfo.InvariantCulture),
        }.ToArray();

    public IReadOnlyList<KeyValuePair<string, string>> ToShellPairsAll() =>
        ToShellPairs().Concat(new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["DVDA_MLP_BATCH_TEMP_DIR"] = MlpBatchTempDirectory,
            ["DVDA_MLP_BATCH_OUTPUT_DIR"] = MlpBatchOutputDirectory,
            ["DVDA_MLP_METADATA_CONTEXT"] = MlpMetadataContext,
            ["DVDA_MLP_EAC3TO_EXE"] = MlpEac3toExecutable,
            ["DVDA_MLP_SURCODE_SAMPLE_RATE"] = MlpSurcodeSampleRate.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MLP_SURCODE_BITS"] = MlpSurcodeBits.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MENU_DIR"] = MenuDirectory,
            ["DVDA_MENU_BINDIR"] = MenuBinaryDirectory,
            ["DVDA_MENU"] = MenuEnabled ? "on" : "off",
            ["DVDA_MENU_TRACKS_PER_PAGE"] = MenuTracksPerPage.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MENU_INDEX_MIN_ALBUMS"] = MenuIndexMinimumAlbums.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MENU_STILLPICS"] = MenuStillPictures ? "on" : "off",
            ["DVDA_MENU_COVER_DIM"] = MenuCoverDim.ToString(CultureInfo.InvariantCulture),
            ["DVDA_MENU_FONT"] = MenuFont,
            ["DVDA_MENU_FONT_JP"] = MenuFontJapanese,
            ["DVDA_MENU_FONT_KR"] = MenuFontKorean,
        }).ToArray();

    private string UnderBuild(string name) => Path.Combine(BuildDirectory, name);
    private int? GetInt(string key) => int.TryParse(Get(key).Trim(), out var value) ? value : null;
    private long? GetLong(string key) => long.TryParse(Get(key).Trim(), out var value) ? value : null;
    private double? GetDouble(string key) => double.TryParse(
        Get(key).Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var value) ? value : null;
    private bool GetBool(string key) => Get(key).Trim().ToLowerInvariant() is
        "1" or "true" or "yes" or "on";
    private static string TrimSlash(string value) => value.TrimEnd('/');

    private static string ParentDirectoryPortable(string value)
    {
        var normalized = value.Replace('\\', '/').TrimEnd('/');
        var slash = normalized.LastIndexOf('/');
        if (slash < 0)
        {
            return ".";
        }
        return slash == 0 ? "/" : normalized[..slash];
    }

    private static string CombinePortable(string directory, string name)
    {
        if (directory.Contains('/'))
        {
            return directory == "/" ? "/" + name : directory.TrimEnd('/') + "/" + name;
        }
        return Path.Combine(directory, name);
    }

    private static string DerivePrefix(string title)
    {
        var value = InvalidPrefixChars().Replace(title ?? string.Empty, "_").Trim('_');
        return value.Length > 0 ? value : "DVD_Audio";
    }

    [GeneratedRegex("[^0-9A-Za-z]+")]
    private static partial Regex InvalidPrefixChars();
}
