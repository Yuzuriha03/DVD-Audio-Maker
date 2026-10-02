using System.Diagnostics;
using System.IO.Compression;
using System.Reflection.PortableExecutable;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;
using DvdaMaker.FontTool;
using DvdaMaker.Toolchain;

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
            Path.Combine(repository, "tools", "win-build",
                options.FrameworkDependent ? "release-framework-dependent" : "release"));
        var destination = Path.Combine(outputBase, "DVD-Audio-Maker");
        var publish = Path.Combine(repository, "tools", "win-build", "publish", "cli-win-x64");
        var artifacts = Path.Combine(repository, "tools", "win-build", "publish", "build-artifacts-win-x64");
        var selfContained = options.FrameworkDependent ? "false" : "true";

        ValidatePrebuilt(prebuilt);
        ValidateSourceTree(sourceTree);
        var imageAuthor = ResolvePath(options.ImageAuthor ?? Path.Combine(repository, "build", "image-author"));
        NativeImagePackager.ValidateAuthor(imageAuthor);
        var magickShim = options.MagickShim is null ? null : ResolvePath(options.MagickShim);
        if (magickShim is not null) NativeToolOptimizer.ValidateShim(magickShim);
        var ffmpegLibraries = options.FfmpegLibraries is null ? null : ResolvePath(options.FfmpegLibraries);
        if (ffmpegLibraries is not null) NativeToolOptimizer.ValidateMinimalFfmpeg(ffmpegLibraries);

        Console.WriteLine("DVD-Audio Maker native Windows packager");
        Console.WriteLine($"  repository : {repository}");
        Console.WriteLine($"  source tree: {sourceTree}");
        Console.WriteLine($"  prebuilt   : {prebuilt}");
        Console.WriteLine($"  output     : {destination}");
        Console.WriteLine($"  entrypoints: {(options.IncludeCli ? "GUI + developer CLI" : "GUI only")}");
        Console.WriteLine($"  runtime    : {(options.FrameworkDependent ? "Requires installed .NET 10 Desktop Runtime x64" : "Self-contained")}");

        // Keep packaging independent of Debug/test builds in the workspace bin/obj.
        RecreateDirectory(artifacts);

        if (options.IncludeCli)
        {
            RecreateDirectory(publish);
            await RunAsync("dotnet",
            [
                "publish", Path.Combine(repository, "src", "DvdaMaker.Cli", "DvdaMaker.Cli.csproj"),
                "--configuration", "Release",
                "--runtime", "win-x64",
                "--self-contained", selfContained,
                "--output", publish,
                "--artifacts-path", artifacts,
                "-p:PublishSingleFile=false",
                "-p:ShareDesktopRuntime=true",
                "-p:DebugType=None",
                "-p:DebugSymbols=false",
            ], repository);
            if (!File.Exists(Path.Combine(publish, "dvda.exe")))
                throw new InvalidOperationException("CLI publish did not create dvda.exe");
        }

        var guiPublish = Path.Combine(repository, "tools", "win-build", "publish", "gui-win-x64");
        RecreateDirectory(guiPublish);
        await RunAsync("dotnet",
        [
            "publish", Path.Combine(repository, "src", "DvdaMaker.Desktop", "DvdaMaker.Desktop.csproj"),
            "--configuration", "Release", "--runtime", "win-x64", "--self-contained", selfContained,
            "--output", guiPublish, "--artifacts-path", artifacts, "-p:PublishSingleFile=false",
            "-p:DebugType=None", "-p:DebugSymbols=false",
        ], repository);
        if (!File.Exists(Path.Combine(guiPublish, "DVD-Audio-Maker.exe")))
            throw new InvalidOperationException("GUI publish did not create DVD-Audio-Maker.exe");
        ValidateMediaRuntime(Path.Combine(guiPublish, "media-native"));
        NativeImagePackager.ValidateRuntime(Path.Combine(guiPublish, "image-native"));

        RecreateDirectory(destination);
        CopyDirectory(guiPublish, destination);
        if (options.IncludeCli)
        {
            MergeSharedPublish(publish, destination);
            WriteLauncher(destination);
            foreach (var name in new[] { "CLI-TOOLS.md", "CLI-TOOLS.en.md" })
                File.Copy(Path.Combine(repository, "tools", "win-build", "docs", name),
                    Path.Combine(destination, name));
        }
        CopyDirectory(prebuilt, Path.Combine(destination, "menu-bin"));
        if (ffmpegLibraries is not null)
        {
            NativeToolOptimizer.InstallMinimalFfmpeg(Path.Combine(destination, "menu-bin"), ffmpegLibraries);
            Console.WriteLine("  native FFmpeg: verified x64 MLP-only libraries");
        }
        var removedNative = NativeToolOptimizer.Optimize(Path.Combine(destination, "menu-bin"), magickShim);
        if (magickShim is not null) Console.WriteLine("  shared ImageMagick: two native forwarding entrypoints");
        if (removedNative.Count > 0) Console.WriteLine("  unused native libraries removed: " + string.Join(", ", removedNative));
        PackMenuFonts(Path.Combine(destination, "menu-bin"));
        NativeImagePackager.Install(destination, imageAuthor);
        CopyDirectory(Path.Combine(sourceTree, "menu"), Path.Combine(destination, "data", "menu"));
        File.Copy(Path.Combine(repository, "config.env"), Path.Combine(destination, "config.env"), true);
        CopyDocumentation(repository, destination);
        WriteRuntimeRequirements(destination, options.FrameworkDependent);
        WriteManifest(destination);

        var archive = destination + ".zip";
        if (File.Exists(archive)) File.Delete(archive);
        ZipFile.CreateFromDirectory(destination, archive, CompressionLevel.SmallestSize, false);

        Console.WriteLine($"[OK] Release directory: {destination}");
        Console.WriteLine($"[OK] ZIP archive: {archive}");
    }

    private static ToolchainOptions Parse(string[] args)
    {
        string? repository = null;
        string? source = null;
        string? prebuilt = null;
        string? output = null;
        string? magickShim = null;
        string? ffmpegLibraries = null;
        string? imageAuthor = null;
        var includeCli = false;
        var frameworkDependent = false;
        for (var index = 0; index < args.Length; index++)
        {
            var value = args[index];
            if (value.Equals("package", StringComparison.OrdinalIgnoreCase)) continue;
            if (value == "--include-cli") { includeCli = true; continue; }
            if (value == "--framework-dependent") { frameworkDependent = true; continue; }
            if (value is "--repo" or "--source" or "--prebuilt" or "--output" or "--magick-shim" or "--ffmpeg-libraries" or "--image-author")
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
                    case "--magick-shim": magickShim = args[index]; break;
                    case "--ffmpeg-libraries": ffmpegLibraries = args[index]; break;
                    case "--image-author": imageAuthor = args[index]; break;
                }
                continue;
            }
            throw new ArgumentException($"Unknown argument: {value}");
        }
        return new ToolchainOptions(repository, source, prebuilt, output, includeCli, frameworkDependent, magickShim, ffmpegLibraries, imageAuthor);
    }

    private static void ValidatePrebuilt(string directory)
    {
        if (!Directory.Exists(directory))
        {
            throw new InvalidOperationException(
                "No native third-party binary directory was found. " +
                $"Create '{directory}' or set DVDA_PREBUILT_DIR. " +
                "The directory must contain already-built Windows x64 dvda-author/menu tools and their DLLs; " +
                "normal GUI packaging does not invoke native build tools; optional native rebuilds are separate.");
        }

        var missing = RequiredExecutables
            .Where(name => !File.Exists(Path.Combine(directory, name)))
            .Concat(File.Exists(Path.Combine(directory, "fonts", "DvdaNotoCJK-Regular.ttc"))
                ? []
                : RequiredFonts.Where(name => !File.Exists(Path.Combine(directory, "fonts", name))))
            .ToArray();
        if (missing.Length > 0)
        {
            throw new InvalidOperationException(
                $"Prebuilt directory is incomplete: {directory}{Environment.NewLine}" +
                string.Join(Environment.NewLine, missing.Select(name => $"  missing: {name}")));
        }
    }

    private static void ValidateMediaRuntime(string directory)
    {
        var manifest = Path.Combine(directory, "media-build.json");
        if (!File.Exists(manifest))
            throw new InvalidOperationException("Missing in-process media runtime. Build it with " +
                "tools/win-build/build-media-bridge.py, or supply the prebuilt files in build/media-native.");
        using var record = JsonDocument.Parse(File.ReadAllText(manifest));
        var files = record.RootElement.GetProperty("files").EnumerateObject()
            .ToDictionary(x => x.Name, x => x.Value, StringComparer.OrdinalIgnoreCase);
        foreach (var name in new[] { "dvda-media.dll", "avcodec-63.dll", "avformat-63.dll", "avutil-61.dll", "swresample-7.dll", "swscale-10.dll" })
            if (!files.ContainsKey(name)) throw new InvalidDataException("Incomplete media manifest: " + name);
        var actual = Directory.EnumerateFiles(directory, "*.dll").Select(Path.GetFileName).ToHashSet(StringComparer.OrdinalIgnoreCase);
        if (!actual.SetEquals(files.Keys)) throw new InvalidDataException("Media DLL files do not match their build manifest.");
        foreach (var (name, metadata) in files)
        {
            if (Path.GetFileName(name) != name || !name.EndsWith(".dll", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Invalid media library name: " + name);
            var path = Path.Combine(directory, name);
            using var stream = File.OpenRead(path);
            if (stream.Length != metadata.GetProperty("bytes").GetInt64() ||
                !Convert.ToHexString(SHA256.HashData(stream)).Equals(metadata.GetProperty("sha256").GetString(), StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Media library checksum mismatch: " + name);
            stream.Position = 0;
            using var pe = new PEReader(stream);
            if (pe.PEHeaders.CoffHeader.Machine != Machine.Amd64)
                throw new InvalidDataException("Media library must be Windows x64: " + name);
            foreach (var import in NativeToolOptimizer.ReadImports(path))
                if (!files.ContainsKey(import) && !File.Exists(Path.Combine(Environment.SystemDirectory, import)))
                    throw new InvalidDataException("Missing media library dependency: " + import);
        }
        Console.WriteLine($"  in-process media: {files.Count} verified x64 libraries");
    }

    private static void PackMenuFonts(string directory)
    {
        var fonts = Path.Combine(directory, "fonts");
        var collection = Path.Combine(fonts, "DvdaNotoCJK-Regular.ttc");
        if (RequiredFonts.All(name => File.Exists(Path.Combine(fonts, name))))
            OpenTypeFontTool.PackNotoCjkFaces(fonts, collection);
        OpenTypeFontTool.VerifyNotoCjkCollection(collection);
        var map = new XElement("typemap");
        var regions = new[] { "SC", "JP", "KR" };
        for (var index = 0; index < regions.Length; index++)
            map.Add(new XElement("type",
                new XAttribute("name", "DVDA-Noto-Sans-CJK-" + regions[index]),
                new XAttribute("family", "DVDA Noto Sans CJK " + regions[index]),
                new XAttribute("format", "truetype"), new XAttribute("style", "normal"),
                new XAttribute("stretch", "normal"), new XAttribute("weight", "400"),
                new XAttribute("face", index),
                new XAttribute("glyphs", "fonts/DvdaNotoCJK-Regular.ttc")));
        new XDocument(map).Save(Path.Combine(directory, "type-dvda-cjk.xml"));
        var typesPath = Path.Combine(directory, "type.xml");
        var types = File.Exists(typesPath) ? XDocument.Load(typesPath) : new XDocument(new XElement("typemap"));
        if (types.Root is not { Name.LocalName: "typemap" } root)
            throw new InvalidDataException("ImageMagick type.xml has no typemap root.");
        if (!root.Elements("include").Any(element => (string?)element.Attribute("file") == "type-dvda-cjk.xml"))
            root.AddFirst(new XElement("include", new XAttribute("file", "type-dvda-cjk.xml")));
        types.Save(typesPath);
        // Remove only the three source copies in the freshly built destination.
        foreach (var name in RequiredFonts) File.Delete(Path.Combine(fonts, name));
        Console.WriteLine($"  shared fonts: {new FileInfo(collection).Length:N0} bytes (SC / JP / KR)");
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
set "DVDA_MENU_FONT=DVDA-Noto-Sans-CJK-SC"
set "DVDA_MENU_FONT_JP=DVDA-Noto-Sans-CJK-JP"
set "DVDA_MENU_FONT_KR=DVDA-Noto-Sans-CJK-KR"
if not exist "%ROOT%dvda.exe" (
    echo [ERROR] Missing dvda.exe
    exit /b 2
)
cd /d "%ROOT%"
"%ROOT%dvda.exe" %*
exit /b %ERRORLEVEL%
""";
        File.WriteAllText(Path.Combine(destination, "dvda.cmd"), content, Encoding.ASCII);
    }

    private static void CopyDocumentation(string repository, string destination)
    {
        var candidates = new Dictionary<string, string[]>
        {
            ["README.en.md"] =
            [
                Path.Combine(repository, "tools", "win-build", "docs", "README.en.md"),
            ],
            ["THIRD-PARTY.en.md"] =
            [
                Path.Combine(repository, "tools", "win-build", "docs", "THIRD-PARTY.en.md"),
            ],
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

    private static void WriteRuntimeRequirements(string destination, bool frameworkDependent)
    {
        var chinese = frameworkDependent
            ? "本精简包不包含 .NET 运行时。**首次运行前，请安装 .NET 10 Desktop Runtime（Windows x64）**。打开 [微软官方下载页](https://dotnet.microsoft.com/download/dotnet/10.0)，在 .NET Desktop Runtime 栏选择 Windows x64 安装程序。只有普通 .NET Runtime、ASP.NET Core Runtime 或 .NET Framework 4.x 不够。已安装兼容的 Microsoft.WindowsDesktop.App 10.0.x 则无需重复安装。"
            : "本包自带 Windows x64 .NET 运行时，无需单独安装 .NET。";
        var english = frameworkDependent
            ? "This compact package excludes the .NET runtime. **Before first use, install .NET 10 Desktop Runtime for Windows x64.** On the [official Microsoft download page](https://dotnet.microsoft.com/download/dotnet/10.0), choose the Windows x64 installer under .NET Desktop Runtime. The plain .NET Runtime, ASP.NET Core Runtime or .NET Framework 4.x alone is insufficient. An existing compatible Microsoft.WindowsDesktop.App 10.0.x installation can be reused."
            : "This package includes its Windows x64 .NET runtime; no separate .NET installation is needed.";
        File.WriteAllText(Path.Combine(destination, "RUNTIME.md"),
            "# 运行要求\n\n" + chinese + "\n\n请完整解压整个目录，包括 media-native 和 menu-bin。媒体处理组件已随包提供，无需安装 FFmpeg 或 FFprobe。\n", new UTF8Encoding(false));
        File.WriteAllText(Path.Combine(destination, "RUNTIME.en.md"),
            "# Runtime requirements\n\n" + english + "\n\nExtract the complete directory, including media-native and menu-bin. Media processing is bundled; no FFmpeg or FFprobe installation is required.\n", new UTF8Encoding(false));
        foreach (var (name, notice) in new[]
        {
            ("README.md", "> **运行环境：** " + chinese),
            ("README.en.md", "> **Runtime requirement:** " + english),
        })
        {
            var path = Path.Combine(destination, name);
            var content = File.ReadAllText(path);
            const string marker = "<!-- RUNTIME_REQUIREMENTS -->";
            content = content.Contains(marker, StringComparison.Ordinal)
                ? content.Replace(marker, notice, StringComparison.Ordinal)
                : content.Insert(content.IndexOf('\n') + 1, "\n" + notice + "\n");
            File.WriteAllText(path, content, new UTF8Encoding(false));
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

    private static void MergeSharedPublish(string source, string destination)
    {
        // GUI and optional CLI share dependencies in either runtime profile.
        // Never silently overwrite a dependency compiled with different settings.
        foreach (var file in Directory.EnumerateFiles(source, "*", SearchOption.AllDirectories))
        {
            var relative = Path.GetRelativePath(source, file);
            var target = Path.Combine(destination, relative);
            if (File.Exists(target))
            {
                using var existing = File.OpenRead(target);
                using var incoming = File.OpenRead(file);
                if (existing.Length != incoming.Length ||
                    !SHA256.HashData(existing).AsSpan().SequenceEqual(SHA256.HashData(incoming)))
                    throw new InvalidOperationException($"GUI/CLI shared dependency differs: {relative}");
                continue;
            }
            Directory.CreateDirectory(Path.GetDirectoryName(target)!);
            File.Copy(file, target);
        }
    }

    private static void PrintUsage() => Console.Error.WriteLine(
        "Usage: build-all.cmd [--source <dvda-author tree>] [--prebuilt <menu-bin>] [--output <directory>] [--include-cli] [--framework-dependent] [--ffmpeg-libraries <verified MLP DLL directory>] [--image-author <rebuilt native author directory>]");

    private sealed record ToolchainOptions(
        string? Repository,
        string? SourceTree,
        string? PrebuiltDirectory,
        string? OutputDirectory,
        bool IncludeCli,
        bool FrameworkDependent,
        string? MagickShim,
        string? FfmpegLibraries,
        string? ImageAuthor);
}
