using System.IO;
using System.Text.Json;
using System.Windows;
using Button = System.Windows.Controls.Button;

namespace LyricIsland;

internal static class RealPlaybackSmoke
{
    public static async Task RunAsync(string path)
    {
        MainWindow? window = null;
        bool? initialPlaying = null;
        var samples = new List<object>();
        var success = false;
        string? error = null;
        try
        {
            window = new MainWindow(SettingsStore.Default.Load()); window.Show(); window.ExpandOnLaunch();
            for (var i = 0; i < 100 && window.ActualPlayback?.Track is null; i++) await Task.Delay(100);
            var initial = window.ActualPlayback ?? throw new InvalidOperationException("No playback source.");
            if (initial.Track is null) throw new InvalidOperationException("No track.");
            if (initial.Track.SourceApp.Equals("Netease", StringComparison.OrdinalIgnoreCase))
                for (var i = 0; i < 30 && window.PlaybackTimingSource != "网易云实时进度"; i++) await Task.Delay(100);
            initial = window.ActualPlayback!;
            initialPlaying = initial.IsPlaying;
            var id = initial.Track?.Id ?? throw new InvalidOperationException("Track disappeared during startup.");
            samples.Add(Sample(window, "initial"));
            for (var cycle = 0; cycle < 4; cycle++)
            {
                if (window.ActualPlayback?.Track?.Id != id) throw new InvalidOperationException("Track changed during test.");
                var before = window.ActualPlayback!.IsPlaying;
                ((Button)window.FindName("PlayButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                for (var i = 0; i < 45 && window.PlaybackControlBusy; i++) await Task.Delay(100);
                await Task.Delay(250);
                var after = window.ActualPlayback!;
                samples.Add(Sample(window, $"click-{cycle}-ack"));
                if (after.IsPlaying == before) throw new InvalidOperationException("Player did not change playback state.");
                var seconds = window.PlaybackSeconds;
                var progress = window.DisplayedProgress;
                var elapsed = window.DisplayedElapsed;
                await Task.Delay(2000);
                var delta = window.PlaybackSeconds - seconds;
                samples.Add(Sample(window, $"click-{cycle}-after-2s"));
                if (after.IsPlaying && delta < 1.5) throw new InvalidOperationException("Playing clock did not advance.");
                if (!after.IsPlaying && Math.Abs(delta) > .05) throw new InvalidOperationException("Paused clock moved.");
                if (!after.IsPlaying && (Math.Abs(window.DisplayedProgress - progress) > .000001 || window.DisplayedElapsed != elapsed)) throw new InvalidOperationException("Paused progress presentation moved.");
                if (window.PlaybackButtonAction != (after.IsPlaying ? "暂停" : "播放")) throw new InvalidOperationException("Button disagrees with player.");
            }
            success = true;
        }
        catch (Exception ex) { error = ex.ToString(); Environment.ExitCode = 1; }
        finally
        {
            if (window is not null && initialPlaying is { } original && window.ActualPlayback?.IsPlaying != original)
            {
                ((Button)window.FindName("PlayButton")).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                for (var i = 0; i < 45 && window.PlaybackControlBusy; i++) await Task.Delay(100);
            }
            if (window is not null) samples.Add(Sample(window, "restored"));
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            File.WriteAllText(path, JsonSerializer.Serialize(new { success, initialPlaying, error, samples }, new JsonSerializerOptions { WriteIndented = true }));
            window?.Close();
        }
    }
    private static object Sample(MainWindow window, string stage) => new
    {
        stage, captured = DateTimeOffset.UtcNow, playing = window.ActualPlayback?.IsPlaying,
        position = window.PlaybackSeconds, action = window.PlaybackButtonAction,
        title = window.ActualPlayback?.Track?.Title, timeline = window.PlaybackTimingSource,
        displayedElapsed = window.DisplayedElapsed, progress = window.DisplayedProgress
    };
}
