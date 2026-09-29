using DvdaMaker.Processes;

namespace DvdaMaker.SurcodeTool;

public sealed class SurcodeBatchEncoder
{
    private readonly ProcessRunner _runner;

    public SurcodeBatchEncoder(ProcessRunner runner)
    {
        _runner = runner;
    }

    public async Task RunAsync(SurcodeEncodingJob job, CancellationToken cancellationToken)
    {
        Validate(job);
        Directory.CreateDirectory(job.TemporaryDirectory);
        Directory.CreateDirectory(job.OutputDirectory);

        var automation = new SurcodeAutomation(job.SurcodeExecutable);
        for (var index = 0; index < job.Tracks.Count; index++)
        {
            var track = job.Tracks[index];
            cancellationToken.ThrowIfCancellationRequested();
            Console.WriteLine($"[SurCode {index + 1}/{job.Tracks.Count}] {track.DisplayName}");

            await PrepareWaveFilesAsync(job, track, cancellationToken).ConfigureAwait(false);
            var ssf = SurcodeSsfWriter.Write(
                track.WorkName,
                job.TemporaryDirectory,
                job.TemporaryDirectory,
                job.OutputDirectory);
            var output = Path.Combine(job.OutputDirectory, track.WorkName + ".mlp");
            var timeout = TimeSpan.FromSeconds(Math.Max(300, track.DurationSeconds * 3 + 120));
            await automation.EncodeAsync(ssf, output, timeout, cancellationToken)
                .ConfigureAwait(false);
            CleanupTrack(job.TemporaryDirectory, track.WorkName);
        }
    }

    private async Task PrepareWaveFilesAsync(
        SurcodeEncodingJob job,
        SurcodeEncodingTrack track,
        CancellationToken cancellationToken)
    {
        CleanupTrack(job.TemporaryDirectory, track.WorkName);
        var target = Path.Combine(job.TemporaryDirectory, track.WorkName + ".wavs");
        var arguments = new List<string>
        {
            track.SourcePath,
            target,
            $"-resampleTo{job.SampleRate}",
        };

        var effectiveBits = track.SourceSampleRate != job.SampleRate
            ? 24
            : track.SourceBits;
        if (effectiveBits > 0 && job.Bits < effectiveBits)
        {
            arguments.Add($"-down{job.Bits}");
        }

        var timeout = TimeSpan.FromSeconds(Math.Max(300, track.DurationSeconds * 2 + 120));
        var result = await _runner.RunAsync(new ProcessRequest
        {
            FileName = job.Eac3toExecutable,
            WorkingDirectory = Path.GetDirectoryName(job.Eac3toExecutable),
            Arguments = arguments,
            Timeout = timeout,
            OnOutputLine = line => Console.WriteLine($"[eac3to] {line}"),
            OnErrorLine = line => Console.Error.WriteLine($"[eac3to] {line}"),
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded)
        {
            throw new InvalidOperationException(
                $"eac3to 退出码 {result.ExitCode}: {track.DisplayName}");
        }

        if (effectiveBits > 0 && job.Bits > effectiveBits)
        {
            SurcodePcmWav.UpconvertProducedFiles(
                job.TemporaryDirectory,
                track.WorkName,
                job.Bits);
        }
    }

    private static void Validate(SurcodeEncodingJob job)
    {
        if (!File.Exists(job.SurcodeExecutable))
        {
            throw new FileNotFoundException("找不到 surcodemlp.exe。", job.SurcodeExecutable);
        }
        if (!File.Exists(job.Eac3toExecutable))
        {
            throw new FileNotFoundException("找不到 eac3to.exe。", job.Eac3toExecutable);
        }
        if (job.SampleRate is not (44_100 or 48_000 or 88_200 or 96_000 or 176_400 or 192_000))
        {
            throw new InvalidDataException($"不支持的目标采样率: {job.SampleRate}");
        }
        if (job.Bits is not (16 or 20 or 24))
        {
            throw new InvalidDataException($"不支持的目标位深: {job.Bits}");
        }
        if (job.Tracks.Count == 0)
        {
            throw new InvalidDataException("编码任务不包含任何音轨。" );
        }
        foreach (var track in job.Tracks)
        {
            if (!File.Exists(track.SourcePath))
            {
                throw new FileNotFoundException("找不到待编码音源。", track.SourcePath);
            }
            if (track.WorkName.Length == 0 || track.WorkName.Any(character =>
                character > 0x7F || Path.GetInvalidFileNameChars().Contains(character)))
            {
                throw new InvalidDataException($"工作文件名必须是安全的 ASCII 名称: {track.WorkName}");
            }
        }
    }

    private static void CleanupTrack(string directory, string baseName)
    {
        foreach (var path in Directory.EnumerateFiles(directory, baseName + ".*"))
        {
            try
            {
                File.Delete(path);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }
}
