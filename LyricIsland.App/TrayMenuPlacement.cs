namespace LyricIsland;
internal readonly record struct TrayMenuBounds(int Left, int Top, int Width, int Height)
{
    public int Right => Left + Width;
    public int Bottom => Top + Height;
}
internal static class TrayMenuPlacement
{
    public static TrayMenuBounds Place(int x, int y, int width, int height, TrayMenuBounds work, int gap)
    {
        var left = Math.Clamp(x + gap, work.Left, Math.Max(work.Left, work.Right - width));
        var top = y - height - gap;
        if (top < work.Top) top = y + gap;
        top = Math.Clamp(top, work.Top, Math.Max(work.Top, work.Bottom - height));
        return new(left, top, width, height);
    }
    public static double DistanceToPointer(TrayMenuBounds bounds, int x, int y)
    {
        var dx = Math.Max(bounds.Left - x, Math.Max(x - bounds.Right, 0));
        var dy = Math.Max(bounds.Top - y, Math.Max(y - bounds.Bottom, 0));
        return Math.Sqrt((double)dx * dx + (double)dy * dy);
    }
}