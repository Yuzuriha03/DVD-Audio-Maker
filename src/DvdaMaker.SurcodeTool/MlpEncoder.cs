using System.Security.Cryptography;
using DvdaMaker.Processes;

namespace DvdaMaker.SurcodeTool;

/// <summary>The pinned Windows x86 core preserves the verified x87 encoding behavior.</summary>
public static class MlpEncoder
{
    public const string BinarySha256 = "88d52e9d1726a54a44bc23d7062a906a8e9ce573726510455b9a6130fa7b3afd";
    public const string MetadataPolicy = "empty-auxiliary-tlv-v1";

    public static string ExtractExecutable()
    {
        if (!OperatingSystem.IsWindows()) throw new PlatformNotSupportedException("MLP 编码核心需要 Windows。");
        var folder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "DVD-Audio-Maker", "native", BinarySha256);
        Directory.CreateDirectory(folder);
        var path = Path.Combine(folder, "mlp_encode.exe");
        if (File.Exists(path)) { Verify(path); return path; }
        var temporary = Path.Combine(folder, Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            using (var input = typeof(MlpEncoder).Assembly.GetManifestResourceStream("DvdaMaker.MlpEncoder.Encoder")
                ?? throw new InvalidOperationException("发布包缺少已MLP 编码核心。"))
            using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                input.CopyTo(output);
            Verify(temporary);
            try { File.Move(temporary, path); }
            catch (IOException) when (File.Exists(path)) { Verify(path); }
            return path;
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    private static void Verify(string path)
    {
        using var stream = File.OpenRead(path);
        if (!Convert.ToHexString(SHA256.HashData(stream)).Equals(BinarySha256, StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("MLP 编码核心哈希不符，请清理损坏的原生核心缓存后重试。");
    }

    public static void WriteMetadata(string path, long frames, int sampleRate)
    {
        var baseRate = sampleRate % 44100 == 0 ? 44100 : 48000;
        var block = 40 * (sampleRate / baseRate);
        if (frames <= 0 || block is not (40 or 80 or 160)) throw new InvalidDataException("无效的 PCM 帧数或采样率。");
        using var writer = new BinaryWriter(new FileStream(path, FileMode.CreateNew, FileAccess.Write));
        writer.Write("MSCTX001"u8);
        writer.Write(checked((ulong)((frames + block - 1) / block)));
        writer.Write(1u);
        writer.Write(0UL);
        writer.Write(4u);
        writer.Write(new byte[] { 0, 0, 0x40, 0 });
    }

    public static async Task EncodeAsync(ProcessRunner runner, string wave, string destination,
        string metadataContext, TimeSpan timeout, CancellationToken cancellationToken)
    {
        // Pass ASCII relative names to the legacy CRT, keeping Unicode paths in managed code.
        var folder = Path.GetDirectoryName(Path.GetFullPath(wave))!;
        var layout = SurcodePcmWav.ReadLayout(wave);
        var output = Path.Combine(folder, "encoded.mlp");
        if (File.Exists(output) || File.Exists(destination))
            throw new IOException("拒绝覆盖已有的 MLP 输出文件。");
        var metadata = Path.Combine(folder, "metadata.stampctx");
        if (string.IsNullOrWhiteSpace(metadataContext))
            WriteMetadata(metadata, layout.DataSize / (layout.BytesPerSample * layout.Channels), layout.SampleRate);
        else File.Copy(metadataContext, metadata, overwrite: false);
        try
        {
            var result = await runner.RunAsync(new ProcessRequest
            {
                FileName = ExtractExecutable(),
                WorkingDirectory = folder,
                Arguments = [Path.GetFileName(wave), "encoded.mlp", "--original", "--stamp-context", "metadata.stampctx"],
                Timeout = timeout,
                OnErrorLine = line => Console.Error.WriteLine($"[MLP] {line}"),
            }, cancellationToken).ConfigureAwait(false);
            if (!result.Succeeded || !File.Exists(output) || new FileInfo(output).Length == 0)
                throw new InvalidOperationException($"MLP 核心编码失败（{result.ExitCode}）：{result.StandardError}");
            cancellationToken.ThrowIfCancellationRequested();
            File.Move(output, destination, overwrite: false);
        }
        finally
        {
            if (File.Exists(output)) File.Delete(output);
            File.Delete(metadata);
        }
    }
}
