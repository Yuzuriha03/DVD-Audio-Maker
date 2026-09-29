using System.Globalization;
using System.Text.RegularExpressions;
using DvdaMaker.Processes;

namespace DvdaMaker.Building;

public sealed partial class MenuBuildVerifier(ProcessRunner runner, BuildLogWriter? log = null)
{
    public async Task<IReadOnlyList<BuildDiagnostic>> VerifyAsync(
        string audioTsDirectory,
        string temporaryDirectory,
        MenuAssets assets,
        CancellationToken cancellationToken = default)
    {
        var diagnostics = new List<BuildDiagnostic>();
        var menuVob = Path.Combine(audioTsDirectory, "AUDIO_TS.VOB");
        if (!File.Exists(menuVob) || new FileInfo(menuVob).Length == 0)
        {
            diagnostics.Add(Error(
                "MENU_VOB_MISSING",
                $"dvda-author 未生成菜单文件 {menuVob}。"));
            return diagnostics;
        }

        if (assets.StillPictures.Any(path => path.Length > 0))
        {
            var stillVob = Path.Combine(audioTsDirectory, "AUDIO_SV.VOB");
            if (!File.Exists(stillVob) || new FileInfo(stillVob).Length == 0)
            {
                diagnostics.Add(new BuildDiagnostic(
                    BuildDiagnosticSeverity.Warning,
                    "MENU_STILL_VOB_MISSING",
                    "已配置播放封面，但未生成 AUDIO_SV.VOB。"));
            }
        }

        VerifyButtons(temporaryDirectory, assets.Plan, diagnostics);
        if (diagnostics.Any(item => item.Severity == BuildDiagnosticSeverity.Error))
        {
            return diagnostics;
        }

        var identify = FindImageMagickIdentify(assets.BinaryDirectory);
        if (identify is null)
        {
            diagnostics.Add(new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "MENU_OVERLAY_CHECK_SKIPPED",
                "找不到 ImageMagick，跳过菜单叠加图墨迹自检。"));
            return diagnostics;
        }

        await VerifyOverlaysAsync(
            identify.Value.Executable,
            identify.Value.PrefixArguments,
            temporaryDirectory,
            assets.Plan,
            diagnostics,
            cancellationToken).ConfigureAwait(false);
        return diagnostics;
    }

    internal static void VerifyButtons(
        string temporaryDirectory,
        MenuPlan plan,
        ICollection<BuildDiagnostic> diagnostics)
    {
        var projectXml = Path.Combine(temporaryDirectory, "xmltemp");
        if (!File.Exists(projectXml))
        {
            diagnostics.Add(Error(
                "MENU_BUTTON_XML_MISSING",
                $"找不到菜单跳转 XML: {projectXml}"));
            return;
        }

        var project = File.ReadAllText(projectXml);
        var pages = project.Split("<pgc>", StringSplitOptions.None).Skip(1).ToArray();
        if (pages.Length != plan.TotalPages)
        {
            diagnostics.Add(Error(
                "MENU_BUTTON_PAGE_MISMATCH",
                $"菜单跳转 XML 有 {pages.Length} 页，规划为 {plan.TotalPages} 页。"));
        }

        for (var pageIndex = 0; pageIndex < pages.Length; pageIndex++)
        {
            var jumpButtons = ButtonNamePattern().Matches(pages[pageIndex])
                .Select(match => int.Parse(
                    match.Groups[1].Value,
                    CultureInfo.InvariantCulture))
                .ToArray();
            var overlayXml = Path.Combine(
                temporaryDirectory,
                $"spu_xmltemp_{pageIndex}.xml");
            if (!File.Exists(overlayXml))
            {
                diagnostics.Add(Error(
                    "MENU_BUTTON_OVERLAY_XML_MISSING",
                    $"第 {pageIndex + 1} 页缺少按钮位置 XML。"));
                continue;
            }

            var overlayButtons = ButtonNamePattern().Matches(File.ReadAllText(overlayXml))
                .Select(match => int.Parse(
                    match.Groups[1].Value,
                    CultureInfo.InvariantCulture))
                .ToArray();
            if (!jumpButtons.SequenceEqual(overlayButtons))
            {
                diagnostics.Add(Error(
                    "MENU_BUTTON_MISMATCH",
                    $"第 {pageIndex + 1} 页按钮编号不一致：" +
                    $"跳转 [{string.Join(',', jumpButtons)}]，" +
                    $"位置 [{string.Join(',', overlayButtons)}]。"));
            }

            if (plan.IndexPages > 0 && pageIndex >= plan.IndexPages)
            {
                var menuTargets = MenuJumpPattern().Matches(pages[pageIndex])
                    .Select(match => match.Groups[1].Value)
                    .ToArray();
                if (menuTargets.Length == 0 || menuTargets[^1] != "1")
                {
                    diagnostics.Add(Error(
                        "MENU_RETURN_BUTTON_MISSING",
                        $"第 {pageIndex + 1} 个菜单页缺少末尾的 jump menu 1 返回索引按钮。"));
                }
            }
        }
    }

    private async Task VerifyOverlaysAsync(
        string executable,
        IReadOnlyList<string> prefixArguments,
        string temporaryDirectory,
        MenuPlan plan,
        ICollection<BuildDiagnostic> diagnostics,
        CancellationToken cancellationToken)
    {
        for (var page = 0; page < plan.TotalPages; page++)
        {
            var normal = Path.Combine(temporaryDirectory, $"impic{page}.png");
            var highlighted = Path.Combine(temporaryDirectory, $"hlpic{page}.png");
            var normalInk = await ReadInkAsync(
                executable, prefixArguments, normal, null, cancellationToken)
                .ConfigureAwait(false);
            var highlightedInk = await ReadInkAsync(
                executable, prefixArguments, highlighted, null, cancellationToken)
                .ConfigureAwait(false);
            if (normalInk is null || highlightedInk is null)
            {
                diagnostics.Add(Error(
                    "MENU_OVERLAY_MISSING",
                    $"第 {page + 1} 页缺少可读取的文字层或高亮层。"));
                continue;
            }
            if (highlightedInk <= normalInk)
            {
                diagnostics.Add(Error(
                    "MENU_OVERLAY_EMPTY",
                    $"第 {page + 1} 页高亮层没有比文字层增加墨迹。"));
            }

            if (page < plan.IndexPages && plan.TotalPages > 1)
            {
                var arrowInk = await ReadInkAsync(
                    executable,
                    prefixArguments,
                    normal,
                    "720x72+0+488",
                    cancellationToken).ConfigureAwait(false);
                if (arrowInk is not null && arrowInk <= 0)
                {
                    diagnostics.Add(Error(
                        "MENU_INDEX_ARROW_MISSING",
                        $"第 {page + 1} 个索引页的翻页箭头区域没有墨迹。"));
                }
            }
        }
    }

    private async Task<double?> ReadInkAsync(
        string executable,
        IReadOnlyList<string> prefixArguments,
        string image,
        string? crop,
        CancellationToken cancellationToken)
    {
        if (!File.Exists(image))
        {
            return null;
        }

        var arguments = new List<string>(prefixArguments);
        if (crop is not null)
        {
            arguments.AddRange(["-crop", crop]);
        }
        arguments.AddRange([
            "-format",
            crop is null ? "%[fx:mean.a*w*h]" : "%[fx:maxima.a]",
            image,
        ]);
        log?.WriteCommand(executable, arguments);
        var result = await runner.RunAsync(new ProcessRequest
        {
            FileName = executable,
            Arguments = arguments,
            Timeout = TimeSpan.FromSeconds(30),
        }, cancellationToken).ConfigureAwait(false);
        if (!result.Succeeded)
        {
            return null;
        }
        return double.TryParse(
            result.StandardOutput.Trim(),
            NumberStyles.Float,
            CultureInfo.InvariantCulture,
            out var value)
            ? value
            : null;
    }

    private static (string Executable, IReadOnlyList<string> PrefixArguments)?
        FindImageMagickIdentify(string preferredDirectory)
    {
        var magick = FindExecutable("magick", preferredDirectory);
        if (magick is not null)
        {
            return (magick, ["identify"]);
        }
        var identify = FindExecutable("identify", preferredDirectory);
        return identify is null ? null : (identify, []);
    }

    private static string? FindExecutable(string name, string preferredDirectory)
    {
        var names = OperatingSystem.IsWindows()
            ? new[] { name + ".exe", name + ".cmd", name + ".bat", name }
            : new[] { name };
        foreach (var candidateName in names)
        {
            var preferred = Path.Combine(preferredDirectory, candidateName);
            if (File.Exists(preferred)) return preferred;
        }
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            foreach (var candidateName in names)
            {
                var candidate = Path.Combine(directory.Trim('"'), candidateName);
                if (File.Exists(candidate)) return candidate;
            }
        }
        return null;
    }

    private static BuildDiagnostic Error(string code, string message) =>
        new(BuildDiagnosticSeverity.Error, code, message);

    [GeneratedRegex("name=\"button(\\d+)\"")]
    private static partial Regex ButtonNamePattern();

    [GeneratedRegex("name=\"button\\d+\">jump menu (\\d+);")]
    private static partial Regex MenuJumpPattern();
}
