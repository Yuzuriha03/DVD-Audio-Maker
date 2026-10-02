using DvdaMaker.FontTool;
using DvdaMaker.Localization;

var arguments = args.ToList();
try { L.SetLanguage(L.TakeLanguage(arguments) ?? Environment.GetEnvironmentVariable("DVDA_LANGUAGE")); }
catch (ArgumentException exception) { Console.Error.WriteLine(exception.Message); return 2; }
args = arguments.ToArray();
L.LocalizeConsole();
return FontToolProgram.Run(args, Console.Out, Console.Error);

internal static class FontToolProgram
{
    public static int Run(string[] args, TextWriter output, TextWriter error)
    {
        if (args.Length == 3 && args[0] == "extract")
        {
            try
            {
                foreach (var result in OpenTypeFontTool.ExtractNotoCjkFaces(args[1], args[2]))
                {
                    output.WriteLine(
                        $"face[{result.SourceFaceIndex}] '{result.FamilyName}' " +
                        $"ps='{result.PostScriptName}' -> {Path.GetFileName(result.OutputPath)} " +
                        $"({new FileInfo(result.OutputPath).Length} bytes)");
                }
                return 0;
            }
            catch (Exception exception) when (
                exception is IOException or InvalidDataException or UnauthorizedAccessException)
            {
                error.WriteLine($"[FAIL] {exception.Message}");
                return 1;
            }
        }

        if (args.Length == 3 && args[0] == "verify")
        {
            try
            {
                var inspection = OpenTypeFontTool.VerifyFace(args[1], args[2]);
                output.WriteLine(
                    $"family='{inspection.FamilyName}' ps='{inspection.PostScriptName}'  " +
                    $"汉字={(inspection.HasHan ? "Y" : "N")} " +
                    $"假名={(inspection.HasKana ? "Y" : "N")} " +
                    $"谚文={(inspection.HasHangul ? "Y" : "N")} " +
                    $"拉丁={(inspection.HasLatin ? "Y" : "N")}");
                return 0;
            }
            catch (Exception exception) when (
                exception is IOException or InvalidDataException or UnauthorizedAccessException)
            {
                error.WriteLine($"[FAIL] {exception.Message}");
                return 1;
            }
        }

        error.WriteLine("用法:");
        error.WriteLine("  dvda-font extract <NotoSansCJK-Regular.ttc> <输出目录>");
        error.WriteLine("  dvda-font verify <单 face OTF/TTF> <期望 family>");
        return 2;
    }
}
