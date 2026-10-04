using DvdaMaker.Configuration;

internal static class GuiConfigurationTests
{
    private static void Require(bool value, string message) { if (!value) throw new Exception(message); }
    public static void ProfileRoundtrip()
    {
        ConfigurationFileMigrationTests.Profiles();
        var root = Path.Combine(Path.GetTempPath(), "dvda-profile-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var settings = ProjectSettings.Defaults();
            settings.Values["DVDA_TITLE"] = "中文音乐 / 日本語";
            settings.Values["CUSTOM_FUTURE_OPTION"] = "value # quoted \"text\"";
            settings.Values["DVDA_MLP_JOBS"] = "4";
            var path = Path.Combine(root, "project.json"); settings.Save(path);
            var loaded = ProjectSettings.Load(path);
            Require(loaded.Values.SequenceEqual(settings.Values), "JSON profile did not preserve all settings");
            loaded.Values["DVDA_TITLE"] = "Changed";
            Require(settings.Values["DVDA_TITLE"] != loaded.Values["DVDA_TITLE"], "Profile snapshots share mutable values");
            loaded.Save(path);
            Require(ProjectSettings.Load(path).Values["DVDA_TITLE"] == "Changed", "Atomic replacement did not persist");
            Require(!Directory.EnumerateFiles(root, "*.tmp").Any(), "Profile save leaked a temporary file");
            File.WriteAllText(path, "{\"Version\":99,\"Values\":{}}");
            try { ProjectSettings.Load(path); throw new Exception("Unsupported profile version accepted"); }
            catch (InvalidDataException) { }
        }
        finally { Directory.Delete(root, true); }
    }
    public static void ImportEnv()
    {
        var path = Path.Combine(Path.GetTempPath(), "dvda-env-" + Guid.NewGuid().ToString("N") + ".env");
        const string original = "DVDA_TITLE=\"音频项目\"\nDVDA_MLP_SOURCE=batch-surcode\nCUSTOM_KEY='keep # me'\n";
        try
        {
            File.WriteAllText(path, original);
            var settings = ProjectSettings.ImportEnv(path);
            Require(settings.ToOptions().Title == "音频项目", "Legacy title import failed");
            Require(settings.ToOptions().MlpSource == "surcode-batch", "Legacy source alias failed");
            Require(settings.Values["CUSTOM_KEY"] == "keep # me", "Unknown imported key lost");
            Require(File.ReadAllText(path) == original, "Import modified legacy file");
            File.WriteAllText(path, "DVDA_MLP_SOURCE=SURCODE");
            var legacy = ProjectSettings.ImportEnv(path);
            Require(legacy.ToOptions().MlpSource == "external", "Removed SurCode mode was not mapped to generic import");
            var profile = path + ".json";
            try { legacy.Save(profile); Require(ProjectSettings.Load(profile).ToOptions().MlpSource == "external", "Legacy JSON import changed source"); }
            finally { File.Delete(profile); }
        }
        finally { File.Delete(path); }
    }
    public static void ExplicitValuesWin()
    {
        var old = Environment.GetEnvironmentVariable("DVDA_MLP_JOBS");
        try
        {
            Environment.SetEnvironmentVariable("DVDA_MLP_JOBS", "16");
            var settings = ProjectSettings.Defaults(); settings.Values["DVDA_MLP_JOBS"] = "2";
            var options = settings.ToOptions(); settings.Values["DVDA_MLP_JOBS"] = "3";
            Require(options.MlpJobs == 2, "GUI options were overridden by environment or mutable settings");
            Require(!options.HasEnvironmentOverrides(), "GUI snapshot has hidden environment overrides");
        }
        finally { Environment.SetEnvironmentVariable("DVDA_MLP_JOBS", old); }
    }
}
