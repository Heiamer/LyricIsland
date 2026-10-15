using System.IO;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;

namespace LyricIsland.Settings;

internal sealed class TrayMenuWindow : Window
{
    private readonly Grid _anchor = new() { Width = 1, Height = 1, HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top };
    private readonly MenuFlyout _menu = new();
    private readonly string? _smokeDirectory;
    private readonly int _pointerX, _pointerY;
    private TrayMenuBounds? _popupBounds;
    private bool _closed;
    private bool _invoking;
    public TrayMenuWindow(int x, int y, string? smokeDirectory = null)
    {
        _smokeDirectory = smokeDirectory;
        if (smokeDirectory is not null)
        {
            var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
            if (GetMonitorInfo(MonitorFromPoint(new NativePoint(0, 0), 2), ref info))
            { x = info.Work.Right - 24; y = info.Work.Bottom - 8; }
        }
        _pointerX = x; _pointerY = y;
        Title = "Lyric Island · 托盘菜单";
        AppWindow.IsShownInSwitchers = false;
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false); presenter.IsResizable = false;
            presenter.IsMaximizable = presenter.IsMinimizable = false; presenter.IsAlwaysOnTop = true;
        }
        AppWindow.Resize(new SizeInt32(1, 1)); AppWindow.Move(new PointInt32(x, y));
        // WinUI can clamp this top-level window to a minimum size. Hide its native
        // surface before activation; MenuFlyout renders in its own popup window.
        var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var styles = GetWindowLongPtr(handle, -20).ToInt64();
        SetWindowLongPtr(handle, -20, new IntPtr((styles | 0x00080000L | 0x00000080L) & ~0x00040000L));
        if (!SetLayeredWindowAttributes(handle, 0, 0, 2)) throw new Win32Exception(Marshal.GetLastWin32Error());
        _menu.Items.Add(new MenuFlyoutItem { Text = "Lyric Island", IsEnabled = false });
        _menu.Items.Add(new MenuFlyoutSeparator());
        Add("展开灵动岛", Symbol.FullScreen, "expand");
        Add("显示 / 隐藏灵动岛", Symbol.View, "toggleVisibility");
        _menu.Items.Add(new MenuFlyoutSeparator());
        Add("设置", Symbol.Setting, "settings");
        Add("退出", Symbol.Cancel, "exit");
        Content = _anchor;
        _menu.Opened += async (_, _) =>
        {
            PositionMenuNearPointer();
            await Task.Delay(20);
            PositionMenuNearPointer();
        };
        _menu.Closed += (_, _) => { if (!_invoking) Finish(); };
        _anchor.Loaded += async (_, _) =>
        {
            var origin = new NativePoint(0, 0); ClientToScreen(handle, ref origin);
            var scale = Math.Max(1, GetDpiForWindow(handle) / 96d);
            _menu.ShowAt(_anchor, new FlyoutShowOptions { Position = new Windows.Foundation.Point((_pointerX - origin.X) / scale, (_pointerY - origin.Y) / scale), Placement = FlyoutPlacementMode.TopEdgeAlignedLeft });
            if (_smokeDirectory is not null) await SmokeAsync();
        };
        Closed += (_, _) => { _closed = true; Application.Current.Exit(); };
    }
    private void PositionMenuNearPointer()
    {
        if (_closed) return;
        var host = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (!GetMonitorInfo(MonitorFromPoint(new NativePoint(_pointerX, _pointerY), 2), ref info)) return;
        var work = new TrayMenuBounds(info.Work.Left, info.Work.Top, info.Work.Right - info.Work.Left, info.Work.Bottom - info.Work.Top);
        var gap = Math.Max(4, (int)Math.Round(4 * GetDpiForWindow(host) / 96d));
        EnumThreadWindows(GetCurrentThreadId(), (window, parameter) =>
        {
            if (window == host || !IsWindowVisible(window) || !GetWindowRect(window, out var rect) ||
                rect.Right - rect.Left <= 80 || rect.Bottom - rect.Top <= 80) return true;
            var target = TrayMenuPlacement.Place(_pointerX, _pointerY, rect.Right - rect.Left, rect.Bottom - rect.Top, work, gap);
            SetWindowPos(window, IntPtr.Zero, target.Left, target.Top, 0, 0, 0x0215);
            if (GetWindowRect(window, out var positioned))
                _popupBounds = new TrayMenuBounds(positioned.Left, positioned.Top, positioned.Right - positioned.Left, positioned.Bottom - positioned.Top);
            return false;
        }, IntPtr.Zero);
    }

    [StructLayout(LayoutKind.Sequential)] private struct NativePoint
    {
        public int X, Y;
        public NativePoint(int x, int y) { X = x; Y = y; }
    }
    [StructLayout(LayoutKind.Sequential)] private struct MonitorInfo { public int Size; public NativeRect Monitor, Work; public uint Flags; }
    [DllImport("user32.dll")] private static extern nint MonitorFromPoint(NativePoint point, uint flags);
    [DllImport("user32.dll", EntryPoint = "GetMonitorInfoW")] private static extern bool GetMonitorInfo(nint monitor, ref MonitorInfo info);
    [DllImport("user32.dll")] private static extern bool ClientToScreen(nint handle, ref NativePoint point);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(nint handle);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);

    private void Add(string text, Symbol icon, string command)
    {
        var item = new MenuFlyoutItem { Text = text, Icon = new SymbolIcon(icon) };
        item.Click += async (_, _) =>
        {
            _invoking = true;
            try { await SettingsProtocol.SendAsync(command); }
            catch (Exception ex) { System.Diagnostics.Debug.WriteLine(ex); }
            finally { Finish(); }
        };
        _menu.Items.Add(item);
    }
    private void Finish() { if (!_closed) { _closed = true; Close(); Application.Current.Exit(); } }
    private async Task SmokeAsync()
    {
        Directory.CreateDirectory(_smokeDirectory!);
        try
        {
            await Task.Delay(300);
            PositionMenuNearPointer();
            var pointerDistance = _popupBounds is { } menuBounds ? TrayMenuPlacement.DistanceToPointer(menuBounds, _pointerX, _pointerY) : double.PositiveInfinity;
            if (pointerDistance > 12) throw new InvalidOperationException("菜单距离鼠标过远：" + pointerDistance);
            var popup = VisualTreeHelper.GetOpenPopupsForXamlRoot(_anchor.XamlRoot).FirstOrDefault(p => p.Child is MenuFlyoutPresenter);
            if (popup?.Child is not MenuFlyoutPresenter presenter) throw new InvalidOperationException("原生 WinUI 菜单未打开");
            if (presenter.ActualWidth <= 0 || presenter.ActualHeight <= 0) throw new InvalidOperationException("WinUI 菜单布局无效");
            var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
            var hostTransparent = GetLayeredWindowAttributes(handle, out _, out var alpha, out var flags) && alpha == 0 && (flags & 2) != 0;
            if (!hostTransparent) throw new InvalidOperationException("托盘菜单宿主未完全透明");
            var popupVisible = false;
            EnumThreadWindows(GetCurrentThreadId(), (window, parameter) =>
            {
                if (window != handle && IsWindowVisible(window) && GetWindowRect(window, out var bounds) && bounds.Right - bounds.Left > 80 && bounds.Bottom - bounds.Top > 80)
                {
                    var layered = GetLayeredWindowAttributes(window, out _, out var popupAlpha, out var popupFlags);
                    popupVisible |= !layered || (popupFlags & 2) == 0 || popupAlpha > 0;
                }
                return true;
            }, IntPtr.Zero);
            if (!popupVisible) throw new InvalidOperationException("菜单弹出窗口不可见");
            var commandsVerified = false;
            if (SettingsProtocol.PipeName.StartsWith("LyricIsland.Settings.test.", StringComparison.Ordinal))
            {
                var expanded = await SettingsProtocol.SendAsync("expand");
                var state = await SettingsProtocol.SendAsync("state");
                commandsVerified = expanded.GetProperty("success").GetBoolean() && state.GetProperty("expanded").GetBoolean();
                if (!commandsVerified) throw new InvalidOperationException("WinUI 菜单展开命令未生效");
            }
            File.WriteAllText(Path.Combine(_smokeDirectory!, "tray-menu.json"), JsonSerializer.Serialize(new
            { success = true, pointerDistance, pointerX = _pointerX, pointerY = _pointerY, popupBounds = _popupBounds, commandsVerified, hostTransparent, popupVisible, framework = "Microsoft.UI.Xaml.Controls.MenuFlyout", presenterWidth = presenter.ActualWidth, presenterHeight = presenter.ActualHeight,
                items = _menu.Items.OfType<MenuFlyoutItem>().Select(i => i.Text).ToArray() }));
        }
        catch (Exception ex) { File.WriteAllText(Path.Combine(_smokeDirectory!, "tray-menu.json"), JsonSerializer.Serialize(new { success = false, error = ex.ToString() })); Environment.ExitCode = 1; }
        finally { _menu.Hide(); Finish(); }
    }
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] private static extern IntPtr GetWindowLongPtr(IntPtr handle, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")] private static extern IntPtr SetWindowLongPtr(IntPtr handle, int index, IntPtr value);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool SetLayeredWindowAttributes(IntPtr handle, uint color, byte alpha, uint flags);
    [DllImport("user32.dll", SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetLayeredWindowAttributes(IntPtr handle, out uint color, out byte alpha, out uint flags);
    [StructLayout(LayoutKind.Sequential)] private struct NativeRect { public int Left, Top, Right, Bottom; }
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool EnumThreadWindows(uint thread, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] private static extern bool GetWindowRect(IntPtr window, out NativeRect bounds);
}
