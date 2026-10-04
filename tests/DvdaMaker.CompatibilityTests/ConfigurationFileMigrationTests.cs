using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using DvdaMaker.Configuration;
using DvdaMaker.Processes;

internal static class ConfigurationFileMigrationTests
{
    private static void Require(bool value, string message)
    {
        if (!value) throw new Exception(message);
    }

    internal static void EnvFiles()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-env-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var path = Path.Combine(root, "config.env");
        try
        {
            const string input = "# ignored\r\nDVDA_TITLE='雨 日本語 🎵'\rCUSTOM_VALUE=x # note\nDVDA_SRC=C:\\Music\\Albums\nEMPTY=\n";
            foreach (var encoding in new Encoding[] { new UTF8Encoding(false), new UTF8Encoding(true),
                new UnicodeEncoding(false, true), new UnicodeEncoding(true, true),
                new UTF32Encoding(false, true), new UTF32Encoding(true, true) })
            {
                File.WriteAllBytes(path, encoding.GetPreamble().Concat(encoding.GetBytes(input)).ToArray());
                var result = ConfigLoader.ParseFile(path);
                Require(result["DVDA_TITLE"] == "雨 日本語 🎵" && result["DVDA_SRC"] == "C:/Music/Albums",
                    "BOM decoding changed legacy configuration");
            }
            foreach (var malformed in new byte[][] { [0x80], [0xE1, 0x80], [0xED, 0xA0, 0x80], [0xF4, 0x90, 0x80, 0x80] })
            {
                File.WriteAllBytes(path, "CUSTOM="u8.ToArray().Concat(malformed).ToArray());
                _ = ConfigLoader.ParseFile(path);
            }
            foreach (var malformed in new byte[][] { [0xFF, 0xFE, 0x41], [0xFF, 0xFE, 0x00, 0xD8, 0x41],
                [0xFF, 0xFE, 0, 0, 0x41], [0xFF, 0xFE, 0, 0, 0, 0, 0x11, 0] })
            {
                File.WriteAllBytes(path, malformed);
                _ = ConfigLoader.ParseFile(path);
            }
            File.WriteAllText(path, input);
            using (var locked = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                try { _ = ConfigLoader.ParseFile(path); throw new Exception("Locked env accepted"); }
                catch (IOException error) { Require((error.HResult & 0xffff) == 32, "Env sharing violation changed"); }
            }
            var loader = new ConfigLoader(new Dictionary<string, string?> { ["DVDA_CONFIG"] = "missing-override.env" });
            Require(loader.FindConfigPath("explicit.env", root) == "explicit.env", "Explicit config priority");
            Require(loader.FindConfigPath(null, root) == "missing-override.env", "Environment config need not exist");
            var plain = new ConfigLoader(new Dictionary<string, string?>());
            var chosen = plain.FindConfigPath(null, root);
            Require(chosen == path || chosen == Path.Combine(AppContext.BaseDirectory, "config.env"), "Default config search");
            File.Delete(path);
            foreach (var missing in new string?[] { null, "", path, root })
                Require(ConfigLoader.ParseFile(missing).Count == 0, "Missing/non-file config must be empty");
        }
        finally { Directory.Delete(root, true); }
    }

    internal static void Profiles()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-profiles-中文-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var path = Path.Combine(root, "settings.json");
        try
        {
            var inputs = new (string Text, Type? Error)[]
            {
                ("{}", null), ("{\"version\":99,\"values\":null}", null),
                ("{\"Version\":1,\"Language\":null,\"Values\":{\"A\":\"雨\",\"A\":\"last\"}}", null),
                ("{\"Version\":99,\"Version\":1,\"Language\":\"ja\",\"Language\":\"en\"}", null),
                ("{\"Values\":{\"A\":\"first\"},\"Values\":{\"B\":\"last\"}}", null),
                ("null", typeof(InvalidDataException)), ("{\"Version\":99}", typeof(InvalidDataException)),
                ("{\"Values\":null}", typeof(InvalidDataException)),
                ("{\"Values\":{\"A\":null}}", typeof(InvalidDataException)),
                ("{\"Language\":\"unsupported\"}", typeof(ArgumentException)),
                ("{\"Version\":null}", typeof(JsonException)), ("{\"Version\":1.0}", typeof(JsonException)),
                ("{\"Version\":2147483648}", typeof(JsonException)), ("{\"Language\":1}", typeof(JsonException)),
                ("{\"Values\":{\"A\":2,\"A\":\"valid\"}}", typeof(JsonException)),
                ("{\"Version\":\"wrong\",\"Version\":1}", typeof(JsonException)),
                ("{\"Version\":1,}", typeof(JsonException)), ("/* comment */{}", typeof(JsonException)),
                ("[]", typeof(JsonException)), ("{}{}", typeof(JsonException)), ("", typeof(JsonException)),
                ("{\"Ignored\":" + new string('[', 63) + "0" + new string(']', 63) + "}", null),
                ("{\"Ignored\":" + new string('[', 64) + "0" + new string(']', 64) + "}", typeof(JsonException)),
            };
            foreach (var (text, expected) in inputs)
            {
                File.WriteAllText(path, text, new UTF8Encoding(false));
                Type? actual = null;
                try { _ = ProjectSettings.Load(path); }
                catch (Exception error) when (error is JsonException or InvalidDataException or ArgumentException)
                { actual = error.GetType(); }
                Require(actual == expected, "Profile error category: " + text);
            }
            File.WriteAllText(path, "{}", new UTF8Encoding(true));
            Require(ProjectSettings.Load(path).Language == "auto", "UTF-8 profile BOM");
            using (var locked = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                try { _ = ProjectSettings.Load(path); throw new Exception("Locked profile accepted"); }
                catch (IOException error) { Require((error.HResult & 0xffff) == 32, "Profile sharing violation"); }
            }
            foreach (var scenario in new[] { "new", "replace", "locked", "directory", "parent-file", "readonly" })
            {
                var expected = Save(Path.Combine(root, scenario, "managed"), scenario, true);
                var actual = Save(Path.Combine(root, scenario, "rust"), scenario, false);
                Require(expected == actual, "Profile transaction mismatch: " + scenario + ": " + expected + " / " + actual);
            }
            File.Delete(path);
            try { _ = ProjectSettings.Load(path); throw new Exception("Missing profile accepted"); }
            catch (FileNotFoundException) { }
        }
        finally
        {
            foreach (var file in Directory.EnumerateFiles(root, "*", SearchOption.AllDirectories)) File.SetAttributes(file, FileAttributes.Normal);
            Directory.Delete(root, true);
        }
    }

    private static string Save(string root, string scenario, bool managed)
    {
        Directory.CreateDirectory(root);
        var parent = Path.Combine(root, "nested");
        var path = Path.Combine(parent, "settings.json");
        if (scenario == "parent-file") File.WriteAllText(parent, "old-parent");
        else if (scenario != "new")
        {
            Directory.CreateDirectory(parent);
            if (scenario == "directory") Directory.CreateDirectory(path);
            else File.WriteAllText(path, "old-profile");
        }
        if (scenario == "readonly") File.SetAttributes(path, FileAttributes.ReadOnly);
        string? errorType = null;
        using (var locked = scenario == "locked" ? new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.None) : null)
        {
            var settings = new ProjectSettings { Language = "ja", Values = new() { ["DVDA_TITLE"] = "雨 日本語 🎵", ["CUSTOM"] = "<>&\"'\n" } };
            try
            {
                if (managed) typeof(ProjectSettings).GetMethod("SaveManaged", BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(settings, [path]);
                else settings.Save(path);
            }
            catch (Exception error)
            {
                if (error is TargetInvocationException wrapper) error = wrapper.InnerException!;
                if (error is not (IOException or UnauthorizedAccessException or ArgumentException)) throw;
                errorType = error.GetType().Name;
            }
            if (errorType is null)
            {
                var saved = JsonNode.Parse(File.ReadAllBytes(path));
                Require(JsonNode.DeepEquals(saved, JsonSerializer.SerializeToNode(settings)), "Saved profile content changed");
                Require(!File.ReadAllBytes(path).AsSpan().StartsWith(new byte[] { 0xEF, 0xBB, 0xBF }), "Profile must be UTF-8 without BOM");
            }
        }
        Require(!Directory.EnumerateFiles(root, "*.tmp", SearchOption.AllDirectories).Any(), "Profile save leaked temporary file");
        var files = Directory.EnumerateFileSystemEntries(root, "*", SearchOption.AllDirectories).Order(StringComparer.Ordinal)
            .Select(entry => Path.GetRelativePath(root, entry) + "|" + (Directory.Exists(entry) ? "directory" :
                errorType is null ? "saved" : File.ReadAllText(entry)));
        return errorType + "|" + string.Join("|", files);
    }
}
