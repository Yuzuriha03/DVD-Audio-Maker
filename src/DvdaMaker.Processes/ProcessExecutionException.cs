namespace DvdaMaker.Processes;

public sealed class ProcessExecutionException : Exception
{
    public ProcessExecutionException(ProcessResult result)
        : base(BuildMessage(result))
    {
        Result = result;
    }

    public ProcessResult Result { get; }

    private static string BuildMessage(ProcessResult result)
    {
        var command = CommandLineFormatter.Format(result.FileName, result.Arguments);
        var detail = string.IsNullOrWhiteSpace(result.StandardError)
            ? string.Empty
            : Environment.NewLine + result.StandardError.TrimEnd();
        return $"外部程序退出码 {result.ExitCode}: {command}{detail}";
    }
}
