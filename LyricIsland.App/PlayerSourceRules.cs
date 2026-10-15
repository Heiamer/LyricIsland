using System.IO;
namespace LyricIsland;
internal static class PlayerSourceRules
{
    public static bool Matches(string rule, string source)
    {
        if (string.Equals(rule, source, StringComparison.OrdinalIgnoreCase)) return true;
        var name = ProgramName(rule); var actual = ProgramName(source);
        if (string.Equals(name, actual, StringComparison.OrdinalIgnoreCase)) return true;
        return !name.Contains('.') && source.Split(['.', '!', '/', '\\', ':'], StringSplitOptions.RemoveEmptyEntries)
            .Contains(name, StringComparer.OrdinalIgnoreCase);
    }
    private static string ProgramName(string source)
    {
        var name = Path.GetFileName(source.Trim().Trim('"'));
        return name.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ? name[..^4] : name;
    }
    public static bool TryParse(string text, out string[] entries, out string error)
    {
        entries = text.Split(['\r', '\n', ',', '，', ';', '；'], StringSplitOptions.RemoveEmptyEntries)
            .Select(x => x.Trim().Trim('"')).Where(x => x.Length > 0).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
        error = entries.Length == 0 ? "请输入程序名或媒体会话标识。" :
            entries.Length > 32 ? "最多允许 32 个播放器，请减少输入。" :
            entries.Any(x => x.Length > 256 || x.Any(char.IsControl) || x.Contains('"') || x.Contains('*') || x.Contains('?'))
                ? "请填写有效的程序名或媒体会话标识，不支持通配符或启动参数。" : string.Empty;
        return error.Length == 0;
    }
}