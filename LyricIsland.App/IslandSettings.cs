using System.IO;
using System.Text.Json;

namespace LyricIsland;

public sealed record IslandSettings
{
    public static readonly (string Id, string Name)[] SupportedPlayers = [("Netease", "网易云音乐"), ("QQMusic", "QQ 音乐"), ("Kugou", "酷狗音乐"), ("Kuwo", "酷我音乐"), ("Spotify", "Spotify"), ("TwilightEcho", "TwilightEcho"), ("Foobar2000", "Foobar2000"), ("MusicBee", "MusicBee"), ("AIMP", "AIMP"), ("Browser", "浏览器媒体")];
    public double CompactWidth { get; set; } = 140;
    public string CardMaterial { get; set; } = "Default";
    public IslandMotionOptions Motion { get; set; } = new();
    public double FontSize { get; set; } = 12.5;
    public bool Floating { get; set; }
    public bool Translation { get; set; } = true;
    public bool Animation { get; set; } = true;
    public int MotionPreferenceVersion { get; set; }
    public bool HoverExpand { get; set; }
    public bool ClickAnywhereExpand { get; set; }
    public bool WordHighlight { get; set; } = true;
    public bool AlwaysOnTop { get; set; } = true;
    public bool AutoStart { get; set; }
    public bool OnlineSongInfo { get; set; }
    public bool HideWhenPaused { get; set; } = true;
    public List<string> AllowedSources { get; set; } = ["Netease", "QQMusic", "Kugou", "Kuwo", "Spotify", "TwilightEcho"];
    public int PlayerWhitelistVersion { get; set; }
    public string ExpandedTextColor { get; set; } = ColorSettings.DefaultText;
    public string ExpandedComponentColor { get; set; } = ColorSettings.DefaultComponent;
    public string LyricProgressColor { get; set; } = ColorSettings.DefaultProgress;
    public double CenterX { get; set; } = 0.5;
    public double TopFraction { get; set; } = 0.08;
    public string Display { get; set; } = string.Empty;
    public int OffsetMilliseconds { get; set; }
    public List<string> LocalFolders { get; set; } = [];

    public void Normalize()
    {
        MotionPreferenceVersion = 1;
        CompactWidth = double.IsFinite(CompactWidth) ? Math.Clamp(CompactWidth, 140, 480) : 140;
        CardMaterial = IslandMaterials.Normalize(CardMaterial);
        Motion ??= new(); Motion.Normalize();
        FontSize = double.IsFinite(FontSize) ? Math.Clamp(FontSize, 11, 17) : 12.5;
        CenterX = double.IsFinite(CenterX) ? Math.Clamp(CenterX, 0, 1) : 0.5;
        TopFraction = double.IsFinite(TopFraction) ? Math.Clamp(TopFraction, 0, 1) : 0.08;
        OffsetMilliseconds = Math.Clamp(OffsetMilliseconds, -5000, 5000);
        LocalFolders = (LocalFolders ?? []).Where(Directory.Exists).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        Display ??= string.Empty;
        ExpandedTextColor = ColorSettings.Normalize(ExpandedTextColor, ColorSettings.DefaultText);
        ExpandedComponentColor = ColorSettings.Normalize(ExpandedComponentColor, ColorSettings.DefaultComponent);
        LyricProgressColor = ColorSettings.Normalize(LyricProgressColor, ColorSettings.DefaultProgress);
        if (PlayerWhitelistVersion < 1)
        {
            AllowedSources ??= [];
            if (!AllowedSources.Contains("TwilightEcho", StringComparer.OrdinalIgnoreCase)) AllowedSources.Insert(0, "TwilightEcho");
            PlayerWhitelistVersion = 1;
        }
        AllowedSources = (AllowedSources ?? []).Where(x => !string.IsNullOrWhiteSpace(x) && x.Length <= 256)
            .Select(x => x.Trim()).Distinct(StringComparer.OrdinalIgnoreCase).Take(32).ToList();
    }
}

internal sealed class SettingsStore(string path)
{
    public static string DataDirectory => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "LyricIsland");
    public static SettingsStore Default => new(Path.Combine(DataDirectory, "settings.json"));

    public IslandSettings Load()
    {
        try
        {
            var settings = File.Exists(path) ? JsonSerializer.Deserialize<IslandSettings>(File.ReadAllText(path)) ?? new() : new();
            // This upgrade restores the requested default; subsequent app-switch choices persist.
            if (settings.MotionPreferenceVersion < 1) settings.Animation = true;
            settings.Normalize();
            return settings;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException)
        {
            TaskbarLyrics.Core.Utilities.Log.Warn($"Settings read failed: {ex.Message}");
            return new();
        }
    }

    public bool Save(IslandSettings settings)
    {
        try
        {
            settings.Normalize();
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            var temporary = path + ".tmp";
            File.WriteAllText(temporary, JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(temporary, path, true);
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            TaskbarLyrics.Core.Utilities.Log.Warn($"Settings save failed: {ex.Message}");
            return false;
        }
    }
}

