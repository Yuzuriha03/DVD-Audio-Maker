using System.Buffers.Binary;
using System.Text;
using DvdaMaker.Formats.Iso9660;

internal static class IsoMigrationTests
{
    public static void CompareFileOperations()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-rust-iso", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var path = Path.Combine(root, "中文-日本語.iso");
            var bytes = new byte[24 * 2048];
            var pvd = bytes.AsSpan(16 * 2048, 2048);
            pvd[0] = 1; "CD001"u8.CopyTo(pvd[1..]); pvd[6] = 1;
            pvd.Slice(40, 32).Fill(32); "RUST ISO FIXTURE"u8.CopyTo(pvd[40..]);
            BinaryPrimitives.WriteUInt16LittleEndian(pvd[128..], 2048);
            BinaryPrimitives.WriteUInt16BigEndian(pvd[130..], 2048);
            Record([0], 20, 2048, true).CopyTo(pvd[156..]);
            var rootRecords = new[] { Record([0], 20, 2048, true), Record([1], 20, 2048, true), Record("AUDIO_TS"u8.ToArray(), 21, 2048, true) };
            var audioRecords = new[] { Record([0], 21, 2048, true), Record([1], 20, 2048, true),
                Record("ATS_01_0.IFO;1"u8.ToArray(), 22, 5, false, 1), Record("EMPTY.BIN;1"u8.ToArray(), 24, 0, false) };
            rootRecords.SelectMany(item => item).ToArray().CopyTo(bytes, 20 * 2048);
            audioRecords.SelectMany(item => item).ToArray().CopyTo(bytes, 21 * 2048);
            byte[] payload = [0, 1, 128, 254, 255];
            payload.CopyTo(bytes, 23 * 2048);
            File.WriteAllBytes(path, bytes);
            using (var iso = new Iso9660Reader(path))
            {
                Check(iso.VolumeIdentifier == "RUST ISO FIXTURE", "PVD volume");
                Check(iso.ListDirectory().Single().Name == "AUDIO_TS", "root directory");
                Check(iso.ListDirectory("AUDIO_TS").Count == 2, "nested directory");
                Check(iso.GetEntry("missing") is null, "missing entry");
                Check(iso.ListDirectory("missing").Count == 0, "missing listing");
                Check(iso.GetDataLogicalBlockAddress("audio_ts/ats_01_0.ifo") == 23, "extended-attribute LBA");
                Check(iso.AllPaths().SequenceEqual(new[] { "/AUDIO_TS", "/AUDIO_TS/ATS_01_0.IFO", "/AUDIO_TS/EMPTY.BIN" }), "recursive paths");
                Check(iso.ReadFile("AUDIO_TS/ATS_01_0.IFO").SequenceEqual(payload), "payload bytes");
                Check(iso.ReadFile("AUDIO_TS/EMPTY.BIN").Length == 0, "zero-length file");
                var extracted = Path.Combine(root, "extracted");
                Check(iso.Extract("AUDIO_TS", extracted), "directory extraction");
                Check(File.ReadAllBytes(Path.Combine(extracted, "ATS_01_0.IFO")).SequenceEqual(payload), "extracted bytes");
                Check(new FileInfo(Path.Combine(extracted, "EMPTY.BIN")).Length == 0, "extracted empty file");
                Check(!iso.Extract("missing", Path.Combine(root, "absent")), "absent extraction");
            }
            // Existing reader returns the available prefix for a truncated payload.
            using (var file = new FileStream(path, FileMode.Open, FileAccess.Write)) file.SetLength(23 * 2048 + 3);
            using (var iso = new Iso9660Reader(path))
            {
                Check(iso.ReadFile("AUDIO_TS/ATS_01_0.IFO").SequenceEqual(payload[..3]), "truncated payload");
                var extracted = Path.Combine(root, "partial.bin");
                Check(iso.Extract("AUDIO_TS/ATS_01_0.IFO", extracted), "truncated extraction");
                Check(File.ReadAllBytes(extracted).SequenceEqual(payload[..3]), "truncated extracted bytes");
            }
        }
        finally { Directory.Delete(root, true); }
    }

    private static byte[] Record(byte[] name, uint lba, uint size, bool directory, byte attributes = 0)
    {
        var record = new byte[33 + name.Length + (name.Length % 2 == 0 ? 1 : 0)];
        record[0] = (byte)record.Length; record[1] = attributes;
        BinaryPrimitives.WriteUInt32LittleEndian(record.AsSpan(2), lba);
        BinaryPrimitives.WriteUInt32BigEndian(record.AsSpan(6), lba);
        BinaryPrimitives.WriteUInt32LittleEndian(record.AsSpan(10), size);
        BinaryPrimitives.WriteUInt32BigEndian(record.AsSpan(14), size);
        record[25] = directory ? (byte)2 : (byte)0;
        record[28] = 1; record[31] = 1; record[32] = (byte)name.Length;
        name.CopyTo(record, 33); return record;
    }
    private static void Check(bool value, string context)
    { if (!value) throw new InvalidOperationException("ISO fixture: " + context); }
}
