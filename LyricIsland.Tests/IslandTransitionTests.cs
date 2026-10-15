using System.Text.Json;
using Xunit;
namespace LyricIsland.Tests;
public sealed class IslandTransitionTests
{
    [Fact]
    public void OpeningRespondsOnFirstFrameAndSettlesWithControlledBounce()
    {
        var opening = new IslandTransition(true, IslandTransitionFrame.Collapsed);
        Assert.Equal(IslandTransitionFrame.Collapsed, opening.Sample(0));
        Assert.True(opening.Sample(8).CardReveal > 0);
        Assert.True(opening.Sample(8).ContentOpacity > 0);
        var previous = opening.Sample(0);
        for (var time = 5d; time < opening.DurationMilliseconds; time += 5)
        {
            var frame = opening.Sample(time);
            Assert.InRange(frame.CardReveal, 0, 1.035);
            Assert.InRange(frame.CardBlur, 0, 6);
            Assert.InRange(frame.CardScale, .98, 1.003);
            Assert.Equal(1, frame.CapsuleScaleY);
            previous = frame;
        }
        Assert.Equal(IslandTransitionFrame.Expanded, opening.Sample(opening.DurationMilliseconds));
        Assert.InRange(Math.Abs(previous.CardOffsetY), 0, .01);
    }
    [Fact]
    public void ClosingUsesTheSameSpatialOrigin()
    {
        var closing = new IslandTransition(false, IslandTransitionFrame.Expanded);
        Assert.Equal(IslandTransitionFrame.Expanded, closing.Sample(0));
        var early = closing.Sample(16);
        Assert.True(early.ContentOpacity < 1); Assert.True(early.CardReveal < 1);
        Assert.True(early.CardOffsetY < 0); Assert.True(early.ContentOffsetY > 0);
        Assert.Equal(IslandTransitionFrame.Collapsed, closing.Sample(closing.DurationMilliseconds));
    }
    [Theory]
    [InlineData(8)] [InlineData(60)] [InlineData(175)] [InlineData(300)] [InlineData(500)]
    public void ReversalPreservesPositionAndVelocity(double time)
    {
        var opening = new IslandTransition(true, IslandTransitionFrame.Collapsed);
        var frame = opening.Sample(time); var velocity = opening.Velocity(time);
        var closing = new IslandTransition(false, frame, velocity: velocity);
        Assert.Equal(frame, closing.Sample(0)); Assert.Equal(velocity, closing.Velocity(0));
        // Finite differences across the seam detect velocity discontinuities.
        var before = (opening.Sample(time).CardOffsetY - opening.Sample(time - .001).CardOffsetY) * 1000000;
        var after = (closing.Sample(.001).CardOffsetY - frame.CardOffsetY) * 1000000;
        Assert.InRange(Math.Abs(after - before), 0, .1);
    }
    [Fact]
    public void RepeatedInterruptionsRemainFiniteAndEventuallySettle()
    {
        var transition = new IslandTransition(true, IslandTransitionFrame.Collapsed);
        for (var i = 0; i < 80; i++)
        {
            var frame = transition.Sample(17); var velocity = transition.Velocity(17);
            Assert.True(double.IsFinite(frame.CardOffsetY));
            Assert.InRange(frame.CardReveal, 0, 1.035); Assert.InRange(frame.CardOpacity, 0, 1);
            transition = new IslandTransition(i % 2 != 0, frame, velocity: velocity);
        }
        Assert.Equal(transition.End, transition.Sample(transition.DurationMilliseconds));
    }
    [Theory]
    [InlineData(100, 80, 2)] [InlineData(1000, 10, .3)] [InlineData(420, 34, .8)]
    public void CustomSpringsRemainFiniteAndReachTheirEndpoint(double stiffness, double damping, double mass)
    {
        var options = new IslandMotionOptions { Spring = new() { Stiffness = stiffness, Damping = damping, Mass = mass } };
        var transition = new IslandTransition(true, IslandTransitionFrame.Collapsed, options);
        for (var time = 0d; time < transition.DurationMilliseconds; time += 17)
        {
            var frame = transition.Sample(time);
            Assert.True(double.IsFinite(frame.CardScale)); Assert.True(double.IsFinite(frame.CardOffsetY));
            Assert.InRange(frame.CardReveal, 0, 1.035); Assert.InRange(frame.CardBlur, 0, 6);
        }
        Assert.Equal(IslandTransitionFrame.Expanded, transition.Sample(transition.DurationMilliseconds));
    }
    [Theory]
    [InlineData(true)] [InlineData(false)]
    public void ReducedMotionOnlyCrossFades(bool expanded)
    {
        var transition = new IslandTransition(expanded, expanded ? IslandTransitionFrame.Collapsed : IslandTransitionFrame.Expanded, reducedMotion: true);
        foreach (var time in new[] { 0, 30, 60, 90 })
        {
            var frame = transition.Sample(time);
            Assert.Equal(0, frame.CardOffsetY); Assert.Equal(1, frame.CardScale);
            Assert.Equal(0, frame.CardBlur); Assert.Equal(0, frame.ContentOffsetY);
        }
        Assert.Equal(transition.End, transition.Sample(120));
    }
    [Theory]
    [InlineData("Mica")] [InlineData("Acrylic")] [InlineData("unknown")] [InlineData(null)]
    public void OldMaterialsMigrateAndCannotBeSelected(string? material)
    {
        var settings = new IslandSettings { CardMaterial = material! }; settings.Normalize();
        Assert.Equal("Default", settings.CardMaterial);
        Assert.False(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key = "CardMaterial", value = material }), settings));
    }
    [Fact]
    public void ParametersControlSpeedAndGeometry()
    {
        var options = new IslandMotionOptions { Gap = 12, Duration = 720, CollapseDuration = 600, Radius = 40 };
        var opening = new IslandTransition(true, IslandTransitionFrame.Hidden(options), options);
        Assert.Equal(-22, opening.Sample(0).CardOffsetY);
        var normal = new IslandTransition(true, IslandTransitionFrame.Hidden(options));
        Assert.Equal(normal.Sample(120), opening.Sample(240));
        var settings = new IslandSettings { Motion = options };
        Assert.True(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key = "CardMaterial", value = "Default" }), settings));
        var plan = IslandLayout.Plan(new System.Windows.Rect(0, 0, 1920, 1080), settings);
        Assert.Equal(plan.Capsule.Top, plan.Card.Top); Assert.True(plan.Host.Contains(plan.Card));
        Assert.False(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key = "MotionGap", value = 100 }), new()));
    }
}
