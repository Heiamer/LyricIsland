using System.Text.Json;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Abstractions;
using TaskbarLyrics.Core.Services;
using Xunit;

namespace LyricIsland.Tests;
public sealed class PlaybackClockTests
{
    private static readonly TrackInfo Track = new("id", "Title", "Artist", "Album", "Netease", TimeSpan.Zero);
    private static PlaybackSnapshot Broken(bool playing = true) => new(playing, TimeSpan.FromSeconds(13_000_000_000), Track,
        RawPosition: TimeSpan.Zero, ExtrapolatedPosition: TimeSpan.FromSeconds(13_000_000_000));
    private static Media.SmtcTimelineDiagnostics InvalidTimeline => new(DateTimeOffset.UtcNow, "Netease", "Netease", "Netease", true,
        TimeSpan.Zero, new DateTimeOffset(1601, 1, 1, 0, 0, 0, TimeSpan.Zero), TimeSpan.FromSeconds(13_000_000_000), TimeSpan.Zero, TimeSpan.Zero, "Broken", "Title", "Artist", false);
    [Fact]
    public void Uninitialized1601TimelineUsesAdvancingLocalClockInsteadOfHugePosition()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        Assert.Equal(0, clock.Observe(Broken(), InvalidTimeline).Position.TotalSeconds);
        time.Advance(12); Assert.Equal(12, clock.Observe(Broken(), InvalidTimeline).Position.TotalSeconds);
        time.Advance(7); Assert.Equal(19, clock.Observe(Broken(), InvalidTimeline).Position.TotalSeconds);
        Assert.True(clock.IsLocal);
    }
    [Fact]
    public void LocalClockFreezesOnPauseAndResumesWithoutJumping()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        clock.Observe(Broken(), InvalidTimeline); time.Advance(10);
        Assert.Equal(10, clock.Observe(Broken(false), InvalidTimeline).Position.TotalSeconds);
        time.Advance(20); Assert.Equal(10, clock.Observe(Broken(false), InvalidTimeline).Position.TotalSeconds);
        clock.Observe(Broken(), InvalidTimeline); time.Advance(4);
        Assert.Equal(14, clock.Observe(Broken(), InvalidTimeline).Position.TotalSeconds);
    }
    [Fact]
    public void ManualCalibrationIsAppliedImmediatelyAndNewSongResetsIt()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        clock.Observe(Broken(), InvalidTimeline); clock.Synchronize(83); time.Advance(2);
        Assert.Equal(85, clock.Observe(Broken(), InvalidTimeline).Position.TotalSeconds);
        var next = Broken() with { Track = Track with { Id = "other", Title = "Other" } };
        Assert.Equal(0, clock.Observe(next, InvalidTimeline).Position.TotalSeconds);
    }
    [Fact]
    public void ValidTimelineUsesPlayerPositionIncludingSeek()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        var track = Track with { Duration = TimeSpan.FromMinutes(3) };
        var timeline = InvalidTimeline with { LastUpdatedTimeUtc = time.GetUtcNow() };
        var playback = Broken() with { Track = track, Position = TimeSpan.FromSeconds(60), RawPosition = TimeSpan.FromSeconds(60) };
        Assert.Equal(60, clock.Observe(playback, timeline).Position.TotalSeconds); Assert.False(clock.IsLocal);
        Assert.Equal(20, clock.Observe(playback with { Position = TimeSpan.FromSeconds(20), RawPosition = TimeSpan.FromSeconds(20) }, timeline).Position.TotalSeconds);
    }
    [Fact]
    public void RecoveredSmtcTimelineImmediatelyOverridesPreviousManualFallback()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        clock.Observe(Broken(), InvalidTimeline); clock.Synchronize(83);
        var actual = Broken() with { Track = Track with { Duration = TimeSpan.FromSeconds(236) }, Position = TimeSpan.FromSeconds(120), RawPosition = TimeSpan.FromSeconds(120) };
        var timeline = InvalidTimeline with { LastUpdatedTimeUtc = time.GetUtcNow() };
        Assert.Equal(120, clock.Observe(actual, timeline).Position.TotalSeconds);
        Assert.False(clock.IsLocal); Assert.Equal("系统时间轴", clock.Status);
    }
    [Fact]
    public async Task InvalidPlayerTimelineStillAdvancesThroughActualLyricLines()
    {
        var time = new FakeTime(); var clock = new PlaybackClock(time);
        var resolved = new ResolvedLyrics(new ParsedLyrics(
            [new ParsedLyricLine(TimeSpan.Zero, null, "First"),
             new ParsedLyricLine(TimeSpan.FromSeconds(5), null, "Second"),
             new ParsedLyricLine(TimeSpan.FromSeconds(10), null, "Third")],
            LyricTimingKind.LineTimed, LyricTimingProvenance.ProviderSupplied, LyricPayloadFormat.Lrc),
            new LyricProviderId("Test"), "three-lines", LyricAcquisitionKind.Remote, new Dictionary<string, string>());
        using var sync = new LyricSyncService(new ImmediateCoordinator(resolved), metadataStabilizationDelay: TimeSpan.Zero, timeProvider: time);
        Assert.Equal("First", (await sync.GetDisplayFrameAsync(clock.Observe(Broken(), InvalidTimeline))).CurrentLine);
        time.Advance(6);
        Assert.Equal("Second", (await sync.GetDisplayFrameAsync(clock.Observe(Broken(), InvalidTimeline))).CurrentLine);
        time.Advance(5);
        Assert.Equal("Third", (await sync.GetDisplayFrameAsync(clock.Observe(Broken(), InvalidTimeline))).CurrentLine);
    }
    private sealed class ImmediateCoordinator(ResolvedLyrics resolved) : ILyricResolutionCoordinator
    {
        public Task<ResolvedLyrics?> ResolveAsync(TrackInfo track, CancellationToken cancellationToken = default) => Task.FromResult<ResolvedLyrics?>(resolved);
        public void Dispose() { }
    }
    [Theory]
    [InlineData("{\"key\":\"CompactWidth\",\"value\":280}", true)]
    [InlineData("{\"key\":\"CompactWidth\",\"value\":9999}", false)]
    [InlineData("{\"key\":\"Animation\",\"value\":true}", true)]
    [InlineData("{\"key\":\"Animation\",\"value\":1}", false)]
    [InlineData("{\"key\":\"unknown\",\"value\":true}", false)]
    [InlineData("[]", false)]
    public void SettingsProtocolRejectsInvalidValues(string json, bool accepted)
    {
        using var document = JsonDocument.Parse(json);
        Assert.Equal(accepted, SettingsProtocol.TryReadSetting(document.RootElement, new IslandSettings()));
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
