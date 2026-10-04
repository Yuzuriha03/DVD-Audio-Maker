using System.Text.Json;
using DvdaMaker.Processes;

namespace DvdaMaker.Configuration;

internal static class ConfigurationFileInterop
{
    internal sealed record Failure(string Kind, int? Code, string? Message);
    internal sealed record Outcome<T>(T? Value, Failure? Failure);

    internal static Outcome<T> Capture<T>(Func<T> action)
    {
        try { return new(action(), null); }
        catch (JsonException) { return new(default, new("Json", null, null)); }
        catch (InvalidDataException error) { return new(default, new("InvalidData", null, error.Message)); }
        catch (ArgumentException error) { return new(default, new("Argument", null, error.Message)); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return new(default, new("Io", error.HResult & 0xffff, null));
        }
    }

    internal static T Read<T>(string operation, object? request, string? path, Func<T> managed) =>
        Unwrap(RustBridge.Run<Outcome<T>>(operation, request, () => Capture(managed)), path);

    internal static T Unwrap<T>(Outcome<T> outcome, string? path)
    {
        if (outcome.Failure is not { } failure) return outcome.Value!;
        if (failure.Kind == "Json") throw new JsonException("配置方案 JSON 无效。");
        if (failure.Kind == "InvalidData") throw new InvalidDataException(failure.Message);
        if (failure.Kind == "Argument") throw new ArgumentException(failure.Message);
        var code = failure.Code ?? 87;
        var message = new System.ComponentModel.Win32Exception(code).Message + ": " + path;
        throw code switch
        {
            2 => new FileNotFoundException(message, path),
            3 => new DirectoryNotFoundException(message),
            5 => new UnauthorizedAccessException(message),
            _ => new IOException(message, unchecked((int)0x80070000) | code),
        };
    }
}
