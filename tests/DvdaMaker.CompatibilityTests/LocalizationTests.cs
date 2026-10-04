using System.Globalization;
using DvdaMaker.Configuration;
using DvdaMaker.Localization;

internal static class LocalizationTests
{
    private static void Require(bool condition, string message) { if (!condition) throw new Exception(message); }
    public static void CatalogCoverage()
    {
        var previous = L.Language;
        try
        {
            L.SetLanguage("en");
            Require(L.Entries.Count > 900, "English message catalog is incomplete");
            foreach (var entry in L.Entries)
            {
                var values = Enumerable.Range(0, entry.Parameters).Select(i => (object)("value_" + i)).ToArray();
                var source = string.Format(CultureInfo.InvariantCulture, entry.Source, values);
                var expected = string.Format(CultureInfo.InvariantCulture, entry.English, values);
                Require(L.T(source) == expected, $"Incorrect English rendering: {entry.Source} => {L.T(source)}; expected {expected}");
            }
        }
        finally { L.SetLanguage(previous); }
    }
    public static void JapaneseCoverage()
    {
        var previous = L.Language;
        try
        {
            foreach (var alias in new[] { "ja", "ja-JP", "日本語", "Japanese" })
                Require(L.Normalize(alias) == "ja", "Japanese language alias was not recognized");
            L.SetLanguage("ja");
            Require(L.JapaneseEntries.Count == L.Entries.Count, "Japanese catalog is incomplete");
            foreach (var entry in L.JapaneseEntries)
            {
                var values = Enumerable.Range(0, entry.Parameters).Select(i => (object)("value_" + i)).ToArray();
                var source = string.Format(CultureInfo.InvariantCulture, entry.Source, values);
                var expected = string.Format(CultureInfo.InvariantCulture, entry.Translation, values);
                Require(L.T(source) == expected, $"Incorrect Japanese rendering: {entry.Source} => {L.T(source)}; expected {expected}");
            }
            const string path = @"D:\音乐\日本語アルバム\01 - 失败.flac";
            Require(L.T("源文件不存在: " + path) == "音源ファイルがありません: " + path, "Japanese localization changed a path");
            Require(L.T("[配置错误] MLP 目标位深必须为 16、20 或 24。") ==
                "[設定エラー] MLP の出力ビット深度は 16、20、24 のいずれかを指定してください。", "Nested Japanese error failed");
            Require(L.T("开始制作") == "作成開始" && L.T("LPCM 编码") == "LPCM エンコード", "Japanese primary controls missing");
        }
        finally { L.SetLanguage(previous); }
    }

    public static void OpaqueValuesAndCulture()
    {
        var previous = L.Language; var culture = CultureInfo.CurrentCulture;
        const string path = @"D:\音乐\中文专辑\01 - 失败.flac";
        try
        {
            L.SetLanguage("en");
            Require(L.T("源文件不存在: " + path) == "Source file does not exist: " + path, "A file path was translated");
            Require(L.T("[FAIL] 源文件不存在: " + path) == "[FAIL] Source file does not exist: " + path, "Nested diagnostic was not translated");
            Require(L.T("[配置错误] MLP 目标位深必须为 16、20 或 24。") == "[Configuration error] MLP target bit depth must be 16, 20 or 24.", "Nested error was not translated");
            Require(CultureInfo.CurrentCulture.Equals(culture), "UI language changed numeric or file-format culture");
            L.SetLanguage("zh-CN");
            Require(L.T("源文件不存在: " + path) == "源文件不存在: " + path, "Chinese messages changed");
        }
        finally { L.SetLanguage(previous); }
    }
    public static void ProfileAndArguments()
    {
        var path = Path.Combine(Path.GetTempPath(), "dvda-language-" + Guid.NewGuid().ToString("N") + ".json");
        try
        {
            var settings = ProjectSettings.Defaults(); settings.Language = "en";
            settings.Values["DVDA_TITLE"] = "我的音乐";
            settings.Save(path);
            var loaded = ProjectSettings.Load(path);
            Require(loaded.Language == "en" && loaded.Clone().Language == "en", "Language preference was not retained");
            Require(loaded.Values.SequenceEqual(settings.Values), "Language preference changed pipeline options");
            settings.Language = "ja"; settings.Save(path);
            Require(ProjectSettings.Load(path).Language == "ja", "Japanese preference was not retained");
            File.WriteAllText(path, "{\"Version\":1,\"Values\":{}}");
            Require(ProjectSettings.Load(path).Language == "auto", "Existing profiles lost language defaults");
            var arguments = new List<string> { "config", "--language", "en", "--config", "custom.env" };
            Require(L.TakeLanguage(arguments) == "en" && arguments.SequenceEqual(["config", "--config", "custom.env"]), "Language parsing consumed unrelated arguments");
            try { L.TakeLanguage(["--language", "en", "--language", "zh-CN"]); throw new Exception("Duplicate language accepted"); }
            catch (ArgumentException) { }
            try { L.TakeLanguage(["--language"]); throw new Exception("Missing language accepted"); }
            catch (ArgumentException) { }
        }
        finally { File.Delete(path); }
    }
}
