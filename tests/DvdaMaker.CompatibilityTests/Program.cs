using DvdaMaker.Building;
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
    if (fixtureProcessName.Contains("fail", StringComparison.OrdinalIgnoreCase))
    {
        Console.Error.WriteLine("fixture author failure");
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
    Console.WriteLine("1  1/1  1  0  99  0  90000  0");
    return 0;
}
if (fixtureProcessName.StartsWith("fake-mkisofs", StringComparison.OrdinalIgnoreCase))
{
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
if (fixtureProcessName.StartsWith("fake-robocopy", StringComparison.OrdinalIgnoreCase))
{
    var sourcePath = Path.Combine(args[0].TrimEnd('\\'), args[2]);
    var destinationPath = Path.Combine(args[1], args[2]);
    if (Directory.Exists(args[1]))
    {
        Directory.CreateDirectory(args[1]);
        File.Copy(sourcePath, destinationPath, overwrite: true);
    }
    return Environment.GetEnvironmentVariable("FAKE_ROBOCOPY_EXIT") is { Length: > 0 } exitText &&
        int.TryParse(exitText, out var exitCode) ? exitCode : 1;
}
if (fixtureProcessName.StartsWith("fake-ffprobe", StringComparison.OrdinalIgnoreCase))
{
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
    Console.WriteLine();
    return 0;
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
        "用法: DvdaMaker.CompatibilityTests [--real-fixtures <ISO目录> <MLP根目录>]");
    return 2;
}

var tests = new (string Name, Action Run)[]
{
    ("解析引号、注释和无效行", ParseAssignments),
    ("Windows 路径规范化", NormalizeWindowsPaths),
    ("环境变量优先于 config.sh", EnvironmentWins),
    ("空环境变量不覆盖配置", EmptyEnvironmentDoesNotOverride),
    ("默认值与数值回退", DefaultsAndNumericFallbacks),
    ("派生路径和 ISO 名称", DerivedPathsAndNames),
    ("限制组轨数量", ClampGroupTrackLimit),
    ("规范化 MLP 来源", NormalizeMlpSource),
    ("Shell 单引号转义", EscapeShellAssignment),
    ("Shell 默认键集兼容 Python", PreserveLegacyShellKeySet),
    ("配置来源与有效键集合", DescribeConfigurationSources),
    ("ISO9660 版本后缀处理", StripIsoVersion),
    ("PTS 五字节解析", ParsePts),
    ("MLP 峰值码率取整", CalculatePeakBitrate),
    ("MLP access unit 遍历", WalkMlpAccessUnits),
    ("MLP CRC 与奇偶校验基线", MlpChecksumBaseline),
    ("MLP 损坏边界与对齐修复", ValidateMlpDamageFixtures),
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
    ("MLP 编码参数构造", BuildMlpEncodingArguments),
    ("外部 MLP 镜像路径优先", ResolveExternalMlp),
    ("MLP 索引结构", WriteMlpIndex),
    ("构建索引预演与失败隔离", PreserveFormalMlpIndex),
    ("dvda-author 参数与 title 边界", BuildDvdaAuthorArguments),
    ("诊断 title 划分模式", BuildDiagnosticTitleModes),
    ("诊断专辑数量限制", LimitDiagnosticAlbums),
    ("mkisofs 参数构造", BuildMkisofsArguments),
    ("构建日志命令格式", WriteCompatibleBuildLog),
    ("ISO 发布长度校验", PublishIso),
    ("多盘与索引事务发布回滚", PublishDiscSetTransactionally),
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
    ("审计按时间选择最新日志", SelectNewestAuditLog),
    ("外部 MLP 重名歧义", DetectExternalMlpAmbiguity),
    ("MLP 索引拒绝重复路径", RejectDuplicateMlpIndexKeys),
    ("正式出盘执行器端到端", BuildDiscEndToEnd),
    ("出盘失败保留诊断现场", PreserveFailedDiscWorkspace),
    ("Robocopy 0-7 退出码兼容", AcceptRobocopySuccessCodes),
    ("Robocopy 复制失败不丢 ISO", PreserveIsoOnWindowsCopyFailure),
    ("菜单配置派生值", LoadMenuConfiguration),
    ("菜单按专辑分页与索引", PlanAlbumMenuPages),
    ("菜单文字净化与截断", SanitizeMenuText),
    ("菜单 author 参数结构", BuildMenuAuthorArguments),
    ("菜单字体脚本识别与区域 face", ResolveMenuFontRules),
    ("纯 C# TTC face 提取与校验", ExtractOpenTypeCollectionFaces),
    ("菜单视觉阈值与 Python 一致", VerifyMenuVisualThresholds),
    ("菜单多索引页格数与 cell 范围", VerifyMultiIndexPageLayout),
    ("AMG 菜单 cell 链损坏检测", ValidateAmgCellChainFixtures),
    ("ASVS 静图表损坏检测", ValidateAsvsFixtures),
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
        Equal("/root/dvda-build", options.BuildDirectory);
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
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_MLP_SOURCE=unknown", path =>
        Equal("ffmpeg", Load(path).MlpSource));
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

static void BuildMlpEncodingArguments()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out", path =>
    {
        var provider = new FfmpegMlpProvider(Load(path), new ProcessRunner());
        var track = BuildTrack("A", "1", "1", 0) with
        {
            Bits = 16,
            ResampleTo = 48_000,
        };
        var arguments = provider.BuildArguments(track).ToArray();
        True(arguments.Contains("aresample=48000:resampler=soxr"), "应使用 soxr 重采样");
        Equal("s16p", arguments[Array.IndexOf(arguments, "-sample_fmt") + 1]);
        Equal("8", arguments[Array.IndexOf(arguments, "-max_interval") + 1]);
        Equal(track.MlpPath, arguments[^1]);
    });
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
    var config = Path.Combine(root, "config.sh");
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
            "DVDA_SRC", "DVDA_FINAL_DIR", "DVDA_WINDOWS_DEST", "DVDA_BUILD_DIR",
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
        Equal("config.sh", options.ValueSource("DVDA_FINAL_DIR"));
        Equal("config.sh", options.ValueSource("CUSTOM_VALUE"));
        Equal("默认值", options.ValueSource("DVDA_TITLE"));
        True(options.HasEnvironmentOverrides(), "应识别非空环境变量覆盖");
        True(options.EffectiveKeys().Contains("CUSTOM_VALUE"), "显式配置的扩展键应出现在诊断输出");
        SequenceEqual(
            options.EffectiveKeys().Order(StringComparer.Ordinal),
            options.EffectiveKeys());
    });
}

static void BuildMkisofsArguments()
{
    WithConfig("DVDA_SRC=/src\nDVDA_FINAL_DIR=/out\nDVDA_TITLE=Test", path =>
    {
        var options = Load(path);
        var disc = new DiscPlan(1, [], []);
        SequenceEqual(new[]
        {
            "-dvd-audio", "-V", "Test 1", "-o", "/work/disc1.iso", "/work/disc1",
        }, DvdaAuthorCommandBuilder.BuildMkisofsArguments(
            options, disc, "/work/disc1.iso", "/work/disc1"));
    });
}

static void WriteCompatibleBuildLog()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-log-tests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var config = Path.Combine(root, "config.sh");
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
            new ProcessRunner(), "unused-ffmpeg", ffprobe, "unused-metaflac",
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
            new ProcessRunner(), "unused-ffmpeg", ffprobe, "unused-metaflac",
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
            new ProcessRunner(), "unused-ffmpeg", ffprobe, "unused-metaflac",
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
        Equal("disc2", parsed.Commands[1].DiscTag);
        Equal(1, parsed.Commands[1].GroupCount);
        Equal(100, parsed.Rows[1].First);
    }
    finally
    {
        Directory.Delete(root, recursive: true);
    }
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
    var config = Path.Combine(root, "config.sh");
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
        True(logText.Contains("fake-mkisofs", StringComparison.OrdinalIgnoreCase),
            "日志应记录 mkisofs 命令");
        True(logText.Contains("1  1/1  1  0  99  0  90000  0"),
            "日志应保留 author 轨道表供审计解析");
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

static void AcceptRobocopySuccessCodes()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-robocopy-success", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var previousRobocopy = Environment.GetEnvironmentVariable("DVDA_ROBOCOPY");
    try
    {
        var source = Path.Combine(root, "disc.iso");
        File.WriteAllBytes(source, [1, 2, 3]);
        var robocopy = CreateFixtureExecutable(root, "fake-robocopy.exe");
        Environment.SetEnvironmentVariable("DVDA_ROBOCOPY", robocopy);
        var result = new WindowsIsoCopier(new ProcessRunner())
            .CopyAsync(source, Path.Combine(root, "windows-dest"), "fixture", default)
            .GetAwaiter().GetResult();

        True(result.Succeeded, "robocopy 退出码 1 是成功（有文件已复制）");
        Equal(1, result.ExitCode);
    }
    finally
    {
        Environment.SetEnvironmentVariable("DVDA_ROBOCOPY", previousRobocopy);
        Directory.Delete(root, recursive: true);
    }
}

static void PreserveIsoOnWindowsCopyFailure()
{
    var root = Path.Combine(Path.GetTempPath(), "dvda-robocopy-failure", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(root);
    var previousRobocopy = Environment.GetEnvironmentVariable("DVDA_ROBOCOPY");
    var previousExit = Environment.GetEnvironmentVariable("FAKE_ROBOCOPY_EXIT");
    try
    {
        var author = CreateFixtureExecutable(root, "fake-dvda-author.exe");
        var mkisofs = CreateFixtureExecutable(root, "fake-mkisofs.exe");
        var robocopy = CreateFixtureExecutable(root, "fake-robocopy.exe");
        Environment.SetEnvironmentVariable("DVDA_ROBOCOPY", robocopy);
        Environment.SetEnvironmentVariable("FAKE_ROBOCOPY_EXIT", "8");
        var options = CreateExecutorOptions(root, author, mkisofs, windowsDestination: "E:/Published");
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

        True(result.Succeeded, "Windows 侧复制失败不应使已发布 ISO 构建失败");
        True(result.Diagnostics.Any(item => item.Code == "WINDOWS_ISO_COPY_FAILED"),
            "复制失败应作为 warning 返回");
        True(File.Exists(result.PublishedIsoPath), "源输出位置的 ISO 必须保留");
    }
    finally
    {
        Environment.SetEnvironmentVariable("DVDA_ROBOCOPY", previousRobocopy);
        Environment.SetEnvironmentVariable("FAKE_ROBOCOPY_EXIT", previousExit);
        Directory.Delete(root, recursive: true);
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
    string? windowsDestination = null)
{
    var config = Path.Combine(root, "config.sh");
    File.WriteAllText(config, string.Join('\n',
    [
        $"DVDA_SRC={Path.Combine(root, "src").Replace('\\', '/')}",
        $"DVDA_FINAL_DIR={Path.Combine(root, "final").Replace('\\', '/')}",
        $"DVDA_BUILD_DIR={Path.Combine(root, "build").Replace('\\', '/')}",
        $"DVDA_AUTHOR={author.Replace('\\', '/')}",
        $"DVDA_MKISOFS={mkisofs.Replace('\\', '/')}",
        $"DVDA_WINDOWS_DEST={windowsDestination ?? string.Empty}",
        "DVDA_TITLE=Fixture Disc",
        "DVDA_KEEP_TMP=off",
        "DVDA_KEEP_INTERMEDIATE=off",
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
    var path = Path.Combine(directory, "config.sh");
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
