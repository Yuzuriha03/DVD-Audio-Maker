using System.Globalization;
using System.Reflection;
using DvdaMaker.Configuration;

internal static class OptionMigrationTests
{
    private static void Require(bool value, string message)
    {
        if (!value) throw new Exception(message);
    }

    private static void Exercise(Dictionary<string, string> values)
    {
        var options = DvdaOptions.FromValues(values);
        foreach (var property in typeof(DvdaOptions).GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            try { _ = property.GetValue(options); }
            catch (TargetInvocationException error) when (error.InnerException is ArgumentException) { }
        }
        Require(options.EffectiveKeys().SequenceEqual(options.EffectiveKeys().Order(StringComparer.Ordinal)),
            "Effective keys must use UTF-16 ordinal ordering");
    }

    internal static void Run()
    {
        foreach (var text in new[] { "", " 	", "0", "-0", "+0001", "99", "-99", "2147483647",
            "2147483648", "-2147483648", "-2147483649", "9223372036854775807", "-9223372036854775808",
            "9223372036854775808", "-9223372036854775809", "１２", "١٢", "1,000", "1.5",
            "1e2", "bad", "12\0\0", "12 \0", "12\0 	", "12\0x", "+ 1", " +17　" })
        {
            var values = new Dictionary<string, string>();
            foreach (var key in new[] { "DVDA_MAX_DISCS", "DVDA_DISC_BYTES", "DVDA_GROUP_TRACK_LIMIT",
                "DVDA_MLP_SURCODE_SAMPLE_RATE", "DVDA_MLP_SURCODE_BITS", "DVDA_MLP_JOBS",
                "DVDA_MENU_TRACKS_PER_PAGE", "DVDA_MENU_INDEX_MIN_ALBUMS", "DVDA_MENU_COVER_DIM",
                "DVDA_ALBUM_LIMIT", "DVDA_TITLE_MODE" }) values[key] = text;
            Exercise(values);
        }
        foreach (var text in new[] { "0", "-0", "+0", "1.25", ".25", "1.", "1e-9", "1e5000", "-1e5000",
            "1e-5000", "-1e-5000", "1,5", "NaN", "-NaN", "+NaN", "Infinity", "-Infinity",
            "+Infinity", "inf", "-inf", "-nAn", "NaN\0", "Infinity\0", "12\0\0", "12 \0",
            "12\0 	", " 0.25　", "１２.５", "-", " " })
            Exercise(new() { ["DVDA_LOSS_ERROR_S"] = text, ["DVDA_LOSS_WARN_S"] = text });
        foreach (var path in new[] { "", "/", @"C:", @"C:/", @"C:\音楽\", "/opt/dvda-author///",
            "relative", "a/b", @"a\b", @"\\server\share\日本語", " 雨 🎵 / " })
            Exercise(new() { ["DVDA_SRC"] = path, ["DVDA_BUILD_DIR"] = path, ["DVDA_AUTHOR_SRC"] = path,
                ["DVDA_MLP_EXTERNAL_DIR"] = path, ["DVDA_MLP_BATCH_TEMP_DIR"] = path });
        foreach (var source in new[] { "SURCODE", "external", "surcode-batch", "batch-surcode", "lpcm", "ffmpeg", "unknown" })
        {
            Exercise(new() { ["DVDA_MLP_SOURCE"] = source });
            var options = DvdaOptions.FromValues(new Dictionary<string, string> { ["DVDA_MLP_SOURCE"] = source,
                ["DVDA_MLP_EXTERNAL_DIR"] = "D:/import", ["DVDA_TITLE"] = "Deferred" });
            Require(options.Title == "Deferred" && options.MlpExternalDirectory == "D:/import", "Unrelated options must not throw");
        }
        foreach (var flag in new[] { "ON", " Off 	", "true", "FALSE", "1", "0", "yes", "no", "unknown" })
            Exercise(new() { ["DVDA_MENU"] = flag, ["DVDA_KEEP_TMP"] = flag, ["DVDA_PREPARE_CACHE"] = flag, ["DVDA_RESUME"] = flag });
        var previous = CultureInfo.CurrentCulture;
        try
        {
            foreach (var name in new[] { "en-US", "zh-CN", "ja-JP", "fr-FR", "sv-SE", "ar-SA", "fa-IR" })
            {
                var culture = CultureInfo.GetCultureInfo(name);
                CultureInfo.CurrentCulture = culture;
                foreach (var text in new[] { "-3", "+3", culture.NumberFormat.NegativeSign + "3", culture.NumberFormat.PositiveSign + "3" })
                    Exercise(new() { ["DVDA_MAX_DISCS"] = text, ["DVDA_DISC_BYTES"] = text, ["DVDA_TITLE_MODE"] = text });
            }
            var custom = (CultureInfo)CultureInfo.InvariantCulture.Clone();
            custom.NumberFormat.PositiveSign = "pos"; custom.NumberFormat.NegativeSign = "neg";
            var options = DvdaOptions.FromValues(new Dictionary<string, string> { ["DVDA_MAX_DISCS"] = "neg3" });
            CultureInfo.CurrentCulture = custom;
            Require(options.MaxDiscs == -3, "Custom number signs");
            CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
            Require(options.MaxDiscs == 0, "Culture change must invalidate evaluated options");
        }
        finally { CultureInfo.CurrentCulture = previous; }
        var environment = new Dictionary<string, string?> { ["ONLY_ENV"] = "exists", ["EMPTY_ENV"] = "",
            ["NULL_ENV"] = null, ["DVDA_TITLE"] = "first" };
        var loaded = new ConfigLoader(environment).Load(Path.Combine(Path.GetTempPath(), "missing-options-" + Guid.NewGuid() + ".env"));
        Require(loaded.Get("EMPTY_ENV", "fallback") == "fallback" && loaded.Get("NULL_ENV", "fallback") == "fallback", "Empty override fallback");
        Require(loaded.Get("ONLY_ENV") == "exists" && !loaded.EffectiveKeys().Contains("ONLY_ENV"), "Environment-only key visibility");
        Require(loaded.HasEnvironmentOverrides(["ONLY_ENV"]) && loaded.ValueSource("ONLY_ENV") == "环境变量", "Override source");
        Require(loaded.Get("DVDA_SRC", "fallback") == "" && loaded.ValueSource("UNSET") == "默认值", "Defined empty default overrides fallback");
        Require(loaded.Title == "first", "Initial environment");
        environment["DVDA_TITLE"] = "second";
        Require(loaded.Title == "second", "Explicit environment dictionary remains live");
        Exercise(new() { [""] = "private", ["𐀀"] = "supplementary", ["CUSTOM_EMPTY"] = "" });
    }
}
