using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using DvdaMaker.Processes;

internal static class ImageMigrationTests
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int ReadRgba([MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        [Out] byte[] pixels, nuint capacity, out uint width, out uint height);
    private static void Require(bool value, string message)
    { if (!value) throw new InvalidDataException(message); }
    public static void Run()
    {
        var oldMode = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-images-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var module = NativeLibrary.Load(BuiltinImages.LibraryPath);
        var read = Marshal.GetDelegateForFunctionPointer<ReadRgba>(NativeLibrary.GetExport(module, "dvda_image_read_rgba"));
        try
        {
            var index = 0;
            var source = CompareImage(["-size", "160x120", "gradient:#204080-#80b0e0"]);
            CompareImage([source, "-resize", "80x80^", "-gravity", "center", "-extent", "80x80", "-brightness-contrast", "-35x0"]);
            CompareImage(["-size", "64x32", "xc:none", "-depth", "8"], "PNG32:");
            foreach (var region in new[] { "SC", "JP", "KR" })
                CompareImage(["-size", "400x80", "xc:none", "-font", "DVDA-Noto-Sans-CJK-" + region,
                    "-pointsize", "24", "-fill", "white", "-annotate", "+2+40", "中文 日本語 한글 ABC"]);
            foreach (var args in new string[][]
            {
                ["identify", "-format", "%w|%h", source],
                [source, "-format", "%[fx:mean.r]|%[fx:mean.a]|%k", "info:"],
                ["-list", "font"],
                [source, "(", "+clone", "-crop", "10x10+0+0", "+repage", "-format", "A%w|%h\\n", "-write", "info:", ")", "-delete", "-1",
                    "(", "+clone", "-crop", "20x20+20+20", "+repage", "-format", "B%w|%h\\n", "-write", "info:", ")", "-delete", "-1", "null:"],
            })
            {
                var expected = Request("managed", args); var actual = Request("rust", args);
                Require(expected.ExitCode == actual.ExitCode && expected.StandardOutput == actual.StandardOutput
                    && expected.StandardError == actual.StandardError, "Image text/statistics mismatch");
            }
            foreach (var scenario in new[] { "invalid", "truncated", "locked", "directory", "parent-file", "pre-cancel", "cancel", "timeout", "capture", "callback" })
            {
                var expected = Scenario("managed", scenario); var actual = Scenario("rust", scenario);
                Require(expected == actual, $"Image {scenario}: {expected} / {actual}");
            }
            Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "rust");
            var parallel = Task.WhenAll(Enumerable.Range(1, 4).Select(i => new ProcessRunner().RunAsync(new ProcessRequest
            { FileName = BuiltinImages.Tool("magick"), Arguments = ["-size", $"{i}x{i}", "xc:black", "-format", "%w", "info:"] }))).GetAwaiter().GetResult();
            Require(parallel.Select(x => x.StandardOutput.Trim()).SequenceEqual(new[] { "1", "2", "3", "4" }), "Image concurrent state leaked");

            string Pixels(string path)
            {
                var data = new byte[720 * 576 * 4];
                Require(read(path, data, (nuint)data.Length, out var width, out var height) == 0, "Could not read image pixels");
                return $"{width}x{height}:" + Convert.ToHexString(SHA256.HashData(data.AsSpan(0, checked((int)(width * height * 4)))));
            }
            string CompareImage(string[] args, string prefix = "")
            {
                var before = Path.Combine(root, index + "-managed.png"); var after = Path.Combine(root, index++ + "-rust.png");
                var expected = Request("managed", [.. args, prefix + before]); var actual = Request("rust", [.. args, prefix + after]);
                Require(expected.Succeeded && actual.Succeeded, "Image conversion failed: " + actual.StandardError);
                Require(Pixels(before) == Pixels(after), "Decoded image pixels differ");
                Require(expected.StandardOutput == actual.StandardOutput && expected.StandardError == actual.StandardError, "Image diagnostics differ");
                return after;
            }
            string Scenario(string mode, string scenario)
            {
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
                var folder = Path.Combine(root, scenario, mode); Directory.CreateDirectory(folder);
                var output = Path.Combine(folder, "existing.png");
                if (scenario == "directory") Directory.CreateDirectory(output);
                else File.WriteAllText(output, "preserve");
                if (scenario == "parent-file") output = Path.Combine(output, "child.png");
                var bad = Path.Combine(folder, "invalid.png");
                if (scenario == "truncated") File.WriteAllBytes(bad, File.ReadAllBytes(source)[..32]);
                var arguments = scenario is "invalid" or "truncated" ? new[] { bad, output }
                    : scenario is "timeout" or "cancel" ? new[] { "-size", "4096x4096", "gradient:", "-blur", "0x100", output }
                    : new[] { "-size", "16x16", "xc:red", output };
                using var cancellation = new CancellationTokenSource();
                if (scenario == "pre-cancel") cancellation.Cancel();
                if (scenario == "cancel") cancellation.CancelAfter(50);
                using var locked = scenario == "locked" ? new FileStream(output, FileMode.Open, FileAccess.Read, FileShare.None) : null;
                var events = new List<string>(); string? error = null; ProcessResult? result = null;
                if (scenario is "capture" or "callback") arguments = ["-size", "16x16", "xc:red", "-format", "%w", "info:"];
                try
                {
                    result = new ProcessRunner().RunAsync(new ProcessRequest
                    {
                        FileName = BuiltinImages.Tool("magick"), Arguments = arguments,
                        CaptureOutput = scenario != "capture", CaptureError = scenario != "capture",
                        Timeout = scenario == "timeout" ? TimeSpan.FromMilliseconds(50) : null,
                        OnOutputLine = text => { events.Add(text); if (scenario == "callback") throw new ApplicationException("callback marker"); },
                    }, cancellation.Token).GetAwaiter().GetResult();
                }
                catch (Exception e) { error = e is OperationCanceledException ? "Cancelled" : e.GetType().Name; }
                locked?.Dispose();
                if (scenario is "cancel" or "pre-cancel") Require(error == "Cancelled", "Image cancellation lost");
                if (scenario == "timeout") Require(error == nameof(TimeoutException), "Image timeout lost");
                if (scenario == "callback") Require(error == nameof(ApplicationException), "Image callback failure lost");
                if (scenario == "capture") Require(result is { StandardOutput.Length: 0 } && events.SequenceEqual(new[] { "16" }), "Image capture changed callbacks");
                if (scenario is not ("parent-file" or "directory")) Require(File.ReadAllText(output) == "preserve", "Image failure damaged old output");
                var entries = Directory.EnumerateFiles(folder, "*", SearchOption.AllDirectories).Order(StringComparer.Ordinal)
                    .Select(p => Path.GetRelativePath(folder, p) + ":" + Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(p))));
                return JsonSerializer.Serialize(new { error, ExitCode = result?.ExitCode, files = entries.ToArray() });
            }
        }
        finally { NativeLibrary.Free(module); Environment.SetEnvironmentVariable("DVDA_RUST_MODE", oldMode); Directory.Delete(root, true); }
    }
    private static ProcessResult Request(string mode, string[] args)
    {
        Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
        return new ProcessRunner().RunAsync(new ProcessRequest { FileName = BuiltinImages.Tool("magick"), Arguments = args }).GetAwaiter().GetResult();
    }
}
