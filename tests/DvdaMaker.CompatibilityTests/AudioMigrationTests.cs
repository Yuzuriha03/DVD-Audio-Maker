using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Nodes;
using DvdaMaker.Building;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

internal static class AudioMigrationTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidDataException(message); }

    public static void Run()
    {
        var previous = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-audio-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var records = new List<object>();
        try
        {
            var runner = new ProcessRunner();
            var metadata = new AudioMetadataReader(runner, BuiltinMedia.Probe);
            var parameters = new AudioParameterProbe(runner, BuiltinMedia.Probe);
            var decoder = new DecodeValidator(runner, BuiltinMedia.Converter);
            var wave = Path.Combine(root, "input.wav");
            var flac = Path.Combine(root, "input.flac");
            var rates = new[] { 44100, 48000, 88200, 96000, 176400, 192000 };
            foreach (var rate in rates)
            foreach (var bits in new[] { 16, 20, 24 })
            for (var channels = 1; channels <= (rate > 96000 ? 2 : 6); channels++)
            {
                MlpEncoderTests.WriteWave(wave, rate, bits, channels, 817);
                var waveHash = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(wave)));
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "managed");
                var conversion = runner.RunAsync(new ProcessRequest { FileName = BuiltinMedia.Converter,
                    Arguments = ["-y", "-i", wave, "-c:a", "flac", "-metadata", "TITLE= 曲目 日本語 🎵  ",
                        "-metadata", "ALBUM=Album=中文", "-metadata", "DATE=2026-01-02", "-metadata", "TRACK=03/12", flac]
                }).GetAwaiter().GetResult();
                Require(conversion.Succeeded, "FLAC fixture generation failed");
                foreach (var input in new[] { wave, flac })
                {
                    var expectedMetadata = Compare(() => metadata.ReadAsync(input).GetAwaiter().GetResult());
                    var expectedParameters = Compare(() => parameters.ProbeAsync(input).GetAwaiter().GetResult());
                    var decodes = new List<object>();
                    foreach (var target in new int?[] { null, 44100, 48000, 88200, 96000, 176400, 192000 })
                    {
                        var decoded = Compare(() => decoder.CheckAsync(input, target).GetAwaiter().GetResult());
                        Require(decoded.ErrorCount == 0 && decoded.ExitCode == 0 && decoded.Samples > 0, "Valid audio decode failed");
                        decodes.Add(new { Target = target, Result = decoded });
                    }
                    records.Add(new { Rate = rate, Bits = bits, Channels = channels, Frames = 817,
                        Format = Path.GetExtension(input)[1..], WaveSha256 = waveHash,
                        Metadata = expectedMetadata with { Path = Path.GetFileName(input) }, Parameters = expectedParameters, Decodes = decodes });
                }
            }
            foreach (var scenario in new[] { "missing", "malformed", "directory", "locked", "pre-cancel" })
            {
                var input = Path.Combine(root, scenario + ".wav");
                MlpEncoderTests.WriteWave(input, 48000, 24, 2, 817);
                if (scenario == "missing") File.Delete(input);
                if (scenario == "malformed") File.WriteAllText(input, "broken audio");
                if (scenario == "directory") { File.Delete(input); Directory.CreateDirectory(input); }
                using var locked = scenario == "locked" ? new FileStream(input, FileMode.Open, FileAccess.ReadWrite, FileShare.None) : null;
                using var token = new CancellationTokenSource(); if (scenario == "pre-cancel") token.Cancel();
                Compare(() => Capture(() => metadata.ReadAsync(input, token.Token).GetAwaiter().GetResult()));
                Compare(() => Capture(() => parameters.ProbeAsync(input, token.Token).GetAwaiter().GetResult()));
                Compare(() => Capture(() => decoder.CheckAsync(input, null, token.Token).GetAwaiter().GetResult()));
            }
            foreach (var target in new[] { -1, 0, 1, 384000 })
                Compare(() => Capture(() => decoder.CheckAsync(wave, target).GetAwaiter().GetResult()));

            var report = Environment.GetEnvironmentVariable("DVDA_AUDIO_FIXTURE_REPORT");
            if (!string.IsNullOrEmpty(report)) File.WriteAllText(report, JsonSerializer.Serialize(new {
                Generator = "MlpEncoderTests.WriteWave integer-ramp-v1", MediaIdentity = BuiltinMedia.Identity,
                Reference = "Independent pre-migration managed audio host with source-built C media component", Cases = records
            }, new JsonSerializerOptions { WriteIndented = true }));
            Require(records.Count == 168, "Audio inspection matrix case count");
            Console.WriteLine("PASS 168 audio profiles: metadata, parameters, 1176 decode/sample comparisons and failure cases");

            T Compare<T>(Func<T> read)
            {
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "managed"); var expected = read();
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "rust"); var actual = read();
                Require(JsonNode.DeepEquals(JsonSerializer.SerializeToNode(expected), JsonSerializer.SerializeToNode(actual)),
                    $"Audio inspection mismatch: expected {JsonSerializer.Serialize(expected)}, actual {JsonSerializer.Serialize(actual)}");
                return expected;
            }
        }
        finally { Environment.SetEnvironmentVariable("DVDA_RUST_MODE", previous); Directory.Delete(root, true); }
    }
    private static object Capture<T>(Func<T> action)
    {
        try { return new { Result = action() }; }
        catch (Exception error) { return new { Error = error is OperationCanceledException ? nameof(OperationCanceledException) : error.GetType().Name }; }
    }
}
