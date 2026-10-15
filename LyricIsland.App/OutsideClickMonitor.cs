using System.Runtime.InteropServices;

namespace LyricIsland;

internal sealed class OutsideClickMonitor(Action<System.Windows.Point> clicked) : IDisposable
{
    private nint _hook;
    private HookCallback? _installedCallback;
    public void SetEnabled(bool enabled)
    {
        if (!enabled) { Dispose(); return; }
        if (_hook != IntPtr.Zero) return;
        _installedCallback = OnMouse;
        _hook = SetWindowsHookEx(14, _installedCallback, GetModuleHandle(null), 0);
    }
    private nint OnMouse(int code, nint message, nint data)
    {
        if (code >= 0 && (message == 0x201 || message == 0x204 || message == 0x207))
        {
            var info = Marshal.PtrToStructure<MouseHookData>(data);
            clicked(new System.Windows.Point(info.X, info.Y));
        }
        return CallNextHookEx(_hook, code, message, data);
    }
    public void Dispose() { if (_hook != IntPtr.Zero) { UnhookWindowsHookEx(_hook); _hook = IntPtr.Zero; } }
    private delegate nint HookCallback(int code, nint message, nint data);
    [StructLayout(LayoutKind.Sequential)] private struct MouseHookData { public int X, Y; public uint MouseData, Flags, Time; public nuint ExtraInfo; }
    [DllImport("user32.dll")] private static extern nint SetWindowsHookEx(int type, HookCallback callback, nint module, uint thread);
    [DllImport("user32.dll")] private static extern bool UnhookWindowsHookEx(nint hook);
    [DllImport("user32.dll")] private static extern nint CallNextHookEx(nint hook, int code, nint message, nint data);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern nint GetModuleHandle(string? name);
}
