using System.Buffers.Binary;
using System.Text;
using DvdaMaker.Preparation;

internal static class FlacMetadataTests
{
    public static void InProcessEditor()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-flac-metadata", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var path = Path.Combine(root, "fixture.flac");
            var image = new byte[] { 0xFF, 0xD8, 0xFF, 0xD9, 0x01 };
            var audio = new byte[] { 0xF8, 0x69, 0x00, 0x00, 0x00, 0x00, 0x3A, 0x11 };
            File.WriteAllBytes(path, BuildFlac(image, audio));

            var comments = FlacMetadataEditor.ReadVorbisComments(path);
            Equal(2, comments.Count);
            Equal(("TITLE", "测试曲目"), comments[0]);
            Equal(("ALBUM", "测试专辑"), comments[1]);
            var picture = FlacMetadataEditor.ReadPicture(path)
                ?? throw new InvalidDataException("PICTURE block was not read");
            Equal(new FlacPictureDescriptor(3, "image/jpeg", 120, 80, 24), picture.Descriptor);
            SequenceEqual(image, picture.ImageData);

            var exported = Path.Combine(root, "cover.jpg");
            FlacMetadataEditor.ExportPicture(path, exported);
            SequenceEqual(image, File.ReadAllBytes(exported));

            var replacementImage = new byte[] { 1, 2, 3, 4, 5, 6 };
            File.WriteAllBytes(exported, replacementImage);
            FlacMetadataEditor.ReplacePicture(path, exported, picture);
            var rewritten = FlacMetadataEditor.ReadPicture(path)
                ?? throw new InvalidDataException("rewritten PICTURE block was not read");
            Equal(picture.Descriptor, rewritten.Descriptor);
            Equal(picture.Description, rewritten.Description);
            Equal(picture.Colors, rewritten.Colors);
            SequenceEqual(replacementImage, rewritten.ImageData);
            SequenceEqual(comments, FlacMetadataEditor.ReadVorbisComments(path));

            var output = File.ReadAllBytes(path);
            SequenceEqual(audio, output[^audio.Length..]);
        }
        finally
        {
            if (Directory.Exists(root)) Directory.Delete(root, recursive: true);
        }
    }

    private static byte[] BuildFlac(byte[] image, byte[] audio)
    {
        var streamInfo = new byte[34];
        var comments = BuildComments(("TITLE", "测试曲目"), ("ALBUM", "测试专辑"));
        var picture = BuildPicture(image);
        using var output = new MemoryStream();
        output.Write("fLaC"u8);
        WriteBlock(output, 0, streamInfo, last: false);
        WriteBlock(output, 4, comments, last: false);
        WriteBlock(output, 6, picture, last: true);
        output.Write(audio);
        return output.ToArray();
    }

    private static byte[] BuildComments(params (string Key, string Value)[] comments)
    {
        using var output = new MemoryStream();
        WriteUInt32LittleEndian(output, (uint)Encoding.UTF8.GetByteCount("test-vendor"));
        output.Write(Encoding.UTF8.GetBytes("test-vendor"));
        WriteUInt32LittleEndian(output, (uint)comments.Length);
        foreach (var comment in comments)
        {
            var bytes = Encoding.UTF8.GetBytes(comment.Key + "=" + comment.Value);
            WriteUInt32LittleEndian(output, (uint)bytes.Length);
            output.Write(bytes);
        }
        return output.ToArray();
    }

    private static byte[] BuildPicture(byte[] image)
    {
        using var output = new MemoryStream();
        WriteUInt32BigEndian(output, 3);
        WriteText(output, "image/jpeg");
        WriteText(output, "front cover");
        WriteUInt32BigEndian(output, 120);
        WriteUInt32BigEndian(output, 80);
        WriteUInt32BigEndian(output, 24);
        WriteUInt32BigEndian(output, 0);
        WriteUInt32BigEndian(output, (uint)image.Length);
        output.Write(image);
        return output.ToArray();
    }

    private static void WriteBlock(Stream output, int type, byte[] data, bool last)
    {
        output.WriteByte((byte)(type | (last ? 0x80 : 0)));
        output.WriteByte((byte)(data.Length >> 16));
        output.WriteByte((byte)(data.Length >> 8));
        output.WriteByte((byte)data.Length);
        output.Write(data);
    }

    private static void WriteText(Stream output, string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value);
        WriteUInt32BigEndian(output, (uint)bytes.Length);
        output.Write(bytes);
    }

    private static void WriteUInt32LittleEndian(Stream output, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes, value);
        output.Write(bytes);
    }

    private static void WriteUInt32BigEndian(Stream output, uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        output.Write(bytes);
    }

    private static void Equal<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new InvalidDataException($"Expected {expected}, got {actual}");
    }

    private static void SequenceEqual<T>(IEnumerable<T> expected, IEnumerable<T> actual)
    {
        if (!expected.SequenceEqual(actual)) throw new InvalidDataException("Sequences differ");
    }
}
