using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.XamlTypeInfo;
using Windows.Graphics;

namespace LyricIsland.Settings;

internal static class Program
{
    [DllImport("Microsoft.UI.Xaml.dll", EntryPoint = "XamlCheckProcessRequirements")]
    private static extern void XamlCheckProcessRequirements();
    [STAThread]
    static void Main()
    {
        var arguments = Environment.GetCommandLineArgs();
        var testPipe = Environment.GetEnvironmentVariable("LYRICISLAND_TEST_PIPE");
        if (arguments.Length == 3 && arguments[1] is "--smoke-ui" or "--smoke-tray" && testPipe?.StartsWith("LyricIsland.Settings.test.", StringComparison.Ordinal) == true)
            SettingsProtocol.PipeName = testPipe;
        XamlCheckProcessRequirements();
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(initialization =>
        {
            SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new SettingsApp();
        });
    }
}

internal sealed partial class SettingsApp : Application, IXamlMetadataProvider
{
    private SettingsWindow? _window;
    private TrayMenuWindow? _trayWindow;
    private readonly XamlControlsXamlMetaDataProvider _metadata = new();
    public IXamlType GetXamlType(Type type) => _metadata.GetXamlType(type);
    public IXamlType GetXamlType(string name) => _metadata.GetXamlType(name);
    public XmlnsDefinition[] GetXmlnsDefinitions() => _metadata.GetXmlnsDefinitions();
    public SettingsApp()
    {
        UnhandledException += (_, e) =>
        {
            var args = Environment.GetCommandLineArgs();
            var directory = args.Length == 3 && args[1] == "--smoke-ui" ? args[2] : Path.Combine(SettingsStore.DataDirectory, "Logs");
            Directory.CreateDirectory(directory);
            File.WriteAllText(Path.Combine(directory, "winui-error.txt"), e.Exception.ToString() + "\n" + e.Message);
            e.Handled = true; Environment.ExitCode = 1; Exit();
        };

    }
    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        XamlControlsXamlMetaDataProvider.Initialize();
        Resources.MergedDictionaries.Add(new XamlControlsResources());
        Resources["ContentControlThemeFontFamily"] = new FontFamily("ms-appx:///Assets/Fonts/OPPOSans.ttf#OPPO Sans 4.0");
        var launch = Environment.GetCommandLineArgs();
        if (launch.Length == 4 && launch[1] == "--tray-menu" && int.TryParse(launch[2], out var x) && int.TryParse(launch[3], out var y))
        {
            _trayWindow = new TrayMenuWindow(x, y); _trayWindow.Activate(); return;
        }
        if (launch.Length == 3 && launch[1] == "--smoke-tray")
        {
            _trayWindow = new TrayMenuWindow(-32000, -32000, launch[2]); _trayWindow.Activate(); return;
        }
        _window = new SettingsWindow();
        _window.Activate();
    }
}

internal sealed class SettingsWindow : Window
{
    private static readonly FontFamily UiFont = new("ms-appx:///Assets/Fonts/OPPOSans.ttf#OPPO Sans 4.0");
    private readonly NavigationView _navigation;
    private readonly StackPanel _page;
    private readonly InfoBar _notice;
    private readonly DispatcherQueueTimer _refresh;
    private readonly SemaphoreSlim _requests = new(1, 1);
    private readonly Dictionary<string, DispatcherQueueTimer> _changes = new();
    private IslandSettings _settings = new();
    private JsonElement _state;
    private TextBlock? _positionLabel;
    private TextBlock? _playerLabel;
    private InfoBar? _timelineNotice;
    private bool _closed;
    private bool _refreshing;
    private string _selected = "appearance";
    private readonly string? _smokeDirectory;
    private readonly List<string> _smokePages = new();
    private Button? _palettePreviewButton, _paletteApplyButton, _systemColorButton;
    private Flyout? _palettePreviewFlyout;
    private ColorPicker? _palettePreviewPicker;

    public SettingsWindow()
    {
        Title = "Lyric Island · 设置";
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "LyricIsland-v2.ico"));
        var args = Environment.GetCommandLineArgs();
        if (args.Length == 3 && args[1] == "--smoke-ui") _smokeDirectory = args[2];
        AppWindow.Resize(new SizeInt32(900, 720));
        ApplyMaterial();
        if (_smokeDirectory is not null) AppWindow.Move(new PointInt32(-32000, -32000));
        _notice = new InfoBar { IsOpen = false, Margin = new Thickness(24, 0, 24, 10), IsClosable = true };
        _page = new StackPanel { Spacing = 10, Margin = new Thickness(28, 18, 28, 28) };
        var content = new Grid(); content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        content.Children.Add(_notice);
        var scroll = new ScrollViewer { Content = _page, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled };
        Grid.SetRow(scroll, 1); content.Children.Add(scroll);
        _navigation = new NavigationView
        {
            FontFamily = UiFont,
            PaneDisplayMode = NavigationViewPaneDisplayMode.Auto, IsBackButtonVisible = NavigationViewBackButtonVisible.Collapsed,
            CompactModeThresholdWidth = 480, ExpandedModeThresholdWidth = 720,
            IsSettingsVisible = false, OpenPaneLength = 190, Content = content,
            PaneHeader = new TextBlock { FontFamily = UiFont, Text = "Lyric Island", FontSize = 19, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Margin = new Thickness(16, 12, 0, 12) }
        };
        _navigation.MenuItems.Add(Item("显示与外观", "appearance", Symbol.View));
        _navigation.MenuItems.Add(Item("歌词与同步", "lyrics", Symbol.MusicInfo));
        _navigation.MenuItems.Add(Item("播放器与白名单", "player", Symbol.Play));
        _navigation.FooterMenuItems.Add(Item("关于", "about", Symbol.Help));
        _navigation.SelectionChanged += (_, e) => { if (e.SelectedItem is NavigationViewItem { Tag: string page }) { _selected = page; RenderPage(); } };
        _navigation.SelectedItem = _navigation.MenuItems[0];
        Content = _navigation;
        _refresh = DispatcherQueue.GetForCurrentThread().CreateTimer(); _refresh.Interval = TimeSpan.FromSeconds(1);
        _refresh.Tick += async (_, _) => await RefreshState();
        _navigation.Loaded += async (_, _) => { await RefreshState(); RenderPage(); if (_smokeDirectory is not null) await SmokeAsync(); else _refresh.Start(); };
        Closed += (_, _) => { _closed = true; _refresh.Stop(); foreach (var timer in _changes.Values) timer.Stop(); };
    }
    private static NavigationViewItem Item(string text, string key, Symbol icon) => new() { Content = text, Tag = key, Icon = new SymbolIcon(icon) };
    private static TextBlock Text(string text, double size = 14, bool muted = false) => new()
    {
        FontFamily = UiFont, Text = text, FontSize = size, TextWrapping = TextWrapping.Wrap,
        Foreground = (Brush)Application.Current.Resources[muted ? "TextFillColorSecondaryBrush" : "TextFillColorPrimaryBrush"]
    };
    private void RenderPage()
    {
        _page.Children.Clear(); _positionLabel = null; _playerLabel = null; _timelineNotice = null;
        var title = _selected switch { "lyrics" => "歌词与同步", "player" => "播放器与白名单", "about" => "关于", _ => "显示与外观" };
        _navigation.Header = new TextBlock { FontFamily = UiFont, Text = title, FontSize = 28, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        if (_selected == "appearance")
        {
            _page.Children.Add(Text("让歌词与音乐保持在眼前。调整会自动保存。", 14, true));
            Section("日常使用");
            ToggleCard("开机自动启动", "登录 Windows 后在系统托盘启动；无音乐播放时灵动岛可能隐藏。关闭仅移除此程序的启动项。", "AutoStart", _settings.AutoStart);
            _page.Children.Add(Text(StateText("startupStatus"), 12, true));
            ToggleCard("始终置顶（所有软件上方）", "切换软件或被其他置顶窗口遮挡时自动恢复到最上层，不抢占键盘焦点。", "AlwaysOnTop", _settings.AlwaysOnTop);
            ToggleCard("歌词进度涂色", "在灵动岛内按演唱进度为歌词涂色。关闭后歌词保持普通文字颜色。", "WordHighlight", _settings.WordHighlight);
            ToggleCard("暂停后隐藏", "暂停时收起并淡出，恢复播放时平滑显示胶囊。", "HideWhenPaused", _settings.HideWhenPaused);
            ToggleCard("悬停展开", "移开鼠标后自动收起；点击封面可保持展开。", "HoverExpand", _settings.HoverExpand);
            Section("尺寸与位置");
            SliderCard("胶囊宽度", "调整收起时的歌词显示空间。", "CompactWidth", 140, 480, _settings.CompactWidth, "px");
            SliderCard("歌词字号", "文字大小即时生效，封面尺寸保持一致。", "FontSize", 11, 17, _settings.FontSize, "px");
            ToggleCard("自由悬浮", "开启后按住移动按钮拖动，方向键可微调位置。", "Floating", _settings.Floating);
            Section("材质与颜色");
            MaterialCard();
            ColorCard("展开卡片字体颜色", "ExpandedTextColor", _settings.ExpandedTextColor);
            ColorCard("展开卡片组件颜色", "ExpandedComponentColor", _settings.ExpandedComponentColor);
            ColorCard("歌词进度填色", "LyricProgressColor", _settings.LyricProgressColor);
            Section("动画与反馈");
            ToggleCard("界面动画", "胶囊连续形变为卡片，支持随时反向操作。动画由此开关控制，不跟随 Windows 动画设置。", "Animation", _settings.Animation);
            MotionCard();
            var reset = new Button { Content = "恢复紧凑尺寸", HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(0, 8, 0, 0) };
            reset.Click += async (_, _) => { await Request("resetAppearance"); await RefreshState(); RenderPage(); }; _page.Children.Add(reset);
        }
        else if (_selected == "lyrics")
        {
            _timelineNotice = new InfoBar { IsClosable = false, Severity = InfoBarSeverity.Warning, Title = "播放器未提供有效进度", Message = "已启用本地计时。若在歌曲中途启动，请填入播放器当前播放秒数进行校准。", IsOpen = StateBool("localClock") };
            _page.Children.Add(_timelineNotice);
            _positionLabel = Text(PositionText(), 13, true); _page.Children.Add(_positionLabel);
            var seconds = new NumberBox { Minimum = 0, Maximum = 86400, Value = Math.Round(StateNumber("position")), SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact, Width = 140 };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(seconds, "播放器当前播放秒数");
            var calibrate = new Button { Content = "校准播放位置" };
            calibrate.Click += async (_, _) => { if (double.IsFinite(seconds.Value)) { await Request("synchronize", seconds.Value); await RefreshState(); } };
            var input = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 }; input.Children.Add(seconds); input.Children.Add(calibrate);
            _page.Children.Add(Card("播放位置校准", "仅用于备用本地计时。实时进度有效时无需校准；例如播放器显示 01:23，填入 83 秒。", input, vertical: true));
            SliderCard("歌词偏移", "正数让歌词提前，负数让歌词延后；不用于替代播放位置校准。", "OffsetMilliseconds", -5000, 5000, _settings.OffsetMilliseconds, "ms");
            ToggleCard("显示翻译", "展开时优先显示当前句翻译，没有翻译时显示下一句。", "Translation", _settings.Translation);
        }
        else if (_selected == "player")
        {
            var sources = new Grid { RowSpacing = 12, ColumnSpacing = 16 };
            for (var column = 0; column < 3; column++) sources.ColumnDefinitions.Add(new ColumnDefinition());
            for (var row = 0; row < (IslandSettings.SupportedPlayers.Length + 2) / 3; row++) sources.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            for (var i = 0; i < IslandSettings.SupportedPlayers.Length; i++)
            {
                var player = IslandSettings.SupportedPlayers[i];
                var check = new CheckBox { Content = player.Name, IsChecked = _settings.AllowedSources.Contains(player.Id, StringComparer.OrdinalIgnoreCase) };
                Grid.SetColumn(check, i % 3); Grid.SetRow(check, i / 3);
                async void SelectSource(object sender, RoutedEventArgs args)
                {
                    var allowed = _settings.AllowedSources.Where(x => !x.Equals(player.Id, StringComparison.OrdinalIgnoreCase)).ToList();
                    if (check.IsChecked == true) allowed.Add(player.Id);
                    await Change("AllowedSources", allowed.ToArray());
                }
                check.Checked += SelectSource; check.Unchecked += SelectSource; sources.Children.Add(check);
            }
            _page.Children.Add(Card("播放器白名单", "仅响应勾选的播放器。浏览器默认关闭，视频不会唤出灵动岛。需播放器提供 Windows 媒体会话；勾选不代表所有版本都支持拖动跳转。", sources, vertical: true));
            CustomPlayerCard();
            _playerLabel = Text(PlayerText()); _page.Children.Add(Card("当前播放信息", "系统媒体会话与网易云实时进度合并，歌词来源会随识别结果更新。", _playerLabel, vertical: true));
            _positionLabel = Text(PositionText()); _page.Children.Add(Card("同步状态", "优先系统时间轴或网易云实时读取，均不可用时才使用本地计时。", _positionLabel, vertical: true));
            _page.Children.Add(Text("封面、播放控制和时间轴使用播放器的媒体会话；歌词按歌曲信息匹配。若进度缺失，可到“歌词与同步”校准。", 13, true));
            ToggleCard("在线补全网易云歌曲信息", "本地列表缺失时，将当前歌曲 ID 发给 music.163.com 获取歌名和封面；不发送账号或令牌。", "OnlineSongInfo", _settings.OnlineSongInfo);
        }
        else
        {
            _page.Children.Add(Card("Lyric Island 0.8", "独立的歌词灵动岛", Text("设置界面使用 Microsoft WinUI 3 / Windows App SDK 2.5.1。胶囊使用独立原生窗口，设置与缓存位于 %APPDATA%\\LyricIsland。", 14, true), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.8", "2026-10-14", Text(
                "• 在 0.7.8.3 基础上，将原始和解析歌词缓存的读写迁入 Rust。\n" +
                "• 缓存保留 UTF-8 数据，仅按需解析最近使用的条目，减少托管对象和整库保存分配。\n" +
                "• 兼容现有缓存文件；保留直连网络、连续形变动画及完整 OPPO Sans。\n" +
                "• 应用、设置和 Rust 内核版本同步为 0.8，运行日志记录版本与引擎。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.8.2", "2026-10-14", Text(
                "• 歌词匹配、检索计划、候选准入与最终选择改由 Rust 内核执行。\n" +
                "• 歌词解析、QRC/KRC 解密与本地歌词取词同样由 Rust 执行；调用失败自动回退原实现。\n" +
                "• 本地 .krc 歌词保留源文件逐字时间，卡拉 OK 高亮更准确。\n" +
                "• 分享版运行目录改为 Runtime\\0.7.8.2，不再覆盖旧版。\n" +
                "• 显示帧计算（行查找、启动守卫、行内进度、逐字扫描）改由 Rust 内核执行。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.8", "2026-10-14", Text(
                "• 歌词、封面和歌曲信息全部直连，不使用系统或环境变量代理。\n" +
                "• 修复歌词请求失败后再次继承失效代理的问题。\n" +
                "• 00:00 时圆点从最左端开始，填色为零；切歌和向后跳转不沿用旧进度。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.7", "2026-10-13", Text(
                "• 默认顶部居中位置紧贴屏幕工作区上沿，移除 24px 空白。\n" +
                "• 保留胶囊连续形变、播放动效及自由悬浮位置。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.6", "2026-10-13", Text(
                "• 动画默认开启，由软件开关控制，不跟随系统动画设置。\n" +
                "• 胶囊连续扩展为卡片，宽高、圆角与文字同步交接。\n" +
                "• 快速反向操作保留位置和速度，默认带柔和回弹。\n" +
                "• 修复点击封面残留的绿色焦点边框。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.5", "2026-10-13", Text(
                "• 统一深色材质，以柔和渐变、上缘高光和阴影建立层次。\n" +
                "• 展开即时响应，反向操作保留当前位置与速度，默认无弹跳。\n" +
                "• 优化设置分组、按钮按下反馈和进度条操作范围。\n" +
                "• 跟随 Windows 动画、透明效果和高对比度偏好。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.4", "2026-10-13", Text(
                "• 顶部胶囊保留，展开卡片从底边向下揭开，默认 8px 间隔、520ms 弹出、320ms 收起。\n" +
                "• 内容延迟出现，收起先退内容，动态模糊随进度消退；快速反复点击也连续衔接。\n" +
                "• 支持调整间隔、时长、圆角和弹簧参数。\n" +
                "• 更换应用、设置窗口和托盘图标。", 13), vertical: true));
            _page.Children.Add(Card("更新日志 · 0.7.3", "2026-10-13", Text(
                "• 展开改为文字先退场，80ms 后胶囊分成两段向中心收缩，卡片快速弹出；收起采用反向顺序。\n" +
                "• 保留动态模糊、始终置顶及歌词进度涂色开关。\n" +
                "• 新增 TwilightEcho 默认白名单，支持自行添加、移除程序名或媒体会话标识。\n" +
                "• 字体、组件和歌词进度填色可分别使用调色盘或 Windows 主题色设置。\n" +
                "• 自启校验启动路径，并在升级后的正常启动中修复已开启的启动项。\n" +
                "• 修正托盘右键菜单的位置与 DPI 换算，让菜单靠近鼠标。", 13), vertical: true));
            _page.Children.Add(Text("复用 TaskbarLyrics 的歌词核心和媒体适配器，保留 MIT 许可证。", 13, true));
        }
    }
    private void ApplyMaterial()
    {
        SystemBackdrop = null;
        if (Content is Control root) root.Background = (Brush)Application.Current.Resources["ApplicationPageBackgroundThemeBrush"];
    }
    private void Section(string title)
    {
        var heading = Text(title, 17); heading.FontWeight = Microsoft.UI.Text.FontWeights.SemiBold;
        heading.Margin = new Thickness(2, 18, 0, 4); _page.Children.Add(heading);
    }
    private void MaterialCard()
    {
        var preview = new Border { Width = 112, Height = 48, CornerRadius = new CornerRadius(14),
            Background = new SolidColorBrush(PickerColor("#181D24")),
            BorderBrush = new SolidColorBrush(PickerColor("#363D46")), BorderThickness = new Thickness(1),
            Child = new TextBlock { Text = "深色", FontFamily = UiFont, FontSize = 14,
                Foreground = new SolidColorBrush(PickerColor("#F1F3F5")), HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center } };
        _page.Children.Add(Card("深色材质", "轻微透光、柔和高光与阴影。关闭系统透明效果时自动变为实色；设置页跟随系统主题。", preview));
    }
    private void MotionCard()
    {
        var panel = new StackPanel { Spacing = 12 };
        void Parameter(string title, string key, double value, double minimum, double maximum)
        {
            var input = new NumberBox { Value = value, Minimum = minimum, Maximum = maximum,
                SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact, Width = 150 };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(input, title);
            input.ValueChanged += (_, args) => { if (double.IsFinite(args.NewValue)) Debounce(key, args.NewValue); };
            var row = new Grid { ColumnSpacing = 12 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var label = Text(title); label.VerticalAlignment = VerticalAlignment.Center;
            Grid.SetColumn(input, 1); row.Children.Add(label); row.Children.Add(input); panel.Children.Add(row);
        }
        var motion = _settings.Motion;
        panel.Children.Add(Text("默认动效带克制的回弹。响应时间控制速度，实际结束时间由弹簧自然收敛决定。", 13, true));
        Parameter("展开响应（ms）", "MotionDuration", motion.Duration, 250, 1200);
        Parameter("收起响应（ms）", "MotionCollapseDuration", motion.CollapseDuration, 150, 700);
        Parameter("卡片圆角（px）", "MotionRadius", motion.Radius, 8, 48);
        Parameter("弹簧刚度 · stiffness", "SpringStiffness", motion.Spring.Stiffness, 100, 1000);
        Parameter("弹簧阻尼 · damping", "SpringDamping", motion.Spring.Damping, 10, 80);
        Parameter("弹簧质量 · mass", "SpringMass", motion.Spring.Mass, .3, 2);
        _page.Children.Add(new Expander { Header = "高级动效参数", Content = panel, HorizontalAlignment = HorizontalAlignment.Stretch });
    }
    private void ShowError(string message)
    {
        _notice.Title = "未能完成操作"; _notice.Message = message;
        _notice.Severity = InfoBarSeverity.Error; _notice.IsOpen = true;
    }
    private void CustomPlayerCard()
    {
        var panel = new StackPanel { Spacing = 10 };
        var input = new TextBox { AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MinHeight = 72,
            PlaceholderText = "例如：MyPlayer.exe 或 com.example.music\n每行一个，也可以用逗号分隔" };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(input, "自定义播放器白名单");
        var add = new Button { Content = "添加到白名单", HorizontalAlignment = HorizontalAlignment.Left };
        add.Click += async (_, _) =>
        {
            if (!PlayerSourceRules.TryParse(input.Text, out var entries, out var error)) { ShowError(error); return; }
            var allowed = _settings.AllowedSources.Concat(entries).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
            if (allowed.Length > 32) { ShowError("白名单最多允许 32 个条目，请先移除不需要的播放器。"); return; }
            add.IsEnabled = false;
            if (await Change("AllowedSources", allowed)) { await RefreshState(); RenderPage(); }
            else add.IsEnabled = true;
        };
        panel.Children.Add(input); panel.Children.Add(add);
        var builtIn = IslandSettings.SupportedPlayers.Select(x => x.Id).ToHashSet(StringComparer.OrdinalIgnoreCase);
        foreach (var source in _settings.AllowedSources.Where(x => !builtIn.Contains(x)).ToArray())
        {
            var row = new Grid { ColumnSpacing = 12 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var label = Text(source, 13); label.VerticalAlignment = VerticalAlignment.Center;
            var remove = new Button { Content = "移除" }; Grid.SetColumn(remove, 1);
            remove.Click += async (_, _) =>
            {
                if (await Change("AllowedSources", _settings.AllowedSources.Where(x => !x.Equals(source, StringComparison.OrdinalIgnoreCase)).ToArray()))
                { await RefreshState(); RenderPage(); }
            };
            row.Children.Add(label); row.Children.Add(remove); panel.Children.Add(row);
        }
        _page.Children.Add(Card("添加播放器白名单", "可填写程序名、EXE 名称或 Windows 媒体会话标识。仅响应允许的播放器；程序仍需提供系统媒体会话。添加后立即生效，可以单独移除。", panel, vertical: true));
    }
    private static Windows.UI.Color PickerColor(string hex) => new()
    {
        A = 255, R = Convert.ToByte(hex.Substring(1, 2), 16),
        G = Convert.ToByte(hex.Substring(3, 2), 16), B = Convert.ToByte(hex.Substring(5, 2), 16)
    };
    private static string ColorHex(Windows.UI.Color color) => $"#{color.R:X2}{color.G:X2}{color.B:X2}";
    private void ColorCard(string title, string key, string initial)
    {
        var swatch = new Border { Width = 30, Height = 30, CornerRadius = new CornerRadius(7), Background = new SolidColorBrush(PickerColor(initial)) };
        var label = Text(initial, 12, true); label.VerticalAlignment = VerticalAlignment.Center;
        var palette = new Button { Content = "调色盘" }; var system = new Button { Content = "系统主题色" };
        var picker = new ColorPicker { Color = PickerColor(initial), ColorSpectrumShape = ColorSpectrumShape.Ring,
            IsAlphaEnabled = false, IsHexInputVisible = true, IsColorChannelTextInputVisible = true, Width = 300 };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(picker, title);
        var popup = new StackPanel { Spacing = 12 }; popup.Children.Add(picker);
        var apply = new Button { Content = "应用颜色" }; var cancel = new Button { Content = "取消" };
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        actions.Children.Add(apply); actions.Children.Add(cancel); popup.Children.Add(actions);
        var flyout = new Flyout { Content = popup }; palette.Flyout = flyout;
        if (key == "ExpandedTextColor")
        {
            _palettePreviewButton = palette; _palettePreviewFlyout = flyout; _palettePreviewPicker = picker;
            _paletteApplyButton = apply; _systemColorButton = system;
        }
        async Task<bool> Apply(string color)
        {
            if (!await Change(key, color)) return false;
            initial = color; picker.Color = PickerColor(color);
            swatch.Background = new SolidColorBrush(picker.Color); label.Text = color;
            return true;
        }
        apply.Click += async (_, _) => { if (await Apply(ColorHex(picker.Color))) flyout.Hide(); };
        cancel.Click += (_, _) => { picker.Color = PickerColor(initial); flyout.Hide(); };
        system.Click += async (_, _) =>
        {
            var accent = new Windows.UI.ViewManagement.UISettings().GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent);
            await Apply(ColorHex(accent));
        };
        var controls = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        controls.Children.Add(swatch); controls.Children.Add(label); controls.Children.Add(palette); controls.Children.Add(system);
        _page.Children.Add(Card(title, "分别调整颜色；可使用圆形调色盘、十六进制色值，或读取当前 Windows 主题色。", controls, vertical: true));
    }

    private static Border Card(string title, string description, FrameworkElement control, bool vertical = false)
    {
        var copy = new StackPanel { Spacing = 6 }; copy.Children.Add(new TextBlock { FontFamily = UiFont, Text = title, FontSize = 14, TextWrapping = TextWrapping.Wrap, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold }); copy.Children.Add(Text(description, 13, true));
        var grid = new Grid { ColumnSpacing = 18, RowSpacing = 13 };
        if (vertical) { grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); Grid.SetRow(control, 1); }
        else { grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) }); grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto }); Grid.SetColumn(control, 1); }
        grid.Children.Add(copy); grid.Children.Add(control);
        return new Border { Child = grid, Padding = new Thickness(18), CornerRadius = new CornerRadius(14), Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"], BorderBrush = (Brush)Application.Current.Resources["CardStrokeColorDefaultBrush"], BorderThickness = new Thickness(1) };
    }
    private void ToggleCard(string title, string description, string key, bool initial)
    {
        var toggle = new ToggleSwitch { IsOn = initial, OnContent = "", OffContent = "", VerticalAlignment = VerticalAlignment.Center, MinWidth = 50 };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(toggle, title);
        var confirmed = initial; var restoring = false;
        toggle.Toggled += async (_, _) =>
        {
            if (restoring) return;
            var requested = toggle.IsOn;
            if (await Change(key, requested)) confirmed = requested;
            else { restoring = true; toggle.IsOn = confirmed; restoring = false; }
        };
        _page.Children.Add(Card(title, description, toggle));
    }
    private void SliderCard(string title, string description, string key, double min, double max, double initial, string unit)
    {
        var label = Text($"{initial:0.#} {unit}", 12, true);
        var slider = new Slider { Minimum = min, Maximum = max, Value = initial, StepFrequency = key == "OffsetMilliseconds" ? 100 : 1, Width = 210, IsThumbToolTipEnabled = true };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(slider, title);
        var control = new StackPanel { Spacing = 2 }; control.Children.Add(label); control.Children.Add(slider);
        var timer = DispatcherQueue.GetForCurrentThread().CreateTimer(); timer.Interval = TimeSpan.FromMilliseconds(60); timer.IsRepeating = false;
        if (_changes.TryGetValue(key, out var old)) old.Stop(); _changes[key] = timer;
        timer.Tick += async (_, _) => await Change(key, slider.Value);
        slider.ValueChanged += (_, _) => { label.Text = $"{slider.Value:0.#} {unit}"; if (!timer.IsRunning) timer.Start(); };
        _page.Children.Add(Card(title, description, control));
    }
    private void Debounce(string key, double value)
    {
        if (_changes.TryGetValue(key, out var previous)) previous.Stop();
        var timer = DispatcherQueue.GetForCurrentThread().CreateTimer();
        timer.Interval = TimeSpan.FromMilliseconds(140); timer.IsRepeating = false;
        timer.Tick += async (_, _) => await Change(key, value);
        _changes[key] = timer; timer.Start();
    }
    private async Task<bool> Change(string key, object value)
    {
        var success = await Request("update", new { key, value });
        if (success) SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key, value }), _settings);
        else await RefreshState();
        return success;
    }
    private async Task<bool> Request(string command, object? value = null)
    {
        await _requests.WaitAsync();
        try
        {
            var response = await SettingsProtocol.SendAsync(command, value);
            if (_closed) return false;
            var success = response.TryGetProperty("success", out var result) && result.ValueKind == JsonValueKind.True;
            _notice.Title = success ? "设置已保存" : "未能完成操作";
            _notice.Message = response.TryGetProperty("message", out var message) ? message.GetString() ?? "" : "";
            _notice.Severity = success ? InfoBarSeverity.Success : InfoBarSeverity.Error; _notice.IsOpen = !success || command == "synchronize";
            return success;
        }
        catch (Exception ex) when (ex is IOException or OperationCanceledException or JsonException)
        { if (!_closed) { _notice.IsOpen = true; _notice.Severity = InfoBarSeverity.Error; _notice.Title = "未连接歌词灵动岛"; _notice.Message = "请先运行 LyricIsland.exe，再重试。"; } return false; }
        finally { _requests.Release(); }
    }
    private async Task RefreshState()
    {
        if (_refreshing || _closed) return; _refreshing = true;
        try
        {
            await _requests.WaitAsync();
            try { _state = await SettingsProtocol.SendAsync("state"); }
            finally { _requests.Release(); }
            if (_closed) return;
            if (_state.TryGetProperty("settings", out var settings)) _settings = settings.Deserialize<IslandSettings>() ?? new();
            _settings.Normalize();
            ApplyMaterial();
            if (_positionLabel is not null) _positionLabel.Text = PositionText();
            if (_playerLabel is not null) _playerLabel.Text = PlayerText();
            if (_timelineNotice is not null) _timelineNotice.IsOpen = StateBool("localClock");
        }
        catch (Exception ex) when (ex is IOException or OperationCanceledException or JsonException)
        { if (!_closed) { _notice.IsOpen = true; _notice.Title = "正在等待歌词程序"; _notice.Message = "请先启动 LyricIsland.exe。"; _notice.Severity = InfoBarSeverity.Warning; } }
        finally { _refreshing = false; }
    }
    private async Task SmokeAsync()
    {
        try
        {
            Directory.CreateDirectory(_smokeDirectory!);
            _selected = "appearance"; RenderPage(); await Task.Delay(100);
            var originalMaterial = _settings.CardMaterial;
            await Change("CardMaterial", "Default"); await RefreshState();
            if (_settings.CardMaterial != "Default" || SystemBackdrop is not null)
                throw new InvalidOperationException("默认材质未同步。");
            await Change("CardMaterial", originalMaterial); await RefreshState();
            var originalGap = _settings.Motion.Gap;
            await Change("MotionGap", 12); await RefreshState();
            if (_settings.Motion.Gap != 12) throw new InvalidOperationException("动画参数未保存。");
            await Change("MotionGap", originalGap);
            var originalPaletteColor = _settings.ExpandedTextColor;
            _palettePreviewFlyout!.ShowAt(_palettePreviewButton!); await Task.Delay(100);
            if (_palettePreviewPicker!.ActualWidth < 200 || _palettePreviewPicker.ActualHeight < 200)
                throw new InvalidOperationException("调色盘未正确布局。");
            var paletteBitmap = new Microsoft.UI.Xaml.Media.Imaging.RenderTargetBitmap();
            await paletteBitmap.RenderAsync(_palettePreviewPicker);
            var palettePixels = await paletteBitmap.GetPixelsAsync();
            using (var paletteReader = Windows.Storage.Streams.DataReader.FromBuffer(palettePixels))
            {
                var data = new byte[palettePixels.Length]; paletteReader.ReadBytes(data);
                using var stream = new Windows.Storage.Streams.InMemoryRandomAccessStream();
                var encoder = await Windows.Graphics.Imaging.BitmapEncoder.CreateAsync(Windows.Graphics.Imaging.BitmapEncoder.PngEncoderId, stream);
                encoder.SetPixelData(Windows.Graphics.Imaging.BitmapPixelFormat.Bgra8, Windows.Graphics.Imaging.BitmapAlphaMode.Premultiplied,
                    (uint)paletteBitmap.PixelWidth, (uint)paletteBitmap.PixelHeight, 96, 96, data);
                await encoder.FlushAsync();
                using var fileReader = new Windows.Storage.Streams.DataReader(stream.GetInputStreamAt(0));
                await fileReader.LoadAsync((uint)stream.Size); var png = new byte[stream.Size]; fileReader.ReadBytes(png);
                File.WriteAllBytes(Path.Combine(_smokeDirectory!, "color-palette.png"), png);
            }
            _palettePreviewPicker.Color = PickerColor("#DCA020");
            ((Microsoft.UI.Xaml.Automation.Provider.IInvokeProvider)new Microsoft.UI.Xaml.Automation.Peers.ButtonAutomationPeer(_paletteApplyButton!).GetPattern(Microsoft.UI.Xaml.Automation.Peers.PatternInterface.Invoke)).Invoke();
            await Task.Delay(250); await RefreshState();
            if (_settings.ExpandedTextColor != "#DCA020") throw new InvalidOperationException("调色盘应用按钮未生效。");
            ((Microsoft.UI.Xaml.Automation.Provider.IInvokeProvider)new Microsoft.UI.Xaml.Automation.Peers.ButtonAutomationPeer(_systemColorButton!).GetPattern(Microsoft.UI.Xaml.Automation.Peers.PatternInterface.Invoke)).Invoke();
            await Task.Delay(250); await RefreshState();
            var expectedAccent = ColorHex(new Windows.UI.ViewManagement.UISettings().GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent));
            if (_settings.ExpandedTextColor != expectedAccent) throw new InvalidOperationException("系统主题色按钮未生效。");
            await Change("ExpandedTextColor", originalPaletteColor);
            foreach (var page in new[] { "appearance", "lyrics", "player", "about" })
            {
                _selected = page; RenderPage(); await Task.Delay(180);
                if (_page.Children.Count < 2 || _page.ActualWidth < 200) throw new InvalidOperationException("页面布局未完成。");
                _smokePages.Add(page);
                var bitmap = new Microsoft.UI.Xaml.Media.Imaging.RenderTargetBitmap();
                await bitmap.RenderAsync(_navigation);
                var pixels = await bitmap.GetPixelsAsync();
                using var reader = Windows.Storage.Streams.DataReader.FromBuffer(pixels);
                var data = new byte[pixels.Length]; reader.ReadBytes(data);
                using var stream = new Windows.Storage.Streams.InMemoryRandomAccessStream();
                var encoder = await Windows.Graphics.Imaging.BitmapEncoder.CreateAsync(Windows.Graphics.Imaging.BitmapEncoder.PngEncoderId, stream);
                encoder.SetPixelData(Windows.Graphics.Imaging.BitmapPixelFormat.Bgra8, Windows.Graphics.Imaging.BitmapAlphaMode.Premultiplied,
                    (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, data);
                await encoder.FlushAsync();
                using var fileReader = new Windows.Storage.Streams.DataReader(stream.GetInputStreamAt(0));
                await fileReader.LoadAsync((uint)stream.Size); var png = new byte[stream.Size]; fileReader.ReadBytes(png);
                File.WriteAllBytes(Path.Combine(_smokeDirectory!, "settings-" + page + ".png"), png);
            }
            var originalWidth = _settings.CompactWidth;
            await Change("CompactWidth", 300); await RefreshState();
            if (Math.Abs(_settings.CompactWidth - 300) > .1) throw new InvalidOperationException("即时设置未生效。");
            await Change("CompactWidth", originalWidth);
            var originalStartup = _settings.AutoStart;
            await Change("AutoStart", !originalStartup); await RefreshState();
            if (_settings.AutoStart == originalStartup) throw new InvalidOperationException("开机启动设置联动失败。");
            await Change("AutoStart", originalStartup);
            var originalTopmost = _settings.AlwaysOnTop;
            await Change("AlwaysOnTop", !originalTopmost); await RefreshState();
            if (_settings.AlwaysOnTop == originalTopmost) throw new InvalidOperationException("置顶设置联动失败。");
            await Change("AlwaysOnTop", originalTopmost);
            var originalHighlight = _settings.WordHighlight;
            await Change("WordHighlight", false); await RefreshState();
            if (_settings.WordHighlight) throw new InvalidOperationException("歌词进度涂色关闭失败。");
            await Change("WordHighlight", true); await RefreshState();
            if (!_settings.WordHighlight) throw new InvalidOperationException("歌词进度涂色开启失败。");
            await Change("WordHighlight", originalHighlight);
            var originalColors = new[] { _settings.ExpandedTextColor, _settings.ExpandedComponentColor, _settings.LyricProgressColor };
            foreach (var colorKey in new[] { "ExpandedTextColor", "ExpandedComponentColor", "LyricProgressColor" })
                if (!await Change(colorKey, "#E06090")) throw new InvalidOperationException("调色设置联动失败。");
            await RefreshState();
            if (_settings.ExpandedTextColor != "#E06090" || _settings.ExpandedComponentColor != "#E06090" || _settings.LyricProgressColor != "#E06090")
                throw new InvalidOperationException("颜色未保存。");
            await Change("ExpandedTextColor", originalColors[0]); await Change("ExpandedComponentColor", originalColors[1]); await Change("LyricProgressColor", originalColors[2]);
            var originalSources = _settings.AllowedSources.ToArray();
            await Change("AllowedSources", new[] { "QQMusic" }); await RefreshState();
            if (_settings.AllowedSources.Count != 1 || _settings.AllowedSources[0] != "QQMusic") throw new InvalidOperationException("白名单设置联动失败。");
            await Change("AllowedSources", originalSources.Concat(new[] { "CustomDemo.exe" }).ToArray()); await RefreshState();
            if (!_settings.AllowedSources.Contains("CustomDemo.exe")) throw new InvalidOperationException("自定义白名单添加失败。");
            await Change("AllowedSources", originalSources); await RefreshState();
            if (_settings.AllowedSources.Contains("CustomDemo.exe")) throw new InvalidOperationException("自定义白名单移除失败。");
            File.WriteAllText(Path.Combine(_smokeDirectory!, "winui-smoke.json"), JsonSerializer.Serialize(new { success = true, pages = _smokePages, liveSettings = true, autoStartToggle = true, topmostToggle = true, lyricProgressColorToggle = true, customWhitelist = true, independentColors = true, palette = true, systemAccent = true, changelog = true, singleMaterial = true, motionParameters = true }));
        }
        catch (Exception ex)
        {
            File.WriteAllText(Path.Combine(_smokeDirectory!, "winui-smoke.json"), JsonSerializer.Serialize(new { success = false, error = ex.ToString() }));
            Environment.ExitCode = 1;
        }
        finally { Close(); }
    }
    private string StateText(string key) => _state.ValueKind == JsonValueKind.Object && _state.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() ?? "" : "等待连接";
    private double StateNumber(string key) => _state.ValueKind == JsonValueKind.Object && _state.TryGetProperty(key, out var value) && value.TryGetDouble(out var number) ? number : 0;
    private bool StateBool(string key) => _state.ValueKind == JsonValueKind.Object && _state.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.True;
    private string PositionText() => $"播放位置  {TimeSpan.FromSeconds(StateNumber("position")):mm\\:ss}   ·   {StateText("timeline")}";
    private string PlayerText() => $"播放器：{StateText("player")}\n歌曲：{StateText("title")}\n歌手：{StateText("artist")}\n歌词来源：{StateText("lyricSource")}";
}

