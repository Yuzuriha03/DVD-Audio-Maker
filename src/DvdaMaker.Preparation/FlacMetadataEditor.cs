using System.Buffers.Binary;
using System.Text;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

/// <summary>
/// Reads and rewrites the FLAC metadata blocks without starting metaflac.exe.
/// Audio frames are copied byte-for-byte; only the metadata prefix is rebuilt.
/// </summary>
public static class FlacMetadataEditor
{
    private const int VorbisCommentType = 4;
    private const int PictureType = 6;
    private static readonly UTF8Encoding Utf8 = new(false, true);

    public sealed record PictureBlock(
        FlacPictureDescriptor Descriptor,
        string Description,
        int Colors,
        byte[] ImageData);

    private sealed record FlacComment(string Key, string Value);

    private sealed record RustPicture(
        FlacPictureDescriptor Descriptor,
        string Description,
        int Colors,
        string ImageHex);

    private sealed record RustWriteResult(long WrittenBytes);

    public static IReadOnlyList<(string Key, string Value)> ReadVorbisComments(string path)
    {
        if (RustBridge.Mode != "managed")
        {
            var comments = RustBridge.Run<List<FlacComment>>("flac.comments", path,
                () => ReadVorbisCommentsManaged(path)
                    .Select(item => new FlacComment(item.Key, item.Value)).ToList());
            return comments.Select(item => (item.Key, item.Value)).ToArray();
        }
        return ReadVorbisCommentsManaged(path);
    }

    private static IReadOnlyList<(string Key, string Value)> ReadVorbisCommentsManaged(string path)
    {
        var document = ReadDocument(path);
        var block = document.Blocks.FirstOrDefault(item => item.Type == VorbisCommentType);
        if (block is null)
        {
            return [];
        }

        var reader = new SpanReader(block.Data);
        var vendorLength = reader.ReadUInt32LittleEndian();
        reader.Skip(vendorLength);
        var count = reader.ReadUInt32LittleEndian();
        if (count > int.MaxValue) throw new InvalidDataException("FLAC Vorbis comment count is too large.");
        var comments = new List<(string Key, string Value)>((int)count);
        for (var index = 0; index < (int)count; index++)
        {
            var length = reader.ReadUInt32LittleEndian();
            var value = reader.ReadUtf8(length);
            var separator = value.IndexOf('=');
            if (separator > 0)
            {
                comments.Add((value[..separator], value[(separator + 1)..]));
            }
        }
        reader.RequireEnd();
        return comments;
    }

    public static PictureBlock? ReadPicture(string path)
    {
        if (RustBridge.Mode != "managed")
        {
            var picture = RustBridge.Run<RustPicture?>("flac.picture", path,
                () => ToRustPicture(ReadPictureManaged(path)));
            return picture is null ? null : new PictureBlock(
                picture.Descriptor, picture.Description, picture.Colors, HexDecode(picture.ImageHex));
        }
        return ReadPictureManaged(path);
    }

    private static PictureBlock? ReadPictureManaged(string path)
    {
        var document = ReadDocument(path);
        var block = document.Blocks.FirstOrDefault(item => item.Type == PictureType);
        return block is null ? null : ParsePicture(block.Data);
    }

    private static RustPicture? ToRustPicture(PictureBlock? picture) => picture is null
        ? null
        : new RustPicture(picture.Descriptor, picture.Description, picture.Colors,
            Convert.ToHexString(picture.ImageData).ToLowerInvariant());

    private static byte[] HexDecode(string value)
    {
        if (value.Length % 2 != 0) throw new InvalidDataException("FLAC picture hex is invalid.");
        var result = new byte[value.Length / 2];
        for (var index = 0; index < result.Length; index++)
        {
            var high = HexDigit(value[index * 2]);
            var low = HexDigit(value[index * 2 + 1]);
            if (high < 0 || low < 0) throw new InvalidDataException("FLAC picture hex is invalid.");
            result[index] = (byte)((high << 4) | low);
        }
        return result;
    }

    private static int HexDigit(char value) => value switch
    {
        >= '0' and <= '9' => value - '0',
        >= 'a' and <= 'f' => value - 'a' + 10,
        >= 'A' and <= 'F' => value - 'A' + 10,
        _ => -1,
    };

    public static void ExportPicture(string path, string destination)
    {
        var picture = ReadPicture(path)
            ?? throw new InvalidDataException("FLAC file does not contain a PICTURE block.");
        var directory = Path.GetDirectoryName(Path.GetFullPath(destination));
        if (directory is not null) Directory.CreateDirectory(directory);
        File.WriteAllBytes(destination, picture.ImageData);
    }

    /// <summary>
    /// Replaces all PICTURE blocks with one block using the supplied image bytes and
    /// the descriptor from the original block. This is the in-process equivalent of
    /// metaflac remove/import used by the preparation pipeline.
    /// </summary>
    public static void ReplacePicture(string path, string imagePath, PictureBlock template)
    {
        if (RustBridge.Mode == "rust")
        {
            _ = RustBridge.Invoke<RustWriteResult>("flac.replace_picture",
                ReplaceRequest(path, imagePath, template));
            return;
        }
        if (RustBridge.Mode == "compare")
        {
            var rustPath = path + ".rust-" + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                File.Copy(path, rustPath, overwrite: false);
                _ = RustBridge.Invoke<RustWriteResult>("flac.replace_picture",
                    ReplaceRequest(rustPath, imagePath, template));
                ReplacePictureManaged(path, imagePath, template);
                var managed = File.ReadAllBytes(path);
                var rust = File.ReadAllBytes(rustPath);
                if (!managed.AsSpan().SequenceEqual(rust))
                {
                    throw new InvalidDataException($"Rust FLAC picture replacement mismatch at byte {FirstMismatch(managed, rust)}.");
                }
                return;
            }
            finally
            {
                if (File.Exists(rustPath)) File.Delete(rustPath);
            }
        }
        ReplacePictureManaged(path, imagePath, template);
    }

    private static object ReplaceRequest(string path, string imagePath, PictureBlock template) => new
    {
        Path = Path.GetFullPath(path),
        ImagePath = Path.GetFullPath(imagePath),
        Template = new
        {
            Descriptor = new
            {
                Type = template.Descriptor.Type,
                MimeType = template.Descriptor.MimeType,
                Width = template.Descriptor.Width,
                Height = template.Descriptor.Height,
                Depth = template.Descriptor.Depth,
            },
            template.Description,
            template.Colors,
        },
    };

    private static int FirstMismatch(ReadOnlySpan<byte> left, ReadOnlySpan<byte> right)
    {
        var common = Math.Min(left.Length, right.Length);
        for (var index = 0; index < common; index++)
        {
            if (left[index] != right[index]) return index;
        }
        return common;
    }

    private static void ReplacePictureManaged(string path, string imagePath, PictureBlock template)
    {
        var document = ReadDocument(path);
        if (!document.Blocks.Any(item => item.Type == PictureType))
        {
            throw new InvalidDataException("FLAC file does not contain a PICTURE block.");
        }
        var image = File.ReadAllBytes(imagePath);
        var replacement = new MetadataBlock(PictureType, BuildPicturePayload(template, image));
        var firstPicture = document.Blocks.FindIndex(item => item.Type == PictureType);
        var blocks = document.Blocks
            .Where(item => item.Type != PictureType)
            .ToList();
        blocks.Insert(Math.Min(firstPicture, blocks.Count), replacement);
        WriteDocument(path, document.AudioOffset, blocks);
    }

    private static PictureBlock ParsePicture(byte[] data)
    {
        var reader = new SpanReader(data);
        var type = reader.ReadUInt32BigEndian();
        var mime = reader.ReadUtf8(reader.ReadUInt32BigEndian());
        var description = reader.ReadUtf8(reader.ReadUInt32BigEndian());
        var width = reader.ReadUInt32BigEndian();
        var height = reader.ReadUInt32BigEndian();
        var depth = reader.ReadUInt32BigEndian();
        var colors = reader.ReadUInt32BigEndian();
        var imageLength = reader.ReadUInt32BigEndian();
        var image = reader.ReadBytes(imageLength);
        reader.RequireEnd();
        if (type > int.MaxValue || width > int.MaxValue || height > int.MaxValue ||
            depth > int.MaxValue || colors > int.MaxValue)
        {
            throw new InvalidDataException("FLAC PICTURE values exceed the supported range.");
        }
        return new PictureBlock(
            new FlacPictureDescriptor((int)type, mime, (int)width, (int)height, (int)depth),
            description, (int)colors, image);
    }

    private static byte[] BuildPicturePayload(PictureBlock template, byte[] image)
    {
        if (template.Descriptor.Type is < 0 or > int.MaxValue ||
            template.Descriptor.Width is < 0 or > int.MaxValue ||
            template.Descriptor.Height is < 0 or > int.MaxValue ||
            template.Descriptor.Depth is < 0 or > int.MaxValue ||
            template.Colors is < 0)
        {
            throw new InvalidDataException("FLAC PICTURE values are invalid.");
        }
        var mime = Utf8.GetBytes(template.Descriptor.MimeType);
        var description = Utf8.GetBytes(template.Description);
        var total = checked(
            4 + 4 + mime.Length + 4 + description.Length +
            4 + 4 + 4 + 4 + 4 + image.Length);
        var payload = new byte[total];
        var offset = 0;
        WriteUInt32BigEndian(payload, ref offset, (uint)template.Descriptor.Type);
        WriteUInt32BigEndian(payload, ref offset, (uint)mime.Length);
        mime.CopyTo(payload.AsSpan(offset));
        offset += mime.Length;
        WriteUInt32BigEndian(payload, ref offset, (uint)description.Length);
        description.CopyTo(payload.AsSpan(offset));
        offset += description.Length;
        WriteUInt32BigEndian(payload, ref offset, (uint)template.Descriptor.Width);
        WriteUInt32BigEndian(payload, ref offset, (uint)template.Descriptor.Height);
        WriteUInt32BigEndian(payload, ref offset, (uint)template.Descriptor.Depth);
        WriteUInt32BigEndian(payload, ref offset, (uint)template.Colors);
        WriteUInt32BigEndian(payload, ref offset, (uint)image.Length);
        image.CopyTo(payload.AsSpan(offset));
        return payload;
    }

    private static void WriteUInt32BigEndian(byte[] destination, ref int offset, uint value)
    {
        BinaryPrimitives.WriteUInt32BigEndian(destination.AsSpan(offset, sizeof(uint)), value);
        offset += sizeof(uint);
    }

    private static FlacDocument ReadDocument(string path)
    {
        using var input = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
        Span<byte> signature = stackalloc byte[4];
        ReadExactly(input, signature);
        if (!signature.SequenceEqual("fLaC"u8))
        {
            throw new InvalidDataException("Not a FLAC stream.");
        }

        var blocks = new List<MetadataBlock>();
        var isLast = false;
        Span<byte> lengthBytes = stackalloc byte[3];
        while (!isLast)
        {
            var header = input.ReadByte();
            if (header < 0) throw new InvalidDataException("FLAC metadata header is truncated.");
            isLast = (header & 0x80) != 0;
            var type = header & 0x7f;
            ReadExactly(input, lengthBytes);
            var length = (lengthBytes[0] << 16) | (lengthBytes[1] << 8) | lengthBytes[2];
            var data = new byte[length];
            ReadExactly(input, data);
            blocks.Add(new MetadataBlock(type, data));
        }
        if (blocks.Count == 0 || blocks[0].Type != 0 || blocks[0].Data.Length != 34)
        {
            throw new InvalidDataException("FLAC STREAMINFO block is missing or invalid.");
        }
        return new FlacDocument(blocks, input.Position);
    }

    private static void WriteDocument(string path, long audioOffset, IReadOnlyList<MetadataBlock> blocks)
    {
        if (blocks.Count == 0 || blocks.Any(block => block.Type is < 0 or > 127 || block.Data.Length > 0xFFFFFF))
        {
            throw new InvalidDataException("FLAC metadata block is too large or has an invalid type.");
        }
        var fullPath = Path.GetFullPath(path);
        var temporary = fullPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var input = new FileStream(fullPath, FileMode.Open, FileAccess.Read, FileShare.Read))
            using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                output.Write("fLaC"u8);
                for (var index = 0; index < blocks.Count; index++)
                {
                    var block = blocks[index];
                    output.WriteByte((byte)(block.Type | (index == blocks.Count - 1 ? 0x80 : 0)));
                    output.WriteByte((byte)(block.Data.Length >> 16));
                    output.WriteByte((byte)(block.Data.Length >> 8));
                    output.WriteByte((byte)block.Data.Length);
                    output.Write(block.Data);
                }
                input.Position = audioOffset;
                input.CopyTo(output);
            }
            File.Move(temporary, fullPath, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporary)) File.Delete(temporary);
        }
    }

    private static void ReadExactly(Stream stream, Span<byte> destination)
    {
        while (!destination.IsEmpty)
        {
            var count = stream.Read(destination);
            if (count <= 0) throw new EndOfStreamException("Unexpected end of FLAC stream.");
            destination = destination[count..];
        }
    }

    private sealed record MetadataBlock(int Type, byte[] Data);
    private sealed record FlacDocument(List<MetadataBlock> Blocks, long AudioOffset);

    private ref struct SpanReader(ReadOnlySpan<byte> source)
    {
        private readonly ReadOnlySpan<byte> _source = source;
        private int _offset;

        public uint ReadUInt32LittleEndian() => ReadUInt32(BinaryPrimitives.ReadUInt32LittleEndian);
        public uint ReadUInt32BigEndian() => ReadUInt32(BinaryPrimitives.ReadUInt32BigEndian);

        private uint ReadUInt32(Func<ReadOnlySpan<byte>, uint> reader)
        {
            Ensure(sizeof(uint));
            var value = reader(_source[_offset..]);
            _offset += sizeof(uint);
            return value;
        }

        public void Skip(uint count)
        {
            if (count > int.MaxValue) throw new InvalidDataException("FLAC metadata length is too large.");
            Ensure((int)count);
            _offset += (int)count;
        }

        public string ReadUtf8(uint count)
        {
            if (count > int.MaxValue) throw new InvalidDataException("FLAC text length is too large.");
            var bytes = ReadBytes(count);
            return Utf8.GetString(bytes);
        }

        public byte[] ReadBytes(uint count)
        {
            if (count > int.MaxValue) throw new InvalidDataException("FLAC byte length is too large.");
            Ensure((int)count);
            var result = _source.Slice(_offset, (int)count).ToArray();
            _offset += (int)count;
            return result;
        }

        public void RequireEnd()
        {
            if (_offset != _source.Length) throw new InvalidDataException("FLAC metadata block has trailing bytes.");
        }

        private void Ensure(int count)
        {
            if (count < 0 || count > _source.Length - _offset)
                throw new InvalidDataException("FLAC metadata block is truncated.");
        }
    }
}
