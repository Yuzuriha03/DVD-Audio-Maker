using System.Diagnostics;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;

return await ToolchainProgram.RunAsync(args);

internal static class ToolchainProgram
{
    private static readonly string[] RequiredExecutables =
    [
        "dvda-author-dev.exe",
        "mkisofs.exe",
        "dvdauthor.exe",
        "spumux.exe",
        "spuunmux.exe",
        "jpeg2yuv.exe",
        "mpeg2enc.exe",
        "mplex.exe",
        "mp2enc.exe",
        "magick.exe",
    ];

    private static readonly string[] RequiredFonts =
    [
        "NotoSansCJKsc-Regular.otf",
        "NotoSansCJKjp-Regular.otf",
        "NotoSansCJKkr-Regular.otf",
    ];

    public static async Task<int> RunAsync(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        try
        {
            var options = Parse(args);
            await PackageAsync(options);
            return 0;
        }
        catch (ArgumentException exception)
        {
            Console.Error.WriteLine($"[ERROR] {exception.Message}");
            PrintUsage();
            return 2;
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
            Console.Error.WriteLine($"[ERROR] {exception.Message}");
            return 1;
        }
    }

    private static async Task PackageAsync(ToolchainOptions options)
    {
        var repository = FindRepositoryRoot(options.Repository);
        var sourceTree = ResolvePath(options.SourceTree ??
            Environment.GetEnvironmentVariable("DVDA_SRC_TREE") ??
            Path.Combine(repository, "tools", "dvda-author-mlp8"));
        var prebuilt = ResolvePath(options.PrebuiltDirectory ??
            Environment.GetEnvironmentVariable("DVDA_PREBUILT_DIR") ??
            Path.Combine(repository, "tools", "win-build", "prebuilt"));
        var outputBase = ResolvePath(options.OutputDirectory ??
            Path.Combine(repository, "tools", "win-build", "release"));
        var destination = Path.Combine(outputBase, "DVD-Audio-Maker");
        var publish = Path.Combine(repository, "tools", "win-build", "publish", "cli-win-x64");

        ValidatePrebuilt(prebuilt);
        ValidateSourceTree(sourceTree);

        Console.WriteLine("DVD-Audio Maker native Windows packager");
        Console.WriteLine($"  repository : {repository}");
        Console.WriteLine($"  source tree: {sourceTree}");
        Console.WriteLine($"  prebuilt   : {prebuilt}");
        Console.WriteLine($"  output     : {destination}");

        RecreateDirectory(publish);
        await RunAsync("dotnet",
        [
            "publish", Path.Combine(repository, "src", "DvdaMaker.Cli", "DvdaMaker.Cli.csproj"),
            "--configuration", "Release",
            "--runtime", "win-x64",
            "--self-contained", "true",
            "--output", publish,
            "-p:PublishSingleFile=true",
            "-p:IncludeNativeLibrariesForSelfExtract=true",
            "-p:DebugType=None",
            "-p:DebugSymbols=false",
        ], repository);

        var guiPublish = Path.Combine(repository, "tools", "win-build", "publish", "gui-win-x64");
        RecreateDirectory(guiPublish);
        await RunAsync("dotnet",
        [
            "publish", Path.Combine(repository, "src", "DvdaMaker.Desktop", "DvdaMaker.Desktop.csproj"),
            "--configuration", "Release", "--runtime", "win-x64", "--self-contained", "true",
            "--output", guiPublish, "-p:PublishSingleFile=true", "-p:IncludeNativeLibrariesForSelfExtract=true",
            "-p:DebugType=None", "-p:DebugSymbols=false",
        ], repository);
        if (!File.Exists(Path.Combine(guiPublish, "DVD-Audio-Maker.exe")))
            throw new InvalidOperationException("GUI publish did not create DVD-Audio-Maker.exe");

        var executable = Path.Combine(publish, "dvda.exe");
        if (!File.Exists(executable))
        {
            throw new InvalidOperationException($"dotnet publish did not create {executable}");
        }

        RecreateDirectory(destination);
        CopyDirectory(publish, Path.Combine(destination, "app"));
        CopyDirectory(guiPublish, destination);
        CopyDirectory(prebuilt, Path.Combine(destination, "menu-bin"));
        CopyDirectory(Path.Combine(sourceTree, "menu"), Path.Combine(destination, "data", "menu"));
        File.Copy(Path.Combine(repository, "config.env"), Path.Combine(destination, "config.env"), true);
        WriteLauncher(destination);
        CopyDocumentation(repository, destination);
        WriteManifest(destination);

        var archive = destination + ".zip";
        if (File.Exists(archive)) File.Delete(archive);
        ZipFile.CreateFromDirectory(destination, archive, CompressionLevel.Optimal, false);

        Console.WriteLine($"[OK] Release directory: {destination}");
        Console.WriteLine($"[OK] ZIP archive: {archive}");
    }

    private static ToolchainOptions Parse(string[] args)
    {
        string? repository = null;
        string? source = null;
        string? prebuilt = null;
        string? output = null;
        for (var index = 0; index < args.Length; index++)
        {
            var value = args[index];
            if (value.Equals("package", StringComparison.OrdinalIgnoreCase)) continue;
            if (value is "--repo" or "--source" or "--prebuilt" or "--output")
            {
                if (++index >= args.Length)
                {
                    throw new ArgumentException($"{value} requires a path.");
                }
                switch (value)
                {
                    case "--repo": repository = args[index]; break;
                    case "--source": source = args[index]; break;
                    case "--prebuilt": prebuilt = args[index]; break;
                    case "--output": output = args[index]; break;
                }
                continue;
            }
            throw new ArgumentException($"Unknown argument: {value}");
        }
        return new ToolchainOptions(repository, source, prebuilt, output);
    }

    private static void ValidatePrebuilt(string directory)
    {
        if (!Directory.Exists(directory))
        {
            throw new InvalidOperationException(
                "No native third-party binary directory was found. " +
                $"Create '{directory}' or set DVDA_PREBUILT_DIR. " +
                "The directory must contain already-built Windows x64 dvda-author/menu tools and their DLLs; " +
                "this repository no longer invokes Autotools, Make, MSYS2, Bash, or WSL.");
        }

        var missing = RequiredExecutables
            .Where(name => !File.Exists(Path.Combine(directory, name)))
            .Concat(RequiredFonts.Where(name =>
                !File.Exists(Path.Combine(directory, "fonts", name))))
            .ToArray();
        if (missing.Length > 0)
        {
            throw new InvalidOperationException(
                $"Prebuilt directory is incomplete: {directory}{Environment.NewLine}" +
                string.Join(Environment.NewLine, missing.Select(name => $"  missing: {name}")));
        }
    }

    private static void ValidateSourceTree(string directory)
    {
        var required = new[]
        {
            Path.Combine(directory, "menu", "silence.wav"),
            Path.Combine(directory, "menu", "activeheader"),
        };
        var missing = required.Where(path => !File.Exists(path)).ToArray();
        if (missing.Length > 0)
        {
            throw new InvalidOperationException(
                "The dvda-author runtime asset tree is incomplete. Set DVDA_SRC_TREE or --source." +
                Environment.NewLine + string.Join(Environment.NewLine, missing.Select(path => $"  missing: {path}")));
        }
    }

    private static async Task RunAsync(
        string fileName,
        IReadOnlyList<string> arguments,
        string workingDirectory)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = fileName,
            WorkingDirectory = workingDirectory,
            UseShellExecute = false,
        };
        foreach (var argument in arguments) startInfo.ArgumentList.Add(argument);
        using var process = Process.Start(startInfo) ??
            throw new InvalidOperationException($"Unable to start {fileName}.");
        await process.WaitForExitAsync();
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException($"{fileName} exited with code {process.ExitCode}.");
        }
    }

    private static void WriteLauncher(string destination)
    {
        const string content = """
@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "ROOT=%~dp0"
if "%~1"=="" (
    start "" "%ROOT%DVD-Audio-Maker.exe"
    exit /b 0
)
set "DVDA_AUTHOR=%ROOT%menu-bin\dvda-author-dev.exe"
set "DVDA_MKISOFS=%ROOT%menu-bin\mkisofs.exe"
set "DVDA_AUTHOR_SRC=%ROOT%data"
set "DVDA_MENU_FONT=%ROOT%menu-bin\fonts\NotoSansCJKsc-Regular.otf"
set "DVDA_MENU_FONT_JP=%ROOT%menu-bin\fonts\NotoSansCJKjp-Regular.otf"
set "DVDA_MENU_FONT_KR=%ROOT%menu-bin\fonts\NotoSansCJKkr-Regular.otf"
if not exist "%ROOT%app\dvda.exe" (
    echo [ERROR] Missing app\dvda.exe
    exit /b 2
)
cd /d "%ROOT%"
"%ROOT%app\dvda.exe" %*
exit /b %ERRORLEVEL%
""";
        File.WriteAllText(Path.Combine(destination, "dvda.cmd"), content, Encoding.ASCII);
    }

    private static void CopyDocumentation(string repository, string destination)
    {
        var candidates = new Dictionary<string, string[]>
        {
            ["README.md"] =
            [
                Path.Combine(repository, "tools", "win-build", "docs", "README.md"),
                Path.Combine(repository, "README.md"),
            ],
            ["THIRD-PARTY.md"] =
            [
                Path.Combine(repository, "tools", "win-build", "docs", "THIRD-PARTY.md"),
            ],
            ["LICENSE"] =
            [
                Path.Combine(repository, "tools", "win-build", "docs", "LICENSE"),
                Path.Combine(repository, "LICENSE"),
            ],
        };
        foreach (var pair in candidates)
        {
            var source = pair.Value.FirstOrDefault(File.Exists) ??
                throw new InvalidOperationException($"Required release document is missing: {pair.Key}");
            File.Copy(source, Path.Combine(destination, pair.Key), true);
        }
    }

    private static void WriteManifest(string destination)
    {
        var files = Directory.EnumerateFiles(destination, "*", SearchOption.AllDirectories)
            .Where(path => !path.EndsWith("MANIFEST.txt", StringComparison.OrdinalIgnoreCase))
            .OrderBy(path => Path.GetRelativePath(destination, path), StringComparer.OrdinalIgnoreCase)
            .ToArray();
        using var writer = new StreamWriter(Path.Combine(destination, "MANIFEST.txt"), false,
            new UTF8Encoding(false));
        writer.WriteLine("# DVD-Audio Maker SHA-256 release manifest");
        writer.WriteLine($"# Generated: {DateTime.UtcNow:yyyy-MM-dd HH:mm:ss} UTC");
        foreach (var path in files)
        {
            using var stream = File.OpenRead(path);
            var hash = Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
            writer.WriteLine($"{hash}  {Path.GetRelativePath(destination, path).Replace('\\', '/')}");
        }
    }

    private static string FindRepositoryRoot(string? requested)
    {
        var current = new DirectoryInfo(ResolvePath(requested ?? Environment.CurrentDirectory));
        while (current is not null)
        {
            if (File.Exists(Path.Combine(current.FullName, "DVD-Audio-Maker.sln")))
                return current.FullName;
            current = current.Parent;
        }
        throw new InvalidOperationException("Unable to locate DVD-Audio-Maker.sln; use --repo.");
    }

    private static string ResolvePath(string path) => Path.GetFullPath(
        Environment.ExpandEnvironmentVariables(path));

    private static void RecreateDirectory(string path)
    {
        if (Directory.Exists(path)) Directory.Delete(path, true);
        Directory.CreateDirectory(path);
    }

    private static void CopyDirectory(string source, string destination)
    {
        Directory.CreateDirectory(destination);
        foreach (var directory in Directory.EnumerateDirectories(source, "*", SearchOption.AllDirectories))
            Directory.CreateDirectory(Path.Combine(destination, Path.GetRelativePath(source, directory)));
        foreach (var file in Directory.EnumerateFiles(source, "*", SearchOption.AllDirectories))
        {
            var target = Path.Combine(destination, Path.GetRelativePath(source, file));
            Directory.CreateDirectory(Path.GetDirectoryName(target)!);
            File.Copy(file, target, true);
        }
    }

    private static void PrintUsage() => Console.Error.WriteLine(
        "Usage: build-all.cmd [--source <dvda-author tree>] [--prebuilt <menu-bin>] [--output <directory>]");

    private sealed record ToolchainOptions(
        string? Repository,
        string? SourceTree,
        string? PrebuiltDirectory,
        string? OutputDirectory);
}
