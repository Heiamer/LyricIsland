using System.Text.Json;
using TaskbarLyrics.Core;
using TaskbarLyrics.Core.Models;
namespace LyricIsland;

// UI facade. Media, timing, searches and artwork are owned by the Rust backend.
internal sealed class MusicEngine : IDisposable
{
    private readonly BackendHandle _handle;
    private readonly IslandSettings _settings;
    private ulong _coverVersion;
    private byte[]? _cover;
    private LyricDisplayFrame _frame=new("","","",0,-1);
    private static readonly JsonSerializerOptions Options=new(){PropertyNameCaseInsensitive=true};
    internal JsonElement State { get; private set; }=JsonDocument.Parse("{}").RootElement.Clone();
    public NativeMediaFacade Media { get; }
    public NativePlaybackClock Clock { get; }
    public PlaybackSnapshot? LatestPlayback { get; private set; }
    public string LyricSource=>Text("lyricSource","等待获取");
    public string DurationSource=>Text("durationSource","等待歌曲信息");
    public string ArtworkIdentity=>Text("artworkIdentity","");
    public string NativeTimelineStatus=>Text("nativeStatus","等待网易云");
    public object ArtworkDiagnostics=>new{Status=NativeTimelineStatus,LastReadError=Text("nativeError","")};
    public bool CanSeek(PlaybackSnapshot playback)=>playback.CanSeek;
    public MusicEngine(IslandSettings settings)
    {
        _settings=settings;
        _handle=NativeBackend.Create(settings,SettingsStore.DataDirectory,Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),TaskbarLyrics.Core.Utilities.Log.GetLogsDirectory());
        Media=new(this);Clock=new(this);
    }
    public void ApplySettings()=>NativeBackend.Apply(_handle,_settings);
    internal string Text(string name,string fallback)=>State.TryGetProperty(name,out var value)&&value.ValueKind==JsonValueKind.String?value.GetString()??fallback:fallback;
    internal double Number(string name)=>State.TryGetProperty(name,out var value)&&value.TryGetDouble(out var number)?number:0;
    internal bool Flag(string name)=>State.TryGetProperty(name,out var value)&&value.ValueKind==JsonValueKind.True;
    public PlaybackSnapshot? RefreshState()
    {
        State=NativeBackend.Snapshot(_handle,_coverVersion);
        var version=State.GetProperty("coverVersion").GetUInt64();
        if(version!=_coverVersion){_coverVersion=version;_cover=State.TryGetProperty("cover",out var cover)&&cover.ValueKind==JsonValueKind.String?Convert.FromBase64String(cover.GetString()!):null;}
        TrackInfo? track=null;
        if(State.TryGetProperty("track",out var item)&&item.ValueKind==JsonValueKind.Object)
        {
            string TextValue(string key)=>item.TryGetProperty(key,out var value)&&value.ValueKind==JsonValueKind.String?value.GetString()??"":"";
            track=new(TextValue("id"),TextValue("title"),TextValue("artist"),TextValue("album"),TextValue("sourceApp"),TimeSpan.FromSeconds(item.GetProperty("durationSeconds").GetDouble()),TextValue("songId"));
        }
        var position=TimeSpan.FromSeconds(Number("positionSeconds"));
        var raw=State.TryGetProperty("rawSeconds",out var rawItem)&&rawItem.ValueKind==JsonValueKind.Number?TimeSpan.FromSeconds(rawItem.GetDouble()):(TimeSpan?)null;
        LatestPlayback=new(Flag("isPlaying"),position,track,_cover,raw,null,Flag("isCoverLoading"),Text("sourceId",""),Flag("canSeek"));
        _frame=JsonSerializer.Deserialize<LyricDisplayFrame>(State.GetProperty("frame"),Options)??new("","","");
        Media.Observe((long)Number("events"));return LatestPlayback;
    }
    public Task<(PlaybackSnapshot Playback,LyricDisplayFrame Lyrics)> ReadAsync(CancellationToken token)
    {token.ThrowIfCancellationRequested();return Task.FromResult((RefreshState()!,_frame));}
    internal Task<bool> CommandAsync(string action,CancellationToken token,double seconds=0)=>Task.Run(()=>{token.ThrowIfCancellationRequested();var result=NativeBackend.Command(_handle,action,seconds);token.ThrowIfCancellationRequested();return result;},token);
    public Task<bool> TogglePlaybackAsync(CancellationToken token)=>CommandAsync("toggle",token);
    public Task<bool> SetPlayingAsync(bool playing,CancellationToken token)=>CommandAsync(playing?"play":"pause",token);
    public Task<bool> SeekAsync(double seconds,CancellationToken token)=>CommandAsync("seek",token,seconds);
    internal void Synchronize(double seconds)=>NativeBackend.Command(_handle,"synchronize",seconds);
    public void Dispose()=>_handle.Dispose();
}
internal sealed class NativePlaybackClock(MusicEngine owner)
{
    public double Position=>owner.Number("positionSeconds");
    public bool IsLocal=>owner.Flag("localClock");
    public string Status=>owner.Text("clockStatus","等待播放器");
    public void Synchronize(double seconds)=>owner.Synchronize(seconds);
}
internal sealed class NativeMediaFacade(MusicEngine owner)
{
    public event EventHandler? Changed;
    public long EventsObserved {get;private set;}
    internal void Observe(long revision){if(revision==EventsObserved)return;EventsObserved=revision;Changed?.Invoke(this,EventArgs.Empty);}
    public async Task ExecuteAsync(Media.MediaHotkeyAction action,CancellationToken token)=>await owner.CommandAsync(action switch{Media.MediaHotkeyAction.PreviousTrack=>"previous",Media.MediaHotkeyAction.NextTrack=>"next",_=>"toggle"},token);
}
