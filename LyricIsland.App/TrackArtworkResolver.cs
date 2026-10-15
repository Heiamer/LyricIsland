using System.Net.Http;
using TaskbarLyrics.Core.Models;

namespace LyricIsland;

internal sealed class TrackArtworkResolver(Func<Uri, CancellationToken, Task<byte[]?>>? fetch = null) : IDisposable
{
    private static readonly HttpClient Http = TaskbarLyrics.Core.DirectNetwork.CreateClient(TimeSpan.FromSeconds(5));
    private readonly Func<Uri, CancellationToken, Task<byte[]?>> _fetch = fetch ?? DownloadAsync;
    private string _key = string.Empty;
    private byte[]? _bytes;
    private Task<byte[]?>? _pending;
    private CancellationTokenSource? _request;
    private DateTimeOffset _retryAt;
    public string Identity => _key;
    public PlaybackSnapshot Enrich(PlaybackSnapshot snapshot, string? coverUrl)
    {
        if (snapshot.Track is not { SourceApp: "Netease", SongId: { Length: > 0 } id } || !ValidUrl(coverUrl, out var uri))
            return snapshot;
        var key = "Netease|" + id + "|" + uri.AbsoluteUri;
        if (key != _key)
        {
            _request?.Cancel(); _request?.Dispose(); _request = null;
            _key = key; _bytes = null; _pending = null; _retryAt = DateTimeOffset.MinValue;
        }
        if (_pending is { IsCompleted: true })
        {
            _bytes = _pending.IsCompletedSuccessfully ? _pending.Result : null;
            _pending = null; _retryAt = DateTimeOffset.UtcNow.AddSeconds(15);
        }
        if (_bytes is null && _pending is null && DateTimeOffset.UtcNow >= _retryAt)
        {
            _request?.Dispose(); _request = new CancellationTokenSource(TimeSpan.FromSeconds(6));
            _pending = _fetch(uri, _request.Token);
        }
        return snapshot with { CoverImageBytes = _bytes, IsCoverLoading = _pending is not null };
    }
    internal static bool ValidUrl(string? value, out Uri uri)
    {
        uri = null!;
        if (!Uri.TryCreate(value, UriKind.Absolute, out var parsed) || parsed.Scheme is not ("http" or "https")) return false;
        if (parsed.Host != "music.126.net" && !parsed.Host.EndsWith(".music.126.net", StringComparison.OrdinalIgnoreCase)) return false;
        var builder = new UriBuilder(parsed) { Scheme = "https", Port = -1 };
        if (string.IsNullOrEmpty(builder.Query)) builder.Query = "param=300y300";
        uri = builder.Uri; return true;
    }
    private static async Task<byte[]?> DownloadAsync(Uri uri, CancellationToken token)
    {
        try
        {
            using var response = await Http.GetAsync(uri, HttpCompletionOption.ResponseHeadersRead, token);
            response.EnsureSuccessStatusCode();
            if (response.Content.Headers.ContentLength is > 4 * 1024 * 1024) return null;
            await using var input = await response.Content.ReadAsStreamAsync(token);
            using var output = new System.IO.MemoryStream();
            var buffer = new byte[65536];
            int count;
            while ((count = await input.ReadAsync(buffer, token)) > 0)
            {
                if (output.Length + count > 4 * 1024 * 1024) return null;
                output.Write(buffer, 0, count);
            }
            var bytes = output.ToArray();
            return bytes.Length is > 0 and <= 4 * 1024 * 1024 ? bytes : null;
        }
        catch (Exception ex) when (ex is HttpRequestException or OperationCanceledException or System.IO.IOException) { return null; }
    }
    public void Dispose() { _request?.Cancel(); _request?.Dispose(); }
}
