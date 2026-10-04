using System.Globalization;

namespace DvdaMaker.Configuration;

/// <summary>Rust evaluation with an independent managed reference during migration.</summary>
public sealed class DvdaOptions
{
    private readonly string? _configPath;
    private readonly IReadOnlyDictionary<string, string> _fileValues;
    private readonly IReadOnlyDictionary<string, string?> _environment;
    private readonly object _gate = new();
    private Dictionary<string, string>? _previousFile;
    private Dictionary<string, string?>? _previousEnvironment;
    private OptionEvaluation? _evaluation;
    private string? _positiveSign, _negativeSign, _mode;

    internal DvdaOptions(string? configPath, IReadOnlyDictionary<string, string> fileValues,
        IReadOnlyDictionary<string, string?> environment)
    {
        _configPath = configPath;
        _fileValues = fileValues;
        _environment = environment;
    }

    public static DvdaOptions FromValues(IReadOnlyDictionary<string, string> values) =>
        new(null, new Dictionary<string, string>(values, StringComparer.Ordinal), new Dictionary<string, string?>());

    private OptionEvaluation Evaluation
    {
        get
        {
            lock (_gate)
            {
                var number = CultureInfo.CurrentCulture.NumberFormat;
                var mode = DvdaMaker.Processes.RustBridge.Mode;
                if (_evaluation is null || _mode != mode || _positiveSign != number.PositiveSign || _negativeSign != number.NegativeSign ||
                    _previousFile is null || !_fileValues.SequenceEqual(_previousFile) ||
                    _previousEnvironment is null || !_environment.SequenceEqual(_previousEnvironment))
                {
                    var file = new Dictionary<string, string>(_fileValues, StringComparer.Ordinal);
                    var environment = new Dictionary<string, string?>(_environment, StringComparer.Ordinal);
                    var result = OptionEvaluation.Evaluate(_configPath, file, environment);
                    _previousFile = file;
                    _previousEnvironment = environment;
                    _evaluation = result;
                    _positiveSign = number.PositiveSign; _negativeSign = number.NegativeSign; _mode = mode;
                }
                return _evaluation;
            }
        }
    }

    public string? ConfigPath => _configPath;
    public string Get(string key, string? fallback = null)
    {
        var evaluation = Evaluation;
        if (evaluation.Raw.TryGetValue(key, out var value) &&
            (value.Length > 0 || ConfigDefaults.Values.ContainsKey(key))) return value;
        return fallback ?? string.Empty;
    }

    public string SourceDirectory => Evaluation.Read<string>(nameof(SourceDirectory));
    public string FinalDirectory => Evaluation.Read<string>(nameof(FinalDirectory));
    public string BuildDirectory => Evaluation.Read<string>(nameof(BuildDirectory));
    public string Title => Evaluation.Read<string>(nameof(Title));
    public string IsoPrefix => Evaluation.Read<string>(nameof(IsoPrefix));
    public string ManifestPath => Evaluation.Read<string>(nameof(ManifestPath));
    public string ReportPath => Evaluation.Read<string>(nameof(ReportPath));
    public string OutputRoot => Evaluation.Read<string>(nameof(OutputRoot));
    public string TemporaryRoot => Evaluation.Read<string>(nameof(TemporaryRoot));
    public string IsoDirectory => Evaluation.Read<string>(nameof(IsoDirectory));
    public string MlpDirectory => Evaluation.Read<string>(nameof(MlpDirectory));
    public string MlpIndexPath => Evaluation.Read<string>(nameof(MlpIndexPath));
    public string AlacFixDirectory => Evaluation.Read<string>(nameof(AlacFixDirectory));
    public string MenuDirectory => Evaluation.Read<string>(nameof(MenuDirectory));
    public string BuildLogPath => Evaluation.Read<string>(nameof(BuildLogPath));
    public string PrepareCachePath => Evaluation.Read<string>(nameof(PrepareCachePath));
    public string PrepareSnapshotPath => Evaluation.Read<string>(nameof(PrepareSnapshotPath));
    public bool PrepareCacheEnabled => Evaluation.Read<bool>(nameof(PrepareCacheEnabled));
    public string DvdaAuthor => Evaluation.Read<string>(nameof(DvdaAuthor));
    public string Mkisofs => Evaluation.Read<string>(nameof(Mkisofs));
    public string Ffmpeg => Evaluation.Read<string>(nameof(Ffmpeg));
    public string Ffprobe => Evaluation.Read<string>(nameof(Ffprobe));
    public string AuthorSource => Evaluation.Read<string>(nameof(AuthorSource));
    public string MenuBinaryDirectory => Evaluation.Read<string>(nameof(MenuBinaryDirectory));
    public int MaxDiscs => Evaluation.Read<int>(nameof(MaxDiscs));
    public int GroupTrackLimit => Evaluation.Read<int>(nameof(GroupTrackLimit));
    public long DiscBytes => Evaluation.Read<long>(nameof(DiscBytes));
    public string MlpSource => Evaluation.Read<string>(nameof(MlpSource));
    public string MlpExternalDirectory => Evaluation.Read<string>(nameof(MlpExternalDirectory));
    public string MlpBatchTempDirectory => Evaluation.Read<string>(nameof(MlpBatchTempDirectory));
    public string MlpBatchOutputDirectory => Evaluation.Read<string>(nameof(MlpBatchOutputDirectory));
    public string MlpMetadataContext => Evaluation.Read<string>(nameof(MlpMetadataContext));
    public string MlpEac3toExecutable => Evaluation.Read<string>(nameof(MlpEac3toExecutable));
    public int MlpSurcodeSampleRate => Evaluation.Read<int>(nameof(MlpSurcodeSampleRate));
    public int MlpSurcodeBits => Evaluation.Read<int>(nameof(MlpSurcodeBits));
    public int MlpJobs => Evaluation.Read<int>(nameof(MlpJobs));
    public bool MenuEnabled => Evaluation.Read<bool>(nameof(MenuEnabled));
    public int MenuTracksPerPage => Evaluation.Read<int>(nameof(MenuTracksPerPage));
    public int MenuIndexMinimumAlbums => Evaluation.Read<int>(nameof(MenuIndexMinimumAlbums));
    public bool MenuStillPictures => Evaluation.Read<bool>(nameof(MenuStillPictures));
    public int MenuCoverDim => Evaluation.Read<int>(nameof(MenuCoverDim));
    public string MenuFont => Evaluation.Read<string>(nameof(MenuFont));
    public string MenuFontJapanese => Evaluation.Read<string>(nameof(MenuFontJapanese));
    public string MenuFontKorean => Evaluation.Read<string>(nameof(MenuFontKorean));
    public bool KeepTemporary => Evaluation.Read<bool>(nameof(KeepTemporary));
    public bool KeepIntermediate => Evaluation.Read<bool>(nameof(KeepIntermediate));
    public bool ResumeEnabled => Evaluation.Read<bool>(nameof(ResumeEnabled));
    public double LossErrorSeconds => Evaluation.Read<double>(nameof(LossErrorSeconds));
    public double LossWarningSeconds => Evaluation.Read<double>(nameof(LossWarningSeconds));
    public int? DiagnosticAlbumLimit => Evaluation.Read<int?>(nameof(DiagnosticAlbumLimit));
    public string DiagnosticTitleMode => Evaluation.Read<string>(nameof(DiagnosticTitleMode));

    public string VolumeId(int index) => $"{Title} {index}";
    public string IsoName(int index) => $"{IsoPrefix}_{index}.iso";
    public IReadOnlyList<string> MissingRequiredValues() => Evaluation.MissingRequired.ToArray();
    public IReadOnlyList<string> EffectiveKeys() => Evaluation.EffectiveKeys.ToArray();
    public string ValueSource(string key) => Evaluation.Sources.GetValueOrDefault(key, "默认值");
    public bool HasEnvironmentOverrides(IEnumerable<string>? keys = null)
    {
        var evaluation = Evaluation;
        return (keys ?? evaluation.EffectiveKeys).Any(key => evaluation.Overrides.Contains(key, StringComparer.Ordinal));
    }

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

}
