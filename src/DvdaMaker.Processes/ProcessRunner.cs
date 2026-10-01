using System.Diagnostics;
using System.Text;

namespace DvdaMaker.Processes;

public sealed class ProcessRunner
{
    public async Task<ProcessResult> RunAsync(
        ProcessRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(request.FileName);

        var startInfo = new ProcessStartInfo
        {
            FileName = request.FileName,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = request.OutputEncoding,
            StandardErrorEncoding = request.ErrorEncoding,
        };

        if (!string.IsNullOrWhiteSpace(request.WorkingDirectory))
        {
            startInfo.WorkingDirectory = request.WorkingDirectory;
        }
        foreach (var argument in request.Arguments)
        {
            startInfo.ArgumentList.Add(argument);
        }
        foreach (var pair in request.Environment)
        {
            if (pair.Value is null)
            {
                startInfo.Environment.Remove(pair.Key);
            }
            else
            {
                startInfo.Environment[pair.Key] = pair.Value;
            }
        }

        using var process = new Process { StartInfo = startInfo };
        var output = new StringBuilder();
        var error = new StringBuilder();
        var outputClosed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var errorClosed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);

        process.OutputDataReceived += (_, eventArgs) =>
        {
            if (eventArgs.Data is null)
            {
                outputClosed.TrySetResult();
                return;
            }
            if (request.CaptureOutput)
            {
                output.AppendLine(eventArgs.Data);
            }
            request.OnOutputLine?.Invoke(eventArgs.Data);
        };
        process.ErrorDataReceived += (_, eventArgs) =>
        {
            if (eventArgs.Data is null)
            {
                errorClosed.TrySetResult();
                return;
            }
            if (request.CaptureError)
            {
                error.AppendLine(eventArgs.Data);
            }
            request.OnErrorLine?.Invoke(eventArgs.Data);
        };

        var startedAt = Stopwatch.GetTimestamp();
        try
        {
            if (!process.Start())
            {
                throw new InvalidOperationException($"无法启动外部程序: {request.FileName}");
            }
        }
        catch (Exception exception) when (
            exception is System.ComponentModel.Win32Exception or InvalidOperationException)
        {
            throw new InvalidOperationException(
                $"无法启动外部程序 {request.FileName}: {exception.Message}", exception);
        }

        process.BeginOutputReadLine();
        process.BeginErrorReadLine();

        using var timeoutSource = request.Timeout is { } timeout
            ? new CancellationTokenSource(timeout)
            : null;
        using var linkedSource = timeoutSource is null
            ? CancellationTokenSource.CreateLinkedTokenSource(cancellationToken)
            : CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, timeoutSource.Token);

        try
        {
            await process.WaitForExitAsync(linkedSource.Token).ConfigureAwait(false);
            await Task.WhenAll(outputClosed.Task, errorClosed.Task).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            TryKill(process);
            try { await process.WaitForExitAsync(CancellationToken.None).WaitAsync(TimeSpan.FromSeconds(5)).ConfigureAwait(false); }
            catch (TimeoutException) { /* Preserve the cancellation result if termination is delayed. */ }
            if (timeoutSource?.IsCancellationRequested == true &&
                !cancellationToken.IsCancellationRequested)
            {
                throw new TimeoutException(
                    $"外部程序运行超时: {CommandLineFormatter.Format(request.FileName, request.Arguments)}");
            }
            throw;
        }

        var result = new ProcessResult(
            request.FileName,
            request.Arguments,
            process.ExitCode,
            output.ToString(),
            error.ToString(),
            Stopwatch.GetElapsedTime(startedAt));

        if (request.ThrowOnNonZeroExitCode && !result.Succeeded)
        {
            throw new ProcessExecutionException(result);
        }
        return result;
    }

    private static void TryKill(Process process)
    {
        try
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
        }
        catch (InvalidOperationException)
        {
        }
    }
}
