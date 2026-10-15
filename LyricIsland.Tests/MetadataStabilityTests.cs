using LyricIsland.Media;
using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class MetadataStabilityTests
{
    [Fact]
    public void ArtistRegressionAroundPauseDoesNotRestartSongIdentity()
    {
        var now = DateTimeOffset.UtcNow; var gate = new PlaybackSnapshotStabilityGate();
        var track = new TrackInfo("id", "Song", "Artist", "", "Netease", TimeSpan.Zero);
        var playing = new PlaybackSnapshot(true, TimeSpan.FromSeconds(30), track);
        Assert.Equal(PlaybackSnapshotGateAction.Accept, gate.Observe(playing, PlaybackInputKind.ValidTrack, now).Action);
        var partial = playing with { IsPlaying = false, Track = track with { Id = "partial", Artist = "Unknown Artist" } };
        Assert.Equal(PlaybackSnapshotGateAction.Hold, gate.Observe(partial, PlaybackInputKind.ValidTrack, now.AddMilliseconds(100)).Action);
        var recovered = gate.Observe(playing with { IsPlaying = false }, PlaybackInputKind.ValidTrack, now.AddMilliseconds(200));
        Assert.Equal(PlaybackSnapshotGateAction.Accept, recovered.Action);
        Assert.Equal(PlaybackSnapshotGateReason.StableIdentityRestored, recovered.Reason);
        Assert.Same(track, recovered.Snapshot.Track);
    }
    [Fact]
    public void ARealTitleChangeIsAcceptedImmediately()
    {
        var now = DateTimeOffset.UtcNow; var gate = new PlaybackSnapshotStabilityGate();
        var snapshot = new PlaybackSnapshot(true, TimeSpan.Zero, new TrackInfo("first", "First", "Artist", "", "Netease", TimeSpan.Zero));
        gate.Observe(snapshot, PlaybackInputKind.ValidTrack, now);
        var next = snapshot with { Track = snapshot.Track! with { Title = "Next", Id = "next" } };
        Assert.Equal(PlaybackSnapshotGateAction.Accept, gate.Observe(next, PlaybackInputKind.ValidTrack, now.AddMilliseconds(10)).Action);
    }
}
