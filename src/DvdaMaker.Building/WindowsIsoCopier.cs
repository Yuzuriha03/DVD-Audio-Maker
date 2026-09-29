using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed record WindowsIsoCopyResult(bool Succeeded, int? ExitCode, string Message);

public sealed class WindowsIsoCopier(ProcessRunner runner)
{
    private const int RobocopySuccessLimit = 8;
    private static readonly TimeSpan CopyTimeout = TimeSpan.FromMinutes(30);

    public async Task<WindowsIsoCopyResult> CopyAsync(
        string isoPath,
        string windowsDestination,
        string? distroName = null,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(windowsDestination))
        {
            return new WindowsIsoCopyResult(true, null, "未配置 Windows 目标目录，跳过复制。");
        }

        distroName ??= Environment.GetEnvironmentVariable("WSL_DISTRO_NAME");
        if (string.IsNullOrWhiteSpace(distroName))
        {
            return new WindowsIsoCopyResult(
                false, null, "不在 WSL 里（WSL_DISTRO_NAME 未设置）；DVDA_WINDOWS_DEST 只在 WSL 下有意义。");
        }

        if (!File.Exists(isoPath))
        {
            return new WindowsIsoCopyResult(false, null, $"ISO 文件不存在: {isoPath}");
        }

        var robocopy = Environment.GetEnvironmentVariable("DVDA_ROBOCOPY") ??
            "/mnt/c/Windows/System32/Robocopy.exe";
        if (!File.Exists(robocopy))
        {
            return new WindowsIsoCopyResult(false, null, $"找不到 {robocopy}（/mnt/c 未挂载？）");
        }

        var sourceDirectory = "\\\\wsl.localhost\\" + distroName +
            Path.GetDirectoryName(Path.GetFullPath(isoPath))!.Replace('/', '\\');
        var destination = windowsDestination.Replace('/', '\\').TrimEnd('\\');
        var fileName = Path.GetFileName(isoPath);
        var arguments = new[]
        {
            sourceDirectory,
            destination,
            fileName,
            "/J", "/MT:8", "/NP", "/R:2", "/W:2", "/NFL", "/NDL",
        };

        ProcessResult result;
        try
        {
            result = await runner.RunAsync(new ProcessRequest
            {
                FileName = robocopy,
                Arguments = arguments,
                Timeout = CopyTimeout,
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (TimeoutException)
        {
            return new WindowsIsoCopyResult(false, null, "robocopy 超过 30 分钟未完成。");
        }
        catch (InvalidOperationException exception)
        {
            return new WindowsIsoCopyResult(false, null, exception.Message);
        }

        if (result.ExitCode < RobocopySuccessLimit)
        {
            return new WindowsIsoCopyResult(
                true,
                result.ExitCode,
                $"robocopy 退出码 {result.ExitCode}（0~7 均为成功）: {destination}\\{fileName}");
        }

        var output = string.Join(
            Environment.NewLine,
            (result.StandardOutput + Environment.NewLine + result.StandardError)
                .Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
                .TakeLast(10));
        return new WindowsIsoCopyResult(
            false,
            result.ExitCode,
            $"robocopy 退出码 {result.ExitCode}（>=8 表示有文件未拷成功）" +
            (output.Length == 0 ? string.Empty : Environment.NewLine + output));
    }
}
