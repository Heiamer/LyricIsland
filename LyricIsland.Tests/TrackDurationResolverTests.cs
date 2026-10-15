using System.IO;
using TaskbarLyrics.Core.Models;
using Xunit;

namespace LyricIsland.Tests;

public sealed class TrackDurationResolverTests
{
    private static readonly TrackInfo Track = new("id", "8.8", "あたらよ", "", "Netease", TimeSpan.Zero);
    private static SourceTrackCandidate Candidate(string artist, double seconds, string title = "8.8") =>
        new(new LyricProviderId("Netease"), "candidate", title, [artist], "", TimeSpan.FromSeconds(seconds), "exact", new Dictionary<string, string>());
    [Fact]
    public void ChoosesDurationOnlyForReliableSongIdentity()
    {
        var good = Candidate("あたらよ", 236);
        Assert.Same(good, TrackDurationResolver.Select(Track, [Candidate("unrelated", 90), good]));
        Assert.Null(TrackDurationResolver.Select(Track, [Candidate("unrelated", 90), Candidate("あたらよ", 0), Candidate("あたらよ", 236, "another song")]));
    }
    [Fact]
    public async Task MissingSystemDurationIsRecoveredAndPersistedWithoutResettingPlayback()
    {
        var folder = Path.Combine(Path.GetTempPath(), "island-duration-" + Guid.NewGuid().ToString("N"));
        var path = Path.Combine(folder, "duration.json");
        try
        {
            var complete = new TaskCompletionSource<TrackDuration?>();
            using var resolver = new TrackDurationResolver(path, (_, _) => complete.Task);
            var snapshot = new PlaybackSnapshot(true, TimeSpan.FromSeconds(83), Track);
            Assert.Equal(TimeSpan.Zero, resolver.Enrich(snapshot).Track!.Duration);
            complete.SetResult(new(TimeSpan.FromSeconds(236), "Netease", "song-id"));
            PlaybackSnapshot result = snapshot;
            for (var i = 0; i < 100; i++) { result = resolver.Enrich(snapshot); if (result.Track!.Duration > TimeSpan.Zero) break; await Task.Delay(5); }
            Assert.Equal(TimeSpan.FromSeconds(236), result.Track!.Duration); Assert.Equal(snapshot.Position, result.Position);
            var presentation = PlaybackPresentation.From(result);
            Assert.Equal("03:56", presentation.Total); Assert.Equal(83d / 236, presentation.Fraction, 6);
            using var restored = new TrackDurationResolver(path, (_, _) => throw new InvalidOperationException("Should use cache"));
            Assert.Equal(TimeSpan.FromSeconds(236), restored.Enrich(snapshot).Track!.Duration);
        }
        finally { if (Directory.Exists(folder)) Directory.Delete(folder, true); }
    }
    [Fact]
    public async Task StaleResultFromPreviousSongIsNeverAppliedToNextSong()
    {
        var firstStarted = new TaskCompletionSource(); var secondStarted = new TaskCompletionSource();
        var firstComplete = new TaskCompletionSource<TrackDuration?>(); var secondComplete = new TaskCompletionSource<TrackDuration?>();
        using var resolver = new TrackDurationResolver(lookup: (track, _) =>
        {
            if (track.Id == "id") { firstStarted.TrySetResult(); return firstComplete.Task; }
            secondStarted.TrySetResult(); return secondComplete.Task;
        });
        var first = new PlaybackSnapshot(true, TimeSpan.Zero, Track);
        resolver.Enrich(first); await firstStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));
        var next = first with { Track = Track with { Title = "Other song", Id = "other" } };
        resolver.Enrich(next); await secondStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));
        firstComplete.SetResult(new(TimeSpan.FromSeconds(236), "Netease", "old-song"));
        await Task.Delay(20);
        Assert.Equal(TimeSpan.Zero, resolver.Enrich(next).Track!.Duration);
        secondComplete.SetResult(new(TimeSpan.FromSeconds(180), "Netease", "new-song"));
        PlaybackSnapshot result = next;
        for (var i = 0; i < 100; i++) { result = resolver.Enrich(next); if (result.Track!.Duration > TimeSpan.Zero) break; await Task.Delay(5); }
        Assert.Equal(TimeSpan.FromSeconds(180), result.Track!.Duration);
    }
    [Fact]
    public void SystemDurationTakesPriorityWithoutStartingNetworkLookup()
    {
        using var resolver = new TrackDurationResolver(lookup: (_, _) => throw new InvalidOperationException("Must not query"));
        var snapshot = new PlaybackSnapshot(true, TimeSpan.FromSeconds(83), Track with { Duration = TimeSpan.FromSeconds(240) });
        Assert.Same(snapshot, resolver.Enrich(snapshot)); Assert.Equal("播放器", resolver.Source);
    }
}
