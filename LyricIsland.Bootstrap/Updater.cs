using System;
using System.IO;
using System.IO.Compression;
using System.IO.Pipes;
using System.Net;
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Collections.Generic;
using System.Web.Script.Serialization;
using System.Windows.Forms;
using System.Drawing;
using System.Drawing.Drawing2D;

internal static class Updater {

    static string Job,Nonce,Pipe,Phase="downloading",Directive="",Message="",UpdateVersion="";static long Seq,Received;static long? Length;static DateTime LastReport=DateTime.MinValue;static Island Overlay;static bool Authenticated;
    static string Report(string phase,long received=0,long? length=null,string message="",bool force=true){
        Phase=phase;Received=received;Length=length;Message=message;
        if(!Authenticated)return "";
        if(!force&&(DateTime.UtcNow-LastReport).TotalMilliseconds<250)return Directive;
        LastReport=DateTime.UtcNow;
        try{using(var p=new NamedPipeClientStream(".",Pipe,PipeDirection.InOut,PipeOptions.Asynchronous)){p.Connect(1000);var json=new JavaScriptSerializer();string request=json.Serialize(new {version=1,command="updateProgress",value=new{jobId=Job,nonce=Nonce,seq=++Seq,phase=phase,receivedBytes=received,totalBytes=length,message=message}})+"\n";byte[] b=Encoding.UTF8.GetBytes(request);p.Write(b,0,b.Length);p.Flush();using(var reader=new StreamReader(p)){var task=reader.ReadLineAsync();if(task.Wait(2000)&&task.Result!=null){var reply=json.Deserialize<Dictionary<string,object>>(task.Result);object action;if(reply.TryGetValue("action",out action))Directive=Convert.ToString(action);}}}}
        catch{if(phase=="ready")throw new IOException("主程序已退出；未确认安装，已保留原版本。");}
        if(Directive=="cancel"&&phase!="applying"&&phase!="restarting"&&phase!="cancelled")throw new OperationCanceledException("更新已取消，原程序保持不变。");
        return Directive;
    }
    sealed class Island:Form{
        readonly System.Windows.Forms.Timer timer=new System.Windows.Forms.Timer();
        public Island(){FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;TopMost=true;StartPosition=FormStartPosition.Manual;BackColor=Color.FromArgb(21,26,30);Size=new Size(420,128);var area=Screen.PrimaryScreen.WorkingArea;Location=new Point(area.Left+(area.Width-Width)/2,area.Top+28);DoubleBuffered=true;using(var p=new GraphicsPath()){p.AddArc(0,0,48,48,180,90);p.AddArc(Width-48,0,48,48,270,90);p.AddArc(Width-48,Height-48,48,48,0,90);p.AddArc(0,Height-48,48,48,90,90);p.CloseFigure();Region=new Region(p);}timer.Interval=33;timer.Tick+=(a,b)=>Invalidate();timer.Start();}
        protected override bool ShowWithoutActivation{get{return true;}}
        protected override CreateParams CreateParams{get{var p=base.CreateParams;p.ExStyle|=0x08000080;return p;}}
        protected override void OnPaint(PaintEventArgs e){base.OnPaint(e);var g=e.Graphics;g.SmoothingMode=SmoothingMode.AntiAlias;float percent=Length.HasValue&&Length.Value>0?Math.Min(1f,(float)Received/Length.Value):-1;var ring=new Rectangle(24,35,54,54);using(var pen=new Pen(Color.FromArgb(48,65,55),4))g.DrawEllipse(pen,ring);using(var pen=new Pen(Color.FromArgb(141,214,173),4)){pen.StartCap=LineCap.Round;pen.EndCap=LineCap.Round;if(Phase=="downloading"&&percent>=0)g.DrawArc(pen,ring,-90,Math.Max(0.1f,percent*359.9f));else g.DrawArc(pen,ring,(Environment.TickCount%1200)/1200f*360,80);}
         using(var title=new Font("Microsoft YaHei UI",13,FontStyle.Bold))using(var body=new Font("Microsoft YaHei UI",9)){g.DrawString(Phase=="completed"?"更新已完成":Phase=="restarting"?"正在启动新版":"正在安装更新",title,Brushes.White,104,33);using(var brush=new SolidBrush(Color.FromArgb(169,181,191)))g.DrawString(Message.Length==0?"正在保留设置并替换应用文件…":Message,body,brush,new RectangleF(104,66,286,45));}}
    }
    static void ShowOverlay(){var ready=new ManualResetEvent(false);var thread=new Thread(()=>{Overlay=new Island();Overlay.Shown+=(a,b)=>ready.Set();Application.Run(Overlay);});thread.IsBackground=true;thread.SetApartmentState(ApartmentState.STA);thread.Start();if(!ready.WaitOne(5000))throw new IOException("安装进度窗口未就绪");}
    static void CloseOverlay(){try{if(Overlay!=null&&!Overlay.IsDisposed)Overlay.BeginInvoke((Action)(()=>Overlay.Close()));}catch{}}
    sealed class Client : WebClient {
        public Client(){Encoding=System.Text.Encoding.UTF8;Headers[HttpRequestHeader.UserAgent]="DotIsle-Updater";}
        protected override WebRequest GetWebRequest(Uri address) { var r=base.GetWebRequest(address);r.Timeout=120000;if(r is HttpWebRequest)((HttpWebRequest)r).ReadWriteTimeout=120000;return r; }
    }
    static string Hash(string path){using(var s=File.OpenRead(path))using(var h=SHA256.Create())return BitConverter.ToString(h.ComputeHash(s)).Replace("-", "").ToLowerInvariant();}
    static void Verify(string zip,string expected){if(!Regex.IsMatch(expected,@"^[0-9a-f]{64}$")||Hash(zip)!=expected)throw new IOException("下载校验失败，原程序保持不变");}
    static void Download(string url,string path,long limit){
        using(var c=new Client())using(var input=c.OpenRead(url))using(var output=File.Create(path)){
            long length;long? known=long.TryParse(c.ResponseHeaders[HttpResponseHeader.ContentLength],out length)&&length>0&&length<=limit?(long?)length:null;
            var buffer=new byte[65536];long total=0;int n;bool bundle=path.EndsWith("update.zip",StringComparison.OrdinalIgnoreCase);
            while((n=input.Read(buffer,0,buffer.Length))>0){total+=n;if(total>limit)throw new IOException("更新文件超过大小限制");output.Write(buffer,0,n);if(bundle)Report("downloading",total,known,"正在下载已选版本 "+UpdateVersion,false);}
            if(known.HasValue&&total!=known.Value)throw new IOException("下载长度不匹配");if(bundle)Report("verifying",total,known,"正在校验下载内容");
        }
    }
    static string Extract(string zip,string stage,string version){
        string folder="LyricIsland-"+version;
        string prefix=Path.GetFullPath(stage).TrimEnd(Path.DirectorySeparatorChar)+Path.DirectorySeparatorChar;
        using(var z=ZipFile.OpenRead(zip)){
            long total=0;var seen=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach(var e in z.Entries){
                if(!e.FullName.StartsWith(folder+"/",StringComparison.Ordinal))throw new IOException("更新包目录无效");
                string rel=e.FullName.Substring(folder.Length+1).Replace('\\','/');
                if(rel.Length==0)continue;
                string path=Path.GetFullPath(Path.Combine(stage,rel.Replace('/',Path.DirectorySeparatorChar)));
                if(!path.StartsWith(prefix,StringComparison.OrdinalIgnoreCase)||!seen.Add(path)||rel.Split('/')[0].Equals("Data",StringComparison.OrdinalIgnoreCase)||rel.Split('/')[0].Equals("Logs",StringComparison.OrdinalIgnoreCase))throw new IOException("更新包包含不安全路径");
                total+=e.Length;if(total>512L*1024*1024)throw new IOException("更新包超过大小限制");
                if(e.FullName.EndsWith("/",StringComparison.Ordinal)){Directory.CreateDirectory(path);continue;}
                Directory.CreateDirectory(Path.GetDirectoryName(path));e.ExtractToFile(path);
            }
        }
        foreach(string required in new[]{"LyricIsland.exe","LyricIsland.Settings.exe","LyricIsland.Updater.exe","e_sqlite3.dll","Assets/Fonts/NotoSansSC.ttf","Assets/Fonts/NotoSansJP.ttf"})if(!File.Exists(Path.Combine(stage,required)))throw new IOException("更新包缺少必要文件："+required);
        return stage;
    }
    internal static void Apply(string stage,string target,string backup,int failAfter,Action after=null){
        var changed=new List<string>();var existed=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        try{
            foreach(string input in Directory.GetFiles(stage,"*",SearchOption.AllDirectories)){
                string rel=input.Substring(stage.Length+1);string dest=Path.Combine(target,rel);string old=Path.Combine(backup,rel);
                if(File.Exists(dest)){Directory.CreateDirectory(Path.GetDirectoryName(old));File.Copy(dest,old);existed.Add(rel);}
                changed.Add(rel);Directory.CreateDirectory(Path.GetDirectoryName(dest));File.Copy(input,dest,true);
                if(failAfter>0&&changed.Count>=failAfter)throw new IOException("simulated write failure");
            }
            if(after!=null)after();
        }catch{
            var errors=new List<Exception>();changed.Reverse();foreach(string rel in changed){try{string dest=Path.Combine(target,rel);if(existed.Contains(rel))File.Copy(Path.Combine(backup,rel),dest,true);else if(File.Exists(dest))File.Delete(dest);}catch(Exception e){errors.Add(e);}}
            if(errors.Count>0)throw new IOException("更新回滚未完成，备份保留在："+backup,new AggregateException(errors));throw;
        }
    }
    internal static void StopApp(string pipe){using(var p=new NamedPipeClientStream(".",pipe,PipeDirection.InOut,PipeOptions.Asynchronous)){p.Connect(5000);byte[] b=Encoding.UTF8.GetBytes("{\"version\":1,\"command\":\"prepareUpdateExit\"}\n");p.Write(b,0,b.Length);p.Flush();using(var reader=new StreamReader(p)){var reply=reader.ReadLineAsync();if(!reply.Wait(5000)||reply.Result==null||!Regex.IsMatch(reply.Result,@"""success""\s*:\s*true"))throw new IOException("应用未确认保存设置，已取消更新");}}}
    internal static void Wait(int id){if(id<=0)return;try{using(var p=Process.GetProcessById(id)){if(!p.WaitForExit(15000))throw new IOException("应用尚未退出，更新已取消");}}catch(ArgumentException){}}
    internal static void Start(string target,string data){Process.Start(new ProcessStartInfo(Path.Combine(target,"LyricIsland.exe"),"--data-dir \""+data+"\""){UseShellExecute=false,WorkingDirectory=target});}
    static bool OfficialUrl(string url){Uri uri;return Uri.TryCreate(url,UriKind.Absolute,out uri)&&uri.Scheme=="https"&&(uri.Host=="github.com"||uri.Host=="gitee.com")&&string.IsNullOrEmpty(uri.UserInfo);}
    static string[] DomesticPair(string target,string version){
        string config=Path.Combine(target,"Assets/UpdateSources.json");if(!File.Exists(config))return null;
        var json=new JavaScriptSerializer();var settings=json.Deserialize<Dictionary<string,object>>(File.ReadAllText(config));object value;if(!settings.TryGetValue("giteeRepository",out value))return null;
        var match=Regex.Match(Convert.ToString(value).TrimEnd('/'),@"^https://gitee\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)$");if(!match.Success)return null;
        string api="https://gitee.com/api/v5/repos/"+match.Groups[1].Value+"/"+match.Groups[2].Value;
        using(var client=new Client()){var release=json.Deserialize<Dictionary<string,object>>(client.DownloadString(api+"/releases/tags/v"+version));long id=Convert.ToInt64(release["id"]);if(id<=0)throw new IOException("国内发行标识无效");var list=json.Deserialize<List<Dictionary<string,object>>>(client.DownloadString(api+"/releases/"+id+"/attach_files?per_page=100"));string manifest=null,zip=null;foreach(var file in list){object name;file.TryGetValue("name",out name);if(name==null)file.TryGetValue("file_name",out name);long fileId=Convert.ToInt64(file["id"]);if(fileId<=0)continue;string url=api+"/releases/"+id+"/attach_files/"+fileId+"/download";if(Convert.ToString(name)=="LyricIsland-"+version+"-SHA256.txt")manifest=url;if(Convert.ToString(name)=="LyricIsland-"+version+"-Portable.zip")zip=url;}if(manifest==null||zip==null)throw new IOException("国内源缺少更新附件");return new[]{manifest,zip};}
    }
    static void DownloadBundle(string target,string version,string scratch,string[] preferred){
        string name="LyricIsland-"+version,github="https://github.com/Heiamer/LyricIsland/releases/download/v"+version+"/";string pinnedHash=null;var errors=new List<string>();var tried=new HashSet<string>();
        // Resolve the other source on failure, even when the first checker selected GitHub.
        for(int step=0;step<3;step++){string[] pair=null;if(step==0){pair=preferred;}else if(step==1){try{pair=DomesticPair(target,version);}catch(Exception e){errors.Add("Gitee："+e.Message);continue;}}else{pair=new[]{github+name+"-SHA256.txt",github+name+"-Portable.zip"};}
            if(pair==null||!tried.Add(pair[0]))continue;
            Report(step==0?"downloading":"switching",0,null,step==0?"开始下载":"切换到备用更新源");
            try{if(!OfficialUrl(pair[0])||!OfficialUrl(pair[1]))throw new IOException("更新下载地址无效");string manifest=Path.Combine(scratch,"SHA256.txt"),zip=Path.Combine(scratch,"update.zip");Download(pair[0],manifest,65536);string expected=null;foreach(string line in File.ReadAllLines(manifest)){var m=Regex.Match(line,@"^([0-9a-fA-F]{64})\s+"+Regex.Escape(name+"-Portable.zip")+"$");if(m.Success)expected=m.Groups[1].Value.ToLowerInvariant();}if(expected==null)throw new IOException("校验清单缺少更新包");if(pinnedHash!=null&&pinnedHash!=expected)throw new InvalidDataException("同一版本的更新源摘要不一致，已停止更新。");pinnedHash=expected;Download(pair[1],zip,256L*1024*1024);Verify(zip,expected);return;}catch(OperationCanceledException){throw;}catch(InvalidDataException){throw;}catch(Exception e){errors.Add("更新源："+e.Message);File.WriteAllText(Path.Combine(scratch,"download-errors.txt"),string.Join(Environment.NewLine,errors));}
        }throw new IOException("国内源与 GitHub 均未能完成更新下载："+string.Join("；",errors));
    }
    static void SelfTest(string report){
        string root=Path.Combine(Path.GetTempPath(),"LyricIsland.UpdateTest."+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(root);
        try{
            string target=Path.Combine(root,"target"),stage=Path.Combine(root,"stage"),backup=Path.Combine(root,"backup");Directory.CreateDirectory(target);Directory.CreateDirectory(stage);Directory.CreateDirectory(Path.Combine(target,"Data"));File.WriteAllText(Path.Combine(target,"Data/settings.json"),"keep");File.WriteAllText(Path.Combine(target,"a"),"old");File.WriteAllText(Path.Combine(stage,"a"),"new");File.WriteAllText(Path.Combine(stage,"b"),"added");
            bool failed=false;try{Apply(stage,target,backup,1);}catch(IOException){failed=true;}if(!failed||File.ReadAllText(Path.Combine(target,"a"))!="old")throw new Exception("rollback failed");
            Apply(stage,target,Path.Combine(root,"backup2"),0);if(File.ReadAllText(Path.Combine(target,"a"))!="new"||File.ReadAllText(Path.Combine(target,"Data/settings.json"))!="keep")throw new Exception("apply failed");
            string zip=Path.Combine(root,"evil.zip");using(var z=ZipFile.Open(zip,ZipArchiveMode.Create)){using(var w=new StreamWriter(z.CreateEntry("LyricIsland-9.9/../escape").Open()))w.Write("bad");}
            bool rejected=false;try{Extract(zip,Path.Combine(root,"extract"),"9.9");}catch(IOException){rejected=true;}if(!rejected)throw new Exception("unsafe ZIP accepted");
            File.WriteAllText(report,"{\"success\":true,\"rollback\":true,\"preservesData\":true,\"rejectsTraversal\":true}");
        }finally{Directory.Delete(root,true);}
    }
    [STAThread]static int Main(string[] args){
        string target=null,data=null;bool stopped=false;string scratch=null;
        try{
            if(args.Length==2&&args[0]=="--self-test"){SelfTest(args[1]);return 0;}
            if(args.Length!=6&&args.Length!=8&&args.Length!=10)throw new ArgumentException("更新参数无效");
            string version=args[0];UpdateVersion=version;if(!Regex.IsMatch(version,@"^\d+\.\d+(\.\d+)?$"))throw new ArgumentException("版本号无效");
            target=Path.GetFullPath(args[1]).TrimEnd(Path.DirectorySeparatorChar);data=Path.GetFullPath(args[2]);int pid=int.Parse(args[3]),settings=int.Parse(args[4]);string pipe=args[5];
            if(args.Length==10){Job=args[8];Nonce=args[9];Pipe=pipe;Authenticated=true;}
            if(!File.Exists(Path.Combine(target,"LyricIsland.exe")))throw new IOException("找不到原应用");
            using(var gate=new Mutex(false,"Local.LyricIsland.OTA."+Hash(Path.Combine(target,"LyricIsland.exe")))){
                if(!gate.WaitOne(0))throw new IOException("已有更新正在进行");
                scratch=Path.Combine(Path.GetTempPath(),"LyricIsland.Update."+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(scratch);
                ServicePointManager.SecurityProtocol=SecurityProtocolType.Tls12;
                DownloadBundle(target,version,scratch,args.Length>=8?new[]{args[6],args[7]}:null);
                string zip=Path.Combine(scratch,"update.zip");
                string stage=Extract(zip,Path.Combine(scratch,"stage"),version);
                if(File.Exists(Path.Combine(target,"Installed.json"))){var json=new JavaScriptSerializer();var marker=json.Deserialize<Dictionary<string,object>>(File.ReadAllText(Path.Combine(target,"Installed.json")));var files=new HashSet<string>(StringComparer.OrdinalIgnoreCase);object existing;if(marker.TryGetValue("files",out existing))foreach(object file in (System.Collections.IEnumerable)existing)files.Add(Convert.ToString(file));foreach(string file in Directory.GetFiles(stage,"*",SearchOption.AllDirectories))files.Add(file.Substring(stage.Length+1));marker["version"]=version;marker["files"]=new List<string>(files);File.WriteAllText(Path.Combine(stage,"Installed.json"),json.Serialize(marker));}

                if(Authenticated){Report("ready",0,null,"校验完成，等待你点击重启并安装");while(Report("ready",0,null,"校验完成，等待你点击重启并安装")!="apply")Thread.Sleep(500);}
                Report("applying",0,null,"正在保存数据并安装");ShowOverlay();
                StopApp(pipe);stopped=true;Wait(pid);Wait(settings);
                Apply(stage,target,Path.Combine(scratch,"backup"),0,()=>{Phase="restarting";Message="正在启动并确认新版…";Start(target,data);var until=DateTime.UtcNow.AddSeconds(20);bool confirmed=false;while(DateTime.UtcNow<until){try{using(var client=new NamedPipeClientStream(".",pipe,PipeDirection.InOut,PipeOptions.Asynchronous)){client.Connect(500);byte[] bytes=Encoding.UTF8.GetBytes("{\"version\":1,\"command\":\"state\"}\n");client.Write(bytes,0,bytes.Length);using(var reader=new StreamReader(client)){var task=reader.ReadLineAsync();if(task.Wait(1000)&&task.Result!=null){var state=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(task.Result);if(Convert.ToString(state["version"])==version&&Convert.ToInt32(state["processId"])!=pid){confirmed=true;break;}}}}}catch{}Thread.Sleep(250);}if(!confirmed){try{StopApp(pipe);}catch{}Thread.Sleep(600);throw new IOException("新版未确认启动，准备回滚");}});stopped=false;Phase="completed";Message="新版已启动，设置已保留。";Thread.Sleep(1200);CloseOverlay();
                // Keep recovery data until the next update; never delete a failed rollback backup.
                File.WriteAllText(Path.Combine(scratch,"success.txt"),version);return 0;
            }
        }catch(OperationCanceledException){try{Directive="";Report("cancelled",0,null,"更新已取消，原程序保持不变。");}catch{}CloseOverlay();return 2;}catch(Exception e){CloseOverlay();try{Report("failed",0,null,e.Message);}catch{}if(stopped){try{Start(target,data);}catch{}}MessageBox.Show("在线更新未完成："+e.Message+(scratch==null?"":"\n诊断和备份："+scratch),"点屿在线更新",MessageBoxButtons.OK,MessageBoxIcon.Error);return 1;}
    }
}
