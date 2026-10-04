using System.Security.Cryptography;

namespace DvdaMaker.Processes;

/// <summary>Streaming SHA-256 with explicit Rust/managed comparison.</summary>
public static class FileHash
{
    private sealed record Outcome(string? Hash, int? ErrorCode);

    public static string Sha256(string path)
    {
        var result = RustBridge.Run<Outcome>("hash.sha256_file", path, () => Managed(path));
        if (result.Hash is not null) return result.Hash;
        var code = result.ErrorCode ?? 87;
        var message = new System.ComponentModel.Win32Exception(code).Message + ": " + path;
        throw code switch
        {
            2 => new FileNotFoundException(message, path),
            3 => new DirectoryNotFoundException(message),
            5 => new UnauthorizedAccessException(message),
            _ => new IOException(message, unchecked((int)0x80070000) | code),
        };
    }

    private static Outcome Managed(string path)
    {
        try
        {
            using var stream = File.OpenRead(path);
            return new(Convert.ToHexString(SHA256.HashData(stream)), null);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return new(null, error.HResult & 0xffff);
        }
    }
}
