using System.Reflection.PortableExecutable;
using System.Security.Cryptography;
using System.Text.Json;
using System.Xml.Linq;

namespace DvdaMaker.Toolchain;

internal static class NativeImagePackager
{
    public static void ValidateRuntime(string directory)
    {
        var path = Path.Combine(directory, "image-build.json");
        if (!File.Exists(path)) throw new InvalidDataException(
            "Missing in-process image runtime. Run build-image-runtime.py and build-image-bridge.py; place the result in build/image-native.");
        using var document = JsonDocument.Parse(File.ReadAllText(path));
        var files = document.RootElement.GetProperty("files").EnumerateObject().ToArray();
        if (files.Length != 1 || files[0].Name != "dvda-image.dll" ||
            Directory.GetFiles(directory, "*.dll").Length != 1)
            throw new InvalidDataException("The image runtime must contain exactly one self-contained image DLL.");
        ValidateBinary(Path.Combine(directory, files[0].Name), files[0].Value);
        foreach (var import in NativeToolOptimizer.ReadImports(Path.Combine(directory, files[0].Name)))
            if (!File.Exists(Path.Combine(Environment.SystemDirectory, import)))
                throw new InvalidDataException("Unexpected image dependency: " + import);
        Console.WriteLine("  in-process images: one verified Windows x64 DLL");
    }

    public static void ValidateAuthor(string directory)
    {
        var manifest = Path.Combine(directory, "author-build.json");
        if (!File.Exists(manifest)) throw new InvalidDataException(
            "Missing rebuilt native author. Run build-image-author.py, or pass --image-author <its output directory>.");
        using var document = JsonDocument.Parse(File.ReadAllText(manifest));
        ValidateBinary(Path.Combine(directory, "dvda-author-dev.exe"),
            document.RootElement.GetProperty("files").GetProperty("dvda-author-dev.exe"));
        if (!document.RootElement.TryGetProperty("runtime_files", out var runtime) ||
            runtime.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException(
                "Native author manifest has no runtime dependency list. Rebuild it with the current build-image-author.py.");

        var names = runtime.EnumerateObject().Select(property => property.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        var actual = Directory.EnumerateFiles(directory, "*.dll")
            .Select(Path.GetFileName).ToHashSet(StringComparer.OrdinalIgnoreCase);
        if (!actual.SetEquals(names))
            throw new InvalidDataException("Native author DLL files do not match author-build.json.");
        foreach (var property in runtime.EnumerateObject())
        {
            if (Path.GetFileName(property.Name) != property.Name ||
                !property.Name.EndsWith(".dll", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Invalid native author DLL name: " + property.Name);
            ValidateBinary(Path.Combine(directory, property.Name), property.Value);
        }

        foreach (var path in Directory.EnumerateFiles(directory, "*.dll").Append(Path.Combine(directory, "dvda-author-dev.exe")))
            foreach (var import in NativeToolOptimizer.ReadImports(path))
                if (!names.Contains(import) &&
                    !import.StartsWith("api-ms-win-", StringComparison.OrdinalIgnoreCase) &&
                    !import.StartsWith("ext-ms-win-", StringComparison.OrdinalIgnoreCase) &&
                    !File.Exists(Path.Combine(Environment.SystemDirectory, import)))
                    throw new InvalidDataException("Missing native author dependency: " + import);
    }

    private static void ValidateBinary(string path, JsonElement metadata)
    {
        using var file = File.OpenRead(path);
        if (file.Length != metadata.GetProperty("bytes").GetInt64() ||
            !Convert.ToHexString(SHA256.HashData(file)).Equals(metadata.GetProperty("sha256").GetString(), StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Native image component checksum mismatch: " + path);
        file.Position = 0;
        using var pe = new PEReader(file);
        if (pe.PEHeaders.CoffHeader.Machine != Machine.Amd64 || pe.PEHeaders.CorHeader is not null)
            throw new InvalidDataException("Native image component must be Windows x64: " + path);
    }

    public static void Install(string destination, string author)
    {
        ValidateAuthor(author);
        var native = Path.Combine(destination, "menu-bin");
        var images = Path.Combine(destination, "image-native");
        ValidateRuntime(images);
        var authorManifest = Path.Combine(author, "author-build.json");
        using var authorDocument = JsonDocument.Parse(File.ReadAllText(authorManifest));
        File.Copy(Path.Combine(author, "dvda-author-dev.exe"), Path.Combine(native, "dvda-author-dev.exe"), true);
        if (authorDocument.RootElement.TryGetProperty("runtime_files", out var runtime) &&
            runtime.ValueKind == JsonValueKind.Object)
            foreach (var property in runtime.EnumerateObject())
                File.Copy(Path.Combine(author, property.Name), Path.Combine(native, property.Name), true);
        File.Copy(authorManifest, Path.Combine(images, "author-build.json"), true);
        // Only the new release staging directory is pruned. No source/prebuilt files are changed.
        foreach (var name in new[] { "magick.exe", "convert.exe", "mogrify.exe", "identify.exe",
            "colors.xml", "delegates.xml", "english.xml", "locale.xml", "log.xml", "mime.xml", "policy.xml",
            "quantization-table.xml", "thresholds.xml", "type.xml", "type-ghostscript.xml", "type-dvda-cjk.xml" })
            File.Delete(Path.Combine(native, name));
        var types = new XElement("typemap");
        var regions = new[] { "SC", "JP", "KR" };
        for (var index = 0; index < regions.Length; index++)
            types.Add(new XElement("type", new XAttribute("name", "DVDA-Noto-Sans-CJK-" + regions[index]),
                new XAttribute("family", "DVDA Noto Sans CJK " + regions[index]), new XAttribute("format", "truetype"),
                new XAttribute("style", "normal"), new XAttribute("stretch", "normal"), new XAttribute("weight", 400),
                new XAttribute("face", index), new XAttribute("glyphs", "../menu-bin/fonts/DvdaNotoCJK-Regular.ttc")));
        new XDocument(types).Save(Path.Combine(images, "type.xml"));
        Console.WriteLine("  native author: in-process image calls; ImageMagick executables removed");
    }
}
