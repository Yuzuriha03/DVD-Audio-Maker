using DvdaMaker.Configuration;
using DvdaMaker.Preparation;
using DvdaMaker.Processes;
using DvdaMaker.SurcodeTool;

namespace DvdaMaker.Building;

/// <summary>Prepare integer WAVE for native DVD-Audio LPCM authoring; never loads the MLP encoder.</summary>
public sealed class LpcmProvider(DvdaOptions options, ProcessRunner runner)
{
    public static void ValidateFormat(int rate, int bits, int channels)
    {
        if (rate is not (44100 or 48000 or 88200 or 96000 or 176400 or 192000) ||
            bits is not (16 or 24) || channels is < 1 or > 6 || (rate > 96000 && channels > 2))
            throw new InvalidDataException("LPCM 支持 16/24 位、最多六声道；176.4/192 kHz 最多双声道。");
        if ((long)rate * bits * channels > 9_600_000)
            throw new InvalidDataException("LPCM 音频码率超过 9.6 Mb/s，请降低采样率或位深，或选择 MLP 编码。");
    }

    public static string CachePath(string buildDirectory, string sourcePath) => Path.Combine(buildDirectory, "lpcm",
        Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(Path.GetFullPath(sourcePath).ToUpperInvariant()))) + ".wav");

    public static void ValidateLayout(SurcodePcmWav.WavLayout layout)
    {
        ValidateFormat(layout.SampleRate, layout.ValidBits, layout.Channels);
        if (layout.ChannelMask is not (4 or 3 or 0x103 or 0x33 or 0xb or 0x10b or 0x3b or 7 or 0x107 or 0x37 or 0xf or 0x10f or 0x3f))
            throw new InvalidDataException("LPCM 不支持此声道布局，请使用标准 DVD-Audio 声道布局。");
    }

    public async Task<MlpAcquisitionResult> AcquireAsync(IReadOnlyList<BuildTrack> tracks,
        CancellationToken cancellationToken = default)
    {
        var output = new List<BuildTrack>();
        var diagnostics = new List<BuildDiagnostic>();
        var root = Path.Combine(options.BuildDirectory, "lpcm");
        Directory.CreateDirectory(root);
        var cachePath = Path.Combine(root, "lpcm-cache.json");
        var cache = MlpCacheIndex.Load(cachePath);
        var probe = new AudioParameterProbe(runner, options.Ffprobe);
        var identity = "lpcm-wave-v1|" + FfmpegPcmConverter.Policy + "|" +
            (BuiltinMedia.IsBuiltin(options.Ffmpeg) ? BuiltinMedia.Identity :
                await ToolIdentity.DescribeAsync(runner, options.Ffmpeg, cancellationToken).ConfigureAwait(false));
        var hits = 0; var rebuilt = 0;
        foreach (var track in tracks)
        {
            cancellationToken.ThrowIfCancellationRequested();
            string? temporary = null;
            try
            {
                var source = await probe.ProbeAsync(track.SourcePath, cancellationToken).ConfigureAwait(false);
                ValidateFormat(options.MlpSurcodeSampleRate, options.MlpSurcodeBits, source.Channels);
                var sourceIdentity = FileIdentityProbe.Compute(track.SourcePath)
                    ?? throw new IOException($"无法读取源文件身份: {track.SourcePath}");
                // Hash the full source path to avoid basename collisions and escaping the cache directory.
                var destination = CachePath(options.BuildDirectory, track.SourcePath);
                var key = Path.GetFileNameWithoutExtension(destination);
                if (cache.Match(destination, sourceIdentity, identity, options.MlpSurcodeBits,
                    options.MlpSurcodeSampleRate, 0) is not null) hits++;
                else
                {
                    temporary = Path.Combine(root, "." + Guid.NewGuid().ToString("N"));
                    Directory.CreateDirectory(temporary);
                    var converted = Path.Combine(temporary, "converted.wav");
                    var normalized = Path.Combine(temporary, "input.wav");
                    await FfmpegPcmConverter.ConvertAsync(runner, options.Ffmpeg, new SurcodeEncodingTrack
                    {
                        SourcePath = track.SourcePath, WorkName = key, DisplayName = track.Title,
                        DurationSeconds = track.Duration, SourceSampleRate = source.SampleRate, SourceBits = source.Bits,
                    }, converted, options.MlpSurcodeSampleRate, options.MlpSurcodeBits, cancellationToken).ConfigureAwait(false);
                    SurcodePcmWav.Normalize(converted, normalized, options.MlpSurcodeSampleRate,
                        options.MlpSurcodeBits, cancellationToken);
                    var layout = SurcodePcmWav.ReadLayout(normalized);
                    ValidateLayout(layout);
                    if (layout.Channels != source.Channels) throw new InvalidDataException("LPCM 转换改变了声道数。");
                    if (!FileIdentityProbe.Matches(track.SourcePath, sourceIdentity))
                        throw new IOException($"编码过程中源文件发生变化: {track.SourcePath}");
                    File.Move(normalized, destination, overwrite: true);
                    cache.Record(destination, new MlpCacheEntry { Source = sourceIdentity,
                        Output = FileIdentityProbe.Compute(destination), Encoder = identity,
                        Bits = options.MlpSurcodeBits, ResampleTo = options.MlpSurcodeSampleRate, MaxInterval = 0 });
                    rebuilt++;
                }
                var wave = SurcodePcmWav.ReadLayout(destination);
                ValidateLayout(wave);
                output.Add(track with { MlpPath = destination, MlpSize = new FileInfo(destination).Length,
                    ChannelMask = wave.ChannelMask, MlpSource = "lpcm", SampleRate = wave.SampleRate, Bits = wave.ValidBits, Channels = wave.Channels,
                    SourceSampleRate = source.SampleRate, SourceBits = source.Bits,
                    ResampleTo = source.SampleRate == wave.SampleRate ? null : wave.SampleRate,
                    ParametersChanged = source.SampleRate != wave.SampleRate || source.Bits != wave.ValidBits });
                Console.WriteLine($"[LPCM] 音源准备完成：{track.Title}");
            }
            catch (Exception error) when (error is IOException or InvalidDataException or InvalidOperationException or ArgumentException)
            {
                diagnostics.Add(new BuildDiagnostic(BuildDiagnosticSeverity.Error, "LPCM_PREPARATION_FAILED",
                    $"LPCM 音源准备失败：{track.Title}（{error.Message}）"));
                output.Add(track);
            }
            finally { if (temporary is not null && Directory.Exists(temporary)) Directory.Delete(temporary, recursive: true); }
        }
        cache.Save(cachePath);
        return new MlpAcquisitionResult(output, hits, rebuilt, diagnostics);
    }
}
