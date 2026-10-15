using System.Windows;
using System.Windows.Controls;
using TaskbarLyrics.Core.Models;
using Xunit;
namespace LyricIsland.Tests;
public sealed class ProgressResetTests
{
    [Fact]
    public void NewTrackAtZeroAndBackwardSeekSnapToTheConfirmedPosition()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = new MainWindow(new IslandSettings { Animation = true }, demo: true);
                var track = new TrackInfo("first", "First", "Artist", "", "Spotify", TimeSpan.FromSeconds(200));
                window.UpdatePlaybackPresentation(new PlaybackSnapshot(true, TimeSpan.FromSeconds(100), track));
                var slider = (Slider)window.FindName("PlaybackProgress"); Assert.Equal(.5, slider.Value);
                window.UpdatePlaybackPresentation(new PlaybackSnapshot(true, TimeSpan.Zero, track with { Id = "second", Title = "Second" }));
                Assert.Equal(0, slider.Value); Assert.False(slider.HasAnimatedProperties);
                window.UpdatePlaybackPresentation(new PlaybackSnapshot(true, TimeSpan.FromSeconds(100), track));
                window.UpdatePlaybackPresentation(new PlaybackSnapshot(true, TimeSpan.Zero, track));
                Assert.Equal(0, slider.Value); Assert.False(slider.HasAnimatedProperties);
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA); thread.Start(); Assert.True(thread.Join(TimeSpan.FromSeconds(10)));
        Assert.Null(failure);
    }
}
