using System.Buffers.Binary;
using System.Text;
using DvdaMaker.Processes;

namespace DvdaMaker.Formats.Iso9660;

public sealed class Iso9660Reader : IDisposable
{
    public const int DefaultSectorSize = 2048;
    private const int VolumeDescriptorSearchCount = 32;

    private readonly Stream _stream;
    private readonly bool _ownsStream;
    private readonly Dictionary<(uint Lba, uint Size), IReadOnlyList<IsoDirectoryEntry>> _directoryCache = [];

    private sealed record RustIsoInfo(
        int SectorSize,
        uint RootLogicalBlockAddress,
        uint RootSize,
        string VolumeIdentifier);

    private sealed record RustIsoFile(string DataHex);
    private sealed record RustIsoExtract(bool Extracted);

    public Iso9660Reader(string path)
    {
        Path = path;
        try
        {
            _stream = File.Open(path, FileMode.Open, FileAccess.Read, FileShare.Read);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            throw new Iso9660Exception($"无法打开 ISO {path}: {exception.Message}", exception);
        }

        _ownsStream = true;
        try
        {
            (SectorSize, RootLogicalBlockAddress, RootSize, VolumeIdentifier) =
                ReadPrimaryVolumeDescriptorWithRust();
        }
        catch
        {
            _stream.Dispose();
            throw;
        }
    }

    public Iso9660Reader(Stream stream, string path = "<stream>", bool leaveOpen = false)
    {
        if (!stream.CanRead || !stream.CanSeek)
        {
            throw new ArgumentException("ISO 流必须可读且可定位。", nameof(stream));
        }

        Path = path;
        _stream = stream;
        _ownsStream = !leaveOpen;
        (SectorSize, RootLogicalBlockAddress, RootSize, VolumeIdentifier) =
            ReadPrimaryVolumeDescriptorWithRust();
    }

    public string Path { get; }
    public int SectorSize { get; }
    public uint RootLogicalBlockAddress { get; }
    public uint RootSize { get; }
    public string VolumeIdentifier { get; }

    public byte[] ReadSectors(long logicalBlockAddress, int sectorCount)
    {
        if (logicalBlockAddress < 0 || sectorCount <= 0)
        {
            return [];
        }

        return ReadAt(checked(logicalBlockAddress * SectorSize), checked(sectorCount * SectorSize));
    }

    public byte[] ReadRange(long logicalBlockAddress, long size)
    {
        if (logicalBlockAddress < 0 || size <= 0)
        {
            return [];
        }
        if (size > int.MaxValue)
        {
            throw new Iso9660Exception($"请求读取的数据过大: {size} 字节");
        }

        return ReadAt(checked(logicalBlockAddress * SectorSize), (int)size);
    }

    public IReadOnlyList<IsoDirectoryEntry> ListDirectory(string innerPath = "")
    {
        if (RustBridge.Mode != "managed")
        {
            return RustBridge.Run<IReadOnlyList<IsoDirectoryEntry>>("iso.list",
                new { Path, InnerPath = innerPath }, () => ListDirectoryManaged(innerPath));
        }
        return ListDirectoryManaged(innerPath);
    }

    private IReadOnlyList<IsoDirectoryEntry> ListDirectoryManaged(string innerPath)
    {
        var entry = Lookup(innerPath);
        return entry is { IsDirectory: true }
            ? ParseDirectory(DataLba(entry), entry.Size)
            : [];
    }

    public IsoDirectoryEntry? GetEntry(string innerPath)
    {
        if (RustBridge.Mode != "managed")
        {
            return RustBridge.Run<IsoDirectoryEntry?>("iso.entry",
                new { Path, InnerPath = innerPath }, () => Lookup(innerPath));
        }
        return Lookup(innerPath);
    }

    public uint GetDataLogicalBlockAddress(string innerPath)
    {
        if (RustBridge.Mode != "managed")
        {
            return RustBridge.Run<uint>("iso.data_lba", new { Path, InnerPath = innerPath },
                () => GetDataLogicalBlockAddressManaged(innerPath));
        }
        return GetDataLogicalBlockAddressManaged(innerPath);
    }

    private uint GetDataLogicalBlockAddressManaged(string innerPath)
    {
        var entry = Lookup(innerPath) ??
            throw new Iso9660Exception($"ISO 里没有 {innerPath}");
        return DataLba(entry);
    }

    public IReadOnlyList<string> AllPaths(string innerPath = "")
    {
        if (RustBridge.Mode != "managed")
        {
            return RustBridge.Run<IReadOnlyList<string>>("iso.all_paths",
                new { Path, InnerPath = innerPath }, () => AllPathsManaged(innerPath));
        }
        return AllPathsManaged(innerPath);
    }

    private IReadOnlyList<string> AllPathsManaged(string innerPath)
    {
        var output = new List<string>();
        var parts = SplitPath(innerPath);
        var prefix = parts.Count == 0 ? string.Empty : "/" + string.Join('/', parts);
        var root = Lookup(innerPath);
        if (root is null)
        {
            return output;
        }
        if (!root.IsDirectory)
        {
            output.Add(prefix);
            return output;
        }

        void Recurse(IsoDirectoryEntry parent, string basePath)
        {
            foreach (var child in ParseDirectory(DataLba(parent), parent.Size))
            {
                var path = basePath + "/" + child.Name;
                output.Add(path);
                if (child.IsDirectory)
                {
                    Recurse(child, path);
                }
            }
        }

        Recurse(root, prefix);
        return output;
    }

    public byte[] ReadFile(string innerPath)
    {
        if (RustBridge.Mode != "managed")
        {
            var rust = RustBridge.Run<RustIsoFile>("iso.read_file",
                new { Path, InnerPath = innerPath }, () => new RustIsoFile(
                    Convert.ToHexString(ReadFileManaged(innerPath)).ToLowerInvariant()));
            var data = HexDecode(rust.DataHex);
            if (RustBridge.Mode == "compare")
            {
                var managed = ReadFileManaged(innerPath);
                if (!managed.AsSpan().SequenceEqual(data))
                {
                    throw new InvalidDataException($"Rust ISO ReadFile mismatch at byte {FirstMismatch(managed, data)}.");
                }
            }
            return data;
        }
        return ReadFileManaged(innerPath);
    }

    private byte[] ReadFileManaged(string innerPath)
    {
        var entry = Lookup(innerPath) ??
            throw new Iso9660Exception($"ISO 里没有 {innerPath}");
        if (entry.IsDirectory)
        {
            throw new Iso9660Exception($"{innerPath} 是目录，不是文件");
        }
        if (entry.IsMultiExtent)
        {
            throw new Iso9660Exception($"{innerPath} 是多段（multi-extent）文件，本读取器不支持。");
        }

        return ReadRange(DataLba(entry), entry.Size);
    }

    public bool Extract(string innerPath, string destination)
    {
        if (RustBridge.Mode == "rust")
        {
            return RustBridge.Invoke<RustIsoExtract>("iso.extract",
                new { Path, InnerPath = innerPath, Destination = System.IO.Path.GetFullPath(destination) }).Extracted;
        }
        if (RustBridge.Mode == "compare")
        {
            var rustDestination = destination + ".rust-" + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                var rust = RustBridge.Invoke<RustIsoExtract>("iso.extract",
                    new { Path, InnerPath = innerPath, Destination = System.IO.Path.GetFullPath(rustDestination) });
                var managed = ExtractManaged(innerPath, destination);
                if (rust.Extracted != managed || (managed && !FileTreesEqual(destination, rustDestination)))
                {
                    throw new InvalidDataException("Rust ISO extraction differs from the managed result.");
                }
                return managed;
            }
            finally
            {
                if (File.Exists(rustDestination)) File.Delete(rustDestination);
                if (Directory.Exists(rustDestination)) Directory.Delete(rustDestination, recursive: true);
            }
        }
        return ExtractManaged(innerPath, destination);
    }

    private bool ExtractManaged(string innerPath, string destination)
    {
        var entry = Lookup(innerPath);
        if (entry is null)
        {
            return false;
        }

        if (entry.IsDirectory)
        {
            ExtractDirectory(entry, destination);
        }
        else
        {
            var parent = System.IO.Path.GetDirectoryName(destination);
            if (!string.IsNullOrEmpty(parent))
            {
                Directory.CreateDirectory(parent);
            }
            File.WriteAllBytes(destination, ReadEntry(entry));
        }
        return true;
    }

    private static bool FileTreesEqual(string left, string right)
    {
        if (File.Exists(left) || File.Exists(right))
        {
            return File.Exists(left) && File.Exists(right) &&
                   File.ReadAllBytes(left).AsSpan().SequenceEqual(File.ReadAllBytes(right));
        }
        if (!Directory.Exists(left) || !Directory.Exists(right)) return false;
        var leftFiles = Directory.GetFiles(left, "*", SearchOption.AllDirectories)
            .Select(path => System.IO.Path.GetRelativePath(left, path))
            .OrderBy(path => path, StringComparer.OrdinalIgnoreCase).ToArray();
        var rightFiles = Directory.GetFiles(right, "*", SearchOption.AllDirectories)
            .Select(path => System.IO.Path.GetRelativePath(right, path))
            .OrderBy(path => path, StringComparer.OrdinalIgnoreCase).ToArray();
        if (!leftFiles.SequenceEqual(rightFiles, StringComparer.OrdinalIgnoreCase)) return false;
        return leftFiles.All(relative =>
            File.ReadAllBytes(System.IO.Path.Combine(left, relative)).AsSpan().SequenceEqual(
                File.ReadAllBytes(System.IO.Path.Combine(right, relative))));
    }

    private static int FirstMismatch(ReadOnlySpan<byte> left, ReadOnlySpan<byte> right)
    {
        var common = Math.Min(left.Length, right.Length);
        for (var index = 0; index < common; index++)
        {
            if (left[index] != right[index]) return index;
        }
        return common;
    }

    private static byte[] HexDecode(string value)
    {
        if (value.Length % 2 != 0) throw new InvalidDataException("Rust ISO file payload is invalid.");
        var bytes = new byte[value.Length / 2];
        for (var index = 0; index < bytes.Length; index++)
        {
            bytes[index] = Convert.ToByte(value.Substring(index * 2, 2), 16);
        }
        return bytes;
    }

    public string? Find(string basename, string innerPath = "") =>
        AllPaths(innerPath).FirstOrDefault(path =>
            string.Equals(System.IO.Path.GetFileName(path), basename, StringComparison.OrdinalIgnoreCase));

    public void Dispose()
    {
        if (_ownsStream)
        {
            _stream.Dispose();
        }
    }

    public static string StripVersion(string name)
    {
        if (RustBridge.Mode != "managed")
        {
            return RustBridge.Run("iso.strip_version", name, () => StripVersionManaged(name));
        }
        return StripVersionManaged(name);
    }

    private static string StripVersionManaged(string name)
    {
        var separator = name.LastIndexOf(';');
        if (separator < 0 || separator == name.Length - 1)
        {
            return name;
        }

        return name[(separator + 1)..].All(char.IsDigit) ? name[..separator] : name;
    }

    private (int SectorSize, uint RootLba, uint RootSize, string VolumeIdentifier)
        ReadPrimaryVolumeDescriptor()
    {
        for (var index = 0; index < VolumeDescriptorSearchCount; index++)
        {
            var lba = 16 + index;
            var sector = ReadAt((long)lba * DefaultSectorSize, DefaultSectorSize);
            if (sector.Length < 7 || !sector.AsSpan(1, 5).SequenceEqual("CD001"u8))
            {
                if (index == 0)
                {
                    continue;
                }
                break;
            }

            var volumeType = sector[0];
            if (volumeType == 0xFF)
            {
                break;
            }
            if (volumeType != 1)
            {
                continue;
            }

            var sectorSize = BinaryPrimitives.ReadUInt16LittleEndian(sector.AsSpan(128, 2));
            if (sectorSize == 0)
            {
                sectorSize = DefaultSectorSize;
            }
            if (sector.Length < 190)
            {
                throw new Iso9660Exception("PVD 里的根目录记录被截断");
            }

            var root = sector.AsSpan(156, 34);
            return (
                sectorSize,
                BinaryPrimitives.ReadUInt32LittleEndian(root[2..6]),
                BinaryPrimitives.ReadUInt32LittleEndian(root[10..14]),
                Encoding.ASCII.GetString(sector, 40, 32).TrimEnd(' ', '\0'));
        }

        throw new Iso9660Exception(
            $"在 {Path} 里找不到 ISO9660 Primary Volume Descriptor（扇区 16..{16 + VolumeDescriptorSearchCount - 1}）");
    }

    private (int SectorSize, uint RootLba, uint RootSize, string VolumeIdentifier)
        ReadPrimaryVolumeDescriptorWithRust()
    {
        if (RustBridge.Mode == "managed")
        {
            return ReadPrimaryVolumeDescriptor();
        }
        var info = RustBridge.Run<RustIsoInfo>("iso.info", Path,
            () =>
            {
                var managed = ReadPrimaryVolumeDescriptor();
                return new RustIsoInfo(managed.SectorSize, managed.RootLba, managed.RootSize,
                    managed.VolumeIdentifier);
            });
        return (info.SectorSize, info.RootLogicalBlockAddress, info.RootSize, info.VolumeIdentifier);
    }

    private IReadOnlyList<IsoDirectoryEntry> ParseDirectory(uint logicalBlockAddress, uint size)
    {
        var key = (logicalBlockAddress, size);
        if (_directoryCache.TryGetValue(key, out var cached))
        {
            return cached;
        }

        var blob = ReadRange(logicalBlockAddress, size);
        var output = new List<IsoDirectoryEntry>();
        var offset = 0;
        while (offset < blob.Length)
        {
            var length = blob[offset];
            if (length == 0)
            {
                var next = ((offset / SectorSize) + 1) * SectorSize;
                if (next <= offset)
                {
                    break;
                }
                offset = next;
                continue;
            }
            if (offset + length > blob.Length)
            {
                break;
            }

            var record = blob.AsSpan(offset, length);
            offset += length;
            if (record.Length < 33)
            {
                continue;
            }

            var nameLength = record[32];
            if (33 + nameLength > record.Length)
            {
                continue;
            }
            var rawName = record.Slice(33, nameLength);
            if (nameLength == 1 && (rawName[0] == 0 || rawName[0] == 1))
            {
                continue;
            }
            if (!IsAscii(rawName))
            {
                continue;
            }

            var name = StripVersion(Encoding.ASCII.GetString(rawName));
            if (name.Length == 0)
            {
                continue;
            }

            var flags = record[25];
            output.Add(new IsoDirectoryEntry(
                name,
                (flags & 0x02) != 0,
                BinaryPrimitives.ReadUInt32LittleEndian(record[2..6]),
                BinaryPrimitives.ReadUInt32LittleEndian(record[10..14]),
                record[1],
                (flags & 0x80) != 0));
        }

        _directoryCache[key] = output;
        return output;
    }

    private IsoDirectoryEntry? Lookup(string innerPath)
    {
        var root = new IsoDirectoryEntry(
            string.Empty, true, RootLogicalBlockAddress, RootSize, 0, false);
        var parts = SplitPath(innerPath);
        if (parts.Count == 0)
        {
            return root;
        }

        var current = root;
        for (var index = 0; index < parts.Count; index++)
        {
            var found = ParseDirectory(DataLba(current), current.Size).FirstOrDefault(entry =>
                string.Equals(entry.Name, parts[index], StringComparison.OrdinalIgnoreCase));
            if (found is null || (index < parts.Count - 1 && !found.IsDirectory))
            {
                return null;
            }
            current = found;
        }
        return current;
    }

    private void ExtractDirectory(IsoDirectoryEntry directory, string destination)
    {
        Directory.CreateDirectory(destination);
        foreach (var child in ParseDirectory(DataLba(directory), directory.Size))
        {
            var path = System.IO.Path.Combine(destination, child.Name);
            if (child.IsDirectory)
            {
                ExtractDirectory(child, path);
            }
            else
            {
                File.WriteAllBytes(path, ReadEntry(child));
            }
        }
    }

    private byte[] ReadEntry(IsoDirectoryEntry entry)
    {
        if (entry.IsMultiExtent)
        {
            throw new Iso9660Exception($"{entry.Name} 是多段（multi-extent）文件，本读取器不支持。");
        }
        return ReadRange(DataLba(entry), entry.Size);
    }

    private static uint DataLba(IsoDirectoryEntry entry) =>
        checked(entry.LogicalBlockAddress + entry.ExtendedAttributeBlocks);

    private static IReadOnlyList<string> SplitPath(string innerPath) =>
        innerPath.Replace('\\', '/').Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Where(part => part != ".")
            .ToArray();

    private static bool IsAscii(ReadOnlySpan<byte> value)
    {
        foreach (var item in value)
        {
            if (item > 0x7F)
            {
                return false;
            }
        }
        return true;
    }

    private byte[] ReadAt(long offset, int count)
    {
        _stream.Seek(offset, SeekOrigin.Begin);
        var buffer = new byte[count];
        var total = 0;
        while (total < count)
        {
            var read = _stream.Read(buffer, total, count - total);
            if (read == 0)
            {
                break;
            }
            total += read;
        }
        return total == count ? buffer : buffer[..total];
    }
}
