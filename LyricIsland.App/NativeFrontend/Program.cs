using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Forms = System.Windows.Forms;

namespace LyricIsland;

internal static class Program
{
    [DllImport("user32.dll")] private static extern bool SetProcessDpiAwarenessContext(IntPtr context);

    [STAThread]
    public static void Main(string[] args)
    {
        SetProcessDpiAwarenessContext(new IntPtr(-4));
        AnimationCadence.Initialize();
        if(args.Length==2 && args[0]=="--engine-info") {File.WriteAllText(args[1],TaskbarLyrics.Core.NativeBackend.BuildInfo());return;}
        if(args.Length==2 && args[0]=="--network-test") {try{var result=TaskbarLyrics.Core.NativeBackend.Io("networkSmoke");File.WriteAllText(args[1],result.GetRawText());if(!result.GetProperty("success").GetBoolean())Environment.ExitCode=1;}catch(Exception e){File.WriteAllText(args[1],e.ToString());Environment.ExitCode=1;}return;}
        var application = new System.Windows.Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        if(args.Length==2 && args[0]=="--native-reference") {NativeReference.Run(args[1]);return;}
        if(args.Length==1 && args[0]=="--native-benchmark") {NativeReference.Benchmark();return;}
        if (args.Length == 2 && args[0] == "--probe-native-controls")
        {
            var probeHost = new Window { Width = 1, Height = 1, Left = -32000, Top = -32000, ShowInTaskbar = false, ShowActivated = false, Opacity = 0 };
            probeHost.Show();
            application.Dispatcher.BeginInvoke(async () =>
            {
                try { await NativeControlProbe.RunAsync(args[1]); }
                catch (Exception ex) { File.WriteAllText(args[1], ex.ToString()); Environment.ExitCode = 1; }
                finally { probeHost.Close(); application.Shutdown(); }
            });
            application.Run(); return;
        }
        if (args.Length == 2 && args[0] == "--material-demo" && IslandMaterials.IsValid(args[1]))
        {
            var preview = new MainWindow(new IslandSettings { Animation = false, CardMaterial = args[1], AlwaysOnTop = false }, demo: true)
                { Title = "Lyric Island · 材质预览 (" + args[1] + ")", ShowInTaskbar = true };
            preview.Show(); preview.SetDemo(true);
            var closeTimer = new System.Windows.Threading.DispatcherTimer { Interval = TimeSpan.FromSeconds(90) };
            closeTimer.Tick += (_, _) => { closeTimer.Stop(); preview.Close(); };
            closeTimer.Start(); application.Run(preview); return;
        }
        if (args.Length == 2 && args[0] == "--animation-preview")
        {
            AnimationPreview.Run(args[1]);
            return;
        }
        if (args.Length == 2 && args[0] == "--render-preview")
        {
            RenderPreview(args[1]);
            return;
        }
        if (args.Length == 2 && args[0] == "--smoke-test")
        {
            NativeSmoke.Run(args[1]);
            return;
        }
        if (args.Length == 3 && args[0] is "--settings-test" or "--tray-test")
        {
            SettingsProtocol.PipeName = "LyricIsland.Settings.test." + Guid.NewGuid().ToString("N");
            var demo = new MainWindow(new IslandSettings { Animation = false }, demo: true) { Opacity = 0, Topmost = false };
            demo.StartSettingsServer(); demo.Show();
            var probeInfo = new System.Diagnostics.ProcessStartInfo(args[1])
            { UseShellExecute = false, WindowStyle = System.Diagnostics.ProcessWindowStyle.Hidden,
                ArgumentList = { args[0] == "--tray-test" ? "--smoke-tray" : "--smoke-ui", args[2] } };
            probeInfo.Environment["LYRICISLAND_TEST_PIPE"] = SettingsProtocol.PipeName;
            var probe = System.Diagnostics.Process.Start(probeInfo)!;
            probe.EnableRaisingEvents = true;
            probe.Exited += (_, _) => application.Dispatcher.BeginInvoke(() => { Environment.ExitCode = probe.ExitCode; demo.Close(); application.Shutdown(); });
            var timeout = new System.Windows.Threading.DispatcherTimer { Interval = TimeSpan.FromSeconds(45) };
            timeout.Tick += (_, _) => { timeout.Stop(); if (!probe.HasExited) probe.Kill(); Environment.ExitCode = 1; demo.Close(); application.Shutdown(); };
            timeout.Start(); application.Run(); timeout.Stop(); probe.Dispose();
            return;
        }
        if (args.Length == 2 && args[0] == "--presence-test")
        {
            NativePresenceSmoke.Run(args[1]);
            return;
        }
        using var mutex = new Mutex(true, "Local.LyricIsland.Native.v2", out var created);
        if (!created) { if (!args.Contains("--startup")) System.Windows.MessageBox.Show("Lyric Island 已在运行，请查看系统托盘。", "Lyric Island"); return; }
        var settings = SettingsStore.Default.Load();
        // A normal launch repairs enabled registrations after an upgrade or path change.
        // A Run-key startup itself never rewrites the Run key.
        if (!args.Contains("--startup") && (settings.AutoStart || StartupRegistration.IsRegistered))
        {
            try { StartupRegistration.SetEnabled(true, settings); }
            catch (Exception ex) { TaskbarLyrics.Core.Utilities.Log.Warn($"Startup repair: {ex.Message}"); }
        }
        TaskbarLyrics.Core.Utilities.Log.Diagnostic("BUILD", "Version=1.0; Engine=" + TaskbarLyrics.Core.RustLyricEngine.Describe() + "; Backend=Rust; GUI=WPF; Settings=GPUI; HTTP=WinHTTP NO_PROXY");
        var window = new MainWindow(settings, args.Contains("--demo")) { Opacity = args.Contains("--demo") ? 1 : 0 };
        window.Closed += (_, _) => application.Shutdown();
        using var tray = new Forms.NotifyIcon { Text = "Lyric Island · 歌词灵动岛", Visible = true };
        var icon = Path.Combine(AppContext.BaseDirectory, "Assets", "LyricIsland-v2.ico");
        using var drawingIcon = File.Exists(icon) ? new System.Drawing.Icon(icon) : (System.Drawing.Icon)System.Drawing.SystemIcons.Application.Clone();
        tray.Icon = drawingIcon;
        tray.MouseClick += (_, e) =>
        {
            if (e.Button == Forms.MouseButtons.Right) application.Dispatcher.Invoke(() => window.ShowTrayMenu(Forms.Cursor.Position));
        };
        tray.DoubleClick += (_, _) => application.Dispatcher.Invoke(() => { window.ShowFromTray(); window.ShowSettings(); });
        window.Show();
        if (args.Contains("--expanded")) window.ExpandOnLaunch();
        if (args.Contains("--demo")) window.SetDemo(false);
        application.Run();
        mutex.ReleaseMutex();
    }

    private static void RenderPreview(string directory)
    {
        Directory.CreateDirectory(directory);
        foreach (var mode in new[] { "compact", "expanded", "long-lyrics", "unknown-duration" })
        {
            var window = new MainWindow(new IslandSettings { Animation = false }, demo: true);
            window.SetDemo(mode != "compact", mode == "long-lyrics" ? "让每一段旋律与此刻的心情相遇，陪你走过所有漫长的夜晚" : null);
            if (mode == "unknown-duration") window.UpdatePlaybackPresentation(new TaskbarLyrics.Core.Models.PlaybackSnapshot(true, TimeSpan.FromSeconds(83),
                new TaskbarLyrics.Core.Models.TrackInfo("demo", "此刻的光", "Lyric Island", "演示", "Netease", TimeSpan.Zero)), localClock: true);
            if (mode == "long-lyrics") window.UpdatePlaybackPresentation(new TaskbarLyrics.Core.Models.PlaybackSnapshot(false, TimeSpan.FromSeconds(83),
                new TaskbarLyrics.Core.Models.TrackInfo("demo", "一首名字很长很长但不会挤占按钮的歌曲", "一位名字很长的歌手 · 合作歌手", "演示", "演示", TimeSpan.FromSeconds(236))));
            var artwork = new DrawingGroup();
            artwork.Children.Add(new GeometryDrawing(new LinearGradientBrush(Colors.Teal, Colors.Coral, 45), null, new RectangleGeometry(new Rect(0, 0, 48, 24))));
            artwork.Children.Add(new GeometryDrawing(System.Windows.Media.Brushes.Gold, null, new EllipseGeometry(new System.Windows.Point(24, 12), 6, 6)));
            window.CoverImage.Source = window.ExpandedCoverImage.Source = new DrawingImage(artwork);
            var island = window.VisualBounds;
            var size = new System.Windows.Size(island.Width, island.Height);
            var hostSize = new System.Windows.Size(window.Width, window.Height);
            var visual = window.Content as FrameworkElement ?? throw new InvalidOperationException("Missing island content.");
            visual.Measure(hostSize); visual.Arrange(new Rect(hostSize)); visual.UpdateLayout();
            var bitmap = new RenderTargetBitmap((int)Math.Ceiling(hostSize.Width * 2), (int)Math.Ceiling(hostSize.Height * 2), 192, 192, PixelFormats.Pbgra32);
            bitmap.Render(visual);
            var cropped = new CroppedBitmap(bitmap, new Int32Rect((int)Math.Round((island.Left - window.Left) * 2),
                (int)Math.Round((island.Top - window.Top) * 2), (int)Math.Round(size.Width * 2), (int)Math.Round(size.Height * 2)));
            var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(cropped));
            using var file = File.Create(Path.Combine(directory, "LyricIsland-" + mode + ".png"));
            encoder.Save(file); window.Close();
        }
    }
}
