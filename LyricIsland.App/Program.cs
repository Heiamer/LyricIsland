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
        if (args.Length >= 1 && args[0] == "--engine-info")
        {
            // Reports which lyric engine this build actually uses. Writes to the file given
            // as the second argument, or to stdout when the process was started with a
            // redirected handle (WinExe has no console of its own).
            var probeTrack = new TaskbarLyrics.Core.Models.TrackInfo(
                "engine-info",
                "Midnight City",
                "M83",
                "Hurry Up, We're Dreaming",
                "Spotify",
                TimeSpan.FromSeconds(244));
            var score = TaskbarLyrics.Core.Utilities.LyricMatcher.Score(probeTrack, "Midnight City", "M83", 244);
            var conflict = TaskbarLyrics.Core.Utilities.LyricMatcher.Score(probeTrack, "Midnight City (Live)", "M83", 244);
            var report = string.Join(Environment.NewLine, new[]
            {
                $"app version : {typeof(Program).Assembly.GetName().Version}",
                $"lyric engine: {TaskbarLyrics.Core.RustLyricEngine.Describe()}",
                $"score probe : {score} (expect 100)",
                $"conflict    : {conflict} (expect 0)",
                $"native dll  : {(TaskbarLyrics.Core.RustLyricEngine.IsActive ? "loaded" : "not loaded, C# fallback active")}",
            });
            if (args.Length >= 2)
            {
                File.WriteAllText(args[1], report + Environment.NewLine);
            }
            else
            {
                Console.WriteLine(report);
            }
            return;
        }
        if (args.Length == 2 && args[0] == "--network-test")
        {
            DirectNetworkSmoke.RunAsync(args[1]).GetAwaiter().GetResult();
            return;
        }
        if (args.Length == 2 && args[0] == "--diagnose-favorite")
        {
            Task.Run(async () =>
            {
                using var media = new Media.SmtcMusicSessionProvider(); using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                var snapshot = await media.GetCurrentAsync(timeout.Token);
                FavoriteResult result;
                try { result = snapshot.Track is { } track ? await NeteaseFavorite.InspectAsync(track, timeout.Token) : new FavoriteResult(false, "未找到歌曲"); }
                catch (Exception ex) { result = new(false, ex.Message); }
                File.WriteAllText(args[1], System.Text.Json.JsonSerializer.Serialize(new { snapshot.Track, result }));
            }).GetAwaiter().GetResult(); return;
        }
        if (args.Length == 2 && args[0] == "--diagnose-twilight")
        {
            PlaybackDiagnostics.CaptureTwilight(args[1]).GetAwaiter().GetResult();
            return;
        }
        if (args.Length == 2 && args[0] == "--diagnose-playback")
        {
            PlaybackDiagnostics.Capture(args[1]).GetAwaiter().GetResult();
            return;
        }
        if (args.Length == 2 && args[0] == "--diagnose-duration")
        {
            PlaybackDiagnostics.CaptureDuration(args[1]).GetAwaiter().GetResult();
            return;
        }
        if (args.Length == 2 && args[0] == "--diagnose-smtc")
        {
            PlaybackDiagnostics.CaptureRawSmtc(args[1]).GetAwaiter().GetResult();
            return;
        }
        var application = new System.Windows.Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        if (args.Length == 2 && args[0] == "--cover-test")
        {
            SettingsProtocol.PipeName = "LyricIsland.Settings.test." + Guid.NewGuid().ToString("N");
            TaskbarLyrics.Core.Database.SongSearchMapDbContext.InitializeDatabase();
            application.Dispatcher.BeginInvoke(async () =>
            {
                var coverWindow = new MainWindow(SettingsStore.Default.Load()) { Opacity = 0, Topmost = false };
                bool? initialPlaying = null;
                string? initialTrack = null;
                try
                {
                    coverWindow.Show();
                    for (var i = 0; i < 40 && coverWindow.ActualPlayback?.Track is null; i++) await Task.Delay(100);
                    initialPlaying = coverWindow.ActualPlayback?.IsPlaying; initialTrack = coverWindow.ActualPlayback?.Track?.Id;
                    if (initialPlaying == false)
                    {
                        using var playTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(3));
                        await coverWindow.SetPlaybackForArtworkTest(true, playTimeout.Token);
                    }
                    for (var i = 0; i < 150 && (coverWindow.CoverByteCount == 0 || string.IsNullOrEmpty(coverWindow.ArtworkBinding)); i++) await Task.Delay(100);
                    var track = coverWindow.ActualPlayback?.Track;
                    var success = track?.SongId is { Length: > 0 } id && coverWindow.ArtworkBinding.StartsWith("Netease|" + id + "|", StringComparison.Ordinal) && coverWindow.CoverByteCount > 0;
                    File.WriteAllText(args[1], System.Text.Json.JsonSerializer.Serialize(new { success, songId = track?.SongId, coverWindow.CoverBinding, coverWindow.ArtworkBinding, coverWindow.CoverByteCount, coverWindow.ArtworkDiagnostics }));
                    if (!success) Environment.ExitCode = 1;
                }
                catch (Exception ex) { File.WriteAllText(args[1], ex.ToString()); Environment.ExitCode = 1; }
                finally
                {
                    if (initialPlaying == false && coverWindow.ActualPlayback?.Track?.Id == initialTrack)
                    {
                        try { using var stop = new CancellationTokenSource(TimeSpan.FromSeconds(3)); await coverWindow.SetPlaybackForArtworkTest(false, stop.Token); }
                        catch (OperationCanceledException) { }
                    }
                    coverWindow.Close(); application.Shutdown();
                }
            });
            application.Run(); return;
        }
        if (args.Length == 2 && args[0] == "--playback-test")
        {
            using var testMutex = new Mutex(true, "Local.LyricIsland.Native.v2", out var testCreated);
            if (!testCreated) { File.WriteAllText(args[1], "Close the running island before this interactive test."); Environment.ExitCode = 1; return; }
            SettingsProtocol.PipeName = "LyricIsland.Settings.test." + Guid.NewGuid().ToString("N");
            TaskbarLyrics.Core.Database.SongSearchMapDbContext.InitializeDatabase();
            application.Dispatcher.BeginInvoke(async () =>
            {
                await RealPlaybackSmoke.RunAsync(args[1]);
                application.Shutdown();
            });
            application.Run(); testMutex.ReleaseMutex(); return;
        }
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
        TaskbarLyrics.Core.Database.SongSearchMapDbContext.InitializeDatabase();
        var settings = SettingsStore.Default.Load();
        // A normal launch repairs enabled registrations after an upgrade or path change.
        // A Run-key startup itself never rewrites the Run key.
        if (!args.Contains("--startup") && (settings.AutoStart || StartupRegistration.IsRegistered))
        {
            try { StartupRegistration.SetEnabled(true, () => true); }
            catch (Exception ex) { TaskbarLyrics.Core.Utilities.Log.Warn($"Startup repair: {ex.Message}"); }
        }
        TaskbarLyrics.Core.Utilities.Log.Diagnostic("BUILD", "Version=1.0; Engine=" + TaskbarLyrics.Core.RustLyricEngine.Describe() + "; Cache=Rust UTF-8 / C# fallback");
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
