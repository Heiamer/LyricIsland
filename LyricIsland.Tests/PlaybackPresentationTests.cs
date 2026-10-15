using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class PlaybackPresentationTests
{
    private static TrackInfo Track(double duration) => new("id", "Song", "Artist", "Album", "Netease", TimeSpan.FromSeconds(duration));
    [Theory]
    [InlineData(83, 236, "01:23", "03:56", 0.3516949152542373)]
    [InlineData(300, 236, "03:56", "03:56", 1)]
    [InlineData(-10, 236, "00:00", "03:56", 0)]
    [InlineData(83, 0, "01:23", "--:--", 0)]
    [InlineData(3900, 7200, "1:05:00", "2:00:00", 0.5416666666666666)]
    public void PlaybackProgressShowsRealPositionAndDoesNotInventUnknownDuration(double position, double duration, string elapsed, string total, double fraction)
    {
        var model = PlaybackPresentation.From(new PlaybackSnapshot(true, TimeSpan.FromSeconds(position), Track(duration)));
        Assert.Equal(elapsed, model.Elapsed); Assert.Equal(total, model.Total); Assert.Equal(fraction, model.Fraction, 6);
        Assert.Equal(duration > 0, model.HasDuration);
    }
    [Fact]
    public void TrackMetadataPauseAndLocalClockStateArePresented()
    {
        var model = PlaybackPresentation.From(new PlaybackSnapshot(false, TimeSpan.FromSeconds(83), Track(236)), true);
        Assert.Equal("Song", model.Title); Assert.Equal("Artist", model.Artist); Assert.Equal("本地计时 · 已暂停", model.Status);
    }
    [Fact]
    public void MissingTrackResetsMetadataAndProgress()
    {
        var model = PlaybackPresentation.From(new PlaybackSnapshot(false, TimeSpan.FromSeconds(83), null));
        Assert.Equal("暂无播放", model.Title); Assert.Equal("等待播放", model.Status); Assert.Equal("00:00", model.Elapsed);
        Assert.Equal("--:--", model.Total); Assert.Equal(0, model.Fraction); Assert.False(model.HasDuration);
    }
}
