using System.IO;
using System.Text.Json;
using TaskbarLyrics.Core.Abstractions;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;
using TaskbarLyrics.Core.Utilities;

namespace LyricIsland;

internal sealed record TrackDuration(TimeSpan Duration, string Provider, string CandidateId);

internal sealed class TrackDurationResolver : IDisposable
{
    private readonly Func<TrackInfo, CancellationToken, Task<TrackDuration?>> _lookup;
    private readonly string? _cachePath;
    private readonly Dictionary<string, TrackDuration> _cache = new(StringComparer.Ordinal);
    private string _identity = "";
    private Task<TrackDuration?>? _pending;
    private CancellationTokenSource? _request;
    private DateTimeOffset _retryAt;
    private bool _disposed;
    public string Source { get; private set; } = "等待歌曲信息";

    public TrackDurationResolver(string? cachePath = null, Func<TrackInfo, CancellationToken, Task<TrackDuration?>>? lookup = null)
    {
        _cachePath = cachePath; _lookup = lookup ?? LookupAsync;
        try
        {
            if (cachePath is not null && File.Exists(cachePath))
                foreach (var pair in (JsonSerializer.Deserialize<Dictionary<string, TrackDuration>>(File.ReadAllText(cachePath)) ?? []).TakeLast(256))
                    if (pair.Value is not null && Valid(pair.Value.Duration)) _cache[pair.Key] = pair.Value;
        }
        catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { Log.Warn($"Duration cache: {ex.Message}"); }
    }

    public PlaybackSnapshot Enrich(PlaybackSnapshot snapshot)
    {
        if (_disposed) return snapshot;
        var identity = snapshot.Track is null ? "" : LyricSyncService.BuildStableTrackIdentity(snapshot.Track);
        if (identity != _identity)
        {
            _request?.Cancel(); _request?.Dispose(); _request = null; _pending = null;
            _identity = identity; _retryAt = DateTimeOffset.MinValue;
        }
        if (snapshot.Track is not { } track) { Source = "等待歌曲信息"; return snapshot; }
        if (Valid(track.Duration)) { Source = "播放器"; return snapshot; }
        if (_pending is { IsCompleted: true })
        {
            if (_pending.IsCompletedSuccessfully && _pending.Result is { } result && Valid(result.Duration))
            {
                _cache[identity] = result;
                while (_cache.Count > 256) _cache.Remove(_cache.Keys.First());
                Save();
            }
            _pending = null; _request?.Dispose(); _request = null;
            _retryAt = DateTimeOffset.UtcNow.AddSeconds(30);
        }
        if (_cache.TryGetValue(identity, out var cached))
        {
            Source = cached.Provider + "歌曲信息";
            return snapshot with { Track = track with { Duration = cached.Duration } };
        }
        Source = "正在补取歌曲时长";
        if (_pending is null && DateTimeOffset.UtcNow >= _retryAt)
        {
            _request = new CancellationTokenSource();
            var token = _request.Token;
            _pending = Task.Run(() => _lookup(track, token), token);
            _ = _pending.ContinueWith(task => _ = task.Exception, CancellationToken.None, TaskContinuationOptions.OnlyOnFaulted, TaskScheduler.Default);
        }
        return snapshot;
    }

    internal static bool Valid(TimeSpan duration) => duration > TimeSpan.Zero && duration <= TimeSpan.FromHours(24);

    internal static SourceTrackCandidate? Select(TrackInfo track, IEnumerable<SourceTrackCandidate> candidates)
    {
        var identity = TrackIdentity.FromTrackInfo(track) with { SongId = null, Duration = TimeSpan.Zero };
        return candidates.Where(candidate => Valid(candidate.Duration))
            .Select(candidate => (Candidate: candidate, Evaluation: LyricIdentityEvaluator.Evaluate(identity, candidate)))
            .Where(item => item.Evaluation.IsAdmitted && item.Evaluation.Score >= LyricMatchingPolicy.ImmediateAcceptanceScore)
            .OrderByDescending(item => item.Evaluation.Score).Select(item => item.Candidate).FirstOrDefault();
    }

    internal static async Task<TrackDuration?> LookupAsync(TrackInfo track, CancellationToken token)
    {
        if (string.IsNullOrWhiteSpace(track.Title)) return null;
        var identity = TrackIdentity.FromTrackInfo(track) with { SongId = null, Duration = TimeSpan.Zero };
        var plan = new LyricSearchPlan(identity, [new SearchQueryVariant("exact", identity.Title, identity.Artists, identity.Album, TimeSpan.Zero, [])]);
        ILyricSource[] sources = track.SourceApp.Contains("Netease", StringComparison.OrdinalIgnoreCase)
            ? [new NeteaseLyricSource(), new QqMusicLyricSource(), new LrcLibLyricSource()]
            : [new QqMusicLyricSource(), new NeteaseLyricSource(), new LrcLibLyricSource()];
        foreach (var source in sources)
        {
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(token); timeout.CancelAfter(TimeSpan.FromSeconds(5));
            try
            {
                var candidates = await source.SearchAsync(plan, timeout.Token).ConfigureAwait(false);
                if (Select(track, candidates) is { } selected) return new(selected.Duration, source.ProviderId.Value, selected.CandidateId);
            }
            catch (OperationCanceledException) when (!token.IsCancellationRequested) { }
            catch (OperationCanceledException) { throw; }
            catch (Exception ex) { Log.Warn($"Duration lookup {source.ProviderId}: {ex.Message}"); }
        }
        return null;
    }
    private void Save()
    {
        if (_cachePath is null) return;
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_cachePath)!);
            var temporary = _cachePath + ".tmp";
            File.WriteAllText(temporary, JsonSerializer.Serialize(_cache)); File.Move(temporary, _cachePath, true);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { Log.Warn($"Duration cache save: {ex.Message}"); }
    }
    public void Dispose() { _disposed = true; _request?.Cancel(); _request?.Dispose(); }
}
