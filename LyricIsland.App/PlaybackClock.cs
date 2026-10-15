using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;

namespace LyricIsland;

internal sealed class PlaybackClock(TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _time = timeProvider ?? TimeProvider.System;
    private string _identity = string.Empty;
    private long _anchorTimestamp;
    private double _anchorSeconds;
    private bool _playing;
    private double _lastRaw = double.NaN;
    private bool _manual;
    private DateTimeOffset? _lastSystemUpdate;
    public bool IsLocal { get; private set; }
    public string Status { get; private set; } = "等待播放器";
    public double Position { get; private set; }

    public PlaybackSnapshot Observe(PlaybackSnapshot snapshot, Media.SmtcTimelineDiagnostics? timeline)
    {
        if (snapshot.Track is null)
        {
            _identity = string.Empty; _playing = false; _manual = false; _lastSystemUpdate = null; Position = 0; IsLocal = false; Status = "等待播放器";
            return snapshot;
        }
        var identity = LyricSyncService.BuildStableTrackIdentity(snapshot.Track);
        var raw = snapshot.RawPosition?.TotalSeconds ?? 0;
        var selected = snapshot.Position.TotalSeconds;
        var duration = snapshot.Track.Duration.TotalSeconds;
        if (!double.IsFinite(raw) || raw < 0 || raw >= 86400 || (duration > 0 && raw > duration + 5)) raw = 0;
        var timestampValid = timeline is { IsFallbackSnapshot: false } && timeline.LastUpdatedTimeUtc > DateTimeOffset.UnixEpoch &&
            timeline.LastUpdatedTimeUtc <= _time.GetUtcNow().AddSeconds(5);
        var positionValid = double.IsFinite(selected) && selected >= 0 && selected < 86400 && (duration <= 0 || selected <= duration + 5);
        var hasSystemTimeline = timestampValid && positionValid && (duration > 0 || raw > 0) ||
            timeline is { IsFallbackSnapshot: false } && raw > 0;
        if (!timestampValid && hasSystemTimeline) selected = raw;
        if (!string.Equals(_identity, identity, StringComparison.Ordinal))
        {
            _identity = identity; _manual = false; _lastRaw = double.NaN; _lastSystemUpdate = null;
            Anchor(hasSystemTimeline ? selected : raw, snapshot.IsPlaying);
        }
        if (hasSystemTimeline)
        {
            // A frozen publisher timestamp is not a new sample. Continue from the
            // same monotonic anchor, especially across pause/resume, rather than
            // repeatedly replaying its old paused position or counting paused time.
            var repeated = _lastSystemUpdate == timeline?.LastUpdatedTimeUtc && Math.Abs(raw - _lastRaw) < .05;
            if (!repeated || _manual) Anchor(selected, snapshot.IsPlaying);
            else if (_playing != snapshot.IsPlaying) Anchor(Estimate(), snapshot.IsPlaying);
            _manual = false; Position = Estimate(); IsLocal = false; Status = timeline?.StrategyName == "NeteaseMemory" ? "网易云实时进度" : "系统时间轴";
            _lastSystemUpdate = timeline?.LastUpdatedTimeUtc;
        }
        else
        {
            var estimate = Estimate();
            if (!_manual && raw > 0 && double.IsFinite(raw) && (double.IsNaN(_lastRaw) || Math.Abs(raw - _lastRaw) > .05))
                estimate = raw;
            if (_playing != snapshot.IsPlaying || (!_manual && raw > 0 && Math.Abs(raw - _lastRaw) > .05)) Anchor(estimate, snapshot.IsPlaying);
            Position = Estimate(); IsLocal = true;
            Status = _manual ? "本地计时 · 已校准" : "本地计时 · 播放器未提供有效进度";
        }
        _lastRaw = raw;
        if (duration > 0) Position = Math.Min(Position, duration);
        return snapshot with { Position = TimeSpan.FromSeconds(Math.Max(0, Position)) };
    }
    public void SetPlaying(bool playing)
    {
        Position = Estimate(); Anchor(Position, playing);
    }
    public void Synchronize(double seconds)
    {
        if (string.IsNullOrEmpty(_identity) || !double.IsFinite(seconds) || seconds < 0 || seconds > 86400) return;
        _manual = true; Anchor(seconds, _playing); Position = seconds; IsLocal = true; Status = "本地计时 · 已校准";
    }
    private double Estimate() => _anchorSeconds + (_playing ? _time.GetElapsedTime(_anchorTimestamp).TotalSeconds : 0);
    private void Anchor(double seconds, bool playing)
    {
        _anchorSeconds = double.IsFinite(seconds) ? Math.Max(0, seconds) : 0;
        _anchorTimestamp = _time.GetTimestamp(); _playing = playing;
    }
}
