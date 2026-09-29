using System.Buffers.Binary;
using System.Text;

namespace DvdaMaker.Formats.Iso9660;

public sealed class Iso9660Reader : IDisposable
{
    public const int DefaultSectorSize = 2048;
    private const int VolumeDescriptorSearchCount = 32;

    private readonly Stream _stream;
    private readonly bool _ownsStream;
    private readonly Dictionary<(uint Lba, uint Size), IReadOnlyList<IsoDirectoryEntry>> _directoryCache = [];

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
                ReadPrimaryVolumeDescriptor();
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
            ReadPrimaryVolumeDescriptor();
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
        var entry = Lookup(innerPath);
        return entry is { IsDirectory: true }
            ? ParseDirectory(DataLba(entry), entry.Size)
            : [];
    }

    public IsoDirectoryEntry? GetEntry(string innerPath) => Lookup(innerPath);

    public uint GetDataLogicalBlockAddress(string innerPath)
    {
        var entry = Lookup(innerPath) ??
            throw new Iso9660Exception($"ISO 里没有 {innerPath}");
        return DataLba(entry);
    }

    public IReadOnlyList<string> AllPaths(string innerPath = "")
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
