using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using Button = System.Windows.Controls.Button;
using System.Windows.Interop;
using System.Windows.Threading;

namespace LyricIsland;

internal static class NativeSmoke
{
    public static void Run(string path)
    {
        MainWindow? window = null;
        try
        {
            var checks = new Dictionary<string, bool>();
            var settings = new IslandSettings { Animation = true, HoverExpand = true, CardMaterial = "Default" };
            window = new MainWindow(settings, demo: true) { Opacity = 0, Topmost = false };
            window.Show(); window.SetDemo(false); Pump(350);
            var spatialMotion = window.SpatialMotionEnabled;
            var materialEffects = window.MaterialEffectsEnabled;
            checks["animationDoesNotFollowWindows"] = spatialMotion;
            var activity = (FrameworkElement)window.FindName("ActivityOne");
            var initialActivity = ((System.Windows.Media.ScaleTransform)activity.RenderTransform).ScaleY;
            Pump(110);
            checks["playingIndicatorMoves"] = ((System.Windows.Media.ScaleTransform)activity.RenderTransform).HasAnimatedProperties &&
                Math.Abs(((System.Windows.Media.ScaleTransform)activity.RenderTransform).ScaleY - initialActivity) > .001;
            var highlight = (TextBlock)window.FindName("HighlightLyric");
            settings.WordHighlight = false; window.ApplySettings();
            checks["lyricProgressDisabled"] = highlight.Opacity == 0;
            settings.WordHighlight = true; window.ApplySettings();
            checks["lyricProgressRestored"] = highlight.Opacity > 0;
            settings.ExpandedTextColor = "#FF6699"; settings.ExpandedComponentColor = "#5577EE"; settings.LyricProgressColor = "#EEDD33"; window.ApplySettings();
            checks["independentColors"] = ((System.Windows.Media.SolidColorBrush)((TextBlock)window.FindName("SongTitle")).Foreground).Color.ToString() == "#FFFF6699" &&
                ((System.Windows.Media.SolidColorBrush)((Button)window.FindName("PlayButton")).Background).Color.ToString() == "#FF5577EE" &&
                ((System.Windows.Media.SolidColorBrush)highlight.Foreground).Color.ToString() == "#FFEEDD33";
            var host = GetBounds(window); var changes = window.HostBoundsChanges;
            var capsule = window.CapsuleBounds; var card = window.CardBounds;
            var workTop = System.Windows.Forms.Screen.FromHandle(new WindowInteropHelper(window).Handle).WorkingArea.Top;
            checks["flushWithTopEdge"] = Math.Abs(window.PointToScreen(new System.Windows.Point(capsule.Left, capsule.Top)).Y - workTop) <= 1;
            checks["geometry"] = capsule.Width == 140 && capsule.Height == 40 && card.Width == 340 && card.Height == 200 && card.Top == capsule.Top;
            var cover = (Button)window.FindName("CoverButton");
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            window.RenderTransitionAt(0);
            checks["startsAsCapsule"] = window.IslandOutline.Bounds == capsule;
            window.RenderTransitionAt(60);
            checks["respondsImmediately"] = window.TransitionFrame.CardReveal > 0 && window.TransitionFrame.ContentOpacity > 0 && window.TransitionFrame.CapsuleScaleY == 1;
            window.RenderTransitionAt(120);
            var firstBlur = BlurRadius(window, "ExpandedView");
            var middleBounds = window.IslandOutline.Bounds;
            checks["capsuleMorphsContinuously"] = middleBounds.Width > capsule.Width && middleBounds.Width < card.Width && middleBounds.Top == capsule.Top && middleBounds.Height > capsule.Height;
            checks["revealsOrCrossFades"] = spatialMotion
                ? window.TransitionFrame.CardReveal is > 0 and < 1 && window.TransitionFrame.ContentOpacity > 0
                : window.TransitionFrame == IslandTransitionFrame.Expanded;
            window.RenderTransitionAt(180);
            checks["approachesCard"] = window.IslandOutline.Bounds.Width > card.Width - 30;
            window.RenderTransitionAt(200);
            checks["contentReadable"] = window.TransitionFrame.ContentOpacity > .9;
            window.RenderTransitionAt(260);
            checks["contentAppearsLater"] = window.TransitionFrame.ContentOpacity > 0;
            checks["contentBlurDecays"] = firstBlur > BlurRadius(window, "ExpandedView");
            Pump(760);
            checks["blurCleared"] = EffectsDetached(window);
            checks["singleExpandedSurface"] = window.IslandOutline.Bounds == card && ((FrameworkElement)window.FindName("CompactLayer")).Visibility == Visibility.Hidden;
            checks["capsuleContentCrossFades"] = ((FrameworkElement)window.FindName("CompactView")).Opacity == 0 && ((FrameworkElement)window.FindName("ExpandedView")).Opacity == 1;
            var root = (FrameworkElement)window.Content;
            root.RaiseEvent(new System.Windows.Input.MouseEventArgs(System.Windows.Input.Mouse.PrimaryDevice, 0) { RoutedEvent = UIElement.MouseLeaveEvent });
            Pump(1300); checks["clickHoldsOpen"] = window.TransitionFrame == IslandTransitionFrame.Expanded;
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); window.RenderTransitionAt(70);
            checks["closingRespondsImmediately"] = window.TransitionFrame.ContentOpacity < 1 && (!spatialMotion || window.TransitionFrame.CardReveal < 1);
            window.RenderTransitionAt(140);
            checks["retractsOrCrossFades"] = spatialMotion
                ? window.TransitionFrame.CardOffsetY < 0 && window.TransitionFrame.CardReveal < 1
                : window.TransitionFrame == IslandTransitionFrame.Collapsed;
            Pump(570);
            checks["returned"] = window.TransitionFrame == IslandTransitionFrame.Collapsed && EffectsDetached(window);
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(175);
            var before = window.TransitionFrame;
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            checks["reversalContinuous"] = before == window.TransitionFrame;
            Pump(40); before = window.TransitionFrame;
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            checks["secondReversalContinuous"] = before == window.TransitionFrame;
            Pump(760);
            checks["interruptionSettles"] = window.TransitionFrame == IslandTransitionFrame.Expanded;
            checks["fixedHost"] = GetBounds(window) == host && window.HostBoundsChanges == changes;
            checks["shadowInsideHost"] = OutsideIsTransparent(window);
            window.HandleOutsideClick(window.PointToScreen(new System.Windows.Point(card.Left + 100, card.Top + 90)));
            checks["insideKeepsOpen"] = window.TransitionFrame == IslandTransitionFrame.Expanded;
            window.HandleOutsideClick(window.PointToScreen(new System.Windows.Point(card.Left - 8, card.Top + 80)));
            Pump(650); checks["outsideClickCloses"] = window.TransitionFrame == IslandTransitionFrame.Collapsed;
            cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent)); Pump(85);
            settings.Animation = false; window.ApplySettings();
            checks["disabledAnimationClearsBlur"] = EffectsDetached(window);
            checks["disabledIndicatorStops"] = !((System.Windows.Media.ScaleTransform)activity.RenderTransform).HasAnimatedProperties;
            cover.ApplyTemplate(); cover.Focus();
            var focusBorder = (Border)cover.Template.FindName("CoverFocus", cover);
            checks["noGreenFocusBorder"] = focusBorder.BorderThickness == new Thickness(0);
            for (var i = 0; i < 24; i++) cover.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
            checks["stressFixedHost"] = GetBounds(window) == host && window.HostBoundsChanges == changes;
            checks["topmost"] = ProbeTopmost().Success;
            checks["singleMaterial"] = window.EffectiveMaterial == "Default" && IslandMaterials.Normalize("obsolete") == "Default";
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            File.WriteAllText(path, JsonSerializer.Serialize(new { success = checks.Values.All(x => x), checks, spatialMotion, materialEffects, renderFrames = window.ResizeFrameCount, host, capsule, card }, new JsonSerializerOptions { WriteIndented = true }));
            if (checks.Values.Any(x => !x)) Environment.ExitCode = 1;
        }
        catch (Exception ex) { File.WriteAllText(path, ex.ToString()); Environment.ExitCode = 1; }
        finally { window?.Close(); }
    }
    private static TopmostProbe ProbeTopmost()
    {
        var foreground = GetForegroundWindow();
        var guarded = new Window { Width = 1, Height = 1, Left = -32000, Top = -32000, Opacity = .01, ShowInTaskbar = false, ShowActivated = false, WindowStyle = WindowStyle.None };
        var competitor = new Window { Width = 1, Height = 1, Left = -32000, Top = -32000, Opacity = .01, ShowInTaskbar = false, ShowActivated = false, WindowStyle = WindowStyle.None, Topmost = true };
        using var controller = new TopmostController(guarded);
        try
        {
            guarded.Show(); controller.SetEnabled(true); competitor.Show();
            var own = new WindowInteropHelper(guarded).Handle;
            var other = new WindowInteropHelper(competitor).Handle;
            SetWindowPos(other, new nint(-1), 0, 0, 0, 0, 0x0013);
            var covered = IsAbove(other, own);
            Pump(650);
            var recovered = covered && IsAbove(own, other) && (GetWindowLong(own, -20) & 8) != 0;
            controller.SetEnabled(false);
            SetWindowPos(other, new nint(-1), 0, 0, 0, 0, 0x0013); Pump(650);
            var disabled = !controller.Monitoring && (GetWindowLong(own, -20) & 8) == 0 && IsAbove(other, own);
            controller.SetEnabled(true); Pump(50);
            var reenabled = controller.Monitoring && IsAbove(own, other);
            guarded.Hide(); Pump(50); var hiddenStopsMonitoring = !controller.Monitoring;
            guarded.Show(); Pump(50); var visibleRestartsMonitoring = controller.Monitoring;
            controller.Dispose(); var disposedStopsMonitoring = !controller.Monitoring;
            return new(recovered, disabled, reenabled, hiddenStopsMonitoring, visibleRestartsMonitoring, disposedStopsMonitoring, GetForegroundWindow() == foreground);
        }
        finally { competitor.Close(); guarded.Close(); }
    }
    private static bool IsAbove(nint higher, nint lower)
    {
        var visited = new HashSet<nint>();
        for (var current = GetWindow(lower, 3); current != 0 && visited.Add(current); current = GetWindow(current, 3))
            if (current == higher) return true;
        return false;
    }
    private sealed record TopmostProbe(bool Recovered, bool Disabled, bool Reenabled, bool HiddenStopsMonitoring, bool VisibleRestartsMonitoring, bool DisposedStopsMonitoring, bool FocusUnchanged)
    {
        public bool Success => Recovered && Disabled && Reenabled && HiddenStopsMonitoring && VisibleRestartsMonitoring && DisposedStopsMonitoring && FocusUnchanged;
    }
    [DllImport("user32.dll")] private static extern nint GetWindow(nint handle, uint command);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] private static extern int GetWindowLong(nint handle, int index);
    [DllImport("user32.dll")] private static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint handle, nint after, int x, int y, int width, int height, uint flags);
    private static double BlurRadius(MainWindow window, string name) =>
        ((FrameworkElement)window.FindName(name)).Effect is System.Windows.Media.Effects.BlurEffect blur ? blur.Radius : 0;
    private static bool EffectsDetached(MainWindow window) =>
        ((FrameworkElement)window.FindName("CompactView")).Effect is null &&
        ((FrameworkElement)window.FindName("ExpandedView")).Effect is null && ((FrameworkElement)window.FindName("CardMask")).Effect is null;
    private static void Pump(int milliseconds)
    {
        var frame = new DispatcherFrame();
        var timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(milliseconds) };
        timer.Tick += (_, _) => { timer.Stop(); frame.Continue = false; };
        timer.Start(); Dispatcher.PushFrame(frame);
    }
    private static bool OutsideIsTransparent(MainWindow window)
    {
        var root = (FrameworkElement)window.Content;
        root.UpdateLayout();
        var bitmap = new System.Windows.Media.Imaging.RenderTargetBitmap((int)Math.Ceiling(window.Width), (int)Math.Ceiling(window.Height), 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
        bitmap.Render(root);
        var stride = bitmap.PixelWidth * 4;
        var pixels = new byte[stride * bitmap.PixelHeight]; bitmap.CopyPixels(pixels, stride, 0);
        var geometry = window.IslandOutline;
        var allowed = System.Windows.Media.Geometry.Combine(geometry,
            geometry.GetWidenedPathGeometry(new System.Windows.Media.Pen(System.Windows.Media.Brushes.Black, 48)),
            System.Windows.Media.GeometryCombineMode.Union, null);
        for (var y = 0; y < bitmap.PixelHeight; y += 2)
            for (var x = 0; x < bitmap.PixelWidth; x += 2)
                if (pixels[y * stride + x * 4 + 3] > 1 && !allowed.FillContains(new System.Windows.Point(x + .5, y + .5))) return false;
        return true;
    }
    private static Bounds GetVisualBounds(MainWindow window)
    {
        var bounds = window.VisualBounds; var dpi = System.Windows.Media.VisualTreeHelper.GetDpi(window);
        return new Bounds((int)Math.Round(bounds.Width * dpi.DpiScaleX), (int)Math.Round(bounds.Height * dpi.DpiScaleY));
    }
    private static Bounds GetBounds(Window window)
    {
        GetWindowRect(new WindowInteropHelper(window).Handle, out var rect);
        return new Bounds(rect.Right - rect.Left, rect.Bottom - rect.Top);
    }
    internal readonly record struct Bounds(int Width, int Height);
    [StructLayout(LayoutKind.Sequential)] private struct NativeRect { public int Left; public int Top; public int Right; public int Bottom; }
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr handle, out NativeRect rect);
}
