using System.IO;
using LyricIsland.Media;
using TaskbarLyrics.Core.Abstractions;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;

namespace LyricIsland;

internal sealed class MusicEngine : IDisposable
{
    public SmtcMusicSessionProvider Media { get; } = new();
    private readonly IResolvedLyricCache _cache = JsonResolvedLyricCache.CreateDefault();
    private readonly IslandSettings _settings;
    private readonly LyricSyncService _lyrics;
    private readonly TrackDurationResolver _duration = new(Path.Combine(SettingsStore.DataDirectory, "database", "track-durations-v1.json"));
    public string DurationSource => Clock.Status == "网易云实时进度" ? "网易云实时读取" : _duration.Source;
    public PlaybackClock Clock { get; } = new();
    private readonly NeteaseMemoryTimeline _nativeTimeline = new();
    private readonly TrackArtworkResolver _artwork = new();
    public string ArtworkIdentity => _artwork.Identity;
    public object ArtworkDiagnostics => new { _nativeTimeline.Status, _nativeTimeline.LastReadError, _nativeTimeline.LastSample };
    public string NativeTimelineStatus => _nativeTimeline.Status;
    public bool CanSeek(PlaybackSnapshot playback) => playback.CanSeek;
    private readonly PlaybackSnapshotStabilityGate _stability = new();
    public PlaybackSnapshot? LatestPlayback { get; private set; }
    public string LyricSource => _lyrics.CurrentLyricSourceApp ?? "等待获取";
    private string _allowedSourcesKey = string.Empty;
    private bool IsAllowed(PlaybackSnapshot playback) => Media.CanUseSource(
        string.IsNullOrWhiteSpace(playback.SourceAppUserModelId) ? playback.Track?.SourceApp : playback.SourceAppUserModelId);
    public void ApplySettings()
    {
        var key = string.Join("\n", _settings.AllowedSources.Order(StringComparer.OrdinalIgnoreCase));
        if (key == _allowedSourcesKey) return;
        _allowedSourcesKey = key;
        Media.SetRecognitionOrder(_settings.AllowedSources, _settings.AllowedSources, _settings.AllowedSources);
        if (LatestPlayback is { Track: not null } previous && !IsAllowed(previous))
            LatestPlayback = previous with { Track = null, IsPlaying = false, CoverImageBytes = null };
    }

    public MusicEngine(IslandSettings settings)
    {
        _settings = settings;
        _nativeTimeline.AllowOnlineMetadata = settings.OnlineSongInfo;
        _allowedSourcesKey = "\0";
        ApplySettings();
        var coordinator = new LyricResolutionCoordinator(
            [new QqMusicLyricSource(), new NeteaseLyricSource(), new KugouLyricSource(), new LrcLibLyricSource()],
            [new LyricifyPayloadDecoder()], [new LyricifyPayloadParser()], LyricPipelineCache.CreateDefault(),
            localProvider: settings.LocalFolders.Count > 0 ? new LocalLyricProvider(settings.LocalFolders) : null,
            resolvedLyricCache: _cache);
        _lyrics = new LyricSyncService(coordinator, _ => TimeSpan.FromMilliseconds(_settings.OffsetMilliseconds));
    }

    public async Task<(PlaybackSnapshot Playback, LyricDisplayFrame Lyrics)> ReadAsync(CancellationToken token)
    {
        using var readTimeout = CancellationTokenSource.CreateLinkedTokenSource(token);
        readTimeout.CancelAfter(TimeSpan.FromSeconds(2));
        var playback = await Media.GetCurrentAsync(readTimeout.Token);
        if (playback.Track is not null && !IsAllowed(playback))
            playback = playback with { Track = null, IsPlaying = false, CoverImageBytes = null };
        _nativeTimeline.AllowOnlineMetadata = _settings.OnlineSongInfo;
        playback = Media.RefreshPlaybackState(playback);
        token.ThrowIfCancellationRequested();
        if (!MediaPresenceState.HasSource(playback) || Media.GetLastTimelineDiagnostics()?.IsFallbackSnapshot == true ||
            Media.LastPlaybackStatus is "Stopped" or "Closed") playback = playback with { Track = null };
        var stable = _stability.Observe(playback, playback.Track is null ? PlaybackInputKind.NoPlayback : PlaybackInputKind.ValidTrack, DateTimeOffset.UtcNow);
        if (stable.Action == PlaybackSnapshotGateAction.Hold && LatestPlayback?.Track is { } previousTrack && IsAllowed(LatestPlayback))
            playback = playback with { Track = previousTrack, CoverImageBytes = LatestPlayback.CoverImageBytes, IsCoverLoading = LatestPlayback.IsCoverLoading };
        var timeline = Media.GetLastTimelineDiagnostics();
        _nativeTimeline.Request(playback.Track);
        if (UseNativeTimeline && _nativeTimeline.TryApply(playback, out var native, out var nativeTimeline))
        { playback = native; timeline = nativeTimeline; }
        playback = Clock.Observe(playback, timeline);
        playback = _duration.Enrich(playback);
        playback = _artwork.Enrich(playback, _nativeTimeline.CoverUrl(playback.Track));
        LatestPlayback = playback;
        var frame = await _lyrics.GetDisplayFrameAsync(playback);
        // Lyrics may involve network I/O. A pause received during that await must
        // win over the playback snapshot captured before the request started.
        var current = RefreshState() ?? playback;
        if (current.IsPlaying != playback.IsPlaying || Math.Abs((current.Position - playback.Position).TotalSeconds) > .1)
            frame = await _lyrics.GetDisplayFrameAsync(current);
        return (RefreshState() ?? current, frame);
    }

    public PlaybackSnapshot? RefreshState()
    {
        if (LatestPlayback is not { Track: not null } previous) return LatestPlayback;
        if (!IsAllowed(previous))
            return LatestPlayback = previous with { Track = null, IsPlaying = false, CoverImageBytes = null };
        var current = Media.RefreshPlaybackState(previous);
        _nativeTimeline.Request(previous.Track);
        if (UseNativeTimeline && _nativeTimeline.TryApply(current, out var native, out var timeline))
        {
            LatestPlayback = Clock.Observe(native, timeline);
            return LatestPlayback;
        }
        if (current.IsPlaying != previous.IsPlaying || Clock.IsLocal)
        {
            Clock.SetPlaying(current.IsPlaying);
            LatestPlayback = current with { Position = TimeSpan.FromSeconds(Clock.Position) };
        }
        return LatestPlayback;
    }
    private bool UseNativeTimeline => Media.GetLastTimelineDiagnostics() is not { IsFallbackSnapshot: false, LastUpdatedTimeUtc: var updated, RawPosition: var raw } ||
        updated <= DateTimeOffset.UnixEpoch || updated > DateTimeOffset.UtcNow.AddSeconds(5) || raw <= TimeSpan.Zero;

    public async Task<bool> TogglePlaybackAsync(CancellationToken token)
    {
        var before = RefreshState();
        if (before?.Track is null) return false;
        var accepted = await Media.TogglePlaybackAsync(token);
        if (!accepted) return false;
        // Observe the player acknowledgement, never infer it from the old button glyph.
        for (var i = 0; i < 30; i++)
        {
            if (i % 5 == 0) await Media.RefreshNativeSessionAsync(token);
            var after = RefreshState();
            if (after?.Track?.Id != before.Track.Id) return false;
            if (after.IsPlaying != before.IsPlaying) return true;
            await Task.Delay(40, token);
        }
        return false;
    }
    public async Task<bool> SetPlayingAsync(bool playing, CancellationToken token)
    {
        var before = LatestPlayback;
        if (before?.Track is null) return false;
        var accepted = await Media.SetPlayingAsync(playing, token);
        if (!accepted) return false;
        for (var i = 0; i < 30; i++)
        {
            if (i % 5 == 0) await Media.RefreshNativeSessionAsync(token);
            var after = RefreshState();
            if (after?.Track?.Id != before.Track.Id) return false;
            if (after.IsPlaying == playing) return true;
            await Task.Delay(40, token);
        }
        return false;
    }

    public async Task<bool> SeekAsync(double seconds, CancellationToken token)
    {
        var before = RefreshState();
        if (before?.Track is not { } track || !CanSeek(before) || track.Duration <= TimeSpan.Zero) return false;
        seconds = Math.Clamp(seconds, 0, Math.Max(0, track.Duration.TotalSeconds - .5));
        var accepted = await Media.TrySeekToAsync(TimeSpan.FromSeconds(seconds), token);
        if (!accepted) return false;
        for (var i = 0; i < 30; i++)
        {
            var current = RefreshState();
            if (current?.Track?.Id != track.Id) return false;
            if (Math.Abs(current.Position.TotalSeconds - seconds) < 2) return true;
            await Task.Delay(80, token);
        }
        return false;
    }
    public void Dispose()
    {
        _duration.Dispose();
        _nativeTimeline.Dispose();
        _artwork.Dispose();
        _lyrics.Dispose();
        Media.Dispose();
        (_cache as IDisposable)?.Dispose();
    }
}
