using TaskbarLyrics.Core;
namespace LyricIsland;
internal static class StartupRegistration
{
    private static System.Text.Json.JsonElement Info=>NativeBackend.Io("startupInfo");
    public static bool IsRegistered=>Info.GetProperty("registered").GetBoolean();
    public static bool IsEnabled=>Info.GetProperty("enabled").GetBoolean();
    public static string Status=>Info.GetProperty("status").GetString()??"";
    public static bool SetEnabled(bool enabled,IslandSettings settings)=>NativeBackend.Io("setStartup",Path.Combine(SettingsStore.DataDirectory,"settings.json"),new{enabled,executable=Environment.ProcessPath,settings}).GetProperty("success").GetBoolean();
}
