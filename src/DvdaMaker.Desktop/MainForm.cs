using System.Collections.Concurrent;
using System.Diagnostics;
using System.Globalization;
using System.Text;
using DvdaMaker.Configuration;

namespace DvdaMaker.Desktop;

internal sealed class MainForm : Form
{
    private ProjectSettings _settings;
    private readonly Dictionary<string, Control> _editors = new(StringComparer.Ordinal);
    private readonly TabControl _tabs = new() { Dock = DockStyle.Fill };
    private readonly FlowLayoutPanel _profileBar = new() { Dock = DockStyle.Fill, AutoSize = true, Padding = new Padding(6), WrapContents = false };
    private readonly List<Button> _actions = [];
    private readonly Button _cancel = new() { Text = "取消任务", Enabled = false, AutoSize = true, Height = 32 };
    private readonly Label _origin = new() { AutoSize = true, Margin = new Padding(14, 9, 0, 0) };
    private readonly Label _status = new() { Text = "就绪", AutoSize = true, Margin = new Padding(8, 5, 0, 0) };
    private readonly ProgressBar _progress = new() { Width = 200, Height = 18, Margin = new Padding(8, 5, 0, 0) };
    private readonly RichTextBox _log = new() { Dock = DockStyle.Fill, ReadOnly = true, BackColor = Color.FromArgb(245, 247, 250), BorderStyle = BorderStyle.None, Font = new Font("Consolas", 9F) };
    private readonly ConcurrentQueue<string> _messages = new();
    private readonly System.Windows.Forms.Timer _timer = new() { Interval = 100 };
    private readonly ToolTip _tips = new();
    private readonly TextWriter _originalOutput = Console.Out;
    private readonly TextWriter _originalError = Console.Error;
    private CancellationTokenSource? _cancellation;
    private Task? _active;
    private bool _closeWhenDone;
    internal int SmokeExitCode { get; private set; }
    private readonly bool _smoke;

    public MainForm(ProjectSettings settings, string origin, bool smoke)
    {
        _settings = settings; _smoke = smoke;
        Text = "DVD-Audio Maker";
        Font = new Font("Microsoft YaHei UI", 9F);
        StartPosition = FormStartPosition.CenterScreen;
        MinimumSize = new Size(920, 660);
        Size = new Size(1180, 850);
        BackColor = Color.White;
        var root = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 6, ColumnCount = 1, Padding = new Padding(12) };
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.Absolute, 180));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        Controls.Add(root);
        var header = new TableLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, ColumnCount = 1, RowCount = 2,
            BackColor = Color.FromArgb(30, 49, 66), Padding = new Padding(16, 10, 16, 10) };
        header.RowStyles.Add(new RowStyle(SizeType.AutoSize)); header.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        header.Controls.Add(new Label { Text = "DVD-Audio Maker", ForeColor = Color.White, Font = new Font(Font.FontFamily, 20F, FontStyle.Bold),
            AutoSize = true, Margin = Padding.Empty }, 0, 0);
        header.Controls.Add(new Label { Text = "配置项目 · 准备音源 · 制作光盘 · 验证成品", ForeColor = Color.FromArgb(199, 220, 231),
            AutoSize = true, Margin = new Padding(2, 5, 0, 0) }, 0, 1);
        root.Controls.Add(header, 0, 0);
        AddProfileButton("导入 config.env", ImportEnv);
        AddProfileButton("打开方案", OpenProfile);
        AddProfileButton("保存方案", SaveProfile);
        _profileBar.Controls.Add(_origin);
        root.Controls.Add(_profileBar, 0, 1);
        root.Controls.Add(_tabs, 0, 2);
        BuildEditors();
        var commands = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, Padding = new Padding(4, 8, 0, 4), WrapContents = false };
        foreach (var pair in new[] { ("检查音源", WorkflowAction.Prepare), ("构建预演", WorkflowAction.Preview), ("开始制作", WorkflowAction.Build), ("验证成品", WorkflowAction.Verify) })
        {
            var button = new Button { Text = pair.Item1, AutoSize = true, Height = 32, Padding = new Padding(8, 2, 8, 2), Margin = new Padding(4) };
            if (pair.Item2 == WorkflowAction.Build) { button.BackColor = Color.FromArgb(22, 111, 116); button.ForeColor = Color.White; button.FlatStyle = FlatStyle.Flat; }
            var action = pair.Item2;
            button.Click += (_, _) => { if (_active is null) _active = RunAsync(action); };
            _actions.Add(button); commands.Controls.Add(button);
        }
        _cancel.Click += (_, _) => CancelTask(); commands.Controls.Add(_cancel);
        var open = new Button { Text = "打开输出目录", AutoSize = true, Height = 32, Margin = new Padding(14, 4, 4, 4) };
        open.Click += (_, _) => Guard(() =>
        {
            var path = CaptureSettings().ToOptions().FinalDirectory;
            if (!Directory.Exists(path)) throw new DirectoryNotFoundException("成品目录尚不存在。");
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
        });
        commands.Controls.Add(open); root.Controls.Add(commands, 0, 3);
        var logPanel = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 2, ColumnCount = 1, Margin = new Padding(5) };
        logPanel.RowStyles.Add(new RowStyle(SizeType.AutoSize)); logPanel.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        var logBar = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, WrapContents = false };
        logBar.Controls.Add(new Label { Text = "任务日志", AutoSize = true, Margin = new Padding(0, 5, 18, 0) });
        var saveLog = new Button { Text = "保存日志", AutoSize = true, Height = 24 };
        saveLog.Click += (_, _) => Guard(() =>
        {
            using var dialog = new SaveFileDialog { Filter = "文本日志|*.txt", FileName = "dvd-audio-log.txt" };
            if (dialog.ShowDialog(this) == DialogResult.OK) File.WriteAllText(dialog.FileName, _log.Text, Encoding.UTF8);
        });
        logBar.Controls.Add(saveLog);
        logPanel.Controls.Add(logBar, 0, 0); logPanel.Controls.Add(_log, 0, 1); root.Controls.Add(logPanel, 0, 4);
        var statusBar = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, WrapContents = false };
        statusBar.Controls.Add(_progress); statusBar.Controls.Add(_status); root.Controls.Add(statusBar, 0, 5);
        Populate(settings, origin);
        Console.SetOut(new QueueWriter(_messages)); Console.SetError(new QueueWriter(_messages));
        _timer.Tick += (_, _) => DrainLog(); _timer.Start();
        FormClosing += OnClosing;
        FormClosed += (_, _) => { _timer.Stop(); _timer.Dispose(); _tips.Dispose(); Console.SetOut(_originalOutput); Console.SetError(_originalError); };
        Shown += (_, _) =>
        {
            _messages.Enqueue("MLP 使用进程内编码 DLL；预演会生成 MLP 缓存，正式制作会发布 ISO。\n");
            if (_smoke)
            {
                var roundtrip = CaptureSettings();
                if (roundtrip.Values.Count < ConfigDefaults.Values.Count || _editors.Count != SettingDefinition.All.Count)
                    throw new InvalidOperationException("GUI 设置控件未完整构建。");
                var snapshot = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_IMAGE");
                if (!string.IsNullOrWhiteSpace(snapshot))
                {
                    using var bitmap = new Bitmap(Width, Height);
                    DrawToBitmap(bitmap, new Rectangle(Point.Empty, Size));
                    bitmap.Save(snapshot, System.Drawing.Imaging.ImageFormat.Png);
                }
                var smokeAction = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_ACTION");
                if (!string.IsNullOrWhiteSpace(smokeAction) && Enum.TryParse<WorkflowAction>(smokeAction, true, out var action))
                {
                    _active = RunAsync(action);
                }
                else BeginInvoke(new Action(Close));
            }
        };
    }

    private void BuildEditors()
    {
        foreach (var group in SettingDefinition.All.GroupBy(item => item.Group))
        {
            var page = new TabPage(group.Key) { AutoScroll = true, BackColor = Color.White };
            var table = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 3, Padding = new Padding(12) };
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 178));
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 60));
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 40));
            var row = 0;
            foreach (var definition in group)
            {
                table.RowStyles.Add(new RowStyle(SizeType.Absolute, 46));
                var label = new Label { Text = definition.Label, Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true };
                table.Controls.Add(label, 0, row);
                Control editor;
                if (definition.Kind == SettingKind.Boolean) editor = new CheckBox { Text = "启用", AutoSize = true, Anchor = AnchorStyles.Left };
                else if (definition.Kind == SettingKind.Number) editor = new NumericUpDown { Minimum = definition.Minimum, Maximum = definition.Maximum, ThousandsSeparator = true, Dock = DockStyle.Fill };
                else if (definition.Kind == SettingKind.Choice)
                {
                    var combo = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Dock = DockStyle.Fill };
                    combo.Items.AddRange(definition.Choices!.Cast<object>().ToArray()); editor = combo;
                }
                else editor = new TextBox { Dock = DockStyle.Fill };
                editor.Name = definition.Key; editor.Margin = new Padding(3, 10, 3, 8);
                _editors.Add(definition.Key, editor);
                if (definition.Kind is SettingKind.Folder or SettingKind.File)
                {
                    var picker = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, Margin = Padding.Empty };
                    picker.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100)); picker.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 43));
                    picker.Controls.Add(editor, 0, 0);
                    var browse = new Button { Text = "…", Dock = DockStyle.Fill, Margin = new Padding(3, 8, 3, 8) };
                    browse.Click += (_, _) => Browse(definition, editor); picker.Controls.Add(browse, 1, 0); table.Controls.Add(picker, 1, row);
                }
                else table.Controls.Add(editor, 1, row);
                var help = new Label { Text = definition.Help, ForeColor = Color.DimGray, Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleLeft, Margin = new Padding(12, 0, 0, 0), AutoEllipsis = true };
                table.Controls.Add(help, 2, row++); _tips.SetToolTip(editor, definition.Help); _tips.SetToolTip(help, definition.Help);
            }
            page.Controls.Add(table); _tabs.TabPages.Add(page);
        }
    }

    private void Browse(SettingDefinition definition, Control editor) => Guard(() =>
    {
        if (definition.Kind == SettingKind.Folder)
        {
            using var dialog = new FolderBrowserDialog { Description = definition.Label, UseDescriptionForTitle = true, SelectedPath = editor.Text };
            if (dialog.ShowDialog(this) == DialogResult.OK) editor.Text = dialog.SelectedPath;
        }
        else
        {
            using var dialog = new OpenFileDialog { Title = definition.Label, Filter = "所有文件|*.*", CheckFileExists = true };
            if (dialog.ShowDialog(this) == DialogResult.OK) editor.Text = dialog.FileName;
        }
    });

    private void Populate(ProjectSettings settings, string origin)
    {
        _settings = settings.Clone();
        foreach (var definition in SettingDefinition.All)
        {
            var value = settings.Values.GetValueOrDefault(definition.Key, definition.Key == "DVDA_TITLE_MODE" ? "album" : "");
            var editor = _editors[definition.Key];
            switch (editor)
            {
                case CheckBox check: check.Checked = value.ToLowerInvariant() is "1" or "true" or "yes" or "on"; break;
                case NumericUpDown number:
                    number.Value = decimal.TryParse(value, NumberStyles.Number, CultureInfo.InvariantCulture, out var n)
                        ? Math.Clamp(n, number.Minimum, number.Maximum) : number.Minimum; break;
                case ComboBox combo:
                    if (definition.Key == "DVDA_MLP_SOURCE" && value.Equals("batch-surcode", StringComparison.OrdinalIgnoreCase)) value = "surcode-batch";
                    if (!combo.Items.Contains(value)) combo.Items.Add(value);
                    combo.SelectedItem = value; break;
                default: editor.Text = value; break;
            }
        }
        _origin.Text = "来源：" + (origin == "新建配置" ? origin : Path.GetFileName(origin));
        _tips.SetToolTip(_origin, origin);
    }

    private ProjectSettings CaptureSettings()
    {
        var result = _settings.Clone();
        foreach (var pair in _editors)
            result.Values[pair.Key] = pair.Value switch
            {
                CheckBox check => check.Checked ? "on" : "off",
                NumericUpDown number => number.Value.ToString(CultureInfo.InvariantCulture),
                ComboBox combo => combo.SelectedItem?.ToString() ?? "",
                _ => pair.Value.Text.Trim(),
            };
        return result;
    }

    private void AddProfileButton(string text, Action action)
    {
        var button = new Button { Text = text, AutoSize = true, Height = 29 };
        button.Click += (_, _) => Guard(action); _profileBar.Controls.Add(button);
    }
    private void ImportEnv()
    {
        using var dialog = new OpenFileDialog { Title = "导入旧配置文件", Filter = "环境配置|*.env|所有文件|*.*" };
        if (dialog.ShowDialog(this) == DialogResult.OK) Populate(ProjectSettings.ImportEnv(dialog.FileName), dialog.FileName);
    }
    private void OpenProfile()
    {
        using var dialog = new OpenFileDialog { Title = "打开配置方案", Filter = "配置方案|*.json" };
        if (dialog.ShowDialog(this) == DialogResult.OK) Populate(ProjectSettings.Load(dialog.FileName), dialog.FileName);
    }
    private void SaveProfile()
    {
        using var dialog = new SaveFileDialog { Title = "保存配置方案", Filter = "配置方案|*.json", FileName = "dvd-audio-project.json" };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        _settings = CaptureSettings(); _settings.Save(dialog.FileName); _settings.Save(ProjectSettings.DefaultPath);
        _origin.Text = "已保存：" + Path.GetFileName(dialog.FileName);
    }

    private async Task RunAsync(WorkflowAction action)
    {
        await Task.Yield(); // Let the click handler assign _active before any early return.
        try
        {
            if (_closeWhenDone) return;
            var snapshot = CaptureSettings();
            var errors = snapshot.Validate(requireSource: action != WorkflowAction.Verify, requireEncoding: action != WorkflowAction.Verify);
            if (errors.Count > 0) throw new InvalidOperationException(string.Join(Environment.NewLine, errors));
            if (!_smoke) snapshot.Save(ProjectSettings.DefaultPath);
            _settings = snapshot;
            _cancellation = new CancellationTokenSource();
            SetBusy(true); _progress.Value = 0; _log.Clear();
            var progress = new Progress<WorkflowProgress>(p => { _progress.Value = p.Percent; _status.Text = p.Message; });
            var token = _cancellation.Token;
            await Task.Run(() => DesktopWorkflow.RunAsync(action, snapshot.ToOptions(), progress, token), token);
            _status.Text = "完成"; _progress.Value = 100;
        }
        catch (OperationCanceledException) { _status.Text = "已取消"; _messages.Enqueue("[取消] 任务已停止，未发布的临时输出已清理。\n"); }
        catch (Exception exception) { SmokeExitCode = 1; _status.Text = "失败，请查看日志"; _messages.Enqueue("[错误] " + exception.Message + "\n"); }
        finally
        {
            DrainLog(); _cancellation?.Dispose(); _cancellation = null; _active = null; SetBusy(false);
            if (_smoke)
            {
                var logPath = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_LOG");
                if (!string.IsNullOrWhiteSpace(logPath)) File.WriteAllText(logPath, _log.Text, Encoding.UTF8);
            }
            if (_closeWhenDone || _smoke) Close();
        }
    }
    private void SetBusy(bool busy)
    {
        _tabs.Enabled = !busy; _profileBar.Enabled = !busy;
        foreach (var button in _actions) button.Enabled = !busy;
        _cancel.Enabled = busy;
    }
    private void CancelTask()
    {
        _cancellation?.Cancel(); _cancel.Enabled = false; _status.Text = "正在取消并释放资源…";
    }
    private void OnClosing(object? sender, FormClosingEventArgs e)
    {
        if (_active is not null)
        {
            e.Cancel = true; _closeWhenDone = true; CancelTask(); return;
        }
        if (!_smoke) Guard(() => CaptureSettings().Save(ProjectSettings.DefaultPath));
    }
    private void DrainLog()
    {
        var text = new StringBuilder();
        for (var i = 0; i < 500 && _messages.TryDequeue(out var message); i++) text.Append(message);
        if (text.Length == 0) return;
        if (_log.TextLength > 500000) { _log.Select(0, 100000); _log.SelectedText = ""; }
        _log.AppendText(text.ToString()); _log.SelectionStart = _log.TextLength; _log.ScrollToCaret();
    }
    private void Guard(Action action)
    {
        try { action(); }
        catch (Exception exception) { MessageBox.Show(this, exception.Message, "DVD-Audio Maker", MessageBoxButtons.OK, MessageBoxIcon.Error); }
    }
    private sealed class QueueWriter(ConcurrentQueue<string> messages) : TextWriter
    {
        public override Encoding Encoding => Encoding.UTF8;
        public override void Write(char value) => messages.Enqueue(value.ToString());
        public override void Write(string? value) { if (value is not null) messages.Enqueue(value); }
        public override void WriteLine(string? value) => messages.Enqueue((value ?? "") + Environment.NewLine);
    }
}
