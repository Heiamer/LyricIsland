namespace LyricIsland;
public sealed record IslandSpring
{
    public double Stiffness { get; set; } = 420;
    public double Damping { get; set; } = 30;
    public double Mass { get; set; } = .8;
    public void Normalize()
    {
        Stiffness = Finite(Stiffness, 420, 100, 1000);
        Damping = Finite(Damping, 30, 10, 80);
        Mass = Finite(Mass, .8, .3, 2);
    }
    internal static double Finite(double value, double fallback, double minimum, double maximum) =>
        double.IsFinite(value) ? Math.Clamp(value, minimum, maximum) : fallback;
}
public sealed record IslandMotionOptions
{
    public double Gap { get; set; } = 8;
    public double Duration { get; set; } = 360;
    public double CollapseDuration { get; set; } = 300;
    public double Radius { get; set; } = 30;
    public IslandSpring Spring { get; set; } = new();
    public void Normalize()
    {
        Gap = IslandSpring.Finite(Gap, 8, 0, 24);
        Duration = IslandSpring.Finite(Duration, 360, 250, 1200);
        CollapseDuration = IslandSpring.Finite(CollapseDuration, 300, 150, 700);
        Radius = IslandSpring.Finite(Radius, 30, 8, 48);
        Spring ??= new(); Spring.Normalize();
    }
}
