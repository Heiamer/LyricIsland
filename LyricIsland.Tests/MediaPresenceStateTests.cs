using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class MediaPresenceStateTests
{
    private static readonly PlaybackSnapshot Source = new(true, TimeSpan.Zero, new TrackInfo("id", "Song", "Artist", "", "Netease", TimeSpan.Zero));
    private static readonly PlaybackSnapshot Missing = new(false, TimeSpan.Zero, null);
    [Fact]
    public void PauseHidesButKeepsSourceAndResumeShows()
    {
        var state = new MediaPresenceState();
        Assert.Equal(MediaPresenceAction.Show, state.Observe(Source));
        Assert.Equal(MediaPresenceAction.CollapseAndHide, state.Observe(Source with { IsPlaying = false }));
        Assert.True(state.HasMedia);
        Assert.Equal(MediaPresenceAction.Show, state.Observe(Source));
    }
    [Fact]
    public void InitiallyPausedStaysHiddenAndRepeatedPausesDoNotRestartTransition()
    {
        var state = new MediaPresenceState();
        Assert.Equal(MediaPresenceAction.CollapseAndHide, state.Observe(Source with { IsPlaying = false }));
        Assert.Equal(MediaPresenceAction.None, state.Observe(Source with { IsPlaying = false }));
        Assert.Equal(MediaPresenceAction.Show, state.Observe(Source));
        Assert.Equal(MediaPresenceAction.CollapseAndHide, state.Observe(Source with { IsPlaying = false }));
        Assert.Equal(MediaPresenceAction.Show, state.Observe(Source));
    }
    [Fact]
    public void PauseHidingCanBeTurnedOff()
    {
        var state = new MediaPresenceState();
        Assert.Equal(MediaPresenceAction.Show, state.Observe(Source, false));
        Assert.Equal(MediaPresenceAction.None, state.Observe(Source with { IsPlaying = false }, false));
    }
    [Fact]
    public void BriefSessionGapDoesNotHideButClosedSourceDoes()
    {
        var time = new FakeTime(); var state = new MediaPresenceState(time);
        state.Observe(Source); Assert.Equal(MediaPresenceAction.None, state.Observe(Missing));
        time.Advance(400); Assert.Equal(MediaPresenceAction.None, state.Observe(Source));
        state.Observe(Missing); time.Advance(601);
        Assert.Equal(MediaPresenceAction.CollapseAndHide, state.Observe(Missing));
        Assert.False(state.HasMedia); Assert.Equal(MediaPresenceAction.Show, state.Observe(Source));
    }
    private sealed class FakeTime : TimeProvider
    {
        private long _ticks;
        public override long TimestampFrequency => TimeSpan.TicksPerSecond;
        public override long GetTimestamp() => _ticks;
        public void Advance(int milliseconds) => _ticks += TimeSpan.FromMilliseconds(milliseconds).Ticks;
    }
}
