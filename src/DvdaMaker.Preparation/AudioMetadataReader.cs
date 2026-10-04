using System.Globalization;
using DvdaMaker.Processes;

namespace DvdaMaker.Preparation;

public sealed class AudioMetadataReader(ProcessRunner processRunner, string ffprobe)
{
    public async Task<AudioTrackMetadata> ReadAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        if (RustBridge.Mode != "managed" && BuiltinMedia.IsBuiltin(ffprobe))
            return await Task.Run(() => RustBridge.InspectAudio<AudioTrackMetadata>("Metadata",
                BuiltinMedia.LibraryPath, path, null, cancellationToken), CancellationToken.None).ConfigureAwait(false);
        var result = await processRunner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments =
            [
                "-v", "error",
                "-select_streams", "a:0",
                "-show_entries", "stream=sample_rate,bits_per_raw_sample,channels,duration_ts,time_base",
                "-show_entries", "format=duration",
                "-show_entries", "format_tags",
                "-of", "default=noprint_wrappers=1",
                path,
            ],
        }, cancellationToken).ConfigureAwait(false);

        if (!result.Succeeded)
        {
            throw new InvalidDataException(
                $"ffprobe 无法读取 {path}（退出码 {result.ExitCode}）{Environment.NewLine}" +
                result.StandardError);
        }

        return RustBridge.Run<AudioTrackMetadata>("metadata.parse", new
        {
            Path = path,
            Output = result.StandardOutput,
        }, () => ParseManaged(path, result.StandardOutput));
    }

    private static AudioTrackMetadata ParseManaged(string path, string standardOutput)
    {
        var sampleRate = 0;
        var bits = 0;
        var channels = 0;
        long? durationTimestamp = null;
        string? timeBase = null;
        double? formatDuration = null;
        var tags = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        foreach (var rawLine in standardOutput.Split('\n'))
        {
            var line = rawLine.Trim();
            if (line.StartsWith("sample_rate=", StringComparison.Ordinal))
            {
                int.TryParse(Value(line), out sampleRate);
            }
            else if (line.StartsWith("bits_per_raw_sample=", StringComparison.Ordinal))
            {
                int.TryParse(Value(line), out bits);
            }
            else if (line.StartsWith("channels=", StringComparison.Ordinal))
            {
                int.TryParse(Value(line), out channels);
            }
            else if (line.StartsWith("duration_ts=", StringComparison.Ordinal) &&
                     long.TryParse(Value(line), out var timestamp))
            {
                durationTimestamp = timestamp;
            }
            else if (line.StartsWith("time_base=", StringComparison.Ordinal))
            {
                timeBase = Value(line);
            }
            else if (line.StartsWith("duration=", StringComparison.Ordinal) &&
                     double.TryParse(Value(line), NumberStyles.Float,
                         CultureInfo.InvariantCulture, out var duration))
            {
                formatDuration = duration;
            }
            else if (line.StartsWith("TAG:", StringComparison.Ordinal) && line.Contains('='))
            {
                var separator = line.IndexOf('=');
                tags[line[4..separator].ToLowerInvariant()] = line[(separator + 1)..];
            }
        }

        var actualDuration = CalculateDuration(durationTimestamp, timeBase) ?? formatDuration ?? 0;
        var fallbackTitle = System.IO.Path.GetFileNameWithoutExtension(path);
        return new AudioTrackMetadata
        {
            Path = path,
            SampleRate = sampleRate,
            Bits = bits,
            Channels = channels,
            Date = Tag(tags, "date") ?? Tag(tags, "releasetime") ?? string.Empty,
            Track = Tag(tags, "track") ?? string.Empty,
            Title = Tag(tags, "title") ?? fallbackTitle,
            Album = Tag(tags, "album") ?? string.Empty,
            Duration = actualDuration,
            SourceSampleRate = sampleRate,
            SourceBits = bits,
        };
    }

    private static string Value(string line) => line[(line.IndexOf('=') + 1)..];

    private static string? Tag(IReadOnlyDictionary<string, string> tags, string key) =>
        tags.TryGetValue(key, out var value) ? value : null;

    private static double? CalculateDuration(long? timestamp, string? timeBase)
    {
        if (timestamp is null || string.IsNullOrWhiteSpace(timeBase))
        {
            return null;
        }
        var parts = timeBase.Split('/');
        if (parts.Length != 2 ||
            !long.TryParse(parts[0], out var numerator) ||
            !long.TryParse(parts[1], out var denominator) || denominator == 0)
        {
            return null;
        }
        return timestamp.Value * numerator / (double)denominator;
    }
}
