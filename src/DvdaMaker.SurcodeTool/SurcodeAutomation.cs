using System.Diagnostics;
using System.Windows.Automation;

namespace DvdaMaker.SurcodeTool;

internal sealed class SurcodeAutomation
{
    private static readonly TimeSpan StartupTimeout = TimeSpan.FromSeconds(15);
    private static readonly TimeSpan DialogTimeout = TimeSpan.FromSeconds(15);
    private static readonly TimeSpan PollInterval = TimeSpan.FromMilliseconds(100);

    private readonly string _executable;

    public SurcodeAutomation(string executable)
    {
        _executable = executable;
    }

    public async Task EncodeAsync(
        string ssfPath,
        string expectedOutput,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        Exception? lastError = null;
        for (var attempt = 1; attempt <= 3; attempt++)
        {
            try
            {
                await EncodeOnceAsync(ssfPath, expectedOutput, timeout, cancellationToken)
                    .ConfigureAwait(false);
                return;
            }
            catch (Exception exception) when (attempt < 3 && exception is not OperationCanceledException)
            {
                lastError = exception;
                Console.Error.WriteLine($"[SurCode] 第 {attempt} 次尝试失败: {exception.Message}");
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken).ConfigureAwait(false);
            }
        }

        throw new InvalidOperationException("SurCode 连续三次编码失败。", lastError);
    }

    private async Task EncodeOnceAsync(
        string ssfPath,
        string expectedOutput,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        ClearStaleWindowTitles();
        TryDelete(expectedOutput);

        using var process = Process.Start(new ProcessStartInfo
        {
            FileName = _executable,
            WorkingDirectory = Path.GetDirectoryName(_executable),
            UseShellExecute = true,
        }) ?? throw new InvalidOperationException("无法启动 surcodemlp.exe。");

        nint mainWindow = 0;
        try
        {
            mainWindow = await WaitForWindowAsync(
                null,
                "MLP Encoder",
                StartupTimeout,
                cancellationToken).ConfigureAwait(false);
            NativeMethods.SendMessage(mainWindow, NativeMethods.WmSetText, 0, "MLP Encoder - In Control");

            var root = AutomationElement.FromHandle(mainWindow);
            var setupMenu = FindNamedElement(root, ControlType.MenuItem, "Item 1", "Setup");
            GetPattern<ExpandCollapsePattern>(setupMenu, ExpandCollapsePattern.Pattern).Expand();
            await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);

            var openMenu = setupMenu.FindFirst(
                TreeScope.Descendants,
                new AndCondition(
                    new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.MenuItem),
                    new PropertyCondition(AutomationElement.AutomationIdProperty, "Item 57601")))
                ?? throw new InvalidOperationException("找不到 SurCode Setup > Open 菜单。" );
            GetPattern<InvokePattern>(openMenu, InvokePattern.Pattern).Invoke();

            var openWindow = await WaitForChildDialogAsync(root, DialogTimeout, cancellationToken)
                .ConfigureAwait(false);
            var textBox = openWindow.FindFirst(
                TreeScope.Descendants,
                new AndCondition(
                    new PropertyCondition(AutomationElement.AutomationIdProperty, "1152"),
                    new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Edit),
                    new PropertyCondition(AutomationElement.ClassNameProperty, "Edit")))
                ?? throw new InvalidOperationException("找不到 SurCode 文件名输入框。" );
            var openButton = openWindow.FindFirst(
                TreeScope.Descendants,
                new AndCondition(
                    new PropertyCondition(AutomationElement.AutomationIdProperty, "1"),
                    new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Button),
                    new PropertyCondition(AutomationElement.ClassNameProperty, "Button")))
                ?? throw new InvalidOperationException("找不到 SurCode Open 按钮。" );

            GetPattern<ValuePattern>(textBox, ValuePattern.Pattern).SetValue(ssfPath);
            await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);
            GetPattern<InvokePattern>(openButton, InvokePattern.Pattern).Invoke();
            await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);

            var encodeButton = root.FindFirst(
                TreeScope.Descendants,
                new AndCondition(
                    new PropertyCondition(AutomationElement.AutomationIdProperty, "1050"),
                    new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Button),
                    new PropertyCondition(AutomationElement.ClassNameProperty, "Button")))
                ?? throw new InvalidOperationException("找不到 SurCode Encode 按钮。" );
            if (!string.Equals(encodeButton.Current.Name, "Encode", StringComparison.Ordinal))
            {
                throw new InvalidOperationException("SurCode 编码按钮名称与预期不符。" );
            }

            GetPattern<InvokePattern>(encodeButton, InvokePattern.Pattern).Invoke();
            await WaitForWindowAsync(
                "#32770",
                "MLP Encoder Log File",
                timeout,
                cancellationToken).ConfigureAwait(false);
            NativeMethods.SendMessage(mainWindow, NativeMethods.WmClose, 0, 0);
            mainWindow = 0;

            var deadline = DateTime.UtcNow.AddSeconds(5);
            while ((!File.Exists(expectedOutput) || new FileInfo(expectedOutput).Length == 0) &&
                DateTime.UtcNow < deadline)
            {
                await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);
            }
            if (!File.Exists(expectedOutput) || new FileInfo(expectedOutput).Length == 0)
            {
                throw new InvalidOperationException($"SurCode 未生成 MLP: {expectedOutput}");
            }
        }
        finally
        {
            if (mainWindow != 0)
            {
                NativeMethods.SendMessage(mainWindow, NativeMethods.WmClose, 0, 0);
            }
            if (!process.HasExited)
            {
                try
                {
                    process.Kill(entireProcessTree: true);
                }
                catch (InvalidOperationException)
                {
                }
            }
        }
    }

    private static AutomationElement FindNamedElement(
        AutomationElement root,
        ControlType type,
        string automationId,
        string name)
    {
        var elements = root.FindAll(
            TreeScope.Descendants,
            new AndCondition(
                new PropertyCondition(AutomationElement.ControlTypeProperty, type),
                new PropertyCondition(AutomationElement.AutomationIdProperty, automationId)));
        foreach (AutomationElement element in elements)
        {
            if (string.Equals(element.Current.Name, name, StringComparison.Ordinal))
            {
                return element;
            }
        }
        throw new InvalidOperationException($"找不到 SurCode 控件: {name}");
    }

    private static T GetPattern<T>(AutomationElement element, AutomationPattern pattern)
        where T : class =>
        element.GetCurrentPattern(pattern) as T
        ?? throw new InvalidOperationException($"SurCode 控件不支持 {typeof(T).Name}。" );

    private static async Task<AutomationElement> WaitForChildDialogAsync(
        AutomationElement root,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var dialog = root.FindFirst(
                TreeScope.Children,
                new PropertyCondition(AutomationElement.ClassNameProperty, "#32770"));
            if (dialog is not null)
            {
                return dialog;
            }
            await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);
        }
        throw new TimeoutException("等待 SurCode Open 对话框超时。" );
    }

    private static async Task<nint> WaitForWindowAsync(
        string? className,
        string title,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var window = NativeMethods.FindWindowEx(0, 0, className, title);
            if (window != 0)
            {
                return window;
            }
            await Task.Delay(PollInterval, cancellationToken).ConfigureAwait(false);
        }
        throw new TimeoutException($"等待窗口超时: {title}");
    }

    private static void ClearStaleWindowTitles()
    {
        ClearTitle(null, "MLP Encoder");
        ClearTitle("#32770", "MLP Encoder Log File");
    }

    private static void ClearTitle(string? className, string title)
    {
        while (NativeMethods.FindWindowEx(0, 0, className, title) is var window && window != 0)
        {
            NativeMethods.SendMessage(window, NativeMethods.WmSetText, 0, string.Empty);
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
