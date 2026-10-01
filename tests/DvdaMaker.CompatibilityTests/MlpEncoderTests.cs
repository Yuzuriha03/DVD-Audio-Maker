using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text.Json;
using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

internal static class MlpEncoderTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidOperationException(message); }

    public static void Metadata()
    {
        var root = NewRoot();
        try
        {
            var path = Path.Combine(root, "context.bin");
            MlpEncoder.WriteMetadata(path, 81, 96000);
            var bytes = File.ReadAllBytes(path);
            Require(bytes.Length == 36 && bytes.AsSpan(0, 8).SequenceEqual("MSCTX001"u8), "Metadata framing");
            Require(BinaryPrimitives.ReadUInt64LittleEndian(bytes.AsSpan(8)) == 2, "AU count must include only final padding");
            Require(bytes.AsSpan(32).SequenceEqual(new byte[] { 0, 0, 0x40, 0 }), "No historical machine/license metadata in default packet");
        }
        finally { Directory.Delete(root, true); }
    }

    public static void EmbeddedCore()
    {
        var executable = MlpEncoder.ExtractLibrary();
        using var stream = File.OpenRead(executable);
        Require(Convert.ToHexString(SHA256.HashData(stream)).Equals(MlpEncoder.BinarySha256, StringComparison.OrdinalIgnoreCase), "Pinned encoder hash");
        Require(executable == MlpEncoder.ExtractLibrary(), "Extraction cache");
        var options = new ConfigLoader(new Dictionary<string, string?>()).Load(workingDirectory: NewRootForConfig());
        Require(options.MlpSource == "surcode-batch", "Default must use encoder");
    }

    // No directory is created: a nonexistent path prevents discovery of the user's local config.
    private static string NewRootForConfig() => Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"));
    private static string NewRoot()
    { var path = Path.Combine(Path.GetTempPath(), "dvda--中文-" + Guid.NewGuid().ToString("N")); Directory.CreateDirectory(path); return path; }

    private static string FixtureExecutable(string root, string name)
    {
        foreach (var path in Directory.EnumerateFiles(AppContext.BaseDirectory))
        {
            var extension = Path.GetExtension(path);
            if (extension is ".dll" or ".json") File.Copy(path, Path.Combine(root, Path.GetFileName(path)), true);
        }
        var result = Path.Combine(root, name);
        File.Copy(Environment.ProcessPath!, result);
        return result;
    }

    private static DvdaOptions Options(string root, string eac3to, string probe, int jobs = 1, int bits = 24, int rate = 48000)
    {
        var path = Path.Combine(root, "config.env");
        var lines = new[]{"DVDA_SRC="+Path.Combine(root,"src"),"DVDA_FINAL_DIR="+Path.Combine(root,"final"),
            "DVDA_BUILD_DIR="+Path.Combine(root,"build"),"DVDA_MLP_SOURCE=surcode-batch",
            "DVDA_MLP_EAC3TO_EXE="+eac3to,"DVDA_FFPROBE="+probe,"DVDA_MLP_JOBS="+jobs,
            "DVDA_MLP_SURCODE_BITS="+bits,"DVDA_MLP_SURCODE_SAMPLE_RATE="+rate};
        File.WriteAllLines(path, lines);
        return new ConfigLoader(new Dictionary<string, string?>()).Load(path);
    }

    private static BuildTrack Track(string source, int index = 1) => new()
    {
        Date = "2026",
        Track = index.ToString(),
        Title = "Synthetic " + index,
        Album = "Test",
        SampleRate = 48000,
        Bits = 24,
        Channels = 2,
        SourcePath = source,
        SourceSize = new FileInfo(source).Length,
        Duration = 0.1,
        ManifestName = Path.GetFileName(source),
        MlpPath = "",
        MlpSize = 0,
    };

    public static void Cache()
    {
        var root = NewRoot(); var saved = Environment.GetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE");
        try
        {
            var template = Path.Combine(root, "template.wav"); WriteWave(template, 48000, 24, 2, 817);
            Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", template);
            var options = Options(root, FixtureExecutable(root, "fake-eac3to.exe"), FixtureExecutable(root, "fake-ffprobe.exe"));
            Directory.CreateDirectory(options.SourceDirectory);
            var source = Path.Combine(options.SourceDirectory, "source.flac"); File.WriteAllBytes(source, new byte[4096]);
            var track = Track(source); var provider = new SurcodeMlpProvider(options, new ProcessRunner());
            var first = provider.AcquireAsync([track]).GetAwaiter().GetResult();
            Require(first.CacheRebuilt == 1 && first.Diagnostics.Count == 0, "First encode: " + string.Join(';', first.Diagnostics.Select(d => d.Message)));
            var output = first.Tracks[0].MlpPath; var original = File.ReadAllBytes(output);
            Require(MlpCacheValidator.IsEncoderValid(output), "Small original-policy MLP must pass validation");
            var second = provider.AcquireAsync([track]).GetAwaiter().GetResult();
            Require(second.CacheHits == 1 && second.CacheRebuilt == 0, "Valid cache must be reused");
            var timestamp = File.GetLastWriteTimeUtc(source); var changed = new byte[4096]; changed[0] = 42;
            File.WriteAllBytes(source, changed); File.SetLastWriteTimeUtc(source, timestamp);
            Require(provider.AcquireAsync([track]).GetAwaiter().GetResult().CacheRebuilt == 1, "Same size/time content change must invalidate");
            File.WriteAllBytes(output, [1, 2, 3]);
            Require(provider.AcquireAsync([track]).GetAwaiter().GetResult().CacheRebuilt == 1, "Output corruption must invalidate");
            File.Delete(MlpCacheIndex.PathFor(options.MlpExternalDirectory));
            Require(provider.AcquireAsync([track]).GetAwaiter().GetResult().CacheRebuilt == 1, "Old encoder output without provenance must invalidate");
            Require(File.ReadAllBytes(output).SequenceEqual(original), "Identical PCM/default metadata must produce identical complete bytes");
            // The core rejects malformed PCM; an old published output must survive a failed rebuild.
            File.WriteAllBytes(template, [1, 2, 3]); File.Delete(MlpCacheIndex.PathFor(options.MlpExternalDirectory));
            var failed = provider.AcquireAsync([track]).GetAwaiter().GetResult();
            Require(failed.Diagnostics.Any(d => d.Severity == BuildDiagnosticSeverity.Error), "Invalid PCM must fail");
            Require(File.ReadAllBytes(output).SequenceEqual(original), "Failed rebuild must preserve published file");
        }
        finally { Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", saved); Directory.Delete(root, true); }
    }

    public static void Parallel()
    {
        var root = NewRoot(); var saved = Environment.GetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE");
        try
        {
            var wave = Path.Combine(root, "source.wav"); WriteWave(wave, 48000, 24, 2, 1457);
            Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", wave);
            var decoder = FixtureExecutable(root, "fake-eac3to.exe");
            var tracks = Enumerable.Range(1, 4).Select(n => new SurcodeEncodingTrack
            { SourcePath = wave, WorkName = "track" + n, DisplayName = "Track " + n, DurationSeconds = 0.1, SourceSampleRate = 48000, SourceBits = 24 }).ToArray();
            var job = new SurcodeEncodingJob
            {
                Eac3toExecutable = decoder,
                TemporaryDirectory = Path.Combine(root, "temp"),
                OutputDirectory = Path.Combine(root, "parallel"),
                SampleRate = 48000,
                Bits = 24,
                Jobs = 4,
                Tracks = tracks
            };
            var encoder = new SurcodeBatchEncoder(new ProcessRunner());
            encoder.RunAsync(job, CancellationToken.None).GetAwaiter().GetResult();
            encoder.RunAsync(job with { Jobs = 1, OutputDirectory = Path.Combine(root, "serial") }, CancellationToken.None).GetAwaiter().GetResult();
            foreach (var track in tracks)
                Require(File.ReadAllBytes(Path.Combine(root, "parallel", track.WorkName + ".mlp")).SequenceEqual(
                    File.ReadAllBytes(Path.Combine(root, "serial", track.WorkName + ".mlp"))), "Serial/parallel bytes differ");
            var existing = File.ReadAllBytes(Path.Combine(job.OutputDirectory, "track1.mlp"));
            try { encoder.RunAsync(job, CancellationToken.None).GetAwaiter().GetResult(); throw new Exception("Existing output accepted"); }
            catch (IOException) { }
            Require(File.ReadAllBytes(Path.Combine(job.OutputDirectory, "track1.mlp")).SequenceEqual(existing), "Existing output was modified");
        }
        finally { Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", saved); Directory.Delete(root, true); }
    }

    public static void DllCancellation()
    {
        var root = NewRoot();
        try
        {
            Require(Path.GetExtension(MlpEncoder.ExtractLibrary()) == ".dll", "Encoder must load a DLL");
            var wave = Path.Combine(root, "input.wav");
            WriteWave(wave, 48000, 24, 2, 960017);
            var output = Path.Combine(root, "canceled.mlp");
            using var cancellation = new CancellationTokenSource();
            var operation = MlpEncoder.EncodeAsync(wave, output, "", TimeSpan.FromSeconds(30), cancellation.Token);
            for (var i = 0; i < 1000 && !operation.IsCompleted && !Directory.EnumerateFiles(root, "*.partial").Any(); i++)
                Thread.Sleep(2);
            Require(!operation.IsCompleted, "Cancellation fixture finished before entering the native call");
            cancellation.Cancel();
            try { operation.GetAwaiter().GetResult(); throw new Exception("Cancellation was ignored"); }
            catch (OperationCanceledException) { }
            Require(!File.Exists(output) && !Directory.EnumerateFiles(root, "*.partial").Any(), "Canceled native output was published or leaked");
            File.WriteAllBytes(output, [1, 2, 3]);
            try { MlpEncoder.EncodeAsync(wave, output, "", TimeSpan.FromSeconds(10), CancellationToken.None).GetAwaiter().GetResult(); throw new Exception("Existing native output was overwritten"); }
            catch (IOException) { }
            Require(File.ReadAllBytes(output).SequenceEqual(new byte[] { 1, 2, 3 }), "Existing output changed");
            var context = Path.Combine(root, "bad.stampctx"); MlpEncoder.WriteMetadata(context, 40, 48000);
            var mismatch = Path.Combine(root, "mismatch.mlp");
            try { MlpEncoder.EncodeAsync(wave, mismatch, context, TimeSpan.FromSeconds(10), CancellationToken.None).GetAwaiter().GetResult(); throw new Exception("Mismatched metadata accepted"); }
            catch (InvalidDataException) { }
            Require(!File.Exists(mismatch), "Bad metadata produced an output file");
        }
        finally { Directory.Delete(root, true); }
    }

    public static void OddPcmTail()
    {
        var root = NewRoot();
        try
        {
            foreach (var bits in new[] { 20, 24 })
                foreach (var channels in new[] { 1, 3, 5 })
                {
                    var source = Path.Combine(root, $"{bits}_{channels}.wav");
                    var target = Path.Combine(root, $"{bits}_{channels}_normalized.wav");
                    var expected = WriteWave(source, 48000, bits, channels, 817);
                    var bytes = File.ReadAllBytes(source)[..^1]; // eac3to's unpadded final data chunk
                    System.Buffers.Binary.BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4), (uint)(bytes.Length - 8));
                    File.WriteAllBytes(source, bytes);
                    SurcodePcmWav.Normalize(source, target, 48000, bits);
                    var layout = SurcodePcmWav.ReadLayout(target);
                    Require(layout.DataSize == expected.Length, "Odd final chunk lost a PCM frame");
                    Require(File.ReadAllBytes(target).AsSpan((int)layout.DataOffset, expected.Length).SequenceEqual(expected),
                        "Normalizing RIFF alignment must preserve every PCM byte");
                    // Removing a PCM byte, even with a revised RIFF size, must remain an error.
                    bytes = bytes[..^1];
                    System.Buffers.Binary.BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4), (uint)(bytes.Length - 8));
                    File.WriteAllBytes(source, bytes);
                    try { SurcodePcmWav.ReadLayout(source); throw new Exception("Truncated PCM was accepted"); }
                    catch (InvalidDataException) { }
                }
        }
        finally { Directory.Delete(root, true); }
    }

    public static byte[] WriteWave(string path, int rate, int bits, int channels, int frames)
    {
        var bytesPerSample = bits == 16 ? 2 : 3; var raw = new byte[frames * channels * 3];
        using var writer = new BinaryWriter(File.Create(path));
        var size = frames * channels * bytesPerSample;
        writer.Write("RIFF"u8); writer.Write((uint)(60 + size + (size & 1))); writer.Write("WAVEfmt "u8);
        writer.Write(40u); writer.Write((ushort)0xfffe); writer.Write((ushort)channels); writer.Write(rate);
        writer.Write(rate * channels * bytesPerSample); writer.Write((ushort)(channels * bytesPerSample)); writer.Write((ushort)(bytesPerSample * 8));
        writer.Write((ushort)22); writer.Write((ushort)bits);
        writer.Write(channels switch { 1 => 4u, 2 => 3u, 3 => 7u, 4 => 0x33u, 5 => 0x37u, 6 => 0x3fu, _ => throw new ArgumentException() });
        writer.Write(new byte[] { 1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113 }); writer.Write("data"u8); writer.Write((uint)size);
        for (var n = 0; n < frames; n++) for (var c = 0; c < channels; c++)
        {
            var sample = (((n * (97 + c * 18) + c * 53) % 100001) - 50000) & ~((1 << (24 - bits)) - 1);
            var at = (n * channels + c) * 3; raw[at] = (byte)sample; raw[at + 1] = (byte)(sample >> 8); raw[at + 2] = (byte)(sample >> 16);
            if (bits == 16) sample >>= 8;
            writer.Write((byte)sample); writer.Write((byte)(sample >> 8)); if (bytesPerSample == 3) writer.Write((byte)(sample >> 16));
        }
        if ((size & 1) != 0) writer.Write((byte)0); return raw;
    }

    public static void OversizedAccessUnit() => CheckOversizedAccessUnit(false);
    public static void OversizedAccessUnitIntegration() => CheckOversizedAccessUnit(true);

    private static void CheckOversizedAccessUnit(bool decode)
    {
        var root = NewRoot();
        try
        {
            const int rate = 88200, channels = 6, frames = rate / 8 + 17;
            var wave = Path.Combine(root, "noise.wav");
            var expected = WriteWave(wave, rate, 24, channels, frames);
            uint state = 0x713291abu;
            for (var n = 0; n < frames; n++) for (var ch = 0; ch < channels; ch++)
            {
                state = unchecked(state * 1664525 + 1013904223);
                var value = (n % 4096) switch
                { 0 => -8388608, 1 => 8388607, 2 => 0, 3 => 1, 4 => -1, _ => (int)(state >> 8) - 8388608 };
                var at = (n * channels + ch) * 3;
                expected[at] = (byte)value; expected[at + 1] = (byte)(value >> 8); expected[at + 2] = (byte)(value >> 16);
            }
            var layout = SurcodePcmWav.ReadLayout(wave);
            using (var stream = new FileStream(wave, FileMode.Open, FileAccess.Write))
            { stream.Position = layout.DataOffset; stream.Write(expected); }
            var output = Path.Combine(root, "noise.mlp");
            MlpEncoder.EncodeAsync(wave, output, "", TimeSpan.FromSeconds(120), CancellationToken.None).GetAwaiter().GetResult();
            Require(MlpCacheValidator.IsEncoderValid(output), "oversized-AU stream structure");
            var bytes = File.ReadAllBytes(output); var offset = 0; var units = 0;
            while (offset < bytes.Length)
            {
                Require(offset + 4 <= bytes.Length, "Truncated AU header");
                var size = (BinaryPrimitives.ReadUInt16BigEndian(bytes.AsSpan(offset)) & 0xfff) * 2;
                Require(size >= 8 && size <= 1536 && offset + size <= bytes.Length, "AU exceeds unchanged 1536-byte limit");
                offset += size; units++;
            }
            Require(units == (frames + 79) / 80, "fallback changed AU count or metadata alignment");
            var repeat = Path.Combine(root, "repeat.mlp");
            var context = Path.Combine(root, "metadata.stampctx");
            var explicitOutput = Path.Combine(root, "explicit.mlp");
            MlpEncoder.WriteMetadata(context, frames, rate);
            Task.WhenAll(
                MlpEncoder.EncodeAsync(wave, repeat, "", TimeSpan.FromSeconds(120), CancellationToken.None),
                MlpEncoder.EncodeAsync(wave, explicitOutput, context, TimeSpan.FromSeconds(120), CancellationToken.None)
            ).GetAwaiter().GetResult();
            Require(File.ReadAllBytes(repeat).AsSpan().SequenceEqual(bytes), "Concurrent oversized-AU fallback is not deterministic");
            Require(File.ReadAllBytes(explicitOutput).AsSpan().SequenceEqual(bytes), "fallback changed explicit metadata alignment");
            if (decode)
            {
                var raw = Path.Combine(root, "decoded.raw");
                var result = new ProcessRunner().RunAsync(new ProcessRequest
                {
                    FileName = "ffmpeg", Arguments = ["-nostdin", "-v", "error", "-xerror", "-f", "mlp", "-i", output,
                        "-c:a", "pcm_s24le", "-f", "s24le", raw], Timeout = TimeSpan.FromSeconds(120)
                }).GetAwaiter().GetResult();
                Require(result.Succeeded, "Oversized-AU independent decode: " + result.StandardError);
                var actual = File.ReadAllBytes(raw);
                Require(actual.Length == units * 80 * channels * 3, "Incorrect padded PCM length");
                Require(actual.AsSpan(0, expected.Length).SequenceEqual(expected), "fallback changed source PCM");
                Require(actual.AsSpan(expected.Length).IndexOfAnyExcept((byte)0) < 0, "Nonzero final AU padding");
                Console.WriteLine("PASS: oversized-AU lossless fallback; artifacts: " + root);
            }
        }
        finally { if (!decode) Directory.Delete(root, true); }
    }

    public static void RealIntegration()
    {
        var root = NewRoot(); Console.WriteLine("Integration artifacts: " + root);
        var runner = new ProcessRunner(); var passed = new List<object>();
        foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
            foreach (var bits in new[] { 16, 20, 24 })
                for (var channels = 1; channels <= (rate > 96000 ? 2 : 6); channels++)
                {
                    var folder = Path.Combine(root, $"{rate}_{bits}_{channels}"); Directory.CreateDirectory(folder);
                    var wave = Path.Combine(folder, "input.wav"); var expected = WriteWave(wave, rate, bits, channels, 817);
                    var output = Path.Combine(root, $"{rate}_{bits}_{channels}.mlp");
                    MlpEncoder.EncodeAsync( wave, output, "", TimeSpan.FromSeconds(120), CancellationToken.None).GetAwaiter().GetResult();
                    Require(MlpCacheValidator.IsEncoderValid(output), $"Inspect rejected {rate}/{bits}/{channels}");
                    var raw = Path.Combine(folder, "decoded.raw");
                    var result = runner.RunAsync(new ProcessRequest
                    {
                        FileName = "ffmpeg",
                        Arguments = ["-nostdin","-v","error","-xerror","-f","mlp","-i",output,
                "-c:a","pcm_s24le","-f","s24le",raw],
                        Timeout = TimeSpan.FromSeconds(120)
                    }).GetAwaiter().GetResult();
                    Require(result.Succeeded, "Independent PCM decode failed: " + result.StandardError);
                    var actual = File.ReadAllBytes(raw); var block = 40 * rate / (rate % 44100 == 0 ? 44100 : 48000);
                    Require(actual.Length == ((817 + block - 1) / block) * block * channels * 3, "Incorrect output frame count");
                    Require(actual.AsSpan(0, expected.Length).SequenceEqual(expected), "Native input PCM changed");
                    Require(actual.AsSpan(expected.Length).IndexOfAnyExcept((byte)0) < 0, "Final AU padding is not zero");
                    passed.Add(new { rate, bits, channels, bytes = new FileInfo(output).Length });
                    Console.WriteLine($"PASS {passed.Count}/84 {rate}/{bits}/{channels}");
                }
        File.WriteAllText(Path.Combine(root, "result.json"), JsonSerializer.Serialize(new { status = "PASS", profiles = passed, encoder = MlpEncoder.BinarySha256 }));
        Console.WriteLine("PASS: embedded Windows core, Unicode directories and independent PCM equality for all 84 default-layout profiles");
    }

    public static void RealBatchIntegration(string eac3to)
    {
        var root = NewRoot(); Console.WriteLine("Real batch artifacts: " + root);
        var runner = new ProcessRunner(); var passed = new List<object>();
        foreach (var profile in new[] { (44100, 16, 2), (48000, 24, 6), (88200, 20, 2), (96000, 24, 6), (176400, 16, 1), (192000, 24, 2), (48000, 20, 2) })
        {
            var (rate, bits, channels) = profile;
            var folder = Path.Combine(root, $"{rate}_{bits}_{channels}"); Directory.CreateDirectory(folder);
            var wav = Path.Combine(folder, "source.wav"); var raw = WriteWave(wav, rate, bits, channels, rate / 4 + 17);
            var flac = Path.Combine(folder, "source.flac");
            var conversion = runner.RunAsync(new ProcessRequest { FileName = "ffmpeg", Arguments = ["-nostdin", "-v", "error", "-i", wav, "-c:a", "flac", flac] }).GetAwaiter().GetResult();
            Require(conversion.Succeeded, "Synthetic FLAC creation: " + conversion.StandardError);
            var job = new SurcodeEncodingJob
            {
                Eac3toExecutable = Path.GetFullPath(eac3to),
                TemporaryDirectory = Path.Combine(folder, "temp"),
                OutputDirectory = Path.Combine(folder, "output"),
                SampleRate = rate,
                Bits = bits,
                Tracks = [new SurcodeEncodingTrack
                {SourcePath=flac,WorkName="track",DisplayName="Synthetic",DurationSeconds=1,SourceSampleRate=rate,SourceBits=bits}]
            };
            new SurcodeBatchEncoder(runner).RunAsync(job, CancellationToken.None).GetAwaiter().GetResult();
            var output = Path.Combine(job.OutputDirectory, "track.mlp"); var decoded = Path.Combine(folder, "decoded.raw");
            var decoding = runner.RunAsync(new ProcessRequest
            {
                FileName = "ffmpeg",
                Arguments = ["-nostdin","-v","error","-xerror","-f","mlp","-i",output,
                "-c:a","pcm_s24le","-f","s24le",decoded]
            }).GetAwaiter().GetResult();
            Require(decoding.Succeeded, "Real batch decode: " + decoding.StandardError);
            var actual = File.ReadAllBytes(decoded);
            Require(actual.Length >= raw.Length && actual.AsSpan(0, raw.Length).SequenceEqual(raw), "eac3to/native pipeline changed PCM");
            Require(actual.AsSpan(raw.Length).IndexOfAnyExcept((byte)0) < 0, "Nonzero batch tail");
            Require(MlpCacheValidator.IsEncoderValid(output), "Batch stream structure failed");
            // Bypass the converter using the exact synthetic PCM: identical final bytes prove no post-encode repair.
            var directFolder = Path.Combine(folder, "direct"); Directory.CreateDirectory(directFolder);
            var directWave = Path.Combine(directFolder, "input.wav"); SurcodePcmWav.Normalize(wav, directWave, rate, bits);
            var direct = Path.Combine(folder, "direct.mlp");
            MlpEncoder.EncodeAsync( directWave, direct, "", TimeSpan.FromSeconds(120), CancellationToken.None).GetAwaiter().GetResult();
            Require(File.ReadAllBytes(output).SequenceEqual(File.ReadAllBytes(direct)), "Batch output differs from direct algorithm");
            passed.Add(new { rate, bits, channels, bytes = new FileInfo(output).Length, pcm_equal = true, direct_bytes_equal = true });
            Console.WriteLine($"PASS real batch {rate}/{bits}/{channels}");
        }
        File.WriteAllText(Path.Combine(root, "result.json"), JsonSerializer.Serialize(new { status = "PASS", profiles = passed, encoder = MlpEncoder.BinarySha256 }));
        Console.WriteLine("PASS: real FLAC -> eac3to -> MLP core; native PCM and direct encoded bytes equal");
    }
}
