using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class TrackArtworkTests
{
    private static PlaybackSnapshot Snapshot(string id) => new(true, TimeSpan.Zero,
        new TrackInfo(id, "Song " + id, "Artist", "", "Netease", TimeSpan.FromSeconds(200), SongId: id));
    [Fact]
    public void LatePreviousSongArtworkCannotOverwriteNextSong()
    {
        var first = new TaskCompletionSource<byte[]?>(); var next = new TaskCompletionSource<byte[]?>();
        using var artwork = new TrackArtworkResolver((uri, _) => uri.AbsolutePath.Contains("first") ? first.Task : next.Task);
        Assert.Null(artwork.Enrich(Snapshot("1"), "https://p1.music.126.net/first.jpg").CoverImageBytes);
        var pending = artwork.Enrich(Snapshot("2"), "https://p1.music.126.net/next.jpg");
        Assert.True(pending.IsCoverLoading); first.SetResult([1, 2, 3]);
        Assert.Null(artwork.Enrich(Snapshot("2"), "https://p1.music.126.net/next.jpg").CoverImageBytes);
        var correct = new byte[] { 4, 5, 6 }; next.SetResult(correct);
        Assert.Same(correct, artwork.Enrich(Snapshot("2"), "https://p1.music.126.net/next.jpg").CoverImageBytes);
    }
    [Fact]
    public void CoverUrlsOnlyUseThePlayerImageCdn()
    {
        Assert.True(TrackArtworkResolver.ValidUrl("http://p2.music.126.net/a.jpg", out var uri));
        Assert.Equal("https", uri.Scheme);
        Assert.False(TrackArtworkResolver.ValidUrl("file:///C:/secret.txt", out _));
        Assert.False(TrackArtworkResolver.ValidUrl("https://p1.music.126.net.example.com/a", out _));
    }
}
