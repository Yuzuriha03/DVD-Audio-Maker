using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text.Json;
using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

internal static class FfmpegPcmTests
{
    private static void Require(bool value, string message) { if (!value) throw new Exception(message); }
    private static string Root() { var p = Path.Combine(Path.GetTempPath(), "ffmpeg-中文-" + Guid.NewGuid().ToString("N")); Directory.CreateDirectory(p); return p; }
    private static SurcodeEncodingTrack Track(string path, int rate = 48000, int bits = 24) => new()
    { SourcePath = path, WorkName = "track", DisplayName = "中文音轨", DurationSeconds = 1, SourceSampleRate = rate, SourceBits = bits };
    private static SurcodeEncodingJob Job(string root, string ffmpeg, string source, int rate = 48000, int bits = 24) => new()
    { FfmpegExecutable = ffmpeg, TemporaryDirectory = Path.Combine(root, "temp"), OutputDirectory = Path.Combine(root, "output"),
        SampleRate = rate, Bits = bits, Tracks = [Track(source, rate, bits)] };

    public static void ConversionContract()
    {
        foreach (var bits in new[] { 16, 20, 24 })
        {
            var args = FfmpegPcmConverter.Arguments("C:/中文 空格/input.flac", "C:/中文 空格/output.wav", 96000, bits).ToArray();
            Require(args[Array.IndexOf(args, "-map") + 1] == "0:a:0", "Wrong audio stream selection");
            Require(!args.Contains("-ac") && !args.Contains("-channel_layout"), "Converter must not remix channels");
            Require(args.Contains("-n") && args.Contains("-nostdin") && args.Contains("-xerror"), "Converter must fail safely");
            Require(args[Array.IndexOf(args, "-c:a") + 1] == (bits == 16 ? "pcm_s16le" : "pcm_s24le"), "MLP encoding escaped the native core");
            Require(args[Array.IndexOf(args, "-af") + 1].Contains("dither_method=none"), "Unexpected dither changed exact PCM");
        }
    }

    public static void LegacySettings()
    {
        var root = Root(); var oldPath = Environment.GetEnvironmentVariable("PATH");
        try
        {
            var tools = Path.Combine(root, "menu-bin"); Directory.CreateDirectory(tools);
            var ffmpeg = Path.Combine(tools, "ffmpeg.exe"); File.WriteAllBytes(ffmpeg, [0]);
            var env = Path.Combine(root, "old.env");
            File.WriteAllText(env, "DVDA_MLP_SOURCE=batch-surcode\nDVDA_MLP_EAC3TO_EXE=C:/missing/eac3to.exe\nDVDA_FFMPEG=ffmpeg\n");
            var settings = ProjectSettings.ImportEnv(env);
            settings.Values["DVDA_BUILD_DIR"] = Path.Combine(root, "work"); settings.Values["DVDA_FINAL_DIR"] = Path.Combine(root, "final");
            Environment.SetEnvironmentVariable("PATH", tools);
            Require(ExecutablePath.Resolve("ffmpeg") == ffmpeg, "Bare FFmpeg PATH lookup");
            Require(settings.Validate(false).Count == 0, "Legacy eac3to setting incorrectly required");
            Require(settings.Values["DVDA_MLP_EAC3TO_EXE"].Contains("missing"), "Legacy value lost on import");
            settings.ApplyBundledToolDefaults(root);
            Require(settings.ToOptions().Ffmpeg == BuiltinMedia.Converter && settings.ToOptions().Ffprobe == BuiltinMedia.Probe, "GUI must use in-process media");
            Require(ExecutablePath.Resolve(Path.Combine(root, "missing.exe")) is null, "Missing explicit path resolved");
            Require(ExecutablePath.Resolve("\"" + ffmpeg + "\"") == ffmpeg, "Quoted executable path failed");
            settings.Values["DVDA_FFMPEG"] = Path.Combine(root, "missing.exe");
            Require(settings.Validate(false).Count == 0, "Legacy external path must not affect GUI media processing");
        }
        finally { Environment.SetEnvironmentVariable("PATH", oldPath); Directory.Delete(root, true); }
    }

    public static void FailureAndCancellation()
    {
        var root = Root(); var old = Environment.GetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE");
        try
        {
            var source = Path.Combine(root, "source.flac"); File.WriteAllBytes(source, [1, 2, 3]);
            Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", source);
            var fail = MlpEncoderTests.FixtureExecutable(root, "fake-ffmpeg-pcm-fail.exe");
            var slow = MlpEncoderTests.FixtureExecutable(root, "fake-ffmpeg-pcm-slow.exe");
            var job = Job(root, fail, source); var encoder = new SurcodeBatchEncoder(new ProcessRunner());
            try { encoder.RunAsync(job, CancellationToken.None).GetAwaiter().GetResult(); throw new Exception("Converter failure ignored"); }
            catch (InvalidOperationException e) { Require(e.Message.Contains("fixture conversion failed"), "Converter diagnostic lost"); }
            Require(!Directory.EnumerateFiles(job.OutputDirectory).Any() && !Directory.EnumerateDirectories(job.TemporaryDirectory).Any(), "Failed conversion leaked output");
            using var cancel = new CancellationTokenSource();
            var pending = encoder.RunAsync(job with { FfmpegExecutable = slow }, cancel.Token);
            for (var n = 0; n < 500 && !pending.IsCompleted && !Directory.EnumerateFiles(job.TemporaryDirectory, "decoded.wav", SearchOption.AllDirectories).Any(); n++) Thread.Sleep(10);
            Require(!pending.IsCompleted, "Cancellation fixture did not enter converter"); cancel.Cancel();
            try { pending.GetAwaiter().GetResult(); throw new Exception("Converter cancellation ignored"); } catch (OperationCanceledException) { }
            Require(!Directory.EnumerateFiles(job.OutputDirectory).Any() && !Directory.EnumerateDirectories(job.TemporaryDirectory).Any(), "Canceled conversion leaked files");
            var existing = Path.Combine(job.OutputDirectory, "track.mlp"); File.WriteAllBytes(existing, [4, 5, 6]);
            try { encoder.RunAsync(job, CancellationToken.None).GetAwaiter().GetResult(); throw new Exception("Existing MLP accepted"); } catch (IOException) { }
            Require(File.ReadAllBytes(existing).AsSpan().SequenceEqual(new byte[] { 4, 5, 6 }), "Existing MLP damaged");
        }
        finally { Environment.SetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE", old); Directory.Delete(root, true); }
    }

    public static void Integration(string directory, bool builtin = false)
    {
        var root = Path.GetFullPath(directory);
        if (Directory.Exists(root) && Directory.EnumerateFileSystemEntries(root).Any()) throw new IOException("Integration output must be empty.");
        Directory.CreateDirectory(root);
        var ffmpeg = ExecutablePath.Resolve("ffmpeg") ?? throw new FileNotFoundException("FFmpeg not in PATH");
        var runner = new ProcessRunner(); var passed = new List<object>(); var failures = new List<object>();
        foreach (var noise in new[] { false, true })
        {
            foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
                foreach (var bits in new[] { 16, 20, 24 })
                    for (var channels = 1; channels <= (rate > 96000 ? 2 : 6); channels++)
                        Check($"native_{rate}_{bits}_{channels}", rate, bits, rate, bits, channels, noise);
            foreach (var bits in new[] { 16, 20, 24 }) foreach (var channels in new[] { 1, 2, 6 })
                Check($"resample96to48_{bits}_{channels}", 96000, 24, 48000, bits, channels, noise);
            foreach (var bits in new[] { 16, 20, 24 }) Check($"resample441to48_{bits}", 44100, 24, 48000, bits, 2, noise);
            foreach (var bits in new[] { 16, 20 }) Check($"quantize24to{bits}", 48000, 24, 48000, bits, 6, noise);
            Check("upconvert16to24", 48000, 16, 96000, 24, 2, noise);
        }
        foreach (var (channels, mask) in new[] { (4, 0x603u), (5, 0x607u), (6, 0x60fu) })
            Check($"side_{channels}", 48000, 24, 48000, 24, channels, false, mask, true);
        Check("alac_6ch", 48000, 24, 48000, 24, 6, false, 0, false, true);
        var result = new { status = failures.Count == 0 ? "PASS" : "FAIL", passed_count = passed.Count, failed_count = failures.Count,
            encoder = MlpEncoder.BinarySha256, converter = builtin ? BuiltinMedia.Identity : ffmpeg, reference_converter_sha256 = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(ffmpeg))),
            policy = FfmpegPcmConverter.Policy, passed, failures };
        File.WriteAllText(Path.Combine(root, "result.json"), JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }));
        Require(failures.Count == 0, $"FFmpeg PCM integration: {failures.Count} failures; {root}");
        Console.WriteLine($"PASS: {passed.Count} FFmpeg batch cases; {root}");

        void Run(params string[] args)
        {
            var result = runner.RunAsync(new ProcessRequest { FileName = ffmpeg, Arguments = args, Timeout = TimeSpan.FromSeconds(120) }).GetAwaiter().GetResult();
            Require(result.Succeeded, result.StandardError);
        }
        void Check(string name, int sourceRate, int sourceBits, int rate, int bits, int channels, bool noise, uint mask = 0, bool waveInput = false, bool alac = false)
        {
            name = (noise ? "noise_" : "smooth_") + name;
            try
            {
                var folder = Path.Combine(root, name); Directory.CreateDirectory(folder);
                var source = Path.Combine(folder, "source.wav"); var raw = WriteWave(source, sourceRate, sourceBits, channels, noise, mask);
                var input = source;
                if (!waveInput)
                {
                    input = Path.Combine(folder, alac ? "音源.m4a" : "音源.flac");
                    Run("-nostdin", "-v", "error", "-xerror", "-n", "-i", source, "-c:a", alac ? "alac" : "flac", input);
                }
                var job = Job(folder, builtin ? BuiltinMedia.Converter : ffmpeg, input, rate, bits) with { Tracks = [Track(input, sourceRate, sourceBits)] };
                new SurcodeBatchEncoder(runner).RunAsync(job, CancellationToken.None).GetAwaiter().GetResult();
                var output = Path.Combine(job.OutputDirectory, "track.mlp");
                var converted = Path.Combine(folder, "reference.wav");
                FfmpegPcmConverter.ConvertAsync(runner, ffmpeg, job.Tracks[0], converted, rate, bits, CancellationToken.None).GetAwaiter().GetResult();
                var prepared = Path.Combine(folder, "prepared.wav"); SurcodePcmWav.Normalize(converted, prepared, rate, bits);
                var layout = SurcodePcmWav.ReadLayout(prepared); var expected = Read24(prepared);
                Require(layout.Channels == channels && layout.SampleRate == rate && layout.ValidBits == bits, "Wrong prepared format");
                if (rate == sourceRate && bits >= sourceBits) Require(raw.AsSpan().SequenceEqual(expected), "Native source PCM changed");
                if (rate == sourceRate && bits == 20 && sourceBits == 24)
                    for (var i = 0; i < raw.Length; i += 3)
                    {
                        var value = (raw[i] | raw[i + 1] << 8 | raw[i + 2] << 16) << 8 >> 8;
                        var rounded = (int)Math.Clamp(Math.Floor((value + 8) / 16.0), -524288, 524287) * 16;
                        Require(expected[i] == (byte)rounded && expected[i + 1] == (byte)(rounded >> 8) && expected[i + 2] == (byte)(rounded >> 16), "20-bit midpoint or clipping error");
                    }
                var decoded = Path.Combine(folder, "decoded.raw");
                Run("-nostdin", "-v", "error", "-xerror", "-n", "-f", "mlp", "-i", output, "-c:a", "pcm_s24le", "-f", "s24le", decoded);
                var actual = File.ReadAllBytes(decoded); var frames = expected.Length / (channels * 3);
                var block = 40 * rate / (rate % 44100 == 0 ? 44100 : 48000);
                Require(actual.Length == (frames + block - 1) / block * block * channels * 3, "Wrong decoded frame count");
                Require(actual.AsSpan(0, expected.Length).SequenceEqual(expected), "Decoded MLP differs from target PCM");
                Require(actual.AsSpan(expected.Length).IndexOfAnyExcept((byte)0) < 0, "Nonzero AU padding");
                Require(MlpCacheValidator.IsMlpEncoderValid(output), "MLP structure invalid");
                var directWave = prepared;
                if (rate == sourceRate && bits >= sourceBits)
                { directWave = Path.Combine(folder, "direct.wav"); SurcodePcmWav.Normalize(source, directWave, rate, bits); }
                var direct = Path.Combine(folder, "direct.mlp");
                MlpEncoder.EncodeAsync(directWave, direct, "", TimeSpan.FromSeconds(120), CancellationToken.None).GetAwaiter().GetResult();
                Require(File.ReadAllBytes(output).AsSpan().SequenceEqual(File.ReadAllBytes(direct)), "Whole MLP differs from direct encoding");
                passed.Add(new { name, sourceRate, sourceBits, rate, bits, channels, mask = layout.ChannelMask, frames, source_pcm_equal = rate == sourceRate && bits >= sourceBits,
                    decoded_pcm_equal = true, direct_mlp_equal = true, mlp_sha256 = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(output))) });
                Console.WriteLine($"PASS {passed.Count}: {name}");
            }
            catch (Exception e) { failures.Add(new { name, error = e.ToString() }); Console.WriteLine($"FAIL {name}: {e.Message}"); }
        }
    }

    public static void OriginalCorpus(string matrixPath, string outputDirectory)
    {
        var root = Path.GetFullPath(outputDirectory);
        if (Directory.Exists(root) && Directory.EnumerateFileSystemEntries(root).Any()) throw new IOException("Corpus output must be empty.");
        Directory.CreateDirectory(root);
        using var matrix = JsonDocument.Parse(File.ReadAllText(matrixPath));
        var runner = new ProcessRunner(); var ffmpeg = ExecutablePath.Resolve("ffmpeg") ?? throw new FileNotFoundException("FFmpeg");
        var passed = new List<object>(); var failures = new List<object>(); long bytesCompared = 0;
        foreach (var item in matrix.RootElement.GetProperty("results").EnumerateArray())
        {
            var profile = item.GetProperty("profile").EnumerateArray().Select(x => x.GetInt32()).ToArray(); var name = string.Join('_', profile);
            try
            {
                Require(item.GetProperty("status").GetString() == "PASS", "Unvalidated reference");
                var pcm = item.GetProperty("pcm").GetString()!; var folder = Path.GetDirectoryName(pcm)!;
                var original = File.ReadAllBytes(item.GetProperty("reference").GetString()!);
                Require(Convert.ToHexString(SHA256.HashData(original)).Equals(item.GetProperty("reference_sha256").GetString(), StringComparison.OrdinalIgnoreCase), "Reference hash changed");
                var output = Path.Combine(root, name); Directory.CreateDirectory(output);
                var flac = Path.Combine(output, "原版输入.flac");
                var conversion = runner.RunAsync(new ProcessRequest { FileName = ffmpeg,
                    Arguments = ["-nostdin", "-v", "error", "-xerror", "-n", "-i", Path.Combine(folder, "source.wav"), "-c:a", "flac", flac] }).GetAwaiter().GetResult();
                Require(conversion.Succeeded, conversion.StandardError);
                var job = Job(output, ffmpeg, flac, profile[0], profile[1]) with { MetadataContext = Path.Combine(folder, "original.stampctx") };
                new SurcodeBatchEncoder(runner).RunAsync(job, CancellationToken.None).GetAwaiter().GetResult();
                var mlp = Path.Combine(job.OutputDirectory, "track.mlp"); var actual = File.ReadAllBytes(mlp);
                Require(actual.AsSpan().SequenceEqual(original), "FFmpeg batch output is not byte-identical to original SurCode");
                var decoded = Path.Combine(output, "decoded.raw");
                var decode = runner.RunAsync(new ProcessRequest { FileName = ffmpeg,
                    Arguments = ["-nostdin", "-v", "error", "-xerror", "-n", "-f", "mlp", "-i", mlp, "-c:a", "pcm_s24le", "-f", "s24le", decoded] }).GetAwaiter().GetResult();
                Require(decode.Succeeded, decode.StandardError);
                var expectedPcm = File.ReadAllBytes(pcm); var decodedPcm = File.ReadAllBytes(decoded);
                Require(decodedPcm.Length >= expectedPcm.Length && decodedPcm.AsSpan(0, expectedPcm.Length).SequenceEqual(expectedPcm) && decodedPcm.AsSpan(expectedPcm.Length).IndexOfAnyExcept((byte)0) < 0, "Original PCM mismatch");
                bytesCompared += actual.Length;
                passed.Add(new { name, bytes = actual.Length, original_bytes_equal = true, pcm_equal = true, sha256 = Convert.ToHexString(SHA256.HashData(actual)) });
                Console.WriteLine($"PASS original {passed.Count}: {name}");
            }
            catch (Exception e) { failures.Add(new { name, error = e.ToString() }); Console.WriteLine($"FAIL original {name}: {e.Message}"); }
        }
        File.WriteAllText(Path.Combine(root, "result.json"), JsonSerializer.Serialize(new { status = failures.Count == 0 ? "PASS" : "FAIL",
            passed_count = passed.Count, failed_count = failures.Count, bytes_compared = bytesCompared, encoder = MlpEncoder.BinarySha256,
            policy = FfmpegPcmConverter.Policy, passed, failures }, new JsonSerializerOptions { WriteIndented = true }));
        Require(failures.Count == 0 && passed.Count > 0, "Original corpus comparison failed: " + root);
        Console.WriteLine($"PASS {passed.Count} original whole-file comparisons, {bytesCompared} bytes");
    }

    private static byte[] Read24(string path)
    {
        var l = SurcodePcmWav.ReadLayout(path); using var input = File.OpenRead(path); input.Position = l.DataOffset;
        var bytes = new byte[checked((int)l.DataSize)]; input.ReadExactly(bytes);
        if (l.BytesPerSample == 3) return bytes;
        Require(l.BytesPerSample == 2, "Unexpected PCM storage width");
        var result = new byte[bytes.Length / 2 * 3];
        for (var i = 0; i < bytes.Length / 2; i++) { result[i * 3 + 1] = bytes[i * 2]; result[i * 3 + 2] = bytes[i * 2 + 1]; }
        return result;
    }
    internal static byte[] WriteWave(string path, int rate, int bits, int channels, bool noise, uint mask)
    {
        var frames = rate / 8 + 17; var width = bits == 16 ? 2 : 3; var size = frames * channels * width; var raw = new byte[frames * channels * 3];
        using var writer = new BinaryWriter(File.Create(path));
        writer.Write("RIFF"u8); writer.Write((uint)(60 + size + (size & 1))); writer.Write("WAVEfmt "u8); writer.Write(40u); writer.Write((ushort)0xfffe);
        writer.Write((ushort)channels); writer.Write(rate); writer.Write(rate * channels * width); writer.Write((ushort)(channels * width));
        writer.Write((ushort)(width * 8)); writer.Write((ushort)22); writer.Write((ushort)bits);
        writer.Write(mask != 0 ? mask : channels switch { 1 => 4u, 2 => 3u, 3 => 7u, 4 => 0x33u, 5 => 0x37u, 6 => 0x3fu, _ => throw new Exception() });
        writer.Write(new byte[] { 1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113 }); writer.Write("data"u8); writer.Write((uint)size);
        uint state = 0x713291abu;
        for (var n = 0; n < frames; n++) for (var ch = 0; ch < channels; ch++)
        {
            state = unchecked(state * 1664525 + 1013904223);
            var value = noise ? (n % 4096) switch { 0 => -8388608, 1 => 8388607, 2 => 0, 3 => 1, 4 => -1, _ => (int)(state >> 8) - 8388608 }
                : ((n * (97 + ch * 18) + ch * 53) % 100001) - 50000;
            value &= ~((1 << (24 - bits)) - 1);
            var at = (n * channels + ch) * 3; raw[at] = (byte)value; raw[at + 1] = (byte)(value >> 8); raw[at + 2] = (byte)(value >> 16);
            if (bits == 16) value >>= 8;
            writer.Write((byte)value); writer.Write((byte)(value >> 8)); if (width == 3) writer.Write((byte)(value >> 16));
        }
        if ((size & 1) != 0) writer.Write((byte)0);
        return raw;
    }
}
