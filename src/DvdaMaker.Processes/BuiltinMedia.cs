using System.Diagnostics;
using System.Globalization;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace DvdaMaker.Processes;

/// <summary>Compatibility adapter for this application's media requests, executed in-process.</summary>
public static class BuiltinMedia
{
    public const string Converter = "builtin:media";
    public const string Probe = "builtin:probe";
    private static readonly Lazy<RunDelegate> Entry = new(Load);
    private static readonly Lazy<string> Fingerprint = new(() =>
    {
        var directory = Path.GetDirectoryName(LibraryPath)!;
        var hashes = Directory.EnumerateFiles(directory, "*.dll").Order(StringComparer.OrdinalIgnoreCase)
            .Select(path => Path.GetFileName(path) + ":" + Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))));
        return "native-media-v1:" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(string.Join("|", hashes))));
    });
    public static bool IsBuiltin(string value) => value is Converter or Probe;
    public static string Identity => Fingerprint.Value;
    public static bool IsAvailable => File.Exists(LibraryPath);
    public static string LibraryPath => Path.Combine(
        Environment.GetEnvironmentVariable("DVDA_MEDIA_NATIVE_DIR") is { Length: > 0 } configured
            ? Path.GetFullPath(configured) : Path.Combine(BundledRuntime.Root,
                File.Exists(Path.Combine(BundledRuntime.Root, "menu-bin", "dvda-media.dll"))
                    ? "menu-bin" : "media-native"), "dvda-media.dll");

    private static RunDelegate Load()
    {
        if (!OperatingSystem.IsWindows() || !Environment.Is64BitProcess)
            throw new PlatformNotSupportedException("内置媒体处理器需要 Windows x64。");
        if (!IsAvailable) throw new FileNotFoundException("内置媒体组件缺失，请完整解压发布包。开发环境请运行 build-media-bridge.py 或配置 DVDA_MEDIA_NATIVE_DIR。", LibraryPath);
        // Resolve dependencies only beside this library and in Windows, never on PATH.
        var handle = LoadLibraryExW(LibraryPath, IntPtr.Zero, 0x100 | 0x800);
        if (handle == IntPtr.Zero) throw new InvalidOperationException($"无法加载内置媒体组件（Windows 错误 {Marshal.GetLastWin32Error()}），请检查完整的 x64 运行目录。");
        return Marshal.GetDelegateForFunctionPointer<RunDelegate>(NativeLibrary.GetExport(handle, "dvdamedia_run"));
    }

    public static async Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token)
    {
        using var timeout = request.Timeout is { } duration ? new CancellationTokenSource(duration) : null;
        using var linked = timeout is null ? CancellationTokenSource.CreateLinkedTokenSource(token)
            : CancellationTokenSource.CreateLinkedTokenSource(token, timeout.Token);
        try { return await Task.Run(() => Execute(request, linked.Token), CancellationToken.None).ConfigureAwait(false); }
        catch (OperationCanceledException) when (timeout?.IsCancellationRequested == true && !token.IsCancellationRequested)
        { throw new TimeoutException("内置媒体处理超时。"); }
    }

    private static ProcessResult Execute(ProcessRequest request, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        var started = Stopwatch.GetTimestamp();
        var args = request.Arguments;
        string? Value(string name) { for (var i = 0; i + 1 < args.Count; i++) if (args[i] == name) return args[i + 1]; return null; }
        string FullPath(string path) => Path.GetFullPath(path, request.WorkingDirectory ?? Environment.CurrentDirectory);
        var native = new NativeRequest { Size = (uint)Marshal.SizeOf<NativeRequest>(), Abi = 1, Operation = 1, Compression = 8 };
        var probing = request.FileName == Probe;
        var input = probing ? args.Last() : Value("-i") ?? throw new ArgumentException("内置媒体请求缺少输入文件。");
        string? output = null;
        if (probing)
        {
            if (args.Any(x => x.StartsWith("packet=", StringComparison.Ordinal))) native.Operation = 2;
        }
        else
        {
            native.Operation = Value("-frames:v") == "1" ? 4u : Value("-f") == "image2" && Value("-c") == "copy" ? 5u : 3u;
            var format = Value("-f");
            var codec = Value("-c:a");
            native.OutputFormat = codec == "flac" ? 6u : format switch
            {
                "null" => 0u, "wav" => 1u, "s24le" => 2u, "s16le" => 3u, "s32le" => 4u, "md5" => 5u,
                _ when native.Operation is 4 or 5 => 0u,
                _ => throw new NotSupportedException("未支持的内置媒体输出格式：" + format),
            };
            native.Bits = native.OutputFormat switch { 2 or 4 => 24u, 3 or 5 => 16u, 1 => codec == "pcm_s16le" ? 16u : 24u, _ => 0u };
            var filter = Value("-af") ?? "";
            if (filter.Length > 0 && filter != "astats=metadata=1")
            {
                var match = Regex.Match(filter, @"^aresample=(\d+):resampler=(swr|soxr)(:osf=(s16|s32|dblp):dither_method=none)?");
                if (!match.Success) throw new NotSupportedException("未支持的内置音频过滤配置：" + filter);
                native.Rate = uint.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture);
                native.Soxr = match.Groups[2].Value == "soxr" ? 1u : 0u;
                var tail = filter[match.Length..];
                const string quantizer = ",aeval=exprs='clip(floor(val(ch)*524288+0.5),-524288,524287)/524288':c=same,aformat=sample_fmts=s32";
                if (tail == quantizer) native.Bits = 20;
                else if (tail.Length > 0 && tail != ",astats=metadata=1") throw new NotSupportedException("未支持的内置音频过滤配置：" + filter);
            }
            if (Value("-compression_level") is { } level) native.Compression = uint.Parse(level, CultureInfo.InvariantCulture);
            native.Cover = args.Contains("0:v:0") ? 1u : 0u;
            if (native.Operation is 4 or 5 || native.OutputFormat is 1 or 2 or 3 or 4 or 6) output = FullPath(args.Last());
        }
        var stdout = new StringBuilder(); var stderr = new StringBuilder();
        var allocations = new List<IntPtr>(); string? temporary = null;
        IntPtr Utf8(string text) { var pointer = Marshal.StringToCoTaskMemUTF8(text); allocations.Add(pointer); return pointer; }
        ExceptionDispatchInfo? callbackFailure = null;
        void Write(int stream, string text)
        {
            text = text.TrimEnd('\r', '\n');
            if (stream == 2 && text.StartsWith("Number of samples:", StringComparison.Ordinal) &&
                !(Value("-af") ?? "").Contains("astats", StringComparison.Ordinal)) return;
            if (stream == 2) { if (request.CaptureError) stderr.AppendLine(text); request.OnErrorLine?.Invoke(text); }
            else if (stream != 3 || args.Contains("-progress"))
            { if (request.CaptureOutput || probing) stdout.AppendLine(text); request.OnOutputLine?.Invoke(text); }
        }
        try
        {
            if (RustBridge.Mode != "managed")
            {
                var metadata = new List<string[]>();
                for (var i = 0; i + 1 < args.Count; ++i) if (args[i] == "-metadata")
                {
                    var pair = args[++i].Split('=', 2);
                    if (pair.Length != 2) throw new ArgumentException("无效音频标签。");
                    metadata.Add(pair);
                }
                // Temporary argument adapter. The final Rust workflow constructs typed jobs.
                // Mutations execute once; isolated migration tests compare both implementations.
                var statusCode = RustBridge.RunMedia(new
                {
                    Library = LibraryPath,
                    Request = new
                    {
                        Operation = new[] { "", "Probe", "Packets", "Audio", "VideoFrame", "Cover" }[native.Operation],
                        native.Rate, native.Bits,
                        OutputFormat = new[] { "None", "Wave", "S24", "S16", "S32", "Md5", "Flac" }[native.OutputFormat],
                        Soxr = native.Soxr != 0, native.Compression, Cover = native.Cover != 0,
                        Input = FullPath(input), Output = output, Tags = metadata,
                    },
                    Replace = args.Contains("-y"), TimeoutMillis = (long?)null,
                }, Write, token);
                var rustOutput = stdout.ToString();
                if (statusCode == 0 && probing && native.Operation == 1) rustOutput = FormatProbe(rustOutput, args);
                var rustResult = new ProcessResult(request.FileName, args, statusCode,
                    request.CaptureOutput ? rustOutput : "", stderr.ToString(), Stopwatch.GetElapsedTime(started));
                if (request.ThrowOnNonZeroExitCode && !rustResult.Succeeded) throw new ProcessExecutionException(rustResult);
                return rustResult;
            }
            native.Input = Utf8(FullPath(input));
            if (output is not null)
            {
                if (string.Equals(output, FullPath(input), StringComparison.OrdinalIgnoreCase)) throw new IOException("音频输入输出不能是同一文件。");
                if (File.Exists(output) && !args.Contains("-y")) throw new IOException("输出文件已存在：" + output);
                temporary = output + "." + Guid.NewGuid().ToString("N") + ".partial";
                native.Output = Utf8(temporary);
            }
            var tags = new List<IntPtr>();
            for (var i = 0; i + 1 < args.Count; ++i) if (args[i] == "-metadata")
            {
                var pair = args[++i].Split('=', 2); if (pair.Length != 2) throw new ArgumentException("无效音频标签。");
                tags.Add(Utf8(pair[0])); tags.Add(Utf8(pair[1]));
            }
            if (tags.Count > 0)
            {
                native.Tags = Marshal.AllocCoTaskMem(tags.Count * IntPtr.Size); allocations.Add(native.Tags);
                Marshal.Copy(tags.ToArray(), 0, native.Tags, tags.Count); native.TagCount = (uint)tags.Count / 2;
            }
            Emit emit = (_, stream, text) =>
            {
                try { Write(stream, Marshal.PtrToStringUTF8(text) ?? ""); }
                catch (Exception e) { callbackFailure ??= ExceptionDispatchInfo.Capture(e); }
            };
            Cancel cancel = _ => token.IsCancellationRequested || callbackFailure is not null ? 1 : 0;
            var status = Entry.Value(ref native, emit, cancel, IntPtr.Zero);
            GC.KeepAlive(emit); GC.KeepAlive(cancel);
            callbackFailure?.Throw(); token.ThrowIfCancellationRequested();
            if (status >= 0 && temporary is not null) { File.Move(temporary, output!, args.Contains("-y")); temporary = null; }
            var textOutput = stdout.ToString();
            if (status >= 0 && probing && native.Operation == 1) textOutput = FormatProbe(textOutput, args);
            var result = new ProcessResult(request.FileName, args, status < 0 ? 1 : 0, request.CaptureOutput ? textOutput : "", stderr.ToString(), Stopwatch.GetElapsedTime(started));
            if (request.ThrowOnNonZeroExitCode && !result.Succeeded) throw new ProcessExecutionException(result);
            return result;
        }
        finally
        {
            foreach (var pointer in allocations) Marshal.FreeCoTaskMem(pointer);
            if (temporary is not null && File.Exists(temporary)) File.Delete(temporary);
        }
    }

    private static string FormatProbe(string text, IReadOnlyList<string> args)
    {
        string? Value(string name) { for (var i = 0; i + 1 < args.Count; i++) if (args[i] == name) return args[i + 1]; return null; }
        if (Value("-of") == "json") return text;
        using var document = JsonDocument.Parse(text);
        var streams = document.RootElement.GetProperty("streams").EnumerateArray().ToArray().AsEnumerable();
        var selector = Value("-select_streams") ?? "";
        if (selector.StartsWith('a')) streams = streams.Where(x => x.GetProperty("codec_type").GetString() == "audio");
        if (selector.StartsWith('v')) streams = streams.Where(x => x.GetProperty("codec_type").GetString() == "video");
        if (selector.EndsWith(":0", StringComparison.Ordinal)) streams = streams.Take(1);
        var entries = new List<string>();
        for (var i = 0; i + 1 < args.Count; i++) if (args[i] == "-show_entries") entries.Add(args[i + 1]);
        var requested = entries.Where(x => x.StartsWith("stream=", StringComparison.Ordinal)).SelectMany(x => x[7..].Split(',')).ToHashSet(StringComparer.Ordinal);
        var format = Value("-of") ?? "";
        var bare = format.Contains("nk=1", StringComparison.Ordinal) || format.StartsWith("csv", StringComparison.Ordinal);
        var result = new StringBuilder();
        string Display(JsonElement value) => value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined ? "N/A" : value.ToString();
        foreach (var stream in streams) foreach (var name in new[] { "codec_name", "codec_type", "sample_rate", "channels", "bits_per_raw_sample", "time_base", "duration_ts", "width", "height" })
            if (requested.Contains(name) && stream.TryGetProperty(name, out var value)) result.AppendLine((bare ? "" : name + "=") + Display(value));
        var info = document.RootElement.GetProperty("format");
        if (entries.Any(x => x == "format=duration")) result.AppendLine((bare ? "" : "duration=") + Display(info.GetProperty("duration")));
        if (entries.Contains("format_tags")) foreach (var tag in info.GetProperty("tags").EnumerateObject()) result.AppendLine("TAG:" + tag.Name + "=" + tag.Value.GetString());
        return result.ToString();
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeRequest
    {
        public uint Size, Abi, Operation, Rate, Bits, OutputFormat, Soxr, Compression, Cover, TagCount;
        public IntPtr Input, Output, Tags;
    }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void Emit(IntPtr state, int stream, IntPtr text);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int Cancel(IntPtr state);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int RunDelegate(ref NativeRequest request, Emit emit, Cancel cancel, IntPtr state);
    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)] private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
}
