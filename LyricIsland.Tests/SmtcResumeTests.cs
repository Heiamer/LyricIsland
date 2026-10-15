using LyricIsland.Media;
using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class SmtcResumeTests
{
    [Fact]
    public void FrozenTimestampAcrossPauseResumeCannotFreezeProgressOrCountPausedWallTime()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        var track = new TrackInfo("id", "Song", "Artist", "", "Spotify", TimeSpan.FromSeconds(180));
        var snapshot = new PlaybackSnapshot(true, TimeSpan.FromSeconds(60), track, RawPosition: TimeSpan.FromSeconds(60));
        var timeline = new SmtcTimelineDiagnostics(time.GetUtcNow(), "spotify", "Spotify", "Spotify", true,
            TimeSpan.FromSeconds(60), time.GetUtcNow(), TimeSpan.Zero, TimeSpan.FromSeconds(60), TimeSpan.FromSeconds(60), "test", "Song", "Artist", false);
        Assert.Equal(60, clock.Observe(snapshot, timeline).Position.TotalSeconds);
        time.Advance(4); Assert.Equal(64, clock.Observe(snapshot, timeline).Position.TotalSeconds);
        Assert.Equal(64, clock.Observe(snapshot with { IsPlaying = false }, timeline).Position.TotalSeconds);
        time.Advance(30); Assert.Equal(64, clock.Observe(snapshot with { IsPlaying = false }, timeline).Position.TotalSeconds);
        clock.Observe(snapshot, timeline); time.Advance(2);
        Assert.Equal(66, clock.Observe(snapshot, timeline).Position.TotalSeconds);
        var seek = snapshot with { Position = TimeSpan.FromSeconds(20), RawPosition = TimeSpan.FromSeconds(20) };
        Assert.Equal(20, clock.Observe(seek, timeline with { LastUpdatedTimeUtc = time.GetUtcNow() }).Position.TotalSeconds);
    }
    private sealed class FakeTime : TimeProvider
    {
        private long _ticks;
        public override long TimestampFrequency => TimeSpan.TicksPerSecond;
        public override long GetTimestamp() => _ticks;
        public override DateTimeOffset GetUtcNow() => new DateTimeOffset(2026, 10, 13, 0, 0, 0, TimeSpan.Zero).AddTicks(_ticks);
        public void Advance(double seconds) => _ticks += TimeSpan.FromSeconds(seconds).Ticks;
    }
}
