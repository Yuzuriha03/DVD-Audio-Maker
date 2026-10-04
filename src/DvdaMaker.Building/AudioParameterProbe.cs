using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record AudioParameters(int SampleRate, int Channels, int Bits);

public sealed class AudioParameterProbe(ProcessRunner runner, string ffprobe)
{
    public async Task<AudioParameters> ProbeAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        // Tiny valid streams may be shorter than FFprobe's MLP detection window.
        string[] inputFormat = Path.GetExtension(path).Equals(".mlp", StringComparison.OrdinalIgnoreCase)
            ? ["-f", "mlp"] : [];
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = ffprobe,
            Arguments =
            [
                "-v", "error",
                "-select_streams", "a:0",
                "-show_entries", "stream=sample_rate,channels,bits_per_raw_sample",
                "-of", "default=nw=1:nk=1",
                .. inputFormat,
                path,
            ],
        }, cancellationToken).ConfigureAwait(false);

        if (!result.Succeeded)
        {
            throw new InvalidDataException(
                $"ffprobe 无法探测 {path}（退出码 {result.ExitCode}）: " +
                result.StandardError.Trim());
        }

        return RustBridge.Run<AudioParameters>("audio.parameters", result.StandardOutput,
            () =>
            {
                var values = result.StandardOutput.Split('\n', StringSplitOptions.TrimEntries)
                    .Where(line => line.Length > 0)
                    .Select(line => int.TryParse(line, out var value) ? value : 0)
                    .Concat([0, 0, 0])
                    .Take(3)
                    .ToArray();
                return new AudioParameters(values[0], values[1], values[2]);
            });
    }
}
