using System.Text.Json;
using DvdaMaker.Building;

internal static class AobMigrationTests
{
    private static void Require(bool value, string message)
    {
        if (!value) throw new Exception(message);
    }

    private static byte[] Sector(long pts, int marker = 4, int length = 2048)
    {
        var sector = new byte[length];
        new byte[] { 0, 0, 1, 0xBA }.CopyTo(sector, 0);
        new byte[] { 0, 0, 1, 0xBD }.CopyTo(sector, marker);
        sector[marker + 7] = 0x80;
        new byte[] { (byte)(0x21 | ((pts >> 30) & 7) << 1),
            (byte)(pts >> 22), (byte)(((pts >> 15) & 127) << 1 | 1),
            (byte)(pts >> 7), (byte)((pts & 127) << 1 | 1) }.CopyTo(sector, marker + 9);
        return sector;
    }

    internal static void ScanBoundaries()
    {
        foreach (var value in new long[] { 0, 1, 90_000, 0x1ffffffff })
        {
            foreach (var marker in new[] { 4, 50, 60 })
            {
                var sector = Sector(value, marker);
                Require(AobSectorScanner.Audit(sector) == value, "PTS audit bit/offset boundary");
                var guarded = new byte[sector.Length + 17];
                sector.CopyTo(guarded, 9);
                Require(AobSectorScanner.Scan(guarded.AsMemory(9, sector.Length), 2048, true)[0] == value,
                    "Pinned memory slice offset was lost");
                sector[3] = 0;
                Require(AobSectorScanner.Audit(sector) == value, "Audit must not require pack header");
                Require(AobSectorScanner.Scan(sector, 2048, true)[0] == -1, "Timeline must require pack header");
            }
        }
        foreach (var length in new[] { 0, 1, 63 })
            Require(AobSectorScanner.Audit(new byte[length]) == -2, "Short audit record must be skipped");
        Require(AobSectorScanner.Audit(Sector(17, 50, 64)) == 17, "Last complete PTS in short record");
        Require(AobSectorScanner.Audit(Sector(17, 60)[..73]) == -1, "Truncated PTS must be missing");
        Require(AobSectorScanner.Audit(Sector(17, 61)) == -1, "Marker crossing search window must be ignored");
        var noFlag = Sector(17); noFlag[11] = 0;
        Require(AobSectorScanner.Audit(noFlag) == -1, "Missing PTS flag");
        var noMarkers = Sector(17);
        noMarkers[13] &= 0xFE; noMarkers[15] &= 0xFE; noMarkers[17] &= 0xFE;
        Require(AobSectorScanner.Audit(noMarkers) == 17, "Preserve existing marker-bit tolerance");

        var enumerated = 0;
        IEnumerable<ReadOnlyMemory<byte>> Chunks()
        {
            enumerated++; yield return Sector(9);
            enumerated++; throw new Exception("Read beyond sector limit");
        }
        Require(AobPtsAnalyzer.AnalyzeChunks(Chunks(), maximumSectors: 1).SectorCount == 1 && enumerated == 1,
            "Maximum sector count must stop the source iterator");
        var split = AobPtsAnalyzer.AnalyzeChunks([Sector(9)[..1000], Sector(9)[1000..]]);
        Require(split.SectorCount == 0, "Existing chunk API discards partial records per chunk");
    }

    internal static void FileStreaming()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-aob-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var path = Path.Combine(root, "timeline.aob");
        try
        {
            var data = Enumerable.Range(0, 139).SelectMany(i => Sector(i * 100L)).Concat(new byte[111]).ToArray();
            File.WriteAllBytes(path, data);
            foreach (int? limit in new int?[] { null, -1, 0, 1, 63, 64, 65, 139, 200 })
            {
                var file = AobPtsAnalyzer.Analyze(path, limit);
                var memory = AobPtsAnalyzer.Analyze(data, path, limit);
                Require(JsonSerializer.Serialize(file) == JsonSerializer.Serialize(memory), "Stream/file PTS mismatch");
            }
            File.WriteAllBytes(path, []);
            Require(AobPtsAnalyzer.Analyze(path).SectorCount == 0, "Empty AOB");
            using (var locked = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                try { AobPtsAnalyzer.Analyze(path); throw new Exception("Locked AOB was accepted"); }
                catch (IOException error) { Require((error.HResult & 0xffff) == 32, "Lock failure category"); }
            }
            File.Delete(path);
            try { AobPtsAnalyzer.Analyze(path); throw new Exception("Missing AOB was accepted"); }
            catch (FileNotFoundException) { }
        }
        finally { Directory.Delete(root, recursive: true); }
    }
}
