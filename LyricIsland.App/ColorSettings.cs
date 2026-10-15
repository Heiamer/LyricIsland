using System.Globalization;
namespace LyricIsland;
internal static class ColorSettings
{
    public const string DefaultText = "#F4F6F8";
    public const string DefaultComponent = "#B1E8D1";
    public const string DefaultProgress = "#A9E6CD";
    public static bool TryNormalize(string? input, out string color)
    {
        color = string.Empty;
        var hex = input?.Trim().TrimStart('#');
        if (hex?.Length != 6 || !uint.TryParse(hex, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out _)) return false;
        color = "#" + hex.ToUpperInvariant(); return true;
    }
    public static string Normalize(string? input, string fallback) => TryNormalize(input, out var color) ? color : fallback;
}