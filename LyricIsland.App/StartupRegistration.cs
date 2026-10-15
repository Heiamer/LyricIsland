using System.IO;
using Microsoft.Win32;
namespace LyricIsland;
internal static class StartupRegistration
{
    internal const string KeyPath = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    internal const string ValueName = "LyricIsland";
    public static string? ReadExecutable(string keyPath = KeyPath)
    {
        using var key = Registry.CurrentUser.OpenSubKey(keyPath);
        return ParseCommand(key?.GetValue(ValueName) as string);
    }
    public static bool IsRegistered => ReadExecutable() is not null;
    public static bool IsEnabled => ReadExecutable() is { } executable && File.Exists(executable);
    public static string Status => IsEnabled ? "已注册登录启动；没有音乐播放时驻留系统托盘。" :
        IsRegistered ? "启动路径已失效，请重新开启以修复。" : "尚未启用登录启动。";
    internal static string? ParseCommand(string? command)
    {
        if (string.IsNullOrWhiteSpace(command) || command[0] != '"') return null;
        var end = command.IndexOf('"', 1);
        if (end < 0 || command[(end + 1)..].Trim() != "--startup") return null;
        var executable = command[1..end];
        return Path.IsPathFullyQualified(executable) &&
            Path.GetFileName(executable).Equals("LyricIsland.exe", StringComparison.OrdinalIgnoreCase) ? executable : null;
    }
    internal static string Command(string executable)
    {
        if (!Path.IsPathFullyQualified(executable) || executable.Contains('"') || !Path.GetFileName(executable).Equals("LyricIsland.exe", StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("Invalid startup executable.");
        var command = $"\"{executable}\" --startup";
        if (command.Length > 260) throw new IOException("程序路径太长，无法注册登录启动。");
        return command;
    }
    // Compatibility overload matching the native backend's signature, so the shared GUI call
    // site (MainWindow's "update"/AutoStart branch) compiles in both backends: register the
    // Run key and persist the settings through the managed store as one transaction.
    public static bool SetEnabled(bool enabled, IslandSettings settings)
        => SetEnabled(enabled, () => SettingsStore.Default.Save(settings));
    public static bool SetEnabled(bool enabled, Func<bool> save, string keyPath = KeyPath, string? executableOverride = null)
    {
        using var key = Registry.CurrentUser.CreateSubKey(keyPath, true);
        var previous = key.GetValue(ValueName, null, RegistryValueOptions.DoNotExpandEnvironmentNames);
        var previousKind = previous is null ? RegistryValueKind.String : key.GetValueKind(ValueName);
        try
        {
            if (enabled)
            {
                var executable = executableOverride ?? Environment.ProcessPath ?? Path.Combine(AppContext.BaseDirectory, "LyricIsland.exe");
                if (!File.Exists(executable)) throw new IOException("启动程序路径不存在。");
                var command = Command(executable);
                key.SetValue(ValueName, command, RegistryValueKind.String);
                if (!string.Equals(key.GetValue(ValueName) as string, command, StringComparison.Ordinal))
                    throw new IOException("启动项写入后校验失败。");
            }
            else
            {
                key.DeleteValue(ValueName, false);
                if (key.GetValue(ValueName) is not null) throw new IOException("启动项未成功移除。");
            }
            if (save()) return true;
            throw new IOException("设置保存失败。");
        }
        catch
        {
            if (previous is null) key.DeleteValue(ValueName, false); else key.SetValue(ValueName, previous, previousKind);
            throw;
        }
    }
}