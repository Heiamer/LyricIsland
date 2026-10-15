using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Threading;
using TaskbarLyrics.Core.Models;

namespace LyricIsland;

internal static class NativePresenceSmoke
{
    public static void Run(string path)
    {
        MainWindow? window = null;
        try
        {
            window = new MainWindow(new IslandSettings { Animation = true }, demo: true) { Opacity = 0, Topmost = false };
            window.Show();
            var source = new PlaybackSnapshot(true, TimeSpan.FromSeconds(10), new TrackInfo("demo", "Song", "Artist", "", "Netease", TimeSpan.FromSeconds(236)));
            var missing = new PlaybackSnapshot(false, TimeSpan.Zero, null);
            window.ObserveMediaPresence(source); Pump(480);
            var appeared = window.IsVisible && Math.Abs(window.VisualBounds.Height - 40) < .1;
            window.ObserveMediaPresence(source with { IsPlaying = false }); Pump(720);
            var pausedHidden = !window.IsVisible;
            window.ObserveMediaPresence(source); Pump(480);
            var resumedCompact = window.IsVisible && Math.Abs(window.VisualBounds.Height - 40) < .1;
            window.ObserveMediaPresence(missing); Pump(650); window.ObserveMediaPresence(missing); Pump(740);
            var hidden = !window.IsVisible && Math.Abs(window.VisualBounds.Height - 40) < .1;
            var hiddenHeight = window.VisualBounds.Height; var visibleAfterHide = window.IsVisible;
            window.ObserveMediaPresence(source); Pump(480);
            var reappeared = window.IsVisible && Math.Abs(window.VisualBounds.Height - 40) < .1;
            window.ObserveMediaPresence(missing); Pump(650); window.ObserveMediaPresence(missing); Pump(100);
            window.ObserveMediaPresence(source); Pump(760);
            var staleHideCanceled = window.IsVisible && Math.Abs(window.VisualBounds.Height - 40) < .1;
            window.ToggleUserVisibility();
            window.ObserveMediaPresence(source with { IsPlaying = false }); window.ObserveMediaPresence(source); Pump(440);
            var manualHideRespected = !window.IsVisible;
            var success = appeared && pausedHidden && resumedCompact && hidden && reappeared && staleHideCanceled && manualHideRespected;
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            File.WriteAllText(path, JsonSerializer.Serialize(new { success, appeared, pausedHidden, resumedCompact, hidden, hiddenHeight, visibleAfterHide, reappeared, staleHideCanceled, manualHideRespected }));
            if (!success) Environment.ExitCode = 1;
        }
        catch (Exception ex) { File.WriteAllText(path, ex.ToString()); Environment.ExitCode = 1; }
        finally { window?.Close(); }
    }
    private static void Pump(int milliseconds)
    {
        var frame = new DispatcherFrame(); var timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(milliseconds) };
        timer.Tick += (_, _) => { timer.Stop(); frame.Continue = false; }; timer.Start(); Dispatcher.PushFrame(frame);
    }
}
