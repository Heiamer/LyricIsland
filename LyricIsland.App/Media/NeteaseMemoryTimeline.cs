using System.Diagnostics;
using System.IO;
using System.Reflection.PortableExecutable;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Microsoft.Win32.SafeHandles;
using TaskbarLyrics.Core.Models;

namespace LyricIsland.Media;

internal sealed record NeteaseTimingSample(string SongId, string Title, double Position, double Duration, bool Playing, DateTimeOffset Captured, string? CoverUrl = null);

// Read-only timing adapter based on AF-Media-Bar and Kxnrl's documented signatures.
// The process handle has PROCESS_VM_READ only. No injection, memory writes or hooks.
internal sealed class NeteaseMemoryTimeline : IDisposable
{
    private const string PlayerPattern = "48 8D 0D ? ? ? ? E8 ? ? ? ? 48 8D 0D ? ? ? ? E8 ? ? ? ? 90 48 8D 0D ? ? ? ? E8 ? ? ? ? 48 8D 05 ? ? ? ? 48 8D A5 ? ? ? ? 5F 5D C3 CC CC CC CC CC 48 89 4C 24 ? 55 57 48 81 EC ? ? ? ? 48 8D 6C 24 ? 48 8D 7C 24";
    private const string SchedulePattern = "66 0F 2E 0D ? ? ? ? 7A ? 75 ? 66 0F 2E 15";
    private readonly object _gate = new();
    private Task? _pending;
    private long _lastPoll;
    private long _retryAt;
    private Reader? _reader;
    private NeteaseTimingSample? _latest;
    private bool _disposed;
    public string Status { get; private set; } = "等待网易云";
    public string? LastReadError { get; private set; }
    public NeteaseTimingSample? LastSample => Volatile.Read(ref _latest);
    public bool AllowOnlineMetadata { get; set; }
    public string? CoverUrl(TrackInfo? track)
    {
        var sample = Volatile.Read(ref _latest);
        return Matches(track, sample, DateTimeOffset.UtcNow) ? sample!.CoverUrl : null;
    }

    public void Request(TrackInfo? track)
    {
        if (track?.SourceApp.Equals("Netease", StringComparison.OrdinalIgnoreCase) != true) return;
        lock (_gate)
        {
            if (_disposed || _pending is { IsCompleted: false } || Stopwatch.GetTimestamp() < _retryAt ||
                _lastPoll != 0 && Stopwatch.GetElapsedTime(_lastPoll).TotalMilliseconds < 233) return;
            _lastPoll = Stopwatch.GetTimestamp();
            _pending = Task.Run(() =>
            {
                try
                {
                    _reader ??= Reader.Attach();
                    var sample = _reader.Read(AllowOnlineMetadata);
                    LastReadError = null;
                    Volatile.Write(ref _latest, sample);
                    Status = sample is null ? "等待网易云有效进度 · " + _reader.NoSampleReason : "网易云实时进度";
                }
                catch (Exception ex) when (ex is IOException or InvalidOperationException or System.ComponentModel.Win32Exception or UnauthorizedAccessException or JsonException or BadImageFormatException or ArgumentException)
                {
                    Volatile.Write(ref _latest, null); _reader?.Dispose(); _reader = null;
                    Status = "网易云读取不可用，使用系统媒体会话";
                    LastReadError = ex.Message;
                    lock (_gate) _retryAt = Stopwatch.GetTimestamp() + Stopwatch.Frequency * 5;
                }
            });
        }
    }

    public bool TryApply(PlaybackSnapshot snapshot, out PlaybackSnapshot corrected, out SmtcTimelineDiagnostics? timeline)
    {
        corrected = snapshot; timeline = null;
        var sample = Volatile.Read(ref _latest);
        if (!Matches(snapshot.Track, sample, DateTimeOffset.UtcNow)) return false;
        var age = DateTimeOffset.UtcNow - sample!.Captured;
        var position = Math.Min(sample.Duration, sample.Position + (sample.Playing ? Math.Max(0, age.TotalSeconds) : 0));
        var raw = TimeSpan.FromSeconds(sample.Position);
        corrected = snapshot with
        {
            IsPlaying = sample.Playing, Position = TimeSpan.FromSeconds(position), RawPosition = raw,
            Track = snapshot.Track! with { SongId = sample.SongId, Duration = TimeSpan.FromSeconds(sample.Duration) }
        };
        timeline = new(sample.Captured, "cloudmusic.exe", "Netease", "Netease", sample.Playing,
            raw, sample.Captured, age, corrected.Position, corrected.Position, "NeteaseMemory", sample.Title, snapshot.Track!.Artist, false);
        return true;
    }

    internal static bool Matches(TrackInfo? track, NeteaseTimingSample? sample, DateTimeOffset now) =>
        track is not null && track.SourceApp.Equals("Netease", StringComparison.OrdinalIgnoreCase) && sample is not null &&
        now >= sample.Captured && now - sample.Captured < TimeSpan.FromMilliseconds(800) &&
        double.IsFinite(sample.Position) && double.IsFinite(sample.Duration) && sample.Duration > 0 && sample.Duration < 86400 &&
        sample.Position >= 0 && sample.Position <= sample.Duration + .5 &&
        (string.IsNullOrEmpty(track.SongId) || track.SongId == sample.SongId) &&
        string.Equals(track.Title.Trim(), sample.Title.Trim(), StringComparison.OrdinalIgnoreCase);

    public void Dispose()
    {
        lock (_gate)
        {
            _disposed = true;
            if (_pending is { IsCompleted: false } task) _ = task.ContinueWith(_ => _reader?.Dispose(), TaskScheduler.Default);
            else _reader?.Dispose();
        }
    }

    private sealed class Reader : IDisposable
    {
        private readonly Process _process;
        private readonly SafeProcessHandle _handle;
        private readonly nint _player;
        private readonly nint _schedule;
        private string? _metadataId;
        private string? _title;
        private string? _coverUrl;
        private long _metadataRetryAt;
        private Task<(string? Title, string? Cover)>? _remoteMetadata;
        private string? _remoteId;
        private long _remoteRetryAt;
        private static readonly System.Net.Http.HttpClient MetadataHttp = TaskbarLyrics.Core.DirectNetwork.CreateClient(TimeSpan.FromSeconds(4));
        public string NoSampleReason { get; private set; } = "读取中";
        private Reader(Process process, ProcessModule module)
        {
            _process = process;
            _handle = OpenProcess(0x0010, false, process.Id);
            try
            {
                if (_handle.IsInvalid) throw new IOException("Cannot read the player process.");
                using var file = File.OpenRead(module.FileName);
                using var pe = new PEReader(file);
                var section = pe.PEHeaders.SectionHeaders.First(s => s.Name == ".text");
                var playerRva = TaskbarLyrics.Core.RustLyricEngine.ScanPlayerSignature(module.FileName, section.PointerToRawData, section.SizeOfRawData, section.VirtualAddress, PlayerPattern, 3);
                var scheduleRva = TaskbarLyrics.Core.RustLyricEngine.ScanPlayerSignature(module.FileName, section.PointerToRawData, section.SizeOfRawData, section.VirtualAddress, SchedulePattern, 4);
                var address = module.BaseAddress + section.VirtualAddress;
                _player = playerRva is { } p ? module.BaseAddress + (nint)p : PlayerSignatureScanner.Resolve(file, section.PointerToRawData, section.SizeOfRawData, address, PlayerPattern, 3);
                _schedule = scheduleRva is { } s ? module.BaseAddress + (nint)s : PlayerSignatureScanner.Resolve(file, section.PointerToRawData, section.SizeOfRawData, address, SchedulePattern, 4);
            }
            catch { _handle.Dispose(); throw; }
        }
        public static Reader Attach()
        {
            foreach (var process in Process.GetProcessesByName("cloudmusic"))
            {
                try
                {
                    if (process.MainWindowHandle != IntPtr.Zero)
                    {
                        var module = process.Modules.Cast<ProcessModule>().FirstOrDefault(m => m.ModuleName.Equals("cloudmusic.dll", StringComparison.OrdinalIgnoreCase));
                        if (module is not null) return new Reader(process, module);
                    }
                }
                catch { process.Dispose(); throw; }
                process.Dispose();
            }
            throw new IOException("No supported NetEase player process.");
        }
        private static nint Resolve(byte[] code, string signature, int displacementOffset, nint baseAddress)
        {
            var pattern = signature.Split(' ').Select(s => s == "?" ? -1 : Convert.ToInt32(s, 16)).ToArray();
            var found = -1;
            for (var i = 0; i <= code.Length - pattern.Length; i++)
            {
                if (code[i] != pattern[0]) continue;
                var matches = true;
                for (var j = 1; j < pattern.Length; j++) if (pattern[j] >= 0 && code[i + j] != pattern[j]) { matches = false; break; }
                if (!matches) continue;
                if (found >= 0) throw new IOException("Ambiguous player signature.");
                found = i;
            }
            if (found < 0) throw new IOException("Unsupported player build.");
            return baseAddress + found + displacementOffset + 4 + BitConverter.ToInt32(code, found + displacementOffset);
        }
        public NeteaseTimingSample? Read(bool allowOnlineMetadata)
        {
            if (_process.HasExited) throw new IOException("Player exited.");
            var state = BitConverter.ToInt32(Bytes(_player + 0x60, 4));
            if (state is not (1 or 2)) { NoSampleReason = "状态 " + state; return null; }
            var songId = SongId();
            var position = BitConverter.ToDouble(Bytes(_schedule, 8));
            var duration = BitConverter.ToDouble(Bytes(_player + 0xA8, 8));
            if (_metadataId != songId || (string.IsNullOrEmpty(_title) || string.IsNullOrEmpty(_coverUrl)) && Stopwatch.GetTimestamp() >= _metadataRetryAt)
            {
                (_title, _coverUrl) = ReadMetadata(songId); _metadataId = songId;
                _metadataRetryAt = Stopwatch.GetTimestamp() + Stopwatch.Frequency;
            }
            if (allowOnlineMetadata && (string.IsNullOrEmpty(_title) || string.IsNullOrEmpty(_coverUrl)))
            {
                if (_remoteId == songId && _remoteMetadata is { IsCompletedSuccessfully: true })
                {
                    var remote = _remoteMetadata.Result;
                    if (!string.IsNullOrEmpty(remote.Title)) _title = remote.Title;
                    if (!string.IsNullOrEmpty(remote.Cover)) _coverUrl = remote.Cover;
                }
                if (_remoteId != songId || _remoteMetadata is null || _remoteMetadata.IsCompleted && Stopwatch.GetTimestamp() >= _remoteRetryAt)
                {
                    _remoteId = songId; _remoteMetadata = LookupMetadataAsync(songId);
                    _remoteRetryAt = Stopwatch.GetTimestamp() + Stopwatch.Frequency * 10;
                }
            }
            if (string.IsNullOrEmpty(_title)) { NoSampleReason = "缺少歌曲 " + songId + " 元数据"; return null; }
            if (SongId() != songId) { NoSampleReason = "切歌中"; return null; }
            var sample = new NeteaseTimingSample(songId, _title, position, duration, state == 1, DateTimeOffset.UtcNow, _coverUrl);
            return double.IsFinite(position) && position >= 0 && double.IsFinite(duration) && duration > 0 && duration < 86400 && position <= duration + .5 ? sample : null;
        }
        private string SongId()
        {
            var info = BitConverter.ToInt64(Bytes(_player + 0x50, 8));
            if (info <= 0) throw new IOException("Missing player track.");
            var text = (nint)info + 0x10;
            var length = BitConverter.ToInt64(Bytes(text + 0x10, 8));
            if (length is <= 0 or > 128) throw new IOException("Invalid track identifier.");
            var address = length <= 15 ? text : (nint)BitConverter.ToInt64(Bytes(text, 8));
            var id = Encoding.UTF8.GetString(Bytes(address, (int)length)).Split('_')[0].TrimEnd('\0');
            if (id.Length is < 1 or > 20 || !id.All(char.IsAsciiDigit)) throw new IOException("Invalid song ID.");
            return id;
        }
        private static (string? Title, string? Cover) ReadMetadata(string id)
        {
            var directory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NetEase", "CloudMusic", "WebData", "file");
            foreach (var (name, property) in new[] { ("playingList", "list"), ("fmPlay", "queue") })
            {
                var path = Path.Combine(directory, name);
                if (!File.Exists(path) || new FileInfo(path).Length > 32 * 1024 * 1024) continue;
                try
                {
                    using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
                    using var document = JsonDocument.Parse(file);
                    if (!document.RootElement.TryGetProperty(property, out var array) || array.ValueKind != JsonValueKind.Array) continue;
                    foreach (var item in array.EnumerateArray())
                    {
                        if (!item.TryGetProperty("id", out var identity) || identity.ToString() != id) continue;
                        var track = item.TryGetProperty("track", out var nested) ? nested : item;
                        string? cover = null;
                        if (track.TryGetProperty("album", out var album) && album.ValueKind == JsonValueKind.Object &&
                            (album.TryGetProperty("cover", out var art) || album.TryGetProperty("picUrl", out art)) && art.ValueKind == JsonValueKind.String) cover = art.GetString();
                        return (track.TryGetProperty("name", out var title) && title.ValueKind == JsonValueKind.String ? title.GetString() : null, cover);
                    }
                }
                catch (Exception ex) when (ex is IOException or JsonException) { }
            }
            return (null, null);
        }
        private static async Task<(string? Title, string? Cover)> LookupMetadataAsync(string id)
        {
            try
            {
                var json = await MetadataHttp.GetStringAsync("https://music.163.com/api/song/detail/?id=" + id + "&ids=%5B" + id + "%5D");
                using var document = JsonDocument.Parse(json);
                if (!document.RootElement.TryGetProperty("songs", out var songs) || songs.GetArrayLength() == 0) return (null, null);
                var song = songs[0];
                if (song.GetProperty("id").ToString() != id) return (null, null);
                return (song.GetProperty("name").GetString(), song.GetProperty("album").GetProperty("picUrl").GetString());
            }
            catch (Exception ex) when (ex is System.Net.Http.HttpRequestException or OperationCanceledException or JsonException or InvalidOperationException or KeyNotFoundException) { return (null, null); }
        }
        private byte[] Bytes(nint address, int count)
        {
            if (address <= 0) throw new IOException("Invalid player address.");
            var bytes = new byte[count];
            if (!ReadProcessMemory(_handle, address, bytes, (nuint)count, out var read) || read != (nuint)count) throw new IOException("Player field unavailable.");
            return bytes;
        }
        public void Dispose() { _handle.Dispose(); _process.Dispose(); }
        [DllImport("kernel32.dll", SetLastError = true)] private static extern SafeProcessHandle OpenProcess(uint access, bool inherit, int pid);
        [DllImport("kernel32.dll", SetLastError = true)] private static extern bool ReadProcessMemory(SafeProcessHandle process, nint address, byte[] bytes, nuint count, out nuint read);
    }
}
