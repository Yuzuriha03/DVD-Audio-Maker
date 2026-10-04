using System.Diagnostics;
using System.IO.Compression;
using System.Reflection.PortableExecutable;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;
using DvdaMaker.FontTool;
using DvdaMaker.Toolchain;
using DvdaMaker.Packaging;

return await ToolchainProgram.RunAsync(args);

internal static class ToolchainProgram
{
    private static readonly string[] RequiredExecutables =
    [
        "dvda-author-dev.exe",
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
                options.OneFile ? "release-onefile" : options.FrameworkDependent ? "release-framework-dependent" : "release"));
        var destination = options.OneFile
            ? Path.Combine(repository, "tools", "win-build", "publish", "package-stage-win-x64")
            : Path.Combine(outputBase, "DVD-Audio-Maker");
        var publish = Path.Combine(repository, "tools", "win-build", "publish", "cli-win-x64");
        var artifacts = Path.Combine(repository, "tools", "win-build", "publish", "build-artifacts-win-x64");
        var selfContained = options.FrameworkDependent ? "false" : "true";
        var nativeMedia = ResolvePath(options.MediaRuntime ?? Path.Combine(repository, "build",
            options.OneFile ? "media-native-shared" : "media-native"));

        ValidatePrebuilt(prebuilt);
        ValidateSourceTree(sourceTree);
        var imageAuthor = ResolvePath(options.ImageAuthor ?? Path.Combine(repository, "build",
            options.OneFile ? "image-author-shared" : "image-author"));
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
        Console.WriteLine($"  format     : {(options.OneFile ? "Single EXE with cached native components" : "Directory + ZIP")}");
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
                "-p:NativeMediaDirectory=" + nativeMedia, "-m:1",
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
            "-p:DebugType=None", "-p:DebugSymbols=false", "-m:1",
            "-p:NativeMediaDirectory=" + nativeMedia,
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
        var legacyMkisofs = Path.Combine(destination, "menu-bin", "mkisofs.exe");
        if (File.Exists(legacyMkisofs))
        {
            File.Delete(legacyMkisofs);
            Console.WriteLine("  built-in ISO writer: legacy mkisofs.exe omitted");
        }
        var unusedSpuunmux = Path.Combine(destination, "menu-bin", "spuunmux.exe");
        if (File.Exists(unusedSpuunmux))
        {
            File.Delete(unusedSpuunmux);
            Console.WriteLine("  unused reverse subpicture parser: spuunmux.exe omitted");
        }
        var unusedJpeg2Yuv = Path.Combine(destination, "menu-bin", "jpeg2yuv.exe");
        if (File.Exists(unusedJpeg2Yuv))
        {
            File.Delete(unusedJpeg2Yuv);
            Console.WriteLine("  in-process menu image conversion: jpeg2yuv.exe omitted");
        }
        foreach (var name in new[] { "mpeg2enc.exe", "mplex.exe", "mp2enc.exe", "spumux.exe", "dvdauthor.exe" })
        {
            var path = Path.Combine(destination, "menu-bin", name);
            if (File.Exists(path))
            {
                File.Delete(path);
                Console.WriteLine("  in-process menu media encoding: " + name + " omitted");
            }
        }
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
        ConsolidateSharedMedia(destination, options.OneFile);
        CopyDirectory(Path.Combine(sourceTree, "menu"), Path.Combine(destination, "data", "menu"));
        CopyDocumentation(repository, destination);
        File.Copy(Path.Combine(repository, "config.env"),
            Path.Combine(destination, options.OneFile ? "config.env.example" : "config.env"), true);
        WriteRuntimeRequirements(destination, options.FrameworkDependent, options.OneFile);
        WriteManifest(destination);

        if (options.OneFile)
        {
            await PublishOneFileAsync(repository, destination, outputBase, artifacts, options.ReleaseVersion);
            return;
        }

        var archive = destination + ".zip";
        if (File.Exists(archive)) File.Delete(archive);
        ZipFile.CreateFromDirectory(destination, archive, CompressionLevel.SmallestSize, false);

        Console.WriteLine($"[OK] Release directory: {destination}");
        Console.WriteLine($"[OK] ZIP archive: {archive}");
    }

    private static async Task PublishOneFileAsync(string repository, string stage, string outputBase, string artifacts, string releaseVersion)
    {
        var bundle = Path.Combine(repository, "tools", "win-build", "publish", "runtime-bundle-win-x64");
        var publish = Path.Combine(repository, "tools", "win-build", "publish", "onefile-win-x64");
        RecreateDirectory(bundle);
        RecreateDirectory(publish);
        // Explicit runtime allowlist: documentation and notices accompany the EXE.
        // Build provenance stays local; the SDK separately bundles managed code.
        var required = new[]
        {
            "image-native/dvda-image.dll", "image-native/colors.xml",
            "image-native/policy.xml", "image-native/type.xml",
            "menu-bin/dvda-author-dev.exe", "menu-bin/fonts/DvdaNotoCJK-Regular.ttc",
            "data/menu/activeheader", "data/menu/silence.wav",
            "data/menu/black_PAL_720x576.jpg", "data/menu/black_PAL_720x576.png",
            "data/menu/black_NTSC_720x480.jpg", "data/menu/black_NTSC_720x480.png",
        };
        var files = required.Select(name => Path.Combine(stage, name))
            .Concat(Directory.EnumerateFiles(Path.Combine(stage, "menu-bin"), "*.dll")).ToArray();
        foreach (var file in files)
            if (!File.Exists(file)) throw new FileNotFoundException("Required single-file runtime asset is missing.", file);
        var record = RuntimeArchive.Create(stage, files, Path.Combine(bundle, "runtime.br"), Path.Combine(bundle, "runtime.json"));
        var unpacked = record.Blobs.Sum(blob => blob.Length * blob.Paths.Length);
        var unique = record.Blobs.Sum(blob => blob.Length);
        Console.WriteLine($"  runtime assets: {files.Length} paths, {record.Blobs.Length} unique contents");
        Console.WriteLine($"  deduplicated : {unpacked - unique:N0} bytes before compression");
        Console.WriteLine($"  payload      : {new FileInfo(Path.Combine(bundle, "runtime.br")).Length:N0} bytes (from {unpacked:N0})");
        await RunAsync("dotnet",
        [
            "publish", Path.Combine(repository, "src", "DvdaMaker.Desktop", "DvdaMaker.Desktop.csproj"),
            "--configuration", "Release", "--runtime", "win-x64", "--self-contained", "false", "-m:1",
            "--output", publish, "--artifacts-path", Path.Combine(artifacts, "onefile"),
            "-p:PublishSingleFile=true", "-p:PublishReadyToRun=false", "-p:PublishTrimmed=false",
            "-p:EnableCompressionInSingleFile=false", "-p:DebugType=None", "-p:DebugSymbols=false",
            "-p:OneFilePayloadDirectory=" + bundle,
        ], repository);
        var executable = Path.Combine(publish, "DVD-Audio-Maker.exe");
        if (!File.Exists(executable) || Directory.EnumerateFiles(publish, "*", SearchOption.AllDirectories).Count() != 1)
            throw new InvalidOperationException("Single-file publish must produce exactly one executable.");
        Directory.CreateDirectory(outputBase);
        var destination = Path.Combine(outputBase, "DVD-Audio-Maker.exe");
        var temporary = destination + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try { File.Copy(executable, temporary); File.Move(temporary, destination, true); }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
        var sidecars = new[]
        {
            "README.md", "README.en.md", "README.ja.md", "RUNTIME.md", "RUNTIME.en.md", "RUNTIME.ja.md",
            "LICENSE", "THIRD-PARTY.md", "THIRD-PARTY.en.md", "config.env.example",
        };
        foreach (var name in sidecars) File.Copy(Path.Combine(stage, name), Path.Combine(outputBase, name), true);
        var notices = new Dictionary<string, string>
        {
            ["image-native/NOTICE.txt"] = "NOTICE-Image.txt",
            ["menu-bin/menu-NOTICE.txt"] = "NOTICE-Menu.txt",
        };
        // Remove only metadata previously generated by this packager. Source
        // build records remain available to validation and are never modified.
        foreach (var name in new[] { "image-native/image-build.json", "image-native/author-build.json",
            "menu-bin/menu-build.json", "menu-bin/media-build.json" })
        {
            var obsolete = Path.Combine(outputBase, "components", name);
            if (File.Exists(obsolete)) File.Delete(obsolete);
        }
        foreach (var name in notices)
        {
            File.Copy(Path.Combine(stage, name.Key), Path.Combine(outputBase, name.Value), true);
            var obsolete = Path.Combine(outputBase, "components", name.Key);
            if (File.Exists(obsolete)) File.Delete(obsolete);
        }
        foreach (var name in new[] { "components/image-native", "components/menu-bin", "components" })
        {
            var directory = Path.Combine(outputBase, name);
            if (Directory.Exists(directory) && !Directory.EnumerateFileSystemEntries(directory).Any())
                Directory.Delete(directory);
        }
        var packaged = sidecars.Concat(notices.Values)
            .Append("DVD-Audio-Maker.exe").Order(StringComparer.Ordinal).ToArray();
        File.WriteAllLines(Path.Combine(outputBase, "MANIFEST.txt"), packaged.Select(name =>
            RuntimeArchive.HashFile(Path.Combine(outputBase, name)) + "  " + name), new UTF8Encoding(false));
        var archive = Path.Combine(outputBase, $"DVD-Audio-Maker-{releaseVersion}-win-x64.zip");
        var temporaryArchive = archive + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var zip = ZipFile.Open(temporaryArchive, ZipArchiveMode.Create))
                foreach (var name in packaged.Append("MANIFEST.txt"))
                    zip.CreateEntryFromFile(Path.Combine(outputBase, name), name, CompressionLevel.SmallestSize);
            File.Move(temporaryArchive, archive, true);
        }
        finally { if (File.Exists(temporaryArchive)) File.Delete(temporaryArchive); }
        Console.WriteLine($"[OK] Single EXE: {destination} ({new FileInfo(destination).Length:N0} bytes)");
        Console.WriteLine($"[OK] SHA-256: {RuntimeArchive.HashFile(destination)}");
        Console.WriteLine($"[OK] Release ZIP with documentation and notices: {archive}");
    }

    private static void ConsolidateSharedMedia(string destination, bool requireShared)
    {
        var media = Path.Combine(destination, "media-native");
        var native = Path.Combine(destination, "menu-bin");
        using var mediaRecord = JsonDocument.Parse(File.ReadAllText(Path.Combine(media, "media-build.json")));
        using var authorRecord = JsonDocument.Parse(File.ReadAllText(Path.Combine(destination, "image-native", "author-build.json")));
        var shared = mediaRecord.RootElement.TryGetProperty("profile", out var profile) && profile.GetString() == "shared" &&
            authorRecord.RootElement.GetProperty("ffmpeg_profile").GetString() == "build-minimal-ffmpeg.py:shared";
        if (!shared)
        {
            if (requireShared) throw new InvalidOperationException(
                "Onefile requires both consumers built against the shared FFmpeg profile. " +
                "Build --profile shared, then rebuild the media bridge and author against that prefix; use --media-runtime and --image-author.");
            return;
        }
        // Verify all collisions before removing any staged duplicate. Both consumers
        // must link the same source build; name/ABI equality alone is insufficient.
        var files = Directory.EnumerateFiles(media).ToArray();
        foreach (var source in files)
        {
            var target = Path.Combine(native, Path.GetFileName(source));
            if (File.Exists(target) && RuntimeArchive.HashFile(source) != RuntimeArchive.HashFile(target))
                throw new InvalidDataException("Shared native component differs between consumers: " + Path.GetFileName(source));
        }
        foreach (var source in files)
        {
            var target = Path.Combine(native, Path.GetFileName(source));
            if (File.Exists(target)) File.Delete(source);
            else File.Move(source, target);
        }
        Directory.Delete(media);
        Console.WriteLine("  shared source build: GUI bridge and author use one DLL set in menu-bin");
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
        string? mediaRuntime = null;
        var includeCli = false;
        var frameworkDependent = true;
        var oneFile = true;
        var releaseVersion = "v1.0";
        for (var index = 0; index < args.Length; index++)
        {
            var value = args[index];
            if (value.Equals("package", StringComparison.OrdinalIgnoreCase)) continue;
            if (value == "--version")
            {
                if (++index >= args.Length || !System.Text.RegularExpressions.Regex.IsMatch(args[index], @"\Av[0-9]+\.[0-9]+(?:\.[0-9]+)?(?:-[A-Za-z0-9]+(?:[.-][A-Za-z0-9]+)*)?\z"))
                    throw new ArgumentException("--version requires a release version such as v1.0 or v1.1.0.");
                releaseVersion = args[index];
                continue;
            }
            if (value == "--include-cli") { includeCli = true; continue; }
            if (value == "--framework-dependent") { frameworkDependent = true; continue; }
            if (value == "--self-contained") { frameworkDependent = false; continue; }
            if (value == "--onefile") { oneFile = true; continue; }
            if (value == "--directory") { oneFile = false; continue; }
            if (value is "--repo" or "--source" or "--prebuilt" or "--output" or "--magick-shim" or "--ffmpeg-libraries" or "--image-author" or "--media-runtime")
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
                    case "--media-runtime": mediaRuntime = args[index]; break;
                }
                continue;
            }
            throw new ArgumentException($"Unknown argument: {value}");
        }
        if (oneFile && (includeCli || !frameworkDependent))
            throw new ArgumentException("The compact onefile release is GUI-only and framework-dependent. Use --directory for --include-cli or --self-contained.");
        return new ToolchainOptions(repository, source, prebuilt, output, includeCli, frameworkDependent, magickShim, ffmpegLibraries, imageAuthor, oneFile, mediaRuntime, releaseVersion);
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
            ["README.ja.md"] = [Path.Combine(repository, "tools", "win-build", "docs", "README.ja.md")],
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

    private static void WriteRuntimeRequirements(string destination, bool frameworkDependent, bool oneFile)
    {
        var chinese = frameworkDependent
            ? "本精简包不包含 .NET 运行时。**首次运行前，请安装 .NET 10 Desktop Runtime（Windows x64）**。打开 [微软官方下载页](https://dotnet.microsoft.com/download/dotnet/10.0)，在 .NET Desktop Runtime 栏选择 Windows x64 安装程序。只有普通 .NET Runtime、ASP.NET Core Runtime 或 .NET Framework 4.x 不够。已安装兼容的 Microsoft.WindowsDesktop.App 10.0.x 则无需重复安装。"
            : "本包自带 Windows x64 .NET 运行时，无需单独安装 .NET。";
        var english = frameworkDependent
            ? "This compact package excludes the .NET runtime. **Before first use, install .NET 10 Desktop Runtime for Windows x64.** On the [official Microsoft download page](https://dotnet.microsoft.com/download/dotnet/10.0), choose the Windows x64 installer under .NET Desktop Runtime. The plain .NET Runtime, ASP.NET Core Runtime or .NET Framework 4.x alone is insufficient. An existing compatible Microsoft.WindowsDesktop.App 10.0.x installation can be reused."
            : "This package includes its Windows x64 .NET runtime; no separate .NET installation is needed.";
        var japanese = frameworkDependent
            ? "この配布パッケージに .NET は含まれません。**初回起動前に .NET 10 Desktop Runtime（Windows x64）をインストールしてください。** [Microsoft 公式ダウンロードページ](https://dotnet.microsoft.com/download/dotnet/10.0) の .NET Desktop Runtime から Windows x64 を選びます。通常の .NET Runtime、ASP.NET Core Runtime、.NET Framework 4.x では代用できません。対応する Microsoft.WindowsDesktop.App 10.0.x があれば再インストールは不要です。"
            : "このパッケージには Windows x64 用 .NET ランタイムが含まれます。別途インストールは不要です。";
        var layoutJa = oneFile ? "DVD-Audio-Maker.exe を実行してください。必要なコンポーネントを初回に %LOCALAPPDATA%/DVD-Audio-Maker/runtime に展開し、以後は検証して再利用します。説明書、設定例、ライセンスは EXE と同じ ZIP に含まれます。ライセンス表記は保管してください。"
            : "すべてのフォルダーとコンポーネントを含めて展開してください。";
        File.WriteAllText(Path.Combine(destination, "RUNTIME.ja.md"),
            "# 実行環境\n\n" + japanese + "\n\n" + layoutJa + " FFmpeg、FFprobe、ImageMagick の別途インストールは不要です。\n", new UTF8Encoding(false));
        var layoutZh = oneFile ? "运行 DVD-Audio-Maker.exe。EXE 只内嵌必需运行组件，首次自动释放到 %LOCALAPPDATA%/DVD-Audio-Maker/runtime；以后复用并校验缓存。文档、配置示例和许可随 ZIP 单独提供，请保留随包授权声明。"
            : "请完整解压整个目录，保留所有组件子目录。";
        var layoutEn = oneFile ? "Run DVD-Audio-Maker.exe. Only required runtime components are embedded and automatically extracted into %LOCALAPPDATA%/DVD-Audio-Maker/runtime, then verified/reused. Documentation, example configuration and licenses accompany the EXE in the ZIP; retain the license notices."
            : "Extract the complete directory and retain all component subdirectories.";
        File.WriteAllText(Path.Combine(destination, "RUNTIME.md"),
            "# 运行要求\n\n" + chinese + "\n\n" + layoutZh + "媒体与图像处理组件已内置，无需安装 FFmpeg、FFprobe 或 ImageMagick。\n", new UTF8Encoding(false));
        File.WriteAllText(Path.Combine(destination, "RUNTIME.en.md"),
            "# Runtime requirements\n\n" + english + "\n\n" + layoutEn + " Media and image processing are bundled; no FFmpeg, FFprobe or ImageMagick installation is required.\n", new UTF8Encoding(false));
        foreach (var (name, notice) in new[]
        {
            ("README.md", "> **运行环境：** " + chinese),
            ("README.en.md", "> **Runtime requirement:** " + english),
            ("README.ja.md", "> **実行環境：** " + japanese),
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
        "Usage: build-all.cmd [--onefile | --directory] [--version <v1.0>] [--source <dvda-author tree>] [--prebuilt <menu-bin>] [--output <directory>] [--include-cli] [--framework-dependent | --self-contained] [--ffmpeg-libraries <verified MLP DLL directory>] [--image-author <rebuilt native author directory>] [--media-runtime <verified shared media directory>]");

    private sealed record ToolchainOptions(
        string? Repository,
        string? SourceTree,
        string? PrebuiltDirectory,
        string? OutputDirectory,
        bool IncludeCli,
        bool FrameworkDependent,
        string? MagickShim,
        string? FfmpegLibraries,
        string? ImageAuthor,
        bool OneFile,
        string? MediaRuntime,
        string ReleaseVersion);
}
