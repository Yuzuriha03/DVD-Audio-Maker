using System.Runtime.InteropServices;

namespace DvdaMaker.SurcodeTool;

internal static class NativeMethods
{
    internal const uint WmSetText = 12;
    internal const uint WmClose = 16;

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    internal static extern nint SendMessage(nint window, uint message, nint wParam, nint lParam);

    [DllImport("user32.dll", EntryPoint = "SendMessageW", CharSet = CharSet.Unicode)]
    internal static extern nint SendMessage(nint window, uint message, nint wParam, string text);

    [DllImport("user32.dll", EntryPoint = "FindWindowExW", CharSet = CharSet.Unicode)]
    internal static extern nint FindWindowEx(
        nint parent,
        nint childAfter,
        string? className,
        string? windowName);
}
