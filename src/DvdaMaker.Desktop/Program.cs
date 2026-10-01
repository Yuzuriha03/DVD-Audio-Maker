using DvdaMaker.Configuration;

namespace DvdaMaker.Desktop;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        try
        {
            Application.SetUnhandledExceptionMode(UnhandledExceptionMode.ThrowException);
            ApplicationConfiguration.Initialize();
            string? config = null;
            var smoke = false;
            for (var i = 0; i < args.Length; i++)
            {
                if (args[i] == "--config" && i + 1 < args.Length) config = args[++i];
                else if (args[i] == "--smoke-test") smoke = true;
                else throw new ArgumentException("用法：DVD-Audio-Maker [--config 配置.env或.json]");
            }
            var settings = ProjectSettings.Defaults();
            var origin = "新建配置";
            var path = config ?? (File.Exists(ProjectSettings.DefaultPath) ? ProjectSettings.DefaultPath :
                new ConfigLoader(new Dictionary<string, string?>()).FindConfigPath(null));
            if (path is not null)
            {
                settings = Path.GetExtension(path).Equals(".json", StringComparison.OrdinalIgnoreCase)
                    ? ProjectSettings.Load(path) : ProjectSettings.ImportEnv(path);
                origin = path;
            }
            settings.ApplyBundledToolDefaults(AppContext.BaseDirectory);
            using var form = new MainForm(settings, origin, smoke);
            Application.Run(form);
            return form.SmokeExitCode;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception.ToString());
            var detail = "";
            try
            {
                var directory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DVD-Audio-Maker", "logs");
                Directory.CreateDirectory(directory);
                var path = Path.Combine(directory, $"startup-{DateTime.Now:yyyyMMdd-HHmmss}.log");
                File.WriteAllText(path, exception.ToString());
                detail = "\n\n详细错误记录：" + path;
            }
            catch (Exception) { /* Preserve the original startup failure if logging is unavailable. */ }
            if (!args.Contains("--smoke-test"))
                MessageBox.Show("程序暂时无法打开，请查看下方原因。\n\n" + exception.Message + detail,
                    "无法打开 DVD-Audio Maker", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return 1;
        }
    }
}
