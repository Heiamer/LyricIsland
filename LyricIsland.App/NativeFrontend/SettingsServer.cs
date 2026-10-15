using System.Runtime.InteropServices;
using System.Text.Json;
using TaskbarLyrics.Core;
namespace LyricIsland;
internal sealed class SettingsServer : IDisposable
{
    private readonly NativeBackend.GuiCallback _callback;
    private readonly ulong _id;
    private bool _disposed;
    public SettingsServer(Func<JsonElement,Task<object>> handler)
    {
        _callback=request=>
        {
            object response;
            try{using var document=JsonDocument.Parse(request);response=handler(document.RootElement.Clone()).GetAwaiter().GetResult();}
            catch(Exception ex){response=new{success=false,message=ex.Message};}
            return Marshal.StringToCoTaskMemUTF8(JsonSerializer.Serialize(response));
        };
        _id=NativeBackend.li_settings_server_create(SettingsProtocol.PipeName,_callback);
        if(_id==0)throw new IOException("Rust 设置通信初始化失败。");
    }
    public void Dispose()
    {
        if(_disposed)return;_disposed=true;NativeBackend.li_settings_server_close(_id);
        // Keep the delegate rooted until all native GUI dispatches have returned.
        _=Task.Run(async()=>{while(NativeBackend.li_settings_server_finished(_id)==0)await Task.Delay(20);NativeBackend.li_settings_server_destroy(_id);GC.KeepAlive(_callback);});
    }
}
