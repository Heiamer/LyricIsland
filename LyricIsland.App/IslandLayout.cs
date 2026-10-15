using System.Windows;
using Size = System.Windows.Size;
namespace LyricIsland;
internal sealed class ExpansionState
{
    public bool Expanded { get; private set; }
    public bool HeldOpen { get; private set; }
    public void ClickCover() { HeldOpen = !HeldOpen; Expanded = HeldOpen; }
    public void HoverEnter() { Expanded = true; }
    public bool HoverLeave() { if (HeldOpen) return false; Expanded = false; return true; }
    public void Close() { Expanded = false; HeldOpen = false; }
}
internal readonly record struct IslandLayoutPlan(Rect Capsule, Rect Card, Rect Host);
internal static class IslandLayout
{
    internal const double ShadowPadding = 24;
    public static Size Measure(IslandSettings settings, bool expanded) => new(
        expanded ? Math.Max(340, settings.CompactWidth + 48) : settings.CompactWidth, expanded ? 200 : 40);
    public static IslandLayoutPlan Plan(Rect work, IslandSettings settings)
    {
        var padding = Math.Min(ShadowPadding, Math.Min(work.Width, work.Height) / 8);
        var availableWidth = Math.Max(1, work.Width - 2 * padding);
        var capsuleWidth = Math.Min(settings.CompactWidth, availableWidth);
        var cardWidth = Math.Min(Measure(settings, true).Width, availableWidth);
        var cardHeight = Math.Min(200, Math.Max(1, work.Height - 2 * padding));
        var hostWidth = Math.Max(capsuleWidth, cardWidth) + 2 * padding;
        var hostHeight = Math.Max(40, cardHeight) + 2 * padding;
        var center = work.Left + work.Width * (settings.Floating ? settings.CenterX : .5);
        var left = Math.Clamp(center - hostWidth / 2, work.Left, Math.Max(work.Left, work.Right - hostWidth));
        var capsuleTop = settings.Floating ? work.Top + (work.Height - 40) * settings.TopFraction : work.Top;
        // Shadow space is outside the work area; the visible surface starts at its top edge.
        var top = settings.Floating
            ? Math.Clamp(capsuleTop - padding, work.Top, Math.Max(work.Top, work.Bottom - hostHeight))
            : work.Top - padding;
        var host = new Rect(left, top, hostWidth, hostHeight);
        var capsule = new Rect(host.Left + (hostWidth - capsuleWidth) / 2, host.Top + padding, capsuleWidth, 40);
        var card = new Rect(host.Left + (hostWidth - cardWidth) / 2, capsule.Top, cardWidth, cardHeight);
        return new(capsule, card, host);
    }
    internal static System.Windows.Media.RectangleGeometry Morph(Rect capsule, Rect card, double progress, double radius)
    {
        var p = Math.Clamp(progress, 0, 1.035);
        double Lerp(double a, double b) => a + (b - a) * p;
        var bounds = new Rect(Lerp(capsule.X, card.X), Lerp(capsule.Y, card.Y),
            Math.Max(1, Lerp(capsule.Width, card.Width)), Math.Max(1, Lerp(capsule.Height, card.Height)));
        var corner = Math.Clamp(Lerp(capsule.Height / 2, radius), 0, Math.Min(bounds.Width, bounds.Height) / 2);
        return new(bounds, corner, corner);
    }
    public static Rect Place(Rect work, Size size, IslandSettings settings)
    {
        var width = Math.Min(size.Width, work.Width); var height = Math.Min(size.Height, work.Height);
        var center = settings.Floating ? settings.CenterX : .5;
        return new(Math.Clamp(work.Left + work.Width * center - width / 2, work.Left, Math.Max(work.Left, work.Right - width)),
            Math.Clamp(settings.Floating ? work.Top + (work.Height - 40) * settings.TopFraction : work.Top, work.Top, Math.Max(work.Top, work.Bottom - height)), width, height);
    }
}
