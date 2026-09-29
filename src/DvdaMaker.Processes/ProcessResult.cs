namespace DvdaMaker.Processes;

public sealed record ProcessResult(
    string FileName,
    IReadOnlyList<string> Arguments,
    int ExitCode,
    string StandardOutput,
    string StandardError,
    TimeSpan Duration)
{
    public bool Succeeded => ExitCode == 0;
}
