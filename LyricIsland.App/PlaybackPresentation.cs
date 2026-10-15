using TaskbarLyrics.Core.Models;

namespace LyricIsland;

internal sealed record PlaybackPresentation(string Title, string Artist, string Status, string Elapsed, string Total, double Fraction, bool HasDuration)
{
    public static PlaybackPresentation From(PlaybackSnapshot playback, bool localClock = false)
    {
        var track = playback.Track;
        var duration = track?.Duration.TotalSeconds ?? 0;
        var known = double.IsFinite(duration) && duration > 0 && duration <= 86400;
        var position = playback.Position.TotalSeconds;
        position = double.IsFinite(position) ? Math.Clamp(position, 0, known ? duration : 86400) : 0;
        if (track is null) position = 0;
        var status = track is null ? "等待播放" : playback.IsPlaying ? "正在播放" : "已暂停";
        if (track is not null && localClock) status = "本地计时 · " + status;
        return new(string.IsNullOrWhiteSpace(track?.Title) ? "暂无播放" : track.Title,
            track is null ? "打开播放器，开始一首音乐" : string.IsNullOrWhiteSpace(track.Artist) ? "未知歌手" : track.Artist,
            status, Format(position), known ? Format(duration) : "--:--", known ? position / duration : 0, known);
    }
    internal static string Format(double seconds)
    {
        var span = TimeSpan.FromSeconds(Math.Floor(seconds));
        return span.TotalHours >= 1 ? $"{(int)span.TotalHours}:{span.Minutes:00}:{span.Seconds:00}" : $"{(int)span.TotalMinutes:00}:{span.Seconds:00}";
    }
}
