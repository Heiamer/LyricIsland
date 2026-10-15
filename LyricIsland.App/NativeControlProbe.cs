using System.IO;
using System.Text.Json;
using Windows.Media.Control;

namespace LyricIsland;

internal static class NativeControlProbe
{
    public static async Task RunAsync(string path)
    {
        var manager = await GlobalSystemMediaTransportControlsSessionManager.RequestAsync();
        var session = manager.GetSessions().FirstOrDefault(s => s.SourceAppUserModelId.Equals("cloudmusic.exe", StringComparison.OrdinalIgnoreCase));
        if (session is null) { File.WriteAllText(path, "No native cloudmusic session"); return; }
        var records = new List<object>();
        var initialPlaying = session.GetPlaybackInfo().PlaybackStatus == GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing;
        var events = 0;
        void Changed(GlobalSystemMediaTransportControlsSession sender, object args) => Interlocked.Increment(ref events);
        session.PlaybackInfoChanged += Changed; session.TimelinePropertiesChanged += Changed;
        object Sample(string label, bool? accepted = null)
        {
            var info = session.GetPlaybackInfo(); var timeline = session.GetTimelineProperties();
            return new { label, accepted, captured = DateTimeOffset.UtcNow, status = info.PlaybackStatus.ToString(),
                canPlay = info.Controls.IsPlayEnabled, canPause = info.Controls.IsPauseEnabled, canToggle = info.Controls.IsPlayPauseToggleEnabled,
                canSeek = info.Controls.IsPlaybackPositionEnabled, position = timeline.Position.TotalSeconds, end = timeline.EndTime.TotalSeconds,
                updated = timeline.LastUpdatedTime, events = Volatile.Read(ref events) };
        }
        try
        {
            records.Add(Sample("before"));
            var explicitCommand = initialPlaying ? await session.TryPauseAsync() : await session.TryPlayAsync();
            await Task.Delay(2400); records.Add(Sample("explicit-play-or-pause", explicitCommand));
            var toggled = await session.TryTogglePlayPauseAsync();
            await Task.Delay(2400); records.Add(Sample("native-toggle", toggled));
            // Check the API itself, independent of advertised capability flags.
            var seek = await session.TryChangePlaybackPositionAsync(TimeSpan.FromSeconds(60).Ticks);
            await Task.Delay(1000); records.Add(Sample("seek-request-60-seconds", seek));
        }
        finally
        {
            if ((session.GetPlaybackInfo().PlaybackStatus == GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing) != initialPlaying)
                await session.TryTogglePlayPauseAsync();
            await Task.Delay(400); records.Add(Sample("restored-playback-state"));
            session.PlaybackInfoChanged -= Changed; session.TimelinePropertiesChanged -= Changed;
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            File.WriteAllText(path, JsonSerializer.Serialize(records, new JsonSerializerOptions { WriteIndented = true }));
        }
    }
}
