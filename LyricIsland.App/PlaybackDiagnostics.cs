using System.IO;
using System.Text.Json;
using LyricIsland.Media;

namespace LyricIsland;
internal static class PlaybackDiagnostics
{
    public static async Task CaptureTwilight(string path)
    {
        using var media = new SmtcMusicSessionProvider();
        media.SetRecognitionOrder(["TwilightEcho"], ["TwilightEcho"], ["TwilightEcho"]);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        var snapshot = await media.GetCurrentAsync(timeout.Token);
        var recognized = media.CanUseSource(snapshot.SourceAppUserModelId) && snapshot.Track?.SourceApp == "TwilightEcho";
        File.WriteAllText(path, JsonSerializer.Serialize(new { recognized, source = snapshot.SourceAppUserModelId,
            player = snapshot.Track?.SourceApp, playing = snapshot.IsPlaying, canSeek = snapshot.CanSeek,
            position = snapshot.Position.TotalSeconds, duration = snapshot.Track?.Duration.TotalSeconds }));
        if (!recognized) Environment.ExitCode = 1;
    }
    public static async Task CaptureRawSmtc(string path)
    {
        var manager = await Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager.RequestAsync();
        var samples = new List<object>();
        for (var sample = 0; sample < 8; sample++)
        {
            var current = manager.GetCurrentSession()?.SourceAppUserModelId;
            foreach (var session in manager.GetSessions())
            {
                var info = session.GetPlaybackInfo(); var timeline = session.GetTimelineProperties();
                var metadata = await session.TryGetMediaPropertiesAsync();
                samples.Add(new { sample, captured = DateTimeOffset.UtcNow, current = session.SourceAppUserModelId == current,
                    title = metadata.Title, artist = metadata.Artist,
                    source = session.SourceAppUserModelId, status = info.PlaybackStatus.ToString(), canSeek = info.Controls.IsPlaybackPositionEnabled,
                    position = timeline.Position.TotalSeconds, start = timeline.StartTime.TotalSeconds, end = timeline.EndTime.TotalSeconds,
                    minSeek = timeline.MinSeekTime.TotalSeconds, maxSeek = timeline.MaxSeekTime.TotalSeconds, updated = timeline.LastUpdatedTime });
            }
            await Task.Delay(1000);
        }
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        File.WriteAllText(path, JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
    }
    public static async Task CaptureDuration(string path)
    {
        using var media = new SmtcMusicSessionProvider();
        media.SetRecognitionOrder(["QQMusic", "Netease", "Spotify", "Kugou", "Browser"], ["QQMusic", "Netease", "Spotify", "Kugou", "Browser"]);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
        var playback = await media.GetCurrentAsync(timeout.Token);
        var duration = playback.Track is { } track ? await TrackDurationResolver.LookupAsync(track, timeout.Token) : null;
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        File.WriteAllText(path, JsonSerializer.Serialize(new { track = playback.Track, duration }, new JsonSerializerOptions { WriteIndented = true }));
    }
    public static async Task Capture(string path)
    {
        using var media = new SmtcMusicSessionProvider();
        media.SetRecognitionOrder(["QQMusic", "Netease", "Spotify", "Kugou", "Browser"], ["QQMusic", "Netease", "Spotify", "Kugou", "Browser"]);
        var samples = new List<object>();
        var clock = new PlaybackClock();
        for (var i = 0; i < 5; i++)
        {
            var snapshot = await media.GetCurrentAsync();
            var diagnostics = media.GetLastTimelineDiagnostics();
            var corrected = clock.Observe(snapshot, diagnostics);
            samples.Add(new { correctedPosition = corrected.Position.TotalSeconds, localClock = clock.IsLocal, playing = snapshot.IsPlaying, hasTrack = snapshot.Track is not null,
                player = snapshot.Track?.SourceApp, position = snapshot.Position.TotalSeconds,
                raw = snapshot.RawPosition?.TotalSeconds, extrapolated = snapshot.ExtrapolatedPosition?.TotalSeconds,
                strategy = diagnostics?.StrategyName, age = diagnostics?.LastUpdateAge.TotalSeconds,
                updated = diagnostics?.LastUpdatedTimeUtc, fallback = diagnostics?.IsFallbackSnapshot,
                duration = snapshot.Track?.Duration.TotalSeconds });
            await Task.Delay(1000);
        }
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        File.WriteAllText(path, JsonSerializer.Serialize(samples, new JsonSerializerOptions { WriteIndented = true }));
    }
}
