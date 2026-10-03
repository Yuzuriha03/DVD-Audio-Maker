using System.Security.Cryptography;
using System.Text.Json;
using DvdaMaker.Packaging;

internal static class RuntimeArchiveTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new Exception(message); }

    public static void RoundtripAndRepair()
    {
        var work = Path.Combine(Path.GetTempPath(), "dvda-bundle-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(work);
        try
        {
            var source = Path.Combine(work, "source");
            var cache = Path.Combine(work, "cache");
            var names = new[] { "media-native/shared.dll", "menu-bin/shared.dll", "data/中文.txt" };
            foreach (var name in names)
            {
                var path = Path.Combine(source, name); Directory.CreateDirectory(Path.GetDirectoryName(path)!);
                File.WriteAllText(path, name.EndsWith(".dll") ? "identical component" : "完整 Unicode 内容");
            }
            var payload = Path.Combine(work, "runtime.br"); var manifest = Path.Combine(work, "runtime.json");
            var index = RuntimeArchive.Create(source, names.Select(name => Path.Combine(source, name)), payload, manifest);
            Require(index.Blobs.Length == 2 && index.Blobs.Sum(blob => blob.Paths.Length) == 3, "Duplicate bytes were stored twice");
            var bytes = File.ReadAllBytes(manifest);
            var outputs = new string[4];
            Parallel.For(0, 4, i => outputs[i] = RuntimeArchive.Extract(bytes, () => File.OpenRead(payload), cache));
            Require(outputs.All(path => path == outputs[0]), "Concurrent extraction chose different versions");
            var root = outputs[0];
            foreach (var name in names)
                Require(File.ReadAllBytes(Path.Combine(source, name)).AsSpan().SequenceEqual(File.ReadAllBytes(Path.Combine(root, name))), "Extraction changed bytes");
            var stamps = names.Select(name => File.GetLastWriteTimeUtc(Path.Combine(root, name))).ToArray();
            RuntimeArchive.Extract(bytes, () => throw new Exception("A valid cache reopened the payload"), cache);
            Require(names.Select(name => File.GetLastWriteTimeUtc(Path.Combine(root, name))).SequenceEqual(stamps), "Warm start rewrote files");
            File.Delete(Path.Combine(root, names[0])); // Break only this directory entry when hard-linked.
            File.WriteAllText(Path.Combine(root, names[0]), "damaged"); File.Delete(Path.Combine(root, names[2]));
            RuntimeArchive.Extract(bytes, () => File.OpenRead(payload), cache);
            Require(names.All(name => RuntimeArchive.HashFile(Path.Combine(source, name)) == RuntimeArchive.HashFile(Path.Combine(root, name))), "Cache repair failed");
            Require(File.GetLastWriteTimeUtc(Path.Combine(root, names[1])) == stamps[1], "Repair rewrote a healthy component");
            Require(!Directory.EnumerateFiles(cache, "*.tmp", SearchOption.AllDirectories).Any(), "Extraction leaked temporary files");
            var broken = File.ReadAllBytes(payload); broken[0] ^= 1;
            try
            {
                RuntimeArchive.Extract(bytes, () => new MemoryStream(broken), Path.Combine(work, "damaged-cache"));
                throw new Exception("Damaged payload was accepted");
            }
            catch (InvalidDataException) { }
            Require(!Directory.EnumerateFiles(Path.Combine(work, "damaged-cache"), "*", SearchOption.AllDirectories).Any(), "Corrupt archive wrote files");
        }
        finally { Directory.Delete(work, true); }
    }

    public static void RejectUnsafePaths()
    {
        var work = Path.Combine(Path.GetTempPath(), "dvda-bundle-path-" + Guid.NewGuid().ToString("N"));
        try
        {
            foreach (var path in new[] { "../escape.dll", "C:/escape.dll", "/escape.dll", "data/../escape.dll", "data//escape.dll", "data/file:stream" })
            {
                var index = new RuntimeIndex(1, new string('0', 64), [new RuntimeBlob(new string('1', 64), 0, [path])]);
                try
                {
                    RuntimeArchive.Extract(JsonSerializer.SerializeToUtf8Bytes(index), () => throw new Exception("Unsafe index opened its payload"), work);
                    throw new Exception("Unsafe path accepted: " + path);
                }
                catch (InvalidDataException) { }
            }
            Require(!Directory.Exists(work), "Unsafe index created a cache directory");
        }
        finally { if (Directory.Exists(work)) Directory.Delete(work, true); }
    }
}
