using System.Text.Json;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

internal static class BuiltinMediaTests
{
    public static async Task Integration(string directory)
    {
        var root = Path.GetFullPath(directory);
        if (Directory.Exists(root)) throw new IOException("Use a fresh integration directory.");
        Directory.CreateDirectory(root);
        var runner = new ProcessRunner();
        var ffmpeg = ExecutablePath.Resolve("ffmpeg") ?? throw new FileNotFoundException("Reference FFmpeg");
        var ffprobe = ExecutablePath.Resolve("ffprobe") ?? throw new FileNotFoundException("Reference FFprobe");
        var passed = new List<string>();
        void Check(string name, bool valid) { if (!valid) throw new InvalidDataException(name); lock (passed) passed.Add(name); Console.WriteLine("PASS " + name); }
        async Task<ProcessResult> Run(string tool, params string[] args)
        {
            var result = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = args, Timeout = TimeSpan.FromSeconds(120) });
            Check("request " + passed.Count + " " + tool, result.Succeeded);
            return result;
        }
        var wave = Path.Combine(root, "中文 日韓 source.wav");
        FfmpegPcmTests.WriteWave(wave, 96000, 24, 6, noise: false, mask: 0);
        var cover = Path.Combine(root, "封面.png");
        await Run(ffmpeg, "-v", "error", "-f", "lavfi", "-i", "color=c=0x308060:s=64x48", "-frames:v", "1", cover);
        var source = Path.Combine(root, "中文 日韓 album.m4a");
        await Run(ffmpeg, "-v", "error", "-i", wave, "-i", cover, "-map", "0:a:0", "-map", "1:v:0", "-c:a", "alac", "-c:v", "copy",
            "-disposition:v:0", "attached_pic", "-metadata", "title=测试 曲目", "-metadata", "album=日韓 中文", "-metadata", "date=2026-10-02", "-metadata", "track=2/8", source);
        var originalMetadata = await new AudioMetadataReader(runner, ffprobe).ReadAsync(source);
        var nativeMetadata = await new AudioMetadataReader(runner, BuiltinMedia.Probe).ReadAsync(source);
        Check("ALAC metadata and Unicode tags", originalMetadata == nativeMetadata);
        var externalRepair = await new AlacEndRepairer(runner, ffprobe).InspectAsync(source);
        var nativeRepair = await new AlacEndRepairer(runner, BuiltinMedia.Probe).InspectAsync(source);
        Check("ALAC packet inspection", externalRepair.Cookie == nativeRepair.Cookie && externalRepair.Patches.SequenceEqual(nativeRepair.Patches));
        var converter = new M4aFlacConverter(runner, BuiltinMedia.Converter, BuiltinMedia.Probe, new AlacEndRepairer(runner, BuiltinMedia.Probe));
        var converted = (await converter.ConvertAsync([source])).Single();
        Check("ALAC to FLAC with exact cover and tags: " + JsonSerializer.Serialize(converted, new JsonSerializerOptions { IncludeFields = true }),
            converted.Status == "OK" && converted.CoverExact && converted.HasCover && converted.MissingTags?.Count == 0);
        var flac = Path.ChangeExtension(source, ".flac");
        var a = await new AudioMetadataReader(runner, ffprobe).ReadAsync(flac);
        var b = await new AudioMetadataReader(runner, BuiltinMedia.Probe).ReadAsync(flac);
        Check("FLAC metadata", a == b);
        foreach (var input in new[] { source, flac })
        {
            var old = await Run(ffmpeg, "-v", "error", "-i", input, "-map", "0:a:0", "-f", "md5", "-");
            var current = await Run(BuiltinMedia.Converter, "-v", "error", "-i", input, "-map", "0:a:0", "-f", "md5", "-");
            Check("PCM MD5 " + Path.GetExtension(input), old.StandardOutput.Trim() == current.StandardOutput.Trim());
            var originalCheck = await new DecodeValidator(runner, ffmpeg).CheckAsync(input, 48000);
            var nativeCheck = await new DecodeValidator(runner, BuiltinMedia.Converter).CheckAsync(input, 48000);
            Check("SOXR decoded count " + Path.GetExtension(input), JsonSerializer.Serialize(originalCheck) == JsonSerializer.Serialize(nativeCheck));
            var oldPcm = Path.Combine(root, Path.GetExtension(input) + ".reference.raw");
            var newPcm = Path.Combine(root, Path.GetExtension(input) + ".native.raw");
            await Run(ffmpeg, "-v", "error", "-i", input, "-af", "aresample=44100:resampler=soxr", "-f", "s24le", oldPcm);
            await Run(BuiltinMedia.Converter, "-v", "error", "-i", input, "-af", "aresample=44100:resampler=soxr", "-f", "s24le", newPcm);
            // SOXR is used only by the diagnostic sample counter, never by the MLP
            // preparation path (which uses SWR and is tested for byte identity).
            // Different libsoxr builds can round a 24-bit output sample by one LSB.
            var reference = File.ReadAllBytes(oldPcm); var actual = File.ReadAllBytes(newPcm);
            Check("SOXR diagnostic sample count exact " + Path.GetExtension(input), reference.Length == actual.Length);
            var maximumDelta = 0;
            for (var i = 0; i < reference.Length; i += 3)
            {
                static int S24(byte[] data, int offset) => (data[offset] << 8 | data[offset + 1] << 16 | data[offset + 2] << 24) >> 8;
                maximumDelta = Math.Max(maximumDelta, Math.Abs(S24(reference, i) - S24(actual, i)));
            }
            Check($"SOXR diagnostic PCM maximum delta {maximumDelta} LSB " + Path.GetExtension(input), maximumDelta <= 1);
        }
        var video = Path.Combine(root, "菜单.vob");
        await Run(ffmpeg, "-v", "error", "-f", "lavfi", "-i", "testsrc2=s=720x576:r=25:d=0.2", "-c:v", "mpeg2video", "-f", "vob", video);
        var before = Path.Combine(root, "frame-before.png"); var after = Path.Combine(root, "frame-after.png");
        await Run(ffmpeg, "-v", "error", "-i", video, "-frames:v", "1", before);
        await Run(BuiltinMedia.Converter, "-v", "error", "-i", video, "-frames:v", "1", after);
        var beforeHash = await Run(ffmpeg, "-v", "error", "-i", before, "-f", "md5", "-");
        var afterHash = await Run(ffmpeg, "-v", "error", "-i", after, "-f", "md5", "-");
        Check("DVD menu frame pixels", beforeHash.StandardOutput == afterHash.StandardOutput);
        var invalid = Path.Combine(root, "invalid.flac"); File.WriteAllText(invalid, "invalid audio");
        var failedOutput = Path.Combine(root, "must-not-exist.raw");
        var failure = await runner.RunAsync(new ProcessRequest { FileName = BuiltinMedia.Converter, Arguments = ["-i", invalid, "-f", "s24le", failedOutput] });
        Check("Invalid audio reports failure without output", !failure.Succeeded && !File.Exists(failedOutput));
        using (var cancellation = new CancellationTokenSource())
        {
            var canceled = false;
            try { await runner.RunAsync(new ProcessRequest { FileName = BuiltinMedia.Converter,
                Arguments = ["-i", flac, "-f", "s24le", "-progress", "pipe:1", failedOutput],
                OnOutputLine = _ => cancellation.Cancel() }, cancellation.Token); }
            catch (OperationCanceledException) { canceled = true; }
            Check("Cancellation removes partial output", canceled && !File.Exists(failedOutput) && !Directory.EnumerateFiles(root, "*.partial").Any());
        }
        var parallel = Enumerable.Range(0, 4).Select(async i =>
        {
            var destination = Path.Combine(root, $"parallel-{i}.raw");
            await Run(BuiltinMedia.Converter, "-i", flac, "-f", "s24le", destination);
            return File.ReadAllBytes(destination);
        });
        var parallelPcm = await Task.WhenAll(parallel);
        Check("Concurrent in-process decoders", parallelPcm.All(x => x.SequenceEqual(parallelPcm[0])));
        File.WriteAllText(Path.Combine(root, "report.json"), JsonSerializer.Serialize(new { status = "PASS", identity = BuiltinMedia.Identity, checks = passed }, new JsonSerializerOptions { WriteIndented = true }));
    }
}
