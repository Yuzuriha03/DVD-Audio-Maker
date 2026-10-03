using System.Collections.Concurrent;
using System.Text.Json;
using System.Text.RegularExpressions;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

public sealed record M4aConversionResult(
    string SourcePath,
    string DestinationPath,
    string Status,
    string? Error,
    int RepairedFrames,
    string? PcmMd5,
    int SourceTagCount,
    int DestinationTagCount,
    bool HasCover,
    bool CoverExact,
    IReadOnlyList<(string Key, string Value)>? MissingTags = null,
    FlacPictureDescriptor? Picture = null,
    string? SourceDeleteError = null);

public sealed record FlacPictureDescriptor(
    int Type,
    string MimeType,
    int Width,
    int Height,
    int Depth);

public sealed class M4aFlacConverter(
    ProcessRunner runner,
    string ffmpeg,
    string ffprobe,
    AlacEndRepairer alacRepairer)
{
    private static readonly Regex Md5Pattern = new("MD5=([0-9a-fA-F]+)", RegexOptions.Compiled);
    private static readonly Regex PictureTypePattern = new(
        @"^\s*type:\s*(\d+)(?!\d)(?![^\r\n]*\(PICTURE\))", RegexOptions.Multiline | RegexOptions.Compiled | RegexOptions.IgnoreCase);
    private static readonly Regex PictureMimePattern = new(
        @"^\s*MIME type:\s*(\S+)", RegexOptions.Multiline | RegexOptions.Compiled | RegexOptions.IgnoreCase);
    private static readonly Regex PictureWidthPattern = new(
        @"^\s*width:\s*(\d+)", RegexOptions.Multiline | RegexOptions.Compiled | RegexOptions.IgnoreCase);
    private static readonly Regex PictureHeightPattern = new(
        @"^\s*height:\s*(\d+)", RegexOptions.Multiline | RegexOptions.Compiled | RegexOptions.IgnoreCase);
    private static readonly Regex PictureDepthPattern = new(
        @"^\s*depth:\s*(\d+)", RegexOptions.Multiline | RegexOptions.Compiled | RegexOptions.IgnoreCase);
    private static readonly IReadOnlyDictionary<string, string> Rename =
        new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["sort_name"] = "TITLESORT",
            ["sort_album"] = "ALBUMSORT",
            ["sort_artist"] = "ARTISTSORT",
            ["sort_album_artist"] = "ALBUMARTISTSORT",
            ["sort_composer"] = "COMPOSERSORT",
            ["track"] = "TRACKNUMBER",
            ["disc"] = "DISCNUMBER",
            ["album_artist"] = "ALBUMARTIST",
            ["upc"] = "BARCODE",
        };

    private static readonly HashSet<string> Drop = new(StringComparer.OrdinalIgnoreCase)
    {
        "major_brand", "minor_brand", "minor_version", "compatible_brands",
        "creation_time", "encoder", "vendor_id", "handler_name", "language",
    };

    public async Task<IReadOnlyList<M4aConversionResult>> ConvertAsync(
        IEnumerable<string> paths,
        int compressionLevel = 8,
        bool dryRun = false,
        int jobs = 1,
        bool deleteSources = false,
        CancellationToken cancellationToken = default)
    {
        if (compressionLevel is < 0 or > 8)
        {
            throw new ArgumentOutOfRangeException(nameof(compressionLevel), "FLAC 压缩级别必须在 0-8 之间。");
        }
        if (jobs <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(jobs), "并发数必须是正整数。");
        }
        var files = Collect(paths).ToArray();
        if (files.Length == 0)
        {
            throw new InvalidOperationException("没有可处理的 .m4a 文件。");
        }

        var results = new ConcurrentBag<M4aConversionResult>();
        await Parallel.ForEachAsync(
            files,
            new ParallelOptions
            {
                MaxDegreeOfParallelism = jobs,
                CancellationToken = cancellationToken,
            },
            async (source, token) =>
            {
                try
                {
                    results.Add(await ConvertOneAsync(
                        source, compressionLevel, dryRun, token).ConfigureAwait(false));
                }
                catch (OperationCanceledException)
                {
                    throw;
                }
                catch (Exception exception)
                {
                    var destination = Path.ChangeExtension(source, ".flac");
                    SafeDelete(destination);
                    results.Add(Failure(source, destination, exception.Message));
                }
            })
            .ConfigureAwait(false);

        var ordered = results.OrderBy(result => result.SourcePath, StringComparer.OrdinalIgnoreCase).ToArray();
        if (deleteSources && !dryRun && ordered.All(result => result.Status == "OK"))
        {
            ordered = ApplySourceDeletion(ordered);
        }
        return ordered;
    }

    public static IReadOnlyList<(string Key, string Value)> NormalizeTags(
        IReadOnlyDictionary<string, string> raw)
    {
        var output = new List<(string Key, string Value)>();
        var seen = new HashSet<(string Key, string Value)>();
        foreach (var pair in raw.OrderBy(pair => pair.Key, StringComparer.OrdinalIgnoreCase))
        {
            if (Drop.Contains(pair.Key) || string.IsNullOrWhiteSpace(pair.Value)) continue;
            var key = Rename.TryGetValue(pair.Key, out var renamed)
                ? renamed
                : pair.Key.ToUpperInvariant();
            var value = pair.Value;
            if (seen.Add((key, value))) output.Add((key, value));
        }
        return output;
    }

    private async Task<M4aConversionResult> ConvertOneAsync(
        string source,
        int level,
        bool dryRun,
        CancellationToken cancellationToken)
    {
        var destination = Path.ChangeExtension(source, ".flac");
        var codec = await ProbeAudioCodecAsync(source, cancellationToken).ConfigureAwait(false);
        if (!string.Equals(codec, "alac", StringComparison.OrdinalIgnoreCase))
        {
            return Failure(source, destination,
                codec is null ? "读不到首音频流编码" : $"首音频流不是 ALAC: {codec}");
        }
        var rawTags = await ProbeTagsAsync(source, cancellationToken).ConfigureAwait(false);
        if (rawTags.Count == 0)
        {
            return Failure(source, destination, "读不到标签");
        }

        var repairedFrames = 0;
        if (dryRun)
        {
            try
            {
                var inspection = await alacRepairer.InspectAsync(source, cancellationToken)
                    .ConfigureAwait(false);
                repairedFrames = inspection.Patches.Count;
            }
            catch (Exception exception) when (exception is InvalidDataException or IOException)
            {
                return Failure(source, destination, $"缺陷检测失败: {exception.Message}");
            }
            return new M4aConversionResult(source, destination, "DRY", null, repairedFrames,
                null, rawTags.Count, 0, false, false);
        }

        var temporary = Path.Combine(Path.GetTempPath(), "dvda-m4a2flac", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(temporary);
        try
        {
            var repair = await alacRepairer.TryRepairAsync(source, temporary, cancellationToken)
                .ConfigureAwait(false);
            var work = repair?.OutputPath ?? source;
            repairedFrames = repair?.Patches.Count ?? 0;
            var tags = NormalizeTags(rawTags);
            var hasCover = await HasCoverAsync(work, cancellationToken).ConfigureAwait(false);
            var arguments = new List<string>
            {
                "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                "-i", work, "-map", "0:a:0",
            };
            if (hasCover)
            {
                arguments.AddRange(["-map", "0:v:0", "-c:v", "copy", "-disposition:v:0", "attached_pic"]);
            }
            arguments.AddRange(["-c:a", "flac", "-compression_level", level.ToString(), "-map_metadata", "-1"]);
            foreach (var tag in tags) arguments.AddRange(["-metadata", $"{tag.Key}={tag.Value}"]);
            arguments.Add(destination);
            var conversion = await runner.RunAsync(new ProcessRequest
            {
                FileName = ffmpeg,
                Arguments = arguments,
            }, cancellationToken).ConfigureAwait(false);
            if (!conversion.Succeeded || !File.Exists(destination))
            {
                return Failure(source, destination, $"ffmpeg 转换失败: {conversion.StandardError[..Math.Min(200, conversion.StandardError.Length)]}");
            }

            var referenceMd5 = await AudioMd5Async(work, cancellationToken).ConfigureAwait(false);
            var destinationMd5 = await AudioMd5Async(destination, cancellationToken).ConfigureAwait(false);
            if (referenceMd5 is null || referenceMd5 != destinationMd5)
            {
                File.Delete(destination);
                return Failure(source, destination, $"PCM MD5 不一致 基准={referenceMd5} 输出={destinationMd5}");
            }

            var destinationTags = await ReadVorbisTagsAsync(destination, cancellationToken).ConfigureAwait(false);
            var missingTags = tags.Where(tag => !destinationTags.Contains(tag)).ToArray();
            var coverExact = false;
            FlacPictureDescriptor? picture = null;
            if (hasCover)
            {
                var ext = await CoverExtensionAsync(source, cancellationToken).ConfigureAwait(false);
                var cover = Path.Combine(temporary, "cover_pic" + ext);
                var pictureBlock = FlacMetadataEditor.ReadPicture(destination);
                if (pictureBlock is null)
                {
                    File.Delete(destination);
                    return Failure(source, destination, "FLAC PICTURE block is missing");
                }
                var normalizedPicture = pictureBlock with
                {
                    Descriptor = pictureBlock.Descriptor with { Type = 3 },
                    Description = string.Empty,
                };
                FlacMetadataEditor.ExportPicture(destination, cover);
                FlacMetadataEditor.ReplacePicture(destination, cover, normalizedPicture);
                if (!File.Exists(cover))
                {
                    File.Delete(destination);
                    return Failure(source, destination, "导出 FLAC 封面失败");
                }
                picture = normalizedPicture.Descriptor;
                var expectedMime = ext.ToLowerInvariant() switch
                {
                    ".png" => "image/png",
                    ".bmp" => "image/bmp",
                    ".gif" => "image/gif",
                    ".webp" => "image/webp",
                    ".tif" => "image/tiff",
                    _ => "image/jpeg",
                };
                if (picture is null || picture.Type != 3 ||
                    !string.Equals(picture.MimeType, expectedMime, StringComparison.OrdinalIgnoreCase) ||
                    picture.Width <= 0 || picture.Height <= 0 || picture.Depth <= 0 ||
                    (expectedMime == "image/jpeg" && picture.Depth != 24))
                {
                    File.Delete(destination);
                    return Failure(source, destination,
                        picture is null
                            ? "无法读取规范化后的 FLAC PICTURE 描述"
                            : $"FLAC PICTURE 描述不合规: type={picture.Type}, " +
                              $"mime={picture.MimeType}, {picture.Width}x{picture.Height}, depth={picture.Depth}");
                }
                var sourceCover = Path.Combine(temporary, "cover_source" + ext);
                var extract = await runner.RunAsync(new ProcessRequest
                {
                    FileName = ffmpeg,
                    Arguments = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i", source,
                        "-map", "0:v:0", "-c", "copy", "-f", "image2", sourceCover],
                }, cancellationToken).ConfigureAwait(false);
                if (!extract.Succeeded || !File.Exists(sourceCover))
                {
                    File.Delete(destination);
                    return Failure(source, destination, "提取源封面失败");
                }
                var sourceCoverBytes = File.ReadAllBytes(sourceCover);
                var outputCoverBytes = File.ReadAllBytes(cover);
                if (sourceCoverBytes.Length != outputCoverBytes.Length)
                {
                    File.Delete(destination);
                    return Failure(source, destination,
                        $"封面字节数不符: 源 {sourceCoverBytes.Length} / 输出 {outputCoverBytes.Length}");
                }
                coverExact = sourceCoverBytes.SequenceEqual(outputCoverBytes);
                if (!coverExact)
                {
                    File.Delete(destination);
                    return Failure(source, destination, "封面字节内容不一致");
                }
            }
            return new M4aConversionResult(source, destination, "OK", null, repairedFrames,
                destinationMd5, tags.Count, destinationTags.Count, hasCover, coverExact,
                missingTags, picture);
        }
        catch (Exception exception) when (exception is IOException or InvalidDataException)
        {
            return Failure(source, destination, exception.Message);
        }
        finally
        {
            try
            {
                if (Directory.Exists(temporary)) Directory.Delete(temporary, recursive: true);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }

    private async Task<string?> ProbeAudioCodecAsync(string path, CancellationToken token)
    {
        using var result = await RunJsonAsync(path, "-show_streams", token).ConfigureAwait(false);
        if (!result.RootElement.TryGetProperty("streams", out var streams)) return null;
        foreach (var stream in streams.EnumerateArray())
        {
            if (stream.TryGetProperty("codec_type", out var type) &&
                type.GetString() == "audio" &&
                stream.TryGetProperty("codec_name", out var codec))
            {
                return codec.GetString();
            }
        }
        return null;
    }

    private async Task<IReadOnlyDictionary<string, string>> ProbeTagsAsync(string path, CancellationToken token)
    {
        var result = await RunJsonAsync(path, "-show_format", token).ConfigureAwait(false);
        if (!result.RootElement.TryGetProperty("format", out var format) ||
            !format.TryGetProperty("tags", out var tags)) return new Dictionary<string, string>();
        return tags.EnumerateObject().ToDictionary(property => property.Name, property => property.Value.GetString() ?? "",
            StringComparer.OrdinalIgnoreCase);
    }

    private async Task<bool> HasCoverAsync(string path, CancellationToken token)
    {
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments = ["-v", "error", "-select_streams", "v", "-show_entries", "stream=codec_type", "-of", "csv=p=0", path],
        }, token).ConfigureAwait(false);
        return result.Succeeded && result.StandardOutput.Trim().Length > 0;
    }

    private async Task<string?> AudioMd5Async(string path, CancellationToken token)
    {
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffmpeg,
            Arguments = ["-hide_banner", "-loglevel", "error", "-i", path, "-map", "0:a:0", "-f", "md5", "-"],
        }, token).ConfigureAwait(false);
        return Md5Pattern.Match(result.StandardOutput + result.StandardError).Groups[1].Value.ToLowerInvariant() is { Length: > 0 } value
            ? value
            : null;
    }

    private Task<IReadOnlyList<(string Key, string Value)>> ReadVorbisTagsAsync(string path, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        return Task.FromResult(FlacMetadataEditor.ReadVorbisComments(path));
    }

    internal static FlacPictureDescriptor? ParsePictureDescriptor(string text)
    {
        var type = PictureTypePattern.Match(text);
        var mime = PictureMimePattern.Match(text);
        var width = PictureWidthPattern.Match(text);
        var height = PictureHeightPattern.Match(text);
        var depth = PictureDepthPattern.Match(text);
        if (!type.Success || !mime.Success || !width.Success || !height.Success || !depth.Success ||
            !int.TryParse(type.Groups[1].Value, out var typeValue) ||
            !int.TryParse(width.Groups[1].Value, out var widthValue) ||
            !int.TryParse(height.Groups[1].Value, out var heightValue) ||
            !int.TryParse(depth.Groups[1].Value, out var depthValue))
        {
            return null;
        }
        return new FlacPictureDescriptor(
            typeValue, mime.Groups[1].Value, widthValue, heightValue, depthValue);
    }

    internal static M4aConversionResult[] ApplySourceDeletion(
        IReadOnlyList<M4aConversionResult> results,
        Action<string>? delete = null)
    {
        delete ??= File.Delete;
        var updated = results.ToArray();
        for (var index = 0; index < updated.Length; index++)
        {
            try
            {
                delete(updated[index].SourcePath);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
            {
                updated[index] = updated[index] with
                {
                    SourceDeleteError = exception.Message,
                };
            }
        }
        return updated;
    }

    private async Task<string> CoverExtensionAsync(string path, CancellationToken token)
    {
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments = ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name", "-of", "csv=p=0", path],
        }, token).ConfigureAwait(false);
        return result.StandardOutput.Trim().ToLowerInvariant() switch
        {
            "png" => ".png",
            "bmp" => ".bmp",
            "gif" => ".gif",
            "webp" => ".webp",
            "tiff" => ".tif",
            _ => ".jpg",
        };
    }

    private async Task<JsonDocument> RunJsonAsync(string path, string argument, CancellationToken token)
    {
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments = ["-v", "error", argument, "-of", "json", path],
        }, token).ConfigureAwait(false);
        if (!result.Succeeded)
        {
            throw new InvalidDataException(
                $"ffprobe 读取失败（退出码 {result.ExitCode}）: {path}");
        }
        return JsonDocument.Parse(result.StandardOutput);
    }

    private static IEnumerable<string> Collect(IEnumerable<string> paths) =>
        paths.SelectMany(path => Directory.Exists(path)
            ? Directory.EnumerateFiles(path, "*.m4a", SearchOption.AllDirectories)
            : File.Exists(path) && path.EndsWith(".m4a", StringComparison.OrdinalIgnoreCase)
                ? [path]
                : [])
        .Distinct(StringComparer.OrdinalIgnoreCase)
        .OrderBy(path => path, StringComparer.OrdinalIgnoreCase);

    private static M4aConversionResult Failure(string source, string destination, string error) =>
        new(source, destination, "FAIL", error, 0, null, 0, 0, false, false);

    private static void SafeDelete(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
