using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text.Json;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

internal static class EncoderMigrationTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidDataException(message); }

    public static void Run()
    {
        var previous = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-encoder-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var records = new List<object>();
        try
        {
            foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
            foreach (var bits in new[] { 16, 20, 24 })
            for (var channels = 1; channels <= (rate > 96000 ? 2 : 6); channels++)
                Compare(rate, bits, channels, 817, null, 0, false);
            foreach (var frames in new[] { 1, 39, 40, 41, 79, 80, 81, 159, 160, 161, 319, 320, 321, 8191, 8192, 8193 })
                Compare(48000, 24, 2, frames, null, frames % 3, false);
            foreach (var mask in new uint[] { 4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f })
                Compare(48000, 24, System.Numerics.BitOperations.PopCount(mask), 817, mask, 2, false);
            foreach (var bits in new[] { 16, 20, 24 }) Compare(96000, bits, 6, 817, null, 1, true);
            foreach (var scenario in new[] { "existing", "directory", "parent-file", "locked-input", "invalid-wave", "wave-rate-overflow", "missing-wave", "invalid-low-bits",
                "metadata-magic", "metadata-header", "metadata-count", "metadata-size", "metadata-short", "metadata-extra", "metadata-units", "metadata-order", "metadata-bits", "pre-cancel" })
            {
                var expected = Failure("managed", scenario);
                var actual = Failure("rust", scenario);
                Require(expected == actual, $"Encoder failure {scenario}: {expected} / {actual}");
            }
            foreach (var frames in new[] { 0L, -1L, 1L, 40L, 81L, long.MaxValue })
            foreach (var rate in new[] { -1, 0, 48000, 96000, 192000 })
            {
                var expected = Metadata("managed", frames, rate);
                var actual = Metadata("rust", frames, rate);
                Require(expected == actual, $"Metadata result {frames}/{rate}: {expected} / {actual}");
            }
            var report = Environment.GetEnvironmentVariable("DVDA_ENCODER_FIXTURE_REPORT");
            if (!string.IsNullOrEmpty(report)) File.WriteAllText(report, JsonSerializer.Serialize(new
            { Generator = "MlpEncoderTests.WriteWave integer-ramp-v1", Encoder = MlpEncoder.BinarySha256,
                Reference = "Independent pre-migration managed host with pinned C encoder; not original SurCode", Cases = records }, new JsonSerializerOptions { WriteIndented = true }));
            Require(records.Count == 116, "Encoder matrix case count");
            Console.WriteLine("PASS 116 complete MLP byte comparisons, metadata and failure transactions");

            void Compare(int rate, int bits, int channels, int frames, uint? mask, int metadataVersion, bool wide)
            {
                var folder = Path.Combine(root, "case-" + records.Count); Directory.CreateDirectory(folder);
                var wave = Path.Combine(folder, "input.wav");
                MlpEncoderTests.WriteWave(wave, rate, bits, channels, frames);
                if (mask.HasValue)
                {
                    var bytes = File.ReadAllBytes(wave); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(40), mask.Value); File.WriteAllBytes(wave, bytes);
                }
                if (wide) Widen(wave);
                var context = metadataVersion == 0 ? "" : Path.Combine(folder, "metadata.bin");
                if (metadataVersion != 0) WriteContext(context, frames, rate, metadataVersion);
                var expected = Encode("managed", wave, Path.Combine(folder, "reference.mlp"), context);
                var actual = Encode("rust", wave, Path.Combine(folder, "actual.mlp"), context);
                Require(expected.AsSpan().SequenceEqual(actual), $"Entire MLP mismatch: {rate}/{bits}/{channels}/{frames}/{mask}/{metadataVersion}/{wide}");
                records.Add(new { Rate = rate, Bits = bits, Channels = channels, Frames = frames, Mask = mask, MetadataVersion = metadataVersion, Wide = wide,
                    WaveSha256 = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(wave))), MlpSha256 = Convert.ToHexString(SHA256.HashData(expected)), Bytes = expected.Length });
            }
            string Failure(string mode, string scenario)
            {
                var folder = Path.Combine(root, scenario, mode); Directory.CreateDirectory(folder);
                var wave = Path.Combine(folder, "input.wav"); MlpEncoderTests.WriteWave(wave, 48000, 24, 2, 817);
                var output = Path.Combine(folder, "output.mlp"); var context = "";
                if (scenario == "existing") File.WriteAllText(output, "preserve");
                if (scenario == "directory") Directory.CreateDirectory(output);
                if (scenario == "parent-file") { File.WriteAllText(output, "parent"); output = Path.Combine(output, "child.mlp"); }
                if (scenario == "invalid-wave") File.WriteAllText(wave, "broken");
                if (scenario == "wave-rate-overflow") { var bytes = File.ReadAllBytes(wave); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(24), uint.MaxValue); File.WriteAllBytes(wave, bytes); }
                if (scenario == "missing-wave") File.Delete(wave);
                if (scenario == "invalid-low-bits") { Widen(wave); var bytes = File.ReadAllBytes(wave); bytes[68] = 1; File.WriteAllBytes(wave, bytes); }
                if (scenario.StartsWith("metadata-", StringComparison.Ordinal))
                {
                    context = Path.Combine(folder, "context.bin"); WriteContext(context, 817, 48000, 2);
                    var bytes = File.ReadAllBytes(context);
                    switch (scenario)
                    {
                        case "metadata-magic": bytes[0] = 0; break;
                        case "metadata-header": bytes = bytes[..9]; break;
                        case "metadata-count": BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(16), 4097); break;
                        case "metadata-size": BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(28), 3); break;
                        case "metadata-short": bytes = bytes[..^1]; break;
                        case "metadata-extra": bytes = [.. bytes, 1]; break;
                        case "metadata-units": bytes[8] = 99; break;
                        case "metadata-order": bytes[20] = 1; break;
                        case "metadata-bits": BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(32), 999); break;
                    }
                    File.WriteAllBytes(context, bytes);
                }
                using var locked = scenario == "locked-input" ? new FileStream(wave, FileMode.Open, FileAccess.ReadWrite, FileShare.None) : null;
                using var token = new CancellationTokenSource(); if (scenario == "pre-cancel") token.Cancel();
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
                string error;
                try { MlpEncoder.EncodeAsync(wave, output, context, TimeSpan.FromSeconds(30), token.Token).GetAwaiter().GetResult(); error = "SUCCESS"; }
                catch (Exception exception) { error = exception.GetType().Name; }
                Require(error != "SUCCESS", "Failure fixture unexpectedly succeeded: " + scenario);
                Require(!Directory.EnumerateFiles(folder, "*.partial", SearchOption.AllDirectories).Any(), "Temporary output leaked: " + scenario);
                var state = File.Exists(output) ? Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(output))) : Directory.Exists(output) ? "DIRECTORY" : "MISSING";
                return error + ":" + state;
            }
            string Metadata(string mode, long frames, int rate)
            {
                var path = Path.Combine(root, Guid.NewGuid() + ".metadata");
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
                try { MlpEncoder.WriteMetadata(path, frames, rate); return Convert.ToHexString(File.ReadAllBytes(path)); }
                catch (Exception error) { Require(!File.Exists(path), "Failed metadata created output"); return error.GetType().Name; }
            }
        }
        finally { Environment.SetEnvironmentVariable("DVDA_RUST_MODE", previous); Directory.Delete(root, true); }
    }
    private static byte[] Encode(string mode, string wave, string output, string context)
    {
        Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
        MlpEncoder.EncodeAsync(wave, output, context, TimeSpan.FromSeconds(30), CancellationToken.None).GetAwaiter().GetResult();
        return File.ReadAllBytes(output);
    }
    private static void WriteContext(string path, int frames, int rate, int version)
    {
        var block = 40 * (rate / (rate % 44100 == 0 ? 44100 : 48000));
        using var writer = new BinaryWriter(File.Create(path));
        writer.Write(version == 1 ? "MSCTX001"u8 : "MSCTX002"u8);
        writer.Write((ulong)((frames + block - 1) / block)); writer.Write(1u); writer.Write(0UL); writer.Write(4u);
        if (version == 2) writer.Write(0u); writer.Write(new byte[] { 0, 0, 0x40, 0 });
    }
    private static void Widen(string path)
    {
        var bytes = File.ReadAllBytes(path); var channels = BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(22));
        var size = BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(64)); var width = BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(34)) / 8;
        var frames = (int)size / (channels * width); var widened = new byte[68 + frames * channels * 4];
        bytes.AsSpan(0, 68).CopyTo(widened);
        BinaryPrimitives.WriteUInt32LittleEndian(widened.AsSpan(4), (uint)widened.Length - 8);
        BinaryPrimitives.WriteUInt32LittleEndian(widened.AsSpan(28), BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(24)) * channels * 4);
        BinaryPrimitives.WriteUInt16LittleEndian(widened.AsSpan(32), (ushort)(channels * 4));
        BinaryPrimitives.WriteUInt16LittleEndian(widened.AsSpan(34), 32);
        BinaryPrimitives.WriteUInt32LittleEndian(widened.AsSpan(64), (uint)widened.Length - 68);
        for (var index = 0; index < frames * channels; index++)
        {
            var sample = width == 2 ? BinaryPrimitives.ReadInt16LittleEndian(bytes.AsSpan(68 + index * width)) << 16
                : (bytes[68 + index * width] | bytes[69 + index * width] << 8 | bytes[70 + index * width] << 16) << 8;
            BinaryPrimitives.WriteInt32LittleEndian(widened.AsSpan(68 + index * 4), sample);
        }
        File.WriteAllBytes(path, widened);
    }
}
