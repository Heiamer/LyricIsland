namespace LyricIsland;

internal readonly record struct IslandTransitionFrame(
    double CapsuleScaleY, double CardReveal, double CardOffsetY, double CardOpacity,
    double CardScale, double CardBlur, double ContentOpacity, double ContentOffsetY)
{
    public static IslandTransitionFrame Collapsed => Hidden(new IslandMotionOptions());
    public static IslandTransitionFrame Expanded => new(1, 1, 0, 1, 1, 0, 1, 0);
    public static IslandTransitionFrame Hidden(IslandMotionOptions options) => new(1, 0, -options.Gap - 10, 0, .98, 6, 0, 6);
}

// Independent analytic springs retain position AND velocity across every retarget.
// Values and velocities are in logical units and units/second, respectively.
internal sealed class IslandTransition
{
    private readonly IslandTransitionFrame _from, _velocity;
    private readonly IslandMotionOptions _options;
    private readonly bool _reducedMotion;
    private readonly double _rate;
    public IslandTransition(bool expanded, IslandTransitionFrame from, IslandMotionOptions? options = null,
        IslandTransitionFrame velocity = default, bool reducedMotion = false)
    {
        _from = from; _velocity = velocity; _reducedMotion = reducedMotion;
        _options = options ?? new();
        End = expanded ? IslandTransitionFrame.Expanded : IslandTransitionFrame.Hidden(_options);
        _rate = (expanded ? _options.Duration : _options.CollapseDuration) / 360;
        var spring = _options.Spring;
        var omega = Math.Sqrt(spring.Stiffness / spring.Mass);
        var zeta = spring.Damping / (2 * Math.Sqrt(spring.Stiffness * spring.Mass));
        var decay = zeta > 1 ? omega / (zeta + Math.Sqrt(zeta * zeta - 1)) : zeta * omega;
        // Settle below subpixel perception before detaching the render loop.
        DurationMilliseconds = reducedMotion ? 120 : Math.Max(1, 12 / decay * 1000 * _rate);
    }
    public double DurationMilliseconds { get; }
    public IslandTransitionFrame End { get; }
    public IslandTransitionFrame Sample(double milliseconds) => Evaluate(milliseconds, false);
    public IslandTransitionFrame Velocity(double milliseconds) => Evaluate(milliseconds, true);

    private IslandTransitionFrame Evaluate(double milliseconds, bool velocity)
    {
        if (milliseconds >= DurationMilliseconds) return velocity ? default : End;
        if (!_reducedMotion && milliseconds <= 0) return velocity ? _velocity : _from;
        var seconds = Math.Max(0, milliseconds) / 1000;
        if (_reducedMotion)
        {
            if (velocity) return default;
            var t = Math.Clamp(milliseconds / DurationMilliseconds, 0, 1);
            // Spatial geometry is static. A short cross-fade preserves feedback.
            var geometry = End.CardReveal > 0 ? End : IslandTransitionFrame.Expanded;
            return geometry with {
                CardOpacity = _from.CardOpacity + (End.CardOpacity - _from.CardOpacity) * t,
                ContentOpacity = _from.ContentOpacity + (End.ContentOpacity - _from.ContentOpacity) * t
            };
        }
        double Value(double from, double to, double initialVelocity, double speed = 1,
            double minimum = double.NegativeInfinity, double maximum = double.PositiveInfinity)
        {
            var (position, currentVelocity) = Spring(from, to, initialVelocity, seconds, speed / _rate);
            if (velocity) return position < minimum || position > maximum ? 0 : currentVelocity;
            return Math.Clamp(position, minimum, maximum);
        }
        return new(
            Value(_from.CapsuleScaleY, 1, _velocity.CapsuleScaleY),
            Value(_from.CardReveal, End.CardReveal, _velocity.CardReveal, minimum: 0, maximum: 1.035),
            Value(_from.CardOffsetY, End.CardOffsetY, _velocity.CardOffsetY),
            Value(_from.CardOpacity, End.CardOpacity, _velocity.CardOpacity, 1.5, 0, 1),
            Value(_from.CardScale, End.CardScale, _velocity.CardScale),
            Value(_from.CardBlur, End.CardBlur, _velocity.CardBlur, 1.2, 0, 6),
            Value(_from.ContentOpacity, End.ContentOpacity, _velocity.ContentOpacity, 1.3, 0, 1),
            Value(_from.ContentOffsetY, End.ContentOffsetY, _velocity.ContentOffsetY));
    }

    private (double Position, double Velocity) Spring(double from, double target, double velocity, double time, double speed)
    {
        var omega = Math.Sqrt(_options.Spring.Stiffness / _options.Spring.Mass) * speed;
        var zeta = _options.Spring.Damping / (2 * Math.Sqrt(_options.Spring.Stiffness * _options.Spring.Mass));
        var displacement = from - target;
        double x, v;
        if (zeta < .999999)
        {
            var decay = zeta * omega; var wd = omega * Math.Sqrt(1 - zeta * zeta);
            var b = (velocity + decay * displacement) / wd;
            var cos = Math.Cos(wd * time); var sin = Math.Sin(wd * time); var envelope = Math.Exp(-decay * time);
            x = envelope * (displacement * cos + b * sin);
            v = envelope * ((b * wd - decay * displacement) * cos - (displacement * wd + decay * b) * sin);
        }
        else if (zeta <= 1.000001)
        {
            var b = velocity + omega * displacement; var envelope = Math.Exp(-omega * time);
            x = (displacement + b * time) * envelope;
            v = (velocity - omega * b * time) * envelope;
        }
        else
        {
            var root = Math.Sqrt(zeta * zeta - 1);
            var r1 = -omega / (zeta + root); var r2 = -omega * (zeta + root);
            var a = (velocity - r2 * displacement) / (r1 - r2); var b = displacement - a;
            x = a * Math.Exp(r1 * time) + b * Math.Exp(r2 * time);
            v = a * r1 * Math.Exp(r1 * time) + b * r2 * Math.Exp(r2 * time);
        }
        return (target + x, v);
    }
}
