using System.Buffers.Binary;
using System.Security.Cryptography;
using DvdaMaker.SurcodeTool;

internal static class PcmMigrationTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidDataException(message); }
    public static void Run()
    {
        var previous = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-pcm-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var count = 0;
        try
        {
            foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
            foreach (var bits in new[] { 16, 20, 24 })
            for (var channels = 1; channels <= (rate > 96000 ? 2 : 6); channels++)
            foreach (var target in new[] { 16, 20, 24 })
            {
                var input = Path.Combine(root, "input.wav");
                MlpEncoderTests.WriteWave(input, rate, bits, channels, 8193);
                Compare(input, rate, target);
            }
            foreach (var bits in new[] { 16, 20, 24 })
            foreach (var target in new[] { 16, 20, 24 })
            {
                var input = Path.Combine(root, "wide.wav");
                MlpEncoderTests.WriteWave(input, 48000, bits, 6, 817); EncoderMigrationTests.Widen(input);
                Compare(input, 48000, target);
            }
            foreach (var scenario in new[] { "side", "side-rear", "side-back-center", "invalid-low-bits", "invalid-header", "empty", "rate", "bits", "existing", "directory", "parent-file", "missing", "locked", "pre-cancel" })
            {
                var folder = Path.Combine(root, scenario); Directory.CreateDirectory(folder);
                var input = Path.Combine(folder, "input.wav"); MlpEncoderTests.WriteWave(input, 48000, 24, 6, 817);
                var bytes = File.ReadAllBytes(input);
                if (scenario.StartsWith("side", StringComparison.Ordinal))
                    BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(40), scenario switch { "side" => 0x60fu, "side-rear" => 0x22fu, _ => 0x30fu });
                if (scenario == "invalid-header") bytes[0] = 0;
                if (scenario == "empty") bytes = [];
                File.WriteAllBytes(input, bytes);
                if (scenario == "invalid-low-bits") { EncoderMigrationTests.Widen(input); bytes = File.ReadAllBytes(input); bytes[68] = 1; File.WriteAllBytes(input, bytes); }
                if (scenario == "missing") File.Delete(input);
                using var locked = scenario == "locked" ? new FileStream(input, FileMode.Open, FileAccess.ReadWrite, FileShare.None) : null;
                Compare(input, scenario == "rate" ? 96000 : 48000, scenario == "bits" ? 32 : 24, scenario);
            }
            Console.WriteLine($"PASS {count} isolated PCM normalization comparisons");

            void Compare(string input, int rate, int bits, string scenario = "normal")
            {
                string Execute(string mode)
                {
                    var destination = Path.Combine(root, $"{count}-{mode}.wav");
                    if (scenario == "existing") File.WriteAllText(destination, "preserve");
                    if (scenario == "directory") Directory.CreateDirectory(destination);
                    if (scenario == "parent-file") { File.WriteAllText(destination, "parent"); destination = Path.Combine(destination, "child.wav"); }
                    Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
                    using var token = new CancellationTokenSource(); if (scenario == "pre-cancel") token.Cancel();
                    var error = "OK";
                    try { SurcodePcmWav.Normalize(input, destination, rate, bits, token.Token); }
                    catch (Exception exception) { error = exception.GetType().Name; }
                    var output = File.Exists(destination) ? Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(destination)))
                        : Directory.Exists(destination) ? "DIRECTORY" : "MISSING";
                    if (scenario is "side-rear" or "side-back-center")
                        Require(error == nameof(InvalidDataException) && output == "MISSING", "Conflicting surround layout accepted");
                    if (scenario == "normal") Require(error == "OK" || (error == nameof(InvalidDataException) && output == "MISSING"), "Unexpected normalization failure");
                    return error + ":" + output;
                }
                var expected = Execute("managed"); var actual = Execute("rust");
                Require(expected == actual, $"PCM normalization {count}, {scenario}: {expected} / {actual}"); count++;
            }
        }
        finally { Environment.SetEnvironmentVariable("DVDA_RUST_MODE", previous); Directory.Delete(root, true); }
    }
}
