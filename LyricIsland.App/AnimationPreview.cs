using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Button = System.Windows.Controls.Button;

namespace LyricIsland;

internal static class AnimationPreview
{
    public static void Run(string directory)
    {
        Directory.CreateDirectory(directory);
        var settings = new IslandSettings { Animation = true };
        var window = new MainWindow(settings, demo: true) { Opacity = 0, Topmost = false };
        try
        {
            window.Show(); window.SetDemo(false);
            var frame = new DispatcherFrame();
            var timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(330) };
            timer.Tick += (_, _) => { timer.Stop(); frame.Continue = false; };
            timer.Start(); Dispatcher.PushFrame(frame);
            var cover = (Button)window.FindName("CoverButton");
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            Capture(window, directory, "opening", Enumerable.Range(0, 34).Select(i => i * 20d).ToArray());
            settings.Animation = false; window.ApplySettings();
            settings.Animation = true; window.ApplySettings();
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            Capture(window, directory, "closing", Enumerable.Range(0, 29).Select(i => i * 20d).ToArray());
        }
        finally { window.Close(); }
    }

    private static void Capture(MainWindow window, string directory, string direction, double[] times)
    {
        var root = (FrameworkElement)window.Content;
        var frames = new List<BitmapSource>();
        foreach (var time in times)
        {
            // Freeze the independent status-bar timeline as well, so two
            // renderers receive identical inputs instead of wall-clock phases.
            for(var i=0;i<3;i++)
            {
                var bar=(System.Windows.Shapes.Rectangle)window.FindName(new[]{"ActivityOne","ActivityTwo","ActivityThree"}[i]);
                bar.BeginAnimation(UIElement.OpacityProperty,null);bar.Opacity=.9;
                var period=320+i*95d;var phase=(330+time)%(period*2)/period;var t=phase<=1?phase:2-phase;
                bar.RenderTransform=new ScaleTransform(1,.3+.7*(1-Math.Cos(Math.PI*t))/2);
            }
            window.RenderTransitionAt(time);
            root.Measure(new System.Windows.Size(window.Width, window.Height));
            root.Arrange(new Rect(0, 0, window.Width, window.Height)); root.UpdateLayout();
            var bitmap = new RenderTargetBitmap((int)Math.Ceiling(window.Width), (int)Math.Ceiling(window.Height), 96, 96, PixelFormats.Pbgra32);
            bitmap.Render(root); frames.Add(bitmap);
            Save(bitmap, Path.Combine(directory, $"{direction}-{time:000}ms.png"));
        }
        const int columns = 3, cellWidth = 470, cellHeight = 350;
        var overview = new DrawingVisual();
        using (var drawing = overview.RenderOpen())
        {
            drawing.DrawRectangle(new SolidColorBrush(System.Windows.Media.Color.FromRgb(242, 244, 246)), null,
                new Rect(0, 0, columns * cellWidth, (int)Math.Ceiling(times.Length / (double)columns) * cellHeight));
            for (var i = 0; i < frames.Count; i++)
            {
                var x = i % columns * cellWidth; var y = i / columns * cellHeight;
                drawing.DrawText(new FormattedText($"{direction}  {times[i]:0} ms", System.Globalization.CultureInfo.InvariantCulture,
                    System.Windows.FlowDirection.LeftToRight, new Typeface(BundledFonts.Body, FontStyles.Normal, FontWeights.Normal, FontStretches.Normal), 14, System.Windows.Media.Brushes.Black, 1), new System.Windows.Point(x + 10, y + 8));
                drawing.DrawImage(frames[i], new Rect(x + 10, y + 30, frames[i].Width, frames[i].Height));
            }
        }
        var sheet = new RenderTargetBitmap(columns * cellWidth, (int)Math.Ceiling(times.Length / (double)columns) * cellHeight, 96, 96, PixelFormats.Pbgra32);
        sheet.Render(overview); Save(sheet, Path.Combine(directory, direction + "-overview.png"));
    }
    private static void Save(BitmapSource image, string path)
    {
        var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(image));
        using var stream = File.Create(path); encoder.Save(stream);
    }
}
