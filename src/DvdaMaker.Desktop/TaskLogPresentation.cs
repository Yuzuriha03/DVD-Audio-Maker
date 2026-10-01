using System.Collections.Concurrent;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace DvdaMaker.Desktop;

internal enum TaskLogLevel { Information, Success, Warning, Error }
internal sealed record TaskLogEntry(TaskLogLevel Level, string Message, bool ActivityOnly = false);
internal sealed record TaskLogLine(DateTime Time, string Text, TaskLogEntry? Summary = null, bool DetailOnly = false);

/// <summary>Only known progress chatter is shortened. Unrecognized diagnostics remain visible.</summary>
internal static class TaskLogPresentation
{
    public static TaskLogEntry? Present(string raw)
    {
        var text = raw.Trim();
        if (text.Length == 0) return null;
        if (text.Contains("VOLUME_ID_MISMATCH:", StringComparison.Ordinal))
            return new(TaskLogLevel.Error, "光盘名称与成品卷标不一致。请确认所选方案对应这份成品；名称含中文时，可改用英文名称重新制作。");
        var match = Regex.Match(text, @"^发现 (\d+) 个音频文件");
        if (match.Success) return Info($"找到 {match.Groups[1]} 首音频，正在检查曲目信息。");
        match = Regex.Match(text, @"^共需重采样 (\d+) 首");
        if (match.Success) return Info(match.Groups[1].Value == "0" ? "音源采样率符合设置，无需重采样。" : $"有 {match.Groups[1]} 首音频需要调整采样率。");
        match = Regex.Match(text, @"^\[缓存\] 复用已校验结果 (\d+) 首 / 重新校验 (\d+) 首");
        if (match.Success) return Info($"音源检查：{match.Groups[1]} 首使用已有结果，{match.Groups[2]} 首完成重新检查。");
        match = Regex.Match(text, @"^\[MLP\] 提交 (\d+) 个音源");
        if (match.Success) return Info($"开始无损编码，共 {match.Groups[1]} 首音频。");
        if (text.StartsWith("[MLP] 临时目录:") || text.StartsWith("[MLP] MLP 输出目录:")) return null;
        if (text.StartsWith("[MLP] ")) return Info("正在编码：" + text[6..]);
        match = Regex.Match(text, @"^\[MLP DLL\].* / (\d+) Hz / (\d+) bit / (\d+) 声道，([\d,]+) 字节");
        if (match.Success) return new(TaskLogLevel.Success,
            $"音轨编码完成 · {double.Parse(match.Groups[1].Value, CultureInfo.InvariantCulture) / 1000:0.###} kHz · {match.Groups[2]} 位 · {match.Groups[3]} 声道");
        if (text.StartsWith("[PCM] ")) return new(TaskLogLevel.Information, text[6..], true);
        if (text.StartsWith("[FFmpeg PCM] "))
        {
            var message = text[13..].Trim();
            if (Regex.IsMatch(message, @"(?i)\[(error|fatal|panic)\]|\b(error|failed|failure|fatal|cannot|invalid|not found)\b"))
                return new(TaskLogLevel.Error, "音源转换失败：" + message);
            return new(TaskLogLevel.Warning, "音源转换提醒：" + message);
        }
        if (text.StartsWith("[eac3to]"))
        {
            var message = text[8..].Trim();
            // stderr also carries normal progress; its stream alone does not imply failure.
            if (Regex.IsMatch(message, @"(?i)\b(error|failed|failure|fatal|cannot|can't|could not|not found|not supported)\b"))
                return new(TaskLogLevel.Error, "音源转换工具报告问题：" + message);
            if (Regex.IsMatch(message, @"(?i)\bwarning\b")) return new(TaskLogLevel.Warning, "音源转换工具提醒：" + message);
            match = Regex.Match(message, @"^(analyze|process):\s*(\d+)%");
            if (match.Success) return new(TaskLogLevel.Information,
                $"正在{(match.Groups[1].Value == "analyze" ? "分析" : "转换")}音源 · 当前转换 {match.Groups[2]}%", true);
            return null;
        }
        if (Regex.IsMatch(text, @"(?i)^\[(ERR|ERROR|FAIL|FATAL|错误)\]|^(error|fatal error):|^!!"))
        {
            if (text.Contains("Directory not recognized", StringComparison.OrdinalIgnoreCase))
                return new(TaskLogLevel.Warning, "制盘工具提示目录无法识别；最终结果以任务状态和成品验证为准。详情已保留。");
            if (text.StartsWith("[ERR]") && text.Contains("Directory", StringComparison.OrdinalIgnoreCase))
                return new(TaskLogLevel.Warning, "制盘工具报告目录问题。详细路径已保留，请结合最终任务状态与成品验证结果判断。");
            if (text.StartsWith("[ERR]")) return new(TaskLogLevel.Warning, "制盘工具报告问题：" + StripPrefix(text));
            return new(TaskLogLevel.Error, StripPrefix(text));
        }
        if (Regex.IsMatch(text, @"(?i)^\[(WAR|WARN|WARNING|警告)\]|^warning:|^\?"))
        {
            if (text.Contains("Coherence test for ISO start sector failed", StringComparison.OrdinalIgnoreCase))
                return new(TaskLogLevel.Warning, "制盘工具报告扇区位置差异。制作后请运行“验证成品”，详细信息已保留。");
            return new(TaskLogLevel.Warning, StripPrefix(text));
        }
        // Preserve unfamiliar failure diagnostics even when an external tool changes its prefix.
        if (Regex.IsMatch(text, @"(?i)\b(error|failed|fatal|cannot|could not|no such file|not found)\b|失败|错误"))
            return new(TaskLogLevel.Warning, "工具提示：" + text);
        if (text.StartsWith("总曲目 ") || text.StartsWith("第 ") || text.StartsWith("容量检查：")) return Info(text);
        if (text.StartsWith("=== 分盘结果")) return Info(text.Trim('=', ' '));
        if (text.StartsWith("成品：")) return new(TaskLogLevel.Success, "已生成光盘镜像：" + Path.GetFileName(text[3..]));
        if (text.StartsWith("[续跑]") || text.StartsWith("[恢复]")) return Info(StripPrefix(text));
        return null;
    }

    public static string ErrorSummary(Exception error)
    {
        const string prefix = "无法启动外部程序 ";
        if (error.Message.StartsWith(prefix, StringComparison.Ordinal))
        {
            var end = error.Message.IndexOf(": ", prefix.Length, StringComparison.Ordinal);
            var tool = end > prefix.Length ? Path.GetFileName(error.Message[prefix.Length..end]) : "所需工具";
            return $"无法启动 {tool}。请确认程序路径正确，并且该程序可以运行。";
        }
        if (error is UnauthorizedAccessException) return "无法访问所选文件或文件夹，当前账户可能没有所需权限。";
        if (error is DirectoryNotFoundException) return "找不到所选文件夹，请检查目录或外接磁盘是否可用。";
        if (error is TimeoutException) return "当前操作等待时间过长，任务已停止。";
        return error.Message;
    }
    public static string ErrorAdvice(Exception error)
    {
        if (error is UnauthorizedAccessException) return "请选择有写入权限的输出与工作目录，然后重试。";
        if (error is TimeoutException) return "处理超时。请检查音源和工具状态；可导出详细日志排查。";
        if (error.Message.Contains("无法启动外部程序") || error is FileNotFoundException)
            return "请在“音频编码”或“工具与高级”中检查程序及文件路径，然后重试。";
        if (error is DirectoryNotFoundException) return "请检查所选目录是否存在，以及移动硬盘或网络盘是否已连接。";
        return "查看下方问题提示，调整设置后重试；需要排查时可导出详细日志。";
    }

    private static TaskLogEntry Info(string message) => new(TaskLogLevel.Information, message);
    private static string StripPrefix(string text) => Regex.Replace(text, @"^\[[^\]]+\]\s*", "");
}

/// <summary>Coalesces TextWriter fragments and carriage-return progress into complete lines.</summary>
internal sealed class TaskLogWriter(ConcurrentQueue<TaskLogLine> messages) : TextWriter
{
    private readonly object _gate = new();
    private readonly StringBuilder _pending = new();
    public override Encoding Encoding => Encoding.UTF8;
    public override void Write(char value) { lock (_gate) Append(value); }
    public override void Write(string? value)
    {
        if (value is null) return;
        lock (_gate) foreach (var character in value) Append(character);
    }
    public override void WriteLine(string? value)
    {
        lock (_gate)
        {
            foreach (var character in value ?? "") Append(character);
            Append('\n');
        }
    }
    public override void Flush()
    {
        lock (_gate) if (_pending.Length > 0) Emit();
    }
    private void Append(char value)
    {
        if (value is '\r' or '\n') { if (_pending.Length > 0) Emit(); }
        else { _pending.Append(value); if (_pending.Length >= 16384) Emit(); }
    }
    private void Emit()
    {
        messages.Enqueue(new(DateTime.Now, _pending.ToString()));
        _pending.Clear();
    }
}
