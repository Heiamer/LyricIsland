using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.Win32.SafeHandles;
namespace TaskbarLyrics.Core;

// Marshaling only: every non-GUI operation is implemented by lyric-backend in Rust.
public static class NativeBackend
{
    private const string Library = "lyric_ffi";
    public static JsonElement Json(Func<IntPtr> call)
    {
        var pointer=call();
        if(pointer==IntPtr.Zero) throw new IOException("Rust 后台未返回数据。");
        try
        {
            using var document=JsonDocument.Parse(Marshal.PtrToStringUTF8(pointer) ?? "{}");
            var result=document.RootElement.Clone();
            if(result.ValueKind==JsonValueKind.Object && result.TryGetProperty("error",out var error)) throw new IOException(error.GetString());
            return result;
        }
        finally { li_free_string(pointer); }
    }
    public static JsonElement Io(string action,string argument="",object? payload=null)=>Json(()=>li_native_io(action,argument,JsonSerializer.Serialize(payload)));
    public static BackendHandle Create(object settings,string data,string local,string logs)
    {
        var id=li_backend_create(JsonSerializer.Serialize(settings),data,local,logs);
        if(id==0) throw new IOException("无法初始化 Rust 后台。");
        return new(id);
    }
    public static JsonElement Snapshot(BackendHandle handle,ulong coverVersion)=>Json(()=>li_backend_snapshot(handle,coverVersion));
    public static void Apply(BackendHandle handle,object settings) { if(li_backend_apply(handle,JsonSerializer.Serialize(settings))!=1) throw new IOException("Rust 设置应用失败。"); }
    public static bool Command(BackendHandle handle,string action,double seconds=0)=>Json(()=>li_backend_command(handle,action,seconds)).GetProperty("success").GetBoolean();
    public static string BuildInfo()=>Json(()=>li_build_info()).GetRawText();
    public static string Version { get { var p=li_version();return Marshal.PtrToStringUTF8(p)??"unknown"; } }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] public delegate IntPtr GuiCallback([MarshalAs(UnmanagedType.LPUTF8Str)] string request);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] public static extern ulong li_settings_server_create([MarshalAs(UnmanagedType.LPUTF8Str)] string name,GuiCallback callback);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] public static extern void li_settings_server_close(ulong handle);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] public static extern int li_settings_server_finished(ulong handle);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] public static extern void li_settings_server_destroy(ulong handle);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr li_native_io([MarshalAs(UnmanagedType.LPUTF8Str)] string action,[MarshalAs(UnmanagedType.LPUTF8Str)] string argument,[MarshalAs(UnmanagedType.LPUTF8Str)] string payload);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern ulong li_backend_create([MarshalAs(UnmanagedType.LPUTF8Str)] string settings,[MarshalAs(UnmanagedType.LPUTF8Str)] string data,[MarshalAs(UnmanagedType.LPUTF8Str)] string local,[MarshalAs(UnmanagedType.LPUTF8Str)] string logs);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] internal static extern void li_backend_destroy(ulong handle);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr li_backend_snapshot(BackendHandle handle,ulong lastCover);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern int li_backend_apply(BackendHandle handle,[MarshalAs(UnmanagedType.LPUTF8Str)] string settings);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr li_backend_command(BackendHandle handle,[MarshalAs(UnmanagedType.LPUTF8Str)] string action,double seconds);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern void li_free_string(IntPtr pointer);
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr li_build_info();
    [DllImport(Library,CallingConvention=CallingConvention.Cdecl)] private static extern IntPtr li_version();
}
public sealed class BackendHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal BackendHandle(ulong id):base(true)=>SetHandle(unchecked((IntPtr)(long)id));
    protected override bool ReleaseHandle(){NativeBackend.li_backend_destroy(unchecked((ulong)handle.ToInt64()));return true;}
}
public static class RustLyricEngine
{
    public static bool IsActive { get { try{return NativeBackend.Version=="1.0";}catch(DllNotFoundException){return false;}catch(EntryPointNotFoundException){return false;} } }
    public static string? Version=>NativeBackend.Version;
    public static string Describe()=>"rust "+Version+" · native desktop backend";
}
