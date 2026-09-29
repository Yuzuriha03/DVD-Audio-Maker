using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using DvdaMaker.Configuration;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

public sealed class PreparationPipeline
{
    private readonly DvdaOptions _options;
    private readonly AudioMetadataReader _metadataReader;
    private readonly DecodeValidator _decodeValidator;
    private readonly AlacEndRepairer _alacRepairer;

    public PreparationPipeline(DvdaOptions options, ProcessRunner? processRunner = null)
    {
        _options = options;
        var runner = processRunner ?? new ProcessRunner();
        _metadataReader = new AudioMetadataReader(runner, options.Ffprobe);
        _decodeValidator = new DecodeValidator(runner, options.Ffmpeg);
        _alacRepairer = new AlacEndRepairer(runner, options.Ffprobe);
    }

    public async Task<PreparationResult> RunAsync(
        CancellationToken cancellationToken = default)
    {
        if (File.Exists(_options.ManifestPath))
        {
            File.Delete(_options.ManifestPath);
            Console.WriteLine($"[清理] 旧 manifest 已移除: {_options.ManifestPath}");
        }

        var sources = Directory.EnumerateFiles(_options.SourceDirectory, "*", SearchOption.AllDirectories)
            .Where(path => path.EndsWith(".flac", StringComparison.OrdinalIgnoreCase) ||
                           path.EndsWith(".m4a", StringComparison.OrdinalIgnoreCase))
            .Order(StringComparer.Ordinal)
            .ToArray();
        Console.WriteLine($"发现 {sources.Length} 个音频文件 (FLAC/M4A)");

        var tracks = new List<AudioTrackMetadata>(sources.Length);
        foreach (var path in sources)
        {
            tracks.Add(await _metadataReader.ReadAsync(path, cancellationToken).ConfigureAwait(false));
        }

        AlbumNormalizer.Apply(tracks);
        foreach (var track in tracks.Where(track => track.ResampleTo is not null))
        {
            Console.WriteLine(
                $"  [重采样] {track.Title}: {track.SourceSampleRate}/{track.SourceBits} -> " +
                $"{track.SampleRate}/{track.Bits}");
        }
        Console.WriteLine($"共需重采样 {tracks.Count(track => track.ResampleTo is not null)} 首");

        var issues = ValidateChannels(tracks).ToList();
        var manifest = new Dictionary<string, ManifestGroup>(StringComparer.Ordinal);
        var repairs = new List<AppliedAudioRepair>();
        var checkedTracks = 0;

        foreach (var parameterGroup in tracks
                     .GroupBy(track => (track.SampleRate, track.Bits))
                     .OrderBy(group => group.Key.SampleRate)
                     .ThenBy(group => group.Key.Bits))
        {
            var groupName = $"group_{parameterGroup.Key.SampleRate}_{parameterGroup.Key.Bits}";
            var ordered = parameterGroup
                .OrderBy(track => track.Date, StringComparer.Ordinal)
                .ThenBy(track => AlbumNormalizer.TrackNumber(track.Track))
                .ThenBy(track => track.Title, StringComparer.Ordinal)
                .ToArray();
            var files = new List<ManifestTrack>(ordered.Length);

            for (var index = 0; index < ordered.Length; index++)
            {
                var track = ordered[index];
                checkedTracks++;
                var sourcePath = track.Path;
                var originalSource = (string?)null;
                IReadOnlyList<string>? repairDetail = null;
                var repaired = 0;
                var check = await _decodeValidator.CheckAsync(
                    sourcePath, track.ResampleTo, cancellationToken).ConfigureAwait(false);
                var expected = track.Duration > 0
                    ? (long)Math.Round(track.Duration * track.SampleRate)
                    : (long?)null;
                if (check.ErrorCount > 0 ||
                    (check.Samples is not null && expected is not null && check.Samples != expected))
                {
                    var repair = await _alacRepairer.TryRepairAsync(
                        sourcePath, _options.AlacFixDirectory, cancellationToken).ConfigureAwait(false);
                    if (repair is not null)
                    {
                        var repairedCheck = await _decodeValidator.CheckAsync(
                            repair.OutputPath, track.ResampleTo, cancellationToken).ConfigureAwait(false);
                        if (repairedCheck.ErrorCount == 0 && repairedCheck.Samples is not null &&
                            (expected is null || repairedCheck.Samples == expected))
                        {
                            originalSource = sourcePath;
                            sourcePath = repair.OutputPath;
                            check = repairedCheck;
                            repaired = repair.Patches.Count;
                            repairDetail = repair.Patches.Select(FormatRepairDetail).ToArray();
                            repairs.Add(new AppliedAudioRepair(
                                originalSource, sourcePath, repair.Patches));
                            Console.WriteLine(
                                $"  ++ [已修复] {track.Title}: ALAC END 标记 {repaired} 处，解码采样数已达标");
                        }
                        else
                        {
                            try
                            {
                                File.Delete(repair.OutputPath);
                            }
                            catch (IOException)
                            {
                            }
                        }
                    }
                }

                var issue = Evaluate(track, sourcePath, check, expected, repaired);
                if (issue is not null)
                {
                    issues.Add(issue);
                    Console.WriteLine($"  {(issue.Level == "FAIL" ? "!!" : " ?")} " +
                                      $"[{issue.Level}] {track.Title}: {issue.Reason}");
                }

                files.Add(new ManifestTrack
                {
                    Number = index + 1,
                    Source = sourcePath,
                    Name = $"{groupName}/{index + 1:0000}__{AlbumNormalizer.SafeBaseName(track.Path)}",
                    Title = track.Title,
                    Date = track.Date,
                    Track = track.Track,
                    Album = track.Album,
                    Duration = Math.Round(track.Duration, 6),
                    ResampleTo = track.ResampleTo,
                    Repaired = repaired,
                    RepairDetail = repairDetail,
                    OriginalSource = originalSource,
                });
            }

            manifest[groupName] = new ManifestGroup
            {
                SampleRate = parameterGroup.Key.SampleRate,
                Bits = parameterGroup.Key.Bits,
                Count = files.Count,
                Files = files,
            };
            Console.WriteLine($"{groupName}: {files.Count} 首");
        }

        var result = new PreparationResult(manifest, issues, checkedTracks, repairs);
        WriteReport(result);
        if (result.FailureCount == 0)
        {
            WriteManifest(manifest);
        }
        return result;
    }

    private IEnumerable<ValidationIssue> ValidateChannels(
        IReadOnlyList<AudioTrackMetadata> tracks)
    {
        foreach (var group in tracks.GroupBy(track => (track.SampleRate, track.Bits)))
        {
            var channels = group.GroupBy(track => track.Channels).ToArray();
            if (channels.Length <= 1)
            {
                continue;
            }
            var description = string.Join("; ", channels.OrderBy(item => item.Key)
                .Select(item => $"{item.Key} 声道 × {item.Count()} 首"));
            yield return new ValidationIssue(
                "FAIL",
                $"音频组 {group.Key.SampleRate}Hz/{group.Key.Bits}bit 声道数不一致",
                _options.SourceDirectory,
                description,
                group.Take(10).Select(track => $"     {track.Title}").ToArray());
        }
    }

    private ValidationIssue? Evaluate(
        AudioTrackMetadata track,
        string sourcePath,
        DecodeCheckResult check,
        long? expected,
        int repaired)
    {
        string? level = null;
        var reasons = new List<string>();
        var detail = new List<string>();
        if (repaired > 0)
        {
            detail.Add($"已修复 ALAC END 标记 {repaired} 处（原文件未改动）");
        }
        if (check.ErrorCount > 0)
        {
            level = "FAIL";
            reasons.Add($"解码报错 {check.ErrorCount} 处");
            detail.AddRange(check.ErrorLines);
        }
        if (check.Samples is null)
        {
            level = "FAIL";
            reasons.Add("未能读到解码采样数(astats 无输出)");
            detail.Add("ffmpeg 未输出 'Number of samples',无法校验完整性");
        }
        else
        {
            double? lossMilliseconds = expected is null
                ? null
                : (expected.Value - check.Samples.Value) /
                  (double)track.SampleRate * 1000;
            detail.Add(
                $"源声明 {track.Duration:F3} 秒 -> 期望 {expected?.ToString() ?? "?"} 采样 / " +
                $"实解 {check.Samples} 采样 / " +
                (lossMilliseconds is null
                    ? "?"
                    : $"{(lossMilliseconds > 0 ? "少" : "多")} {Math.Abs(lossMilliseconds.Value):F0} ms"));
            if (lossMilliseconds is not null &&
                Math.Abs(lossMilliseconds.Value) > _options.LossErrorSeconds * 1000)
            {
                level = "FAIL";
                reasons.Add(
                    $"解码采样数{(lossMilliseconds > 0 ? "少" : "多")} " +
                    $"{Math.Abs(lossMilliseconds.Value):F0} ms");
            }
            else if (lossMilliseconds is not null && level is null &&
                     Math.Abs(lossMilliseconds.Value) > _options.LossWarningSeconds * 1000)
            {
                level = "WARN";
                reasons.Add(
                    $"解码采样数{(lossMilliseconds > 0 ? "少" : "多")} " +
                    $"{Math.Abs(lossMilliseconds.Value):F0} ms");
            }
        }

        return level is null
            ? null
            : new ValidationIssue(level, track.Title, sourcePath,
                string.Join("; ", reasons), detail);
    }

    private static string FormatRepairDetail(AlacFramePatch patch)
    {
        var minutes = (int)(patch.PresentationTime / 60);
        var seconds = patch.PresentationTime - minutes * 60;
        return $"{patch.PresentationTime,10:F3}s ({minutes}分{seconds:00.00}秒)  " +
               $"标记 {Convert.ToString(patch.PreviousBits, 2).PadLeft(3, '0')} -> 111  " +
               $"({patch.SampleCount} 采样)";
    }

    private void WriteManifest(IReadOnlyDictionary<string, ManifestGroup> manifest)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(_options.ManifestPath)!);
        var json = JsonSerializer.Serialize(manifest, JsonOptions);
        File.WriteAllText(_options.ManifestPath, json + Environment.NewLine, new UTF8Encoding(false));
        Console.WriteLine($"manifest.json 已生成 -> {_options.ManifestPath}");
        Console.WriteLine($"总计 {manifest.Values.Sum(group => group.Count)} 首");
    }

    private void WriteReport(PreparationResult result)
    {
        var failures = result.Issues.Where(issue => issue.Level == "FAIL").ToArray();
        var warnings = result.Issues.Where(issue => issue.Level == "WARN").ToArray();
        var lines = new List<string>
        {
            "音源校验报告",
            new('=', 68),
            $"已校验 {result.CheckedTracks} 首；失败 {failures.Length} 首，警告 {warnings.Length} 首",
            string.Empty,
        };
        if (result.Issues.Count == 0)
        {
            lines.Add("全部通过：无解码错误，解码采样数与源声明一致，组内参数一致。");
        }
        foreach (var issue in failures.Concat(warnings))
        {
            lines.Add($"[{issue.Level}] {issue.Title}");
            lines.Add($"    原因: {issue.Reason}");
            lines.Add($"    源文件: {issue.Path}");
            lines.AddRange(issue.Detail.Select(detail => $"    {detail}"));
            lines.Add(string.Empty);
        }
        if (result.Repairs.Count > 0)
        {
            lines.Add(string.Empty);
            lines.Add(new string('-', 68));
            lines.Add("ALAC END 标记修复记录");
            lines.Add(new string('-', 68));
            lines.Add("说明：仅补写 Apple ALAC 未压缩帧尾缺失的 3 位 END 标记，原文件不修改。");
            lines.Add(string.Empty);
            foreach (var repair in result.Repairs)
            {
                lines.Add($"[已修复 {repair.Patches.Count} 帧] {Path.GetFileName(repair.OriginalPath)}");
                lines.Add($"    原文件: {repair.OriginalPath}");
                lines.Add($"    修复后: {repair.RepairedPath}");
                lines.AddRange(repair.Patches.Select(patch => $"    {FormatRepairDetail(patch)}"));
                lines.Add(string.Empty);
            }
        }
        Directory.CreateDirectory(Path.GetDirectoryName(_options.ReportPath)!);
        File.WriteAllText(_options.ReportPath,
            string.Join(Environment.NewLine, lines) + Environment.NewLine,
            new UTF8Encoding(false));
        Console.WriteLine($"报告已写入: {_options.ReportPath}");
    }

    private static JsonSerializerOptions JsonOptions { get; } = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        PropertyNamingPolicy = null,
    };
}
