using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class PlaybackCommandStateTests
{
    private static PlaybackSnapshot Snapshot(bool playing = true) => new(playing, TimeSpan.FromSeconds(12), new TrackInfo("id", "Song", "Artist", "", "Netease", TimeSpan.Zero));
    [Fact]
    public void AcceptedPauseFreezesClockWhileSessionAcknowledgementIsDelayed()
    {
        var time = new FakeTime(); var pending = new PlaybackCommandState(time); var clock = new PlaybackClock(time);
        clock.Observe(Snapshot(), null); time.Advance(4);
        pending.Accept(Snapshot(), false);
        Assert.False(clock.Observe(pending.Apply(Snapshot()), null).IsPlaying);
        time.Advance(1);
        Assert.Equal(4, clock.Observe(pending.Apply(Snapshot()), null).Position.TotalSeconds);
        clock.Observe(pending.Apply(Snapshot(false)), null); time.Advance(10);
        Assert.Equal(4, clock.Observe(pending.Apply(Snapshot(false)), null).Position.TotalSeconds);
        pending.Accept(Snapshot(false), true); clock.Observe(pending.Apply(Snapshot(false)), null);
        time.Advance(.5);
        Assert.Equal(4.5, clock.Observe(pending.Apply(Snapshot()), null).Position.TotalSeconds);
    }
    [Fact]
    public void UnconfirmedCommandExpiresAndChangingSongDropsAcknowledgement()
    {
        var time = new FakeTime(); var state = new PlaybackCommandState(time);
        state.Accept(Snapshot(), false); time.Advance(2.1);
        Assert.True(state.Apply(Snapshot()).IsPlaying);
        state.Accept(Snapshot(), false);
        var next = Snapshot() with { Track = Snapshot().Track! with { Id = "next", Title = "Next" } };
        Assert.True(state.Apply(next).IsPlaying); Assert.True(state.Apply(Snapshot()).IsPlaying);
    }
    [Fact]
    public void FastPlaybackRefreshFreezesAndRestartsClockWithoutWaitingForMetadata()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        clock.Observe(Snapshot(), null); time.Advance(4); clock.SetPlaying(false);
        time.Advance(30); clock.SetPlaying(false); Assert.Equal(4, clock.Position);
        clock.SetPlaying(true); time.Advance(.5); clock.SetPlaying(true); Assert.Equal(4.5, clock.Position);
    }
    private sealed class FakeTime : TimeProvider
    {
        private long _ticks;
        public override long TimestampFrequency => TimeSpan.TicksPerSecond;
        public override long GetTimestamp() => _ticks;
        public void Advance(double seconds) => _ticks += TimeSpan.FromSeconds(seconds).Ticks;
    }
}
