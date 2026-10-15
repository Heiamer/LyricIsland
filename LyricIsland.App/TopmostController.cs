using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;

namespace LyricIsland;

// Restore Z order without moving, resizing or activating the island.
internal sealed class TopmostController : IDisposable
{
    private const uint ForegroundEvent = 3;
    private const uint PreviousWindow = 3;
    private const int ExtendedStyle = -20;
    private const int TopmostStyle = 0x0008;
    private readonly Window _window;
    private readonly Action? _afterRaise;
    private readonly DispatcherTimer _timer;
    private readonly WinEventCallback _callback;
    private nint _hook;
    private bool _enabled;
    private bool _disposed;
    private bool _checkQueued;
    internal bool Monitoring => _timer.IsEnabled;

    public TopmostController(Window window, Action? afterRaise = null)
    {
        _window = window; _afterRaise = afterRaise;
        _callback = OnForegroundChanged; // Keep the unmanaged callback rooted until unhooked.
        _timer = new DispatcherTimer(DispatcherPriority.Background, window.Dispatcher)
            { Interval = TimeSpan.FromMilliseconds(500) };
        _timer.Tick += OnTick;
        window.SourceInitialized += OnSourceInitialized;
        window.IsVisibleChanged += OnVisibilityChanged;
    }

    public void SetEnabled(bool enabled)
    {
        if (_disposed) return;
        _enabled = enabled;
        _window.Topmost = enabled;
        UpdateMonitoring();
    }

    private void OnSourceInitialized(object? sender, EventArgs e) => UpdateMonitoring();
    private void OnVisibilityChanged(object sender, DependencyPropertyChangedEventArgs e) => UpdateMonitoring();
    private void OnTick(object? sender, EventArgs e) => EnsureAboveOtherWindows();

    private void UpdateMonitoring()
    {
        if (_disposed) return;
        if (_enabled && _window.IsVisible && new WindowInteropHelper(_window).Handle != 0)
        {
            if (_hook == 0)
                _hook = SetWinEventHook(ForegroundEvent, ForegroundEvent, 0, _callback, 0, 0, 0x0002);
            _timer.Start();
            EnsureAboveOtherWindows();
        }
        else StopMonitoring();
    }

    private void OnForegroundChanged(nint hook, uint eventType, nint window, int objectId, int childId, uint threadId, uint time)
    {
        if (_disposed || !_enabled || _checkQueued || _window.Dispatcher.HasShutdownStarted) return;
        _checkQueued = true;
        _window.Dispatcher.BeginInvoke(DispatcherPriority.Background, new Action(() =>
        {
            _checkQueued = false;
            EnsureAboveOtherWindows();
        }));
    }

    internal void EnsureAboveOtherWindows()
    {
        if (_disposed || !_enabled || !_window.IsVisible || _window.Opacity <= 0) return;
        var handle = new WindowInteropHelper(_window).Handle;
        if (handle == 0) return;
        var needsRaise = (GetWindowLong(handle, ExtendedStyle) & TopmostStyle) == 0;
        var visited = new HashSet<nint>();
        for (var other = GetWindow(handle, PreviousWindow); other != 0 && visited.Add(other); other = GetWindow(other, PreviousWindow))
        {
            if (!IsWindowVisible(other) || IsIconic(other)) continue;
            needsRaise = true;
            break;
        }
        if (needsRaise)
            SetWindowPos(handle, new nint(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010 | 0x0200);
        _afterRaise?.Invoke();
    }

    private void StopMonitoring()
    {
        _timer.Stop();
        if (_hook != 0) { UnhookWinEvent(_hook); _hook = 0; }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        StopMonitoring();
        _timer.Tick -= OnTick;
        _window.SourceInitialized -= OnSourceInitialized;
        _window.IsVisibleChanged -= OnVisibilityChanged;
    }

    private delegate void WinEventCallback(nint hook, uint eventType, nint window, int objectId, int childId, uint threadId, uint time);
    [DllImport("user32.dll")] private static extern nint SetWinEventHook(uint minimum, uint maximum, nint module, WinEventCallback callback, uint process, uint thread, uint flags);
    [DllImport("user32.dll")] private static extern bool UnhookWinEvent(nint hook);
    [DllImport("user32.dll")] private static extern nint GetWindow(nint handle, uint command);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] private static extern int GetWindowLong(nint handle, int index);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(nint handle);
    [DllImport("user32.dll")] private static extern bool IsIconic(nint handle);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);
}