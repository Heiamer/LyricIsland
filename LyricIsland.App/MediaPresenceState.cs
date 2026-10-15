using TaskbarLyrics.Core.Models;

namespace LyricIsland;

internal enum MediaPresenceAction { None, Show, CollapseAndHide }

internal sealed class MediaPresenceState(TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _time = timeProvider ?? TimeProvider.System;
    private bool _initialized;
    private bool _displayWanted;
    private long? _missingSince;
    public bool HasMedia { get; private set; }
    public static bool HasSource(PlaybackSnapshot snapshot) => snapshot.Track is { } track &&
        !string.IsNullOrWhiteSpace(track.Title) && !track.Id.EndsWith("|ProcessFallback", StringComparison.Ordinal);

    public MediaPresenceAction Observe(PlaybackSnapshot snapshot, bool hideWhenPaused = true)
    {
        if (HasSource(snapshot))
        {
            var wanted = snapshot.IsPlaying || !hideWhenPaused;
            var changed = !_initialized || wanted != _displayWanted;
            _initialized = true; HasMedia = true; _displayWanted = wanted; _missingSince = null;
            return changed ? (wanted ? MediaPresenceAction.Show : MediaPresenceAction.CollapseAndHide) : MediaPresenceAction.None;
        }
        if (!_initialized) { _initialized = true; return MediaPresenceAction.CollapseAndHide; }
        if (!HasMedia) return MediaPresenceAction.None;
        _missingSince ??= _time.GetTimestamp();
        if (_time.GetElapsedTime(_missingSince.Value) < TimeSpan.FromMilliseconds(600)) return MediaPresenceAction.None;
        HasMedia = false; _displayWanted = false; _missingSince = null;
        return MediaPresenceAction.CollapseAndHide;
    }
}
