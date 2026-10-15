using System.Windows;
using Xunit;

namespace LyricIsland.Tests;

public sealed class SeekAndStartupTests
{
    [Fact]
    public void StartupCommandQuotesPathsAndRejectsOtherExecutables()
    {
        Assert.Equal("\"C:\\Some Folder\\LyricIsland.exe\" --startup", StartupRegistration.Command("C:\\Some Folder\\LyricIsland.exe"));
        Assert.Throws<ArgumentException>(() => StartupRegistration.Command("C:\\Windows\\cmd.exe"));
        Assert.Throws<ArgumentException>(() => StartupRegistration.Command("LyricIsland.exe"));
    }
    [Fact]
    public void AddedSettingsDoNotEnableStartupByDefault()
    {
        Assert.False(new IslandSettings().AutoStart);
    }
    [Fact]
    public void StartupRegistrationRollsBackOnSettingsFailureAndPreservesOtherValues()
    {
        var path = "Software\\LyricIsland\\StartupTests\\" + Guid.NewGuid().ToString("N");
        var directory = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "island-startup-" + Guid.NewGuid().ToString("N"));
        System.IO.Directory.CreateDirectory(directory);
        var exe = System.IO.Path.Combine(directory, "LyricIsland.exe"); System.IO.File.WriteAllText(exe, "test only");
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(path);
            key.SetValue("OtherApp", "preserve");
            Assert.True(StartupRegistration.SetEnabled(true, () => true, path, exe));
            var command = key.GetValue(StartupRegistration.ValueName);
            Assert.Equal(StartupRegistration.Command(exe), command);
            Assert.Throws<System.IO.IOException>(() => StartupRegistration.SetEnabled(false, () => false, path, exe));
            Assert.Equal(command, key.GetValue(StartupRegistration.ValueName));
            Assert.True(StartupRegistration.SetEnabled(false, () => true, path, exe));
            Assert.Null(key.GetValue(StartupRegistration.ValueName)); Assert.Equal("preserve", key.GetValue("OtherApp"));
        }
        finally
        {
            Microsoft.Win32.Registry.CurrentUser.DeleteSubKeyTree(path, false);
            System.IO.File.Delete(exe); System.IO.Directory.Delete(directory);
        }
    }
}
