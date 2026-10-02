using System.Text.Json;
using System.Runtime.InteropServices;
using DvdaMaker.Processes;

internal static class BuiltinImagesTests
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int NativeCommand([MarshalAs(UnmanagedType.LPUTF8Str)] string command);
    public static async Task Integration(string directory)
    {
        var root = Path.GetFullPath(directory);
        if (Directory.Exists(root)) throw new IOException("Use a fresh image integration directory.");
        Directory.CreateDirectory(root);
        var reference = Environment.GetEnvironmentVariable("DVDA_TEST_MAGICK") ?? throw new FileNotFoundException("Set DVDA_TEST_MAGICK to the reference build.");
        var runner = new ProcessRunner(); var passed = new List<string>();
        void Check(string name, bool ok) { if (!ok) throw new InvalidDataException(name); passed.Add(name); Console.WriteLine("PASS " + name); }
        async Task<ProcessResult> Run(string tool, params string[] arguments)
        {
            var r = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = arguments, Timeout = TimeSpan.FromSeconds(90) });
            if (!r.Succeeded) throw new InvalidDataException(string.Join(' ', arguments) + "\n" + r.StandardError);
            return r;
        }
        var tool = BuiltinImages.Tool("magick");
        var nativeModule = NativeLibrary.Load(BuiltinImages.LibraryPath);
        try
        {
            var command = Marshal.GetDelegateForFunctionPointer<NativeCommand>(NativeLibrary.GetExport(nativeModule, "dvda_image_command"));
            var nativeOutput = Path.Combine(root, "原生 索引.png");
            Check("Native command accepts leading whitespace and Unicode paths",
                command($" \t\"convert\" -size 8x8 xc:black \"{nativeOutput}\"") == 0 && File.Exists(nativeOutput));
            Check("Empty native command fails cleanly", command(" \t") != 0);
        }
        finally { NativeLibrary.Free(nativeModule); }
        var fonts = await Run(tool, "-list", "font");
        Check("Three bundled regional font faces", new[] { "SC", "JP", "KR" }.All(x => fonts.StandardOutput.Contains("DVDA-Noto-Sans-CJK-" + x)));
        var fixture = Path.Combine(root, "中文 日韓 gradient.png");
        await Run(reference, "-size", "720x576", "gradient:#204080-#80b0e0", fixture);
        foreach (var extension in new[] { "jpg", "png", "webp" })
        {
            var source = Path.Combine(root, "源文件." + extension);
            await Run(reference, fixture, source);
            var target = Path.Combine(root, "缩放 结果." + extension + ".png");
            await Run(tool, source, "-resize", "720x576^", "-gravity", "center", "-extent", "720x576", "-brightness-contrast", "-35x0", target);
            var dimensions = await Run(tool, "identify", "-format", "%w|%h", target);
            Check(extension + " decoding, drawing and Unicode output", dimensions.StandardOutput.Trim() == "720|576");
        }
        var transparent = Path.Combine(root, "透明.png");
        await Run(tool, "-size", "160x48", "xc:none", "-depth", "8", "PNG32:" + transparent);
        var alpha = await Run(tool, transparent, "-format", "%[fx:mean.a]", "info:");
        Check("Transparent alpha statistics", alpha.StandardOutput.Trim() == "0");
        foreach (var region in new[] { "SC", "JP", "KR" })
        {
            var target = Path.Combine(root, region + ".png");
            var font = "DVDA-Noto-Sans-CJK-" + region;
            await Run(tool, "-size", "500x100", "xc:none", "-font", font, "-pointsize", "30", "-fill", "white", "-annotate", "+2+40", "中文 日本語 한글 ABC", target);
            var stats = await Run(tool, target, "-format", "%[fx:mean.a]", "info:");
            Check(region + " nonempty antialiased text", double.Parse(stats.StandardOutput.Trim(), System.Globalization.CultureInfo.InvariantCulture) > 0.01);
            var caption = Path.Combine(root, region + "-caption.png");
            await Run(tool, "-size", "200x80", "-font", font, "-pointsize", "18", "-background", "none", "-fill", "white", "caption:专辑标题 日本語 한글", caption);
            Check(region + " index caption", File.Exists(caption));
        }
        var batch = await Run(tool, fixture, "(", "+clone", "-crop", "10x10+0+0", "+repage", "-format", "A%w|%h\\n", "-write", "info:", ")", "-delete", "-1",
            "(", "+clone", "-crop", "20x20+20+20", "+repage", "-format", "B%w|%h\\n", "-write", "info:", ")", "-delete", "-1", "null:");
        Check("Independent INFO writes preserve image stack order", batch.StandardOutput.Replace("\r\n", "\n").Trim() == "A10|10\nB20|20");
        var preserved = Path.Combine(root, "existing.png"); File.WriteAllText(preserved, "preserve");
        var error = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = [Path.Combine(root, "missing.png"), preserved] });
        Check("Invalid input preserves existing output and reports error", !error.Succeeded && File.ReadAllText(preserved) == "preserve" && error.StandardError.Length > 0);
        var corrupt = Path.Combine(root, "truncated.png");
        File.WriteAllBytes(corrupt, File.ReadAllBytes(fixture)[..80]);
        var decodeError = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = [corrupt, preserved] });
        Check("Truncated PNG reports error without terminating host", !decodeError.Succeeded && File.ReadAllText(preserved) == "preserve");
        foreach (var extension in new[] { "jpg", "webp" })
        {
            var damaged = Path.Combine(root, "truncated." + extension);
            File.WriteAllBytes(damaged, File.ReadAllBytes(Path.Combine(root, "源文件." + extension))[..32]);
            var failure = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = [damaged, preserved] });
            Check("Truncated " + extension + " reports error without terminating host", !failure.Succeeded && File.ReadAllText(preserved) == "preserve");
        }
        var video = Path.Combine(root, "disabled-video.png");
        var disabled = await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = ["MP4:" + fixture, video] });
        Check("Video coder/delegate unavailable", !disabled.Succeeded && !File.Exists(video));
        using (var cancellation = new CancellationTokenSource(TimeSpan.FromMilliseconds(100)))
        {
            var canceled = false;
            try { await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = ["-size", "4096x4096", "gradient:", "-blur", "0x100", preserved] }, cancellation.Token); }
            catch (OperationCanceledException) { canceled = true; }
            Check("Cancellation preserves output", canceled && File.ReadAllText(preserved) == "preserve");
        }
        var timedOut = false;
        try { await runner.RunAsync(new ProcessRequest { FileName = tool, Arguments = ["-size", "4096x4096", "gradient:", "-blur", "0x100", preserved], Timeout = TimeSpan.FromMilliseconds(100) }); }
        catch (TimeoutException) { timedOut = true; }
        Check("Timeout is distinct from caller cancellation", timedOut && File.ReadAllText(preserved) == "preserve");
        var parallel = await Task.WhenAll(Enumerable.Range(1, 4).Select(i => Run(tool, "-size", $"{i}x{i}", "xc:black", "-format", "%w", "info:")));
        Check("Concurrent INFO requests are isolated", parallel.Select(x => x.StandardOutput.Trim()).SequenceEqual(new[] { "1", "2", "3", "4" }));
        Check("No partial image files remain", !Directory.EnumerateFiles(root, ".dvda-image-*").Any());
        File.WriteAllText(Path.Combine(root, "report.json"), JsonSerializer.Serialize(new { status = "PASS", checks = passed }, new JsonSerializerOptions { WriteIndented = true }));
    }
}
