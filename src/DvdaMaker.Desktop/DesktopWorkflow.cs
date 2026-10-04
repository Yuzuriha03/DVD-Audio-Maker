using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.Preparation;

namespace DvdaMaker.Desktop;

internal enum WorkflowAction { Prepare, Build, Verify }
internal sealed record WorkflowProgress(string Title, string Detail);
internal sealed record WorkflowOutcome(string Title, string Detail);

internal static class DesktopWorkflow
{
    public static async Task<WorkflowOutcome> RunAsync(WorkflowAction action, DvdaOptions options,
        IProgress<WorkflowProgress> progress, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        if (action is WorkflowAction.Prepare or WorkflowAction.Build)
        {
            if (action == WorkflowAction.Build &&
                PreparationSnapshotStore.TryReuse(options, out _, out var reuseReason))
            {
                progress.Report(new("复用音源检查结果", $"{reuseReason}，直接进入制作。"));
                Console.WriteLine($"[准备] {reuseReason}：{options.PrepareSnapshotPath}");
            }
            else
            {
                progress.Report(new("正在检查音源", "读取曲目信息并确认音频能够正常解码，请稍候。"));
                var prepared = await new PreparationPipeline(options).RunAsync(cancellationToken: token).ConfigureAwait(false);
                foreach (var issue in prepared.Issues) Console.WriteLine($"[{issue.Level}] {issue}");
                if (prepared.FailureCount > 0) throw new InvalidOperationException($"音源检查失败 {prepared.FailureCount} 项，请查看日志。" );
            }
            if (action == WorkflowAction.Prepare) return new("音源检查完成", "请点击“开始制作”生成光盘镜像。" );
            token.ThrowIfCancellationRequested();
            progress.Report(new("正在制作光盘", "准备无损编码、计算分盘并处理光盘内容；较长音轨需要一些时间。"));
            var built = await new BuildPipeline(options).RunAsync(dryRun: false, cancellationToken: token).ConfigureAwait(false);
            BuildPlanService.Print(built.Plan, options, Console.Out);
            if (built.Plan.HasErrors || built.DiscResults.Any(result => !result.Succeeded))
                throw new InvalidOperationException("构建未全部成功，请查看日志和构建目录。");
            Console.WriteLine($"索引：{built.IndexPath}");
            foreach (var disc in built.DiscResults) Console.WriteLine($"成品：{disc.PublishedIsoPath}");
            return new("制作完成", $"已制作 {built.Plan.Discs.Count} 张光盘。点击“查看成品”打开保存位置，建议再运行“验证成品”。");
        }
        var pipeline = new VerificationPipeline(options);
        var errors = new List<string>();
        progress.Report(new("正在检查光盘容量", "确认已制作的光盘镜像与容量限制。"));
        foreach (var result in pipeline.VerifyCapacity())
        {
            token.ThrowIfCancellationRequested();
            foreach (var issue in result.Issues) errors.Add($"{issue.Code}: {issue.Message}");
            Console.WriteLine($"容量检查：{result.IsoPath}，{(result.Succeeded ? "通过" : "失败")}");
        }
        progress.Report(new("正在检查曲目索引", "核对光盘中的曲目与制作记录。"));
        foreach (var result in new DiscVerifier().Audit(options.FinalDirectory, options.BuildLogPath,
            File.Exists(options.ManifestPath) ? options.ManifestPath : null, allowLogFallback: true, isoPrefix: options.IsoPrefix))
        {
            token.ThrowIfCancellationRequested();
            foreach (var issue in result.Issues) errors.Add($"{issue.Code}: {issue.Message}");
            if (result.Unavailable) errors.Add("部分光盘索引验证缺少必要材料。");
        }
        progress.Report(new("正在检查播放时间轴", "检查所有音频组的播放时间和顺序。"));
        token.ThrowIfCancellationRequested();
        var timeline = pipeline.VerifyTimeline();
        foreach (var issue in timeline.Issues) errors.Add($"{issue.Code}: {issue.Message}");
        if (timeline.Unavailable) errors.Add("时间轴验证不可用。");
        if (options.MenuEnabled)
        {
            progress.Report(new("正在检查选曲菜单", "确认菜单结构和跳转目标。"));
            foreach (var menu in await pipeline.VerifyMenuAsync(token).ConfigureAwait(false))
            {
                foreach (var issue in menu.Issues) errors.Add($"{issue.Code}: {issue.Message}");
                if (menu.Unavailable) errors.Add("菜单验证不可用。");
            }
        }
        progress.Report(new("正在逐轨检查音频", "逐轨比较目标 PCM，并核对每张盘内的全部音频字节。"));
        var lossless = await pipeline.VerifyLosslessAsync(token).ConfigureAwait(false);
        foreach (var issue in lossless.Issues) errors.Add($"{issue.Code}: {issue.Message}");
        if (lossless.Unavailable) errors.Add("逐轨无损验证不可用。");
        foreach (var error in errors) Console.Error.WriteLine("[错误] " + error);
        if (errors.Count > 0) throw new InvalidOperationException($"验证有 {errors.Count} 项未通过，请查看日志。");
        return new("成品验证通过", "容量、索引、时间轴、菜单和全部轨道的无损检查完成。");
    }
}
