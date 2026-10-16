using System;
using System.IO;
using System.IO.Pipes;
using System.Diagnostics;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Collections.Generic;
internal static class UpdaterIntegration {
    static object Call(string name,params object[] args){try{return typeof(Updater).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);}catch(TargetInvocationException e){throw e.InnerException;}}
    static string Request(string pipe,string command){using(var p=new NamedPipeClientStream(".",pipe,PipeDirection.InOut)){p.Connect(5000);byte[] b=Encoding.UTF8.GetBytes("{\"version\":1,\"command\":\""+command+"\"}\n");p.Write(b,0,b.Length);p.Flush();return new StreamReader(p).ReadLine();}}
    static string State(string pipe){for(int i=0;i<50;i++){try{return Request(pipe,"state");}catch{Thread.Sleep(100);}}throw new Exception("app pipe unavailable");}
    static int Main(string[] args){
        string zip=Path.GetFullPath(args[0]),expected=args[1],root=Path.GetFullPath(args[2]),report=Path.GetFullPath(args[3]);Directory.CreateDirectory(root);
        string target=Path.Combine(root,"app"),stage=Path.Combine(root,"stage"),data=Path.Combine(target,"Data"),pipe="LyricIsland.Settings.test.ota."+Guid.NewGuid().ToString("N");Process old=null;
        Environment.SetEnvironmentVariable("LYRICISLAND_TEST_PIPE",pipe);
        try{
            Call("Verify",zip,expected);bool rejected=false;try{Call("Verify",zip,new string('0',64));}catch(IOException){rejected=true;}if(!rejected)throw new Exception("bad checksum accepted");
            Call("Extract",zip,target,"1.2.1");Call("Extract",zip,stage,"1.2.1");Directory.CreateDirectory(data);File.WriteAllText(Path.Combine(data,"settings.json"),"{\"CompactWidth\":317,\"MusicClickOpenApp\":true,\"NotificationsEnabled\":false,\"AllowedSources\":[\"LyricIsland.OTATest\"],\"OnlineSongInfo\":false}");
            old=Process.Start(new ProcessStartInfo(Path.Combine(target,"LyricIsland.exe"),"--data-dir \""+data+"\""){UseShellExecute=false,WorkingDirectory=target});
            if(!State(pipe).Contains("317"))throw new Exception("initial settings missing");int oldId=old.Id;Call("StopApp",pipe);Call("Wait",oldId);
            Call("Apply",stage,target,Path.Combine(root,"backup"),0,(Action)(()=>Call("Start",target,data)));
            string state=State(pipe);if(!state.Contains("317")||!state.Contains("\"MusicClickOpenApp\":true")||state.Contains("\"processId\":"+oldId+","))throw new Exception("settings/restart failed");
            Request(pipe,"exit");File.WriteAllText(report,"{\"success\":true,\"checksumFailureRejected\":true,\"originalDirectoryUpdated\":true,\"restarted\":true,\"preservedSettings\":true}");return 0;
        }catch(Exception e){Console.Error.WriteLine(e);return 1;}finally{try{Request(pipe,"exit");}catch{}if(old!=null){if(!old.HasExited)old.Kill();old.Dispose();}}
    }
}
