using System.Buffers.Binary;
using System.Reflection.PortableExecutable;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace DvdaMaker.Toolchain;

/// <summary>Conservative optimization of verified native tool bundles.</summary>
public static class NativeToolOptimizer
{
    private const string PortableMagickHash = "e57f70ca7e7542adf8f47a286f4082dbb2b1823812c47d25ba6a78e0cae3d4c7";
    private static readonly Dictionary<string, string> UnusedLibraryHashes = new(StringComparer.OrdinalIgnoreCase)
    {
        ["libfftw3-3.dll"] = "3cf91e10bd0c715e9e2a8a595d653dfa27252f587861ffbdc321806e1b259c89",
        ["liblqr-1-0.dll"] = "19641e285e2fbb4719d3e9e4684ff6f85445a8343121bf0e4122a50d637cb4ef",
        ["libltdl-7.dll"] = "b77c20d66c1b360324b179549414360e58ba7c28db0214b4f3364aee12c32414",
        ["libMagickCore-7.Q16HDRI-10.dll"] = "5aa9d4f7ada52cba7d157bb9e463440dceb019aedf7e4a9a8ff3be6f48ab837d",
        ["libMagickWand-7.Q16HDRI-10.dll"] = "c0c9f734b23e96dab9840bbd40744dc9f927d020ccb92bd98ba4d02e73b03fd6",
        ["libraqm-0.dll"] = "90dbfceab1a12d261c30425d573a361fa8081b4812311caa379313108ecafdd3",
    };

    public static IReadOnlyList<string> Optimize(string directory, string? shimPath = null)
    {
        directory = Path.GetFullPath(directory);
        var magick = Path.Combine(directory, "magick.exe");
        // Do not apply this profile to a different ImageMagick version or build.
        if (!File.Exists(magick) || Hash(magick) != PortableMagickHash)
        {
            if (shimPath is not null)
                throw new InvalidOperationException("The ImageMagick forwarder requires the verified portable 7.0.8-47 build.");
            return [];
        }
        if (shimPath is not null)
        {
            ValidateShim(shimPath);
            foreach (var name in new[] { "convert.exe", "mogrify.exe" })
                File.Copy(shimPath, Path.Combine(directory, name), overwrite: true);
        }

        // This cleanup is scoped to the complete, tested native bundle. A new
        // DLL or changed build must be reviewed before it is eligible for pruning.
        using var profileStream = typeof(NativeToolOptimizer).Assembly.GetManifestResourceStream(
            "DvdaMaker.NativeOptimizationProfile") ?? throw new InvalidDataException("Missing native optimization profile.");
        var profile = new Dictionary<string, string>(
            JsonSerializer.Deserialize<Dictionary<string, string>>(profileStream)!, StringComparer.OrdinalIgnoreCase);
        var unusedLibraries = new Dictionary<string, string>(UnusedLibraryHashes, StringComparer.OrdinalIgnoreCase);
        var minimal = LoadMinimalProfile();
        if (minimal.Replacements.All(pair => File.Exists(Path.Combine(directory, pair.Key)) &&
            Hash(Path.Combine(directory, pair.Key)) == pair.Value))
        {
            foreach (var pair in minimal.Replacements) profile[pair.Key] = pair.Value;
            foreach (var pair in minimal.UnusedLibraries) unusedLibraries.Add(pair.Key, pair.Value);
        }
        var replacementHash = shimPath is null
            ? "f867e492fb02c637e25c1c8b7491a8246e44df1a1e25ffd2855ed1f6ab64c17b" : Hash(shimPath);
        foreach (var file in Directory.GetFiles(directory, "*", SearchOption.AllDirectories)
            .Where(path => Path.GetExtension(path).ToLowerInvariant() is ".exe" or ".dll"))
        {
            var name = Path.GetRelativePath(directory, file);
            var hash = Hash(file);
            if (profile.TryGetValue(name, out var expected) && hash == expected) continue;
            if ((name.Equals("convert.exe", StringComparison.OrdinalIgnoreCase) ||
                name.Equals("mogrify.exe", StringComparison.OrdinalIgnoreCase)) && hash == replacementHash) continue;
            return [];
        }
        if (profile.Keys.Any(name => !unusedLibraries.ContainsKey(name) && !File.Exists(Path.Combine(directory, name))))
            return [];

        // Pin every candidate; never remove an unknown file merely by its name.
        var removable = unusedLibraries
            .Where(pair => File.Exists(Path.Combine(directory, pair.Key)) &&
                Hash(Path.Combine(directory, pair.Key)) == pair.Value)
            .Select(pair => pair.Key).ToHashSet(StringComparer.OrdinalIgnoreCase);
        if (removable.Count == 0) return [];
        var files = Directory.GetFiles(directory, "*", SearchOption.AllDirectories)
            .Where(path => Path.GetExtension(path).ToLowerInvariant() is ".exe" or ".dll" or ".xml")
            .ToArray();
        bool changed;
        do
        {
            changed = false;
            foreach (var file in files)
            {
                if (string.Equals(Path.GetDirectoryName(file), directory, StringComparison.OrdinalIgnoreCase) &&
                    removable.Contains(Path.GetFileName(file))) continue;
                var imports = Path.GetExtension(file).Equals(".xml", StringComparison.OrdinalIgnoreCase)
                    ? new HashSet<string>(StringComparer.OrdinalIgnoreCase) : ReadImports(file);
                var bytes = Encoding.Latin1.GetString(File.ReadAllBytes(file));
                foreach (var name in removable.ToArray())
                {
                    var wideName = string.Concat(name.Select(character => character + "\0"));
                    if (!imports.Contains(name) &&
                        !bytes.Contains(name, StringComparison.OrdinalIgnoreCase) &&
                        !bytes.Contains(wideName, StringComparison.OrdinalIgnoreCase)) continue;
                    removable.Remove(name);
                    changed = true;
                }
            }
            // A retained DLL may depend on another candidate: iterate to closure.
        } while (changed);

        var removed = removable.Order(StringComparer.OrdinalIgnoreCase).ToArray();
        foreach (var name in removed) File.Delete(Path.Combine(directory, name));
        return removed;
    }

    public static void ValidateMinimalFfmpeg(string directory)
    {
        foreach (var (name, expected) in LoadMinimalProfile().Replacements)
        {
            var path = Path.Combine(directory, name);
            if (!File.Exists(path) || Hash(path) != expected)
                throw new InvalidDataException($"Unverified minimal FFmpeg library: {path}. " +
                    "Rebuild and run the native validation suite before updating MinimalFfmpegProfile.json.");
        }
    }

    public static void InstallMinimalFfmpeg(string destination, string libraries)
    {
        ValidateMinimalFfmpeg(libraries);
        foreach (var name in LoadMinimalProfile().Replacements.Keys)
            File.Copy(Path.Combine(libraries, name), Path.Combine(destination, name), overwrite: true);
    }

    private static MinimalFfmpegProfile LoadMinimalProfile()
    {
        using var stream = typeof(NativeToolOptimizer).Assembly.GetManifestResourceStream(
            "DvdaMaker.MinimalFfmpegProfile") ?? throw new InvalidDataException("Missing minimal FFmpeg profile.");
        return JsonSerializer.Deserialize<MinimalFfmpegProfile>(stream,
            new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
    }

    private sealed record MinimalFfmpegProfile(
        Dictionary<string, string> Replacements, Dictionary<string, string> UnusedLibraries);

    public static void ValidateShim(string path)
    {
        using var reader = new PEReader(File.OpenRead(path));
        if (reader.PEHeaders.CoffHeader.Machine != Machine.Amd64 || reader.PEHeaders.CorHeader is not null)
            throw new InvalidDataException("ImageMagick forwarder must be a native Windows x64 executable.");
        var imports = ReadImports(path);
        if (imports.Count != 1 || !imports.Contains("kernel32.dll"))
            throw new InvalidDataException("ImageMagick forwarder must depend only on Windows Kernel32.");
    }

    public static HashSet<string> ReadImports(string path)
    {
        using var reader = new PEReader(File.OpenRead(path));
        var header = reader.PEHeaders.PEHeader ?? throw new InvalidDataException("Missing PE header: " + path);
        var result = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        ReadDirectory(header.ImportTableDirectory, delayed: false);
        ReadDirectory(header.DelayImportTableDirectory, delayed: true);
        return result;

        void ReadDirectory(DirectoryEntry directory, bool delayed)
        {
            if (directory.RelativeVirtualAddress == 0) return;
            var content = reader.GetSectionData(directory.RelativeVirtualAddress).GetContent();
            var recordSize = delayed ? 32 : 20;
            for (var offset = 0; offset <= content.Length - recordSize; offset += recordSize)
            {
                var record = content.AsSpan(offset, recordSize);
                if (record.IndexOfAnyExcept((byte)0) < 0) return;
                var address = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(delayed ? 4 : 12, 4));
                if (delayed && (BinaryPrimitives.ReadUInt32LittleEndian(record[..4]) & 1) == 0)
                    address = checked((uint)(address - header.ImageBase));
                var name = reader.GetSectionData(checked((int)address)).GetContent();
                var end = name.AsSpan().IndexOf((byte)0);
                if (end <= 0 || end > 512) throw new InvalidDataException("Invalid PE import name: " + path);
                result.Add(Encoding.ASCII.GetString(name.AsSpan(0, end)));
            }
            throw new InvalidDataException("Unterminated PE imports: " + path);
        }
    }

    private static string Hash(string path)
    {
        using var file = File.OpenRead(path);
        return Convert.ToHexStringLower(SHA256.HashData(file));
    }
}
