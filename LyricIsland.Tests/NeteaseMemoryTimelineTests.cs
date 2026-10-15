using LyricIsland.Media;
using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class NeteaseMemoryTimelineTests
{
    private static readonly DateTimeOffset Now = DateTimeOffset.UtcNow;
    private static TrackInfo Track(string title = "Song", string? id = null) => new("track", title, "Artist", "", "Netease", TimeSpan.Zero, SongId: id);
    private static NeteaseTimingSample Sample() => new("123", "Song", 43, 240, false, Now);
    [Fact]
    public void RejectsOldSongDuringMetadataTransition()
    {
        Assert.True(NeteaseMemoryTimeline.Matches(Track(), Sample(), Now));
        Assert.False(NeteaseMemoryTimeline.Matches(Track("Next"), Sample(), Now));
        Assert.False(NeteaseMemoryTimeline.Matches(Track(id: "456"), Sample(), Now));
    }
    [Fact]
    public void ExpiredSampleAndInvalidFieldsCannotBecomeRealProgress()
    {
        Assert.False(NeteaseMemoryTimeline.Matches(Track(), Sample(), Now.AddSeconds(1)));
        Assert.False(NeteaseMemoryTimeline.Matches(Track(), Sample() with { Position = double.NaN }, Now));
        Assert.False(NeteaseMemoryTimeline.Matches(Track(), Sample() with { Position = 400 }, Now));
        Assert.False(NeteaseMemoryTimeline.Matches(Track(), Sample() with { Duration = 0 }, Now));
        Assert.False(NeteaseMemoryTimeline.Matches(Track() with { SourceApp = "Spotify" }, Sample(), Now));
    }
}
