using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;
using System.Runtime.InteropServices;

namespace DvdaMaker.Packaging;

// Shared by the packager and runtime; the archive stores identical bytes once,
// while the index preserves each native component's relative directory layout.
public sealed record RuntimeBlob(string Sha256, long Length, string[] Paths);
public sealed record RuntimeIndex(int Version, string PayloadSha256, RuntimeBlob[] Blobs);

public static class RuntimeArchive
{
    public const string IndexResource = "DvdaMaker.Runtime.Index";
    public const string DataResource = "DvdaMaker.Runtime.Data";

    public static RuntimeIndex Create(string root, IEnumerable<string> files, string payload, string index)
    {
        var blobs = files.Order(StringComparer.Ordinal).Select(path => new
        {
            Path = Path.GetRelativePath(root, path).Replace('\\', '/'),
            Length = new FileInfo(path).Length,
            Hash = HashFile(path),
        }).GroupBy(file => file.Hash, StringComparer.Ordinal)
            .Select(group => new RuntimeBlob(group.Key, group.First().Length,
                group.Select(file => file.Path).Order(StringComparer.Ordinal).ToArray()))
            .OrderBy(blob => blob.Sha256, StringComparer.Ordinal).ToArray();
        using (var output = File.Create(payload))
        using (var compressed = new BrotliStream(output, CompressionLevel.SmallestSize))
            foreach (var blob in blobs)
            {
                using var input = File.OpenRead(Path.Combine(root, blob.Paths[0]));
                input.CopyTo(compressed);
            }
        var result = new RuntimeIndex(1, HashFile(payload), blobs);
        File.WriteAllBytes(index, JsonSerializer.SerializeToUtf8Bytes(result));
        return result;
    }

    public static string Extract(byte[] indexBytes, Func<Stream> openPayload, string cacheRoot)
    {
        var index = JsonSerializer.Deserialize<RuntimeIndex>(indexBytes)
            ?? throw new InvalidDataException("Missing runtime archive index.");
        var id = Convert.ToHexStringLower(SHA256.HashData(indexBytes));
        var root = Path.GetFullPath(Path.Combine(cacheRoot, id));
        ValidateIndex(index);
        Directory.CreateDirectory(root);
        var mutexId = Convert.ToHexStringLower(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(root.ToUpperInvariant())));
        using var gate = new Mutex(false, "Local\\DVD-Audio-Maker.Bundle." + mutexId);
        var entered = false;
        try
        {
            try { entered = gate.WaitOne(TimeSpan.FromMinutes(2)); }
            catch (AbandonedMutexException) { entered = true; }
            if (!entered) throw new IOException("Another instance is preparing the application components. Please try again.");
            var damaged = index.Blobs.SelectMany(blob => blob.Paths.Where(path =>
                !Matches(Path.Combine(root, path), blob)).Select(path => path)).ToHashSet(StringComparer.OrdinalIgnoreCase);
            if (damaged.Count > 0)
            {
                // Check the embedded compressed stream before writing any executable.
                using (var payload = openPayload())
                    if (Convert.ToHexStringLower(SHA256.HashData(payload)) != index.PayloadSha256)
                        throw new InvalidDataException("The application resource archive is damaged. Please download it again.");
                using var input = openPayload();
                using var decoded = new BrotliStream(input, CompressionMode.Decompress);
                foreach (var blob in index.Blobs)
                {
                    var data = new byte[checked((int)blob.Length)];
                    decoded.ReadExactly(data);
                    if (Convert.ToHexStringLower(SHA256.HashData(data)) != blob.Sha256)
                        throw new InvalidDataException("Runtime component checksum mismatch: " + blob.Paths[0]);
                    string? original = blob.Paths.Where(path => !damaged.Contains(path))
                        .Select(path => Path.Combine(root, path)).FirstOrDefault();
                    foreach (var path in blob.Paths.Where(damaged.Contains))
                    {
                        var destination = Path.Combine(root, path);
                        if (original is null || !AtomicLink(destination, original)) AtomicWrite(destination, data);
                        original ??= destination;
                    }
                }
                if (decoded.ReadByte() != -1) throw new InvalidDataException("Unexpected runtime archive data.");
            }
            var manifest = Path.Combine(root, "BUNDLE-MANIFEST.json");
            if (!File.Exists(manifest) || !File.ReadAllBytes(manifest).AsSpan().SequenceEqual(indexBytes))
                AtomicWrite(manifest, indexBytes);
            return root;
        }
        finally { if (entered) gate.ReleaseMutex(); }
    }

    private static void ValidateIndex(RuntimeIndex index)
    {
        if (index.Version != 1 || !IsHash(index.PayloadSha256) || index.Blobs is not { Length: > 0 and < 10000 })
            throw new InvalidDataException("Invalid runtime archive version or index.");
        var paths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var blob in index.Blobs)
        {
            if (!IsHash(blob.Sha256) || blob.Length is < 0 or > 536870912 || blob.Paths is not { Length: > 0 })
                throw new InvalidDataException("Invalid runtime component record.");
            foreach (var path in blob.Paths)
                if (string.IsNullOrEmpty(path) || path.Contains('\\') || path.Contains(':') ||
                    Path.IsPathRooted(path) || path.Split('/').Any(part => part is "" or "." or ".." ||
                        part.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || part.EndsWith('.') || part.EndsWith(' ')) ||
                    !paths.Add(path) || path.Equals("BUNDLE-MANIFEST.json", StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("Invalid or duplicate runtime component path.");
        }
    }

    private static bool IsHash(string? value) => value is { Length: 64 } && value.All(c =>
        c is >= '0' and <= '9' or >= 'a' and <= 'f');

    private static bool Matches(string path, RuntimeBlob blob) => File.Exists(path) &&
        new FileInfo(path).Length == blob.Length && HashFile(path) == blob.Sha256;

    public static string HashFile(string path)
    {
        using var stream = File.OpenRead(path);
        return Convert.ToHexStringLower(SHA256.HashData(stream));
    }

    private static void AtomicWrite(string path, byte[] data)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllBytes(temporary, data);
            File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    private static bool AtomicLink(string path, string original)
    {
        if (!OperatingSystem.IsWindows()) return false;
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            // NTFS shares storage; filesystems without hard links use exact copies.
            if (!CreateHardLinkW(temporary, original, IntPtr.Zero)) return false;
            File.Move(temporary, path, true);
            return true;
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreateHardLinkW(string fileName, string existingFileName, IntPtr securityAttributes);
}
