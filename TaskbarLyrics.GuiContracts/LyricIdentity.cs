namespace TaskbarLyrics.Core.Services;
public static class LyricSyncService
{
    public static string BuildStableTrackIdentity(Models.TrackInfo track)=>NativeBackend.Io("stableIdentity","",track).GetString()??"";
}
