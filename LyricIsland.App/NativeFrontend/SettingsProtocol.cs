using System.Text.Json;
using TaskbarLyrics.Core;
namespace LyricIsland;
internal static class SettingsProtocol
{
    public static string PipeName {get;set;}="LyricIsland.Settings.v3";
    public static Task<JsonElement> SendAsync(string command,object? value=null)=>Task.Run(()=>NativeBackend.Io("ipc",PipeName,new{version=1,command,value}));
    public static bool TryReadSetting(JsonElement input,IslandSettings settings)
    {
        if(input.ValueKind!=JsonValueKind.Object||!input.TryGetProperty("key",out var key)||!input.TryGetProperty("value",out var value))return false;
        try
        {
            var result=NativeBackend.Io("validateSetting","",new{key=key.GetString(),value,settings});
            var update=JsonSerializer.Deserialize<IslandSettings>(result);
            if(update is null)return false;
            // GUI binding model identity is preserved; validation and merging stay in Rust.
            foreach(var property in typeof(IslandSettings).GetProperties().Where(p=>p.CanWrite))property.SetValue(settings,property.GetValue(update));
            return true;
        }
        catch(Exception ex) when(ex is IOException or JsonException or ArgumentException){return false;}
    }
}
