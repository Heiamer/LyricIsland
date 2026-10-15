using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.Win32.SafeHandles;
using TaskbarLyrics.Core.Models;
using TaskbarLyrics.Core.Utilities;

namespace TaskbarLyrics.Core;

// Native disk entries stay as UTF-8. Keep only a small working set of managed payloads.
internal sealed class RustJsonCacheStore<T> : IDisposable where T : class
{
    private const int HotCapacity = 8;
    private readonly object _gate = new();
    private readonly RustJsonCacheHandle _handle;
    private readonly string _path;
    private readonly Dictionary<string, LinkedListNode<(string Key, T Payload)>> _hot = new(StringComparer.OrdinalIgnoreCase);
    private readonly LinkedList<(string Key, T Payload)> _order = new();
    private bool _disposed;

    private RustJsonCacheStore(ulong handle, string path) { _handle = new RustJsonCacheHandle(handle); _path = path; }
    internal static RustJsonCacheStore<T>? TryCreate(string path)
    {
        if (!RustLyricEngine.IsActive) return null;
        try
        {
            var absolute = Path.GetFullPath(path);
            var handle = RustJsonCacheNative.li_json_cache_create(absolute);
            return handle == 0 ? null : new(handle, absolute);
        }
        catch (Exception ex) when (IsInteropFailure(ex) || ex is ArgumentException or IOException) { return null; }
    }

    // Return indicates whether the native operation was handled, independently of found/missing.
    internal bool TryGet(string key, out bool found, out T? payload, out LyricAcquisitionKind acquisition)
    {
        found = false; payload = null; acquisition = LyricAcquisitionKind.Unknown;
        lock (_gate)
        {
            if (_disposed) return false;
            if (_hot.TryGetValue(key, out var node))
            {
                _order.Remove(node); _order.AddFirst(node);
                payload = node.Value.Payload; found = true; acquisition = LyricAcquisitionKind.MemoryCache;
                return true;
            }
            try
            {
                var status = -1;
                var json = RustLyricEngine.InvokeUtf8ForHandle(() => RustJsonCacheNative.li_json_cache_get(_handle, key, out status));
                if (status == 0) return true;
                if (status < 0 || json is null) return false;
                try { payload = JsonSerializer.Deserialize<T>(json); }
                catch (Exception ex) when (ex is JsonException or NotSupportedException or InvalidOperationException)
                {
                    _ = RustJsonCacheNative.li_json_cache_clear(_handle); _hot.Clear(); _order.Clear();
                    Log.Warn($"Discarding invalid Rust cache payload '{_path}': {ex.Message}");
                    return true;
                }
                if (payload is null) return true;
                found = true; acquisition = status == 2 ? LyricAcquisitionKind.DiskCache : LyricAcquisitionKind.MemoryCache;
                Remember(key, payload); return true;
            }
            catch (Exception ex) when (IsInteropFailure(ex)) { return false; }
        }
    }
    internal bool Store(string key, T payload)
    {
        lock (_gate)
        {
            if (_disposed) return false;
            try
            {
                var status = RustJsonCacheNative.li_json_cache_store(_handle, key, JsonSerializer.Serialize(payload));
                if (status == 0) return false;
                Remember(key, payload);
                if (status == 2) Log.Warn($"Failed to persist Rust lyric cache '{_path}'; memory entry retained.");
                return true;
            }
            catch (Exception ex) when (IsInteropFailure(ex) || ex is JsonException or NotSupportedException) { return false; }
        }
    }
    internal bool Remove(string key)
    {
        lock (_gate)
        {
            if (_disposed) return false;
            try
            {
                if (RustJsonCacheNative.li_json_cache_remove(_handle, key) == 0) return false;
                if (_hot.Remove(key, out var node)) _order.Remove(node);
                return true;
            }
            catch (Exception ex) when (IsInteropFailure(ex)) { return false; }
        }
    }
    internal bool Clear()
    {
        lock (_gate)
        {
            if (_disposed) return false;
            try
            {
                if (RustJsonCacheNative.li_json_cache_clear(_handle) == 0) return false;
                _hot.Clear(); _order.Clear(); return true;
            }
            catch (Exception ex) when (IsInteropFailure(ex)) { return false; }
        }
    }
    private void Remember(string key, T payload)
    {
        if (_hot.Remove(key, out var existing)) _order.Remove(existing);
        var node = _order.AddFirst((key, payload)); _hot[key] = node;
        if (_hot.Count <= HotCapacity) return;
        var oldest = _order.Last!; _hot.Remove(oldest.Value.Key); _order.RemoveLast();
    }
    public void Dispose()
    {
        lock (_gate) { if (_disposed) return; _disposed = true; _hot.Clear(); _order.Clear(); _handle.Dispose(); }
    }
    private static bool IsInteropFailure(Exception ex) => ex is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException;
}

internal sealed class RustJsonCacheHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        internal RustJsonCacheHandle(ulong id) : base(true) { SetHandle(unchecked((IntPtr)(long)id)); }
        protected override bool ReleaseHandle()
        {
            try { RustJsonCacheNative.li_json_cache_destroy(unchecked((ulong)handle.ToInt64())); } catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException or BadImageFormatException) { }
            return true;
        }
    }
internal static class RustJsonCacheNative
    {
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong li_json_cache_create([MarshalAs(UnmanagedType.LPUTF8Str)] string path);
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void li_json_cache_destroy(ulong handle);
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr li_json_cache_get(RustJsonCacheHandle handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string key, out int acquisition);
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_json_cache_store(RustJsonCacheHandle handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string key, [MarshalAs(UnmanagedType.LPUTF8Str)] string payload);
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_json_cache_remove(RustJsonCacheHandle handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string key);
        [DllImport("lyric_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int li_json_cache_clear(RustJsonCacheHandle handle);
    }

