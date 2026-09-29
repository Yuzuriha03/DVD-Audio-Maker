using System.Text;
using DvdaMaker.Configuration;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed class BuildLogWriter : IDisposable
{
    private readonly StreamWriter _writer;
    private readonly object _gate = new();

    public BuildLogWriter(DvdaOptions options, bool dryRun)
    {
        Path = dryRun
            ? System.IO.Path.Combine(options.BuildDirectory, "build-dryrun.log")
            : options.BuildLogPath;
        Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
        _writer = new StreamWriter(Path, append: true, new UTF8Encoding(false))
        {
            AutoFlush = true,
        };
        WriteLine(string.Empty);
        WriteLine(new string('=', 60));
        WriteLine($"[C# build]{(dryRun ? " [DRY-RUN]" : string.Empty)} {DateTime.Now:yyyy-MM-dd HH:mm:ss}");
        WriteLine($"  dvda-author : {options.DvdaAuthor}");
        WriteLine($"  mkisofs     : {options.Mkisofs}");
        WriteLine($"  output      : {options.FinalDirectory}");
        WriteLine($"  iso prefix  : {options.IsoPrefix}");
        WriteLine($"  title       : {options.Title}");
        WriteLine($"  max discs   : {(options.MaxDiscs == 0 ? "unlimited" : options.MaxDiscs)}");
        if (dryRun)
        {
            WriteLine("  注: dry-run 未执行 dvda-author，本文件不含轨道表；");
            WriteLine("      审计请用 build.log（上次真出盘）。");
        }
        WriteLine(new string('=', 60));
    }

    public string Path { get; }

    public void WriteCommand(string fileName, IReadOnlyList<string> arguments) =>
        WriteLine("+ " + CommandLineFormatter.Format(fileName, arguments));

    public void WriteLine(string line)
    {
        lock (_gate)
        {
            _writer.WriteLine(line);
        }
    }

    public void Dispose() => _writer.Dispose();
}
