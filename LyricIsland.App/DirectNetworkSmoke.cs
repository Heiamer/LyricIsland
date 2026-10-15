using System.IO;
using System.Net;
using System.Net.Http;
using System.Text.Json;
using TaskbarLyrics.Core;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Services;

namespace LyricIsland;

internal static class DirectNetworkSmoke
{
    internal static async Task RunAsync(string path)
    {
        var inheritedProxy = HttpClient.DefaultProxy;
        var checks = new Dictionary<string, bool>();
        var errors = new Dictionary<string, string>();
        int coverBytes = 0, lyricCharacters = 0;
        try
        {
            // Isolated process only: a dead default proxy proves all tested routes bypass it.
            HttpClient.DefaultProxy = new WebProxy("http://127.0.0.1:1");
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            using var client = DirectNetwork.CreateClient(TimeSpan.FromSeconds(10));
            using var metadata = JsonDocument.Parse(await client.GetStringAsync(
                "https://music.163.com/api/song/detail/?id=347230&ids=%5B347230%5D", timeout.Token));
            var song = metadata.RootElement.GetProperty("songs")[0];
            checks["metadataDirect"] = song.GetProperty("id").GetInt64() == 347230;
            var title = song.GetProperty("name").GetString()!;
            var cover = song.GetProperty("album").GetProperty("picUrl").GetString();
            var track = new TrackInfo("network-test", title, "", "", "Netease", TimeSpan.FromSeconds(240), SongId: "347230");
            using (var artwork = new TrackArtworkResolver())
            {
                var snapshot = new PlaybackSnapshot(false, TimeSpan.Zero, track);
                for (var i = 0; i < 90; i++)
                {
                    var result = artwork.Enrich(snapshot, cover);
                    if (result.CoverImageBytes is { Length: > 0 } bytes) { coverBytes = bytes.Length; break; }
                    await Task.Delay(100, timeout.Token);
                }
                checks["coverDirect"] = coverBytes > 0;
            }
            var source = new NeteaseLyricSource();
            var candidate = new SourceTrackCandidate(KnownLyricProviders.Netease, "347230", title, [], "",
                TimeSpan.FromSeconds(240), "direct-network-test", new Dictionary<string, string>());
            var lyrics = await source.FetchAsync(candidate, timeout.Token);
            lyricCharacters = lyrics?.OriginalLyrics?.Length ?? 0;
            checks["helperLyricsDirect"] = lyricCharacters > 0;
        }
        catch (Exception ex)
        {
            errors["request"] = ex.GetType().Name + ": " + ex.Message;
        }
        finally
        {
            HttpClient.DefaultProxy = inheritedProxy;
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            var success = errors.Count == 0 && checks.Count == 3 && checks.Values.All(x => x);
            File.WriteAllText(path, JsonSerializer.Serialize(new { success, checks, errors, coverBytes, lyricCharacters,
                deadDefaultProxyBypassed = success, systemProxySettingsChanged = false }, new JsonSerializerOptions { WriteIndented = true }));
            if (!success) Environment.ExitCode = 1;
        }
    }
}
