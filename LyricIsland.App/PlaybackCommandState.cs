using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;

namespace LyricIsland;

// An accepted command may precede PlaybackInfoChanged. Keep its acknowledgement
// briefly, then fall back to the actual session instead of guessing indefinitely.
internal sealed class PlaybackCommandState(TimeProvider? timeProvider = null)
{
    private readonly TimeProvider _time = timeProvider ?? TimeProvider.System;
    private string _identity = "";
    private bool? _expected;
    private long _sent;
    public void Accept(PlaybackSnapshot snapshot, bool playing)
    {
        if (snapshot.Track is null) return;
        _identity = LyricSyncService.BuildStableTrackIdentity(snapshot.Track);
        _expected = playing; _sent = _time.GetTimestamp();
    }
    public PlaybackSnapshot Apply(PlaybackSnapshot snapshot)
    {
        if (!_expected.HasValue) return snapshot;
        if (snapshot.Track is null || LyricSyncService.BuildStableTrackIdentity(snapshot.Track) != _identity ||
            _time.GetElapsedTime(_sent) >= TimeSpan.FromSeconds(2) || snapshot.IsPlaying == _expected.Value)
        {
            _expected = null; return snapshot;
        }
        return snapshot with { IsPlaying = _expected.Value };
    }
}
