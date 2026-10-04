using DvdaMaker.Building;
using DvdaMaker.SurcodeTool;
using DvdaMaker.Configuration;
using DvdaMaker.FontTool;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Formats.Mpeg;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

Console.OutputEncoding = System.Text.Encoding.UTF8;
Console.InputEncoding = System.Text.Encoding.UTF8;

var fixtureProcessName = Path.GetFileNameWithoutExtension(Environment.ProcessPath ?? string.Empty);
if (fixtureProcessName.StartsWith("fake-dvda-author", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("author");
    if (fixtureProcessName.Contains("fail", StringComparison.OrdinalIgnoreCase))
    {
        Console.Error.WriteLine("fixture author failure");
        return 9;
    }
    var failDisc = Environment.GetEnvironmentVariable("DVDA_FIXTURE_FAIL_DISC");
    if (!string.IsNullOrEmpty(failDisc) &&
        args.Any(argument => argument.Contains(failDisc, StringComparison.OrdinalIgnoreCase)))
    {
        Console.Error.WriteLine($"fixture author failure for {failDisc}");
        return 9;
    }

    var outputIndex = Array.IndexOf(args, "-o");
    if (outputIndex < 0 || outputIndex + 1 >= args.Length)
    {
        Console.Error.WriteLine("missing -o output directory");
        return 8;
    }
    var audioTs = Path.Combine(args[outputIndex + 1], "AUDIO_TS");
    Directory.CreateDirectory(audioTs);
    File.WriteAllBytes(Path.Combine(audioTs, "AUDIO_TS.IFO"), [1, 2, 3]);
    var isoArgument = args.FirstOrDefault(argument =>
        argument.StartsWith("--iso=", StringComparison.OrdinalIgnoreCase) ||
        argument.StartsWith("--mkisofs=", StringComparison.OrdinalIgnoreCase));
    if (isoArgument is not null)
    {
        var separator = isoArgument.IndexOf('=');
        var isoPath = isoArgument[(separator + 1)..];
        Directory.CreateDirectory(Path.GetDirectoryName(isoPath)!);
        File.WriteAllBytes(isoPath, [0x49, 0x53, 0x4F, 0x21]);
        Console.WriteLine("fixture in-process ISO created");
    }
    Console.WriteLine("1  1/1  1  0  99  0  90000  0");
    return 0;
}
if (fixtureProcessName.StartsWith("fake-mkisofs", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("mkisofs");
    var outputIndex = Array.IndexOf(args, "-o");
    if (outputIndex < 0 || outputIndex + 1 >= args.Length)
    {
        Console.Error.WriteLine("missing -o ISO path");
        return 8;
    }
    Directory.CreateDirectory(Path.GetDirectoryName(args[outputIndex + 1])!);
    File.WriteAllBytes(args[outputIndex + 1], [0x49, 0x53, 0x4F, 0x21]);
    Console.WriteLine("fixture ISO created");
    return 0;
}
if (fixtureProcessName.StartsWith("fake-magick", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("magick");
    if (args.Length == 0 || args[0] == "identify" || !args.Contains("null:"))
    {
        Console.Error.WriteLine("expected a single batch command without identify");
        return 8;
    }
    for (var index = 0; index < args.Length - 1; index++)
    {
        if (args[index] != "-format") continue;
        var format = args[index + 1];
        var prefix = format.Split('%')[0];
        var value = prefix[0] switch
        {
            'F' => "0.5|0.1|1234",
            'B' => "75",
            'T' => "116",
            'L' => "240|80",
            'N' => "10",
            'H' => "20",
            'A' => "1",
            _ => "bad",
        };
        Console.WriteLine(prefix + value);
    }
    return 0;
}
if (fixtureProcessName.StartsWith("fake-ffmpeg-pcm", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("encode");
    var template = Environment.GetEnvironmentVariable("DVDA_MLP_FIXTURE_TEMPLATE");
    if (string.IsNullOrEmpty(template) || !File.Exists(template)) return 3;
    if (fixtureProcessName.Contains("fail")) { Console.Error.WriteLine("[error] fixture conversion failed"); return 7; }
    File.Copy(template, args[^1], overwrite: false);
    if (fixtureProcessName.Contains("slow")) Thread.Sleep(30000);
    return 0;
}
if (fixtureProcessName.StartsWith("fake-ffmpeg", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("ffmpeg");
    // DecodeValidator 从 stderr 读取 astats 的采样数；mismatch 变体用于制造采样数不吻合。
    var samples = fixtureProcessName.Contains("mismatch", StringComparison.OrdinalIgnoreCase)
        ? 100
        : 48_000;
    Console.Error.WriteLine($"Number of samples: {samples}");
    return 0;
}
if (fixtureProcessName.StartsWith("fake-ffprobe", StringComparison.OrdinalIgnoreCase))
{
    RecordFixtureCall("ffprobe");
    var source = args.LastOrDefault() ?? string.Empty;
    if (source.Contains("broken", StringComparison.OrdinalIgnoreCase))
    {
        Console.WriteLine("not-json");
        return 0;
    }
    if (args.Contains("-show_streams"))
    {
        var codec = source.Contains("aac", StringComparison.OrdinalIgnoreCase) ? "aac" : "alac";
        Console.WriteLine($"{{\"streams\":[{{\"codec_type\":\"audio\",\"codec_name\":\"{codec}\"}}]}}");
        return 0;
    }
    if (args.Contains("-show_format"))
    {
        Console.WriteLine("{\"format\":{\"tags\":{\"title\":\"Fixture\"}}}");
        return 0;
    }
    if (args.Any(argument => argument.StartsWith("packet=", StringComparison.Ordinal)))
    {
        Console.WriteLine("0.000000,0.021333,4100,64");
        return 0;
    }
    if (args.Contains("stream=sample_rate,channels,bits_per_raw_sample"))
    {
        // AudioParameterProbe 的应答（default=nw=1:nk=1）。
        Console.WriteLine("48000");
        Console.WriteLine("2");
        Console.WriteLine("24");
        return 0;
    }
    if (args.Contains("stream=sample_rate,bits_per_raw_sample,channels,duration_ts,time_base"))
    {
        // AudioMetadataReader 的探测应答（default=noprint_wrappers=1）。
        Console.WriteLine("sample_rate=48000");
        Console.WriteLine("bits_per_raw_sample=24");
        Console.WriteLine("channels=2");
        Console.WriteLine("duration_ts=48000");
        Console.WriteLine("time_base=1/48000");
        Console.WriteLine("TAG:title=Fixture Song");
        Console.WriteLine("TAG:album=Fixture Album");
        Console.WriteLine("TAG:track=1");
        Console.WriteLine("TAG:date=2026");
        return 0;
    }
    Console.WriteLine();
    return 0;
}

static void RecordFixtureCall(string tool)
{
    var directory = Environment.GetEnvironmentVariable("DVDA_FIXTURE_CALL_LOG_DIR");
    if (string.IsNullOrWhiteSpace(directory))
    {
        return;
    }
    for (var attempt = 0; attempt < 10; attempt++)
    {
        try
        {
            Directory.CreateDirectory(directory);
            File.AppendAllText(
                Path.Combine(directory, $"fixture-calls-{tool}.txt"),
                "call" + Environment.NewLine);
            return;
        }
        catch (IOException)
        {
            // 并发夹具可能同时写同一个计数文件，短暂重试即可。
            Thread.Sleep(20);
        }
    }
}

if (args.Length > 0 && args[0] == "--process-fixture")
{
    Console.WriteLine(string.Join('|', args.Skip(1)));
    Console.Error.WriteLine("fixture-error");
    return 0;
}
if (args.Length > 0 && args[0] == "--process-fixture-fail")
{
    Console.Error.WriteLine("fixture-failure");
    return 7;
}

if (args.Length == 1 && args[0] == "--oversized-au-integration")
{
    MlpEncoderTests.OversizedAccessUnitIntegration();
    return 0;
}

if (args.Length == 1 && args[0] == "--mlpencoder-integration")
{
    MlpEncoderTests.RealIntegration();
    return 0;
}

if (args.Length == 2 && args[0] == "--mlpencoder-batch-integration")
{
    MlpEncoderTests.RealBatchIntegration(args[1]);
    return 0;
}

if (args.Length == 3 && args[0] == "--ffmpeg-original-corpus")
{
    FfmpegPcmTests.OriginalCorpus(args[1], args[2]);
    return 0;
}

if (args.Length == 2 && args[0] == "--ffmpeg-pcm-integration")
{
    FfmpegPcmTests.Integration(args[1]);
    return 0;
}

if (args.Length == 2 && args[0] == "--builtin-pcm-integration")
{
    FfmpegPcmTests.Integration(args[1], builtin: true);
    return 0;
}

if (args.Length == 2 && args[0] == "--builtin-media-integration")
{
    await BuiltinMediaTests.Integration(args[1]);
    return 0;
}

if (args.Length == 2 && args[0] == "--builtin-images-integration")
{
    await BuiltinImagesTests.Integration(args[1]);
    return 0;
}

if (args.Length > 0)
{
    if (args.Length == 3 && args[0] == "--real-fixtures")
    {
        try
        {
            RealFixtureBaseline.Run(args[1], args[2]);
            Console.WriteLine("真实样本对拍: 3/3 通过");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"[FAIL] 真实样本对拍: {exception.Message}");
            return 1;
        }
    }

    Console.Error.WriteLine(
        "用法: DvdaMaker.CompatibilityTests [--real-fixtures <ISO目录> <MLP根目录> | --mlpencoder-integration | --oversized-au-integration | --mlpencoder-batch-integration <ffmpeg.exe> | --ffmpeg-pcm-integration <output目录> | --ffmpeg-original-corpus <原版矩阵.json> <output目录>]");
    return 2;
}

var tests = new (string Name, Action Run)[]
{
    ("Onefile deduplication, concurrency and cache repair", RuntimeArchiveTests.RoundtripAndRepair),
    ("Onefile rejects unsafe component paths", RuntimeArchiveTests.RejectUnsafePaths),
    ("FLAC metadata in-process read/write", FlacMetadataTests.InProcessEditor),
    ("English catalog template coverage", LocalizationTests.CatalogCoverage),
    ("Japanese catalog, paths and nested diagnostics", LocalizationTests.JapaneseCoverage),
    ("LPCM format limits and channel grouping", LpcmTests.FormatAndGrouping),
    ("Language preserves paths and numeric culture", LocalizationTests.OpaqueValuesAndCulture),
    ("Language profile and argument compatibility", LocalizationTests.ProfileAndArguments),
    ("解析引号、注释和无效行", ParseAssignments),
    ("Windows 路径规范化", NormalizeWindowsPaths),
    ("环境变量优先于 config.env", EnvironmentWins),
    ("空环境变量不覆盖配置", EmptyEnvironmentDoesNotOverride),
    ("默认值与数值回退", DefaultsAndNumericFallbacks),
    ("派生路径和 ISO 名称", DerivedPathsAndNames),
    ("限制组轨数量", ClampGroupTrackLimit),
    ("规范化 MLP 来源", NormalizeMlpSource),
    ("GUI 配置方案保存与版本校验", GuiConfigurationTests.ProfileRoundtrip),
    ("GUI 导入旧 env 与未知键保留", GuiConfigurationTests.ImportEnv),
    ("GUI 显式设置隔离环境变量", GuiConfigurationTests.ExplicitValuesWin),
    ("GUI 摘要保留工具错误与告警", GuiPresentationTests.ImportantDiagnosticsSurvive),
    ("GUI 完整行与并发日志捕获", GuiPresentationTests.CompleteLineCapture),
    ("GUI 友好选项保留配置原值", GuiPresentationTests.ChoiceValuesRemainStable),
    ("进程内 DLL 取消与失败保护", MlpEncoderTests.DllCancellation),
    ("FFmpeg 参数保留声道与显式目标精度", FfmpegPcmTests.ConversionContract),
    ("FFmpeg 失败取消与 PATH 配置", FfmpegPcmTests.FailureAndCancellation),
    ("旧 eac3to 配置迁移与 FFmpeg 工具设置", FfmpegPcmTests.LegacySettings),
    ("超大 AU 无损回退与大小上限", MlpEncoderTests.OversizedAccessUnit),
    ("eac3to 奇数 PCM 尾部封装", MlpEncoderTests.OddPcmTail),
    ("SurCode 内置任务与路径", BuildSurcodeBatchJob),
    ("MLP 编码核心确定性元数据", MlpEncoderTests.Metadata),
    ("SurCode PCM 16 位升至 24 位", UpconvertSurcodePcmWav),
    ("Shell 单引号转义", EscapeShellAssignment),
    ("Shell 默认键集兼容 Python", PreserveLegacyShellKeySet),
    ("配置来源与有效键集合", DescribeConfigurationSources),
    ("ISO9660 版本后缀处理", StripIsoVersion),
    ("PTS 五字节解析", ParsePts),
    ("MLP 峰值码率取整", CalculatePeakBitrate),
    ("MLP access unit 遍历", WalkMlpAccessUnits),
    ("MLP CRC 与奇偶校验基线", MlpChecksumBaseline),
    ("MLP 损坏边界与对齐修复", ValidateMlpDamageFixtures),
    ("MLP 内存与流式检查结果一致", MlpStreamingInspection),
    ("命令行日志格式化", FormatCommandLine),
    ("外部进程参数与输出捕获", RunExternalProcess),
    ("外部进程非零退出码", HandleNonZeroExitCode),
    ("专辑多数参数归一化", NormalizeAlbumParameters),
    ("曲序和安全文件名", PrepareNamingHelpers),
    ("ffmpeg 解码错误扫描", ScanDecodeErrors),
    ("构建曲目全局排序", SortBuildTracks),
    ("按专辑贪心分盘", SplitAlbumsAcrossDiscs),
    ("专辑不跨盘", KeepAlbumOnOneDisc),
    ("按参数与专辑边界分组", GroupAtAlbumBoundaries),
    ("组轨超限诊断", DiagnoseOversizedAlbumGroup),
    ("MLP 编码核心内嵌产物身份", MlpEncoderTests.EmbeddedCore),
    ("外部 MLP 镜像路径优先", ResolveExternalMlp),
    ("MLP 索引结构", WriteMlpIndex),
    ("构建索引预演与失败隔离", PreserveFormalMlpIndex),
    ("dvda-author 参数与 title 边界", BuildDvdaAuthorArguments),
    ("诊断 title 划分模式", BuildDiagnosticTitleModes),
    ("诊断专辑数量限制", LimitDiagnosticAlbums),
    ("内置 ISO 写入参数构造", BuildInProcessIsoArguments),
    ("构建日志命令格式", WriteCompatibleBuildLog),
    ("ISO 发布长度校验", PublishIso),
    ("同卷 ISO 暂存移动与冲突保护", StageIsoWithoutCopy),
    ("多盘与索引事务发布回滚", PublishDiscSetTransactionally),
    ("多盘同卷移动发布与失败回滚", PublishMovedDiscSetTransactionally),
    ("ALAC magic cookie 解析", ParseAlacMagicCookie),
    ("ALAC END 标记修补", PatchAlacEndMarker),
    ("ALAC extra bits 行为与 Python 一致", PatchAlacWithExtraBits),
    ("M4A 标签保留原始值", PreserveM4aTagValues),
    ("M4A dry-run 不写文件", M4aDryRunDoesNotWrite),
    ("M4A 拒绝非 ALAC 音频", RejectNonAlacM4a),
    ("M4A 单文件异常不终止批次", IsolateM4aFileFailures),
    ("M4A PICTURE 描述解析", ParseFlacPictureDescriptor),
    ("M4A 原文件删除失败不中断批次", PreserveConversionOnDeleteFailure),
    ("时间轴覆盖后续 AOB 分段", AnalyzeAllAobSegments),
    ("AOB PTS 损坏边界检测", ValidateAobPtsDamageFixtures),
    ("审计解析多行 ANSI 轨道表", ParseAnsiAuditLog),
    ("审计只解析最后一次正式构建", ParseLatestFormalAuditLog),
    ("校验只选择当前编号 ISO", SelectCurrentIsoNames),
    ("审计按时间选择最新日志", SelectNewestAuditLog),
    ("外部 MLP 重名歧义", DetectExternalMlpAmbiguity),
    ("MLP 索引拒绝重复路径", RejectDuplicateMlpIndexKeys),
    ("正式出盘执行器端到端", BuildDiscEndToEnd),
    ("事务暂存移动与保留中间 ISO", StageDiscAndKeepIntermediate),
    ("出盘失败保留诊断现场", PreserveFailedDiscWorkspace),
    ("菜单配置派生值", LoadMenuConfiguration),
    ("菜单按专辑分页与索引", PlanAlbumMenuPages),
    ("菜单文字净化与截断", SanitizeMenuText),
    ("菜单 author 参数结构", BuildMenuAuthorArguments),
    ("菜单字体脚本识别与区域 face", ResolveMenuFontRules),
    ("纯 C# TTC face 提取与校验", ExtractOpenTypeCollectionFaces),
    ("原生依赖普通与延迟导入及保守清理", NativeOptimizerTests.Run),
    ("菜单视觉阈值与 Python 一致", VerifyMenuVisualThresholds),
    ("菜单批量统计解析与缺失回退", ParseBatchedMenuStatistics),
    ("菜单批量统计单页单进程", BatchMenuProcessCalls),
    ("菜单多索引页格数与 cell 范围", VerifyMultiIndexPageLayout),
    ("AMG 菜单 cell 链损坏检测", ValidateAmgCellChainFixtures),
    ("ASVS 静图表损坏检测", ValidateAsvsFixtures),
    ("build.cmd 参数分支依次准备并出盘", BuildScriptBranching),
    ("build.cmd prepare 失败不进入出盘", BuildScriptStopsAfterPrepareFailure),
    ("PCM 严格比较与 SurCode 有界零尾", ComparePcmFixtures),
    ("工作盘空间预检估算", EstimateDiskSpaceRequirements),
    ("准备缓存复用与源变化失效", PrepareCacheReuseAndInvalidation),
    ("准备缓存不记录未通过校验的轨道", PrepareCacheSkipsFailedTracks),
    ("准备缓存身份与存储规则", PrepareCacheIdentityAndStorage),
    ("准备快照指纹复用与失效", PreparationSnapshotChecks),
    ("审计 PTS 流式扇区扫描", ScanPtsSectorsStreamingly),
    ("共享 AOB 扫描保持审计与时间轴语义", SharedAobScanPreservesDiagnostics),
    ("SurCode 新产物校验凭据失效回退", ReuseValidatedSurcodeOutput),
    ("MLP 编码核心 MLP 缓存凭据复用", MlpEncoderTests.Cache),
    ("MLP 缓存索引存储与淘汰", MlpCacheIndexStorage),
    ("逐盘续跑凭据规则", DiscResumeStoreRules),
    ("出盘签名随配置与 MLP 变化", DiscSignatureChanges),
    ("构建流水线逐盘续跑", BuildPipelineResumesDiscs),
    ("MLP 有界并发编码等效性", MlpEncoderTests.Parallel),
};

var failed = 0;
foreach (var test in tests)
{
    try
    {
        test.Run();
        Console.WriteLine($"[PASS] {test.Name}");
    }
    catch (Exception exception)
    {
        failed++;
        Console.Error.WriteLine($"[FAIL] {test.Name}: {exception.Message}");
    }
}

Console.WriteLine($"兼容性测试: {tests.Length - failed}/{tests.Length} 通过");
return failed == 0 ? 0 : 1;

static void ParseAssignments()
{
    WithConfig(
        """
        # comment
        DVDA_SRC="/music/a # b"
        DVDA_TITLE=Plain title # trailing comment
        DVDA_FINAL_DIR='/output'
        export INVALID=value
        """,
        path =>
        {
            var values = ConfigLoader.ParseFile(path);
            Equal("/music/a # b", values["DVDA_SRC"]);
            Equal("Plain title", values["DVDA_TITLE"]);
            Equal("/output", values["DVDA_FINAL_DIR"]);
            False(values.ContainsKey("INVALID"), "不应执行或接受 export 语句");
        });
}

static void NormalizeWindowsPaths()
{
    WithConfig("DVDA_SRC=\"D:\\Music\\Albums\"\nDVDA_TITLE=Keep\\Backslash", path =>
    {
        var values = ConfigLoader.ParseFile(path);
        Equal("D:/Music/Albums", values["DVDA_SRC"]);
        Equal("Keep/Backslash", values["DVDA_TITLE"]);
    });
}

static void EnvironmentWins()
{
    WithConfig("DVDA_SRC=/from-file\nDVDA_FINAL_DIR=/out", path =>
    {
        var options = Load(path, new() { ["DVDA_SRC"] = "/from-env" });
        Equal("/from-env", options.SourceDirectory);
    });
}

static void EmptyEnvironmentDoesNotOverride()
{
    WithConfig("DVDA_SRC=/from-file\nDVDA_FINAL_DIR=/out", path =>
    {
        var options = Load(path, new() { ["DVDA_SRC"] = "" });
        Equal("/from-file", options.SourceDirectory);
    });
}

static void DefaultsAndNumericFallbacks()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_DISC_BYTES=bad", path =>
    {
        var options = Load(path);
        Equal(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "DVD-Audio-Maker", "build"), options.BuildDirectory);
        Equal(ConfigDefaults.Dvd5Bytes, options.DiscBytes);
        Equal(2, options.MaxDiscs);
    });
}

static void DerivedPathsAndNames()
{
    WithConfig(
        "DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_BUILD_DIR=/work/\nDVDA_TITLE=鸣潮 DVD-Audio 2026",
        path =>
        {
            var options = Load(path);
            Equal("DVD_Audio_2026", options.IsoPrefix);
            Equal(Path.Combine("/work", "manifest.json"), options.ManifestPath);
            Equal("DVD_Audio_2026_2.iso", options.IsoName(2));
            Equal("鸣潮 DVD-Audio 2026 2", options.VolumeId(2));
        });
}

static void ClampGroupTrackLimit()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_GROUP_TRACK_LIMIT=500", path =>
        Equal(99, Load(path).GroupTrackLimit));
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_GROUP_TRACK_LIMIT=0", path =>
        Equal(1, Load(path).GroupTrackLimit));
}

static void NormalizeMlpSource()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_MLP_SOURCE=EXTERNAL", path =>
        Equal("external", Load(path).MlpSource));
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_MLP_SOURCE=SURCODE", path =>
        Equal("external", Load(path).MlpSource));
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_MLP_SOURCE=SURCODE-BATCH", path =>
        Equal("surcode-batch", Load(path).MlpSource));
    WithConfig("DVDA_MLP_SOURCE=LPCM", path => Equal("lpcm", Load(path).MlpSource));
    WithConfig("DVDA_MLP_SOURCE=ffmpeg", RejectRemovedSource);
    WithConfig("DVDA_MLP_SOURCE=batch-surcode", path => Equal("surcode-batch", Load(path).MlpSource));
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_MLP_SOURCE=unknown", path =>
        RejectRemovedSource(path));
}

static void BuildSurcodeBatchJob()
{
    WithConfig(
        "DVDA_SRC=C:/Music\nDVDA_FINAL_DIR=C:/Out\n" +
        "DVDA_MLP_SOURCE=surcode-batch\nDVDA_MLP_EXTERNAL_DIR=C:/MLP\n" +
        "DVDA_MLP_BATCH_TEMP_DIR=C:/ConfiguredTemp\n" +
        "DVDA_MLP_BATCH_OUTPUT_DIR=C:/ConfiguredOutput\n" +
        "DVDA_MLP_METADATA_CONTEXT=C:/Contexts/job.stampctx\n" +
        "DVDA_FFMPEG=C:/FFmpeg/ffmpeg.exe\n",
        path =>
        {
            var options = Load(path);
            Equal("C:/ConfiguredTemp", options.MlpBatchTempDirectory);
            Equal("C:/ConfiguredOutput", options.MlpBatchOutputDirectory);
            var provider = new SurcodeMlpProvider(options, new ProcessRunner());
            var first = new BuildTrack
            {
                Date = "2026", Track = "1", Title = "One", Album = "Album",
                SampleRate = 48_000, Bits = 24,
                SourcePath = "C:/Music/Album/01.flac", ManifestName = "01.flac",
                Duration = 60, SourceSize = 1, MlpPath = "", MlpSize = 0,
                SourceSampleRate = 44_100, SourceBits = 16,
            };
            var second = first with
            {
                Track = "2", Title = "Two", SourcePath = "C:/Music/Album/02.flac",
                ManifestName = "02.flac",
            };
            var job = provider.BuildJob("C:/Temp", "C:/Stage",
            [
                new SurcodeMlpProvider.PendingTrack(first, "C:/MLP/Album/01.mlp"),
                new SurcodeMlpProvider.PendingTrack(second, "C:/MLP/Album/02.mlp"),
            ]);
            Equal("C:\\Temp", job.TemporaryDirectory);
            Equal("C:\\Stage", job.OutputDirectory);
            Equal("C:\\Contexts\\job.stampctx", job.MetadataContext);
            Equal("C:/FFmpeg/ffmpeg.exe", job.FfmpegExecutable);
            Equal(48_000, job.SampleRate);
            Equal(24, job.Bits);
            Equal(2, job.Tracks.Count);
            Equal("__surcode_0001", job.Tracks[0].WorkName);
            Equal("__surcode_0002", job.Tracks[1].WorkName);
            Equal(44_100, job.Tracks[0].SourceSampleRate);
            Equal(16, job.Tracks[0].SourceBits);
            Equal(
                Path.Combine("C:/MLP", "Album", "01.mlp"),
                SurcodeMlpProvider.DestinationPath(
                    Path.GetFullPath("C:/Music/Album/01.flac"),
                    Path.GetFullPath("C:/Music"),
                    "C:/MLP"));
        });
}


static void UpconvertSurcodePcmWav()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-surcode-wav", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        const string name = "__surcode_0001";
        var path = Path.Combine(root, name + ".L.wav");
        using (var writer = new BinaryWriter(File.Create(path)))
        {
            writer.Write(System.Text.Encoding.ASCII.GetBytes("RIFF"));
            writer.Write(42);
            writer.Write(System.Text.Encoding.ASCII.GetBytes("WAVEfmt "));
            writer.Write(16);
            writer.Write((short)1);
            writer.Write((short)1);
            writer.Write(48_000);
            writer.Write(96_000);
            writer.Write((short)2);
            writer.Write((short)16);
            writer.Write(System.Text.Encoding.ASCII.GetBytes("data"));
            writer.Write(6);
            writer.Write(short.MinValue);
            writer.Write((short)0);
            writer.Write(short.MaxValue);
        }

        var converted = Path.Combine(root, "normalized.wav");
        SurcodePcmWav.Normalize(path, converted, 48_000, 24);
        path = converted;
        var layout = SurcodePcmWav.ReadLayout(path);
        Equal(24, layout.ContainerBits);
        Equal(24, layout.ValidBits);
        Equal(3, layout.BytesPerSample);
        Equal(9L, layout.DataSize);

        using var stream = File.OpenRead(path);
        stream.Position = layout.DataOffset;
        var samples = new byte[9];
        stream.ReadExactly(samples);
        SequenceEqual(new byte[]
        {
            0x00, 0x00, 0x80,
            0x00, 0x00, 0x00,
            0x00, 0xFF, 0x7F,
        }, samples);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void EscapeShellAssignment() =>
    Equal("KEY='a'\\''b'", ShellFormatter.Assignment("KEY", "a'b"));

static void StripIsoVersion()
{
    Equal("ATS_01_0.IFO", Iso9660Reader.StripVersion("ATS_01_0.IFO;1"));
    Equal("ATSI", Iso9660Reader.StripVersion("ATSI;12"));
    Equal("NAME;X", Iso9660Reader.StripVersion("NAME;X"));
}

static void ParsePts()
{
    foreach (var expected in new long[] { 0, 1, 90_000, 0x1FFFFFFFFL })
    {
        var encoded = EncodePts(expected);
        Equal(expected, PesTimestampParser.ParsePts(encoded));
    }
}

static void CalculatePeakBitrate()
{
    Equal(3200, MlpStreamAligner.PeakBitrateRaw(48_000));
    Equal(3483, MlpStreamAligner.PeakBitrateRaw(44_100));
}

static void WalkMlpAccessUnits()
{
    var data = new byte[12];
    data[0] = 0xF0;
    data[1] = 0x02;
    data[4] = 0x00;
    data[5] = 0x04;
    var units = MlpStreamAligner.Walk(data);
    Equal(2, units.Count);
    Equal(4, units[0].Length);
    Equal(8, units[1].Length);
    False(units[0].HasMajorSync, "首个测试 AU 不应含 major sync");
}

static void MlpChecksumBaseline()
{
    byte[] body = [0x10, 0x20, 0x30, 0x40];
    Equal((byte)0x40, MlpStreamAligner.CalculateParity(body));

    byte[] checksum8Input = [0x01, 0x02, 0x03, 0x00];
    Equal((byte)0xB0, MlpStreamAligner.Checksum8(checksum8Input, checksum8Input.Length));

    byte[] checksum16Input = [0xF8, 0x72, 0x6F, 0xBB, 0x00, 0x00];
    Equal((ushort)0x8699, MlpStreamAligner.Checksum16(checksum16Input, checksum16Input.Length));
}

static void ValidateMlpDamageFixtures()
{
    static byte[] AccessUnit(bool includeMajorSync = true, bool includeEndOfStream = true)
    {
        var majorSize = includeMajorSync ? MlpStreamAligner.MajorSyncSize : 0;
        var body = includeEndOfStream
            ? new byte[] { 0xD2, 0x34, 0xD2, 0x34 }
            : new byte[] { 0x10, 0x20, 0x30, 0x40 };
        var substreamLength = body.Length + 2;
        var length = 4 + majorSize + 2 + substreamLength;
        var data = new byte[length];

        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
            data.AsSpan(2, 2), 0);
        if (includeMajorSync)
        {
            var major = data.AsSpan(4, MlpStreamAligner.MajorSyncSize);
            major[0] = 0xF8;
            major[1] = 0x72;
            major[2] = 0x6F;
            major[5] = 0x00;
            System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(major[8..10], 0xB752);
            System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
                major[14..16], (ushort)MlpStreamAligner.PeakBitrateRaw(48_000));
            major[16] = 1;
            var checksum = MlpStreamAligner.Checksum16(
                major, MlpStreamAligner.MajorSyncSize - 2);
            System.Buffers.Binary.BinaryPrimitives.WriteUInt16LittleEndian(major[26..28], checksum);
        }

        var substreamOffset = 4 + majorSize;
        var substreamHeader = (ushort)(substreamLength / 2);
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
            data.AsSpan(substreamOffset, 2), substreamHeader);
        body.CopyTo(data.AsSpan(substreamOffset + 2));

        var lengthWords = length / 2;
        var parity = lengthWords ^ (substreamHeader >> 8) ^ (substreamHeader & 0xFF);
        parity ^= parity >> 8;
        parity ^= parity >> 4;
        parity &= 0xF;
        var accessUnitHeader = (ushort)(((parity ^ 0xF) << 12) | lengthWords);
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
            data.AsSpan(0, 2), accessUnitHeader);
        return data;
    }

    var valid = AccessUnit();
    var validInspection = MlpStreamAligner.Inspect(valid);
    True(validInspection.IsValid, "完整 major sync、正确字段和 EOS 的 MLP 应通过检查");

    var noMajorSync = AccessUnit(includeMajorSync: false);
    var noMajorInspection = MlpStreamAligner.Inspect(noMajorSync);
    Equal(0, noMajorInspection.MajorSyncCount);
    False(noMajorInspection.IsValid, "没有 major sync 的 MLP 不得通过检查");

    var truncatedMajor = new byte[12];
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
        truncatedMajor.AsSpan(0, 2), 6);
    truncatedMajor[4] = 0xF8;
    truncatedMajor[5] = 0x72;
    truncatedMajor[6] = 0x6F;
    var truncatedInspection = MlpStreamAligner.Inspect(truncatedMajor);
    True(truncatedInspection.MajorSyncErrors.Any(error => error.Message == "截断"),
        "被截断的 major sync 必须被报告");

    var badFields = valid.ToArray();
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
        badFields.AsSpan(18, 2), 1);
    badFields[20] = 2;
    var badInspection = MlpStreamAligner.Inspect(badFields);
    True(badInspection.MajorSyncErrors.Any(error => error.Message.StartsWith("peak=", StringComparison.Ordinal)),
        "错误 peak bitrate 必须被检测到");
    True(badInspection.MajorSyncErrors.Any(error => error.Message.StartsWith("extended_substream_info=", StringComparison.Ordinal)),
        "错误 extended_substream_info 必须被检测到");
    True(badInspection.MajorSyncErrors.Any(error => error.Message == "checksum16 不符"),
        "修改 major sync 字段后 checksum 损坏必须被检测到");

    var missingEos = AccessUnit(includeEndOfStream: false);
    False(MlpStreamAligner.Inspect(missingEos).HasEndOfStream,
        "缺少 EOS 的 MLP 必须被识别");
    var aligned = MlpStreamAligner.Align(missingEos);
    True(aligned.Changes.AddedEndOfStream, "对齐器应补写 EOS");
    Equal(missingEos.Length + 4, aligned.Data.Length);
    True(MlpStreamAligner.Inspect(aligned.Data).IsValid,
        "补写 EOS 并重算 AU 头后应通过完整检查");

    var malformedLength = new byte[] { 0x00, 0x03, 0, 0 };
    var rejected = false;
    try
    {
        MlpStreamAligner.Walk(malformedLength);
    }
    catch (InvalidDataException)
    {
        rejected = true;
    }
    True(rejected, "越过文件结尾的 access unit 长度必须被拒绝");
}

static void FormatCommandLine()
{
    Equal("ffmpeg -i \"path with spaces.flac\"", CommandLineFormatter.Format(
        "ffmpeg", ["-i", "path with spaces.flac"]));
}

static void MlpStreamingInspection()
{
    var data = BuildValidMlpFixture();
    var memory = MlpStreamAligner.Inspect(data);
    using var stream = new MemoryStream(data, writable: false);
    var streamed = MlpStreamAligner.Inspect(stream);
    Equal(memory.Size, streamed.Size);
    Equal(memory.AccessUnitCount, streamed.AccessUnitCount);
    Equal(memory.MajorSyncCount, streamed.MajorSyncCount);
    Equal(memory.HasEndOfStream, streamed.HasEndOfStream);
    Equal(memory.SampleRate, streamed.SampleRate);
    Equal(memory.PeakBitrateRaw, streamed.PeakBitrateRaw);
    True(streamed.IsValid, "流式 MLP 检查必须通过有效样本");

    using var asyncStream = new MemoryStream(data, writable: false);
    var asynchronous = MlpStreamAligner.InspectAsync(asyncStream)
        .GetAwaiter().GetResult();
    Equal(memory.Size, asynchronous.Size);
    Equal(memory.AccessUnitCount, asynchronous.AccessUnitCount);
    Equal(memory.MajorSyncCount, asynchronous.MajorSyncCount);
    Equal(memory.HasEndOfStream, asynchronous.HasEndOfStream);
    True(asynchronous.IsValid, "异步流式 MLP 检查必须通过有效样本");
}

static void RunExternalProcess()
{
    var executable = Environment.ProcessPath ??
        throw new InvalidOperationException("无法定位当前测试进程。");
    var result = new ProcessRunner().RunAsync(new ProcessRequest
    {
        FileName = executable,
        Arguments = ["--process-fixture", "value with spaces", "韩文-한글"],
    }).GetAwaiter().GetResult();

    Equal(0, result.ExitCode);
    Equal("value with spaces|韩文-한글", result.StandardOutput.Trim());
    Equal("fixture-error", result.StandardError.Trim());
}

static void HandleNonZeroExitCode()
{
    var executable = Environment.ProcessPath ??
        throw new InvalidOperationException("无法定位当前测试进程。");
    var result = new ProcessRunner().RunAsync(new ProcessRequest
    {
        FileName = executable,
        Arguments = ["--process-fixture-fail"],
    }).GetAwaiter().GetResult();
    Equal(7, result.ExitCode);
}

static void NormalizeAlbumParameters()
{
    var tracks = new List<AudioTrackMetadata>
    {
        Track("a.flac", 48_000, 24),
        Track("b.flac", 48_000, 24),
        Track("c.flac", 44_100, 16),
    };
    AlbumNormalizer.Apply(tracks);
    Equal(48_000, tracks[2].SampleRate);
    Equal(24, tracks[2].Bits);
    Equal(48_000, tracks[2].ResampleTo);
    Equal(44_100, tracks[2].SourceSampleRate);
    Equal(16, tracks[2].SourceBits);
}

static void PrepareNamingHelpers()
{
    Equal(12, AlbumNormalizer.TrackNumber("12/20"));
    Equal(9999, AlbumNormalizer.TrackNumber("disc one"));
    Equal("01_ A_B_C", AlbumNormalizer.SafeBaseName("C:/album/01: A?B*C.flac"));
}

static void ScanDecodeErrors()
{
    var result = DecodeValidator.ScanErrors(
        "ok\nError submitting packet to decoder\nCRC mismatch\ncorrupt frame\n");
    Equal(3, result.Count);
    Equal(3, result.Lines.Count);
}

static void SortBuildTracks()
{
    var tracks = new[]
    {
        BuildTrack("B", "2026-02-01", "2/3", 1),
        BuildTrack("C", "2026-02-01", "", 1),
        BuildTrack("A", "2026-01-01", "10", 1),
        BuildTrack("D", "2026-02-01", "1", 1),
    }.OrderBy(item => item.Date, StringComparer.Ordinal)
        .ThenBy(item => ManifestBuildReader.TrackNumber(item.Track))
        .ThenBy(item => item.Title, StringComparer.Ordinal)
        .Select(item => item.Title)
        .ToArray();
    SequenceEqual(new[] { "A", "D", "B", "C" }, tracks);
}

static void SplitAlbumsAcrossDiscs()
{
    const long gib = 1024L * 1024 * 1024;
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 2 * gib, album: "A"),
        BuildTrack("B1", "2", "1", 2 * gib, album: "B"),
        BuildTrack("C1", "3", "1", 1 * gib, album: "C"),
    };
    var plan = new DiscPlanner().Plan(tracks, 4_707_319_808, 0, 70);
    Equal(2, plan.Discs.Count);
    Equal(2, plan.Discs[0].Albums.Count);
    Equal("C", plan.Discs[1].Albums[0].Name);
}

static void KeepAlbumOnOneDisc()
{
    const long gib = 1024L * 1024 * 1024;
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 3 * gib, album: "A"),
        BuildTrack("A2", "1", "2", 1 * gib, album: "A"),
        BuildTrack("B1", "2", "1", 1 * gib, album: "B"),
    };
    var plan = new DiscPlanner().Plan(tracks, 4_707_319_808, 0, 70);
    Equal(2, plan.Discs.Count);
    Equal(2, plan.Discs[0].Tracks.Count);
    Equal("A", plan.Discs[0].Albums.Single().Name);
}

static void GroupAtAlbumBoundaries()
{
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 10, album: "A"),
        BuildTrack("A2", "1", "2", 10, album: "A"),
        BuildTrack("B1", "2", "1", 10, album: "B"),
        BuildTrack("B2", "2", "2", 10, album: "B"),
    };
    var plan = new DiscPlanner().Plan(tracks, long.MaxValue / 2, 0, 2);
    Equal(2, plan.Discs.Single().Groups.Count);
    Equal("A", plan.Discs[0].Groups[0].Tracks[0].Album);
    Equal("B", plan.Discs[0].Groups[1].Tracks[0].Album);
}

static void DiagnoseOversizedAlbumGroup()
{
    var tracks = Enumerable.Range(1, 3)
        .Select(index => BuildTrack($"A{index}", "1", index.ToString(), 10, album: "A"))
        .ToArray();
    var plan = new DiscPlanner().Plan(tracks, long.MaxValue / 2, 0, 2);
    Equal(1, plan.Discs.Single().Groups.Count);
    True(plan.Diagnostics.Any(item => item.Code == "ALBUM_EXCEEDS_GROUP_LIMIT"),
        "应报告单专辑超过组轨上限");
}


static void ResolveExternalMlp()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-external-mlp", Guid.NewGuid().ToString("N"));
    var sourceRoot = Path.Combine(root, "source");
    var externalRoot = Path.Combine(root, "external");
    var source = Path.Combine(sourceRoot, "album", "track.flac");
    var mirror = Path.Combine(externalRoot, "album", "track.mlp");
    Directory.CreateDirectory(Path.GetDirectoryName(mirror)!);
    File.WriteAllBytes(mirror, [1]);
    try
    {
        Equal(mirror, ExternalMlpProvider.Resolve(
            source, sourceRoot, externalRoot,
            new Dictionary<string, string> { ["track"] = "fallback.mlp" }));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void WriteMlpIndex()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-index-tests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var config = Path.Combine(root, "config.env");
    File.WriteAllText(config,
        $"DVDA_SRC={root}/src\nDVDA_FINAL_DIR={root}/final\nDVDA_BUILD_DIR={root}/build\nDVDA_TITLE=Test Disc");
    try
    {
        var options = Load(config);
        var track = BuildTrack("Track", "1", "1", 100, album: "Album") with
        {
            MlpPath = Path.Combine(root, "track.mlp"),
        };
        var plan = new DiscPlanner().Plan([track], ConfigDefaults.Dvd5Bytes, 2, 70);
        MlpIndexWriter.Write(options.MlpIndexPath, plan, options, dryRun: true,
            new DateTime(2026, 9, 29, 12, 0, 0));
        var text = File.ReadAllText(options.MlpIndexPath);
        True(text.Contains("\"__meta__\""), "索引应包含 __meta__");
        True(text.Contains("\"__discs__\""), "索引应包含 __discs__");
        True(text.Contains("ATS_01_1.AOB"), "索引应记录 AOB 映射");
        True(text.Contains(track.MlpPath.Replace("\\", "\\\\")), "索引应以 MLP 路径为键");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PreserveFormalMlpIndex()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-index-publish", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var formal = Path.Combine(root, "mlp_index.json");
        var dryRun = BuildPipeline.DryRunIndexPath(formal);
        var pending = BuildPipeline.PendingIndexPath(formal);
        File.WriteAllText(formal, "formal-old");
        File.WriteAllText(dryRun, "dry-run-new");

        Equal("mlp_index-dryrun.json", Path.GetFileName(dryRun));
        Equal("mlp_index.pending.json", Path.GetFileName(pending));
        Equal("formal-old", File.ReadAllText(formal));

        File.WriteAllText(dryRun, "{\"__meta__\":{\"dry_run\":true},\"__discs__\":[]}");
        Equal("MLP_INDEX_DRY_RUN", VerificationPipeline.ValidateIndexFile(dryRun)?.Code);
        True(VerificationPipeline.ValidateIndexFile(formal)?.Code == "MLP_INDEX_INVALID",
            "非 JSON 正式索引应报告 invalid，而不是被当成可用索引");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PublishDiscSetTransactionally()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-publish-set", Guid.NewGuid().ToString("N"));
    var stage = Path.Combine(root, "stage");
    var final = Path.Combine(root, "final");
    var build = Path.Combine(root, "build");
    Directory.CreateDirectory(stage);
    Directory.CreateDirectory(final);
    Directory.CreateDirectory(build);
    try
    {
        var staged1 = Path.Combine(stage, "disc1.iso");
        var staged2 = Path.Combine(stage, "disc2.iso");
        var final1 = Path.Combine(final, "disc1.iso");
        var final2 = Path.Combine(final, "disc2.iso");
        var pending = Path.Combine(build, "mlp_index.pending.json");
        var formal = Path.Combine(build, "mlp_index.json");
        File.WriteAllText(staged1, "new-disc-1");
        File.WriteAllText(staged2, "new-disc-2");
        File.WriteAllText(final1, "old-disc-1");
        File.WriteAllText(final2, "old-disc-2");
        File.WriteAllText(pending, "new-index");
        File.WriteAllText(formal, "old-index");

        var published = DiscPublisher.PublishSet(
            [(staged1, "disc1.iso"), (staged2, "disc2.iso")],
            final, pending, formal);
        Equal(2, published.Count);
        Equal("new-disc-1", File.ReadAllText(final1));
        Equal("new-disc-2", File.ReadAllText(final2));
        Equal("new-index", File.ReadAllText(formal));
        False(File.Exists(pending), "事务成功后 pending 索引应删除");
        Equal("new-disc-1", File.ReadAllText(staged1));
        Equal("new-disc-2", File.ReadAllText(staged2));

        File.WriteAllText(staged1, "broken-new-disc-1");
        File.WriteAllText(staged2, "broken-new-disc-2");
        File.WriteAllText(pending, "broken-new-index");
        File.Delete(final2);
        Directory.CreateDirectory(final2);
        var failed = false;
        try
        {
            DiscPublisher.PublishSet(
                [(staged1, "disc1.iso"), (staged2, "disc2.iso")],
                final, pending, formal);
        }
        catch (IOException)
        {
            failed = true;
        }
        True(failed, "任一目标无法提交时事务必须失败");
        Equal("new-disc-1", File.ReadAllText(final1));
        Equal("new-index", File.ReadAllText(formal));
        True(Directory.Exists(final2), "失败目标目录不得被事务破坏");
        Equal("broken-new-disc-1", File.ReadAllText(staged1));
        Equal("broken-new-disc-2", File.ReadAllText(staged2));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PublishMovedDiscSetTransactionally()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-moved-publish-set", Guid.NewGuid().ToString("N"));
    var stage = Path.Combine(root, "stage");
    var final = Path.Combine(root, "final");
    Directory.CreateDirectory(stage);
    Directory.CreateDirectory(final);
    try
    {
        var staged1 = Path.Combine(stage, "disc1.iso");
        var staged2 = Path.Combine(stage, "disc2.iso");
        var final1 = Path.Combine(final, "disc1.iso");
        var final2 = Path.Combine(final, "disc2.iso");
        var pending = Path.Combine(root, "mlp_index.pending.json");
        var formal = Path.Combine(root, "mlp_index.json");
        File.WriteAllText(staged1, "new-disc-1");
        File.WriteAllText(staged2, "new-disc-2");
        File.WriteAllText(final1, "old-disc-1");
        File.WriteAllText(final2, "old-disc-2");
        File.WriteAllText(pending, "new-index");
        File.WriteAllText(formal, "old-index");

        DiscPublisher.PublishSet(
            [(staged1, "disc1.iso"), (staged2, "disc2.iso")],
            final, pending, formal, moveStagedIsos: true);
        False(File.Exists(staged1), "同卷提交后不应保留暂存 ISO");
        False(File.Exists(staged2), "同卷提交后不应保留暂存 ISO");
        Equal("new-disc-1", File.ReadAllText(final1));
        Equal("new-disc-2", File.ReadAllText(final2));
        Equal("new-index", File.ReadAllText(formal));
        False(File.Exists(pending), "成功后应清理待发布索引");
        False(Directory.EnumerateFiles(final).Any(path =>
            path.Contains(".backup-", StringComparison.Ordinal) ||
            path.Contains(".publishing-", StringComparison.Ordinal)),
            "成功后应清理备份和临时文件");

        File.WriteAllText(staged1, "retry-disc-1");
        File.WriteAllText(staged2, "retry-disc-2");
        File.WriteAllText(pending, "retry-index");
        File.Delete(final2);
        Directory.CreateDirectory(final2);
        var failed = false;
        try
        {
            DiscPublisher.PublishSet(
                [(staged1, "disc1.iso"), (staged2, "disc2.iso")],
                final, pending, formal, moveStagedIsos: true);
        }
        catch (IOException)
        {
            failed = true;
        }
        True(failed, "第二盘无法提交时应失败");
        Equal("retry-disc-1", File.ReadAllText(staged1));
        Equal("retry-disc-2", File.ReadAllText(staged2));
        Equal("new-disc-1", File.ReadAllText(final1));
        Equal("new-index", File.ReadAllText(formal));
        True(Directory.Exists(final2), "失败目标目录应保持原样");
        Equal("retry-index", File.ReadAllText(pending));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void StageIsoWithoutCopy()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-stage-iso", Guid.NewGuid().ToString("N"));
    var stage = Path.Combine(root, "stage");
    var final = Path.Combine(root, "final");
    Directory.CreateDirectory(root);
    try
    {
        var source = Path.Combine(root, "disc1.iso");
        var pending = Path.Combine(root, "mlp_index.pending.json");
        var formal = Path.Combine(root, "mlp_index.json");
        File.WriteAllText(source, "new-disc");
        File.WriteAllText(pending, "new-index");
        var staged = DiscPublisher.StageIso(source, stage, "disc1.iso");
        False(File.Exists(source), "同卷暂存应移动 ISO 而非留下原件");
        Equal("new-disc", File.ReadAllText(staged.Path));
        True(staged.Diagnostic is null, "正常暂存不应产生警告");
        File.WriteAllText(source, "another-disc");
        var rejected = false;
        try
        {
            DiscPublisher.StageIso(source, stage, "disc1.iso");
        }
        catch (IOException)
        {
            rejected = true;
        }
        True(rejected, "暂存目标已存在时不得覆盖");
        Equal("another-disc", File.ReadAllText(source));
        Equal("new-disc", File.ReadAllText(staged.Path));

        var published = DiscPublisher.PublishSet(
            [(staged.Path, "disc1.iso")], final, pending, formal);
        Equal("new-disc", File.ReadAllText(published.Single()));
        Equal("new-index", File.ReadAllText(formal));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void BuildDvdaAuthorArguments()
{
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 10, album: "A"),
        BuildTrack("A2", "1", "2", 10, album: "A"),
        BuildTrack("B1", "2", "1", 10, album: "B"),
    };
    var plan = new DiscPlanner().Plan(tracks, ConfigDefaults.Dvd5Bytes, 0, 70);
    var arguments = DvdaAuthorCommandBuilder.BuildArguments(
        plan.Discs.Single(), "/work/out/disc1", "/work/tmp/disc1").ToArray();
    SequenceEqual(new[]
    {
        "-g", "A1.mlp", "A2.mlp", "-z", "B1.mlp",
        "-o", "/work/out/disc1", "-D", "/work/tmp/disc1", "-W", "-P0", "-n",
    }, arguments);
}

static void BuildDiagnosticTitleModes()
{
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 10, album: "A"),
        BuildTrack("A2", "1", "2", 10, album: "A"),
        BuildTrack("B1", "2", "1", 10, album: "B"),
        BuildTrack("B2", "2", "2", 10, album: "B"),
        BuildTrack("C1", "3", "1", 10, album: "C"),
    };
    var disc = new DiscPlanner().Plan(tracks, ConfigDefaults.Dvd5Bytes, 0, 70).Discs.Single();
    var one = DvdaAuthorCommandBuilder.BuildArguments(disc, "out", "tmp", titleMode: "one");
    Equal(0, one.Count(argument => argument == "-z"));
    var everyTwo = DvdaAuthorCommandBuilder.BuildArguments(disc, "out", "tmp", titleMode: "2");
    Equal(2, everyTwo.Count(argument => argument == "-z"));
    Equal("album", DvdaAuthorCommandBuilder.NormalizeTitleMode("invalid"));
}

static void LimitDiagnosticAlbums()
{
    var tracks = new[]
    {
        BuildTrack("A1", "1", "1", 10, album: "A"),
        BuildTrack("B1", "2", "1", 10, album: "B"),
        BuildTrack("B2", "2", "2", 10, album: "B"),
        BuildTrack("C1", "3", "1", 10, album: "C"),
    };
    var limited = BuildPlanService.ApplyDiagnosticAlbumLimit(tracks, 2);
    Equal(3, limited.Count);
    SequenceEqual(new[] { "A", "B", "B" }, limited.Select(track => track.Album).ToArray());
    Equal(tracks.Length, BuildPlanService.ApplyDiagnosticAlbumLimit(tracks, null).Count);
}

static void PreserveLegacyShellKeySet()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out", path =>
    {
        var keys = Load(path).ToShellPairs().Select(pair => pair.Key).ToArray();
        SequenceEqual(new[]
        {
            "DVDA_SRC", "DVDA_FINAL_DIR", "DVDA_BUILD_DIR",
            "DVDA_TITLE", "DVDA_ISO_PREFIX", "DVDA_MANIFEST", "DVDA_REPORT",
            "DVDA_OUT_ROOT", "DVDA_TMP_ROOT", "DVDA_ISO_DIR", "DVDA_MLP_DIR",
            "DVDA_MLP_INDEX", "DVDA_ALAC_FIX_DIR", "DVDA_BUILD_LOG", "DVDA_AUTHOR",
            "DVDA_MKISOFS", "DVDA_FFMPEG", "DVDA_FFPROBE", "DVDA_AUTHOR_SRC",
            "DVDA_MAX_DISCS", "DVDA_GROUP_TRACK_LIMIT", "DVDA_DISC_BYTES",
            "DVDA_MLP_SOURCE", "DVDA_MLP_EXTERNAL_DIR", "DVDA_LOSS_ERROR_S",
            "DVDA_LOSS_WARN_S",
        }, keys);
        True(Load(path).ToShellPairsAll().Count > keys.Length,
            "--shell-all 应包含 C# 新增配置键");
    });
}

static void DescribeConfigurationSources()
{
    WithConfig("DVDA_SRC=/from-file\nDVDA_FINAL_DIR=/out\nCUSTOM_VALUE=fixture", path =>
    {
        var options = Load(path, new()
        {
            ["DVDA_SRC"] = "/from-environment",
            ["DVDA_TITLE"] = "",
        });
        Equal("环境变量", options.ValueSource("DVDA_SRC"));
        Equal("config.env", options.ValueSource("DVDA_FINAL_DIR"));
        Equal("config.env", options.ValueSource("CUSTOM_VALUE"));
        Equal("默认值", options.ValueSource("DVDA_TITLE"));
        True(options.HasEnvironmentOverrides(), "应识别非空环境变量覆盖");
        True(options.EffectiveKeys().Contains("CUSTOM_VALUE"), "显式配置的扩展键应出现在诊断输出");
        SequenceEqual(
            options.EffectiveKeys().Order(StringComparer.Ordinal),
            options.EffectiveKeys());
    });
}

static void BuildInProcessIsoArguments()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_TITLE=Test", path =>
    {
        var options = Load(path);
        var disc = new DiscPlan(1, [], []);
        SequenceEqual(new[]
        {
            "-o", "/work/disc1", "-D", "/work/tmp", "-W", "-P0", "-n",
            "--iso=/work/disc1.iso",
        }, DvdaAuthorCommandBuilder.BuildArguments(
            disc, "/work/disc1", "/work/tmp", isoPath: "/work/disc1.iso"));
    });
}

static void WriteCompatibleBuildLog()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-log-tests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var config = Path.Combine(root, "config.env");
    File.WriteAllText(config,
        $"DVDA_SRC={root}/src\nDVDA_FINAL_DIR={root}/final\nDVDA_BUILD_DIR={root}/build");
    try
    {
        var options = Load(config);
        using (var log = new BuildLogWriter(options, dryRun: false))
        {
            log.WriteCommand("dvda-author", ["-g", "track.mlp", "-o", "disc1", "-D", "tmp"]);
            log.WriteLine("1  1/1  1  0  99  0  90000  0");
        }
        var text = File.ReadAllText(options.BuildLogPath);
        True(text.Contains("+ dvda-author -g track.mlp -o disc1 -D tmp"),
            "日志应包含 audit_disc 可解析的命令行");
        True(text.Contains("1  1/1  1  0  99  0  90000  0"),
            "日志应保留轨道表行");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PublishIso()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-publish-tests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var source = Path.Combine(root, "source.iso");
    File.WriteAllBytes(source, [1, 2, 3, 4]);
    try
    {
        var published = DiscPublisher.Publish(source, Path.Combine(root, "final"), "disc.iso");
        Equal(4L, new FileInfo(published.Path).Length);
        True(File.ReadAllBytes(published.Path).SequenceEqual(new byte[] { 1, 2, 3, 4 }),
            "发布 ISO 内容应一致");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void ParseAlacMagicCookie()
{
    var data = new byte[64];
    "alac"u8.CopyTo(data.AsSpan(4));
    var cookie = data.AsSpan(12, 24);
    System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(cookie[..4], 4096);
    cookie[5] = 24;
    cookie[9] = 2;
    System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(cookie[12..16], 24_600);
    System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(cookie[20..24], 48_000);
    var parsed = AlacEndRepairer.ReadMagicCookie(data) ??
        throw new InvalidOperationException("应识别有效 ALAC magic cookie");
    Equal(4096, parsed.MaxSamplesPerFrame);
    Equal(24, parsed.SampleSize);
    Equal(2, parsed.Channels);
    Equal(48_000, parsed.SampleRate);
}

static void PatchAlacEndMarker()
{
    var data = new byte[8];
    data[2] = 0x02;
    var cookie = new AlacMagicCookie(1, 16, 2, 48_000, 0, 0);
    var packets = new[] { new AlacPacket(32, data.Length, 0) };
    var patches = AlacEndRepairer.FindBadFrames(data, cookie, packets);
    Equal(1, patches.Count);
    Equal(0, patches[0].PreviousBits);
    AlacEndRepairer.ApplyPatches(data, patches);
    Equal(0b0000_0001, data[6] & 0b0000_0001);
    Equal(0b1100_0000, data[7] & 0b1100_0000);
    Equal(0, AlacEndRepairer.FindBadFrames(data, cookie, packets).Count);
}

static void PatchAlacWithExtraBits()
{
    var data = new byte[8];
    data[2] = 0x06;
    var cookie = new AlacMagicCookie(1, 16, 2, 48_000, 0, 0);
    var packets = new[] { new AlacPacket(0, data.Length, 0) };
    Equal(1, AlacEndRepairer.FindBadFrames(data, cookie, packets).Count);
}

static void PreserveM4aTagValues()
{
    var tags = M4aFlacConverter.NormalizeTags(new Dictionary<string, string>
    {
        ["sort_name"] = "  Title  ",
        ["encoder"] = "drop",
        ["empty"] = "   ",
    });
    Equal(1, tags.Count);
    Equal(("TITLESORT", "  Title  "), tags[0]);
}

static void M4aDryRunDoesNotWrite()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-m4a-dryrun", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var source = Path.Combine(root, "track.m4a");
        WriteAlacFixture(source);
        var converter = new M4aFlacConverter(
            new ProcessRunner(), "unused-ffmpeg", ffprobe,
            new AlacEndRepairer(new ProcessRunner(), ffprobe));

        var before = Directory.EnumerateFileSystemEntries(root).OrderBy(path => path).ToArray();
        var results = converter.ConvertAsync([source], dryRun: true)
            .GetAwaiter().GetResult();
        var after = Directory.EnumerateFileSystemEntries(root).OrderBy(path => path).ToArray();

        Equal(1, results.Count);
        Equal("DRY", results[0].Status);
        Equal(1, results[0].RepairedFrames);
        SequenceEqual(before, after);
        False(File.Exists(Path.ChangeExtension(source, ".flac")), "dry-run 不得生成 FLAC");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void RejectNonAlacM4a()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-m4a-aac", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var source = Path.Combine(root, "aac-track.m4a");
        File.WriteAllBytes(source, [1, 2, 3]);
        var converter = new M4aFlacConverter(
            new ProcessRunner(), "unused-ffmpeg", ffprobe,
            new AlacEndRepairer(new ProcessRunner(), ffprobe));

        var result = converter.ConvertAsync([source], dryRun: true)
            .GetAwaiter().GetResult().Single();
        Equal("FAIL", result.Status);
        True(result.Error?.Contains("不是 ALAC", StringComparison.Ordinal) == true,
            "AAC 文件应明确报告首音频流不是 ALAC");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void IsolateM4aFileFailures()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-m4a-isolation", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var good = Path.Combine(root, "good.m4a");
        var broken = Path.Combine(root, "broken.m4a");
        WriteAlacFixture(good);
        File.WriteAllBytes(broken, [1]);
        var converter = new M4aFlacConverter(
            new ProcessRunner(), "unused-ffmpeg", ffprobe,
            new AlacEndRepairer(new ProcessRunner(), ffprobe));

        var results = converter.ConvertAsync([good, broken], dryRun: true, jobs: 2)
            .GetAwaiter().GetResult();
        Equal(2, results.Count);
        Equal("FAIL", results.Single(result => result.SourcePath == broken).Status);
        Equal("DRY", results.Single(result => result.SourcePath == good).Status);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void ParseFlacPictureDescriptor()
{
    var descriptor = M4aFlacConverter.ParsePictureDescriptor("""
        METADATA block #2
          type: 6 (PICTURE)
          is last: false
          length: 32487
          type: 3 (Cover (front))
          MIME type: image/jpeg
          width: 1200
          height: 1200
          depth: 24
          colors: 0
        """) ?? throw new InvalidOperationException("应解析有效 PICTURE 描述");
    Equal(3, descriptor.Type);
    Equal("image/jpeg", descriptor.MimeType);
    Equal(1200, descriptor.Width);
    Equal(1200, descriptor.Height);
    Equal(24, descriptor.Depth);
    True(M4aFlacConverter.ParsePictureDescriptor("type: 3") is null,
        "缺少 MIME/尺寸/depth 时不得视为有效描述");
}

static void PreserveConversionOnDeleteFailure()
{
    var results = new[]
    {
        new M4aConversionResult("one.m4a", "one.flac", "OK", null, 0, "abc", 1, 1, false, false),
        new M4aConversionResult("two.m4a", "two.flac", "OK", null, 0, "def", 1, 1, false, false),
    };
    var deleted = new List<string>();
    var updated = M4aFlacConverter.ApplySourceDeletion(results, path =>
    {
        if (path == "two.m4a") throw new IOException("fixture locked");
        deleted.Add(path);
    });
    SequenceEqual(new[] { "one.m4a" }, deleted);
    True(updated[0].SourceDeleteError is null, "成功删除不应产生警告");
    True(updated[1].SourceDeleteError?.Contains("fixture locked", StringComparison.Ordinal) == true,
        "删除失败应记录在结果中而不是抛出异常");
    Equal("OK", updated[1].Status);
}

static void AnalyzeAllAobSegments()
{
    static byte[] Sector(long pts)
    {
        var sector = new byte[2048];
        sector[0] = 0;
        sector[1] = 0;
        sector[2] = 1;
        sector[3] = 0xBA;
        sector[4] = 0;
        sector[5] = 0;
        sector[6] = 1;
        sector[7] = 0xBD;
        sector[11] = 0x80;
        EncodePts(pts).CopyTo(sector, 13);
        return sector;
    }

    static byte[] Join(params byte[][] values)
    {
        var result = new byte[values.Sum(value => value.Length)];
        var offset = 0;
        foreach (var value in values)
        {
            value.CopyTo(result, offset);
            offset += value.Length;
        }
        return result;
    }

    var analysis = VerificationPipeline.AnalyzeAobGroup(
        [
            ("ATS_01_1.AOB", Join(Sector(100), Sector(200), Sector(300))),
            ("ATS_01_2.AOB", Join(Sector(300), Sector(300), Sector(300))),
        ],
        "fixture group");
    True(analysis.Issues.Any(issue => issue.Code == "PTS_ABNORMAL_RATIO"),
        "后续 AOB 分段中的固定 PTS 必须被检测到");
}

static void ValidateAobPtsDamageFixtures()
{
    static byte[] Sector(long? pts)
    {
        var sector = new byte[2048];
        sector[0] = 0;
        sector[1] = 0;
        sector[2] = 1;
        sector[3] = 0xBA;
        if (pts is null) return sector;

        sector[4] = 0;
        sector[5] = 0;
        sector[6] = 1;
        sector[7] = 0xBD;
        sector[11] = 0x80;
        EncodePts(pts.Value).CopyTo(sector, 13);
        return sector;
    }

    static byte[] Join(params byte[][] sectors)
    {
        var result = new byte[sectors.Sum(sector => sector.Length)];
        var offset = 0;
        foreach (var sector in sectors)
        {
            sector.CopyTo(result, offset);
            offset += sector.Length;
        }
        return result;
    }

    var noPts = AobPtsAnalyzer.Analyze(
        Join(Sector(null), Sector(null), Sector(null)), "no-pts");
    Equal(0, noPts.PtsSectorCount);
    True(noPts.Issues.Any(issue => issue.Code == "PTS_TOO_FEW"),
        "完全缺失 PTS 必须报告 PTS_TOO_FEW");

    foreach (var count in new[] { 1, 2 })
    {
        var sectors = Enumerable.Range(0, 3)
            .Select(index => Sector(index < count ? 100L + index * 100L : null))
            .ToArray();
        var tooFew = AobPtsAnalyzer.Analyze(Join(sectors), $"pts-{count}");
        Equal(count, tooFew.PtsSectorCount);
        True(tooFew.Issues.Any(issue => issue.Code == "PTS_TOO_FEW"),
            $"只有 {count} 个 PTS 时必须报告 PTS_TOO_FEW");
    }

    var fixedPts = AobPtsAnalyzer.Analyze(
        Join(Sector(500), Sector(500), Sector(500)), "fixed-pts");
    True(fixedPts.Issues.Any(issue => issue.Code == "PTS_NOT_ADVANCING"),
        "固定 PTS 必须报告时间轴未推进");
    True(fixedPts.ZeroSteps == 2 && fixedPts.AbnormalSteps == 2,
        "固定 PTS 的零步长与异常步长计数必须准确");

    var decreasing = AobPtsAnalyzer.Analyze(
        Join(Sector(300), Sector(200), Sector(100)), "decreasing-pts");
    Equal(2, decreasing.NegativeSteps);
    True(decreasing.Issues.Any(issue => issue.Code == "PTS_ABNORMAL_RATIO"),
        "持续下降的 PTS 必须报告异常比例");

    var normal = Enumerable.Range(0, 102)
        .Select(index => Sector(index * 100L))
        .ToArray();
    normal[^1] = Sector(100_000);
    var abnormal = AobPtsAnalyzer.Analyze(Join(normal), "large-step");
    Equal(1, abnormal.AbnormalSteps);
    True(abnormal.AbnormalRatio <= 1d,
        "恰好一个异常步长占比不超过 1% 时不应触发阈值");
    False(abnormal.Issues.Any(issue => issue.Code == "PTS_ABNORMAL_RATIO"),
        "异常比例等于或低于 1% 应通过");

    normal[^2] = Sector(50_000);
    var aboveThreshold = AobPtsAnalyzer.Analyze(Join(normal), "large-steps");
    True(aboveThreshold.AbnormalRatio > 1d,
        "两个异常步长应超过 1% 阈值");
    True(aboveThreshold.Issues.Any(issue => issue.Code == "PTS_ABNORMAL_RATIO"),
        "异常比例超过 1% 必须报告失败");
}

static void WriteAlacFixture(string path)
{
    var data = new byte[4_164];
    "alac"u8.CopyTo(data.AsSpan(4));
    var cookie = data.AsSpan(12, 24);
    System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(cookie[..4], 1_024);
    cookie[5] = 16;
    cookie[9] = 2;
    System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(cookie[20..24], 48_000);
    data[66] = 0x02;
    File.WriteAllBytes(path, data);
}

static void ParseAnsiAuditLog()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-audit-log", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var path = Path.Combine(root, "build.log");
    try
    {
        File.WriteAllText(path, string.Join('\n',
        [
            "+ dvda-author -g a.mlp b.mlp -g c.mlp -o \"/work/output/disc1\" -D /work/tmp/disc1",
            "\u001b[32m1  1/2  1  0  99  0  90000  0\u001b[0m",
            "1  1/2  2  100  199  90000  90000  0",
            "+ dvda-author -g d.mlp -o /work/output/disc2 -D /work/tmp/disc2",
            "1  1/1  1  0  49  0  45000  0",
        ]));

        var parsed = DiscVerifier.ParseAuditLog(path);
        Equal(3, parsed.Rows.Count);
        Equal(2, parsed.Commands.Count);
        Equal("disc1", parsed.Commands[0].DiscTag);
        Equal(2, parsed.Commands[0].GroupCount);
        Equal(2, parsed.Commands[0].Rows.Count);
        Equal("disc2", parsed.Commands[1].DiscTag);
        Equal(1, parsed.Commands[1].GroupCount);
        Equal(1, parsed.Commands[1].Rows.Count);
        Equal(100, parsed.Rows[1].First);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void ParseLatestFormalAuditLog()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-audit-latest", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var path = Path.Combine(root, "build.log");
    try
    {
        File.WriteAllText(path, string.Join('\n',
        [
            "[C# build] 2026-09-28 10:00:00",
            "+ dvda-author -g old.mlp -o /work/output/disc1",
            "1  1/1  1  0  9  0  9000  0",
            "[C# build] [DRY-RUN] 2026-09-28 11:00:00",
            "[C# build] 2026-09-29 12:00:00",
            "+ dvda-author -g one.mlp two.mlp -o /work/output/disc1",
            "1  1/2  1  0  19  0  9000  0",
            "1  1/2  2  20  39  9000  9000  0",
            "+ dvda-author -g three.mlp -o /work/output/disc2",
            "1  1/1  1  0  29  0  9000  0",
        ]));

        var parsed = DiscVerifier.ParseAuditLog(path);
        Equal(2, parsed.Commands.Count);
        Equal(3, parsed.Rows.Count);
        Equal(2, parsed.Commands[0].Rows.Count);
        Equal(1, parsed.Commands[1].Rows.Count);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void SelectCurrentIsoNames()
{
    True(DiscVerifier.MatchesCurrentIsoName(
        "Wuthering_Waves_Singles_EPs_1.iso", "Wuthering_Waves_Singles_EPs"),
        "当前第 1 盘应被选中");
    True(DiscVerifier.MatchesCurrentIsoName(
        "Wuthering_Waves_Singles_EPs_2.ISO", "Wuthering_Waves_Singles_EPs"),
        "扩展名大小写不应影响匹配");
    False(DiscVerifier.MatchesCurrentIsoName(
        "Wuthering_Waves_Singles_EPs_SurCode_2.iso", "Wuthering_Waves_Singles_EPs"),
        "历史 SurCode ISO 不应被纳入当前构建校验");
}

static void SelectNewestAuditLog()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-audit-selection", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var build = Path.Combine(root, "build.log");
        var rebuild = Path.Combine(root, "rebuild-final.log");
        var final = Path.Combine(root, "finalrebuild.log");
        File.WriteAllText(build, "build");
        File.WriteAllText(rebuild, "rebuild");
        File.WriteAllText(final, "final");
        var baseline = new DateTime(2026, 9, 28, 0, 0, 0, DateTimeKind.Utc);
        File.SetLastWriteTimeUtc(build, baseline);
        File.SetLastWriteTimeUtc(rebuild, baseline.AddMinutes(1));
        File.SetLastWriteTimeUtc(final, baseline.AddMinutes(2));

        Equal(final, DiscVerifier.SelectLatestBuildLog(build));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void DetectExternalMlpAmbiguity()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-ambiguous-mlp", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(Path.Combine(root, "a"));
    Directory.CreateDirectory(Path.Combine(root, "b"));
    File.WriteAllBytes(Path.Combine(root, "a", "same.mlp"), [1]);
    File.WriteAllBytes(Path.Combine(root, "b", "same.mlp"), [2]);
    try
    {
        var lookup = ExternalMlpProvider.BuildBasenameLookup(root);
        True(lookup.AmbiguousNames.Contains("same", StringComparer.OrdinalIgnoreCase),
            "应报告重复 basename");
        False(lookup.Index.ContainsKey("same"), "歧义 basename 不得进入回退索引");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void RejectDuplicateMlpIndexKeys()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-duplicate-index", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var config = Path.Combine(root, "config.env");
    File.WriteAllText(config,
        $"DVDA_SRC={root}/src\nDVDA_FINAL_DIR={root}/final\nDVDA_BUILD_DIR={root}/build");
    try
    {
        var options = Load(config);
        var shared = Path.Combine(root, "shared.mlp");
        var tracks = new[]
        {
            BuildTrack("A", "1", "1", 10) with { MlpPath = shared },
            BuildTrack("B", "2", "1", 10) with { MlpPath = shared },
        };
        var plan = new DiscPlanner().Plan(tracks, ConfigDefaults.Dvd5Bytes, 0, 70);
        var rejected = false;
        try
        {
            MlpIndexWriter.Write(options.MlpIndexPath, plan, options, dryRun: true);
        }
        catch (InvalidDataException)
        {
            rejected = true;
        }
        True(rejected, "重复 MLP 路径必须拒绝写入索引");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static DiscPlan SpaceDisc(int number, long mlpBytes) => new(
    number,
    [new AlbumPlan($"Album{number}", [BuildTrack($"Track{number}", "1", "1", mlpBytes)])],
    []);

static int FixtureCallCount(string root, string tool)
{
    var path = Path.Combine(root, $"fixture-calls-{tool}.txt");
    return File.Exists(path) ? File.ReadAllLines(path).Length : 0;
}

static void SetFixtureCallLogDirectory(string? directory) =>
    Environment.SetEnvironmentVariable("DVDA_FIXTURE_CALL_LOG_DIR", directory);

static DvdaOptions CreatePrepareOptions(string root, string ffprobe, string ffmpeg)
{
    var source = Path.Combine(root, "src");
    Directory.CreateDirectory(source);
    var config = Path.Combine(root, "config.env");
    File.WriteAllText(config, string.Join('\n',
    [
        $"DVDA_SRC={source.Replace('\\', '/')}",
        $"DVDA_FINAL_DIR={Path.Combine(root, "final").Replace('\\', '/')}",
        $"DVDA_BUILD_DIR={Path.Combine(root, "build").Replace('\\', '/')}",
        $"DVDA_FFPROBE={ffprobe.Replace('\\', '/')}",
        $"DVDA_FFMPEG={ffmpeg.Replace('\\', '/')}",
        "DVDA_TITLE=Prepare Fixture",
    ]));
    return Load(config);
}

static void PrepareCacheReuseAndInvalidation()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-prepare-cache", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    SetFixtureCallLogDirectory(root);
    try
    {
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var ffmpeg = CreateFixtureExecutable(root, "fake-ffmpeg.exe");
        var options = CreatePrepareOptions(root, ffprobe, ffmpeg);
        var source = Path.Combine(options.SourceDirectory, "song.flac");
        File.WriteAllBytes(source, new byte[4096]);

        var first = new PreparationPipeline(options).RunAsync().GetAwaiter().GetResult();
        Equal(1, first.CheckedTracks);
        Equal(0, first.FailureCount);
        Equal(1, FixtureCallCount(root, "ffprobe"));
        Equal(1, FixtureCallCount(root, "ffmpeg"));
        True(File.Exists(options.PrepareCachePath), "首次运行应写出准备缓存");

        var second = new PreparationPipeline(options).RunAsync().GetAwaiter().GetResult();
        Equal(0, second.FailureCount);
        Equal(1, FixtureCallCount(root, "ffprobe"));
        Equal(1, FixtureCallCount(root, "ffmpeg"));
        Equal(second.Manifest.Count, first.Manifest.Count);
        True(second.Manifest.Values.SelectMany(group => group.Files)
                .All(file => file.Title == "Fixture Song"),
            "复用缓存时应能还原标签");

        // 同长度、同修改时间但内容变化：必须重新探测与校验。
        var lastWrite = File.GetLastWriteTimeUtc(source);
        File.WriteAllBytes(source, Enumerable.Repeat((byte)0xA5, 4096).ToArray());
        File.SetLastWriteTimeUtc(source, lastWrite);
        var third = new PreparationPipeline(options).RunAsync().GetAwaiter().GetResult();
        Equal(0, third.FailureCount);
        Equal(2, FixtureCallCount(root, "ffprobe"));
        Equal(2, FixtureCallCount(root, "ffmpeg"));

        // --force 必须忽略已有记录。
        var forced = new PreparationPipeline(options)
            .RunAsync(forceRevalidation: true).GetAwaiter().GetResult();
        Equal(0, forced.FailureCount);
        Equal(3, FixtureCallCount(root, "ffprobe"));
        Equal(3, FixtureCallCount(root, "ffmpeg"));
    }
    finally
    {
        SetFixtureCallLogDirectory(null);
        Directory.Delete(root, recursive: true);
    }
}

static void PrepareCacheSkipsFailedTracks()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-prepare-fail", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    SetFixtureCallLogDirectory(root);
    try
    {
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var ffmpeg = CreateFixtureExecutable(root, "fake-ffmpeg-mismatch.exe");
        var options = CreatePrepareOptions(root, ffprobe, ffmpeg);
        File.WriteAllBytes(Path.Combine(options.SourceDirectory, "song.flac"), new byte[4096]);

        var first = new PreparationPipeline(options).RunAsync().GetAwaiter().GetResult();
        True(first.FailureCount > 0, "采样数不吻合应判为失败");
        False(File.Exists(options.ManifestPath), "失败时不得生成 manifest");

        var second = new PreparationPipeline(options).RunAsync().GetAwaiter().GetResult();
        True(second.FailureCount > 0, "失败记录不得被缓存");
        Equal(2, FixtureCallCount(root, "ffprobe"));
        Equal(2, FixtureCallCount(root, "ffmpeg"));
    }
    finally
    {
        SetFixtureCallLogDirectory(null);
        Directory.Delete(root, recursive: true);
    }
}

static void PrepareCacheIdentityAndStorage()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-identity", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var file = Path.Combine(root, "sample.bin");
        File.WriteAllBytes(file, [1, 2, 3, 4, 5, 6, 7, 8]);
        var identity = FileIdentityProbe.Compute(file);
        True(identity is not null, "应能计算文件身份");
        Equal(8L, identity!.Size);
        True(FileIdentityProbe.Matches(file, identity), "未变化时应匹配");
        Equal(identity.HeadHash, identity.TailHash);

        File.WriteAllBytes(file, [9, 9, 9, 4, 5, 6, 7, 8]);
        False(FileIdentityProbe.Matches(file, identity), "内容变化后必须不匹配");
        var currentIdentity = FileIdentityProbe.Compute(file);
        True(currentIdentity is not null, "应能重新计算文件身份");
        True(currentIdentity!.HeadHash != identity.HeadHash,
            "同长度不同内容应由首尾哈希陶汰");

        // 缓存存储往返：补丁与修复文件身份必须保留。
        var cachePath = Path.Combine(root, "nested", PrepareCache.FileName);
        var patches = new[] { new AlacFramePatch(3, 32.0, 4096, 64, 4096, 100, 0b000) };
        var entry = new PrepareCacheEntry
        {
            Identity = currentIdentity!,
            Probe = new AudioProbeFacts
            {
                SampleRate = 48_000, Bits = 24, Channels = 2,
                Title = "T", Album = "A", Date = "2026", Track = "1", Duration = 1.0,
            },
            Validation = new PrepareValidationFacts
            {
                SampleRate = 44_100, Bits = 24, ResampleTo = 44_100,
                ExpectedSamples = 44_100, DecodedSamples = 44_100,
                Patches = patches, RepairedFile = currentIdentity,
            },
        };
        var cache = PrepareCache.Load(cachePath);
        Equal(0, cache.Count);
        cache.Record(file, entry);
        cache.Save(cachePath);
        True(File.Exists(cachePath), "保存应创建目录与文件");

        var reloaded = PrepareCache.Load(cachePath);
        Equal(1, reloaded.Count);
        var roundTrip = reloaded.Match(file);
        True(roundTrip is not null, "身份一致时应能命申缓存");
        Equal("T", roundTrip!.Probe.Title);
        Equal(44_100, roundTrip.Validation.ResampleTo);
        Equal(1, roundTrip.Validation.Patches!.Count);
        Equal(32.0, roundTrip.Validation.Patches[0].PresentationTime);
        True(roundTrip.Validation.RepairedFile is not null, "修复文件身份应保留");
        Equal(0, PrepareCache.Load(Path.Combine(root, "missing.json")).Count);

        // 损坏文件不得抛异常，也不得返回陈旧记录。
        var broken = Path.Combine(root, "broken.json");
        File.WriteAllText(broken, "{ not json");
        Equal(0, PrepareCache.Load(broken).Count);

        // 复用规则：归一化参数或期望采样数不一致，以及修复文件缺失，都必须重新校验。
        var track = Track("song.flac", 48_000, 24);
        var reusableEntry = new PrepareCacheEntry
        {
            Identity = currentIdentity!,
            Validation = new PrepareValidationFacts
            {
                SampleRate = 48_000, Bits = 24, ExpectedSamples = 48_000, DecodedSamples = 48_000,
            },
        };
        True(PreparationPipeline.IsReusable(reusableEntry, track, 48_000),
            "参数一致且校验通过时应可复用");
        False(PreparationPipeline.IsReusable(reusableEntry, track, 44_100),
            "期望采样数变化后不得复用");
        False(PreparationPipeline.IsReusable(new PrepareCacheEntry
        {
            Identity = currentIdentity!,
            Validation = new PrepareValidationFacts
            {
                SampleRate = 48_000, Bits = 24, ResampleTo = 44_100,
                ExpectedSamples = 48_000, DecodedSamples = 48_000,
            },
        }, track, 48_000), "重采样目标变化后不得复用");
        False(PreparationPipeline.IsReusable(new PrepareCacheEntry
        {
            Identity = currentIdentity!,
            Validation = new PrepareValidationFacts
            {
                SampleRate = 48_000, Bits = 24, ExpectedSamples = 48_000, DecodedSamples = 48_000,
                RepairedFile = new FileIdentity(
                    Path.Combine(root, "missing-repaired.m4a"), 8, 0, "a", "a"),
            },
        }, track, 48_000), "修复文件已消失时不得复用");
        False(PreparationPipeline.IsReusable(new PrepareCacheEntry
        {
            Identity = currentIdentity!,
            Validation = new PrepareValidationFacts
            {
                SampleRate = 48_000, Bits = 24, ExpectedSamples = 48_000, DecodedSamples = 0,
            },
        }, track, 48_000), "没有采样数证据时不得复用");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PreparationSnapshotChecks()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-prepare-snapshot", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var options = CreatePrepareOptions(root, "ffprobe", "ffmpeg");
        var source = Path.Combine(options.SourceDirectory, "song.flac");
        var original = Enumerable.Range(0, 4096).Select(index => (byte)(index & 0xFF)).ToArray();
        File.WriteAllBytes(source, original);
        Directory.CreateDirectory(Path.GetDirectoryName(options.ManifestPath)!);
        File.WriteAllText(options.ManifestPath, "{}\n");

        True(PreparationSnapshotStore.Save(options, [source]), "成功准备后应保存源指纹快照");
        True(PreparationSnapshotStore.TryReuse(options, out var snapshot, out _),
            "清单和源文件未变化时应复用准备快照");
        Equal(1, snapshot!.Sources.Count);

        File.WriteAllBytes(source, Enumerable.Repeat((byte)0xA5, original.Length).ToArray());
        False(PreparationSnapshotStore.TryReuse(options, out _, out _),
            "源文件内容变化后不得复用准备快照");
        File.WriteAllBytes(source, original);
        File.WriteAllBytes(Path.Combine(options.SourceDirectory, "new.flac"), [1, 2, 3]);
        False(PreparationSnapshotStore.TryReuse(options, out _, out _),
            "新增源文件后不得复用准备快照");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void ScanPtsSectorsStreamingly()
{
    static byte[] Sector(long? pts)
    {
        var sector = new byte[2048];
        sector[0] = 0;
        sector[1] = 0;
        sector[2] = 1;
        sector[3] = 0xBA;
        if (pts is null)
        {
            return sector;
        }
        sector[4] = 0;
        sector[5] = 0;
        sector[6] = 1;
        sector[7] = 0xBD;
        sector[11] = 0x80;
        EncodePts(pts.Value).CopyTo(sector, 13);
        return sector;
    }

    static List<DiscVerifier.TrackRow> Rows(params (int Title, int First)[] values) =>
        values.Select(value => new DiscVerifier.TrackRow(1, value.Title, 1, value.First, 0, 0, 0))
            .ToList();

    static List<VerificationIssue> Scan(
        IEnumerable<byte[]> sectors,
        IReadOnlyList<DiscVerifier.TrackRow> rows,
        out DiscVerifier.PtsScanResult result)
    {
        var issues = new List<VerificationIssue>();
        result = DiscVerifier.ScanPtsSectors(
            sectors.Select(sector => (ReadOnlyMemory<byte>)sector), rows, 1, issues);
        return issues;
    }

    // 正常递增：不计问题，扇区数准确。
    var issues = Scan(
        [Sector(100), Sector(200), Sector(300)],
        Rows((1, 0)),
        out var normal);
    Equal(0, issues.Count);
    Equal(3, normal.SectorCount);
    Equal(0, normal.Drops.Count);

    // 轨道边界处的 PTS 下降属于预期，不计问题。
    issues = Scan(
        [Sector(300), Sector(100)],
        Rows((1, 0), (2, 1)),
        out var boundary);
    Equal(0, issues.Count);
    Equal(1, boundary.Drops.Count);
    True(boundary.Drops.Contains(1), "边界下降应记录为 drop");

    // 非边界的 PTS 下降必须报告。
    issues = Scan(
        [Sector(300), Sector(100)],
        Rows((1, 0)),
        out _);
    True(issues.Count == 1, $"非边界下降应报告一次，实际 {issues.Count}");
    True(issues.Count > 0 && issues[0].Code == "PTS_DROP_OFF_BOUNDARY",
        $"非边界下降的代码应为 PTS_DROP_OFF_BOUNDARY，实际 {(issues.Count > 0 ? issues[0].Code : "<无>")}");

    // 轨道起点缺少 PTS 下降必须报告。
    issues = Scan(
        [Sector(100), Sector(200), Sector(300)],
        Rows((1, 0), (2, 1)),
        out _);
    True(issues.Count == 1, $"缺失重置应报告一次，实际 {issues.Count}");
    True(issues.Count > 0 && issues[0].Code == "PTS_RESET_MISSING",
        $"缺失重置的代码应为 PTS_RESET_MISSING，实际 {(issues.Count > 0 ? issues[0].Code : "<无>")}");

    // 缺少 PTS 的扇区应立即停止解析，不再读取后续扇区。
    var produced = 0;
    IEnumerable<byte[]> Tracked()
    {
        produced++;
        yield return Sector(100);
        produced++;
        yield return Sector(null);
        produced++;
        yield return Sector(300);
    }
    issues = Scan(Tracked(), Rows((1, 0)), out var stopped);
    True(issues.Count == 1, $"缺少 PTS 应报告一次，实际 {issues.Count}");
    True(issues.Count > 0 && issues[0].Code == "PTS_MISSING",
        $"缺少 PTS 的代码应为 PTS_MISSING，实际 {(issues.Count > 0 ? issues[0].Code : "<无>")}");
    Equal(2, stopped.SectorCount);
    Equal(2, produced);

    // 大批扇区：扇区数与 drop 序号在长序列中保持正确。
    var many = Enumerable.Range(0, 9000).Select(index => Sector(index * 100L + 100)).ToArray();
    many[5000] = Sector(0);
    issues = Scan(many, Rows((1, 0), (2, 5000)), out var large);
    True(issues.Count == 0, $"长序列不应报告问题，实际 {issues.Count}: " +
        $"{(issues.Count > 0 ? issues[0].Code : string.Empty)}");
    Equal(9000, large.SectorCount);
    True(large.Drops.Count == 1, $"长序列应记录一次下降，实际 {large.Drops.Count}");
    True(large.Drops.Contains(5000), "长序列中的下降扇区序号应准确");
}

static void SharedAobScanPreservesDiagnostics()
{
    static byte[] Sector(long? pts)
    {
        var sector = new byte[2048];
        sector[0] = 0; sector[1] = 0; sector[2] = 1; sector[3] = 0xBA;
        if (pts is not null)
        {
            sector[4] = 0; sector[5] = 0; sector[6] = 1; sector[7] = 0xBD;
            sector[11] = 0x80;
            EncodePts(pts.Value).CopyTo(sector, 13);
        }
        return sector;
    }

    var sectors = new[] { Sector(300), Sector(200), Sector(null), Sector(400), Sector(500) };
    var rows = new[]
    {
        new DiscVerifier.TrackRow(1, 1, 1, 0, 0, 0, 0),
        new DiscVerifier.TrackRow(1, 2, 1, 1, 0, 0, 0),
        new DiscVerifier.TrackRow(1, 3, 1, 2, 0, 0, 0),
    };
    var originalIssues = new List<VerificationIssue>();
    var original = DiscVerifier.ScanPtsSectors(
        sectors.Select(sector => (ReadOnlyMemory<byte>)sector), rows, 1, originalIssues);
    var observations = new DiscVerifier.AuditPtsObservation();
    var enumerations = 0;
    IEnumerable<ReadOnlyMemory<byte>> Chunks()
    {
        enumerations++;
        yield return sectors.Take(2).SelectMany(sector => sector).ToArray();
        yield return sectors.Skip(2).SelectMany(sector => sector).ToArray();
    }
    var timeline = AobPtsAnalyzer.AnalyzeChunks(Chunks(), "shared", null, observations.Observe);
    var sharedIssues = new List<VerificationIssue>();
    observations.AddIssues(rows, 1, sharedIssues);
    Equal(1, enumerations);
    Equal(3, original.SectorCount);
    Equal(5, timeline.SectorCount);
    True(originalIssues.SequenceEqual(sharedIssues),
        "共享扫描应保持审计诊断代码、消息及顺序，且缺失 PTS 后继续时间轴扫描");
    Equal(4, timeline.PtsSectorCount);
}


static void ReuseValidatedSurcodeOutput()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-surcode-reuse", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    SetFixtureCallLogDirectory(root);
    try
    {
        var probe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var external = Path.Combine(root, "external");
        var sourceRoot = Path.Combine(root, "source");
        Directory.CreateDirectory(external);
        Directory.CreateDirectory(sourceRoot);
        var config = Path.Combine(root, "config.env");
        File.WriteAllText(config, string.Join('\n',
        [
            $"DVDA_SRC={sourceRoot.Replace('\\', '/')}",
            $"DVDA_FINAL_DIR={Path.Combine(root, "final").Replace('\\', '/')}",
            $"DVDA_BUILD_DIR={Path.Combine(root, "build").Replace('\\', '/')}",
            $"DVDA_MLP_EXTERNAL_DIR={external.Replace('\\', '/')}",
            $"DVDA_FFPROBE={probe.Replace('\\', '/')}",
            "DVDA_MLP_SOURCE=surcode-batch",
        ]));
        var options = Load(config);
        var source = Path.Combine(sourceRoot, "song.flac");
        var output = Path.Combine(external, "song.mlp");
        File.WriteAllBytes(source, [1, 2, 3]);
        File.WriteAllBytes(output, BuildValidMlpFixture());
        var track = BuildTrack("Song", "1", "1", 0) with { SourcePath = source };
        var provider = new ExternalMlpProvider(options, new ProcessRunner());
        var identity = FileIdentityProbe.Compute(output)!;
        var verified = new Dictionary<string, FileIdentity>(StringComparer.OrdinalIgnoreCase)
        {
            [Path.GetFullPath(output)] = identity,
        };

        var reused = provider.AcquireAsync([track], verified).GetAwaiter().GetResult();
        Equal(0, reused.Diagnostics.Count);
        Equal(2, FixtureCallCount(root, "ffprobe"));
        Equal(Path.GetFullPath(output), Path.GetFullPath(reused.Tracks[0].MlpPath));

        // 修改后的文件必须回退到完整结构检查，而非信任旧凭据。
        var changed = File.ReadAllBytes(output);
        changed[0] = 0;
        File.WriteAllBytes(output, changed);
        File.SetLastWriteTimeUtc(output, new DateTime(identity.LastWriteUtcTicks, DateTimeKind.Utc));
        var rejected = provider.AcquireAsync([track], verified).GetAwaiter().GetResult();
        True(rejected.Diagnostics.Any(diagnostic => diagnostic.Code == "EXTERNAL_MLP_INVALID"),
            "凭据失效后必须重新完整检查并拒绝损坏的 MLP");
        Equal(2, FixtureCallCount(root, "ffprobe"));
    }
    finally
    {
        SetFixtureCallLogDirectory(null);
        Directory.Delete(root, recursive: true);
    }
}


static void MlpCacheIndexStorage()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-mlp-index", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var mlpDirectory = Path.Combine(root, "mlp");
        Directory.CreateDirectory(mlpDirectory);
        var source = Path.Combine(root, "song.flac");
        var output = Path.Combine(mlpDirectory, "song.mlp");
        File.WriteAllBytes(source, Enumerable.Repeat((byte)0x33, 4096).ToArray());
        File.WriteAllBytes(output, BuildValidMlpFixture());

        var sourceIdentity = FileIdentityProbe.Compute(source)!;
        var outputIdentity = FileIdentityProbe.Compute(output)!;
        var index = MlpCacheIndex.Load(MlpCacheIndex.PathFor(mlpDirectory));
        Equal(0, index.Count);
        index.Record(output, new MlpCacheEntry
        {
            Source = sourceIdentity,
            Output = outputIdentity,
            Encoder = "mlpencoder|1",
            Bits = 24,
            ResampleTo = null,
            MaxInterval = MlpCacheValidator.RequiredMajorSyncInterval,
        });
        index.Save(MlpCacheIndex.PathFor(mlpDirectory));

        var reloaded = MlpCacheIndex.Load(MlpCacheIndex.PathFor(mlpDirectory));
        Equal(1, reloaded.Count);
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|1", 24, null,
            MlpCacheValidator.RequiredMajorSyncInterval) is not null,
            "凭据一致时应命中");
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|2", 24, null,
            MlpCacheValidator.RequiredMajorSyncInterval) is null,
            "编码器身份变化时不得命中");
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|1", 16, null,
            MlpCacheValidator.RequiredMajorSyncInterval) is null,
            "位深变化时不得命中");
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|1", 24, 44_100,
            MlpCacheValidator.RequiredMajorSyncInterval) is null,
            "重采样目标变化时不得命中");
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|1", 24, null, 4) is null,
            "major sync 间隔变化时不得命中");
        True(reloaded.Match(output, new FileIdentity(source, 1, 0, "a", "b"), "mlpencoder|1", 24, null,
            MlpCacheValidator.RequiredMajorSyncInterval) is null,
            "源身份变化时不得命中");

        // 输出文件被改动后必须重新编码。
        File.WriteAllBytes(output, Enumerable.Repeat((byte)0x44, 4096).ToArray());
        True(reloaded.Match(output, sourceIdentity, "mlpencoder|1", 24, null,
            MlpCacheValidator.RequiredMajorSyncInterval) is null,
            "MLP 被改动后不得命中");

        // 损坏的索引文件不得抛异常，也不得返回陈旧凭据。
        var indexPath = MlpCacheIndex.PathFor(mlpDirectory);
        File.WriteAllText(indexPath, "{ not json");
        Equal(0, MlpCacheIndex.Load(indexPath).Count);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static int CountOccurrences(string text, string value)
{
    var count = 0;
    var index = 0;
    while ((index = text.IndexOf(value, index, StringComparison.Ordinal)) >= 0)
    {
        count++;
        index += value.Length;
    }
    return count;
}

static void WriteBuildManifest(
    string path,
    params (string Album, string Title, string Source, string Name)[] tracks)
{
    var directory = Path.GetDirectoryName(path);
    if (!string.IsNullOrEmpty(directory))
    {
        Directory.CreateDirectory(directory);
    }
    var group = new ManifestGroup
    {
        SampleRate = 48_000,
        Bits = 24,
        Count = tracks.Length,
        Files = tracks.Select((track, index) => new ManifestTrack
        {
            Number = index + 1,
            Source = track.Source.Replace('\\', '/'),
            Name = track.Name,
            Title = track.Title,
            Date = "2026",
            Track = (index + 1).ToString(),
            Album = track.Album,
            Duration = 1,
        }).ToArray(),
    };
    var manifest = new Dictionary<string, ManifestGroup> { ["group_48000_24"] = group };
    File.WriteAllText(path, System.Text.Json.JsonSerializer.Serialize(
        manifest, new System.Text.Json.JsonSerializerOptions { WriteIndented = true }));
}

static void DiscResumeStoreRules()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-resume-store", Guid.NewGuid().ToString("N"));
    var directory = Path.Combine(root, "staging");
    Directory.CreateDirectory(directory);
    try
    {
        var store = DiscResumeStore.Load(directory);
        Equal(0, store.Count);

        // 没有暂存 ISO 时记录应被忽略。
        store.Record(1, "sig", "Disc_1.iso");
        Equal(0, store.Count);

        var isoPath = Path.Combine(directory, "Disc_1.iso");
        File.WriteAllBytes(isoPath, [1, 2, 3, 4]);
        store.Record(1, "sig", "Disc_1.iso");
        Equal(1, store.Count);
        store.Save();

        var reloaded = DiscResumeStore.Load(directory);
        Equal(1, reloaded.Count);
        True(reloaded.TryReuse(1, "sig", "Disc_1.iso") is not null, "签名与产物一致时应可复用");
        True(reloaded.TryReuse(1, "other", "Disc_1.iso") is null, "签名不同不得复用");
        True(reloaded.TryReuse(2, "sig", "Disc_1.iso") is null, "无记录的盘号不得复用");

        // 同长度、同时间但产物变化：不得复用。
        var lastWrite = File.GetLastWriteTimeUtc(isoPath);
        File.WriteAllBytes(isoPath, [9, 9, 9, 4]);
        File.SetLastWriteTimeUtc(isoPath, lastWrite);
        True(DiscResumeStore.Load(directory).TryReuse(1, "sig", "Disc_1.iso") is null,
            "暂存产物变化后不得复用");

        // Discard 同时清理产物与记录。
        File.WriteAllBytes(isoPath, [1, 2, 3, 4]);
        var third = DiscResumeStore.Load(directory);
        third.Record(1, "sig", "Disc_1.iso");
        third.Save();
        var fourth = DiscResumeStore.Load(directory);
        fourth.Discard(1, "Disc_1.iso");
        Equal(0, fourth.Count);
        False(File.Exists(isoPath), "Discard 应删除暂存产物");

        // 损坏的续跑记录不得抛异常。
        File.WriteAllText(
            Path.Combine(directory, DiscResumeStore.IndexFileName), "{ broken");
        Equal(0, DiscResumeStore.Load(directory).Count);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void DiscSignatureChanges()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-signature", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var mlp = Path.Combine(root, "a.mlp");
        File.WriteAllBytes(mlp, BuildValidMlpFixture());
        var configOne = Path.Combine(root, "one.env");
        var configTwo = Path.Combine(root, "two.env");
        File.WriteAllText(configOne, string.Join('\n',
        [
            $"DVDA_SRC={root}/src",
            $"DVDA_FINAL_DIR={root}/final",
            $"DVDA_BUILD_DIR={root}/build",
            $"DVDA_AUTHOR_SRC={root}/data",
            "DVDA_TITLE=Title One",
        ]));
        File.WriteAllText(configTwo, string.Join('\n',
        [
            $"DVDA_SRC={root}/src",
            $"DVDA_FINAL_DIR={root}/final",
            $"DVDA_BUILD_DIR={root}/build",
            $"DVDA_AUTHOR_SRC={root}/data",
            "DVDA_TITLE=Title Two",
        ]));
        var track = BuildTrack("Track", "1", "1", 100) with { MlpPath = mlp };
        var disc = new DiscPlanner().Plan([track], ConfigDefaults.Dvd5Bytes, 1, 70).Discs.Single();
        var runner = new ProcessRunner();

        var first = DiscSignature.ComputeAsync(Load(configOne), disc, runner)
            .GetAwaiter().GetResult();
        var second = DiscSignature.ComputeAsync(Load(configOne), disc, runner)
            .GetAwaiter().GetResult();
        Equal(first, second);

        var other = DiscSignature.ComputeAsync(Load(configTwo), disc, runner)
            .GetAwaiter().GetResult();
        True(!string.Equals(first, other, StringComparison.Ordinal), "标题变化应改变签名");

        var lastWrite = File.GetLastWriteTimeUtc(mlp);
        var data = File.ReadAllBytes(mlp);
        data[100] ^= 0xFF;
        File.WriteAllBytes(mlp, data);
        File.SetLastWriteTimeUtc(mlp, lastWrite);
        var changed = DiscSignature.ComputeAsync(Load(configOne), disc, runner)
            .GetAwaiter().GetResult();
        True(!string.Equals(first, changed, StringComparison.Ordinal),
            "MLP 内容变化应改变签名");
        var libraries = Path.Combine(root, "menu-bin");
        Directory.CreateDirectory(libraries);
        var library = Path.Combine(libraries, "dvda-menu-nav.dll");
        File.WriteAllBytes(library, [1, 2, 3, 4]);
        var nativeBefore = DiscSignature.ComputeAsync(Load(configOne), disc, runner)
            .GetAwaiter().GetResult();
        True(nativeBefore != changed, "新增原生模块必须使续跑签名失效");
        var libraryTime = File.GetLastWriteTimeUtc(library);
        File.WriteAllBytes(library, [1, 2, 3, 5]);
        File.SetLastWriteTimeUtc(library, libraryTime);
        var nativeAfter = DiscSignature.ComputeAsync(Load(configOne), disc, runner)
            .GetAwaiter().GetResult();
        True(nativeAfter != nativeBefore, "同长度同时间戳的模块内容变化必须使续跑签名失效");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void BuildPipelineResumesDiscs()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-resume-build", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    SetFixtureCallLogDirectory(root);
    try
    {
        var author = CreateFixtureExecutable(root, "fake-dvda-author.exe");
        var mkisofs = CreateFixtureExecutable(root, "fake-mkisofs.exe");
        var ffprobe = CreateFixtureExecutable(root, "fake-ffprobe.exe");
        var sourceRoot = Path.Combine(root, "src");
        var externalRoot = Path.Combine(root, "external");
        var externalMlpA = Path.Combine(externalRoot, "AlbumA", "a.mlp");
        var externalMlpB = Path.Combine(externalRoot, "AlbumB", "b.mlp");
        foreach (var path in new[] { externalMlpA, externalMlpB })
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllBytes(path, BuildValidMlpFixture());
        }
        var sourceA = Path.Combine(sourceRoot, "AlbumA", "a.flac");
        var sourceB = Path.Combine(sourceRoot, "AlbumB", "b.flac");
        foreach (var path in new[] { sourceA, sourceB })
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllBytes(path, Enumerable.Repeat((byte)0x55, 1024).ToArray());
        }

        var buildDirectory = Path.Combine(root, "build");
        var config = Path.Combine(root, "config.env");
        File.WriteAllText(config, string.Join('\n',
        [
            $"DVDA_SRC={sourceRoot.Replace('\\', '/')}",
            $"DVDA_FINAL_DIR={Path.Combine(root, "final").Replace('\\', '/')}",
            $"DVDA_BUILD_DIR={buildDirectory.Replace('\\', '/')}",
            $"DVDA_AUTHOR={author.Replace('\\', '/')}",
            $"DVDA_MKISOFS={mkisofs.Replace('\\', '/')}",
            $"DVDA_FFPROBE={ffprobe.Replace('\\', '/')}",
            "DVDA_MLP_SOURCE=external",
            $"DVDA_MLP_EXTERNAL_DIR={externalRoot.Replace('\\', '/')}",
            "DVDA_TITLE=Resume Fixture",
            "DVDA_MENU=off",
            "DVDA_MAX_DISCS=2",
            // 单盘容量小于安全余量时每个专辑各占一张盘。
            "DVDA_DISC_BYTES=1000000",
            "DVDA_RESUME=on",
        ]));
        var options = Load(config);
        WriteBuildManifest(
            options.ManifestPath,
            ("AlbumA", "A", sourceA, "group_48000_24/0001__a"),
            ("AlbumB", "B", sourceB, "group_48000_24/0002__b"));

        // 第一次：第 2 盘故意失败，第 1 盘的暂存 ISO 与续跑记录应保留。
        Environment.SetEnvironmentVariable("DVDA_FIXTURE_FAIL_DISC", "disc2");
        var first = new BuildPipeline(options).RunAsync(false).GetAwaiter().GetResult();
        Equal(2, first.DiscResults.Count);
        True(first.DiscResults[0].Succeeded, "第 1 盘应成功");
        False(first.DiscResults[1].Succeeded, "第 2 盘应失败");
        Equal(string.Empty, first.IndexPath);
        True(FixtureCallCount(root, "author") == 2,
            $"第一次应出盘两次，实际 {FixtureCallCount(root, "author")}");
        var stagingDirectory = DiscResumeStore.DirectoryFor(options);
        True(File.Exists(Path.Combine(stagingDirectory, options.IsoName(1))),
            "失败的运行应保留第 1 盘暂存 ISO");
        True(File.Exists(Path.Combine(stagingDirectory, DiscResumeStore.IndexFileName)),
            "失败的运行应写出续跑记录");

        // 第二次：MLP 内容变化（长度与时间不变）后签名失效，第 1 盘必须重新出盘。
        var lastWrite = File.GetLastWriteTimeUtc(externalMlpA);
        var mlpBytes = File.ReadAllBytes(externalMlpA);
        mlpBytes[100] ^= 0xFF;
        File.WriteAllBytes(externalMlpA, mlpBytes);
        File.SetLastWriteTimeUtc(externalMlpA, lastWrite);
        var second = new BuildPipeline(options).RunAsync(false).GetAwaiter().GetResult();
        Equal(2, second.DiscResults.Count);
        False(second.DiscResults[1].Succeeded, "第 2 盘仍应失败");
        True(FixtureCallCount(root, "author") == 4,
            $"签名失效后第 1 盘应重新出盘，实际 {FixtureCallCount(root, "author")}");

        // 第三次：签名恢复一致，第 1 盘应直接复用暂存 ISO。
        Environment.SetEnvironmentVariable("DVDA_FIXTURE_FAIL_DISC", null);
        var third = new BuildPipeline(options).RunAsync(false).GetAwaiter().GetResult();
        Equal(2, third.DiscResults.Count);
        True(third.DiscResults.All(result => result.Succeeded), "第三次应全部成功");
        Equal(options.MlpIndexPath, third.IndexPath);
        True(FixtureCallCount(root, "author") == 5,
            $"第三次只应为第 2 盘出盘，实际 {FixtureCallCount(root, "author")}");
        True(CountOccurrences(File.ReadAllText(options.BuildLogPath), "[恢复] 第 1 盘") == 1,
            "第三次应复用第 1 盘暂存 ISO");
        True(File.Exists(Path.Combine(options.FinalDirectory, options.IsoName(1))),
            "第 1 盘应发布到成品目录");
        True(File.Exists(Path.Combine(options.FinalDirectory, options.IsoName(2))),
            "第 2 盘应发布到成品目录");
        False(Directory.Exists(stagingDirectory), "发布成功后应清理暂存目录");
    }
    finally
    {
        Environment.SetEnvironmentVariable("DVDA_FIXTURE_FAIL_DISC", null);
        SetFixtureCallLogDirectory(null);
        Directory.Delete(root, recursive: true);
    }
}


static byte[] BuildValidMlpFixture()
{
    var units = new List<byte[]>();
    for (var index = 0; index < 16; index++)
    {
        units.Add(BuildMlpFixtureUnit(
            majorSync: index % 8 == 0,
            endOfStream: index == 15));
    }
    return units.SelectMany(unit => unit).ToArray();
}

static byte[] BuildMlpFixtureUnit(bool majorSync, bool endOfStream)
{
    const int bodySize = 250;
    var body = new byte[bodySize];
    for (var index = 0; index < body.Length; index++)
    {
        body[index] = (byte)(index & 0xFF);
    }
    if (endOfStream)
    {
        body[^4] = 0xD2;
        body[^3] = 0x34;
        body[^2] = 0xD2;
        body[^1] = 0x34;
    }
    var majorSize = majorSync ? MlpStreamAligner.MajorSyncSize : 0;
    var substreamLength = body.Length + 2;
    var length = 4 + majorSize + 2 + substreamLength;
    var data = new byte[length];
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(data.AsSpan(2, 2), 0);

    if (majorSync)
    {
        var major = data.AsSpan(4, MlpStreamAligner.MajorSyncSize);
        major[0] = 0xF8;
        major[1] = 0x72;
        major[2] = 0x6F;
        major[5] = 0x00;
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(major[8..10], 0xB752);
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
            major[14..16], (ushort)MlpStreamAligner.PeakBitrateRaw(48_000));
        major[16] = 1;
        var checksum = MlpStreamAligner.Checksum16(major, MlpStreamAligner.MajorSyncSize - 2);
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16LittleEndian(major[26..28], checksum);
    }

    var substreamOffset = 4 + majorSize;
    var substreamHeader = (ushort)(substreamLength / 2);
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
        data.AsSpan(substreamOffset, 2), substreamHeader);
    body.CopyTo(data.AsSpan(substreamOffset + 2));

    var lengthWords = length / 2;
    var parity = lengthWords ^ (substreamHeader >> 8) ^ (substreamHeader & 0xFF);
    parity ^= parity >> 8;
    parity ^= parity >> 4;
    parity &= 0xF;
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(
        data.AsSpan(0, 2), (ushort)(((parity ^ 0xF) << 12) | lengthWords));
    return data;
}

static DvdaOptions LoadSpaceOptions(
    string buildDirectory,
    string finalDirectory,
    bool keepIntermediate,
    bool menu,
    string mlpSource)
{
    var directory = Path.Combine(
        Path.GetTempPath(), "dvda-space-config", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(directory);
    var config = Path.Combine(directory, "config.env");
    File.WriteAllText(config, string.Join('\n',
    [
        "DVDA_SRC=D:/fixture-src",
        $"DVDA_FINAL_DIR={finalDirectory}",
        $"DVDA_BUILD_DIR={buildDirectory}",
        $"DVDA_MLP_SOURCE={mlpSource}",
        $"DVDA_KEEP_INTERMEDIATE={(keepIntermediate ? "on" : "off")}",
        $"DVDA_MENU={(menu ? "on" : "off")}",
    ]));
    return Load(config);
}

static void EstimateDiskSpaceRequirements()
{
    var temp = Path.Combine(Path.GetTempPath(), "dvda-space", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(temp);
    try
    {
        var buildDirectory = Path.Combine(temp, "build");
        var finalSameVolume = Path.Combine(temp, "final");
        const string finalOtherVolume = @"\\dvda-fixture\share\final";
        var tracks = new[] { BuildTrack("Track", "1", "1", 1_000_000) };
        var discs = new[] { SpaceDisc(1, 1_000_000), SpaceDisc(2, 2_000_000) };
        var isoBytes = discs.Sum(disc => disc.EstimatedAobBytes);

        // 同卷：中间产物与成品集合合并统计。
        var sameVolume = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalSameVolume, false, false, "external"),
            tracks, discs, freeSpace: _ => 1L << 40);
        Equal(1, sameVolume.Count);
        Equal(2 * isoBytes, sameVolume[0].RequiredBytes);
        True(sameVolume[0].IsSufficient == true, "空间充足不应报不足");
        Equal(0, DiskSpacePlanner.Evaluate(sameVolume).Count);

        // 跨卷：构建卷与成品卷分别统计。
        var crossVolume = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalOtherVolume, false, false, "external"),
            tracks, discs, freeSpace: _ => 1L << 40);
        Equal(2, crossVolume.Count);
        Equal(isoBytes, crossVolume.Single(item =>
            item.Root.Contains("dvda-fixture", StringComparison.OrdinalIgnoreCase)).RequiredBytes);
        Equal(isoBytes, crossVolume.Single(item =>
            item.Root.StartsWith(Path.GetPathRoot(temp)!, StringComparison.OrdinalIgnoreCase))
            .RequiredBytes);

        // 保留中间产物 + 菜单：额外一份 ISO 与固定余量。
        var keep = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalSameVolume, true, true, "external"),
            tracks, discs, freeSpace: _ => 1L << 40);
        Equal(1, keep.Count);
        Equal(3 * isoBytes + DiskSpacePlanner.MenuSafetyBytes, keep[0].RequiredBytes);

        // ffmpeg 模式且 MLP 缺失：按源大小预留编码输出。
        var missingMlp = new[]
        {
            BuildTrack("Track", "1", "1", 0) with { SourceSize = 5_000_000 },
        };
        var encoded = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalOtherVolume, false, false, "surcode-batch"),
            missingMlp, discs, freeSpace: _ => 1L << 40);
        Equal(isoBytes + 5_000_000, encoded.Single(item =>
            item.Root.StartsWith(Path.GetPathRoot(temp)!, StringComparison.OrdinalIgnoreCase))
            .RequiredBytes);

        // 空间不足：产生警告而不是错误。
        var tight = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalSameVolume, false, false, "external"),
            tracks, discs, freeSpace: _ => 1);
        var warnings = DiskSpacePlanner.Evaluate(tight);
        Equal(tight.Count, warnings.Count);
        True(warnings.All(item =>
                item.Code == "DISK_SPACE_LOW" &&
                item.Severity == BuildDiagnosticSeverity.Warning),
            "空间不足应只产生警告");
        True(DiskSpacePlanner.Describe(tight).Contains("需要", StringComparison.Ordinal),
            "空间描述应包含需求量");

        // 无法得知可用空间时不得误报。
        var unknown = DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalSameVolume, false, false, "external"),
            tracks, discs, freeSpace: _ => null);
        True(unknown.All(item => item.IsSufficient is null), "未知可用空间应为未知状态");
        Equal(0, DiskSpacePlanner.Evaluate(unknown).Count);
        True(DiskSpacePlanner.AvailableBytes(@"\\dvda-nonexistent\share\") is null,
            "无法访问的卷应返回未知可用空间");

        // 外部 MLP 模式且没有分盘计划时无需估算。
        Equal(0, DiskSpacePlanner.Estimate(
            LoadSpaceOptions(buildDirectory, finalSameVolume, false, false, "external"),
            tracks, discs: null, freeSpace: _ => 1).Count);
    }
    finally
    {
        Directory.Delete(temp, recursive: true);
    }
}

static void ComparePcmFixtures()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-pcm", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var source = Path.Combine(root, "source.raw");
        var same = Path.Combine(root, "same.raw");
        var longer = Path.Combine(root, "longer.raw");
        var shorter = Path.Combine(root, "shorter.raw");
        var tailDiffers = Path.Combine(root, "tail.raw");
        File.WriteAllBytes(source, [1, 2, 3, 4, 5, 6, 7, 8]);
        File.WriteAllBytes(same, [1, 2, 3, 4, 5, 6, 7, 8]);
        File.WriteAllBytes(longer, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        File.WriteAllBytes(shorter, [1, 2, 3, 4, 5, 6, 7]);
        File.WriteAllBytes(tailDiffers, [1, 2, 3, 4, 5, 6, 7, 9]);

        var match = PcmComparer.Compare(source, same);
        True(match.Match, "等长且内容一致应判定一致");
        Equal(8L, match.SourceBytes);
        Equal(8L, match.DecodedBytes);
        True(match.Reason is null, "一致时不应给出原因");

        var extended = PcmComparer.Compare(source, longer);
        False(extended.Match, "解码结果比源更长必须判为不一致");
        Equal(9L, extended.DecodedBytes);
        True(extended.Reason!.Contains("相差 1", StringComparison.Ordinal),
            $"长度不一致应说明差值: {extended.Reason}");

        // 仅显式允许完整的零采样帧；默认比较仍须等长。
        var padded = Path.Combine(root, "padded.raw");
        var nonzeroPadding = Path.Combine(root, "nonzero-padding.raw");
        var excessivePadding = Path.Combine(root, "excessive-padding.raw");
        var changedPrefix = Path.Combine(root, "changed-prefix.raw");
        var partialFrame = Path.Combine(root, "partial-frame.raw");
        File.WriteAllBytes(padded, [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0]);
        File.WriteAllBytes(nonzeroPadding, [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 1, 0]);
        File.WriteAllBytes(excessivePadding, [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0]);
        File.WriteAllBytes(changedPrefix, [1, 2, 3, 4, 5, 6, 7, 9, 0, 0, 0, 0]);
        File.WriteAllBytes(partialFrame, [1, 2, 3, 4, 5, 6, 7, 8, 0]);
        False(PcmComparer.Compare(source, padded).Match, "默认必须拒绝零尾");
        var allowed = PcmComparer.Compare(source, padded, 2, 2);
        True(allowed.Match, "有界完整零采样帧可以接受");
        Equal(4L, allowed.TrailingZeroBytes);
        False(PcmComparer.Compare(source, nonzeroPadding, 2, 2).Match,
            "非零填充不能通过");
        False(PcmComparer.Compare(source, excessivePadding, 2, 2).Match,
            "超出上限不能通过");
        False(PcmComparer.Compare(source, changedPrefix, 2, 2).Match,
            "源 PCM 内容变化不能通过");
        False(PcmComparer.Compare(source, partialFrame, 2, 2).Match,
            "不足完整采样帧不能通过");
        False(PcmComparer.Compare(source, shorter, 2, 2).Match,
            "截短不能通过宽容比较");

        // 48 kHz、双声道、24-bit：实际遇到的 29 帧零尾应通过，
        // 但恰好 1 ms（48 帧）必须拒绝。
        var stereoSource = Path.Combine(root, "stereo-source.raw");
        var surcodeTail = Path.Combine(root, "surcode-tail.raw");
        var millisecondTail = Path.Combine(root, "millisecond-tail.raw");
        var stereoSamples = new byte[] { 1, 2, 3, 4, 5, 6 };
        File.WriteAllBytes(stereoSource, stereoSamples);
        File.WriteAllBytes(surcodeTail, [.. stereoSamples, .. new byte[29 * 6]]);
        File.WriteAllBytes(millisecondTail, [.. stereoSamples, .. new byte[48 * 6]]);
        var surcodeResult = PcmComparer.Compare(stereoSource, surcodeTail, 6, 47);
        True(surcodeResult.Match, "SurCode 29 帧零尾应通过");
        Equal(174L, surcodeResult.TrailingZeroBytes);
        False(PcmComparer.Compare(stereoSource, millisecondTail, 6, 47).Match,
            "整 1 ms 的零尾不能通过");

        var truncated = PcmComparer.Compare(source, shorter);
        False(truncated.Match, "解码结果比源更短必须判为不一致");

        var tail = PcmComparer.Compare(source, tailDiffers);
        False(tail.Match, "同长但尾部不同必须判为不一致");
        True(tail.Reason!.Contains("偏移 7", StringComparison.Ordinal),
            $"内容不一致应报告首个不同偏移: {tail.Reason}");

        // 跨越 128 KiB 缓冲边界的差异，必须定位到真实偏移。
        var large = new byte[300 * 1024];
        var largeCopy = (byte[])large.Clone();
        largeCopy[200_000] = 0xFF;
        var largeSource = Path.Combine(root, "large.raw");
        var largeDecoded = Path.Combine(root, "large-decoded.raw");
        File.WriteAllBytes(largeSource, large);
        File.WriteAllBytes(largeDecoded, largeCopy);
        var largeResult = PcmComparer.Compare(largeSource, largeDecoded);
        False(largeResult.Match, "跨缓冲区差异必须判为不一致");
        True(largeResult.Reason!.Contains(200_000.ToString("N0"), StringComparison.Ordinal),
            $"跨缓冲区应报告真实偏移: {largeResult.Reason}");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void BuildScriptBranching()
{
    var scenarios = new (string[] Arguments, string[] Expected)[]
    {
        ([], ["prepare", "build"]),
        (["--dry-run"], ["prepare", "build --dry-run"]),
        (["--config", "{config}"], ["prepare --config", "build --config"]),
        (["--dry-run", "--config", "{config}"],
            ["prepare --config", "build --dry-run --config"]),
        (["--config", "{config}", "--dry-run"],
            ["prepare --config", "build --dry-run --config"]),
    };

    foreach (var scenario in scenarios)
    {
        var outcome = RunBuildScript(scenario.Arguments, stubExitCode: 0);
        if (outcome is null)
        {
            return;
        }
        var (exitCode, calls) = outcome.Value;
        Equal(0, exitCode);
        Equal(scenario.Expected.Length, calls.Count);
        for (var index = 0; index < scenario.Expected.Length; index++)
        {
            True(calls[index].Contains(scenario.Expected[index], StringComparison.Ordinal),
                $"build.cmd 第 {index + 1} 次调用应包含 “{scenario.Expected[index]}”，" +
                $"实际: {calls[index]}");
        }
    }
}

static void BuildScriptStopsAfterPrepareFailure()
{
    var outcome = RunBuildScript([], stubExitCode: 3);
    if (outcome is null)
    {
        return;
    }
    var (exitCode, calls) = outcome.Value;
    Equal(1, exitCode);
    Equal(1, calls.Count);
    True(calls[0].Contains("prepare", StringComparison.Ordinal),
        "准备失败时只应调用 prepare");
}

static (int ExitCode, IReadOnlyList<string> Calls)? RunBuildScript(
    IReadOnlyList<string> arguments,
    int stubExitCode)
{
    const string configToken = "{config}";
    if (!OperatingSystem.IsWindows())
    {
        Console.WriteLine("  (跳过: 该用例需要 Windows cmd.exe)");
        return null;
    }
    var root = FindRepositoryRoot();
    if (root is null)
    {
        Console.WriteLine("  (跳过: 未找到含 build.cmd 的仓库根目录)");
        return null;
    }

    var work = Path.Combine(Path.GetTempPath(), "dvda-buildcmd", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(work);
    try
    {
        var log = Path.Combine(work, "calls.txt");
        File.WriteAllText(Path.Combine(work, "dotnet.cmd"),
            "@echo off" + Environment.NewLine +
            "echo %* >> \"%~dp0calls.txt\"" + Environment.NewLine +
            $"exit /b {stubExitCode}" + Environment.NewLine);
        var config = Path.Combine(work, "config.env");
        File.WriteAllText(config, "DVDA_TITLE=fixture" + Environment.NewLine);
        var effective = arguments
            .Select(argument => argument == configToken ? config : argument)
            .ToArray();

        var request = new ProcessRequest
        {
            FileName = "cmd.exe",
            Arguments = new[] { "/d", "/c", Path.Combine(root, "build.cmd") }
                .Concat(effective).ToArray(),
            WorkingDirectory = work,
            Environment = new Dictionary<string, string?>(StringComparer.OrdinalIgnoreCase)
            {
                ["PATH"] = work + Path.PathSeparator +
                    (Environment.GetEnvironmentVariable("PATH") ?? string.Empty),
            },
        };
        var result = new ProcessRunner().RunAsync(request).GetAwaiter().GetResult();
        IReadOnlyList<string> calls = File.Exists(log)
            ? File.ReadAllLines(log).Where(line => line.Trim().Length > 0).ToArray()
            : [];
        True(result.ExitCode == stubExitCode || calls.Count > 0,
            $"build.cmd 未按预期调用 dotnet: {result.StandardError}");
        return (result.ExitCode, calls);
    }
    finally
    {
        Directory.Delete(work, recursive: true);
    }
}

static string? FindRepositoryRoot()
{
    var directory = new DirectoryInfo(AppContext.BaseDirectory);
    while (directory is not null)
    {
        if (File.Exists(Path.Combine(directory.FullName, "build.cmd")) &&
            File.Exists(Path.Combine(directory.FullName, "DVD-Audio-Maker.sln")))
        {
            return directory.FullName;
        }
        directory = directory.Parent;
    }
    return null;
}

static void BuildDiscEndToEnd()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-build-e2e", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var author = CreateFixtureExecutable(root, "fake-dvda-author.exe");
        var mkisofs = CreateFixtureExecutable(root, "fake-mkisofs.exe");
        var options = CreateExecutorOptions(root, author, mkisofs);
        var mlp = Path.Combine(root, "track.mlp");
        File.WriteAllBytes(mlp, [1, 2, 3]);
        var track = BuildTrack("Track", "1", "1", 3, album: "Album") with
        {
            MlpPath = mlp,
        };
        var disc = new DiscPlanner()
            .Plan([track], ConfigDefaults.Dvd5Bytes, 1, 70)
            .Discs.Single();

        DiscBuildResult result;
        using (var log = new BuildLogWriter(options, dryRun: false))
        {
            result = new DiscBuildExecutor(options, new ProcessRunner(), log)
                .BuildAsync(disc).GetAwaiter().GetResult();
        }

        True(result.Succeeded, "假 author/mkisofs 正常时正式出盘应成功");
        True(File.Exists(result.PublishedIsoPath), "最终 ISO 应发布到目标目录");
        Equal(4L, new FileInfo(result.PublishedIsoPath).Length);
        False(File.Exists(result.IntermediateIsoPath), "发布成功后应清理中间 ISO");
        False(Directory.Exists(Path.Combine(options.OutputRoot, "disc1")),
            "发布成功后应清理 author 输出目录");
        False(Directory.Exists(Path.Combine(options.TemporaryRoot, "disc1")),
            "发布成功后应清理临时目录");

        var logText = File.ReadAllText(options.BuildLogPath);
        True(logText.Contains("fake-dvda-author", StringComparison.OrdinalIgnoreCase),
            "日志应记录 dvda-author 命令");
        False(logText.Contains("fake-mkisofs", StringComparison.OrdinalIgnoreCase),
            "日志不应再记录外部 mkisofs 命令");
        True(logText.Contains("ISO writer", StringComparison.OrdinalIgnoreCase),
            "日志应说明使用内置 ISO 写入器");
        True(logText.Contains("1  1/1  1  0  99  0  90000  0"),
            "日志应保留 author 轨道表供审计解析");
        True(logText.Contains("[耗时] 第 1 盘总耗时:", StringComparison.Ordinal),
            "日志应记录按盘总耗时");
        True(logText.Contains("[结果] 第 1 盘:", StringComparison.Ordinal),
            "日志应记录按盘结果行");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void PreserveFailedDiscWorkspace()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-build-failure", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    try
    {
        var author = CreateFixtureExecutable(root, "fake-dvda-author-fail.exe");
        var mkisofs = CreateFixtureExecutable(root, "fake-mkisofs.exe");
        var options = CreateExecutorOptions(root, author, mkisofs);
        var mlp = Path.Combine(root, "track.mlp");
        File.WriteAllBytes(mlp, [1]);
        var track = BuildTrack("Track", "1", "1", 1) with { MlpPath = mlp };
        var disc = new DiscPlanner()
            .Plan([track], ConfigDefaults.Dvd5Bytes, 1, 70)
            .Discs.Single();

        DiscBuildResult result;
        using (var log = new BuildLogWriter(options, dryRun: false))
        {
            result = new DiscBuildExecutor(options, new ProcessRunner(), log)
                .BuildAsync(disc).GetAwaiter().GetResult();
        }

        False(result.Succeeded, "author 非零退出时正式出盘必须失败");
        True(result.Diagnostics.Any(item => item.Code == "DVDA_AUTHOR_FAILED"),
            "失败结果应包含 DVDA_AUTHOR_FAILED");
        True(Directory.Exists(Path.Combine(options.OutputRoot, "disc1")),
            "author 失败时应保留输出目录用于排查");
        True(Directory.Exists(Path.Combine(options.TemporaryRoot, "disc1")),
            "author 失败时应保留临时目录用于排查");
        False(File.Exists(Path.Combine(options.FinalDirectory, options.IsoName(1))),
            "author 失败时不得发布 ISO");
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static void StageDiscAndKeepIntermediate()
{
    foreach (var keepIntermediate in new[] { false, true })
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-stage-executor", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var author = CreateFixtureExecutable(root, "fake-dvda-author.exe");
            var mkisofs = CreateFixtureExecutable(root, "fake-mkisofs.exe");
            var options = CreateExecutorOptions(root, author, mkisofs, keepIntermediate);
            var mlp = Path.Combine(root, "track.mlp");
            File.WriteAllBytes(mlp, [1, 2, 3]);
            var track = BuildTrack("Track", "1", "1", 3, album: "Album") with
            {
                MlpPath = mlp,
            };
            var disc = new DiscPlanner().Plan([track], ConfigDefaults.Dvd5Bytes, 1, 70)
                .Discs.Single();
            var stage = Path.Combine(options.BuildDirectory, "publish-staging", "fixture");
            DiscBuildResult result;
            using (var log = new BuildLogWriter(options, dryRun: false))
            {
                result = new DiscBuildExecutor(options, new ProcessRunner(), log)
                    .BuildAsync(disc, stageForTransactionalPublication: true,
                        publishDirectory: stage).GetAwaiter().GetResult();
            }

            True(result.Succeeded, "事务暂存执行器应成功");
            Equal(Path.Combine(stage, options.IsoName(1)), result.PublishedIsoPath);
            Equal(4L, new FileInfo(result.PublishedIsoPath).Length);
            Equal(keepIntermediate, File.Exists(result.IntermediateIsoPath));
            var logText = File.ReadAllText(options.BuildLogPath);
            True(logText.Contains("[耗时] 第 1 盘 ISO 暂存:", StringComparison.Ordinal),
                "应记录 ISO 暂存耗时");
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }
}

static void LoadMenuConfiguration()
{
    WithConfig(string.Join('\n',
    [
        "DVDA_SRC=/src",
        "DVDA_FINAL_DIR=/out",
        "DVDA_AUTHOR_SRC=/opt/dvda-author",
        "DVDA_MENU=on",
        "DVDA_MENU_TRACKS_PER_PAGE=99",
        "DVDA_MENU_INDEX_MIN_ALBUMS=-1",
        "DVDA_MENU_STILLPICS=off",
        "DVDA_MENU_COVER_DIM=150",
        "DVDA_MENU_FONT=C:/fonts/main.otf",
    ]), path =>
    {
        var options = Load(path);
        True(options.MenuEnabled, "DVDA_MENU=on 应启用菜单");
        Equal(32, options.MenuTracksPerPage);
        Equal(0, options.MenuIndexMinimumAlbums);
        False(options.MenuStillPictures, "DVDA_MENU_STILLPICS=off 应关闭静图");
        Equal(100, options.MenuCoverDim);
        Equal("C:/fonts/main.otf", options.MenuFont);
        Equal("/opt/menu-bin", options.MenuBinaryDirectory);
    });
}

static void PlanAlbumMenuPages()
{
    WithConfig(string.Join('\n',
    [
        "DVDA_SRC=/src",
        "DVDA_FINAL_DIR=/out",
        "DVDA_TITLE=Disc,Title",
        "DVDA_MENU_TRACKS_PER_PAGE=2",
        "DVDA_MENU_INDEX_MIN_ALBUMS=3",
    ]), path =>
    {
        var tracks = new[]
        {
            BuildTrack("A1", "1", "1", 1, "Album A") with { SourcePath = "/music/Album A/1.flac" },
            BuildTrack("A2", "1", "2", 1, "Album A") with { SourcePath = "/music/Album A/2.flac" },
            BuildTrack("A3", "1", "3", 1, "Album A") with { SourcePath = "/music/Album A/3.flac" },
            BuildTrack("B1", "2", "1", 1, "Album B") with { SourcePath = "/music/Album B/1.flac" },
            BuildTrack("C1", "3", "1", 1, "Album C") with { SourcePath = "/music/Album C/1.flac" },
        };
        var disc = new DiscPlan(1, [], [new AudioGroupPlan(1, 48_000, 24, tracks)]);
        var plan = MenuPlanner.Create(disc, Load(path));

        Equal(4, plan.AlbumPageCount);
        Equal(1, plan.IndexPages);
        Equal(5, plan.TotalPages);
        SequenceEqual(new[] { 2, 1, 1, 1 }, plan.AlbumPages.Select(page => page.TrackCount));
        True(plan.AlbumPages[1].Continuation, "超长专辑第二页应标记为续页");
        Equal(5, plan.TrackCount);
        True(plan.ScreenText.StartsWith("Disc·Title=选择专辑=", StringComparison.Ordinal),
            "ASCII 分隔符应净化，且索引页应排在专辑页之前");
    });
}

static void SanitizeMenuText()
{
    Equal(("繁星、新生，与你：终章＝一", true),
        MenuPlanner.Sanitize("繁星、新生,与你:终章=一"));
    Equal(("A·B·C·D", true), MenuPlanner.Sanitize("A,B:C=D"));
    Equal("Album", MenuPlanner.ShortAlbum("Album(Original Soundtrack)"));
    True(MenuPlanner.Truncate(new string('中', 100), 20).EndsWith('~'),
        "超出像素预算的文字应以 ~ 截断");
}

static void BuildMenuAuthorArguments()
{
    var page = new MenuPagePlan("Album", 0, 1, false,
        [BuildTrack("Track", "1", "1", 1)]);
    var plan = new MenuPlan(
        2, 1, 1, 24, 5, "Disc=选择专辑=Album:Album=Track",
        [page], [["Album"]], []);
    var arguments = plan.BuildAuthorArguments(
        "/menu/blank.png",
        ["/menu/bg0.jpg"],
        "/author",
        "/menu-bin",
        "C:\\fonts\\sc.otf",
        "C:\\fonts\\jp.otf",
        "",
        "/menu/index.txt",
        ["C:/menu/still.jpg"],
        ';');

    True(arguments.Contains("--topmenu"), "应启用 top menu");
    True(arguments.Contains("--nmenus=2"), "应传总菜单页数");
    Equal("C:/fonts/sc.otf", arguments[Array.IndexOf(arguments.ToArray(), "--fontname") + 1]);
    Equal("C:/menu/still.jpg", arguments[Array.IndexOf(arguments.ToArray(), "--stillpics") + 1]);
}

static void ResolveMenuFontRules()
{
    var scripts = MenuFontResolver.NeededScripts(["ASCII 中文 あいう アイウ 한글、《》"]);
    SequenceEqual(
        new[] { "ASCII", "CJK标点", "平假名", "汉字", "片假名", "韩文" },
        scripts.Order(StringComparer.Ordinal));
    Equal("平假名", MenuFontResolver.ScriptOf('あ'));
    Equal("片假名", MenuFontResolver.ScriptOf('ア'));
    Equal("韩文", MenuFontResolver.ScriptOf('한'));
    Equal("汉字", MenuFontResolver.ScriptOf('中'));
    Equal("CJK标点", MenuFontResolver.ScriptOf('《'));

    Equal(
        "Noto-Sans-CJK-JP",
        MenuFontResolver.DeriveRegionalFace(
            "Noto-Sans-CJK-SC", "jp", value => value == "Noto-Sans-CJK-JP"));
    Equal(
        "C:/fonts/NotoSansCJKkr-Regular.otf",
        MenuFontResolver.DeriveRegionalFace(
            "C:/fonts/NotoSansCJKsc-Regular.otf", "kr",
            value => value == "C:/fonts/NotoSansCJKkr-Regular.otf"));
}

static void ExtractOpenTypeCollectionFaces()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-font-tests", Guid.NewGuid().ToString("N"));
    var source = Path.Combine(root, "NotoSansCJK-Regular.ttc");
    var output = Path.Combine(root, "fonts");
    Directory.CreateDirectory(root);
    try
    {
        File.WriteAllBytes(source, BuildSyntheticTtc(
            "Noto Sans CJK JP",
            "Noto Sans CJK KR",
            "Noto Sans CJK SC"));

        var extracted = OpenTypeFontTool.ExtractNotoCjkFaces(source, output);
        Equal(3, extracted.Count);
        SequenceEqual(new[] { 2, 0, 1 }, extracted.Select(item => item.SourceFaceIndex));

        foreach (var (family, fileName) in new[]
        {
            ("Noto Sans CJK SC", "NotoSansCJKsc-Regular.otf"),
            ("Noto Sans CJK JP", "NotoSansCJKjp-Regular.otf"),
            ("Noto Sans CJK KR", "NotoSansCJKkr-Regular.otf"),
        })
        {
            var path = Path.Combine(output, fileName);
            True(File.Exists(path), $"缺少提取结果: {fileName}");
            var inspection = OpenTypeFontTool.VerifyFace(path, family);
            True(inspection.ChecksumValid, $"{fileName} 的全字体校验和应有效");
            True(inspection.HasHan && inspection.HasKana && inspection.HasHangul && inspection.HasLatin,
                $"{fileName} 应包含四种测试字符");
        }

        var rejectedTtc = false;
        try
        {
            OpenTypeFontTool.InspectFace(source);
        }
        catch (InvalidDataException)
        {
            rejectedTtc = true;
        }
        True(rejectedTtc, "单 face 校验必须拒绝 TTC 集合");

        var packed = Path.Combine(root, "shared.ttc");
        OpenTypeFontTool.PackNotoCjkFaces(output, packed);
        OpenTypeFontTool.VerifyNotoCjkCollection(packed);
        True(new FileInfo(packed).Length < extracted.Sum(item => new FileInfo(item.OutputPath).Length),
            "共同字体表应共享存储，不能只是串接三个完整字体");
        var roundTrip = OpenTypeFontTool.ExtractNotoCjkFaces(packed, Path.Combine(root, "roundtrip"));
        SequenceEqual(new[] { 0, 1, 2 }, roundTrip.Select(item => item.SourceFaceIndex));
        foreach (var face in roundTrip)
            SequenceEqual(File.ReadAllBytes(Path.Combine(output, Path.GetFileName(face.OutputPath))),
                File.ReadAllBytes(face.OutputPath));
        var firstPack = File.ReadAllBytes(packed);
        OpenTypeFontTool.PackNotoCjkFaces(output, packed);
        SequenceEqual(firstPack, File.ReadAllBytes(packed));

        var bundle = Path.Combine(root, "bundle");
        Directory.CreateDirectory(Path.Combine(bundle, "menu-bin", "fonts"));
        File.Copy(packed, Path.Combine(bundle, "menu-bin", "fonts", "DvdaNotoCJK-Regular.ttc"));
        File.WriteAllText(Path.Combine(bundle, "menu-bin", "type-dvda-cjk.xml"), "<typemap/>");
        var settings = ProjectSettings.Defaults();
        settings.ApplyBundledToolDefaults(bundle);
        Equal("DVDA-Noto-Sans-CJK-SC", settings.ToOptions().MenuFont);
        Equal("DVDA-Noto-Sans-CJK-JP", settings.ToOptions().MenuFontJapanese);
        Equal("DVDA-Noto-Sans-CJK-KR", settings.ToOptions().MenuFontKorean);
        settings.Values["DVDA_MENU_FONT"] = "C:/custom/font.otf";
        settings.ApplyBundledToolDefaults(bundle);
        Equal("C:/custom/font.otf", settings.ToOptions().MenuFont);
        settings.Values["DVDA_MENU_FONT_JP"] = Path.Combine(bundle, "menu-bin", "fonts", "NotoSansCJKjp-Regular.otf");
        settings.Values["DVDA_MENU_FONT_KR"] = Path.Combine(bundle, "menu-bin", "fonts", "NotoSansCJKkr-Regular.otf").Replace('\\', '/');
        settings.ApplyBundledToolDefaults(bundle);
        Equal("DVDA-Noto-Sans-CJK-JP", settings.ToOptions().MenuFontJapanese);
        Equal("DVDA-Noto-Sans-CJK-KR", settings.ToOptions().MenuFontKorean);
        Equal("C:/custom/font.otf", settings.ToOptions().MenuFont);

        File.Copy(Path.Combine(output, "NotoSansCJKjp-Regular.otf"),
            Path.Combine(output, "NotoSansCJKsc-Regular.otf"), overwrite: true);
        var rejectedWrongRegion = false;
        try { OpenTypeFontTool.PackNotoCjkFaces(output, packed); }
        catch (InvalidDataException) { rejectedWrongRegion = true; }
        True(rejectedWrongRegion, "合并必须拒绝错误的区域 face");
        SequenceEqual(firstPack, File.ReadAllBytes(packed));
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
}

static byte[] BuildSyntheticTtc(params string[] families)
{
    var faces = families.Select(BuildSyntheticSfnt).ToArray();
    var offsets = new int[faces.Length];
    var length = Align4ForFixture(12 + faces.Length * 4);
    for (var index = 0; index < faces.Length; index++)
    {
        offsets[index] = length;
        length = checked(length + Align4ForFixture(faces[index].Length));
    }

    var result = new byte[length];
    WriteFixtureU32(result, 0, 0x74746366);
    WriteFixtureU32(result, 4, 0x00010000);
    WriteFixtureU32(result, 8, checked((uint)faces.Length));
    for (var index = 0; index < faces.Length; index++)
    {
        WriteFixtureU32(result, 12 + index * 4, checked((uint)offsets[index]));
        faces[index].CopyTo(result, offsets[index]);
        var tableCount = ReadFixtureU16(result, offsets[index] + 4);
        for (var tableIndex = 0; tableIndex < tableCount; tableIndex++)
        {
            var record = offsets[index] + 12 + tableIndex * 16;
            WriteFixtureU32(
                result,
                record + 8,
                checked(ReadFixtureU32(result, record + 8) + (uint)offsets[index]));
        }
    }
    return result;
}

static byte[] BuildSyntheticSfnt(string family)
{
    var postScript = family.Replace(" ", string.Empty, StringComparison.Ordinal) + "-Regular";
    var familyBytes = System.Text.Encoding.BigEndianUnicode.GetBytes(family);
    var postScriptBytes = System.Text.Encoding.BigEndianUnicode.GetBytes(postScript);

    var name = new byte[6 + 24 + familyBytes.Length + postScriptBytes.Length];
    WriteFixtureU16(name, 2, 2);
    WriteFixtureU16(name, 4, 30);
    WriteNameRecord(name, 6, 1, familyBytes.Length, 0);
    WriteNameRecord(name, 18, 6, postScriptBytes.Length, familyBytes.Length);
    familyBytes.CopyTo(name, 30);
    postScriptBytes.CopyTo(name, 30 + familyBytes.Length);

    var codePoints = new uint[] { 0x0041, 0x3042, 0x6C49, 0xAC00 };
    var cmap = new byte[12 + 16 + codePoints.Length * 12];
    WriteFixtureU16(cmap, 2, 1);
    WriteFixtureU16(cmap, 4, 3);
    WriteFixtureU16(cmap, 6, 10);
    WriteFixtureU32(cmap, 8, 12);
    WriteFixtureU16(cmap, 12, 12);
    WriteFixtureU32(cmap, 16, checked((uint)(cmap.Length - 12)));
    WriteFixtureU32(cmap, 24, checked((uint)codePoints.Length));
    for (var index = 0; index < codePoints.Length; index++)
    {
        var group = 28 + index * 12;
        WriteFixtureU32(cmap, group, codePoints[index]);
        WriteFixtureU32(cmap, group + 4, codePoints[index]);
        WriteFixtureU32(cmap, group + 8, checked((uint)(index + 1)));
    }

    var tables = new[]
    {
        (Tag: "cmap", Data: cmap),
        (Tag: "head", Data: new byte[54]),
        (Tag: "name", Data: name),
    };
    var resultLength = 12 + tables.Length * 16;
    foreach (var table in tables)
        resultLength = checked(Align4ForFixture(resultLength) + Align4ForFixture(table.Data.Length));
    var result = new byte[resultLength];
    WriteFixtureU32(result, 0, 0x00010000);
    WriteFixtureU16(result, 4, checked((ushort)tables.Length));
    WriteFixtureU16(result, 6, 32);
    WriteFixtureU16(result, 8, 1);
    WriteFixtureU16(result, 10, 16);

    var offset = 12 + tables.Length * 16;
    for (var index = 0; index < tables.Length; index++)
    {
        offset = Align4ForFixture(offset);
        var record = 12 + index * 16;
        System.Text.Encoding.ASCII.GetBytes(tables[index].Tag).CopyTo(result, record);
        WriteFixtureU32(result, record + 8, checked((uint)offset));
        WriteFixtureU32(result, record + 12, checked((uint)tables[index].Data.Length));
        tables[index].Data.CopyTo(result, offset);
        offset += Align4ForFixture(tables[index].Data.Length);
    }
    return result;
}

static void WriteNameRecord(byte[] data, int offset, ushort nameId, int length, int stringOffset)
{
    WriteFixtureU16(data, offset, 3);
    WriteFixtureU16(data, offset + 2, 1);
    WriteFixtureU16(data, offset + 4, 0x0409);
    WriteFixtureU16(data, offset + 6, nameId);
    WriteFixtureU16(data, offset + 8, checked((ushort)length));
    WriteFixtureU16(data, offset + 10, checked((ushort)stringOffset));
}

static int Align4ForFixture(int value) => checked((value + 3) & ~3);

static ushort ReadFixtureU16(byte[] data, int offset) =>
    System.Buffers.Binary.BinaryPrimitives.ReadUInt16BigEndian(data.AsSpan(offset, 2));

static uint ReadFixtureU32(byte[] data, int offset) =>
    System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(data.AsSpan(offset, 4));

static void WriteFixtureU16(byte[] data, int offset, ushort value) =>
    System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(data.AsSpan(offset, 2), value);

static void WriteFixtureU32(byte[] data, int offset, uint value) =>
    System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(data.AsSpan(offset, 4), value);

static void VerifyMenuVisualThresholds()
{
    False(MenuVisualVerifier.IsIndexBackgroundInvalid(75),
        "正常的深色拼贴背景不应判为白底");
    True(MenuVisualVerifier.IsIndexBackgroundInvalid(238),
        "接近白色的格子角落应判为画布异常");
    True(MenuVisualVerifier.IsIndexThumbnailMissing(3),
        "近黑缩略图应判为空");
    False(MenuVisualVerifier.IsIndexThumbnailMissing(116),
        "正常缩略图不应判为空");
    False(MenuVisualVerifier.IsIndexLabelMissing(240, 80),
        "深底亮字名称条应通过");
    True(MenuVisualVerifier.IsIndexLabelMissing(150, 80),
        "没有亮字的名称条应失败");
    True(MenuVisualVerifier.IsIndexLabelMissing(240, 238),
        "接近纯白的名称条应失败");
    True(MenuVisualVerifier.IsFrameNearSolid(0.5, 1000),
        "标准差过低的首帧应判为近纯色");
    True(MenuVisualVerifier.IsFrameNearSolid(20, 20),
        "颜色过少的首帧应判为近纯色");
    False(MenuVisualVerifier.IsFrameNearSolid(20, 1000),
        "正常菜单首帧应通过纯色检查");
}

static void ParseBatchedMenuStatistics()
{
    var output = "F|0.5|0.1|1234\n" +
        "B|1|75\nT|1|116\nL|1|240|80\n" +
        "B|2|238\nT|2|0\nL|2|150|80\n";
    var frame = MenuVisualVerifier.ParseBatchFrameStats(output);
    True(frame is not null && frame.Length == 3 && frame[2] == 1234,
        "完整整帧统计应可解析");
    var parsed = MenuVisualVerifier.ParseIndexBatchOutput(output, 2, true);
    SequenceEqual(new[] { 2 }, parsed.Background);
    SequenceEqual(new[] { 2 }, parsed.Thumbnail);
    SequenceEqual(new[] { 2 }, parsed.Label);

    var damaged = "B|1|75\nB|1|75\nT|1|NaN\nL|1|240|80\n";
    parsed = MenuVisualVerifier.ParseIndexBatchOutput(damaged, 2, true);
    SequenceEqual(new[] { 1, 2 }, parsed.Background);
    SequenceEqual(new[] { 1, 2 }, parsed.Thumbnail);
    SequenceEqual(new[] { 2 }, parsed.Label);
    parsed = MenuVisualVerifier.ParseIndexBatchOutput(output, 2, false);
    SequenceEqual(new[] { 1, 2 }, parsed.Background);
    SequenceEqual(new[] { 1, 2 }, parsed.Thumbnail);
    SequenceEqual(new[] { 1, 2 }, parsed.Label);
    True(MenuVisualVerifier.ParseBatchFrameStats("F|0.5|NaN|1234\n") is null,
        "整帧非有限数值不得通过");

    var overlay = MenuBuildVerifier.ParseOverlayBatchOutput(
        "N|10\nA|0\nH|20\n", true, true);
    True(overlay.Normal == 10 && overlay.Highlighted == 20 && overlay.Arrow == 0,
        "叠加图数值和箭头墨迹应正确解析");
    overlay = MenuBuildVerifier.ParseOverlayBatchOutput("N|10\nN|10\nH|20\n", true, true);
    True(overlay.Normal is null && overlay.Highlighted == 20 && overlay.Arrow is null,
        "重复统计不得被信任；缺失箭头保持未知");
    overlay = MenuBuildVerifier.ParseOverlayBatchOutput("N|10\nH|20\n", false, false);
    True(overlay.Normal is null && overlay.Highlighted is null,
        "失败的图像命令不得被当作有效数据");
}

static void BatchMenuProcessCalls()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-magick-batch", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    SetFixtureCallLogDirectory(root);
    try
    {
        var magick = CreateFixtureExecutable(root, "fake-magick.exe");
        var frame = Path.Combine(root, "frame with spaces.png");
        var normal = Path.Combine(root, "impic0.png");
        var highlighted = Path.Combine(root, "hlpic0.png");
        File.WriteAllBytes(frame, [1]);
        File.WriteAllBytes(normal, [1]);
        File.WriteAllBytes(highlighted, [1]);

        var visual = new MenuVisualVerifier(new ProcessRunner());
        var batch = visual.ReadIndexPageBatchAsync(magick, frame, 12, CancellationToken.None)
            .GetAwaiter().GetResult();
        True(batch is not null, "索引页批量命令应返回整帧与逐格结果");
        Equal(1, FixtureCallCount(root, "magick"));
        Equal(1234d, batch!.FrameStats[2]);
        var parsed = MenuVisualVerifier.ParseIndexBatchOutput(batch.Output, 12, true);
        Equal(0, parsed.Background.Count + parsed.Thumbnail.Count + parsed.Label.Count);

        var overlay = new MenuBuildVerifier(new ProcessRunner())
            .ReadOverlayBatchAsync(magick, normal, highlighted, true, CancellationToken.None)
            .GetAwaiter().GetResult();
        True(overlay.Normal == 10 && overlay.Highlighted == 20 && overlay.Arrow == 1,
            "叠加图批量命令应返回两层及箭头区域数值");
        Equal(2, FixtureCallCount(root, "magick"));
    }
    finally
    {
        SetFixtureCallLogDirectory(null);
        Directory.Delete(root, recursive: true);
    }
}

static void VerifyMultiIndexPageLayout()
{
    SequenceEqual(new[] { 12, 1 },
        Enumerable.Range(0, 2).Select(page => MenuVisualVerifier.ExpectedIndexCells(13, page)));
    SequenceEqual(new[] { 12, 5 },
        Enumerable.Range(0, 2).Select(page => MenuVisualVerifier.ExpectedIndexCells(17, page)));
    SequenceEqual(new[] { 12, 12, 4 },
        Enumerable.Range(0, 3).Select(page => MenuVisualVerifier.ExpectedIndexCells(28, page)));
    Equal(0, MenuVisualVerifier.ExpectedIndexCells(12, 1));

    var continuationTracks = Enumerable.Range(1, 49)
        .Select(index => BuildTrack($"Track {index}", "1", index.ToString(), 10, album: "Album"))
        .ToArray();
    var continuationPages = MenuPlanner.CreateAlbumPages(
        continuationTracks,
        continuationTracks.Select(_ => "Album").ToArray(),
        24);
    Equal(3, continuationPages.Count);
    Equal(3, MenuVisualVerifier.ExpectedIndexCells(continuationPages.Count, 0));

    const int languageUnit = 0x1810;
    const int firstPgcPointer = 0x181C;
    const int pgcIndex = 0x1820;
    const int stride = 0x132;
    const int cellStart = 0x11E;
    const int cellEnd = 0x12A;
    static void U32(byte[] data, int offset, uint value) =>
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(data.AsSpan(offset, 4), value);

    var firstBase = 0x1900;
    var data = new byte[firstBase + 3 * stride + cellEnd + 4];
    U32(data, firstPgcPointer, (uint)(firstBase - languageUnit));
    for (var index = 1; index < 3; index++)
    {
        U32(data, pgcIndex + (index - 1) * 8 + 4,
            (uint)(firstBase + index * stride - languageUnit));
    }
    U32(data, firstBase + cellEnd, 3);
    U32(data, firstBase + stride + cellStart, 4);
    U32(data, firstBase + stride + cellEnd, 8);
    U32(data, firstBase + 2 * stride + cellStart, 9);
    U32(data, firstBase + 2 * stride + cellEnd, 10);

    SequenceEqual(
        new (uint Start, uint End)[] { (0, 3), (4, 8), (9, 10) },
        MenuDiscVerifier.ReadMenuCellRanges(data, 3));
}

static void ValidateAmgCellChainFixtures()
{
    const int languageUnit = 0x1810;
    const int firstPgcPointer = 0x181C;
    const int pgcIndex = 0x1820;
    const int stride = 0x132;
    const int nextMenu = 0x09C;
    const int previousMenu = 0x09E;
    const int cellStart = 0x11E;
    const int cellStartCopy = 0x126;
    const int cellEnd = 0x12A;

    static void U16(byte[] data, int offset, ushort value) =>
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(data.AsSpan(offset, 2), value);
    static void U32(byte[] data, int offset, uint value) =>
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(data.AsSpan(offset, 4), value);

    var firstBase = 0x1900;
    var secondBase = firstBase + stride;
    var data = new byte[secondBase + cellEnd + 4];
    U16(data, languageUnit, 2);
    U32(data, firstPgcPointer, (uint)(firstBase - languageUnit));
    U32(data, pgcIndex + 4, (uint)(secondBase - languageUnit));

    U16(data, firstBase + nextMenu, 2);
    U32(data, firstBase + cellEnd, 1);
    U16(data, secondBase + previousMenu, 1);
    U32(data, secondBase + cellStart, 2);
    U32(data, secondBase + cellStartCopy, 2);
    U32(data, secondBase + cellEnd, 3);

    var validIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateMenuCellChain(data, 2, 4L * 2048, validIssues);
    Equal(0, validIssues.Count);

    var badCopy = data.ToArray();
    U32(badCopy, secondBase + cellStartCopy, 3);
    var copyIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateMenuCellChain(badCopy, 2, 4L * 2048, copyIssues);
    True(copyIssues.Any(issue => issue.Code == "AMG_CELL_START_COPY_MISMATCH"),
        "第二页 cell 起始地址副本损坏必须被检测到");

    var badNext = data.ToArray();
    U16(badNext, secondBase + nextMenu, 3);
    var nextIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateMenuCellChain(badNext, 2, 4L * 2048, nextIssues);
    True(nextIssues.Any(issue => issue.Code == "AMG_LAST_NEXT_MENU_INVALID"),
        "末页 Next 非零必须被检测到");

    var badStride = data.ToArray();
    U32(badStride, pgcIndex + 4, (uint)(secondBase + 1 - languageUnit));
    var strideIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateMenuCellChain(badStride, 2, 4L * 2048, strideIssues);
    True(strideIssues.Any(issue => issue.Code == "AMG_PGC_STRIDE_MISMATCH"),
        "PGC stride 损坏必须被检测到");
}

static void ValidateAsvsFixtures()
{
    const int titleCount = 0x0C;
    const int lastSector = 0x14;
    const int table = 0x60;
    const int entryLength = 8;

    static void U16(byte[] data, int offset, ushort value) =>
        System.Buffers.Binary.BinaryPrimitives.WriteUInt16BigEndian(data.AsSpan(offset, 2), value);
    static void U32(byte[] data, int offset, uint value) =>
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(data.AsSpan(offset, 4), value);

    var data = new byte[table + 2 * entryLength];
    U16(data, titleCount, 2);
    U32(data, lastSector, 1);
    data[table] = 1;
    U16(data, table + 2, 1);
    U32(data, table + 4, 0);
    data[table + entryLength] = 1;
    U16(data, table + entryLength + 2, 2);
    U32(data, table + entryLength + 4, 1);
    var expectation = new MenuVerificationExpectation(0, 0, 2, 2, 0);

    var validIssues = new List<VerificationIssue>();
    Equal(2, MenuDiscVerifier.ValidateAsvs(data, 2L * 2048, expectation, validIssues));
    Equal(0, validIssues.Count);

    var badSequence = data.ToArray();
    U16(badSequence, table + entryLength + 2, 4);
    var sequenceIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateAsvs(badSequence, 2L * 2048, expectation, sequenceIssues);
    True(sequenceIssues.Any(issue => issue.Code == "ASVS_PICTURE_SEQUENCE_BROKEN"),
        "静图编号不连续必须被检测到");

    var badSector = data.ToArray();
    U32(badSector, table + entryLength + 4, 2);
    var sectorIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateAsvs(badSector, 2L * 2048, expectation, sectorIssues);
    True(sectorIssues.Any(issue => issue.Code == "ASVS_SECTOR_OUT_OF_RANGE"),
        "静图起始扇区越界必须被检测到");

    var truncated = data[..(table + entryLength + 4)];
    var truncatedIssues = new List<VerificationIssue>();
    MenuDiscVerifier.ValidateAsvs(truncated, 2L * 2048, expectation, truncatedIssues);
    True(truncatedIssues.Any(issue => issue.Code == "ASVS_TABLE_TRUNCATED"),
        "ASVS 表截断必须被检测到");
}

static DvdaOptions CreateExecutorOptions(
    string root,
    string author,
    string mkisofs,
    bool keepIntermediate = false)
{
    var config = Path.Combine(root, "config.env");
    File.WriteAllText(config, string.Join('\n',
    [
        $"DVDA_SRC={Path.Combine(root, "src").Replace('\\', '/')}",
        $"DVDA_FINAL_DIR={Path.Combine(root, "final").Replace('\\', '/')}",
        $"DVDA_BUILD_DIR={Path.Combine(root, "build").Replace('\\', '/')}",
        $"DVDA_AUTHOR={author.Replace('\\', '/')}",
        $"DVDA_MKISOFS={mkisofs.Replace('\\', '/')}",
        "DVDA_TITLE=Fixture Disc",
        "DVDA_KEEP_TMP=off",
        $"DVDA_KEEP_INTERMEDIATE={(keepIntermediate ? "on" : "off")}",
    ]));
    return Load(config);
}

static string CreateFixtureExecutable(string root, string fileName)
{
    const string applicationName = "DvdaMaker.CompatibilityTests";
    var source = Path.Combine(AppContext.BaseDirectory, applicationName + ".exe");
    if (!File.Exists(source))
    {
        throw new InvalidOperationException($"找不到兼容测试 apphost: {source}");
    }
    foreach (var extension in new[] { ".dll", ".deps.json", ".runtimeconfig.json" })
    {
        var companionSource = Path.Combine(AppContext.BaseDirectory, applicationName + extension);
        if (!File.Exists(companionSource))
        {
            throw new InvalidOperationException($"找不到兼容测试运行文件: {companionSource}");
        }
        File.Copy(
            companionSource,
            Path.Combine(root, applicationName + extension),
            overwrite: true);
    }
    var destination = Path.Combine(root, fileName);
    File.Copy(source, destination, overwrite: true);
    return destination;
}

static BuildTrack BuildTrack(
    string title,
    string date,
    string track,
    long mlpSize,
    string? album = null,
    int sampleRate = 48_000,
    int bits = 24) => new()
{
    Date = date,
    Track = track,
    Title = title,
    Album = album ?? title,
    SampleRate = sampleRate,
    Bits = bits,
    SourcePath = title + ".flac",
    ManifestName = title,
    Duration = 1,
    SourceSize = mlpSize,
    MlpPath = title + ".mlp",
    MlpSize = mlpSize,
};

static AudioTrackMetadata Track(string path, int sampleRate, int bits) => new()
{
    Path = path,
    SampleRate = sampleRate,
    Bits = bits,
    Channels = 2,
    Album = "album",
    Title = path,
    SourceSampleRate = sampleRate,
    SourceBits = bits,
};

static byte[] EncodePts(long value) =>
[
    (byte)(0x20 | (((value >> 30) & 0x07) << 1) | 1),
    (byte)(value >> 22),
    (byte)((((value >> 15) & 0x7F) << 1) | 1),
    (byte)(value >> 7),
    (byte)(((value & 0x7F) << 1) | 1),
];

static DvdaOptions Load(string path, Dictionary<string, string?>? environment = null) =>
    new ConfigLoader(environment ?? new Dictionary<string, string?>()).Load(path);

static void WithConfig(string content, Action<string> action)
{
    var directory = Path.Combine(Path.GetTempPath(), "dvda-config-tests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(directory);
    var path = Path.Combine(directory, "config.env");
    File.WriteAllText(path, content);
    try
    {
        action(path);
    }
    finally
    {
        Directory.Delete(directory, recursive: true);
    }
}

static void Equal<T>(T expected, T actual)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"期望 <{expected}>，实际 <{actual}>");
    }
}

static void False(bool value, string message)
{
    if (value)
    {
        throw new InvalidOperationException(message);
    }
}

static void True(bool value, string message)
{
    if (!value)
    {
        throw new InvalidOperationException(message);
    }
}

static void SequenceEqual<T>(IEnumerable<T> expected, IEnumerable<T> actual)
{
    if (!expected.SequenceEqual(actual))
    {
        throw new InvalidOperationException(
            $"期望 <{string.Join(", ", expected)}>，实际 <{string.Join(", ", actual)}>");
    }
}

static void RejectRemovedSource(string path)
{
    try { _ = Load(path).MlpSource; }
    catch (ArgumentException) { return; }
    throw new Exception("Unknown/removed MLP source must be rejected");
}
