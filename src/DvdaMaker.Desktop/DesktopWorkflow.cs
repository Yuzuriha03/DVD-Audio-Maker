using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.Preparation;

namespace DvdaMaker.Desktop;

internal enum WorkflowAction { Prepare, Preview, Build, Verify }
internal sealed record WorkflowProgress(int Percent, string Message);

internal static class DesktopWorkflow
{
    public static async Task RunAsync(WorkflowAction action, DvdaOptions options,
        IProgress<WorkflowProgress> progress, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        if (action is WorkflowAction.Prepare or WorkflowAction.Preview or WorkflowAction.Build)
        {
            progress.Report(new(5, "检查音源与生成清单"));
            var prepared = await new PreparationPipeline(options).RunAsync(cancellationToken: token).ConfigureAwait(false);
            foreach (var issue in prepared.Issues) Console.WriteLine($"[{issue.Level}] {issue}");
            if (prepared.FailureCount > 0) throw new InvalidOperationException($"音源检查失败 {prepared.FailureCount} 项，请查看日志。");
            if (action == WorkflowAction.Prepare) { progress.Report(new(100, "音源准备完成")); return; }
            token.ThrowIfCancellationRequested();
            progress.Report(new(35, action == WorkflowAction.Preview ? "编码 MLP 与预演分盘" : "编码 MLP 与制作光盘"));
            var built = await new BuildPipeline(options).RunAsync(action == WorkflowAction.Preview, cancellationToken: token).ConfigureAwait(false);
            BuildPlanService.Print(built.Plan, options, Console.Out);
            if (built.Plan.HasErrors || built.DiscResults.Any(result => !result.Succeeded))
                throw new InvalidOperationException("构建未全部成功，请查看日志和构建目录。");
            Console.WriteLine($"索引：{built.IndexPath}");
            foreach (var disc in built.DiscResults) Console.WriteLine($"成品：{disc.PublishedIsoPath}");
            progress.Report(new(100, action == WorkflowAction.Preview ? "预演完成（已编码 MLP，未生成 ISO）" : "光盘构建完成"));
            return;
        }
        var pipeline = new VerificationPipeline(options);
        var errors = new List<string>();
        progress.Report(new(10, "验证容量与光盘结构"));
        foreach (var result in pipeline.VerifyCapacity())
        {
            token.ThrowIfCancellationRequested();
            foreach (var issue in result.Issues) errors.Add($"{issue.Code}: {issue.Message}");
            Console.WriteLine($"容量检查：{result.IsoPath}，{(result.Succeeded ? "通过" : "失败")}");
        }
        progress.Report(new(25, "验证曲目索引"));
        foreach (var result in new DiscVerifier().Audit(options.FinalDirectory, options.BuildLogPath,
            File.Exists(options.ManifestPath) ? options.ManifestPath : null, allowLogFallback: true, isoPrefix: options.IsoPrefix))
        {
            token.ThrowIfCancellationRequested();
            foreach (var issue in result.Issues) errors.Add($"{issue.Code}: {issue.Message}");
            if (result.Unavailable) errors.Add("部分光盘索引验证缺少必要材料。");
        }
        progress.Report(new(45, "验证全部音频组时间轴"));
        token.ThrowIfCancellationRequested();
        var timeline = pipeline.VerifyTimeline();
        foreach (var issue in timeline.Issues) errors.Add($"{issue.Code}: {issue.Message}");
        if (timeline.Unavailable) errors.Add("时间轴验证不可用。");
        if (options.MenuEnabled)
        {
            progress.Report(new(65, "验证菜单"));
            foreach (var menu in await pipeline.VerifyMenuAsync(token).ConfigureAwait(false))
            {
                foreach (var issue in menu.Issues) errors.Add($"{issue.Code}: {issue.Message}");
                if (menu.Unavailable) errors.Add("菜单验证不可用。");
            }
        }
        progress.Report(new(80, "验证首轨 PCM 与成品 MLP（抽样）"));
        var lossless = await pipeline.VerifyLosslessAsync(token).ConfigureAwait(false);
        foreach (var issue in lossless.Issues) errors.Add($"{issue.Code}: {issue.Message}");
        if (lossless.Unavailable) errors.Add("抽样无损验证不可用。");
        foreach (var error in errors) Console.Error.WriteLine(error);
        if (errors.Count > 0) throw new InvalidOperationException($"验证有 {errors.Count} 项未通过，请查看日志。");
        progress.Report(new(100, "验证通过（无损项为首轨抽样）"));
    }
}
