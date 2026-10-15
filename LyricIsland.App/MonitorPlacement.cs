using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using Forms = System.Windows.Forms;

namespace LyricIsland;

internal static class MonitorPlacement
{
    public static (Rect Work, string Name) Get(string? display)
    {
        var screen = Forms.Screen.AllScreens.FirstOrDefault(s => s.DeviceName == display) ?? Forms.Screen.PrimaryScreen!;
        var bounds = screen.Bounds;
        var monitor = MonitorFromPoint(new PointNative(bounds.Left + bounds.Width / 2, bounds.Top + bounds.Height / 2), 2);
        var scale = 1.0;
        if (GetDpiForMonitor(monitor, 0, out var dpiX, out _) == 0) scale = dpiX / 96.0;
        var work = screen.WorkingArea;
        return (new Rect(work.Left / scale, work.Top / scale, work.Width / scale, work.Height / scale), screen.DeviceName);
    }

    public static void Commit(Window window, Rect bounds)
    {
        var handle = new WindowInteropHelper(window).Handle;
        if (handle == IntPtr.Zero) return;
        var scale = GetDpiForWindow(handle) / 96.0;
        if (scale <= 0) scale = 1;
        SetWindowPos(handle, IntPtr.Zero, (int)Math.Round(bounds.Left * scale), (int)Math.Round(bounds.Top * scale),
            Math.Max(1, (int)Math.Round(bounds.Width * scale)), Math.Max(1, (int)Math.Round(bounds.Height * scale)), 0x0014);
    }

    public static void Drag(Window window)
    {
        ReleaseCapture();
        SendMessage(new WindowInteropHelper(window).Handle, 0x00A1, 2, 0);
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly record struct PointNative(int X, int Y);
    [DllImport("user32.dll")] private static extern IntPtr MonitorFromPoint(PointNative point, uint flags);
    [DllImport("shcore.dll")] private static extern int GetDpiForMonitor(IntPtr monitor, int type, out uint dpiX, out uint dpiY);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr handle);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(IntPtr handle, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] private static extern bool ReleaseCapture();
    [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wParam, IntPtr lParam);
}
