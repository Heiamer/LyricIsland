using System.Text;
using TaskbarLyrics.Core.Models;
namespace LyricIsland.Media;
internal static class CoverIdentity
{
    public static string FromTrack(TrackInfo? track)
    {
        return track is null
            ? string.Empty
            : FromMetadata(track.SourceApp, track.SongId, track.Title, track.Artist, track.Album);
    }

    public static string FromMetadata(
        string? sourceApp,
        string? songId,
        string? title,
        string? artist,
        string? album)
    {
        var normalizedSource = Normalize(sourceApp);
        var normalizedSongId = Normalize(songId);
        return !string.IsNullOrEmpty(normalizedSongId)
            ? Compose("song", normalizedSource, normalizedSongId)
            : Compose(
                "metadata",
                normalizedSource,
                Normalize(title),
                Normalize(artist),
                Normalize(album));
    }

    private static string Compose(string scope, params string[] parts)
    {
        var builder = new StringBuilder(scope);
        foreach (var part in parts)
        {
            builder.Append('|').Append(part.Length).Append(':').Append(part);
        }

        return builder.ToString();
    }

    private static string Normalize(string? value)
    {
        return string.IsNullOrWhiteSpace(value)
            ? string.Empty
            : string.Join(" ", value.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries));
    }
}

internal sealed class CoverVisualTransitionState
{
    internal static readonly TimeSpan DefaultRetention = TimeSpan.FromMilliseconds(1500);

    /// <summary>
    /// Players can report a new track several hundred milliseconds before publishing its artwork.
    /// </summary>
    internal static readonly TimeSpan DefaultFallbackSettleDelay = TimeSpan.FromMilliseconds(800);

    private readonly TimeSpan _retention;
    private readonly TimeSpan _fallbackSettleDelay;
    private readonly Func<DateTimeOffset> _utcNow;
    private DateTimeOffset _transitionDeadlineUtc;
    private DateTimeOffset _fallbackSettleDeadlineUtc;

    internal CoverVisualTransitionState()
        : this(DefaultRetention, DefaultFallbackSettleDelay, () => DateTimeOffset.UtcNow)
    {
    }

    internal CoverVisualTransitionState(TimeSpan retention, Func<DateTimeOffset> utcNow)
        : this(retention, DefaultFallbackSettleDelay, utcNow)
    {
    }

    internal CoverVisualTransitionState(
        TimeSpan retention,
        TimeSpan fallbackSettleDelay,
        Func<DateTimeOffset> utcNow)
    {
        ArgumentOutOfRangeException.ThrowIfLessThan(retention, TimeSpan.Zero);
        ArgumentOutOfRangeException.ThrowIfLessThan(fallbackSettleDelay, TimeSpan.Zero);

        _retention = retention;
        _fallbackSettleDelay = fallbackSettleDelay;
        _utcNow = utcNow ?? throw new ArgumentNullException(nameof(utcNow));
    }

    internal string RequestedIdentity { get; private set; } = string.Empty;

    internal string VisualIdentity { get; private set; } = string.Empty;

    internal bool Begin(string? identity)
    {
        var normalizedIdentity = (identity ?? string.Empty).Trim();
        if (string.Equals(RequestedIdentity, normalizedIdentity, StringComparison.Ordinal))
        {
            return false;
        }

        RequestedIdentity = normalizedIdentity;
        var now = _utcNow();
        _transitionDeadlineUtc = string.IsNullOrEmpty(normalizedIdentity)
            ? now
            : now + _retention;
        _fallbackSettleDeadlineUtc = string.IsNullOrEmpty(normalizedIdentity)
            ? now
            : now + _fallbackSettleDelay;
        return true;
    }

    internal bool IsVisualFor(string? identity)
    {
        return string.Equals(VisualIdentity, identity ?? string.Empty, StringComparison.Ordinal);
    }

    internal bool ShouldRetainPreviousVisual()
    {
        return !string.IsNullOrEmpty(RequestedIdentity) &&
               !IsVisualFor(RequestedIdentity) &&
               _utcNow() < _transitionDeadlineUtc;
    }

    /// <summary>
    /// True right after a track change while its artwork may still be arriving. The caller holds the
    /// previous visual instead of switching to the fallback icon, so a late cover cannot flash the
    /// fallback in and immediately replace it.
    /// </summary>
    internal bool ShouldDebounceFallback()
    {
        return VisualIdentity.Length > 0 &&
               !string.IsNullOrEmpty(RequestedIdentity) &&
               !IsVisualFor(RequestedIdentity) &&
               _utcNow() < _fallbackSettleDeadlineUtc;
    }

    internal void MarkVisual(string? identity)
    {
        VisualIdentity = (identity ?? string.Empty).Trim();
    }
}

