using System.Collections.Concurrent;
using DvdaMaker.Desktop;

internal static class GuiPresentationTests
{
    private static void Require(bool value, string message) { if (!value) throw new Exception(message); }
    public static void ImportantDiagnosticsSurvive()
    {
        foreach (var text in new[] { "[ERROR] Cannot write file", "[FAIL] 音频检查失败", "[WARN] 缓存不可用", "[eac3to] Error: unsupported format", "unexpected tool: failed to open track", "[ERR] Directory not recognized.", "[WAR] Coherence test for ISO start sector failed: 278 != 280" })
        {
            var message = TaskLogPresentation.Present(text);
            Require(message is not null && !message.ActivityOnly && message.Level is TaskLogLevel.Warning or TaskLogLevel.Error, "A diagnostic disappeared from summary: " + text);
        }
        Require(TaskLogPresentation.Present("[错误] VOLUME_ID_MISMATCH: unexpected label") is { Level: TaskLogLevel.Error }, "Verification issue omitted from summary");
        Require(TaskLogPresentation.ErrorSummary(new InvalidOperationException("无法启动外部程序 C:/Tools/author.exe: file missing")).Contains("author.exe"), "Missing-tool hint omitted tool name");
        Require(TaskLogPresentation.Present("[eac3to] process: 73%") is { ActivityOnly: true, Level: TaskLogLevel.Information }, "Normal conversion progress classified as a problem");
        Require(TaskLogPresentation.Present("[eac3to] Done.") is null, "Routine tool chatter flooded the summary");
        Require(TaskLogPresentation.Present("[MLP] 临时目录: C:\\temp\\guid") is null, "Internal temporary path leaked into summary");
        Require(TaskLogPresentation.Present("[MLP] 曲名")?.Message.Contains("曲名") == true, "Track name lost");
        Require(TaskLogPresentation.Present("[MLP DLL] 192,017 帧 / 48000 Hz / 24 bit / 2 声道，397,374 字节") is { Level: TaskLogLevel.Success }, "Successful encoding not recognized");
    }
    public static void CompleteLineCapture()
    {
        var queue = new ConcurrentQueue<TaskLogLine>();
        var writer = new TaskLogWriter(queue);
        writer.Write("中文"); writer.Write('曲'); writer.Write("名\r\n");
        writer.Write("progress: 10%\rprogress: 20%\r\n"); writer.Write("partial tail"); writer.Flush();
        Require(queue.Select(l => l.Text).SequenceEqual(new[] { "中文曲名", "progress: 10%", "progress: 20%", "partial tail" }), "Fragmented or carriage-return lines were corrupted");
        Parallel.For(0, 500, i => writer.WriteLine($"track-{i}"));
        var lines = queue.Select(l => l.Text).Skip(4).ToArray();
        Require(lines.Length == 500 && lines.Distinct().Count() == 500 && lines.All(l => l.StartsWith("track-")), "Concurrent writes merged or lost lines");
        Require(TaskLogPresentation.Present(lines[0]) is null, "Unclassified technical text should remain in raw logs only");
    }
    public static void ChoiceValuesRemainStable()
    {
        foreach (var definition in SettingDefinition.All.Where(d => d.Kind == SettingKind.Choice))
            foreach (var raw in definition.Choices!)
            {
                var choice = new SettingChoice(raw, definition.DisplayValue(raw));
                Require(choice.Value == raw && choice.ToString().Length > 0, "Friendly display changed the underlying option");
            }
        var source = SettingDefinition.All.Single(d => d.Key == "DVDA_MLP_SOURCE");
        Require(source.DisplayValue("surcode-batch").Contains("内置"), "Internal source key still shown as main label");
        Require(source.DisplayValue("future-custom") == "future-custom", "Unknown imported values were disguised");
        Require(SettingDefinition.All.Select(d => d.Key).Distinct().Count() == SettingDefinition.All.Count, "Duplicate editable option");
    }
}
