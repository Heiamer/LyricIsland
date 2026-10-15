using FontFamily = System.Windows.Media.FontFamily;

namespace LyricIsland;

internal static class BundledFonts
{
    public static FontFamily Body { get; } = new(new Uri(AppContext.BaseDirectory), "./Assets/Fonts/#OPPO Sans 4.0");
}
