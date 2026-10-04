using System.Diagnostics;
using System.Text;
using System.Text.Json;
using DvdaMaker.Processes;

internal static class ProcessMigrationTests
{
    private static void Require(bool condition, string message)
    { if (!condition) throw new InvalidDataException(message); }
    public static int? Fixture(string[] args)
    {
        if (args.Length == 0 || args[0] != "--process-migration-fixture") return null;
        var kind = args[1];
        if (kind == "echo")
        {
            Console.WriteLine(JsonSerializer.Serialize(args.Skip(2)));
            Console.Error.WriteLine(Environment.CurrentDirectory);
            Console.Error.WriteLine(Environment.GetEnvironmentVariable("DVDA_PROCESS_VALUE") ?? "missing");
            Console.Error.WriteLine(Environment.GetEnvironmentVariable("DVDA_PROCESS_REMOVE") ?? "missing");
            return 0;
        }
        if (kind == "lines")
        {
            var encoding = int.Parse(args[2]) switch
            {
                1200 => new UnicodeEncoding(false, false), 1201 => new UnicodeEncoding(true, false),
                12000 => new UTF32Encoding(false, false), 12001 => new UTF32Encoding(true, false),
                _ => (Encoding)new UTF8Encoding(false),
            };
            using var output = Console.OpenStandardOutput(); using var error = Console.OpenStandardError();
            foreach (var b in encoding.GetBytes("中文-日本語-🎵\r\n\nalpha\rbravo\nlast\0tail"))
            { output.WriteByte(b); output.Flush(); }
            error.Write(encoding.GetBytes("error\r\nlast"));
            return 0;
        }
        if (kind == "burst")
        {
            for (var i = 0; i < 2048; i++) { Console.WriteLine($"out-{i:D5}" + new string('x', 80)); Console.Error.WriteLine($"err-{i:D5}" + new string('y', 80)); }
            return 7;
        }
        if (kind is "sleep" or "tree")
        {
            File.WriteAllText(args[2], Environment.ProcessId.ToString());
            if (kind == "tree")
            {
                var start = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = false, CreateNoWindow = true };
                foreach (var argument in new[] { args[0], "sleep", args[3] }) start.ArgumentList.Add(argument);
                using var child = Process.Start(start)!;
                var ready = Stopwatch.StartNew();
                while (!File.Exists(args[3]) && ready.Elapsed < TimeSpan.FromSeconds(10)) Thread.Sleep(5);
                Require(File.Exists(args[3]), "Child did not start");
            }
            Console.WriteLine("ready"); Console.Out.Flush(); Thread.Sleep(30000); return 0;
        }
        throw new ArgumentException(kind);
    }
    public static void Run()
    {
        var previous = Environment.GetEnvironmentVariable("DVDA_RUST_MODE");
        var remove = Environment.GetEnvironmentVariable("DVDA_PROCESS_REMOVE");
        var root = Path.Combine(Path.GetTempPath(), "dvda-process-中文-日本語-🎵-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        Environment.SetEnvironmentVariable("DVDA_PROCESS_REMOVE", "must-remove");
        try
        {
            var echo = new ProcessRequest { FileName = Environment.ProcessPath!, WorkingDirectory = root,
                Arguments = ["--process-migration-fixture", "echo", "", "with spaces", "中日韩🎵", "\"", "a\\\"b", "\\", "ends with \\", "tab\tvalue", "line\nbreak"],
                Environment = new Dictionary<string, string?> { ["DVDA_PROCESS_VALUE"] = "值=日本語🎵", ["DVDA_PROCESS_REMOVE"] = null } };
            Compare(echo);
            foreach (var encoding in new Encoding[] { new UTF8Encoding(false), new UnicodeEncoding(false, false),
                new UnicodeEncoding(true, false), new UTF32Encoding(false, false), new UTF32Encoding(true, false) })
                Compare(echo with { Arguments = ["--process-migration-fixture", "lines", encoding.CodePage.ToString()], OutputEncoding = encoding, ErrorEncoding = encoding });
            var burst = echo with { Arguments = ["--process-migration-fixture", "burst"] };
            Compare(burst); Compare(burst with { CaptureOutput = false, CaptureError = false });
            foreach (var scenario in new[] { "missing", "directory", "nonzero", "timeout", "cancel" })
            {
                var expected = Failure("managed", scenario); var actual = Failure("rust", scenario);
                Require(expected == actual, $"Process {scenario}: {expected} / {actual}");
            }
            // The previous runner allowed callback exceptions to escape an event
            // thread. Verify the Rust owner now cancels and reports safely.
            Failure("rust", "callback");
            var results = Task.WhenAll(Enumerable.Range(0, 4).Select(async i =>
            {
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", "rust");
                var result = await new ProcessRunner().RunAsync(echo with { Arguments = ["--process-fixture", i.ToString()] });
                Require(result.Succeeded && result.StandardOutput.Trim() == i.ToString(), "Concurrent process pipes mixed");
            })); results.GetAwaiter().GetResult();

            void Compare(ProcessRequest request)
            {
                var expected = Run("managed", request); var actual = Run("rust", request);
                Require(expected == actual, "Process result or ordered per-stream events differ: " + JsonSerializer.Serialize(request.Arguments));
            }
            string Failure(string mode, string scenario)
            {
                var folder = Path.Combine(root, mode + "-" + scenario); Directory.CreateDirectory(folder);
                var parent = Path.Combine(folder, "parent.pid"); var child = Path.Combine(folder, "child.pid");
                using var cancellation = new CancellationTokenSource();
                var request = echo with { Arguments = ["--process-migration-fixture", "tree", parent, child], Timeout = TimeSpan.FromSeconds(20) };
                if (scenario == "missing") request = request with { FileName = Path.Combine(root, "missing.exe") };
                if (scenario == "directory") request = request with { WorkingDirectory = Path.Combine(root, "absent") };
                if (scenario == "nonzero") request = request with { Arguments = ["--process-fixture-fail"], ThrowOnNonZeroExitCode = true };
                request = request with { OnOutputLine = line =>
                {
                    if (line != "ready") return;
                    if (scenario == "cancel") cancellation.Cancel();
                    if (scenario == "callback") throw new FormatException("intentional callback failure");
                } };
                if (scenario == "timeout") request = request with { Timeout = TimeSpan.FromSeconds(3) };
                Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
                string type;
                try { new ProcessRunner().RunAsync(request, cancellation.Token).GetAwaiter().GetResult(); throw new Exception("Expected process failure"); }
                catch (Exception error) { type = error is OperationCanceledException ? nameof(OperationCanceledException) : error.GetType().Name; }
                if (scenario == "callback") Require(type == nameof(FormatException), "Callback failure was lost");
                if (scenario is "timeout" or "cancel" or "callback")
                {
                    Require(File.Exists(parent) && File.Exists(child), "Tree fixture did not start before cancellation");
                    foreach (var path in new[] { parent, child })
                    {
                        var pid = int.Parse(File.ReadAllText(path));
                        try { using var process = Process.GetProcessById(pid); Require(process.WaitForExit(5000), "Child process leaked: " + pid); }
                        catch (ArgumentException) { }
                    }
                }
                return type;
            }
        }
        finally { Environment.SetEnvironmentVariable("DVDA_RUST_MODE", previous); Environment.SetEnvironmentVariable("DVDA_PROCESS_REMOVE", remove); Directory.Delete(root, true); }
    }
    private static string Run(string mode, ProcessRequest request)
    {
        Environment.SetEnvironmentVariable("DVDA_RUST_MODE", mode);
        var output = new List<string>(); var error = new List<string>();
        var result = new ProcessRunner().RunAsync(request with { OnOutputLine = output.Add, OnErrorLine = error.Add }).GetAwaiter().GetResult();
        Require(result.Duration >= TimeSpan.Zero, "Negative process duration");
        return JsonSerializer.Serialize(new { result.ExitCode, result.StandardOutput, result.StandardError, Output = output, Error = error });
    }
}
