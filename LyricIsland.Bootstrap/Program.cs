using System;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Uses the .NET Framework already supplied by supported Windows versions.
// The native build extracts Rust executables and fonts; this bootstrap exits
// after starting them.
// This directory owns extraction, package verification and previous-version data migration.
internal static class Program
{
    private const string Gui = "Direct2D";

    private static readonly string Folder = "LyricIsland-" + BundleVersion.Version;
    [STAThread]
    private static int Main(string[] args)
    {
        try
        {
            bool verify = args.Length == 2 && args[0] == "--verify-package";
            string root = verify ? Path.GetFullPath(args[1]) : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "LyricIsland", "Runtime", BundleVersion.Version);
            using (var gate = new Mutex(false, "Local.LyricIsland.Bundle." + BundleVersion.Version))
            {
                try { if (!gate.WaitOne(TimeSpan.FromSeconds(60))) throw new IOException("另一份程序正在准备组件，请稍后重试。"); }
                catch (AbandonedMutexException) { }
                try
                {
                    string executable = Path.Combine(root, Folder, "LyricIsland.exe");
                    string marker = Path.Combine(root, "ready.hash");
                    if (!File.Exists(marker) || File.ReadAllText(marker) != BundleVersion.PayloadHash || !Complete(root))
                    {
                        Directory.CreateDirectory(root);
                        using (Stream payload = Assembly.GetExecutingAssembly().GetManifestResourceStream("LyricIsland.Payload.zip"))
                        {
                            if (payload == null) throw new IOException("程序组件缺失。");
                            using (var archive = new ZipArchive(payload, ZipArchiveMode.Read))
                            {
                                string prefix = Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
                                foreach (var entry in archive.Entries)
                                {
                                    if (!entry.FullName.StartsWith(Folder + "/", StringComparison.Ordinal)) throw new IOException("无效的组件路径。");
                                    string path = Path.GetFullPath(Path.Combine(root, entry.FullName.Replace('/', Path.DirectorySeparatorChar)));
                                    if (!path.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) throw new IOException("无效的组件路径。");
                                    if (entry.FullName.EndsWith("/", StringComparison.Ordinal)) { Directory.CreateDirectory(path); continue; }
                                    Directory.CreateDirectory(Path.GetDirectoryName(path));
                                    using (Stream input = entry.Open()) using (Stream output = File.Create(path)) input.CopyTo(output);
                                }
                            }
                        }
                        if (!Complete(root)) throw new IOException("组件释放不完整，请重新下载后运行。");
                        File.WriteAllText(marker, BundleVersion.PayloadHash);
                    }
                    if (verify)
                    {
                        File.WriteAllText(Path.Combine(root, "verification.json"), "{\"success\":true,\"version\":\"" + BundleVersion.Version + "\",\"executable\":\"" + executable.Replace("\\", "\\\\") + "\",\"gui\":\"" + Gui + "\",\"settingsUi\":\"GPUI\",\"backend\":\"Rust\"}", Encoding.UTF8);
                        return 0;
                    }
                    MigratePreviousData(root);
                    Process.Start(new ProcessStartInfo(executable) { UseShellExecute = true, WorkingDirectory = Path.GetDirectoryName(executable) });
                    return 0;
                }
                finally { gate.ReleaseMutex(); }
            }
        }
        catch (Exception ex) { MessageBox(IntPtr.Zero, "无法启动歌词灵动岛：" + ex.Message, "Lyric Island", 0x10); return 1; }
    }
    private static void MigratePreviousData(string root)
    {
        string destination = Path.Combine(root, Folder, "Data");
        if (File.Exists(Path.Combine(destination, "settings.json"))) return;
        string previous = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "LyricIsland", "Runtime", "1.0", "LyricIsland-1.0", "Data");
        if (!File.Exists(Path.Combine(previous, "settings.json"))) return;
        Directory.CreateDirectory(destination);
        File.Copy(Path.Combine(previous, "settings.json"), Path.Combine(destination, "settings.json"), false);
        string maps = Path.Combine(previous, "database");
        if (Directory.Exists(maps))
        {
            string target = Path.Combine(destination, "database");
            Directory.CreateDirectory(target);
            foreach (string file in Directory.GetFiles(maps))
            {
                string output = Path.Combine(target, Path.GetFileName(file));
                if (!File.Exists(output)) File.Copy(file, output, false);
            }
        }
    }
    private static bool Complete(string root)
    {
        string[] required = { "LyricIsland.exe", "LyricIsland.NotificationHost.exe", "LyricIsland.Settings.exe", "e_sqlite3.dll", "Assets/Fonts/NotoSansSC.ttf", "Assets/Fonts/NotoSansJP.ttf" };

        foreach (string name in required)
            if (!File.Exists(Path.Combine(root, Folder, name.Replace('/', Path.DirectorySeparatorChar)))) return false;
        return true;
    }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int MessageBox(IntPtr handle, string text, string caption, uint type);
}
