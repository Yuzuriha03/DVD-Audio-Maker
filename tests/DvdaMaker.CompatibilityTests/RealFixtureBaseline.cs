using System.Security.Cryptography;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Formats.Mlp;

internal static class RealFixtureBaseline
{
    private sealed record FileBaseline(int Size, string Sha256);

    private sealed record SectorBaseline(uint Lba, string Sha256);

    private sealed record IsoBaseline(
        string Name,
        long Size,
        int SectorSize,
        uint RootLba,
        uint RootSize,
        string[] Paths,
        IReadOnlyDictionary<string, FileBaseline> Files,
        IReadOnlyDictionary<string, SectorBaseline> SectorSamples);

    private static readonly IsoBaseline[] IsoBaselines =
    [
        new(
            "Wuthering Waves Singles EPs 1.iso",
            4_659_234_816,
            2048,
            283,
            2048,
            [
                "/AUDIO_TS",
                "/AUDIO_TS/ATS_01_0.BUP",
                "/AUDIO_TS/ATS_01_0.IFO",
                "/AUDIO_TS/ATS_01_1.AOB",
                "/AUDIO_TS/ATS_01_2.AOB",
                "/AUDIO_TS/ATS_01_3.AOB",
                "/AUDIO_TS/ATS_01_4.AOB",
                "/AUDIO_TS/ATS_01_5.AOB",
                "/AUDIO_TS/AUDIO_PP.IFO",
                "/AUDIO_TS/AUDIO_SV.BUP",
                "/AUDIO_TS/AUDIO_SV.IFO",
                "/AUDIO_TS/AUDIO_SV.VOB",
                "/AUDIO_TS/AUDIO_TS.BUP",
                "/AUDIO_TS/AUDIO_TS.IFO",
                "/AUDIO_TS/AUDIO_TS.VOB",
                "/VIDEO_TS",
            ],
            new Dictionary<string, FileBaseline>(StringComparer.Ordinal)
            {
                ["/AUDIO_TS/ATS_01_0.BUP"] = new(8192, "87729e4323ffc44500bc9862208db65cf741a73cce3f6a02aad0dd8fba4a88df"),
                ["/AUDIO_TS/ATS_01_0.IFO"] = new(8192, "87729e4323ffc44500bc9862208db65cf741a73cce3f6a02aad0dd8fba4a88df"),
                ["/AUDIO_TS/AUDIO_PP.IFO"] = new(131072, "8ee9a3df956e7ce00dbf40a74f25bf83560d350ca6c65bec7886ba3ee86990a0"),
                ["/AUDIO_TS/AUDIO_SV.BUP"] = new(4096, "8ef3206432a09f9ccd48ea9a2cfb1e39f0a7e208231ac3f85845c6c777217d60"),
                ["/AUDIO_TS/AUDIO_SV.IFO"] = new(4096, "8ef3206432a09f9ccd48ea9a2cfb1e39f0a7e208231ac3f85845c6c777217d60"),
                ["/AUDIO_TS/AUDIO_TS.BUP"] = new(16384, "770afaedf2d8a1251034352e3b56c31a6fa28e7d2049cee3273f7f863d22a64e"),
                ["/AUDIO_TS/AUDIO_TS.IFO"] = new(16384, "770afaedf2d8a1251034352e3b56c31a6fa28e7d2049cee3273f7f863d22a64e"),
            },
            new Dictionary<string, SectorBaseline>(StringComparer.Ordinal)
            {
                ["/AUDIO_TS/ATS_01_1.AOB"] = new(5315, "8d6939c6c60092019cbc5bf1f7146883fd91d9f8a7bab3d9e9f96103c4ac4e9b"),
                ["/AUDIO_TS/AUDIO_SV.VOB"] = new(2003, "ac4d49b5eefd33423a51a01ca4a18eb615d7b2402fb7454daa602d24581071a6"),
            }),
        new(
            "Wuthering Waves Singles EPs 2.iso",
            3_134_130_176,
            2048,
            281,
            2048,
            [
                "/AUDIO_TS",
                "/AUDIO_TS/ATS_01_0.BUP",
                "/AUDIO_TS/ATS_01_0.IFO",
                "/AUDIO_TS/ATS_01_1.AOB",
                "/AUDIO_TS/ATS_01_2.AOB",
                "/AUDIO_TS/ATS_01_3.AOB",
                "/AUDIO_TS/AUDIO_PP.IFO",
                "/AUDIO_TS/AUDIO_SV.BUP",
                "/AUDIO_TS/AUDIO_SV.IFO",
                "/AUDIO_TS/AUDIO_SV.VOB",
                "/AUDIO_TS/AUDIO_TS.BUP",
                "/AUDIO_TS/AUDIO_TS.IFO",
                "/AUDIO_TS/AUDIO_TS.VOB",
                "/VIDEO_TS",
            ],
            new Dictionary<string, FileBaseline>(StringComparer.Ordinal)
            {
                ["/AUDIO_TS/ATS_01_0.BUP"] = new(6144, "9ea0bbc4dac9325912b42edcdabf5b6e980b26555afa835a74d5340b1c49d48d"),
                ["/AUDIO_TS/ATS_01_0.IFO"] = new(6144, "9ea0bbc4dac9325912b42edcdabf5b6e980b26555afa835a74d5340b1c49d48d"),
                ["/AUDIO_TS/AUDIO_PP.IFO"] = new(131072, "7be5bf319f6b4830333cefc48d56a39bc010ef87ac313ea46ad1729d4d72d601"),
                ["/AUDIO_TS/AUDIO_SV.BUP"] = new(4096, "3563b8750a0c45e4955b47e3b89036f043b633dcc04f2560767feac83d5b64ee"),
                ["/AUDIO_TS/AUDIO_SV.IFO"] = new(4096, "3563b8750a0c45e4955b47e3b89036f043b633dcc04f2560767feac83d5b64ee"),
                ["/AUDIO_TS/AUDIO_TS.BUP"] = new(12288, "1c2d8170d14745a09e2610c7a15bf1815a778b4354e9b940932ead3118f29c7c"),
                ["/AUDIO_TS/AUDIO_TS.IFO"] = new(12288, "1c2d8170d14745a09e2610c7a15bf1815a778b4354e9b940932ead3118f29c7c"),
            },
            new Dictionary<string, SectorBaseline>(StringComparer.Ordinal)
            {
                ["/AUDIO_TS/ATS_01_1.AOB"] = new(3282, "599f0cc4668730543f0b70e1a200033e8cc01a728340bbe5943a669db4c9c647"),
                ["/AUDIO_TS/AUDIO_SV.VOB"] = new(1317, "c805e18eb0fd2736d8eb10caa8e815404f92810fa1f68a97714b54121a0aad51"),
            }),
    ];

    private static readonly string MlpRelativePath = Path.Combine(
        "一千万种可能(《鸣潮》先约电台EP1.4) - Single",
        "02. 一千万种可能(伴奏).mlp");

    public static void Run(string isoDirectory, string mlpRoot)
    {
        foreach (var baseline in IsoBaselines)
        {
            VerifyIso(Path.Combine(isoDirectory, baseline.Name), baseline);
            Console.WriteLine($"[PASS] 真实 ISO 对拍: {baseline.Name}");
        }

        VerifyMlp(Path.Combine(mlpRoot, MlpRelativePath));
        Console.WriteLine("[PASS] 真实 SurCode MLP 对拍");
    }

    private static void VerifyIso(string path, IsoBaseline baseline)
    {
        Require(File.Exists(path), $"找不到参考 ISO: {path}");
        Require(new FileInfo(path).Length == baseline.Size, $"ISO 大小不符: {path}");

        using var iso = new Iso9660Reader(path);
        Require(iso.SectorSize == baseline.SectorSize, $"扇区大小不符: {path}");
        Require(iso.RootLogicalBlockAddress == baseline.RootLba, $"根目录 LBA 不符: {path}");
        Require(iso.RootSize == baseline.RootSize, $"根目录大小不符: {path}");
        Require(iso.AllPaths().SequenceEqual(baseline.Paths), $"ISO 路径清单不符: {path}");

        foreach (var pair in baseline.Files)
        {
            var data = iso.ReadFile(pair.Key);
            Require(data.Length == pair.Value.Size, $"文件大小不符: {pair.Key}");
            Require(Hash(data) == pair.Value.Sha256, $"文件 SHA256 不符: {pair.Key}");
        }

        foreach (var pair in baseline.SectorSamples)
        {
            var lba = iso.GetDataLogicalBlockAddress(pair.Key);
            Require(lba == pair.Value.Lba, $"文件 LBA 不符: {pair.Key}");
            Require(Hash(iso.ReadSectors(lba, 4)) == pair.Value.Sha256,
                $"扇区样本 SHA256 不符: {pair.Key}");
        }
    }

    private static void VerifyMlp(string path)
    {
        Require(File.Exists(path), $"找不到参考 MLP: {path}");
        var data = File.ReadAllBytes(path);
        Require(data.Length == 21_246_766, "参考 MLP 大小不符");
        Require(Hash(data) == "1055502891a71c3bebcfd4656e645afd4ad401621cc4c92c7b622f49f60df770",
            "参考 MLP SHA256 不符");

        var inspection = MlpStreamAligner.Inspect(data);
        Require(inspection.Size == 21_246_766, "MLP inspection 大小不符");
        Require(inspection.AccessUnitCount == 124_052, "MLP AU 数量不符");
        Require(inspection.MajorSyncCount == 15_507, "MLP major sync 数量不符");
        Require(inspection.MajorSyncErrors.Count == 0, "MLP major sync 校验失败");
        Require(inspection.AccessUnitParityErrors.Count == 0, "MLP AU 奇偶校验失败");
        Require(inspection.SubstreamErrors.Count == 0, "MLP 子流校验失败");
        Require(inspection.HasEndOfStream, "MLP 缺少 END_OF_STREAM");
        Require(inspection.PeakBitrateRaw == 3200, "MLP peak_bitrate 不符");
        Require(inspection.ExtendedSubstreamInfo == 1, "MLP extended_substream_info 不符");
        Require(inspection.SampleRate == 48_000, "MLP 采样率不符");

        var aligned = MlpStreamAligner.Align(data);
        Require(!aligned.Changes.AddedEndOfStream, "已对齐 MLP 不应再次添加 EOS");
        Require(aligned.Changes.PeakBitrateChanges == 0, "已对齐 MLP 不应修改 peak_bitrate");
        Require(aligned.Changes.ExtendedSubstreamInfoChanges == 0,
            "已对齐 MLP 不应修改 extended_substream_info");
        Require(aligned.Changes.ChecksumChanges == 0, "已对齐 MLP 不应修改 checksum");
        Require(data.AsSpan().SequenceEqual(aligned.Data), "MLP 对齐操作不幂等");
    }

    private static string Hash(ReadOnlySpan<byte> data) =>
        Convert.ToHexStringLower(SHA256.HashData(data));

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
