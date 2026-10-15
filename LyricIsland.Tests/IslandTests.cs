using System.IO;
using System.Windows;
using System.Windows.Controls;
using Xunit;
using TaskbarLyrics.Core.Services;
using LyricIsland.Media;

namespace LyricIsland.Tests;

public sealed class IslandTests
{
    [Fact]
    public void CoverClickHoldsOpenAcrossHoverLeaveAndRepeatedHover()
    {
        var state = new ExpansionState();
        state.HoverEnter(); state.ClickCover();
        Assert.True(state.Expanded); Assert.True(state.HeldOpen);
        for (var i = 0; i < 5; i++) { Assert.False(state.HoverLeave()); state.HoverEnter(); Assert.True(state.Expanded); }
        state.ClickCover(); Assert.False(state.Expanded); Assert.False(state.HeldOpen);
    }
    [Fact]
    public void OnlyHoverExpansionCanAutomaticallyCollapse()
    {
        var state = new ExpansionState(); state.HoverEnter();
        Assert.True(state.HoverLeave()); Assert.False(state.Expanded);
        state.ClickCover(); state.Close(); Assert.False(state.Expanded); Assert.False(state.HeldOpen);
    }
    [Fact]
    public void DefaultCapsuleIsCompactAndDoesNotAutomaticallyExpand()
    {
        var settings = new IslandSettings();
        Assert.False(settings.HoverExpand); Assert.True(settings.Animation);
        var size = IslandLayout.Measure(settings, false);
        Assert.Equal(140, size.Width); Assert.Equal(40, size.Height);
        Assert.True(size.Width * size.Height < 344 * 60 * .55);
        Assert.Equal(new System.Windows.Size(340, 200), IslandLayout.Measure(settings, true));
    }
    [Fact]
    public void ExpansionKeepsTheSameCenterAndTopOnASecondMonitor()
    {
        var work = new Rect(-1920, -200, 1920, 1040); var settings = new IslandSettings();
        var small = IslandLayout.Place(work, IslandLayout.Measure(settings, false), settings);
        var large = IslandLayout.Place(work, IslandLayout.Measure(settings, true), settings);
        Assert.Equal(small.Left + small.Width / 2, large.Left + large.Width / 2);
        Assert.Equal(small.Top, large.Top); Assert.Equal(work.Top, large.Top);
        var plan = IslandLayout.Plan(work, settings);
        Assert.Equal(work.Top, plan.Capsule.Top); Assert.Equal(work.Top, plan.Card.Top);
    }
    [Fact]
    public void FloatingPositionIsClampedWhenScreenShrinks()
    {
        var settings = new IslandSettings { Floating = true, CenterX = 1, TopFraction = 1 };
        var work = new Rect(0, 0, 250, 180);
        var bounds = IslandLayout.Place(work, IslandLayout.Measure(settings, true), settings);
        Assert.True(work.Contains(bounds));
    }
    [Fact]
    public void BadSettingsFallBackAndAtomicWritesRoundTrip()
    {
        var directory = Path.Combine(Path.GetTempPath(), "lyric-island-test-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var path = Path.Combine(directory, "settings.json"); var store = new SettingsStore(path);
            File.WriteAllText(path, "broken"); Assert.Equal(140, store.Load().CompactWidth);
            var settings = new IslandSettings { CompactWidth = double.NaN, FontSize = 200, Floating = true, CenterX = 2, OffsetMilliseconds = -9999 };
            Assert.True(store.Save(settings)); var saved = store.Load();
            Assert.Equal(140, saved.CompactWidth); Assert.Equal(17, saved.FontSize); Assert.True(saved.Floating);
            Assert.Equal(1, saved.CenterX); Assert.Equal(-5000, saved.OffsetMilliseconds); Assert.False(File.Exists(path + ".tmp"));
            Assert.EndsWith("LyricIsland", SettingsStore.DataDirectory);
        }
        finally { Directory.Delete(directory, true); }
    }
    [Fact]
    public void NativeViewDisplaysLyricsInsideCapsuleAndCoverClickKeepsToolbarOpen()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try
            {
                var window = new MainWindow(new IslandSettings { Animation = false }, demo: true);
                window.SetDemo(false);
                var root = Assert.IsAssignableFrom<FrameworkElement>(window.Content);
                window.Content = null;
                root.Measure(new System.Windows.Size(window.Width, window.Height)); root.Arrange(new Rect(0, 0, window.Width, window.Height)); root.UpdateLayout();
                var lyric = Assert.IsType<TextBlock>(window.FindName("CurrentLyric"));
                Assert.Contains("旋律", lyric.Text); Assert.True(lyric.ActualWidth > 60);
                var cover = Assert.IsType<Button>(window.FindName("CoverButton"));
                var coverVisual = Assert.IsType<Grid>(cover.Content);
                var circle = Assert.IsType<System.Windows.Media.RectangleGeometry>(coverVisual.Clip);
                Assert.Equal(4, circle.RadiusX); Assert.Equal(4, circle.RadiusY);
                Assert.Equal(24, cover.Width); Assert.Equal(24, cover.Height);
                var image = Assert.IsType<System.Windows.Controls.Image>(window.FindName("CoverImage"));
                image.Source = new System.Windows.Media.DrawingImage(new System.Windows.Media.GeometryDrawing(
                    System.Windows.Media.Brushes.Red, null, new System.Windows.Media.RectangleGeometry(new Rect(0, 0, 48, 16))));
                root.UpdateLayout();
                var raster = new System.Windows.Media.Imaging.RenderTargetBitmap(24, 24, 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
                raster.Render(coverVisual);
                var pixels = new byte[24 * 24 * 4]; raster.CopyPixels(pixels, 24 * 4, 0);
                Assert.Equal(0, pixels[3]); Assert.Equal(0, pixels[(24 * 24 - 1) * 4 + 3]);
                Assert.Equal(255, pixels[(12 * 24 + 12) * 4 + 3]);
                cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                var toolbar = Assert.IsType<Grid>(window.FindName("Toolbar"));
                Assert.True(toolbar.IsHitTestVisible); Assert.Equal(Visibility.Visible, toolbar.Visibility);
                Assert.Equal(248, window.Height);
                Assert.Equal(24, cover.Width); Assert.Equal(24, cover.Height);
                Assert.Same(circle, coverVisual.Clip);
                root.Measure(new System.Windows.Size(window.Width, window.Height)); root.Arrange(new Rect(0, 0, window.Width, window.Height)); root.UpdateLayout();
                var expandedCover = Assert.IsType<Button>(window.FindName("ExpandedCoverButton"));
                Assert.Equal(80, expandedCover.ActualWidth); Assert.Equal(80, expandedCover.ActualHeight);
                var expandedVisual = Assert.IsType<Grid>(window.FindName("ExpandedCoverVisual"));
                Assert.IsType<System.Windows.Media.RectangleGeometry>(expandedVisual.Clip);
                var expandedLyric = Assert.IsType<TextBlock>(window.FindName("ExpandedLyric"));
                var title = Assert.IsType<TextBlock>(window.FindName("SongTitle"));
                var artist = Assert.IsType<TextBlock>(window.FindName("ArtistName"));
                var progress = Assert.IsType<Slider>(window.FindName("PlaybackProgress"));
                Assert.Equal(lyric.Text, expandedLyric.Text); Assert.Equal("此刻的光", title.Text); Assert.Equal("Lyric Island", artist.Text);
                Assert.Equal(83d / 236, progress.Value, 6);
                var play = Assert.IsType<Button>(window.FindName("PlayButton"));
                var favorite = Assert.IsType<Button>(window.FindName("FavoriteButton"));
                Assert.Equal(24, favorite.Width);
                window.UpdatePlaybackPresentation(new TaskbarLyrics.Core.Models.PlaybackSnapshot(false, TimeSpan.Zero,
                    new TaskbarLyrics.Core.Models.TrackInfo("ncm", "Song", "Artist", "", "Netease", TimeSpan.FromSeconds(180))));
                Assert.True(favorite.IsEnabled);
                var playback = new TaskbarLyrics.Core.Models.PlaybackSnapshot(false, TimeSpan.FromSeconds(60),
                    new TaskbarLyrics.Core.Models.TrackInfo("demo", "Song", "Artist", "", "Spotify", TimeSpan.FromSeconds(180)), CanSeek: true);
                window.UpdatePlaybackPresentation(playback);
                Assert.True(progress.IsEnabled); Assert.Equal(1d / 3, progress.Value, 6); Assert.Equal("播放", play.ToolTip);
                progress.ApplyTemplate();
                Assert.IsType<System.Windows.Controls.Primitives.Track>(progress.Template.FindName("PART_Track", progress));
                var track = (System.Windows.Controls.Primitives.Track)progress.Template.FindName("PART_Track", progress);
                Assert.NotNull(track.Thumb); Assert.True(progress.IsMoveToPointEnabled);
                window.UpdatePlaybackPresentation(playback with { IsPlaying = true, CanSeek = false });
                Assert.False(progress.IsEnabled); Assert.False(favorite.IsEnabled); Assert.Equal("暂停", play.ToolTip);
                Assert.Contains("未开放", (string)progress.ToolTip);
                Assert.True(toolbar.ActualHeight >= play.ActualHeight);
                foreach (var child in new FrameworkElement[] { expandedCover, expandedLyric, title, artist, progress, favorite, toolbar, Assert.IsType<Button>(window.FindName("PlayButton")) })
                {
                    var point = child.TranslatePoint(new System.Windows.Point(0, 0), root);
                    Assert.True(point.X >= 0 && point.Y >= 0);
                    Assert.True(point.X + child.ActualWidth <= window.Width && point.Y + child.ActualHeight <= window.Height);
                }
                cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Assert.False(toolbar.IsHitTestVisible);
                window.Close();
            }
            catch (Exception ex) { failure = ex; }
        });
        thread.SetApartmentState(ApartmentState.STA); thread.Start(); Assert.True(thread.Join(TimeSpan.FromSeconds(10)));
        Assert.Null(failure);
    }
}
