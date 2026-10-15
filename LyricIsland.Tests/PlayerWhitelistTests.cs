using LyricIsland.Media;
using Xunit;

namespace LyricIsland.Tests;

public sealed class PlayerWhitelistTests
{
    [Theory]
    [InlineData("cloudmusic.exe", true)]
    [InlineData("QQMusic.exe", true)]
    [InlineData("Kugou.exe", true)]
    [InlineData("Kuwo.exe", true)]
    [InlineData("Spotify.exe", true)]
    [InlineData("MSEdge", false)]
    [InlineData("chrome.exe", false)]
    [InlineData("firefox.exe", false)]
    [InlineData("SomeVideoPlayer.exe", false)]
    public void MusicDefaultsExcludeBrowsersAndUnlistedApps(string source, bool expected)
    {
        using var provider = new SmtcMusicSessionProvider();
        var settings = new IslandSettings();
        provider.SetRecognitionOrder(settings.AllowedSources, settings.AllowedSources);
        Assert.Equal(expected, provider.CanUseSource(source));
    }
    [Fact]
    public void DisablingPlayerAppliesToKnownAndGenericPaths()
    {
        using var provider = new SmtcMusicSessionProvider();
        provider.SetRecognitionOrder(["QQMusic"], ["QQMusic"]);
        Assert.True(provider.CanUseSource("QQMusic.exe"));
        Assert.False(provider.CanUseSource("cloudmusic.exe"));
        Assert.False(provider.CanUseSource("msedge.exe"));
        provider.SetRecognitionOrder([], []);
        Assert.False(provider.CanUseSource("QQMusic.exe"));
    }
    [Fact]
    public void ExplicitBrowserOrExactCustomSourceIsOptIn()
    {
        using var provider = new SmtcMusicSessionProvider();
        provider.SetRecognitionOrder(["Browser", "CustomMusic.exe"], ["Browser", "CustomMusic.exe"], ["CustomMusic.exe"]);
        Assert.True(provider.CanUseSource("msedge.exe"));
        Assert.True(provider.CanUseSource("CustomMusic.exe"));
        Assert.False(provider.CanUseSource("CustomVideo.exe"));
        Assert.False(provider.CanUseSource("explorer.exe"));
    }
}
