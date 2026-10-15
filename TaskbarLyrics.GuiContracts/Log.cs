namespace TaskbarLyrics.Core.Utilities;
public static class Log
{
    public enum Level { Debug,Info,Warn,Error }
    private static bool _verbose;
    public static void SetVerboseEnabled(bool enabled)=>_verbose=enabled;
    public static string GetLogsDirectory()=>Path.Combine(AppContext.BaseDirectory,"Logs");
    public static string GetDebugLogPath()=>Path.Combine(GetLogsDirectory(),"app_debug.log");
    public static string GetErrorLogPath()=>Path.Combine(GetLogsDirectory(),"app_error.log");
    public static void EnsureLogsDirectory()=>Diagnostic("GUI","Log initialized");
    public static void Write(Level level,string message){if(_verbose||level>=Level.Warn)Diagnostic(level.ToString().ToUpperInvariant(),message);}
    public static void Diagnostic(string category,string message){try{NativeBackend.Io("log",GetLogsDirectory(),new{category,message});}catch(IOException){}catch(DllNotFoundException){}catch(EntryPointNotFoundException){}}
    public static void Debug(string message)=>Write(Level.Debug,message);
    public static void Info(string message)=>Write(Level.Info,message);
    public static void Warn(string message)=>Write(Level.Warn,message);
    public static void Error(string message)=>Write(Level.Error,message);
}
