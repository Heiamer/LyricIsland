namespace LyricIsland.Media;

public interface ITimelinePositionStrategy
{
    string Name { get; }

    bool CanApply(SmtcTimelineDiagnostics diagnostics);

    TimeSpan SelectPosition(SmtcTimelineDiagnostics diagnostics);
}
