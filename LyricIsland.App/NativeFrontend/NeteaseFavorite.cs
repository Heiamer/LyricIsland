using TaskbarLyrics.Core;
using TaskbarLyrics.Core.Models;
namespace LyricIsland;
internal sealed record FavoriteResult(bool Success,string Message);
internal static class NeteaseFavorite
{
    public static Task<FavoriteResult> ToggleAsync(TrackInfo track,CancellationToken token)=>Access(track,true,token);
    public static Task<FavoriteResult> InspectAsync(TrackInfo track,CancellationToken token)=>Access(track,false,token);
    private static Task<FavoriteResult> Access(TrackInfo track,bool invoke,CancellationToken token)=>Task.Run(()=>{token.ThrowIfCancellationRequested();var result=NativeBackend.Io("favorite","",new{title=track.Title,sourceApp=track.SourceApp,invoke});return new FavoriteResult(result.GetProperty("success").GetBoolean(),result.GetProperty("message").GetString()??"");},token);
}
