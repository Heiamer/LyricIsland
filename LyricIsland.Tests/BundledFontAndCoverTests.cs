using System.IO;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class BundledFontAndCoverTests
{
    [Fact]
    public void FullBundledFontLoadsAndNextTrackClearsPreviousCoverWhileLoading()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            try
            {
                var window = new MainWindow(new IslandSettings { Animation = false }, demo: true);
                var typeface = new Typeface(window.FontFamily, System.Windows.FontStyles.Normal, System.Windows.FontWeights.Normal, System.Windows.FontStretches.Normal);
                Assert.True(typeface.TryGetGlyphTypeface(out var glyph)); Assert.Equal("file", glyph.FontUri.Scheme);
                Assert.Equal(Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "Assets", "Fonts", "OPPOSans.ttf")), glyph.FontUri.LocalPath, ignoreCase: true);
                Assert.True(glyph.CharacterToGlyphMap.ContainsKey('歌'));
                Assert.True(glyph.CharacterToGlyphMap.ContainsKey('夢'));
                Assert.True(glyph.CharacterToGlyphMap.ContainsKey('A'));
                var bitmap = BitmapSource.Create(1, 1, 96, 96, PixelFormats.Bgra32, null, new byte[] { 0, 0, 255, 255 }, 4);
                var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap));
                using var stream = new MemoryStream(); encoder.Save(stream);
                var first = new PlaybackSnapshot(true, TimeSpan.Zero, new TrackInfo("a", "A", "Artist", "", "Netease", TimeSpan.Zero, SongId: "1")) { CoverImageBytes = stream.ToArray() };
                window.UpdateCover(first);
                var image = (System.Windows.Controls.Image)window.FindName("CoverImage"); Assert.NotNull(image.Source);
                window.UpdateCover(first with { Track = first.Track! with { Id = "b", Title = "B", SongId = "2" }, CoverImageBytes = null, IsCoverLoading = true });
                Assert.Null(image.Source); Assert.Null(((System.Windows.Controls.Image)window.FindName("ExpandedCoverImage")).Source);
                window.Close();
            }
            catch (Exception ex) { failure = ex; }
        });
        thread.SetApartmentState(ApartmentState.STA); thread.Start(); thread.Join();
        if (failure is not null) throw failure;
    }
}
