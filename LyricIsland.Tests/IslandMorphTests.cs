using System.Windows;
using System.Windows.Controls;
using Xunit;
namespace LyricIsland.Tests;
public sealed class IslandMorphTests
{
    [Fact]
    public void UpgradeRestoresAnimationAndLaterUserChoicePersists()
    {
        var directory = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "island-motion-" + Guid.NewGuid().ToString("N"));
        System.IO.Directory.CreateDirectory(directory);
        try
        {
            var path = System.IO.Path.Combine(directory, "settings.json");
            System.IO.File.WriteAllText(path, "{\"Animation\":false}");
            var store = new SettingsStore(path); var settings = store.Load();
            Assert.True(settings.Animation); Assert.Equal(1, settings.MotionPreferenceVersion);
            settings.Animation = false; Assert.True(store.Save(settings));
            Assert.False(store.Load().Animation);
        }
        finally { System.IO.Directory.Delete(directory, true); }
    }
    [Fact]
    public void SurfaceKeepsOneCenterAndTopAcrossTheWholeMorph()
    {
        var plan = IslandLayout.Plan(new Rect(0, 0, 1920, 1080), new());
        Assert.Equal(plan.Capsule.Top, plan.Card.Top);
        Assert.Equal(plan.Capsule, IslandLayout.Morph(plan.Capsule, plan.Card, 0, 30).Rect);
        Assert.Equal(plan.Card, IslandLayout.Morph(plan.Capsule, plan.Card, 1, 30).Rect);
        var center = plan.Capsule.X + plan.Capsule.Width / 2;
        for (var p = 0d; p < 1.035; p += .01)
        {
            var shape = IslandLayout.Morph(plan.Capsule, plan.Card, p, 30);
            Assert.Equal(center, shape.Rect.X + shape.Rect.Width / 2, 8);
            Assert.Equal(plan.Capsule.Top, shape.Rect.Top);
            Assert.True(plan.Host.Contains(shape.Rect));
        }
    }
    [Fact]
    public void AppAnimationAndActivityRemainEnabledRegardlessOfSystemMotion()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                var settings = new IslandSettings { Animation = true };
                window = new MainWindow(settings, demo: true); window.SetDemo(false);
                Assert.True(window.SpatialMotionEnabled);
                var bar = (FrameworkElement)window.FindName("ActivityOne");
                Assert.True(((System.Windows.Media.ScaleTransform)bar.RenderTransform).HasAnimatedProperties);
                var cover = (Button)window.FindName("CoverButton"); cover.ApplyTemplate(); cover.Focus();
                var border = (Border)cover.Template.FindName("CoverFocus", cover);
                Assert.Equal(new Thickness(0), border.BorderThickness);
                settings.Animation = false; window.ApplySettings();
                Assert.False(window.SpatialMotionEnabled);
                Assert.False(((System.Windows.Media.ScaleTransform)bar.RenderTransform).HasAnimatedProperties);
                window.SetDemo(true);
                var root = (FrameworkElement)window.Content;
                root.Measure(new System.Windows.Size(window.Width, window.Height));
                root.Arrange(new Rect(0, 0, window.Width, window.Height)); root.UpdateLayout();
                var slider = (Slider)window.FindName("PlaybackProgress"); slider.ApplyTemplate();
                var track = (System.Windows.Controls.Primitives.Track)slider.Template.FindName("PART_Track", slider);
                track.DecreaseRepeatButton.ApplyTemplate();
                var fill = (Border)track.DecreaseRepeatButton.Template.FindName("ProgressFill", track.DecreaseRepeatButton);
                foreach (var fraction in new[] { 0d, .2, .5, .9, 1 })
                {
                    slider.Value = fraction; root.UpdateLayout();
                    var fillEnd = fill.TranslatePoint(new Point(fill.ActualWidth, 0), slider).X;
                    var knobCenter = track.Thumb.TranslatePoint(new Point(track.Thumb.ActualWidth / 2, 0), slider).X;
                    Assert.InRange(Math.Abs(fillEnd - knobCenter), 0, .7);
                    Assert.InRange(Math.Abs(knobCenter - slider.ActualWidth * fraction), 0, .7);
                    if (fraction == 0) Assert.Equal(0, fill.ActualWidth);
                }
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA); thread.Start(); Assert.True(thread.Join(TimeSpan.FromSeconds(10)));
        Assert.Null(failure);
    }
}
