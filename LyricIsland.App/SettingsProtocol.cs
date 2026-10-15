using System.IO;
using System.IO.Pipes;
using System.Text.Json;

namespace LyricIsland;

internal static class SettingsProtocol
{
    public static string PipeName { get; set; } = "LyricIsland.Settings.v3";
    public static async Task<JsonElement> SendAsync(string command, object? value = null)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(8));
        await using var pipe = new NamedPipeClientStream(".", PipeName, PipeDirection.InOut, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        await pipe.ConnectAsync(timeout.Token);
        using var writer = new StreamWriter(pipe, leaveOpen: true) { AutoFlush = true };
        using var reader = new StreamReader(pipe, leaveOpen: true);
        await writer.WriteLineAsync(JsonSerializer.Serialize(new { version = 1, command, value }));
        var line = await reader.ReadLineAsync(timeout.Token) ?? throw new IOException("歌词程序没有返回状态。");
        using var document = JsonDocument.Parse(line);
        return document.RootElement.Clone();
    }
    public static bool TryReadSetting(JsonElement input, IslandSettings settings)
    {
        if (input.ValueKind != JsonValueKind.Object || !input.TryGetProperty("key", out var key) || key.ValueKind != JsonValueKind.String || !input.TryGetProperty("value", out var value)) return false;
        switch (key.GetString())
        {
            case "CompactWidth": if (!ReadDouble(value, 140, 480, out var width)) return false; settings.CompactWidth = width; break;
            case "CardMaterial": if (value.ValueKind != JsonValueKind.String || !IslandMaterials.IsValid(value.GetString())) return false; settings.CardMaterial = value.GetString()!; break;
            case "MotionGap": if (!ReadDouble(value, 0, 24, out var gap)) return false; settings.Motion.Gap = gap; break;
            case "MotionDuration": if (!ReadDouble(value, 250, 1200, out var duration)) return false; settings.Motion.Duration = duration; break;
            case "MotionCollapseDuration": if (!ReadDouble(value, 150, 700, out var collapseDuration)) return false; settings.Motion.CollapseDuration = collapseDuration; break;
            case "MotionRadius": if (!ReadDouble(value, 8, 48, out var radius)) return false; settings.Motion.Radius = radius; break;
            case "SpringStiffness": if (!ReadDouble(value, 100, 1000, out var stiffness)) return false; settings.Motion.Spring.Stiffness = stiffness; break;
            case "SpringDamping": if (!ReadDouble(value, 10, 80, out var damping)) return false; settings.Motion.Spring.Damping = damping; break;
            case "SpringMass": if (!ReadDouble(value, .3, 2, out var mass)) return false; settings.Motion.Spring.Mass = mass; break;
            case "FontSize": if (!ReadDouble(value, 11, 17, out var font)) return false; settings.FontSize = font; break;
            case "OffsetMilliseconds": if (!ReadDouble(value, -5000, 5000, out var offset)) return false; settings.OffsetMilliseconds = (int)offset; break;
            case "Floating": if (!ReadBool(value, out var floating)) return false; settings.Floating = floating; break;
            case "Animation": if (!ReadBool(value, out var animation)) return false; settings.Animation = animation; break;
            case "HoverExpand": if (!ReadBool(value, out var hover)) return false; settings.HoverExpand = hover; break;
            case "Translation": if (!ReadBool(value, out var translation)) return false; settings.Translation = translation; break;
            case "WordHighlight": if (!ReadBool(value, out var word)) return false; settings.WordHighlight = word; break;
            case "ExpandedTextColor": if (value.ValueKind != JsonValueKind.String || !ColorSettings.TryNormalize(value.GetString(), out var textColor)) return false; settings.ExpandedTextColor = textColor; break;
            case "ExpandedComponentColor": if (value.ValueKind != JsonValueKind.String || !ColorSettings.TryNormalize(value.GetString(), out var componentColor)) return false; settings.ExpandedComponentColor = componentColor; break;
            case "LyricProgressColor": if (value.ValueKind != JsonValueKind.String || !ColorSettings.TryNormalize(value.GetString(), out var progressColor)) return false; settings.LyricProgressColor = progressColor; break;
            case "AlwaysOnTop": if (!ReadBool(value, out var top)) return false; settings.AlwaysOnTop = top; break;
            case "AutoStart": if (!ReadBool(value, out var startup)) return false; settings.AutoStart = startup; break;
            case "OnlineSongInfo": if (!ReadBool(value, out var online)) return false; settings.OnlineSongInfo = online; break;
            case "HideWhenPaused": if (!ReadBool(value, out var hidePaused)) return false; settings.HideWhenPaused = hidePaused; break;
            case "AllowedSources":
                if (value.ValueKind != JsonValueKind.Array || value.GetArrayLength() > 32 || value.EnumerateArray().Any(x => x.ValueKind != JsonValueKind.String || string.IsNullOrWhiteSpace(x.GetString()) || x.GetString()!.Length > 256)) return false;
                settings.AllowedSources = value.EnumerateArray().Select(x => x.GetString()!.Trim()).Distinct(StringComparer.OrdinalIgnoreCase).ToList(); break;
            default: return false;
        }
        return true;
    }
    private static bool ReadDouble(JsonElement input, double min, double max, out double value)
    {
        value = 0; return input.ValueKind == JsonValueKind.Number && input.TryGetDouble(out value) && double.IsFinite(value) && value >= min && value <= max;
    }
    private static bool ReadBool(JsonElement input, out bool value)
    {
        value = input.ValueKind == JsonValueKind.True; return input.ValueKind is JsonValueKind.True or JsonValueKind.False;
    }
}
