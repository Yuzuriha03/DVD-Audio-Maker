using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.Formats.Iso9660;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;

Console.OutputEncoding = System.Text.Encoding.UTF8;
Console.InputEncoding = System.Text.Encoding.UTF8;

var arguments = args.ToList();
var command = arguments.Count > 0 && !arguments[0].StartsWith("--", StringComparison.Ordinal)
    ? arguments[0]
    : "config";
if (arguments.Count > 0 && arguments[0] == command)
{
    arguments.RemoveAt(0);
}

string? configPath = null;
for (var index = 0; index < arguments.Count;)
{
    if (arguments[index] != "--config")
    {
        index++;
        continue;
    }
    if (configPath is not null)
    {
        Console.Error.WriteLine("[配置错误] --config 只能指定一次");
        return 2;
    }
    if (index + 1 >= arguments.Count ||
        arguments[index + 1].StartsWith("--", StringComparison.Ordinal))
    {
        Console.Error.WriteLine("[配置错误] --config 后需要路径");
        return 2;
    }
    configPath = arguments[index + 1];
    arguments.RemoveRange(index, 2);
}

var options = new ConfigLoader().Load(configPath);

if (command == "aob-pts")
{
    if (arguments.Count == 0 || arguments.Any(argument => argument.StartsWith("--", StringComparison.Ordinal)))
    {
        Console.Error.WriteLine("用法: dvda aob-pts <AOB 文件>...");
        return 2;
    }
    var failed = false;
    foreach (var path in arguments)
    {
        if (!File.Exists(path))
        {
            Console.Error.WriteLine($"[FAIL] 不是文件: {path}");
            failed = true;
            continue;
        }
        try
        {
            var result = AobPtsAnalyzer.Analyze(path);
            Console.WriteLine($"文件: {path}");
            Console.WriteLine($"总扇区: {result.SectorCount}  含PTS扇区: {result.PtsSectorCount}");
            if (result.FirstPts is not null && result.LastPts is not null)
            {
                Console.WriteLine($"首个 PTS: {result.FirstPts}  末个 PTS: {result.LastPts}");
                Console.WriteLine($"时间跨度: {result.DurationSeconds:F3} 秒 ({result.DurationSeconds / 60:F3} 分钟)");
                Console.WriteLine($"步长: 最小 {result.MinimumStep} 最大 {result.MaximumStep}  " +
                    $"负步长 {result.NegativeSteps} 个  零步长 {result.ZeroSteps} 个");
                Console.WriteLine($"步长中位数: {result.MedianStep:F0}");
                Console.WriteLine($"异常步长数量: {result.AbnormalSteps} 占比 {result.AbnormalRatio:F3}%");
            }
            foreach (var issue in result.Issues)
            {
                Console.WriteLine($"[FAIL] {issue.Code}: {issue.Message}");
            }
            if (result.Succeeded) Console.WriteLine("[OK] PTS 时间轴正常");
            failed |= !result.Succeeded;
        }
        catch (Exception exception) when (exception is IOException or InvalidDataException)
        {
            Console.Error.WriteLine($"[FAIL] {path}: {exception.Message}");
            failed = true;
        }
    }
    return failed ? 1 : 0;
}

if (command == "mlp")
{
    var check = arguments.Remove("--check");
    var align = arguments.Remove("--align");
    var quiet = arguments.Remove("-q") | arguments.Remove("--quiet");
    string? outputDirectory = null;
    for (var index = 0; index < arguments.Count; index++)
    {
        if (arguments[index] is not "-o" and not "--outdir") continue;
        if (index + 1 >= arguments.Count)
        {
            Console.Error.WriteLine("[错误] -o/--outdir 后需要目录");
            return 2;
        }
        outputDirectory = arguments[index + 1];
        arguments.RemoveRange(index, 2);
        break;
    }
    if (check == align || arguments.Count == 0 ||
        arguments.Any(argument => argument.StartsWith("-", StringComparison.Ordinal)))
    {
        Console.Error.WriteLine("用法: dvda mlp (--check|--align) [-o 目录] [-q] <MLP 文件>...");
        return 2;
    }
    if (check && outputDirectory is not null)
    {
        Console.Error.WriteLine("[错误] --check 不能与 -o/--outdir 同时使用");
        return 2;
    }

    var failed = false;
    foreach (var path in arguments)
    {
        if (!File.Exists(path))
        {
            Console.Error.WriteLine($"[跳过] 不是文件: {path}");
            failed = true;
            continue;
        }
        try
        {
            var data = await File.ReadAllBytesAsync(path);
            if (check)
            {
                var inspection = MlpStreamAligner.Inspect(data);
                Console.WriteLine($"[{(inspection.IsValid ? "OK " : "BAD")}] {Path.GetFileName(path)}");
                Console.WriteLine($"       AU {inspection.AccessUnitCount}，major sync {inspection.MajorSyncCount}" +
                    $"（每 {inspection.MajorSyncInterval:F1} 个），peak={inspection.PeakBitrateRaw}，" +
                    $"ext={inspection.ExtendedSubstreamInfo}，EOS={inspection.HasEndOfStream}");
                if (inspection.MajorSyncErrors.Count > 0)
                    Console.WriteLine($"       major sync 异常 {inspection.MajorSyncErrors.Count} 处");
                if (inspection.AccessUnitParityErrors.Count > 0)
                    Console.WriteLine($"       AU 奇偶异常 {inspection.AccessUnitParityErrors.Count} 处");
                if (inspection.SubstreamErrors.Count > 0)
                    Console.WriteLine($"       子流校验异常 {inspection.SubstreamErrors.Count} 处");
                failed |= !inspection.IsValid;
                continue;
            }

            var aligned = MlpStreamAligner.Align(data);
            var destination = outputDirectory is null
                ? path
                : Path.Combine(outputDirectory, Path.GetFileName(path));
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(destination))!);
            if (aligned.Data.AsSpan().SequenceEqual(data))
            {
                if (!quiet) Console.WriteLine($"[已对齐] {Path.GetFileName(path)}");
                continue;
            }
            var temporary = destination + ".tmp";
            await File.WriteAllBytesAsync(temporary, aligned.Data);
            File.Move(temporary, destination, overwrite: true);
            if (!quiet)
            {
                var changes = aligned.Changes;
                Console.WriteLine($"[对齐] {Path.GetFileName(path)}   " +
                    $"peak×{changes.PeakBitrateChanges} ext×{changes.ExtendedSubstreamInfoChanges} " +
                    $"cksum×{changes.ChecksumChanges} EOS={changes.AddedEndOfStream} " +
                    $"(+{aligned.Data.Length - data.Length} 字节)");
            }
        }
        catch (Exception exception) when (exception is IOException or InvalidDataException)
        {
            Console.Error.WriteLine($"[FAIL] {path}: {exception.Message}");
            failed = true;
        }
    }
    return failed ? 1 : 0;
}

if (command == "alac")
{
    if (arguments.Count < 2 || arguments[0] is not ("check" or "repair"))
    {
        Console.Error.WriteLine("用法: dvda alac check <文件> | dvda alac repair <输入> [输出]");
        return 2;
    }
    var operation = arguments[0];
    var source = arguments[1];
    if (!File.Exists(source))
    {
        Console.Error.WriteLine($"[错误] 找不到文件: {source}");
        return 2;
    }
    if (!ToolExists(options.Ffprobe))
    {
        Console.Error.WriteLine($"[错误] 找不到 {options.Ffprobe}");
        return 2;
    }
    if (!ToolExists(options.Ffmpeg))
    {
        Console.Error.WriteLine($"[错误] 找不到 {options.Ffmpeg}");
        return 2;
    }
    if (operation == "check" && arguments.Count != 2 || operation == "repair" && arguments.Count > 3)
    {
        Console.Error.WriteLine("用法: dvda alac check <文件> | dvda alac repair <输入> [输出]");
        return 2;
    }
    try
    {
        var repairer = new AlacEndRepairer(new ProcessRunner(), options.Ffprobe);
        var inspection = await repairer.InspectAsync(source);
        Console.WriteLine($"文件: {source}");
        Console.WriteLine($"  magic cookie: {inspection.Cookie.SampleRate} Hz / " +
            $"{inspection.Cookie.SampleSize} bit / {inspection.Cookie.Channels} ch / " +
            $"max frame {inspection.Cookie.MaxSamplesPerFrame}");
        var sourceDecode = await CheckAlacDecodeAsync(
            source, options.Ffmpeg, options.Ffprobe);
        Console.WriteLine($"  解码采样 {sourceDecode.DecodedSamples?.ToString() ?? "未知"} / " +
            $"声明 {sourceDecode.DeclaredSamples?.ToString() ?? "未知"} / " +
            $"报错 {sourceDecode.ErrorCount} 行");
        Console.WriteLine($"  待修补帧: {inspection.Patches.Count}");
        foreach (var patch in inspection.Patches)
        {
            Console.WriteLine($"    {patch.PresentationTime,9:F3}s  size={patch.Size} " +
                $"标记={Convert.ToString(patch.PreviousBits, 2).PadLeft(3, '0')}");
        }
        if (operation == "check")
        {
            return inspection.Patches.Count == 0 &&
               sourceDecode.ProcessSucceeded &&
               sourceDecode.ErrorCount == 0 &&
               sourceDecode.DecodedSamples is not null &&
               sourceDecode.DeclaredSamples is not null &&
               sourceDecode.DecodedSamples == sourceDecode.DeclaredSamples
            ? 0
            : 1;
        }

        var destination = arguments.Count == 3 ? arguments[2] : source + ".fixed.m4a";
        var work = Path.Combine(Path.GetTempPath(), "dvda-alac-repair", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(work);
        try
        {
            var result = await repairer.TryRepairAsync(source, work);
            if (result is null)
            {
                File.Copy(source, destination, overwrite: true);
                Console.WriteLine($"[已正常] 无需修补，已复制到 {destination}");
            }
            else
            {
                Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(destination))!);
                File.Move(result.OutputPath, destination, overwrite: true);
                Console.WriteLine($"[已修复] {result.Patches.Count} 帧 -> {destination}");
            }

            var repairedDecode = await CheckAlacDecodeAsync(
                destination, options.Ffmpeg, options.Ffprobe);
            Console.WriteLine($"  修复前: 采样 {sourceDecode.DecodedSamples?.ToString() ?? "未知"}  " +
                $"报错 {sourceDecode.ErrorCount} 行");
            Console.WriteLine($"  修复后: 采样 {repairedDecode.DecodedSamples?.ToString() ?? "未知"}  " +
                $"报错 {repairedDecode.ErrorCount} 行  " +
                $"(容器声明 {repairedDecode.DeclaredSamples?.ToString() ?? "未知"})");
            var repairedOk = repairedDecode.ProcessSucceeded &&
                             repairedDecode.ErrorCount == 0 &&
                             repairedDecode.DecodedSamples is not null &&
                             repairedDecode.DeclaredSamples is not null &&
                             repairedDecode.DecodedSamples == repairedDecode.DeclaredSamples;
            Console.WriteLine(repairedOk ? "  结果: 完全修复 ✔" : "  结果: 仍有问题 ✗");
            if (!repairedOk) return 1;
        }
        finally
        {
            if (Directory.Exists(work)) Directory.Delete(work, recursive: true);
        }
        return 0;
    }
    catch (Exception exception) when (exception is IOException or InvalidDataException or InvalidOperationException)
    {
        Console.Error.WriteLine($"[ALAC 失败] {exception.Message}");
        return 1;
    }
}

if (command == "iso")
{
    if (arguments.Count < 2 || arguments[0] is not ("list" or "extract"))
    {
        Console.Error.WriteLine("用法: dvda iso list <ISO> [目录] | dvda iso extract <ISO> <内部路径> <目标>");
        return 2;
    }
    try
    {
        using var iso = new Iso9660Reader(arguments[1]);
        if (arguments[0] == "list")
        {
            if (arguments.Count > 3) return 2;
            foreach (var path in iso.AllPaths(arguments.Count == 3 ? arguments[2] : string.Empty))
                Console.WriteLine(path);
            return 0;
        }
        if (arguments.Count != 4)
        {
            Console.Error.WriteLine("用法: dvda iso extract <ISO> <内部路径> <目标>");
            return 2;
        }
        if (!iso.Extract(arguments[2], arguments[3]))
        {
            Console.Error.WriteLine($"[FAIL] ISO 中不存在 {arguments[2]}");
            return 1;
        }
        return 0;
    }
    catch (Exception exception) when (exception is IOException or Iso9660Exception)
    {
        Console.Error.WriteLine($"[ISO 失败] {exception.Message}");
        return 1;
    }
}

if (command == "build")
{
    var unknown = arguments.Where(argument => argument != "--dry-run").ToArray();
    if (unknown.Length > 0)
    {
        Console.Error.WriteLine($"[错误] build 未知参数: {unknown[0]}");
        return 2;
    }
    var dryRun = arguments.Contains("--dry-run");
    try
    {
        var result = await new BuildPipeline(options).RunAsync(dryRun);
        BuildPlanService.Print(result.Plan, options, Console.Out);
        if (result.IndexPath.Length > 0)
        {
            Console.WriteLine($"MLP 索引已写入: {result.IndexPath}");
        }
        if (options.MlpSource == "ffmpeg")
        {
            Console.WriteLine(
                $"[缓存] 复用 {result.Acquisition.CacheHits} 个，" +
                $"重新编码 {result.Acquisition.CacheRebuilt} 个");
        }
        if (dryRun)
        {
            Console.WriteLine("[DRY-RUN] 已完成 MLP 获取、分盘规划和独立预演索引生成；未执行出盘，也未覆盖正式索引。");
            return 0;
        }
        foreach (var disc in result.DiscResults)
        {
            foreach (var diagnostic in disc.Diagnostics)
            {
                Console.WriteLine($"[{diagnostic.Severity}] {diagnostic.Code}: {diagnostic.Message}");
            }
            if (!disc.Succeeded)
            {
                Console.Error.WriteLine("[索引] 构建未全部成功，正式 mlp_index.json 保持不变。");
                return 1;
            }
            Console.WriteLine($"[OK] {disc.PublishedIsoPath} ({disc.IsoSize / 1024d / 1024 / 1024:F2} GB)");
        }
        return result.DiscResults.Count == result.Plan.Discs.Count ? 0 : 1;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[构建失败] {exception.Message}");
        return 1;
    }
}

if (command is "convert" or "m4a2flac")
{
    var paths = new List<string>();
    var dryRun = false;
    var inPlace = false;
    var level = 8;
    var jobs = Math.Min(4, Environment.ProcessorCount);
    for (var index = 0; index < arguments.Count; index++)
    {
        switch (arguments[index])
        {
            case "--dry-run":
                dryRun = true;
                break;
            case "--in-place":
                inPlace = true;
                break;
            case "--level":
                if (++index >= arguments.Count || !int.TryParse(arguments[index], out level))
                {
                    Console.Error.WriteLine("[错误] --level 必须是 0-8 的整数");
                    return 2;
                }
                break;
            case "--jobs":
                if (++index >= arguments.Count || !int.TryParse(arguments[index], out jobs))
                {
                    Console.Error.WriteLine("[错误] --jobs 必须是正整数");
                    return 2;
                }
                break;
            default:
                if (arguments[index].StartsWith("--", StringComparison.Ordinal))
                {
                    Console.Error.WriteLine($"[错误] 未知参数: {arguments[index]}");
                    return 2;
                }
                paths.Add(arguments[index]);
                break;
        }
    }
    if (level is < 0 or > 8)
    {
        Console.Error.WriteLine("[错误] --level 必须在 0-8 之间");
        return 2;
    }
    if (jobs <= 0)
    {
        Console.Error.WriteLine("[错误] --jobs 必须是正整数");
        return 2;
    }
    if (paths.Count == 0)
    {
        Console.Error.WriteLine("用法: dvda convert <目录或 .m4a 文件>... [--in-place] [--dry-run] [--level N] [--jobs N]");
        return 2;
    }
    foreach (var tool in new[] { options.Ffmpeg, options.Ffprobe, options.Metaflac })
    {
        if (!ToolExists(tool))
        {
            Console.Error.WriteLine($"[错误] 找不到 {tool}");
            return 2;
        }
    }
    try
    {
        var converter = new M4aFlacConverter(
            new ProcessRunner(), options.Ffmpeg, options.Ffprobe, options.Metaflac,
            new AlacEndRepairer(new ProcessRunner(), options.Ffprobe));
        var results = await converter.ConvertAsync(paths, level, dryRun, jobs, inPlace);
        foreach (var result in results)
        {
            var name = Path.GetFileName(result.SourcePath);
            if (result.Status == "DRY")
            {
                Console.WriteLine($"[DRY] {name}  " +
                    (result.RepairedFrames > 0
                        ? $"需修补 {result.RepairedFrames} 帧"
                        : "无需修补"));
                continue;
            }
            if (result.Status != "OK")
            {
                Console.WriteLine($"[FAIL] {name} {result.Error ?? string.Empty}");
                continue;
            }

            var details = new List<string>();
            if (result.RepairedFrames > 0) details.Add($"修补 {result.RepairedFrames} 帧");
            if (result.PcmMd5 is { Length: > 0 })
            {
                details.Add($"MD5 {result.PcmMd5[..Math.Min(12, result.PcmMd5.Length)]}");
            }
            details.Add($"标签 {result.DestinationTagCount} 项");
            if (result.HasCover)
            {
                details.Add(result.CoverExact ? "封面逐字节一致" : "封面存在");
            }
            if (result.MissingTags is { Count: > 0 })
            {
                details.Add("缺标签 " + string.Join(", ",
                    result.MissingTags.Take(5).Select(tag => $"{tag.Key}={tag.Value}")));
            }
            if (result.Picture is not null)
            {
                details.Add($"封面 type={result.Picture.Type} {result.Picture.MimeType} " +
                    $"{result.Picture.Width}x{result.Picture.Height}/{result.Picture.Depth}bit");
            }
            if (result.SourceDeleteError is { Length: > 0 })
            {
                details.Add($"源文件删除失败: {result.SourceDeleteError}");
            }
            Console.WriteLine($"[OK] {name}  ({string.Join(", ", details)})");
        }
        return results.Any(result => result.Status == "FAIL") ? 1 : 0;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[转换失败] {exception.Message}");
        return 1;
    }
}

if (command == "plan")
{
    if (arguments.Count > 0)
    {
        Console.Error.WriteLine($"[错误] plan 未知参数: {arguments[0]}");
        return 2;
    }
    try
    {
        var plan = new BuildPlanService(options).Create();
        BuildPlanService.Print(plan, options, Console.Out);
        return plan.HasErrors ? 1 : 0;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[规划失败] {exception.Message}");
        return 1;
    }
}

if (command == "verify")
{
    var mode = arguments.Count > 0 && !arguments[0].StartsWith("--", StringComparison.Ordinal)
        ? arguments[0]
        : "all";
    if (arguments.Count > 0 && arguments[0] == mode)
    {
        arguments.RemoveAt(0);
    }
    if (mode is not ("all" or "quick" or "capacity" or "audit" or "menu" or "timeline" or "lossless" or "config"))
    {
        Console.Error.WriteLine("用法: dvda verify [config|quick|capacity|audit|menu|timeline|lossless|all] [--iso FILE]");
        return 2;
    }
    string? explicitMenuIso = null;
    if (mode == "menu" && arguments.Count == 2 && arguments[0] == "--iso")
    {
        explicitMenuIso = arguments[1];
        arguments.Clear();
    }
    if (arguments.Count > 0)
    {
        Console.Error.WriteLine($"[错误] verify {mode} 未知参数: {arguments[0]}");
        return 2;
    }
    if (mode == "config")
    {
        PrintConfiguration(options);
        return 0;
    }
    try
    {
        var pipeline = new VerificationPipeline(options);
        var failed = false;
        var unavailable = false;

        if (mode is "all" or "capacity")
        {
            Console.WriteLine("=================== 容量与结构 ===================");
            foreach (var result in pipeline.VerifyCapacity())
            {
                Console.WriteLine($"{Path.GetFileName(result.IsoPath),-40} {result.Size,13:N0} B  " +
                    $"余 {result.Limit - result.Size:N0} B  卷标: {result.VolumeIdentifier}");
                PrintIssues(result.Issues, false, ref failed, ref unavailable);
                if (result.Succeeded) Console.WriteLine("[OK] 容量与顶层结构通过");
            }
        }
        if (mode is "all" or "quick" or "audit")
        {
            Console.WriteLine(mode == "quick"
                ? "=================== 快速结构校验 ==================="
                : "=================== 光盘一致性审计 ===================");
            var verifier = new DiscVerifier();
            var manifest = File.Exists(options.ManifestPath) ? options.ManifestPath : null;
            var log = File.Exists(options.BuildLogPath) ? options.BuildLogPath : null;
            var results = mode == "quick"
                ? verifier.QuickCheck(
                    options.FinalDirectory, manifest, log, options.IsoPrefix)
                : verifier.Audit(
                    options.FinalDirectory,
                    log ?? options.BuildLogPath,
                    manifest,
                    isoPrefix: options.IsoPrefix);
            foreach (var result in results)
            {
                Console.WriteLine($"=== {Path.GetFileName(result.IsoPath)}: {result.TrackCount} 轨 ===");
                PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
                if (result.Succeeded) Console.WriteLine("[OK] 校验通过");
            }
        }
        if (mode is "all" or "menu")
        {
            Console.WriteLine("=================== 菜单校验（AMG / ASVS） ===================");
            if (explicitMenuIso is not null)
            {
                var result = await pipeline.VerifyMenuIsoAsync(explicitMenuIso);
                Console.WriteLine($"=== {Path.GetFileName(result.IsoPath)}: " +
                    $"菜单 {result.MenuPages} 页 / 静图 {result.StillPictures} 张 ===");
                PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
                if (result.Succeeded) Console.WriteLine("[OK] AMG/ASVS 结构与菜单画面通过");
            }
            else if (!options.MenuEnabled)
            {
                Console.WriteLine("[跳过] DVDA_MENU=off，成品按预期没有菜单。");
            }
            else
            {
                foreach (var result in await pipeline.VerifyMenuAsync())
                {
                    Console.WriteLine($"=== {Path.GetFileName(result.IsoPath)}: " +
                        $"菜单 {result.MenuPages} 页 / 静图 {result.StillPictures} 张 ===");
                    PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
                    if (result.Succeeded) Console.WriteLine("[OK] AMG/ASVS 结构通过");
                }
            }
        }
        if (mode is "all" or "timeline")
        {
            Console.WriteLine("=================== 时间轴校验（PTS） ===================");
            var result = pipeline.VerifyTimeline();
            PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
            if (result.Succeeded) Console.WriteLine("[OK] 全部盘、全部音频组 PTS 时间轴通过");
        }
        if (mode is "all" or "lossless")
        {
            Console.WriteLine("=================== MLP 无损验证 ===================");
            var result = await pipeline.VerifyLosslessAsync();
            PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
            if (result.Succeeded) Console.WriteLine("[OK] 源 PCM 与成品 MLP 校验通过");
        }
        if (mode == "all") return failed ? 1 : unavailable ? 2 : 0;
        return failed ? 1 : unavailable ? 2 : 0;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[校验失败] {exception.Message}");
        return 1;
    }
}

if (command is "quick-check" or "audit")
{
    var isoDirectory = options.FinalDirectory;
    string? manifestPath = File.Exists(options.ManifestPath) ? options.ManifestPath : null;
    string? buildLogPath = File.Exists(options.BuildLogPath) ? options.BuildLogPath : null;
    var explicitIsoDirectory = false;
    var explicitManifest = false;
    var explicitBuildLog = false;
    for (var index = 0; index < arguments.Count;)
    {
        var option = arguments[index];
        if (option is not ("--iso-dir" or "--manifest" or "--log") ||
            index + 1 >= arguments.Count ||
            arguments[index + 1].StartsWith("--", StringComparison.Ordinal))
        {
            Console.Error.WriteLine(
                $"用法: dvda {command} [--iso-dir DIR] [--manifest FILE] [--log FILE]");
            return 2;
        }
        var value = arguments[index + 1];
        switch (option)
        {
            case "--iso-dir":
                isoDirectory = value;
                explicitIsoDirectory = true;
                break;
            case "--manifest":
                manifestPath = value;
                explicitManifest = true;
                break;
            case "--log":
                buildLogPath = value;
                explicitBuildLog = true;
                break;
        }
        arguments.RemoveRange(index, 2);
    }
    if (explicitIsoDirectory && !Directory.Exists(isoDirectory))
    {
        Console.Error.WriteLine($"[错误] ISO 目录不存在: {isoDirectory}");
        return 2;
    }
    if (explicitManifest && (manifestPath is null || !File.Exists(manifestPath)))
    {
        Console.Error.WriteLine($"[错误] manifest 不存在: {manifestPath}");
        return 2;
    }
    if (explicitBuildLog && (buildLogPath is null || !File.Exists(buildLogPath)))
    {
        Console.Error.WriteLine($"[错误] 构建日志不存在: {buildLogPath}");
        return 2;
    }
    try
    {
        var verifier = new DiscVerifier();
        var results = command == "audit"
            ? verifier.Audit(
                isoDirectory,
                buildLogPath ?? options.BuildLogPath,
                manifestPath,
                allowLogFallback: !explicitBuildLog,
                isoPrefix: options.IsoPrefix)
            : verifier.QuickCheck(
                isoDirectory, manifestPath, buildLogPath, options.IsoPrefix);
        var failed = false;
        var unavailable = false;
        foreach (var result in results)
        {
            Console.WriteLine($"=== {Path.GetFileName(result.IsoPath)}: {result.TrackCount} 轨 ===");
            PrintIssues(result.Issues, result.Unavailable, ref failed, ref unavailable);
            if (result.Succeeded)
            {
                Console.WriteLine("[OK] 校验通过");
            }
        }
        return failed ? 1 : unavailable ? 2 : 0;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[校验失败] {exception.Message}");
        return 1;
    }
}

if (command == "prepare")
{
    if (arguments.Count > 0)
    {
        Console.Error.WriteLine($"[错误] prepare 未知参数: {arguments[0]}");
        return 2;
    }
    var missing = options.MissingRequiredValues();
    if (missing.Count > 0)
    {
        Console.Error.WriteLine($"[配置缺失] {string.Join(", ", missing)}");
        return 2;
    }
    if (!Directory.Exists(options.SourceDirectory))
    {
        Console.Error.WriteLine($"[缺少] 音源目录不存在: {options.SourceDirectory}");
        return 2;
    }

    try
    {
        var result = await new PreparationPipeline(options).RunAsync();
        Console.WriteLine();
        Console.WriteLine(
            $"已校验 {result.CheckedTracks} 首；失败 {result.FailureCount} 首，" +
            $"警告 {result.WarningCount} 首");
        if (result.FailureCount > 0)
        {
            Console.Error.WriteLine("未生成 manifest.json；请修复音源后重试。");
            return 1;
        }
        return 0;
    }
    catch (Exception exception)
    {
        Console.Error.WriteLine($"[准备失败] {exception.Message}");
        return 1;
    }
}

if (command != "config")
{
    Console.Error.WriteLine("用法: dvda [config|prepare|plan|build|convert|verify|quick-check|audit|aob-pts|mlp|alac|iso] [--config PATH]");
    return 2;
}

var configArguments = arguments.Where(argument =>
    argument is not "--check" and not "--shell" and not "--shell-all").ToArray();
if (configArguments.Length > 0 || arguments.Count(argument => argument == "--check") > 1 ||
    arguments.Count(argument => argument == "--shell") > 1 ||
    arguments.Count(argument => argument == "--shell-all") > 1 ||
    arguments.Count(argument => argument is "--check" or "--shell" or "--shell-all") > 1)
{
    Console.Error.WriteLine(configArguments.Length > 0
        ? $"[错误] config 未知参数: {configArguments[0]}"
        : "[错误] config 的 --check、--shell 与 --shell-all 不能重复或同时使用");
    return 2;
}

if (arguments.Contains("--check"))
{
    var missing = options.MissingRequiredValues();
    if (missing.Count == 0)
    {
        return 0;
    }

    Console.Error.WriteLine(new string('=', 68));
    Console.Error.WriteLine("[配置缺失] 请编辑配置文件后重试");
    Console.Error.WriteLine(new string('=', 68));
    if (options.ConfigPath is { Length: > 0 } path)
    {
        Console.Error.WriteLine($"  配置文件: {path}");
    }
    else
    {
        Console.Error.WriteLine("  未找到 config.sh —— 请把它与程序放在同一目录，");
        Console.Error.WriteLine("  或设置环境变量 DVDA_CONFIG 指向它");
    }
    Console.Error.WriteLine();
    foreach (var key in missing)
    {
        var label = key switch
        {
            "DVDA_SRC" => "音源目录",
            "DVDA_FINAL_DIR" => "ISO 输出目录",
            _ => key,
        };
        Console.Error.WriteLine($"  还未填写: {label}");
        Console.Error.WriteLine($"            {key}=\"/你的/路径\"");
    }
    Console.Error.WriteLine();
    Console.Error.WriteLine("  示例:");
    Console.Error.WriteLine("    DVDA_SRC=\"/mnt/d/Music/我的专辑\"");
    Console.Error.WriteLine("    DVDA_FINAL_DIR=\"/mnt/d/DVD_Output\"");
    Console.Error.WriteLine();
    return 2;
}

if (arguments.Contains("--shell"))
{
    foreach (var pair in options.ToShellPairs())
    {
        Console.WriteLine(ShellFormatter.Assignment(pair.Key, pair.Value));
    }
    return 0;
}

if (arguments.Contains("--shell-all"))
{
    foreach (var pair in options.ToShellPairsAll())
    {
        Console.WriteLine(ShellFormatter.Assignment(pair.Key, pair.Value));
    }
    return 0;
}

PrintConfiguration(options);
return 0;

static void PrintConfiguration(DvdaOptions options)
{
    var keys = options.EffectiveKeys();
    Console.WriteLine($"配置文件: {options.ConfigPath ?? "（未找到 config.sh，使用默认值）"}");
    Console.WriteLine("优先级  : 环境变量 > config.sh > 内置默认值" +
        (options.HasEnvironmentOverrides(keys) ? "（当前有环境变量覆盖）" : string.Empty));
    foreach (var key in keys)
    {
        Console.WriteLine($"  {key,-24} = {QuoteConfigurationValue(options.Get(key))}  [{options.ValueSource(key)}]");
    }

    Console.WriteLine();
    Console.WriteLine("派生路径:");
    foreach (var pair in new[]
    {
        ("manifest", options.ManifestPath),
        ("report", options.ReportPath),
        ("out_root", options.OutputRoot),
        ("tmp_root", options.TemporaryRoot),
        ("iso_dir", options.IsoDirectory),
        ("mlp_dir", options.MlpDirectory),
        ("mlp_index", options.MlpIndexPath),
        ("alac_fix_dir", options.AlacFixDirectory),
        ("build_log", options.BuildLogPath),
    })
    {
        Console.WriteLine($"  {pair.Item1,-14} = {pair.Item2}");
    }

    Console.WriteLine();
    Console.WriteLine("MLP 来源:");
    if (options.MlpSource == "external")
    {
        var directory = options.MlpExternalDirectory;
        var status = directory.Length > 0 && Directory.Exists(directory) ? "✔" : "✗ 目录不存在";
        Console.WriteLine($"  external      = {(directory.Length > 0 ? directory : "(未设 DVDA_MLP_EXTERNAL_DIR)")}   {status}");
        Console.WriteLine("                  （跳过编码；按 <外部目录>/<专辑目录>/<曲名>.mlp 取文件）");
    }
    else
    {
        Console.WriteLine($"  ffmpeg        = 本工具链自行编码 -> {options.MlpDirectory}");
    }

    Console.WriteLine();
    Console.WriteLine("工具:");
    foreach (var pair in new[]
    {
        ("dvda-author", options.DvdaAuthor),
        ("mkisofs", options.Mkisofs),
        ("ffmpeg", options.Ffmpeg),
        ("ffprobe", options.Ffprobe),
    })
    {
        var isPath = pair.Item2.Contains('/') || pair.Item2.Contains('\\');
        var status = isPath ? (File.Exists(pair.Item2) ? "✔" : "✗ 不存在") : "(用 PATH 解析)";
        Console.WriteLine($"  {pair.Item1,-14} = {pair.Item2}   {status}");
    }

    Console.WriteLine();
    Console.WriteLine("分盘:");
    Console.WriteLine($"  max_discs         = {(options.MaxDiscs == 0 ? "不限制" : options.MaxDiscs)}");
    Console.WriteLine($"  group_track_limit = {options.GroupTrackLimit}");
    Console.WriteLine($"  disc_bytes        = {options.DiscBytes:N0}");

    Console.WriteLine();
    Console.WriteLine("目录就绪检查:");
    PrintDirectoryStatus("音源", options.SourceDirectory, allowMissing: false);
    PrintDirectoryStatus("输出", options.FinalDirectory, allowMissing: true);
    PrintDirectoryStatus("工作目录", options.BuildDirectory, allowMissing: false);
    Console.WriteLine();
}

static string QuoteConfigurationValue(string value) =>
    "'" + value.Replace("\\", "\\\\", StringComparison.Ordinal)
        .Replace("'", "\\'", StringComparison.Ordinal) + "'";

static void PrintDirectoryStatus(string label, string directory, bool allowMissing)
{
    if (directory.Length == 0)
    {
        Console.WriteLine($"  {label,-8} = (未配置)");
    }
    else if (Directory.Exists(directory))
    {
        Console.WriteLine($"  {label,-8} = {directory}   ✔");
    }
    else if (allowMissing)
    {
        Console.WriteLine($"  {label,-8} = {directory}   （尚不存在，出盘时会自动创建）");
    }
    else
    {
        Console.WriteLine($"  {label,-8} = {directory}   ✗ 不存在");
    }
}

static void PrintIssues(
    IReadOnlyList<VerificationIssue> issues,
    bool sectionUnavailable,
    ref bool failed,
    ref bool unavailable)
{
    foreach (var issue in issues)
    {
        if (IsUnavailableIssue(issue) || sectionUnavailable && issues.Count == 1)
        {
            unavailable = true;
            Console.WriteLine($"[跳过] {issue.Code}: {issue.Message}");
        }
        else
        {
            failed = true;
            Console.WriteLine($"[FAIL] {issue.Code}: {issue.Message}");
        }
    }
}

static bool IsUnavailableIssue(VerificationIssue issue) =>
    issue.Code.EndsWith("_UNAVAILABLE", StringComparison.Ordinal) ||
    issue.Code is "BUILD_LOG_MISSING" or "TRACK_TABLE_MISSING" or
        "DVDA_COMMAND_MISSING" or "MLP_INDEX_MISSING" or "MLP_INDEX_DRY_RUN";

static async Task<(
    long? DecodedSamples,
    long? DeclaredSamples,
    int ErrorCount,
    bool ProcessSucceeded)> CheckAlacDecodeAsync(
    string path,
    string ffmpeg,
    string ffprobe)
{
    var runner = new ProcessRunner();
    var decode = await runner.RunAsync(new ProcessRequest
    {
        FileName = ffmpeg,
        Arguments =
        [
            "-hide_banner", "-nostdin", "-v", "info", "-i", path,
            "-af", "astats=metadata=1", "-f", "null", "-",
        ],
    });
    var text = decode.StandardOutput + Environment.NewLine + decode.StandardError;
    var sampleMatch = System.Text.RegularExpressions.Regex.Match(
        text, @"Number of samples:\s*(\d+)");
    long? decodedSamples = sampleMatch.Success && long.TryParse(
        sampleMatch.Groups[1].Value,
        System.Globalization.NumberStyles.Integer,
        System.Globalization.CultureInfo.InvariantCulture,
        out var sampleValue)
        ? sampleValue
        : null;
    var errorCount = DecodeValidator.ScanErrors(text).Count;
    if (!decode.Succeeded && errorCount == 0) errorCount = 1;

    var declared = await runner.RunAsync(new ProcessRequest
    {
        FileName = ffprobe,
        Arguments =
        [
            "-v", "error", "-select_streams", "a:0",
            "-show_entries", "stream=duration_ts",
            "-of", "default=nw=1:nk=1", path,
        ],
    });
    long? declaredSamples = declared.Succeeded && long.TryParse(
        declared.StandardOutput.Trim(),
        System.Globalization.NumberStyles.Integer,
        System.Globalization.CultureInfo.InvariantCulture,
        out var declaredValue)
        ? declaredValue
        : null;
    return (
        decodedSamples, declaredSamples, errorCount,
        decode.Succeeded && declared.Succeeded);
}

static bool ToolExists(string tool)
{
    if (string.IsNullOrWhiteSpace(tool)) return false;
    if (Path.IsPathRooted(tool) || tool.Contains(Path.DirectorySeparatorChar) ||
        tool.Contains(Path.AltDirectorySeparatorChar))
    {
        return File.Exists(tool);
    }

    var extensions = OperatingSystem.IsWindows()
        ? (Environment.GetEnvironmentVariable("PATHEXT") ?? ".EXE;.CMD;.BAT;.COM")
            .Split(';', StringSplitOptions.RemoveEmptyEntries)
        : [string.Empty];
    foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                 .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
    {
        foreach (var extension in extensions)
        {
            var candidate = Path.Combine(directory, tool);
            if (OperatingSystem.IsWindows() && Path.GetExtension(tool).Length == 0)
            {
                candidate += extension;
            }
            if (File.Exists(candidate)) return true;
        }
    }
    return false;
}
