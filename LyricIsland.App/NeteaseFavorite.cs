using System.Diagnostics;
using System.Runtime.InteropServices;
using TaskbarLyrics.Core.Models;

namespace LyricIsland;

internal sealed record FavoriteResult(bool Success, string Message);

internal static class NeteaseFavorite
{
    // SMTC does not define favorites. Use the player's documented local Ctrl+L
    // command, scoped to its own foreground window and the current song identity.
    public static Task<FavoriteResult> ToggleAsync(TrackInfo track, CancellationToken token) => AccessAsync(track, true, token);
    public static Task<FavoriteResult> InspectAsync(TrackInfo track, CancellationToken token) => AccessAsync(track, false, token);
    private static async Task<FavoriteResult> AccessAsync(TrackInfo track, bool invoke, CancellationToken token, ushort commandKey = 0x4C, string action = "喜欢 / 取消喜欢")
    {
        if (!track.SourceApp.Equals("Netease", StringComparison.OrdinalIgnoreCase)) return new(false, "当前播放器暂不支持喜欢操作");
        foreach (var process in Process.GetProcessesByName("cloudmusic"))
        {
            using (process)
            {
                token.ThrowIfCancellationRequested();
                var handle = process.MainWindowHandle;
                if (handle == IntPtr.Zero || !process.MainWindowTitle.Contains(track.Title, StringComparison.OrdinalIgnoreCase)) continue;
                if (!invoke) return new(true, "网易云当前歌曲可通过原生 Ctrl+L 喜欢命令调用");
                if (new[] { 0x10, 0x11, 0x12 }.Any(key => (GetAsyncKeyState(key) & 0x8000) != 0))
                    return new(false, "请松开 Ctrl、Alt、Shift 后重试喜欢操作");
                var previous = GetForegroundWindow(); var minimized = IsIconic(handle);
                try
                {
                    if (minimized) ShowWindow(handle, 9);
                    if (!SetForegroundWindow(handle)) return new(false, "无法连接网易云窗口，请打开后重试");
                    await Task.Delay(90, token);
                    process.Refresh();
                    if (GetForegroundWindow() != handle || !process.MainWindowTitle.Contains(track.Title, StringComparison.OrdinalIgnoreCase))
                        return new(false, "歌曲或窗口已切换，未执行喜欢操作");
                    var keys = new[] { Key(0x11, false), Key(commandKey, false), Key(commandKey, true), Key(0x11, true) };
                    var sent = SendInput((uint)keys.Length, keys, Marshal.SizeOf<NativeInput>());
                    if (sent != keys.Length)
                    {
                        if (sent > 0) { var release = new[] { Key(commandKey, true), Key(0x11, true) }; SendInput(2, release, Marshal.SizeOf<NativeInput>()); }
                        return new(false, "网易云未接受操作");
                    }
                    await Task.Delay(140, CancellationToken.None);
                    return new(true, $"已向网易云提交{action}操作");
                }
                finally
                {
                    if (minimized) ShowWindow(handle, 6);
                    if (previous != IntPtr.Zero && previous != handle) SetForegroundWindow(previous);
                }
            }
        }
        return new(false, "没有找到网易云当前歌曲窗口，请打开网易云后重试");
    }
    private static NativeInput Key(ushort key, bool up) => new() { Type = 1, Key = new KeyboardInput { VirtualKey = key, Flags = up ? 2u : 0u } };
    [StructLayout(LayoutKind.Explicit, Size = 40)] private struct NativeInput { [FieldOffset(0)] public uint Type; [FieldOffset(8)] public KeyboardInput Key; }
    [StructLayout(LayoutKind.Sequential)] private struct KeyboardInput { public ushort VirtualKey; public ushort ScanCode; public uint Flags; public uint Time; public UIntPtr ExtraInfo; }
    [DllImport("user32.dll")] private static extern uint SendInput(uint count, NativeInput[] inputs, int size);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr handle);
    [DllImport("user32.dll")] private static extern bool IsIconic(IntPtr handle);
    [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr handle, int command);
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
}
