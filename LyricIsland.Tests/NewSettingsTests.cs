using System.Text.Json;
using System.Windows;
using System.Windows.Media;
using LyricIsland.Media;
using Xunit;
namespace LyricIsland.Tests;
public sealed class NewSettingsTests
{
    [Theory]
    [InlineData("com.TwilightEcho.music")] [InlineData("TwilightEcho.exe")] [InlineData("TWILIGHTECHO")]
    public void TwilightEchoIsRecognizedAndCanBeDisabled(string id)
    {
        using var provider = new SmtcMusicSessionProvider();
        provider.SetRecognitionOrder(["TwilightEcho"], ["TwilightEcho"], ["TwilightEcho"]);
        Assert.True(provider.CanUseSource(id));
        Assert.Equal("TwilightEcho", SmtcMusicSessionProvider.NormalizeSource(id));
        provider.SetRecognitionOrder([], [], []);
        Assert.False(provider.CanUseSource(id));
    }
    [Fact]
    public void CustomProgramMatchesWholeComponentsAndRemovalRevokesIt()
    {
        using var provider = new SmtcMusicSessionProvider();
        provider.SetRecognitionOrder(["MyPlayer"], ["MyPlayer"], ["MyPlayer"]);
        Assert.True(provider.CanUseSource("MyPlayer.exe"));
        Assert.True(provider.CanUseSource("com.MyPlayer.music"));
        Assert.False(provider.CanUseSource("MyPlayerPlus.exe"));
        Assert.False(provider.CanUseSource("com.NotMyPlayer.music"));
        provider.SetRecognitionOrder([], [], []);
        Assert.False(provider.CanUseSource("MyPlayer.exe"));
        Assert.True(PlayerSourceRules.TryParse(" MyPlayer.exe\ncom.example.music，MYPLAYER.exe", out var entries, out _));
        Assert.Equal(2, entries.Length);
        Assert.False(PlayerSourceRules.TryParse("*", out _, out _));
    }
    [Fact]
    public void OldWhitelistGainsTwilightOnceAndLaterOptOutPersists()
    {
        var settings = JsonSerializer.Deserialize<IslandSettings>("{\"AllowedSources\":[\"QQMusic\"]}")!;
        settings.Normalize();
        Assert.Contains("QQMusic", settings.AllowedSources);
        Assert.Contains("TwilightEcho", settings.AllowedSources);
        settings.AllowedSources.Remove("TwilightEcho");
        var reloaded = JsonSerializer.Deserialize<IslandSettings>(JsonSerializer.Serialize(settings))!;
        reloaded.Normalize();
        Assert.DoesNotContain("TwilightEcho", reloaded.AllowedSources);
    }
    [Theory]
    [InlineData("ExpandedTextColor")] [InlineData("ExpandedComponentColor")] [InlineData("LyricProgressColor")]
    public void ColorSettingsAcceptHexAndRejectInvalidInput(string key)
    {
        var settings = new IslandSettings();
        Assert.True(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key, value = "#abcdef" }), settings));
        Assert.Contains("#ABCDEF", JsonSerializer.Serialize(settings));
        Assert.False(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key, value = "invalid" }), settings));
        Assert.False(SettingsProtocol.TryReadSetting(JsonSerializer.SerializeToElement(new { key, value = false }), settings));
    }
    [Theory]
    [InlineData(1850, 1030)] [InlineData(40, 10)] [InlineData(1900, 500)]
    public void TrayMenuStaysNearPointerAndInsideWorkArea(int x, int y)
    {
        var work = new TrayMenuBounds(0, 0, 1920, 1040);
        var menu = TrayMenuPlacement.Place(x, y, 300, 330, work, 6);
        Assert.True(menu.Left >= 0 && menu.Top >= 0 && menu.Right <= work.Right && menu.Bottom <= work.Bottom);
        Assert.InRange(TrayMenuPlacement.DistanceToPointer(menu, x, y), 0, 9);
    }
    [Fact]
    public void StartupParserRejectsInvalidTargetsAndExtraCommands()
    {
        Assert.Equal(@"C:\App\LyricIsland.exe", StartupRegistration.ParseCommand("\"C:\\App\\LyricIsland.exe\" --startup"));
        Assert.Null(StartupRegistration.ParseCommand("\"C:\\Windows\\cmd.exe\" --startup"));
        Assert.Null(StartupRegistration.ParseCommand("\"C:\\App\\LyricIsland.exe\" --startup & other"));
        Assert.Null(StartupRegistration.ParseCommand("LyricIsland.exe --startup"));
    }
}