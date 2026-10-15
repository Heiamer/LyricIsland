using System.IO;
using System.Diagnostics;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;
using System.Windows.Media.Effects;
using System.Windows.Threading;
using LyricIsland.Media;
using Microsoft.Win32;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Utilities;
using Button = System.Windows.Controls.Button;
using Image = System.Windows.Controls.Image;
using Rectangle = System.Windows.Shapes.Rectangle;
using SystemColors = System.Windows.SystemColors;

namespace LyricIsland;

public partial class MainWindow : Window
{
    private readonly IslandSettings _settings;
    private readonly MusicEngine? _engine;
    private readonly CancellationTokenSource _lifetime = new();
    private readonly DispatcherTimer _poll;
    private readonly DispatcherTimer _hoverClose;
    private bool _resizing;
    private TimeSpan _lastRenderTime;
    internal int ResizeFrameCount { get; private set; }
    private readonly Stopwatch _resizeClock = new();
    private Rect _compactBounds;
    private Rect _cardBounds;
    private IslandTransitionFrame _transitionFrame = IslandTransitionFrame.Collapsed;
    private IslandTransition _transition = new(false, IslandTransitionFrame.Collapsed);
    private bool _transitionExpanded;
    private readonly RectangleGeometry _compactShape = new();
    private readonly ScaleTransform _capsuleScale = new(1, 1);
    private readonly TransformGroup _capsuleTransform = new();
    private readonly TransformGroup _cardTransform = new();
    private readonly TranslateTransform _cardTranslate = new();
    private readonly TranslateTransform _contentTranslate = new();
    private readonly RectangleGeometry _revealClip = new();
    private double _sampledTransitionTime;
    private readonly Windows.UI.ViewManagement.UISettings _uiSettings = new();
    private bool _transparencyAllowed = true;
    private static System.Windows.Media.Brush SurfaceBrush(bool transparent)
    {
        var alpha = transparent ? (byte)245 : (byte)255;
        var brush = new LinearGradientBrush(
            System.Windows.Media.Color.FromArgb(alpha, 27, 31, 37),
            System.Windows.Media.Color.FromArgb(alpha, 12, 15, 20), 90);
        brush.Freeze(); return brush;
    }
    internal Rect CapsuleBounds => _compactBounds;
    internal Rect CardBounds => _cardBounds;
    internal string EffectiveMaterial => "Default";
    internal bool SpatialMotionEnabled => Animate;
    internal bool MaterialEffectsEnabled => _transparencyAllowed;
    private readonly RectangleGeometry _compactClip = new();
    private readonly TranslateTransform _compactTranslate = new();
    private readonly ScaleTransform _cardScale = new(.96, .96);
    private readonly RectangleGeometry _surfaceShape = new(new Rect(0, 0, 280, 38), 19, 19);
    private readonly RectangleGeometry _contentClip = new(new Rect(0, 0, 280, 38), 19, 19);
    private readonly TranslateTransform _surfaceTranslate = new();
    private readonly ScaleTransform _contentScale = new(1, 1);
    private readonly BlurEffect _compactTransitionBlur = new() { KernelType = KernelType.Gaussian, RenderingBias = RenderingBias.Performance };
    private readonly BlurEffect _expandedTransitionBlur = new() { KernelType = KernelType.Gaussian, RenderingBias = RenderingBias.Performance };
    private readonly DispatcherTimer _resizeFinish = new() { Interval = TimeSpan.FromMilliseconds(330) };
    internal List<double> ResizeFrameIntervals { get; } = [];
    private double _lastFrameMilliseconds;
    internal Rect VisualBounds { get { var shape = IslandOutline.Bounds; if (shape.IsEmpty) shape = _expansion.Expanded ? _cardBounds : _compactBounds; return new Rect(Left + shape.X, Top + shape.Y, shape.Width, shape.Height); } }
    internal int HostBoundsChanges { get; private set; }
    internal IslandTransitionFrame TransitionFrame => _transitionFrame;
    internal Geometry IslandOutline
    {
        get
        {
            return _surfaceShape.CloneCurrentValue();
        }
    }
    internal bool PlaybackControlBusy => _controlBusy;
    internal PlaybackSnapshot? ActualPlayback => _engine?.RefreshState();
    internal double PlaybackSeconds => _engine?.Clock.Position ?? 0;
    internal string PlaybackButtonAction => PlayButton.ToolTip?.ToString() ?? string.Empty;
    internal string PlaybackTimingSource => _engine?.Clock.Status ?? string.Empty;
    internal string DisplayedElapsed => ElapsedTime.Text;
    internal double DisplayedProgress => PlaybackProgress.Value;
    internal string CoverBinding => _coverIdentity;
    internal int CoverByteCount => _cover?.Length ?? 0;
    internal string ArtworkBinding => _engine?.ArtworkIdentity ?? string.Empty;
    internal object? ArtworkDiagnostics => _engine?.ArtworkDiagnostics;
    internal Task<bool> SetPlaybackForArtworkTest(bool playing, CancellationToken token) => _engine!.SetPlayingAsync(playing, token);
    private readonly ExpansionState _expansion = new();
    private readonly MediaPresenceState _presence = new();
    private CancellationTokenSource? _visibilityTransition;
    private bool _userHidden;
    private bool _busy;
    private bool _closed;
    private bool _playing;
    private bool _controlBusy;
    private bool _mediaRefreshQueued;
    private bool _seeking;
    private double _seekDuration;
    private string? _seekTrackId;
    private PlaybackSnapshot? _presentedPlayback;
    private string? _controlMessage;
    private long _controlMessageUntil;
    private byte[]? _cover;
    private string _coverIdentity = string.Empty;
    private LyricDisplayFrame? _frame;
    private Process? _settingsProcess;
    private SettingsServer? _settingsServer;
    private readonly bool _demo;
    private readonly OutsideClickMonitor _outsideClicks;
    private readonly TopmostController _topmostController;

    public MainWindow(IslandSettings settings, bool demo = false)
    {
        InitializeComponent();
        FontFamily = BundledFonts.Body;
        IslandSurface.Data = CardHighlight.Data = _surfaceShape;
        CardShadow.Data = _revealClip; CardMask.Clip = _revealClip;
        IslandContent.Clip = _contentClip;
        var contentTransform = new TransformGroup();
        contentTransform.Children.Add(_contentScale); contentTransform.Children.Add(_surfaceTranslate);
        IslandContent.RenderTransform = contentTransform;
        ExpandedView.RenderTransform = _contentTranslate;
        _capsuleTransform.Children.Add(_capsuleScale); _capsuleTransform.Children.Add(_compactTranslate);
        CompactLayer.Clip = _compactClip; CompactLayer.RenderTransform = _capsuleTransform;
        _cardTransform.Children.Add(_cardScale); _cardTransform.Children.Add(_cardTranslate);
        CardLayer.RenderTransform = _cardTransform;
        _resizeFinish.Tick += (_, _) => FinishResize();
        _settings = settings;
        _demo = demo;
        _topmostController = new TopmostController(this);
        _outsideClicks = new OutsideClickMonitor(point => Dispatcher.BeginInvoke(() => HandleOutsideClick(point)));
        if (!demo) settings.AutoStart = StartupRegistration.IsEnabled;
        settings.Normalize();
        _engine = demo ? null : new MusicEngine(settings);
        if (_engine is not null) _engine.Media.Changed += OnNativeMediaChanged;
        if (!demo) StartSettingsServer();
        _poll = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(90) };
        _poll.Tick += OnPoll;
        _hoverClose = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(1200) };
        _hoverClose.Tick += OnHoverClose;
        Closed += OnClosed;
        Loaded += OnLoaded;
        SystemEvents.DisplaySettingsChanged += OnDisplayChanged;
        SystemEvents.UserPreferenceChanged += OnUserPreferenceChanged;
        _uiSettings.AdvancedEffectsEnabledChanged += OnEffectsPreferenceChanged;
        ApplySettings();
        UpdatePlaybackPresentation(new PlaybackSnapshot(false, TimeSpan.Zero, null));
    }

    private bool Animate => _settings.Animation;
    private void OnLoaded(object sender, RoutedEventArgs e) { if (!_resizing) Position(false); if (!_demo) _poll.Start(); }

    internal void ApplySettings()
    {
        _transparencyAllowed = _uiSettings.AdvancedEffectsEnabled && !SystemParameters.HighContrast;
        _engine?.ApplySettings();
        if (_engine?.LatestPlayback is { } playback) ObserveMediaPresence(playback);
        _topmostController.SetEnabled(_settings.AlwaysOnTop);
        CurrentLyric.FontSize = PreviousLyric.FontSize = HighlightLyric.FontSize = _settings.FontSize;
        ExpandedLyric.FontSize = _settings.FontSize + 3;
        MoveButton.IsEnabled = _settings.Floating;
        ApplyColors();
        UpdateActivity(_playing, force: true);
        Position(false);
        UpdateSecondary();
        UpdateHighlight(_frame?.WordScanProgress);
    }

    private void ApplyColors()
    {
        var text = new SolidColorBrush((System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(_settings.ExpandedTextColor));
        if (SystemParameters.HighContrast) text = SystemColors.WindowTextBrush.Clone();
        var secondary = text.Clone(); secondary.Opacity = SystemParameters.HighContrast ? 1 : .76;
        foreach (var label in new[] { SongTitle, ExpandedLyric }) label.Foreground = text;
        foreach (var label in new[] { ArtistName, SecondaryLyric, PlaybackStatus, ElapsedTime, TotalTime }) label.Foreground = secondary;
        Resources["IslandComponentBrush"] = new SolidColorBrush((System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(_settings.ExpandedComponentColor));
        if (SystemParameters.HighContrast) Resources["IslandComponentBrush"] = SystemColors.WindowTextBrush;
        Resources["IslandTrackBrush"] = SystemParameters.HighContrast ? SystemColors.WindowTextBrush : new SolidColorBrush(System.Windows.Media.Color.FromRgb(52, 59, 69));
        foreach (var bar in new[] { ActivityOne, ActivityTwo, ActivityThree }) bar.Fill = (System.Windows.Media.Brush)Resources["IslandComponentBrush"];
        var accent = ((SolidColorBrush)Resources["IslandComponentBrush"]).Color;
        PlayGlyph.Stroke = new SolidColorBrush(accent.R * .299 + accent.G * .587 + accent.B * .114 > 145 ? System.Windows.Media.Color.FromRgb(18, 37, 29) : Colors.White);
        HighlightLyric.Foreground = new SolidColorBrush((System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(_settings.LyricProgressColor));
        var surface = SystemParameters.HighContrast ? SystemColors.WindowBrush : SurfaceBrush(_transparencyAllowed);
        IslandSurface.Fill = CompactSurface.Fill = surface;
        IslandSurface.Stroke = CompactSurface.Stroke = SystemParameters.HighContrast ? SystemColors.WindowTextBrush : new SolidColorBrush(System.Windows.Media.Color.FromArgb(34, 255, 255, 255));
        CardHighlight.Visibility = SystemParameters.HighContrast ? Visibility.Hidden : Visibility.Visible;
        CardShadow.Effect = SystemParameters.HighContrast ? null : new DropShadowEffect { Color = Colors.Black, BlurRadius = 22, ShadowDepth = 5, Opacity = .3, RenderingBias = RenderingBias.Performance };
        if (SystemParameters.HighContrast)
        {
            CurrentLyric.Foreground = PreviousLyric.Foreground = HighlightLyric.Foreground = SystemColors.WindowTextBrush;
            PlayGlyph.Stroke = SystemColors.WindowBrush;
        }
        else CurrentLyric.Foreground = PreviousLyric.Foreground = new SolidColorBrush(System.Windows.Media.Color.FromRgb(241, 243, 245));
    }

    private void Position(bool animate)
    {
        if (_closed) return;
        if (!_demo) _outsideClicks.SetEnabled(_expansion.Expanded);
        if (animate && _settings.Animation && _resizing && _transitionExpanded == _expansion.Expanded) return;
        var velocity = _resizing ? _transition.Velocity(_sampledTransitionTime) : default;
        StopResize();
        var (work, _) = MonitorPlacement.Get(_settings.Display);
        var plan = IslandLayout.Plan(work, _settings);
        // Layout remains fixed throughout the entire transition.
        CommitBounds(plan.Host);
        PrepareContent(plan.Capsule, plan.Card);
        var changed = _transitionExpanded != _expansion.Expanded;
        _transitionExpanded = _expansion.Expanded;
        if (animate && _settings.Animation && changed)
        {
            _transition = new IslandTransition(_expansion.Expanded, _transitionFrame, _settings.Motion, velocity);
            _sampledTransitionTime = 0;
            _lastRenderTime = TimeSpan.MinValue; _lastFrameMilliseconds = 0;
            _resizeClock.Restart(); _resizing = true;
            ApplyTransitionFrame(_transitionFrame);
            CompositionTarget.Rendering += OnResizeFrame;
            _resizeFinish.Interval = TimeSpan.FromMilliseconds(_transition.DurationMilliseconds + 30);
            _resizeFinish.Start();
        }
        else CompleteExpansionVisuals();
    }

    private void PrepareContent(Rect compact, Rect card)
    {
        _compactBounds = new Rect(compact.Left - Left, compact.Top - Top, compact.Width, compact.Height);
        _cardBounds = new Rect(card.Left - Left, card.Top - Top, card.Width, card.Height);
        _compactTranslate.X = _compactBounds.X; _compactTranslate.Y = _compactBounds.Y;
        _compactClip.Rect = new Rect(0, 0, compact.Width, compact.Height);
        _compactClip.RadiusX = _compactClip.RadiusY = compact.Height / 2;
        CompactLayer.Width = compact.Width; CompactLayer.Height = compact.Height;
        CompactView.Width = Math.Max(1, compact.Width - 14);
        CardLayer.Width = Width; CardLayer.Height = Height;
        _surfaceShape.Rect = _cardBounds; _surfaceShape.RadiusX = _surfaceShape.RadiusY = _settings.Motion.Radius;
        _surfaceTranslate.X = _cardBounds.X; _surfaceTranslate.Y = _cardBounds.Y;
        _contentClip.Rect = new Rect(0, 0, card.Width, card.Height);
        _contentClip.RadiusX = _contentClip.RadiusY = _settings.Motion.Radius;
        IslandContent.Width = card.Width; IslandContent.Height = card.Height;
        ExpandedView.Width = Math.Max(1, card.Width - 36); ExpandedView.Height = Math.Max(1, card.Height - 32);
        _cardScale.CenterX = _cardBounds.X + card.Width / 2;
        _cardScale.CenterY = _cardBounds.Y;
        _capsuleScale.CenterX = compact.Width / 2; _capsuleScale.CenterY = compact.Height / 2;
    }

    private static double Blend(double progress, double start, double end)
    {
        var t = Math.Clamp((progress - start) / (end - start), 0, 1);
        return t * t * (3 - 2 * t);
    }
    private void ApplyTransitionFrame(IslandTransitionFrame frame)
    {
        _transitionFrame = frame;
        var progress = frame.CardReveal;
        var shape = IslandLayout.Morph(_compactBounds, _cardBounds, progress, _settings.Motion.Radius);
        _surfaceShape.Rect = _revealClip.Rect = shape.Rect;
        _surfaceShape.RadiusX = _surfaceShape.RadiusY = _revealClip.RadiusX = _revealClip.RadiusY = shape.RadiusX;
        var contentScale = Math.Min(shape.Rect.Width / _cardBounds.Width, shape.Rect.Height / _cardBounds.Height);
        _contentScale.ScaleX = _contentScale.ScaleY = contentScale;
        _surfaceTranslate.X = shape.Rect.X + (shape.Rect.Width - _cardBounds.Width * contentScale) / 2;
        _surfaceTranslate.Y = shape.Rect.Y + (shape.Rect.Height - _cardBounds.Height * contentScale) / 2;
        CompactSurface.Visibility = Visibility.Hidden;
        _capsuleScale.ScaleY = 1;
        CompactView.BeginAnimation(OpacityProperty, null);
        CompactView.Opacity = 1 - Blend(progress, .06, .46);
        CompactLayer.Visibility = CompactView.Visibility = CompactView.Opacity > .001 ? Visibility.Visible : Visibility.Hidden;
        CompactLayer.IsHitTestVisible = CompactView.Opacity > .1;
        _compactTransitionBlur.Radius = 3 * Blend(progress, .06, .4);
        CompactView.Effect = Animate && _resizing && CompactView.Opacity > .001 && _compactTransitionBlur.Radius > .05 ? _compactTransitionBlur : null;
        CardLayer.Opacity = 1;
        _cardScale.ScaleX = _cardScale.ScaleY = 1;
        _cardTranslate.Y = 0;
        CardLayer.Visibility = Visibility.Visible;
        ExpandedView.BeginAnimation(OpacityProperty, null);
        ExpandedView.Opacity = frame.ContentOpacity * Blend(progress, .25, .88);
        _contentTranslate.Y = frame.ContentOffsetY;
        ExpandedView.Visibility = Toolbar.Visibility = ExpandedView.Opacity > .001 ? Visibility.Visible : Visibility.Hidden;
        ExpandedView.IsHitTestVisible = Toolbar.IsHitTestVisible = _expansion.Expanded && ExpandedView.Opacity > .01;
        _expandedTransitionBlur.Radius = frame.CardBlur;
        ExpandedView.Effect = Animate && _resizing && ExpandedView.Opacity > .001 && frame.CardBlur > .05 ? _expandedTransitionBlur : null;
        CardMask.Effect = null;
    }

    private void OnMorphSurfaceClick(object sender, MouseButtonEventArgs e)
    {
        if (!_expansion.Expanded && !_settings.ClickAnywhereExpand) return;
        // The morphing surface remains a target even while both content layers cross-fade.
        if (_expansion.Expanded) _expansion.Close(); else _expansion.ClickCover();
        RefreshExpansion(); e.Handled = true;
    }

    // Shared by the live render loop and deterministic animation previews.
    internal void RenderTransitionAt(double milliseconds)
    {
        _sampledTransitionTime = milliseconds;
        ApplyTransitionFrame(_transition.Sample(milliseconds));
    }

    private void StopResize()
    {
        _resizeFinish.Stop();
        if (!_resizing) return;
        // Preserve the last displayed frame when reversing, rather than skipping ahead between render ticks.
        _resizeClock.Stop(); CompositionTarget.Rendering -= OnResizeFrame; _resizing = false;
    }
    private void FinishResize()
    {
        if (!_resizing || _closed) return;
        StopResize(); CompleteExpansionVisuals();
    }
    private void OnResizeFrame(object? sender, EventArgs e)
    {
        if (e is RenderingEventArgs rendering)
        {
            if (rendering.RenderingTime == _lastRenderTime) return;
            _lastRenderTime = rendering.RenderingTime;
        }
        ResizeFrameCount++;
        var now = _resizeClock.Elapsed.TotalMilliseconds;
        if (_lastFrameMilliseconds > 0) ResizeFrameIntervals.Add(now - _lastFrameMilliseconds);
        if (ResizeFrameIntervals.Count > 8192) ResizeFrameIntervals.RemoveRange(0, 4096);
        _lastFrameMilliseconds = now;
        RenderTransitionAt(now);
        if (now >= _transition.DurationMilliseconds) FinishResize();
    }
    private void CompleteExpansionVisuals()
    {
        ApplyTransitionFrame(_expansion.Expanded ? IslandTransitionFrame.Expanded : IslandTransitionFrame.Hidden(_settings.Motion));
        ClearExpansionBlur();
    }
    private void CommitBounds(Rect bounds)
    {
        // Native positions round to physical pixels, which can differ from the
        // requested logical coordinates by half a pixel at fractional DPI.
        var dpi = VisualTreeHelper.GetDpi(this);
        var positionToleranceX = .5 / dpi.DpiScaleX + .01;
        var positionToleranceY = .5 / dpi.DpiScaleY + .01;
        if (Math.Abs(Width - bounds.Width) < .1 && Math.Abs(Height - bounds.Height) < .1 &&
            double.IsFinite(Left) && double.IsFinite(Top) && Math.Abs(Left - bounds.Left) <= positionToleranceX && Math.Abs(Top - bounds.Top) <= positionToleranceY) return;
        HostBoundsChanges++;
        Width = bounds.Width; Height = bounds.Height; Left = bounds.Left; Top = bounds.Top;
        MonitorPlacement.Commit(this, bounds);
    }

    private void RefreshExpansion()
    {
        _hoverClose.Stop();
        Position(true);
        ExpandedCoverButton.ToolTip = _expansion.HeldOpen ? "点击收起" : "点击保持展开";
        System.Windows.Automation.AutomationProperties.SetName(CoverButton, _expansion.Expanded ? "收起灵动岛" : "展开灵动岛");
    }
    private void ClearExpansionBlur()
    {
        _compactTransitionBlur.BeginAnimation(BlurEffect.RadiusProperty, null);
        _expandedTransitionBlur.BeginAnimation(BlurEffect.RadiusProperty, null);
        _compactTransitionBlur.Radius = _expandedTransitionBlur.Radius = 0;
        // Detach effects at rest to keep text sharp and avoid an offscreen render pass.
        CompactView.Effect = ExpandedView.Effect = CardMask.Effect = null;
    }
    private void Fade(UIElement element, double target)
    {
        var current = element.Opacity;
        element.BeginAnimation(OpacityProperty, null);
        element.Opacity = target;
        if (_settings.Animation) element.BeginAnimation(OpacityProperty, new DoubleAnimation(current, target, TimeSpan.FromMilliseconds(Animate ? 180 : 120)) { EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut }, FillBehavior = FillBehavior.Stop });
    }

    private void OnCompactSurfaceClick(object sender, MouseButtonEventArgs e) { if (_settings.ClickAnywhereExpand) OnCoverClick(sender, e); e.Handled = true; }
    private void OnIslandBodyClick(object sender, MouseButtonEventArgs e)
    {
        if (!_settings.ClickAnywhereExpand || _expansion.Expanded) return;
        // The cover keeps its existing Click handler. Handling that here too
        // would toggle twice and immediately collapse the island again.
        for (var source = e.OriginalSource as DependencyObject; source is not null; source = source is Visual or System.Windows.Media.Media3D.Visual3D ? VisualTreeHelper.GetParent(source) : LogicalTreeHelper.GetParent(source))
            if (source is System.Windows.Controls.Button) return;
        if (!IslandOutline.FillContains(e.GetPosition(IslandRoot))) return;
        _expansion.ClickCover(); RefreshExpansion(); e.Handled = true;
    }
    private void OnCoverClick(object sender, RoutedEventArgs e) { _expansion.ClickCover(); RefreshExpansion(); }
    private void OnCollapse(object sender, RoutedEventArgs e) { _expansion.Close(); RefreshExpansion(); }
    internal void HandleOutsideClick(System.Windows.Point screenPoint)
    {
        if (_closed || !IsVisible || !_expansion.Expanded || _seeking) return;
        if (IslandOutline.FillContains(PointFromScreen(screenPoint))) return;
        _expansion.Close(); RefreshExpansion();
    }
    private void OnIslandEnter(object sender, System.Windows.Input.MouseEventArgs e)
    {
        _hoverClose.Stop();
        if (_settings.HoverExpand && !_expansion.Expanded) { _expansion.HoverEnter(); RefreshExpansion(); }
    }
    private void OnIslandLeave(object sender, System.Windows.Input.MouseEventArgs e)
    {
        if (_settings.HoverExpand && _expansion.Expanded && !_expansion.HeldOpen) _hoverClose.Start();
    }
    private void OnHoverClose(object? sender, EventArgs e)
    {
        _hoverClose.Stop();
        if (!IsMouseOver && !IsKeyboardFocusWithin && _expansion.HoverLeave()) RefreshExpansion();
    }
    private void OnKeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (e.Key != Key.Escape) return;
        _expansion.Close(); RefreshExpansion(); CoverButton.Focus(); e.Handled = true;
    }

    private void OnNativeMediaChanged(object? sender, EventArgs e)
    {
        if (_closed || Dispatcher.HasShutdownStarted) return;
        Dispatcher.BeginInvoke(() =>
        {
            if (_closed || _mediaRefreshQueued) return;
            _mediaRefreshQueued = true;
            Dispatcher.BeginInvoke(() => { _mediaRefreshQueued = false; OnPoll(null, EventArgs.Empty); }, DispatcherPriority.Background);
        }, DispatcherPriority.Normal);
    }
    private async void OnPoll(object? sender, EventArgs e)
    {
        if (_closed || _engine is null) return;
        if (_engine.RefreshState() is { } state && (state.IsPlaying != _playing || _busy))
        {
            ObserveMediaPresence(state);
            _playing = state.IsPlaying; UpdateActivity(_playing); UpdatePlaybackPresentation(state, _engine.Clock.IsLocal);
        }
        if (_busy) return;
        _busy = true;
        try
        {
            var (playback, frame) = await _engine.ReadAsync(_lifetime.Token);
            if (_closed) return;
            playback = _engine.RefreshState() ?? playback;
            ObserveMediaPresence(playback);
            if (!MediaPresenceState.HasSource(playback) && _presence.HasMedia) return;
            _frame = frame;
            _playing = playback.IsPlaying;
            UpdateActivity(_playing);
            var text = playback.Track is null ? "打开播放器，开始一首音乐" :
                string.IsNullOrWhiteSpace(frame.CurrentLine) ? "等待歌词…" : frame.CurrentLine;
            SetLyric(text);
            IslandSurface.ToolTip = playback.Track is null ? "Lyric Island" : $"{playback.Track.Title} · {playback.Track.Artist}";
            UpdateSecondary();
            UpdateHighlight(frame.WordScanProgress);
            UpdateCover(playback);
            UpdatePlaybackPresentation(playback, _engine.Clock.IsLocal);
        }
        catch (OperationCanceledException) when (_lifetime.IsCancellationRequested) { }
        catch (Exception ex)
        {
            if (!_closed) { SetLyric("正在重试播放器连接…"); Log.Error($"Island playback read: {ex}"); }
        }
        finally { _busy = false; }
    }

    internal void ObserveMediaPresence(PlaybackSnapshot playback)
    {
        var action = _presence.Observe(playback, _settings.HideWhenPaused);
        if (action == MediaPresenceAction.None) return;
        _visibilityTransition?.Cancel(); _visibilityTransition?.Dispose();
        _visibilityTransition = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token);
        if (_userHidden) return;
        var token = _visibilityTransition.Token;
        _ = Dispatcher.InvokeAsync(() => TransitionMediaVisibilityAsync(action, token)).Task.Unwrap();
    }
    private async Task TransitionMediaVisibilityAsync(MediaPresenceAction action, CancellationToken token)
    {
        try
        {
            if (action == MediaPresenceAction.Show)
            {
                if (!IsVisible)
                {
                    _expansion.Close(); Position(false);
                    Opacity = 0; Show();
                }
                AnimatePresenceOpacity(1);
                return;
            }
            _expansion.Close(); RefreshExpansion();
            if (_resizing) await Task.Delay((int)Math.Ceiling(_transition.DurationMilliseconds) + 30, token);
            token.ThrowIfCancellationRequested();
            AnimatePresenceOpacity(0);
            if (_settings.Animation) await Task.Delay(Animate ? 220 : 140, token);
            token.ThrowIfCancellationRequested();
            if (!_closed) Hide();
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested) { }
        catch (Exception ex) { Log.Warn($"Media visibility: {ex.Message}"); }
    }
    private void AnimatePresenceOpacity(double target)
    {
        // Invisible native smoke windows remain invisible while exercising the same lifecycle.
        if (_demo && Opacity == 0) target = 0;
        var from = Opacity; BeginAnimation(OpacityProperty, null); Opacity = target;
        if (_settings.Animation)
        {
            var animation = new DoubleAnimation(from, target, TimeSpan.FromMilliseconds(Animate ? 200 : 120))
            { EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut }, FillBehavior = FillBehavior.Stop };
            Timeline.SetDesiredFrameRate(animation, AnimationCadence.TargetRate);
            BeginAnimation(OpacityProperty, animation);
        }
    }
    internal void ToggleUserVisibility()
    {
        _visibilityTransition?.Cancel();
        if (IsVisible) { _userHidden = true; Hide(); }
        else ShowFromTray();
    }
    internal void ShowFromTray()
    {
        _userHidden = false; _visibilityTransition?.Cancel();
        BeginAnimation(OpacityProperty, null); Opacity = 1;
        Show(); if (!_resizing) Position(false);
    }

    private void SetLyric(string text)
    {
        if (ExpandedLyric.Text != text)
        {
            ExpandedLyric.Text = text; ExpandedLyric.ToolTip = text;
            AnimateLine(ExpandedLyric, 0, 1, 5, 0);
        }
        if (CurrentLyric.Text == text) return;
        var outgoingOpacity = CurrentLyric.Opacity;
        var outgoingY = (CurrentLyric.RenderTransform as TranslateTransform)?.Y ?? 0;
        PreviousLyric.Text = CurrentLyric.Text;
        CurrentLyric.Text = HighlightLyric.Text = text;
        CurrentLyric.ToolTip = text;
        HighlightLyric.Clip = new RectangleGeometry(new Rect(0, 0, 0, 30));
        AnimateLine(PreviousLyric, outgoingOpacity, 0, outgoingY, -4);
        AnimateLine(CurrentLyric, 0, 1, 4, 0);
    }

    private void AnimateLine(TextBlock line, double from, double to, double yFrom, double yTo)
    {
        line.BeginAnimation(OpacityProperty, null);
        line.Opacity = to;
        var transform = new TranslateTransform();
        line.RenderTransform = transform;
        if (!_settings.Animation) return;
        var duration = TimeSpan.FromMilliseconds(Animate ? 180 : 120);
        line.BeginAnimation(OpacityProperty, new DoubleAnimation(from, to, duration) { FillBehavior = FillBehavior.Stop });
        if (!Animate) return;
        transform.BeginAnimation(TranslateTransform.YProperty, new DoubleAnimation(yFrom, yTo, duration)
        { EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }, FillBehavior = FillBehavior.Stop });
    }

    private void UpdateHighlight(double? progress)
    {
        var amount = _settings.WordHighlight && progress.HasValue ? Math.Clamp(progress.Value, 0, 1) : 0;
        HighlightLyric.Clip = new RectangleGeometry(new Rect(0, 0, LyricViewport.ActualWidth * amount, 30));
        HighlightLyric.Opacity = amount > 0 ? CurrentLyric.Opacity : 0;
        HighlightLyric.RenderTransform = CurrentLyric.RenderTransform;
    }

    private void UpdateSecondary()
    {
        var text = _frame is null ? "点击封面展开歌词和播放控制" :
            _settings.Translation && !string.IsNullOrWhiteSpace(_frame.CurrentTranslation) ? _frame.CurrentTranslation : _frame.NextLine;
        SecondaryLyric.Text = string.IsNullOrWhiteSpace(text) ? "暂无下一句歌词" : text;
        SecondaryLyric.ToolTip = SecondaryLyric.Text;
    }

    private bool? _lastActivity;
    private void UpdateActivity(bool playing, bool force = false)
    {
        if (!force && _lastActivity == playing) return;
        _lastActivity = playing;
        var bars = new Rectangle[] { ActivityOne, ActivityTwo, ActivityThree };
        for (var i = 0; i < bars.Length; i++)
        {
            var currentScale = (bars[i].RenderTransform as ScaleTransform)?.ScaleY ?? .35;
            if (bars[i].RenderTransform is ScaleTransform oldTransform) oldTransform.BeginAnimation(ScaleTransform.ScaleYProperty, null);
            var transform = new ScaleTransform(1, playing ? 1 : .35);
            bars[i].RenderTransform = transform;
            Fade(bars[i], playing ? .9 : .45);
            if (playing && Animate) transform.BeginAnimation(ScaleTransform.ScaleYProperty,
                new DoubleAnimation(.3, 1, TimeSpan.FromMilliseconds(320 + i * 95)) { EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut }, AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever });
            else if (Animate && Math.Abs(currentScale - .35) > .001)
                transform.BeginAnimation(ScaleTransform.ScaleYProperty, new DoubleAnimation(currentScale, .35, TimeSpan.FromMilliseconds(160))
                { EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }, FillBehavior = FillBehavior.Stop });
        }
    }

    internal void UpdateCover(PlaybackSnapshot playback)
    {
        var identity = CoverIdentity.FromTrack(playback.Track);
        if (identity != _coverIdentity)
        {
            _coverIdentity = identity; _cover = null;
            CoverImage.Source = ExpandedCoverImage.Source = null;
        }
        var bytes = playback.CoverImageBytes;
        if (bytes is not { Length: > 0 })
        {
            _cover = null; CoverImage.Source = ExpandedCoverImage.Source = null; return;
        }
        if (ReferenceEquals(bytes, _cover)) return;
        _cover = bytes;
        try
        {
            using var stream = new MemoryStream(bytes);
            var image = new BitmapImage(); image.BeginInit(); image.CacheOption = BitmapCacheOption.OnLoad; image.StreamSource = stream; image.DecodePixelWidth = 192; image.EndInit(); image.Freeze();
            CoverImage.Source = ExpandedCoverImage.Source = image; Fade(CoverImage, 1); Fade(ExpandedCoverImage, 1);
        }
        catch (Exception ex) when (ex is NotSupportedException or FileFormatException or ArgumentException) { CoverImage.Source = ExpandedCoverImage.Source = null; Log.Warn($"Cover decode: {ex.Message}"); }
    }

    internal void UpdatePlaybackPresentation(PlaybackSnapshot playback, bool localClock = false)
    {
        var sameTrack = _presentedPlayback?.Track is { } previousTrack && playback.Track is { } currentTrack &&
            CoverIdentity.FromTrack(previousTrack) == CoverIdentity.FromTrack(currentTrack);
        _presentedPlayback = playback;
        var model = PlaybackPresentation.From(playback, localClock);
        PreviousButton.IsEnabled = PlayButton.IsEnabled = NextButton.IsEnabled = playback.Track is not null && !_controlBusy;
        FavoriteButton.IsEnabled = playback.Track?.SourceApp.Equals("Netease", StringComparison.OrdinalIgnoreCase) == true && !_controlBusy;
        PlayGlyph.Data = Geometry.Parse(playback.IsPlaying ? "M5,2 L5,14 M11,2 L11,14" : "M4,2 L13,8 L4,14 Z");
        PlayButton.ToolTip = playback.IsPlaying ? "暂停" : "播放";
        System.Windows.Automation.AutomationProperties.SetName(PlayButton, (string)PlayButton.ToolTip);
        SongTitle.Text = model.Title; SongTitle.ToolTip = model.Title;
        ArtistName.Text = model.Artist; ArtistName.ToolTip = model.Artist;
        PlaybackStatus.Text = Environment.TickCount64 < _controlMessageUntil ? _controlMessage : model.Status;
        TotalTime.Text = model.Total;
        if (_seeking && _seekTrackId != playback.Track?.Id) _seeking = false;
        if (_seeking) return;
        ElapsedTime.Text = model.Elapsed;
        var canSeek = _engine?.CanSeek(playback) ?? playback.CanSeek;
        PlaybackProgress.IsEnabled = canSeek && model.HasDuration && !_controlBusy;
        var previousProgress = PlaybackProgress.Value;
        PlaybackProgress.BeginAnimation(System.Windows.Controls.Primitives.RangeBase.ValueProperty, null);
        PlaybackProgress.Value = model.Fraction;
        if (Animate && playback.IsPlaying && sameTrack && model.Fraction > previousProgress + .000001 && model.Fraction - previousProgress < .02)
            PlaybackProgress.BeginAnimation(System.Windows.Controls.Primitives.RangeBase.ValueProperty,
                new DoubleAnimation(previousProgress, model.Fraction, TimeSpan.FromMilliseconds(85)) { FillBehavior = FillBehavior.Stop });
        PlaybackProgress.ToolTip = !canSeek ? "此播放器未开放 SMTC 跳转接口" : model.HasDuration ? $"拖动跳转 · {model.Elapsed} / {model.Total}" : "播放器未提供歌曲总时长";
        System.Windows.Automation.AutomationProperties.SetHelpText(PlaybackProgress, (string)PlaybackProgress.ToolTip);
    }

    private void NotifyControl(string message)
    {
        _controlMessage = message; _controlMessageUntil = Environment.TickCount64 + 3000; PlaybackStatus.Text = message;
    }
    private async void OnMediaAction(object sender, RoutedEventArgs e)
    {
        if (_controlBusy || _engine is null || sender is not Button { Tag: string name } || !Enum.TryParse<MediaHotkeyAction>(name, out var action)) return;
        _controlBusy = true;
        var before = _engine.LatestPlayback;
        if (before is not null) UpdatePlaybackPresentation(before, _engine.Clock.IsLocal);
        try
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token);
            timeout.CancelAfter(TimeSpan.FromSeconds(3));
            if (action == MediaHotkeyAction.TogglePlayPause)
            {
                var accepted = await _engine.TogglePlaybackAsync(timeout.Token);
                if (!accepted) NotifyControl("播放器未接受播放控制，请重试");
            }
            else await _engine.Media.ExecuteAsync(action, timeout.Token);
        }
        catch (OperationCanceledException) { if (!_closed) NotifyControl("播放控制超时，请重试"); }
        catch (Exception ex) { Log.Warn($"Media action: {ex.Message}"); if (!_closed) NotifyControl("播放控制失败，请重试"); }
        finally
        {
            _controlBusy = false;
            if (!_closed && _engine.LatestPlayback is { } playback)
            {
                _playing = playback.IsPlaying; UpdateActivity(_playing); UpdatePlaybackPresentation(playback, _engine.Clock.IsLocal);
            }
        }
    }

    private void OnSeekStart(object sender, System.Windows.Controls.Primitives.DragStartedEventArgs e) => StartSeek();
    private void StartSeek()
    {
        if (_presentedPlayback is not { Track: { } track } playback || !(_engine?.CanSeek(playback) ?? playback.CanSeek) || track.Duration <= TimeSpan.Zero) return;
        _seeking = true; _seekDuration = track.Duration.TotalSeconds; _seekTrackId = track.Id;
        PlaybackProgress.BeginAnimation(System.Windows.Controls.Primitives.RangeBase.ValueProperty, null);
    }
    private void OnSeekValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (_seeking) ElapsedTime.Text = PlaybackPresentation.Format(_seekDuration * e.NewValue);
    }
    private async void OnSeekCompleted(object sender, System.Windows.Controls.Primitives.DragCompletedEventArgs e)
    {
        if (e.Canceled) { _seeking = false; return; }
        await CommitSeekAsync();
    }
    private void OnSeekKeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (e.Key is Key.Left or Key.Right or Key.Home or Key.End) StartSeek();
    }
    private async void OnSeekKeyUp(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (_seeking && e.Key is Key.Left or Key.Right or Key.Home or Key.End) await CommitSeekAsync();
    }
    private void OnSeekPointerDown(object sender, MouseButtonEventArgs e) { if (PlaybackProgress.IsEnabled) StartSeek(); }
    private async void OnSeekPointerUp(object sender, MouseButtonEventArgs e) { if (_seeking) await CommitSeekAsync(); }
    private async Task CommitSeekAsync()
    {
        if (!_seeking) return;
        if (_controlBusy || _engine is null) { _seeking = false; return; }
        var seconds = _seekDuration * PlaybackProgress.Value;
        _seeking = false;
        if (_engine.LatestPlayback?.Track?.Id != _seekTrackId) return;
        _controlBusy = true;
        try
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token); timeout.CancelAfter(TimeSpan.FromSeconds(5));
            var accepted = await _engine.SeekAsync(seconds, timeout.Token);
            if (!accepted) NotifyControl("播放器未接受进度跳转");
            // Do not replace real position with the requested value. Next SMTC sample confirms it.
        }
        catch (OperationCanceledException) { if (!_closed) NotifyControl("进度跳转超时"); }
        catch (Exception ex) { Log.Warn($"Seek: {ex.Message}"); if (!_closed) NotifyControl("进度跳转失败"); }
        finally { _controlBusy = false; if (!_closed && _engine.LatestPlayback is { } playback) UpdatePlaybackPresentation(playback, _engine.Clock.IsLocal); }
    }
    private async void OnFavorite(object sender, RoutedEventArgs e)
    {
        if (_controlBusy || _engine?.LatestPlayback?.Track is not { } track) return;
        _controlBusy = true; FavoriteButton.IsEnabled = false;
        try
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token); timeout.CancelAfter(TimeSpan.FromSeconds(3));
            var result = await NeteaseFavorite.ToggleAsync(track, timeout.Token); NotifyControl(result.Message);
        }
        catch (OperationCanceledException) { if (!_closed) NotifyControl("喜欢操作超时，请重试"); }
        catch (Exception ex) { Log.Warn($"Favorite: {ex.Message}"); if (!_closed) NotifyControl("喜欢操作失败，请打开网易云后重试"); }
        finally { _controlBusy = false; if (!_closed && _engine.LatestPlayback is { } playback) UpdatePlaybackPresentation(playback, _engine.Clock.IsLocal); }
    }
    internal void ShowTrayMenu(System.Drawing.Point point)
    {
        var executable = Path.Combine(AppContext.BaseDirectory, "LyricIsland.Settings.exe");
        if (!File.Exists(executable)) { NotifyControl("WinUI 菜单组件缺失，请完整解压运行包"); return; }
        Process.Start(new ProcessStartInfo(executable)
        { UseShellExecute = false, WindowStyle = ProcessWindowStyle.Hidden, ArgumentList = { "--tray-menu", point.X.ToString(), point.Y.ToString() } });
    }
    private void OnTranslation(object sender, RoutedEventArgs e) { _settings.Translation = !_settings.Translation; SaveSettings(); UpdateSecondary(); }
    private void OnSettings(object sender, RoutedEventArgs e) => ShowSettings();
    internal void ShowSettings()
    {
        if (_settingsProcess is { HasExited: false }) return;
        var executable = Path.Combine(AppContext.BaseDirectory, "LyricIsland.Settings.exe");
        if (!File.Exists(executable))
        {
            System.Windows.MessageBox.Show("设置组件缺失，请完整解压新版运行包。", "Lyric Island");
            return;
        }
        _settingsProcess?.Dispose();
        _settingsProcess = Process.Start(new ProcessStartInfo(executable) { UseShellExecute = true, WorkingDirectory = Path.GetDirectoryName(executable)! });
    }
    internal void StartSettingsServer()
    {
        _settingsServer ??= new SettingsServer(request => Dispatcher.InvokeAsync(() => HandleSettingsRequest(request)).Task);
    }
    private object HandleSettingsRequest(JsonElement request)
    {
        if (request.ValueKind != JsonValueKind.Object || !request.TryGetProperty("version", out var version) || version.ValueKind != JsonValueKind.Number || !version.TryGetInt32(out var number) || number != 1 ||
            !request.TryGetProperty("command", out var command) || command.ValueKind != JsonValueKind.String)
            return new { success = false, message = "请求版本无效。" };
        switch (command.GetString())
        {
            case "state":
                return new { success = true, startupStatus = _demo ? "演示模式，不修改系统启动项。" : StartupRegistration.Status, sourceId = _engine?.LatestPlayback?.SourceAppUserModelId ?? "", settings = _settings, title = _engine?.LatestPlayback?.Track?.Title ?? "暂无播放",
                    effectiveMaterial = EffectiveMaterial, artist = _engine?.LatestPlayback?.Track?.Artist ?? "", player = _engine?.LatestPlayback?.Track?.SourceApp ?? "等待播放器",
                    position = _engine?.Clock.Position ?? 0, duration = _engine?.LatestPlayback?.Track?.Duration.TotalSeconds ?? 0, durationSource = _engine?.DurationSource ?? "演示", displayedTotal = TotalTime.Text, displayedElapsed = ElapsedTime.Text, progress = PlaybackProgress.Value, islandVisible = IsVisible, expanded = _expansion.Expanded, hasMedia = _presence.HasMedia, timeline = _engine?.Clock.Status ?? "演示模式",
                    smtcEvents = _engine?.Media.EventsObserved ?? 0, lyric = CurrentLyric.Text, lyricIndex = _frame?.CurrentLineIndex ?? -1, isPlaying = _engine?.LatestPlayback?.IsPlaying ?? false,
                    canSeek = _engine is { LatestPlayback: { } seekPlayback } && _engine.CanSeek(seekPlayback), smtcCanSeek = _engine?.LatestPlayback?.CanSeek ?? false,
                    playButton = PlayButton.ToolTip, renderFrames = ResizeFrameCount, hostBoundsChanges = HostBoundsChanges, localClock = _engine?.Clock.IsLocal ?? false, lyricSource = _engine?.LyricSource ?? "演示歌词" };
            case "toggleVisibility":
                ToggleUserVisibility(); return new { success = true };
            case "expand":
                ShowFromTray(); ExpandOnLaunch(); return new { success = true };
            case "settings":
                ShowSettings(); return new { success = true };
            case "exit":
                Dispatcher.BeginInvoke(() => Close(), DispatcherPriority.Background); return new { success = true };
            case "update":
                if (!request.TryGetProperty("value", out var update) || !SettingsProtocol.TryReadSetting(update, _settings))
                    return new { success = false, message = "设置值无效，未保存。" };
                bool saved;
                try
                {
                    saved = _demo || (update.GetProperty("key").GetString() == "AutoStart"
                        ? StartupRegistration.SetEnabled(_settings.AutoStart, _settings)
                        : SettingsStore.Default.Save(_settings));
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or ArgumentException)
                {
                    _settings.AutoStart = StartupRegistration.IsEnabled;
                    return new { success = false, message = $"无法更新开机启动：{ex.Message}" };
                }
                ApplySettings(); return new { success = saved, message = saved ? "已保存" : "保存失败，请检查数据目录权限。" };
            case "synchronize":
                if (!request.TryGetProperty("value", out var seconds) || seconds.ValueKind != JsonValueKind.Number || !seconds.TryGetDouble(out var position) || !double.IsFinite(position) || position < 0 || position > 86400)
                    return new { success = false, message = "请输入有效的播放秒数。" };
                _engine?.Clock.Synchronize(position);
                return new { success = true, message = "已按输入进度校准。" };
            case "resetAppearance":
                _settings.CompactWidth = 140; _settings.FontSize = 12.5; _settings.Floating = false;
                SaveSettings(); ApplySettings(); return new { success = true, message = "已恢复紧凑尺寸。" };
            default: return new { success = false, message = "不支持的操作。" };
        }
    }
    private void OnDrag(object sender, MouseButtonEventArgs e)
    {
        if (!_settings.Floating) return;
        Position(false); MonitorPlacement.Drag(this); RememberPosition(); e.Handled = true;
    }
    private void OnMoveKey(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (!_settings.Floating) return;
        var dx = e.Key == Key.Left ? -8 : e.Key == Key.Right ? 8 : 0;
        var dy = e.Key == Key.Up ? -8 : e.Key == Key.Down ? 8 : 0;
        if (dx == 0 && dy == 0) return;
        Position(false); Left += dx; Top += dy; RememberPosition(); Position(false); e.Handled = true;
    }
    private void RememberPosition()
    {
        var screen = System.Windows.Forms.Screen.FromHandle(new System.Windows.Interop.WindowInteropHelper(this).Handle);
        _settings.Display = screen.DeviceName;
        var (work, _) = MonitorPlacement.Get(_settings.Display);
        _settings.CenterX = (Left + Width / 2 - work.Left) / work.Width;
        _settings.TopFraction = (Top + _compactBounds.Top - work.Top) / Math.Max(1, work.Height - 40);
        SaveSettings();
    }
    private void SaveSettings() { if (!_demo && !SettingsStore.Default.Save(_settings)) SecondaryLyric.Text = "位置保存失败，请检查数据目录权限"; }
    private void OnDisplayChanged(object? sender, EventArgs e) => Dispatcher.BeginInvoke(() => Position(false));
    private void OnUserPreferenceChanged(object sender, UserPreferenceChangedEventArgs e) => Dispatcher.BeginInvoke(() => { if (!_closed) ApplySettings(); });
    private void OnEffectsPreferenceChanged(Windows.UI.ViewManagement.UISettings sender, object args) => Dispatcher.BeginInvoke(() => { if (!_closed) ApplySettings(); });
    protected override void OnDpiChanged(DpiScale oldDpi, DpiScale newDpi) { base.OnDpiChanged(oldDpi, newDpi); Dispatcher.BeginInvoke(() => Position(false), DispatcherPriority.ContextIdle); }
    private void OnClosed(object? sender, EventArgs e)
    {
        _closed = true; _poll.Stop(); _hoverClose.Stop(); StopResize(); ClearExpansionBlur(); _lifetime.Cancel();
        _outsideClicks.Dispose();
        _topmostController.Dispose();
        _poll.Tick -= OnPoll; _hoverClose.Tick -= OnHoverClose;
        SystemEvents.DisplaySettingsChanged -= OnDisplayChanged;
        SystemEvents.UserPreferenceChanged -= OnUserPreferenceChanged;
        _uiSettings.AdvancedEffectsEnabledChanged -= OnEffectsPreferenceChanged;
        _visibilityTransition?.Cancel(); _visibilityTransition?.Dispose();
        _settingsServer?.Dispose();
        if (_engine is not null) _engine.Media.Changed -= OnNativeMediaChanged;
        _engine?.Dispose(); _lifetime.Dispose();
        if (_settingsProcess is { HasExited: false }) _settingsProcess.CloseMainWindow();
        _settingsProcess?.Dispose();
    }

    internal void ExpandOnLaunch()
    {
        if (!_expansion.Expanded) _expansion.ClickCover();
        RefreshExpansion();
    }

    internal void SetDemo(bool expanded, string? lyric = null)
    {
        _frame = new LyricDisplayFrame(lyric ?? "每一段旋律，都有自己的光", "让歌词陪伴此刻的音乐", "演示", WordScanProgress: .4);
        UpdatePlaybackPresentation(new PlaybackSnapshot(true, TimeSpan.FromSeconds(83),
            new TrackInfo("demo", "此刻的光", "Lyric Island", "夜色与旋律", "演示", TimeSpan.FromSeconds(236))));
        SetLyric(_frame.CurrentLine); UpdateSecondary(); _playing = true; UpdateActivity(true);
        if (expanded) _expansion.ClickCover(); else _expansion.Close();
        RefreshExpansion();
    }
}

