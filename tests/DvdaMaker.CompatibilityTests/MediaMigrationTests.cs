using System.Text.Json;
using DvdaMaker.Processes;

internal static class MediaMigrationTests
{
    private static void Require(bool value, string message)
    { if (!value) throw new InvalidDataException(message); }

    public static void Run()
    {
        var mode = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-media-迁移-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var wave = Path.Combine(root, "input.wav");
            FfmpegPcmTests.WriteWave(wave, 96000, 24, 6, true, 0);
            var index = 0;
            foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
            foreach (var bits in new[] { 16, 20, 24 })
            {
                // Fixed input, independent invocation paths, complete output bytes.
                var codec = bits == 16 ? "pcm_s16le" : "pcm_s24le";
                var filter = $"aresample={rate}:resampler=swr:osf=" + (bits == 20 ? "dblp" : bits == 16 ? "s16" : "s32") + ":dither_method=none";
                if (bits == 20) filter += ",aeval=exprs='clip(floor(val(ch)*524288+0.5),-524288,524287)/524288':c=same,aformat=sample_fmts=s32";
                CompareOutput(["-i", wave, "-af", filter, "-c:a", codec, "-f", "wav"], ".wav");
            }
            foreach (var format in new[] { "s16le", "s24le", "s32le" }) CompareOutput(["-i", wave, "-f", format], ".raw");
            var flac = CompareOutput(["-i", wave, "-c:a", "flac", "-compression_level", "8", "-metadata", "title=雨 日本語 🎵",
                "-metadata", "CUSTOM=x=y", "-metadata", "title=最後"], ".flac");
            foreach (var input in new[] { wave, flac })
            {
                foreach (var args in new string[][]
                {
                    ["-of", "json", input],
                    ["-select_streams", "a:0", "-show_entries", "stream=codec_name,sample_rate,channels,bits_per_raw_sample", "-of", "default=nw=1:nk=1", input],
                    ["-show_entries", "format_tags", "-show_entries", "format=duration", input],
                    ["-show_entries", "packet=pts_time,duration_time,pos,size", input],
                }) CompareRequest(BuiltinMedia.Probe, args);
                CompareRequest(BuiltinMedia.Converter, ["-i", input, "-f", "md5", "-"]);
                CompareRequest(BuiltinMedia.Converter, ["-i", input, "-af", "aresample=48000:resampler=soxr,astats=metadata=1", "-f", "null", "-"]);
            }
            foreach (var scenario in new[] { "existing", "replace", "same", "invalid", "locked", "directory", "parent-file", "cancel", "pre-cancel", "callback", "timeout", "capture" })
            {
                var expected = FailureScenario("managed", scenario);
                var actual = FailureScenario("rust", scenario);
                Require(expected == actual, $"Media scenario {scenario}: {expected} / {actual}");
            }
            Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "rust");
            var concurrent = Enumerable.Range(0, 4).Select(async i =>
            {
                var path = Path.Combine(root, "parallel-" + i + ".raw");
                var result = await new ProcessRunner().RunAsync(new ProcessRequest { FileName = BuiltinMedia.Converter,
                    Arguments = ["-i", wave, "-f", "s24le", path] });
                Require(result.Succeeded, "Concurrent native media failed");
                return File.ReadAllBytes(path);
            });
            var bytes = Task.WhenAll(concurrent).GetAwaiter().GetResult();
            Require(bytes.All(b => b.AsSpan().SequenceEqual(bytes[0])), "Concurrent requests contaminated PCM");
            Require(!Directory.EnumerateFiles(root, "*.partial", SearchOption.AllDirectories).Any(), "Leaked media temporary file");

            string CompareOutput(string[] args, string extension)
            {
                var paths = new[] { Path.Combine(root, index + "-managed" + extension), Path.Combine(root, index++ + "-rust" + extension) };
                var expected = Request("managed", BuiltinMedia.Converter, [.. args, paths[0]]);
                var actual = Request("rust", BuiltinMedia.Converter, [.. args, paths[1]]);
                Require(expected.Succeeded && actual.Succeeded, "Media conversion failed: " + actual.StandardError);
                Require(File.ReadAllBytes(paths[0]).AsSpan().SequenceEqual(File.ReadAllBytes(paths[1])), "Whole media output differs: " + string.Join(' ', args));
                Require(expected.StandardOutput == actual.StandardOutput && expected.StandardError == actual.StandardError, "Media conversion diagnostics differ");
                return paths[1];
            }
            void CompareRequest(string tool, string[] args)
            {
                var expected = Request("managed", tool, args); var actual = Request("rust", tool, args);
                Require(expected.ExitCode == actual.ExitCode && expected.StandardOutput == actual.StandardOutput && expected.StandardError == actual.StandardError,
                    "Media result differs: " + string.Join(' ', args));
            }
            string FailureScenario(string backend, string scenario)
            {
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", backend);
                var folder = Path.Combine(root, scenario, backend); Directory.CreateDirectory(folder);
                var input = Path.Combine(folder, "input.wav"); File.Copy(wave, input);
                var output = Path.Combine(folder, "output.raw");
                if (scenario == "same") output = input;
                else if (scenario == "directory") Directory.CreateDirectory(output);
                else if (scenario == "parent-file") { File.WriteAllText(output, "parent"); output = Path.Combine(output, "child.raw"); }
                else File.WriteAllText(output, "preserve");
                if (scenario == "invalid") File.WriteAllText(input, "invalid audio");
                using var token = new CancellationTokenSource();
                if (scenario == "pre-cancel") token.Cancel();
                using var locked = scenario == "locked" ? new FileStream(output, FileMode.Open, FileAccess.Read, FileShare.None) : null;
                var events = new List<string>(); string? error = null; ProcessResult? result = null;
                try
                {
                    result = new ProcessRunner().RunAsync(new ProcessRequest
                    {
                        FileName = BuiltinMedia.Converter,
                        Arguments = [.. (scenario == "existing" ? Array.Empty<string>() : new[] { "-y" }), "-i", input, "-f", "s24le", "-progress", "pipe:1", output],
                        CaptureOutput = scenario != "capture", CaptureError = scenario != "capture",
                        Timeout = scenario == "timeout" ? TimeSpan.FromMilliseconds(20) : null,
                        OnOutputLine = text =>
                        {
                            events.Add(text);
                            if (scenario == "cancel") token.Cancel();
                            if (scenario == "callback") throw new ApplicationException("callback marker");
                            if (scenario == "timeout") Thread.Sleep(50);
                        },
                    }, token.Token).GetAwaiter().GetResult();
                }
                catch (Exception e) { error = e is OperationCanceledException ? "Cancelled" : e.GetType().Name; }
                locked?.Dispose();
                if (scenario == "cancel" || scenario == "pre-cancel") Require(error == "Cancelled", "Cancellation disappeared");
                if (scenario == "timeout") Require(error == nameof(TimeoutException), "Timeout disappeared");
                if (scenario == "callback") Require(error == nameof(ApplicationException), "Callback error lost");
                if (scenario is "cancel" or "pre-cancel" or "callback" or "timeout" or "invalid" or "existing" or "locked")
                    Require(File.ReadAllText(output) == "preserve", "Media failure damaged prior output");
                if (scenario == "capture") Require(result is { StandardOutput.Length: 0, StandardError.Length: 0 } && events.Count > 0, "Capture flags suppressed callbacks");
                Require(!Directory.EnumerateFiles(folder, "*.partial").Any(), "Failure left partial output");
                var contents = Directory.EnumerateFiles(folder, "*", SearchOption.AllDirectories).Order(StringComparer.Ordinal)
                    .Select(p => Path.GetRelativePath(folder, p) + ":" + Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(File.ReadAllBytes(p))));
                return JsonSerializer.Serialize(new { error, ExitCode = result?.ExitCode, files = contents.ToArray() });
            }
        }
        finally { Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode); Directory.Delete(root, true); }
    }
    private static ProcessResult Request(string mode, string tool, string[] arguments)
    {
        Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
        return new ProcessRunner().RunAsync(new ProcessRequest { FileName = tool, Arguments = arguments }).GetAwaiter().GetResult();
    }
}
