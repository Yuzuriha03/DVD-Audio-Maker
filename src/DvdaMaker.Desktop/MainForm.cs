using DvdaMaker.Localization;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Globalization;
using System.Text;
using DvdaMaker.Configuration;

namespace DvdaMaker.Desktop;

internal sealed class MainForm : Form
{
    private ProjectSettings _settings;
    private string _profileOrigin = "";
    private readonly ComboBox _language = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 108, Margin = new Padding(6, 7, 0, 0) };
    internal string? RequestedLanguage { get; private set; }
    internal ProjectSettings CurrentSettings => _settings.Clone();
    internal string ProfileOrigin => _profileOrigin;
    private readonly Dictionary<string, Control> _editors = new(StringComparer.Ordinal);
    private readonly TabControl _tabs = new() { Dock = DockStyle.Fill };
    private readonly FlowLayoutPanel _profileBar = new() { Dock = DockStyle.Fill, AutoSize = true, Padding = new Padding(6), WrapContents = false };
    private readonly List<Button> _actions = [];
    private readonly Button _cancel = MakeCommandButton(L.T("停止任务"));
    private readonly Label _origin = new() { AutoSize = true, Margin = new Padding(14, 9, 0, 0) };
    private readonly Label _status = new() { Text = L.T("准备就绪"), AutoSize = true, Font = new Font("Microsoft YaHei UI", 12F, FontStyle.Bold), ForeColor = Color.FromArgb(31, 48, 63) };
    private readonly ProgressBar _progress = new() { Dock = DockStyle.Top, Height = 5, MarqueeAnimationSpeed = 25, Margin = new Padding(3, 5, 3, 8) };
    private readonly RichTextBox _log = new() { Dock = DockStyle.Fill, ReadOnly = true, BackColor = Color.White, BorderStyle = BorderStyle.None, Font = new Font("Microsoft YaHei UI", 9F), DetectUrls = false };
    private readonly ConcurrentQueue<TaskLogLine> _messages = new();
    private readonly List<TaskLogLine> _rawLines = [];
    private readonly List<TaskLogLine> _summaryLines = [];
    private readonly ComboBox _logView = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 135 };
    private readonly CheckBox _onlyProblems = new() { Text = L.T("只看提醒"), AutoSize = true, Margin = new Padding(10, 8, 10, 0) };
    private readonly CheckBox _live = new() { Text = L.T("实时更新"), Checked = true, AutoSize = true, Margin = new Padding(0, 8, 10, 0) };
    private readonly Label _logCount = new() { Text = L.T("暂无问题提示"), AutoSize = true, ForeColor = Color.DimGray, Margin = new Padding(10, 8, 0, 0) };
    private readonly Label _activity = new() { Text = L.T("选择音源和成品位置，然后点击“开始制作”。"), AutoSize = true, ForeColor = Color.FromArgb(98, 113, 128) };
    private readonly Label _elapsedLabel = new() { Text = L.T("尚未开始任务"), AutoSize = true, ForeColor = Color.DimGray, Anchor = AnchorStyles.Right };
    private readonly Stopwatch _elapsed = new();
    private StreamWriter? _archive;
    private string? _archivePath;
    private int _rendered;
    private bool _rebuildLog;
    private int _problemCount;
    private bool _running;
    private readonly TaskLogWriter _output;
    private readonly TaskLogWriter _error;
    private readonly System.Windows.Forms.Timer _timer = new() { Interval = 100 };
    private readonly ToolTip _tips = new();
    private readonly TextWriter _originalOutput = Console.Out;
    private readonly TextWriter _originalError = Console.Error;
    private CancellationTokenSource? _cancellation;
    private Task? _active;
    private bool _closeWhenDone;
    internal int SmokeExitCode { get; private set; }
    private readonly bool _smoke;
    private bool _smokeReady;

    public MainForm(ProjectSettings settings, string origin, bool smoke)
    {
        _settings = settings; _smoke = smoke; _output = new(_messages); _error = new(_messages);
        Text = "DVD-Audio Maker"; Font = new Font("Microsoft YaHei UI", 9F);
        AutoScaleDimensions = new SizeF(96, 96); AutoScaleMode = AutoScaleMode.Dpi;
        StartPosition = FormStartPosition.CenterScreen; MinimumSize = new Size(950, 730); Size = new Size(1160, 900);
        BackColor = Color.FromArgb(244, 247, 250); ForeColor = Color.FromArgb(31, 48, 63);
        var root = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 3, ColumnCount = 1, Padding = new Padding(14) };
        root.ColumnStyles.Add(new(SizeType.Percent, 100));
        root.RowStyles.Add(new(SizeType.AutoSize)); root.RowStyles.Add(new(SizeType.Percent, 100));
        root.RowStyles.Add(new(SizeType.AutoSize)); Controls.Add(root);
        var header = new TableLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, ColumnCount = 2, Margin = new Padding(0, 0, 0, 10) };
        header.ColumnStyles.Add(new(SizeType.Percent, 100)); header.ColumnStyles.Add(new(SizeType.AutoSize));
        var branding = new TableLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, ColumnCount = 1, Margin = Padding.Empty };
        branding.Controls.Add(new Label { Text = "DVD-Audio Maker", Font = new Font(Font.FontFamily, 18F, FontStyle.Bold), AutoSize = true, Margin = Padding.Empty });
        branding.Controls.Add(new Label { Text = L.T("把音乐制作成 DVD-Audio 光盘镜像"), ForeColor = Color.FromArgb(98, 113, 128), AutoSize = true, Margin = new Padding(1, 3, 0, 0) });
        _language.Items.AddRange(["中文", "English", "日本語"]);
        _language.SelectedIndex = L.Language switch { "en" => 1, "ja" => 2, _ => 0 };
        _language.AccessibleName = "Language / 语言 / 言語";
        _tips.SetToolTip(_language, "Language / 语言 / 言語");
        _language.SelectedIndexChanged += (_, _) =>
        {
            if (_active is not null) return;
            RequestedLanguage = _language.SelectedIndex switch { 1 => "en", 2 => "ja", _ => "zh-CN" };
            _settings.Language = RequestedLanguage;
            Close();
        };
        var profiles = new TableLayoutPanel { AutoSize = true, ColumnCount = 1, Anchor = AnchorStyles.Top | AnchorStyles.Right, Margin = Padding.Empty };
        profiles.ColumnStyles.Add(new(SizeType.Percent, 100));
        _profileBar.WrapContents = false; _profileBar.Padding = Padding.Empty; _profileBar.Margin = Padding.Empty;
        AddProfileButton(L.T("打开方案"), OpenProfile); AddProfileButton(L.T("保存方案"), SaveProfile); AddProfileButton(L.T("导入旧配置…"), ImportEnv);
        _origin.AutoSize = false; _origin.AutoEllipsis = true; _origin.Dock = DockStyle.Fill;
        _origin.Height = 24; _origin.TextAlign = ContentAlignment.MiddleRight; _origin.Margin = new Padding(0, 3, 6, 0);
        _profileBar.Controls.Add(_language);
        profiles.Controls.Add(_profileBar); profiles.Controls.Add(_origin);
        header.Controls.Add(branding, 0, 0); header.Controls.Add(profiles, 1, 0); root.Controls.Add(header, 0, 0);
        var split = new SplitContainer { Dock = DockStyle.Fill, Orientation = Orientation.Horizontal, Size = new Size(1050, 640), SplitterWidth = 8, SplitterDistance = 355, Panel1MinSize = 180, Panel2MinSize = 190 };
        BuildEditors(); _tabs.Padding = new Point(18, 8); split.Panel1.Controls.Add(_tabs); root.Controls.Add(split, 0, 1);
        var logPanel = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 5, ColumnCount = 1, Padding = new Padding(12, 8, 12, 8), BackColor = Color.White };
        logPanel.ColumnStyles.Add(new(SizeType.Percent, 100));
        for (var i = 0; i < 4; i++) logPanel.RowStyles.Add(new(SizeType.AutoSize));
        logPanel.RowStyles.Add(new(SizeType.Percent, 100));
        var statusHeading = new TableLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, ColumnCount = 3 };
        statusHeading.ColumnStyles.Add(new(SizeType.Percent, 100)); statusHeading.ColumnStyles.Add(new(SizeType.AutoSize)); statusHeading.ColumnStyles.Add(new(SizeType.AutoSize));
        _logCount.Anchor = AnchorStyles.Right; _logCount.Margin = new Padding(12, 0, 12, 0);
        statusHeading.Controls.Add(_status, 0, 0); statusHeading.Controls.Add(_logCount, 1, 0); statusHeading.Controls.Add(_elapsedLabel, 2, 0);
        logPanel.Controls.Add(statusHeading, 0, 0); logPanel.Controls.Add(_activity, 0, 1); logPanel.Controls.Add(_progress, 0, 2);
        logPanel.SizeChanged += (_, _) => _activity.MaximumSize = new Size(Math.Max(200, logPanel.ClientSize.Width - 34), 0);
        var logBar = new TableLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, ColumnCount = 2, Padding = new Padding(0, 3, 0, 5), Margin = Padding.Empty };
        logBar.ColumnStyles.Add(new(SizeType.Percent, 100)); logBar.ColumnStyles.Add(new(SizeType.AutoSize));
        var logFilters = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, WrapContents = false, Margin = Padding.Empty };
        var logActions = new FlowLayoutPanel { AutoSize = true, WrapContents = false, Anchor = AnchorStyles.Top | AnchorStyles.Right, Margin = Padding.Empty };
        _logView.Items.AddRange([L.T("任务摘要"), L.T("详细日志")]); _logView.SelectedIndex = 0;
        _logView.SelectedIndexChanged += (_, _) => { _onlyProblems.Enabled = _logView.SelectedIndex == 0; RenderLog(true, true); };
        _onlyProblems.CheckedChanged += (_, _) => RenderLog(true, true);
        _live.CheckedChanged += (_, _) => { if (_live.Checked) RenderLog(true, true); };
        _tips.SetToolTip(_live, L.T("取消勾选可停留在当前内容，后台仍完整记录。再次勾选同步最新内容。"));
        _tips.SetToolTip(_logView, L.T("摘要展示重要步骤与问题。详细日志显示最近的原始记录；导出保存完整任务日志。"));
        logFilters.Controls.AddRange([_logView, _onlyProblems, _live]);
        var copy = MakeButton(L.T("复制当前内容")); copy.Click += (_, _) => Guard(() => { if (_log.TextLength > 0) Clipboard.SetText(_log.Text); });
        var save = MakeButton(L.T("导出详细日志…")); save.Click += (_, _) => Guard(SaveLog);
        logActions.Controls.Add(copy); logActions.Controls.Add(save);
        logBar.Controls.Add(logFilters, 0, 0); logBar.Controls.Add(logActions, 1, 0);
        logPanel.Controls.Add(logBar, 0, 3); logPanel.Controls.Add(_log, 0, 4); split.Panel2.Controls.Add(logPanel);
        var commands = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true, Padding = new Padding(0, 10, 0, 0), WrapContents = true };
        foreach (var (text, action, help) in new[] {
            (L.T("检查音源"), WorkflowAction.Prepare, L.T("检查曲目信息和解码是否正常，不制作光盘。")),
            (L.T("开始制作"), WorkflowAction.Build, L.T("自动检查音源、编码并生成 ISO 光盘镜像。")),
            (L.T("验证成品"), WorkflowAction.Verify, L.T("验证光盘结构、时间轴、菜单及全部轨道的 PCM 和光盘音频字节。")) })
        {
            var button = MakeCommandButton(text);
            if (action == WorkflowAction.Build) { button.BackColor = Color.FromArgb(22, 111, 116); button.ForeColor = Color.White; }
            button.Click += (_, _) => { if (_active is null) _active = RunAsync(action); };
            _tips.SetToolTip(button, help); _actions.Add(button); commands.Controls.Add(button);
        }
        _cancel.ForeColor = Color.White;
        _cancel.FlatAppearance.MouseOverBackColor = Color.FromArgb(169, 45, 34);
        _cancel.FlatAppearance.MouseDownBackColor = Color.FromArgb(145, 35, 26);
        _cancel.EnabledChanged += (_, _) =>
        {
            _cancel.BackColor = _cancel.Enabled ? Color.FromArgb(192, 57, 43) : Color.FromArgb(246, 221, 218);
            _cancel.FlatAppearance.BorderColor = _cancel.Enabled ? Color.FromArgb(192, 57, 43) : Color.FromArgb(226, 171, 164);
        };
        _cancel.Enabled = false; _cancel.Click += (_, _) => CancelTask(); commands.Controls.Add(_cancel);
        var open = MakeCommandButton(L.T("查看成品")); open.Click += (_, _) => Guard(() =>
        {
            var path = CaptureSettings().ToOptions().FinalDirectory;
            if (!Directory.Exists(path)) throw new DirectoryNotFoundException(L.T("尚未找到成品文件夹，请先完成制作或检查保存位置。"));
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
        });
        commands.Controls.Add(open); root.Controls.Add(commands, 0, 2);
        Populate(settings, origin); Console.SetOut(_output); Console.SetError(_error);
        _timer.Tick += (_, _) => { DrainLog(); if (_running) _elapsedLabel.Text = L.T("已用时 " + FormatDuration(_elapsed.Elapsed)); }; _timer.Start();
        FormClosing += OnClosing;
        FormClosed += (_, _) => { _output.Flush(); _error.Flush(); DrainLog(true); _archive?.Dispose(); _timer.Stop(); _timer.Dispose(); _tips.Dispose(); Console.SetOut(_originalOutput); Console.SetError(_originalError); };
        Shown += (_, _) =>
        {
            _logView.Width = _logView.Items.Cast<string>().Max(item => TextRenderer.MeasureText(item, _logView.Font).Width) + SystemInformation.VerticalScrollBarWidth + 16;
            var commandButtons = commands.Controls.OfType<Button>().ToArray();
            var commandSize = new Size(commandButtons.Max(button => button.PreferredSize.Width), commandButtons.Max(button => button.PreferredSize.Height));
            foreach (var button in commandButtons) button.MinimumSize = commandSize;
            if (split.Height > split.Panel1MinSize + split.Panel2MinSize + split.SplitterWidth)
                split.SplitterDistance = Math.Clamp((int)(split.Height * .55), split.Panel1MinSize, split.Height - split.Panel2MinSize - split.SplitterWidth);
            Post(TaskLogLevel.Information, L.T("先选择音源文件夹和成品保存位置。其他选项可按需调整。"));
            DrainLog(true); if (_smoke) RunSmoke();
        };
    }

    private void BuildEditors()
    {
        foreach (var group in SettingDefinition.All.GroupBy(item => item.Group))
        {
            var page = new TabPage(L.T(group.Key)) { Name = group.Key, AutoScroll = true, BackColor = Color.White };
            var content = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 1, Padding = new Padding(14, 10, 14, 10) };
            content.ColumnStyles.Add(new(SizeType.Percent, 100));
            content.Controls.Add(new Label { Text = group.Key switch {
                "开始设置" => L.T("选择音乐、保存位置和光盘容量，即可开始制作。"),
                "音频编码" => L.T("选择 MLP 无损压缩或 LPCM 非压缩，也可以导入已有 MLP 文件。"),
                "光盘菜单" => L.T("设置播放器中的选曲菜单与专辑封面。"),
                _ => L.T("发布包通常已经配好工具，仅在更换工具或排查问题时调整。") },
                AutoSize = true, ForeColor = Color.DimGray, Margin = new Padding(0, 0, 0, 10) });
            content.Controls.Add(BuildFields(group.Where(d => !d.Advanced)));
            var advanced = BuildFields(group.Where(d => d.Advanced)); advanced.Visible = false;
            var expand = MakeButton(L.T("更多设置  ▾")); expand.ForeColor = Color.FromArgb(22, 111, 116); expand.FlatAppearance.BorderSize = 0;
            expand.Click += (_, _) => { advanced.Visible = !advanced.Visible; expand.Text = advanced.Visible ? L.T("收起更多设置  ▴") : L.T("更多设置  ▾"); };
            content.Controls.Add(expand); content.Controls.Add(advanced); page.Controls.Add(content); _tabs.TabPages.Add(page);
        }
    }
    private Control BuildFields(IEnumerable<SettingDefinition> definitions)
    {
        var table = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 2, Margin = Padding.Empty };
        table.ColumnStyles.Add(new(SizeType.Percent, 50)); table.ColumnStyles.Add(new(SizeType.Percent, 50));
        var index = 0;
        foreach (var definition in definitions)
        {
            var row = index / 2; var column = index % 2;
            if (column == 0) table.RowStyles.Add(new(SizeType.AutoSize));
            var field = new TableLayoutPanel { AutoSize = true, Dock = DockStyle.Top, ColumnCount = 1,
                Margin = new Padding(column == 0 ? 0 : 10, 0, column == 0 ? 10 : 0, 10) };
            field.ColumnStyles.Add(new(SizeType.Percent, 100));
            field.Controls.Add(new Label { Text = L.T(definition.Label), AutoSize = true,
                Font = new Font(Font, FontStyle.Bold), Margin = new Padding(0, 0, 0, 4) });
            Control editor = definition.Kind switch {
                SettingKind.Boolean => new CheckBox { Text = L.T("开启"), AutoSize = true },
                SettingKind.Number => new NumericUpDown { Minimum = definition.Minimum, Maximum = definition.Maximum, ThousandsSeparator = true },
                SettingKind.Choice => new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList },
                SettingKind.Capacity => new CapacityEditor(), _ => new TextBox(),
            };
            if (editor is ComboBox combo) foreach (var value in definition.Choices!) combo.Items.Add(new SettingChoice(value, definition.DisplayValue(value)));
            editor.Name = definition.Key; editor.AccessibleName = L.T(definition.Label); editor.Dock = DockStyle.Top; editor.Margin = Padding.Empty;
            _editors.Add(definition.Key, editor); _tips.SetToolTip(editor, L.T(definition.Help));
            if (definition.Kind is SettingKind.Folder or SettingKind.File)
            {
                var picker = new TableLayoutPanel { AutoSize = true, Dock = DockStyle.Top, ColumnCount = 2, Margin = Padding.Empty };
                picker.ColumnStyles.Add(new(SizeType.Percent, 100)); picker.ColumnStyles.Add(new(SizeType.AutoSize));
                var browse = MakeButton(L.T("浏览…")); browse.Margin = new Padding(7, 0, 0, 0); browse.AccessibleName = L.T("浏览" + definition.Label);
                editor.Margin = new Padding(0, 4, 0, 0);
                browse.Click += (_, _) => Browse(definition, editor); picker.Controls.Add(editor, 0, 0); picker.Controls.Add(browse, 1, 0); field.Controls.Add(picker);
            }
            else field.Controls.Add(editor);
            if (definition.Help.Length > 0)
            {
                var help = new Label { Text = L.T(definition.Help), AutoSize = true, ForeColor = Color.FromArgb(98, 113, 128), Margin = new Padding(0, 4, 0, 0), MaximumSize = new Size(450, 0) };
                field.SizeChanged += (_, _) => help.MaximumSize = new Size(Math.Max(120, field.ClientSize.Width - 4), 0); field.Controls.Add(help);
            }
            table.Controls.Add(field, column, row); index++;
        }
        return table;
    }

    private void Browse(SettingDefinition definition, Control editor) => Guard(() =>
    {
        if (definition.Kind == SettingKind.Folder)
        {
            using var dialog = new FolderBrowserDialog { Description = L.T(definition.Label), UseDescriptionForTitle = true, SelectedPath = editor.Text };
            if (dialog.ShowDialog(this) == DialogResult.OK) editor.Text = dialog.SelectedPath;
        }
        else
        {
            using var dialog = new OpenFileDialog { Title = L.T(definition.Label), Filter = L.T("所有文件|*.*"), CheckFileExists = true };
            if (dialog.ShowDialog(this) == DialogResult.OK) editor.Text = dialog.FileName;
        }
    });

    private void Populate(ProjectSettings settings, string origin)
    {
        _settings = settings.Clone(); _settings.Language = L.Language; _profileOrigin = origin;
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
                    if (definition.Key == "DVDA_MLP_SOURCE") value = value.Trim().ToLowerInvariant() switch { "batch-surcode" => "surcode-batch", "surcode" => "external", var source => source };
                    var choice = combo.Items.Cast<SettingChoice>().FirstOrDefault(item => item.Value == value);
                    if (choice is null) { choice = new(value, definition.DisplayValue(value)); combo.Items.Add(choice); }
                    combo.SelectedItem = choice; break;
                case CapacityEditor capacity: capacity.Value = value; break;
                default: editor.Text = value; break;
            }
        }
        _origin.Text = L.T("当前方案：" + (origin == "新建配置" ? L.T(origin) : Path.GetFileName(origin)));
        _tips.SetToolTip(_origin, L.T((origin == "新建配置" ? L.T(origin) : origin) + "\n开始任务或退出时自动保存当前设置。"));
    }

    private ProjectSettings CaptureSettings()
    {
        var result = _settings.Clone();
        foreach (var pair in _editors)
            result.Values[pair.Key] = pair.Value switch
            {
                CheckBox check => check.Checked ? "on" : "off",
                NumericUpDown number => number.Value.ToString(CultureInfo.InvariantCulture),
                ComboBox combo => (combo.SelectedItem as SettingChoice)?.Value ?? "",
                CapacityEditor capacity => capacity.Value,
                _ => pair.Value.Text.Trim(),
            };
        return result;
    }

    private void AddProfileButton(string text, Action action)
    {
        var button = MakeButton(text);
        button.Click += (_, _) => Guard(action); _profileBar.Controls.Add(button);
    }
    private void ImportEnv()
    {
        using var dialog = new OpenFileDialog { Title = L.T("导入旧配置文件"), Filter = L.T("环境配置|*.env|所有文件|*.*") };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        var settings = ProjectSettings.ImportEnv(dialog.FileName); settings.ApplyBundledToolDefaults(DvdaMaker.Processes.BundledRuntime.Root);
        Populate(settings, dialog.FileName); Post(TaskLogLevel.Success, L.T("已导入旧配置，原文件不会被修改。请确认目录后开始制作。"));
    }
    private void OpenProfile()
    {
        using var dialog = new OpenFileDialog { Title = L.T("打开配置方案"), Filter = L.T("配置方案|*.json") };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        var settings = ProjectSettings.Load(dialog.FileName); settings.ApplyBundledToolDefaults(DvdaMaker.Processes.BundledRuntime.Root);
        Populate(settings, dialog.FileName); Post(TaskLogLevel.Success, L.T("已打开方案：" + Path.GetFileName(dialog.FileName)));
    }
    private void SaveProfile()
    {
        using var dialog = new SaveFileDialog { Title = L.T("保存配置方案"), Filter = L.T("配置方案|*.json"), FileName = "dvd-audio-project.json" };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        _settings = CaptureSettings(); _settings.Save(dialog.FileName); _settings.Save(ProjectSettings.DefaultPath);
        _profileOrigin = dialog.FileName;
        _origin.Text = L.T("已保存：" + Path.GetFileName(dialog.FileName)); Post(TaskLogLevel.Success, L.T("制作方案已保存。"));
    }

    private async Task RunAsync(WorkflowAction action)
    {
        await Task.Yield();
        var title = action switch { WorkflowAction.Prepare => L.T("检查音源"), WorkflowAction.Build => L.T("制作光盘"), _ => L.T("验证成品") };
        try
        {
            if (_closeWhenDone) return;
            ResetLog(); _running = true; SetBusy(true); _elapsed.Restart();
            _status.Text = L.T("正在" + title); _status.ForeColor = Color.FromArgb(22, 111, 116); _progress.Style = ProgressBarStyle.Marquee;
            Post(TaskLogLevel.Information, L.T("开始" + title + "。"));
            var snapshot = CaptureSettings();
            var errors = snapshot.Validate(requireSource: action != WorkflowAction.Verify, requireEncoding: action == WorkflowAction.Build);
            if (errors.Count > 0)
            {
                foreach (var error in errors) Post(TaskLogLevel.Error, error);
                _running = false; SmokeExitCode = 1; _status.Text = L.T("请先完善设置"); _status.ForeColor = Color.Firebrick;
                _activity.Text = L.T("按下方提示调整设置，然后重新点击任务按钮。"); return;
            }
            if (!_smoke) snapshot.Save(ProjectSettings.DefaultPath);
            _settings = snapshot; _cancellation = new();
            var progress = new Progress<WorkflowProgress>(p =>
            {
                if (!_running || _cancellation?.IsCancellationRequested == true) return;
                _status.Text = L.T(p.Title); _activity.Text = L.T(p.Detail); Post(TaskLogLevel.Information, L.T(p.Title) + " · " + L.T(p.Detail));
            });
            var token = _cancellation.Token;
            var outcome = await Task.Run(() => DesktopWorkflow.RunAsync(action, snapshot.ToOptions(), progress, token), token);
            _running = false; _output.Flush(); _error.Flush(); DrainLog(true);
            _status.Text = L.T(outcome.Title) + (_problemCount > 0 ? L.T(" · 有提示待查看") : "");
            _status.ForeColor = _problemCount > 0 ? Color.FromArgb(158, 95, 15) : Color.FromArgb(22, 111, 116);
            _activity.Text = L.T(outcome.Detail); _progress.Style = ProgressBarStyle.Continuous; _progress.Value = 100;
            Post(TaskLogLevel.Success, L.T(outcome.Title) + (L.IsEnglish ? ". " : "。") + L.T(outcome.Detail));
        }
        catch (OperationCanceledException)
        {
            _running = false; _status.Text = L.T("任务已停止"); _status.ForeColor = Color.DimGray;
            _activity.Text = L.T("需要时可以重新开始；程序会按设置复用已有检查和制作结果。");
            Post(TaskLogLevel.Warning, L.T("已按你的要求停止任务。已完成的成品不会被删除。"));
        }
        catch (Exception exception)
        {
            _running = false; SmokeExitCode = 1; _status.Text = L.T(title + "未完成"); _status.ForeColor = Color.Firebrick;
            _activity.Text = L.T(TaskLogPresentation.ErrorAdvice(exception));
            foreach (var line in exception.ToString().Split('\n')) _messages.Enqueue(new(DateTime.Now, line.TrimEnd('\r'), DetailOnly: true));
            Post(TaskLogLevel.Error, TaskLogPresentation.ErrorSummary(exception)); Post(TaskLogLevel.Information, _activity.Text);
        }
        finally
        {
            _running = false; _elapsed.Stop(); _elapsedLabel.Text = L.T("本次用时 " + FormatDuration(_elapsed.Elapsed));
            _progress.Style = ProgressBarStyle.Continuous;
            _output.Flush(); _error.Flush(); DrainLog(true); RenderLog(true, true); FlushArchive();
            _cancellation?.Dispose(); _cancellation = null; _active = null; SetBusy(false);
            if (_smoke)
            {
                var path = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_LOG");
                if (!string.IsNullOrWhiteSpace(path)) File.WriteAllText(path, _log.Text, Encoding.UTF8);
                var raw = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_RAW_OUTPUT");
                if (!string.IsNullOrWhiteSpace(raw) && _archivePath is not null) CopyArchive(raw);
                Snapshot(Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_FINAL_IMAGE"));
            }
            if (_closeWhenDone || _smoke) Close();
        }
    }
    private void ResetLog()
    {
        _output.Flush(); _error.Flush(); DrainLog(true); _archive?.Dispose(); _archive = null; _archivePath = null;
        _rawLines.Clear(); _summaryLines.Clear(); _problemCount = 0; _rendered = 0; _rebuildLog = false; _log.Clear(); _progress.Value = 0;
        var directory = _smoke && Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_DIRECTORY") is { Length: > 0 } smokeDirectory
            ? smokeDirectory : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DVD-Audio-Maker", "logs");
        Directory.CreateDirectory(directory);
        _archivePath = Path.Combine(directory, $"task-{DateTime.Now:yyyyMMdd-HHmmss}-{Guid.NewGuid():N}.log");
        _archive = new StreamWriter(_archivePath, false, new UTF8Encoding(false));
    }
    private void SetBusy(bool busy)
    {
        _tabs.Enabled = !busy; _profileBar.Enabled = !busy;
        foreach (var button in _actions) button.Enabled = !busy;
        _cancel.Enabled = busy;
    }
    private void CancelTask()
    {
        _cancellation?.Cancel(); _cancel.Enabled = false; _status.Text = L.T("正在停止任务…");
        _activity.Text = L.T("等待当前操作结束并释放文件，请稍候。");
    }
    private void OnClosing(object? sender, FormClosingEventArgs e)
    {
        if (_active is not null) { e.Cancel = true; _closeWhenDone = true; CancelTask(); return; }
        _settings = CaptureSettings();
        if (!_smoke) Guard(() => _settings.Save(ProjectSettings.DefaultPath));
    }
    private void Post(TaskLogLevel level, string message) => _messages.Enqueue(new(DateTime.Now, message, new(level, message)));
    private void DrainLog(bool all = false)
    {
        var changed = false;
        for (var i = 0; (all || i < 800) && _messages.TryDequeue(out var line); i++)
        {
            changed = true; _rawLines.Add(line);
            try { _archive?.WriteLine($"[{line.Time:HH:mm:ss}] {L.T(line.Text)}"); }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { ArchiveFailed(e); }
            var summary = line.DetailOnly ? null : line.Summary ?? TaskLogPresentation.Present(line.Text);
            if (summary?.ActivityOnly == true) { if (_running) _activity.Text = L.T(summary.Message); }
            else if (summary is not null)
            {
                _summaryLines.Add(line with { Summary = summary });
                if (summary.Level is TaskLogLevel.Warning or TaskLogLevel.Error) _problemCount++;
            }
        }
        if (_rawLines.Count > 2500) { _rawLines.RemoveRange(0, _rawLines.Count - 2000); _rebuildLog = true; }
        if (_summaryLines.Count > 1200) { _summaryLines.RemoveRange(0, _summaryLines.Count - 1000); _rebuildLog = true; }
        if (!changed) return;
        FlushArchive(); _logCount.Text = _problemCount == 0 ? L.T("暂无问题提示") : L.T($"{_problemCount} 条提醒 / 问题");
        RenderLog(_rebuildLog);
    }
    private void FlushArchive()
    {
        try { _archive?.Flush(); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { ArchiveFailed(e); }
    }
    private void ArchiveFailed(Exception error)
    {
        var archive = _archive; _archive = null; _archivePath = null;
        try { archive?.Dispose(); } catch (IOException) { }
        Post(TaskLogLevel.Warning, L.T("无法继续保存完整日志，窗口仅保留最近记录。请检查磁盘空间或目录权限。" + error.Message));
    }
    private void RenderLog(bool rebuild = false, bool force = false)
    {
        if (!_live.Checked && !force) { _rebuildLog |= rebuild; return; }
        var detailed = _logView.SelectedIndex == 1;
        var entries = (detailed ? _rawLines : _summaryLines.Where(line => !_onlyProblems.Checked || line.Summary!.Level is TaskLogLevel.Warning or TaskLogLevel.Error)).ToArray();
        if (rebuild || _rebuildLog || _rendered > entries.Length)
        {
            _log.Clear(); _rendered = 0; _rebuildLog = false;
            _log.Font = new Font(detailed ? "Consolas" : "Microsoft YaHei UI", 9F);
        }
        foreach (var line in entries.Skip(_rendered))
        {
            var summary = line.Summary; _log.SelectionStart = _log.TextLength;
            _log.SelectionColor = detailed ? ForeColor : summary!.Level switch {
                TaskLogLevel.Success => Color.FromArgb(22, 111, 116), TaskLogLevel.Warning => Color.FromArgb(156, 91, 15), TaskLogLevel.Error => Color.Firebrick, _ => ForeColor,
            };
            var category = summary?.Level switch { TaskLogLevel.Success => L.T("完成"), TaskLogLevel.Warning => L.T("提醒"), TaskLogLevel.Error => L.T("问题"), _ => L.T("进度") };
            _log.AppendText(detailed ? $"{line.Time:HH:mm:ss}  {L.T(line.Text)}{Environment.NewLine}" : $"{line.Time:HH:mm:ss}  {category}  {L.T(summary!.Message)}{Environment.NewLine}{Environment.NewLine}");
        }
        _rendered = entries.Length; _log.SelectionStart = _log.TextLength; _log.ScrollToCaret();
    }
    private void CopyArchive(string destination)
    {
        if (Path.GetFullPath(destination).Equals(Path.GetFullPath(_archivePath!), StringComparison.OrdinalIgnoreCase))
            throw new IOException(L.T("请选择其他文件名，不能覆盖正在记录的日志。"));
        using var source = new FileStream(_archivePath!, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        using var target = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None);
        source.CopyTo(target);
    }
    private void SaveLog()
    {
        _output.Flush(); _error.Flush(); DrainLog(true); FlushArchive();
        using var dialog = new SaveFileDialog { Title = _archivePath is null ? L.T("导出当前缓存日志") : L.T("导出完整任务日志"), Filter = L.T("文本日志|*.txt"), FileName = L.T($"DVD-Audio-日志-{DateTime.Now:yyyyMMdd-HHmmss}.txt") };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        if (_archivePath is not null && File.Exists(_archivePath)) CopyArchive(dialog.FileName);
        else File.WriteAllLines(dialog.FileName, _rawLines.Select(line => $"[{line.Time:HH:mm:ss}] {L.T(line.Text)}"), Encoding.UTF8);
        Post(TaskLogLevel.Success, L.T("详细日志已导出，可用于排查问题。"));
    }
    private void Guard(Action action)
    {
        try { action(); }
        catch (Exception exception) { MessageBox.Show(this, L.T(exception.Message) + Environment.NewLine + Environment.NewLine + L.T(TaskLogPresentation.ErrorAdvice(exception)), L.T("暂时无法完成此操作"), MessageBoxButtons.OK, MessageBoxIcon.Information); }
    }
    private static Button MakeButton(string text)
    {
        var button = new Button { Text = text, AutoSize = true, FlatStyle = FlatStyle.Flat, BackColor = Color.White, Padding = new Padding(10, 4, 10, 4), Margin = new Padding(3, 3, 6, 3), Cursor = Cursors.Hand };
        button.FlatAppearance.BorderColor = Color.FromArgb(210, 219, 226); return button;
    }
    private static Button MakeCommandButton(string text)
    {
        var button = MakeButton(text);
        button.AutoSizeMode = AutoSizeMode.GrowAndShrink; button.MinimumSize = new Size(104, 40);
        button.Padding = new Padding(14, 6, 14, 6);
        return button;
    }
    private static string FormatDuration(TimeSpan time) => time.TotalHours >= 1 ? L.T($"{(int)time.TotalHours} 小时 {time.Minutes} 分 {time.Seconds} 秒") : L.T($"{(int)time.TotalMinutes} 分 {time.Seconds} 秒");
    private void Snapshot(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)) return;
        File.WriteAllText(path + ".layout.json", System.Text.Json.JsonSerializer.Serialize(new
        { Width, Height, MinimumWidth = MinimumSize.Width, MinimumHeight = MinimumSize.Height, DeviceDpi, AutoSize, State = WindowState.ToString(), MinimumRequested = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_MINIMUM") }));
        using var bitmap = new Bitmap(Width, Height); DrawToBitmap(bitmap, new Rectangle(Point.Empty, Size)); bitmap.Save(path, System.Drawing.Imaging.ImageFormat.Png);
    }
    private void CheckLogControls()
    {
        ResetLog();
        for (var i = 0; i < 3100; i++) _messages.Enqueue(new(DateTime.Now, $"raw-retention-{i}"));
        Post(TaskLogLevel.Warning, "retained-warning"); Post(TaskLogLevel.Error, "retained-error"); DrainLog(true);
        using var reader = new StreamReader(new FileStream(_archivePath!, FileMode.Open, FileAccess.Read, FileShare.ReadWrite));
        var full = reader.ReadToEnd();
        if (!full.Contains("raw-retention-0" + Environment.NewLine) || !full.Contains("raw-retention-3099") || _rawLines.Count > 2500)
            throw new InvalidOperationException(L.T("完整日志保存或窗口缓存限制失效。"));
        _logView.SelectedIndex = 1;
        if (!_log.Text.Contains("raw-retention-3099")) throw new InvalidOperationException(L.T("详细日志未显示最新内容。"));
        _logView.SelectedIndex = 0; _onlyProblems.Checked = true;
        if (!_log.Text.Contains("retained-warning") || !_log.Text.Contains("retained-error") || _log.Text.Contains("raw-retention"))
            throw new InvalidOperationException(L.T("摘要问题筛选失效。"));
        _live.Checked = false; var paused = _log.Text; Post(TaskLogLevel.Warning, "arrived-while-paused"); DrainLog(true);
        if (_log.Text != paused) throw new InvalidOperationException(L.T("暂停查看时内容发生跳动。"));
        _live.Checked = true;
        if (!_log.Text.Contains("arrived-while-paused")) throw new InvalidOperationException(L.T("恢复实时日志时丢失消息。"));
        _onlyProblems.Checked = false;
    }
    private void RunSmoke()
    {
        if (!_smokeReady)
        {
            _smokeReady = true;
            BeginInvoke(new Action(RunSmoke));
            return;
        }
        var switchLanguage = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_SWITCH_LANGUAGE");
        if (switchLanguage is not null && L.Normalize(switchLanguage) != L.Language)
        {
            BeginInvoke(new Action(() => _language.SelectedIndex = L.Normalize(switchLanguage) switch { "en" => 1, "ja" => 2, _ => 0 }));
            return;
        }
        var captured = CaptureSettings();
        if (Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_SETTINGS") is { Length: > 0 } settingsReport)
            captured.Save(settingsReport);
        if (captured.Values.Count < ConfigDefaults.Values.Count || _editors.Count != SettingDefinition.All.Count) throw new InvalidOperationException(L.T("GUI 设置控件未完整构建。"));
        foreach (var definition in SettingDefinition.All.Where(d => d.Kind is SettingKind.Choice or SettingKind.Capacity))
        {
            var expected = _settings.Values.GetValueOrDefault(definition.Key, "");
            if (definition.Key == "DVDA_MLP_SOURCE") expected = expected.Trim().ToLowerInvariant() switch { "batch-surcode" => "surcode-batch", "surcode" => "external", var source => source };
            if (captured.Values[definition.Key] != expected) throw new InvalidOperationException(L.T("选项显示改变了配置值：" + definition.Key));
        }
        var page = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_PAGE");
        foreach (TabPage tab in _tabs.TabPages) if (tab.Name == page || tab.Text == page) _tabs.SelectedTab = tab;
        if (Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_VIEW") == "details") _logView.SelectedIndex = 1;
        if (Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_MINIMUM") == "1")
        {
            WindowState = FormWindowState.Normal;
            Size = MinimumSize;
            PerformLayout();
        }
        if (Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_LOG_SELFTEST") == "1") CheckLogControls();
        Snapshot(Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_IMAGE"));
        if (_actions.Count != 3 || Enum.GetNames<WorkflowAction>().Length != 3 ||
            _editors["DVDA_MLP_SOURCE"] is not ComboBox sources ||
            !sources.Items.Cast<SettingChoice>().Select(choice => choice.Value).SequenceEqual(["surcode-batch", "lpcm", "external"]))
            throw new InvalidOperationException("Unexpected desktop action or legacy encoding choice.");
        var action = Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_ACTION");
        if (!string.IsNullOrWhiteSpace(action) && (!Enum.TryParse<WorkflowAction>(action, true, out var candidate) || !Enum.IsDefined(candidate)))
            throw new ArgumentException("Unsupported desktop smoke action: " + action);
        if (!string.IsNullOrWhiteSpace(action) && Enum.TryParse<WorkflowAction>(action, true, out var parsed))
        {
            _active = RunAsync(parsed);
            if (int.TryParse(Environment.GetEnvironmentVariable("DVDA_GUI_SMOKE_CANCEL_MS"), out var delay))
            {
                var cancelTimer = new System.Windows.Forms.Timer { Interval = Math.Max(delay, 1) };
                cancelTimer.Tick += (_, _) => { cancelTimer.Stop(); cancelTimer.Dispose(); if (_active is not null) CancelTask(); };
                cancelTimer.Start();
            }
        }
        else BeginInvoke(new Action(Close));
    }
}
