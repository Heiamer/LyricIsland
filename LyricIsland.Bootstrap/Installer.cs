using System;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;
using System.Web.Script.Serialization;
using System.Threading;
using Microsoft.Win32;

// Per-user installation. Never elevates or removes user data.
internal static class Installer {
    const string RegistryPath=@"Software\Microsoft\Windows\CurrentVersion\Uninstall\DotIsle";
    static JavaScriptSerializer Json=new JavaScriptSerializer();
    static string MigrationData,TestPipe;
    static string Pipe {get{return TestPipe??Environment.GetEnvironmentVariable("LYRICISLAND_TEST_PIPE")??"LyricIsland.Native.Settings.v1";}}
    static string Quote(string value){return "\""+value.Replace("\"", "")+"\"";}
    static string Digest(Stream stream){using(var hash=SHA256.Create())return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();}
    static string SafeChild(string root,string relative){
        if(string.IsNullOrEmpty(relative)||Path.IsPathRooted(relative)||relative.Contains(":")||relative.Split('/','\\')[0].Equals("Data",StringComparison.OrdinalIgnoreCase)||relative.Split('/','\\')[0].Equals("Logs",StringComparison.OrdinalIgnoreCase))throw new IOException("安装清单路径无效");
        string full=Path.GetFullPath(Path.Combine(root,relative.Replace('/',Path.DirectorySeparatorChar)));
        if(!full.StartsWith(Path.GetFullPath(root).TrimEnd('\\')+"\\",StringComparison.OrdinalIgnoreCase))throw new IOException("安装路径越界");return full;
    }
    static string Extract(string directory){
        using(var resource=Assembly.GetExecutingAssembly().GetManifestResourceStream("DotIsle.InstallPayload.zip")){
            if(resource==null)throw new IOException("安装组件缺失");if(Digest(resource)!=BundleVersion.PayloadHash)throw new IOException("安装组件校验失败");resource.Position=0;
            using(var zip=new ZipArchive(resource,ZipArchiveMode.Read)){
                long total=0;var seen=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                foreach(var entry in zip.Entries){string prefix="LyricIsland-"+BundleVersion.Version+"/";if(!entry.FullName.StartsWith(prefix,StringComparison.Ordinal))throw new IOException("组件目录无效");string relative=entry.FullName.Substring(prefix.Length);if(relative.Length==0||entry.FullName.EndsWith("/"))continue;string path=SafeChild(directory,relative);if(!seen.Add(path))throw new IOException("重复组件");total+=entry.Length;if(total>512L*1024*1024)throw new IOException("安装组件超过大小限制");Directory.CreateDirectory(Path.GetDirectoryName(path));entry.ExtractToFile(path);}
            }
        }
        foreach(string file in new[]{"LyricIsland.exe","LyricIsland.Settings.exe","LyricIsland.Updater.exe","Uninstall.exe","Assets/Fonts/NotoSansSC.ttf","Assets/Fonts/NotoSansJP.ttf"})if(!File.Exists(Path.Combine(directory,file)))throw new IOException("缺少组件："+file);
        return directory;
    }
    static Dictionary<string,object> ReadMarker(string target){return Json.Deserialize<Dictionary<string,object>>(File.ReadAllText(Path.Combine(target,"Installed.json")));}
    static string NormalizeTarget(string target){
        target=Path.GetFullPath(target.Trim()).TrimEnd('\\');
        if(target.Length<=3||target.Equals(Environment.GetFolderPath(Environment.SpecialFolder.Windows),StringComparison.OrdinalIgnoreCase)||target.StartsWith(Environment.GetFolderPath(Environment.SpecialFolder.Windows)+"\\",StringComparison.OrdinalIgnoreCase))throw new IOException("请选择独立的应用文件夹");
        if(Directory.Exists(target)&&Directory.GetFileSystemEntries(target).Length>0&&!File.Exists(Path.Combine(target,"Installed.json")))throw new IOException("所选文件夹不为空，请选择空文件夹或点屿原安装目录。");
        return target;
    }
    static void CopyData(string source,string target){
        if(string.IsNullOrEmpty(source)||!Directory.Exists(source)||Path.GetFullPath(source).TrimEnd('\\').Equals(Path.GetFullPath(target).TrimEnd('\\'),StringComparison.OrdinalIgnoreCase))return;
        string prefix=Path.GetFullPath(source).TrimEnd('\\')+"\\";Directory.CreateDirectory(target);
        var pending=new Queue<string>();pending.Enqueue(source);long total=0;
        while(pending.Count>0){string directory=pending.Dequeue();foreach(string child in Directory.GetDirectories(directory)){if((File.GetAttributes(child)&FileAttributes.ReparsePoint)!=0)throw new IOException("迁移数据目录包含链接，原数据已保留。");if(Path.GetFileName(child).Equals("Logs",StringComparison.OrdinalIgnoreCase))continue;pending.Enqueue(child);}
            foreach(string path in Directory.GetFiles(directory)){var info=new FileInfo(path);if((info.Attributes&FileAttributes.ReparsePoint)!=0)throw new IOException("迁移数据包含链接");total+=info.Length;if(total>1024L*1024*1024)throw new IOException("迁移数据超过1GB，请先整理缓存；原数据未删除。");string relative=path.Substring(prefix.Length);string output=Path.Combine(target,relative);Directory.CreateDirectory(Path.GetDirectoryName(output));if(!File.Exists(output))File.Copy(path,output,false);}
        }
    }

    sealed class Running{public string Exe,Data;}
    static Running StopRunning(){
        Dictionary<string,object> state;
        try{using(var p=new System.IO.Pipes.NamedPipeClientStream(".",Pipe,System.IO.Pipes.PipeDirection.InOut,System.IO.Pipes.PipeOptions.Asynchronous)){
            p.Connect(600);var bytes=Encoding.UTF8.GetBytes("{\"version\":1,\"command\":\"state\"}\n");p.Write(bytes,0,bytes.Length);p.Flush();using(var reader=new StreamReader(p)){var task=reader.ReadLineAsync();if(!task.Wait(3000)||task.Result==null)throw new IOException("应用未响应；请先退出点屿。");state=Json.Deserialize<Dictionary<string,object>>(task.Result);}}
        }catch(TimeoutException){return null;}
        int pid=Convert.ToInt32(state["processId"]);string exe;using(var process=Process.GetProcessById(pid))exe=process.MainModule.FileName;
        object value;string data=state.TryGetValue("dataDirectory",out value)?Convert.ToString(value):MigrationData;
        if(string.IsNullOrEmpty(data)&&File.Exists(Path.Combine(Path.GetDirectoryName(exe),"Data","settings.json")))data=Path.Combine(Path.GetDirectoryName(exe),"Data");
        Updater.StopApp(Pipe);Updater.Wait(pid);return new Running{Exe=exe,Data=data};
    }
    static void Shortcut(string path,string target){
        var type=Type.GetTypeFromProgID("WScript.Shell");object shell=Activator.CreateInstance(type);object shortcut=type.InvokeMember("CreateShortcut",BindingFlags.InvokeMethod,null,shell,new object[]{path});
        shortcut.GetType().InvokeMember("TargetPath",BindingFlags.SetProperty,null,shortcut,new object[]{target});shortcut.GetType().InvokeMember("WorkingDirectory",BindingFlags.SetProperty,null,shortcut,new object[]{Path.GetDirectoryName(target)});shortcut.GetType().InvokeMember("Save",BindingFlags.InvokeMethod,null,shortcut,new object[0]);
        System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shortcut);System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shell);
    }
    static void Register(string target){
        using(var key=Registry.CurrentUser.CreateSubKey(RegistryPath)){
            key.SetValue("DisplayName","点屿 · DotIsle");key.SetValue("DisplayVersion",BundleVersion.Version);key.SetValue("Publisher","DotIsle");key.SetValue("InstallLocation",target);key.SetValue("DisplayIcon",Path.Combine(target,"LyricIsland.exe"));key.SetValue("UninstallString",Quote(Path.Combine(target,"Uninstall.exe"))+" --uninstall");key.SetValue("NoModify",1);key.SetValue("NoRepair",1);
        }
        string pointer=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"DotIsle");Directory.CreateDirectory(pointer);File.WriteAllText(Path.Combine(pointer,"install.json"),Json.Serialize(new{target=target}));
        string programs=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs),"DotIsle");Directory.CreateDirectory(programs);Shortcut(Path.Combine(programs,"点屿.lnk"),Path.Combine(target,"LyricIsland.exe"));
        using(var startup=Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run")){if(startup.GetValue("LyricIsland")!=null)startup.SetValue("LyricIsland",Quote(Path.Combine(target,"LyricIsland.exe")));}
    }
    static void Install(string requested,bool launch,bool register){
        string target=NormalizeTarget(requested);string scratch=Path.Combine(Path.GetTempPath(),"DotIsle.Setup."+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(scratch);string stage=Extract(Path.Combine(scratch,"stage"));
        string data=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),"DotIsle");
        if(File.Exists(Path.Combine(target,"Installed.json"))){object existing;if(ReadMarker(target).TryGetValue("dataDirectory",out existing))data=Path.GetFullPath(Convert.ToString(existing));}
        if(!register)data=Path.Combine(scratch,"test-data");
        var files=new List<string>();foreach(string path in Directory.GetFiles(stage,"*",SearchOption.AllDirectories))files.Add(path.Substring(stage.Length+1));files.Add("Installed.json");
        File.WriteAllText(Path.Combine(stage,"Installed.json"),Json.Serialize(new{product="DotIsle",version=BundleVersion.Version,dataDirectory=data,files=files}),new UTF8Encoding(false));
        Directory.CreateDirectory(target);string probe=Path.Combine(target,".dotisle-write-"+Guid.NewGuid().ToString("N"));File.WriteAllText(probe,"");File.Delete(probe);
        Running previous=StopRunning();
        if(string.IsNullOrEmpty(MigrationData)&&previous!=null)MigrationData=previous.Data;
        try{
        if(!string.IsNullOrEmpty(MigrationData)&&Directory.Exists(MigrationData)){
            if(File.Exists(Path.Combine(data,"settings.json"))&&!Path.GetFullPath(MigrationData).Equals(Path.GetFullPath(data),StringComparison.OrdinalIgnoreCase)){CopyData(MigrationData,Path.Combine(data,"MigrationBackups",DateTime.Now.ToString("yyyyMMdd-HHmmss")));}
            else CopyData(MigrationData,data);
        }
        Updater.Apply(stage,target,Path.Combine(scratch,"backup"),0);
        if(register)Register(target);if(launch)Updater.Start(target,data);
        }catch{if(previous!=null&&File.Exists(previous.Exe)){try{var info=new ProcessStartInfo(previous.Exe,string.IsNullOrEmpty(previous.Data)?"":"--data-dir "+Quote(previous.Data)){UseShellExecute=false,WorkingDirectory=Path.GetDirectoryName(previous.Exe)};Process.Start(info);}catch{}}throw;}
    }
    static void Uninstall(string target){
        target=Path.GetFullPath(target).TrimEnd('\\');var marker=ReadMarker(target);if(Convert.ToString(marker["product"])!="DotIsle")throw new IOException("安装标识无效");
        using(var key=Registry.CurrentUser.OpenSubKey(RegistryPath)){if(key==null||!target.Equals(Convert.ToString(key.GetValue("InstallLocation")),StringComparison.OrdinalIgnoreCase))throw new IOException("卸载目录与安装记录不一致");}
        if(MessageBox.Show("卸载点屿？设置、待办和流程模板将保留。","点屿",MessageBoxButtons.OKCancel,MessageBoxIcon.Question)!=DialogResult.OK)return;
        StopRunning();foreach(object file in (System.Collections.IEnumerable)marker["files"]){string path=SafeChild(target,Convert.ToString(file));if(File.Exists(path))File.Delete(path);}
        Registry.CurrentUser.DeleteSubKeyTree(RegistryPath,false);
        string pointer=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"DotIsle","install.json");if(File.Exists(pointer)){var item=Json.Deserialize<Dictionary<string,object>>(File.ReadAllText(pointer));if(Convert.ToString(item["target"]).Equals(target,StringComparison.OrdinalIgnoreCase))File.Delete(pointer);}
        string shortcut=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs),"DotIsle","点屿.lnk");if(File.Exists(shortcut))File.Delete(shortcut);
        using(var key=Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run",true)){if(key!=null&&Convert.ToString(key.GetValue("LyricIsland")).Contains(target))key.DeleteValue("LyricIsland",false);}
        MessageBox.Show("点屿已卸载，用户数据已保留。","点屿");
    }
    sealed class Wizard:Form {
        TextBox folder=new TextBox();Button install=new Button();Label status=new Label();
        public Wizard(){Text="安装点屿 · DotIsle "+BundleVersion.Version;ClientSize=new Size(620,370);AutoScaleMode=AutoScaleMode.Dpi;FormBorderStyle=FormBorderStyle.FixedDialog;MaximizeBox=false;StartPosition=FormStartPosition.CenterScreen;BackColor=Color.FromArgb(247,248,246);Font=new Font("Microsoft YaHei UI",10);
            var title=new Label{Text="让重要的事，一眼可见。",Location=new Point(30,30),Size=new Size(550,45),Font=new Font(Font.FontFamily,19,FontStyle.Bold)};Controls.Add(title);
            Controls.Add(new Label{Text="安装点屿  "+BundleVersion.Version+"\n选择安装位置；你的设置和待办会保留。",Location=new Point(32,88),Size=new Size(550,54)});
            folder.Location=new Point(32,167);folder.Size=new Size(450,30);folder.Text=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"Programs","DotIsle");using(var key=Registry.CurrentUser.OpenSubKey(RegistryPath)){if(key!=null)folder.Text=Convert.ToString(key.GetValue("InstallLocation",folder.Text));}Controls.Add(folder);
            var browse=new Button{Text="浏览…",Location=new Point(495,165),Size=new Size(90,34)};browse.Click+=(a,b)=>{using(var dialog=new FolderBrowserDialog()){dialog.Description="选择点屿安装文件夹（需为空或已有点屿安装目录）";dialog.SelectedPath=folder.Text;if(dialog.ShowDialog(this)==DialogResult.OK)folder.Text=dialog.SelectedPath;}};Controls.Add(browse);
            status.Location=new Point(32,216);status.Size=new Size(550,68);status.Text="安装到当前用户，无需管理员权限。\n旧单文件版数据不会被删除；已有安装数据优先，迁移冲突保留备份。";status.ForeColor=Color.FromArgb(99,109,105);Controls.Add(status);
            install.Text="安装并启动";install.Location=new Point(430,300);install.Size=new Size(155,42);install.BackColor=Color.FromArgb(20,130,103);install.ForeColor=Color.White;install.FlatStyle=FlatStyle.Flat;
            install.Click+=async(a,b)=>{try{string path=folder.Text;install.Enabled=false;browse.Enabled=false;folder.Enabled=false;status.Text="正在校验组件、保存数据并安装…";await System.Threading.Tasks.Task.Run(()=>Install(path,true,true));Close();}catch(Exception e){status.Text=e.Message;install.Enabled=true;browse.Enabled=true;folder.Enabled=true;}};Controls.Add(install);
        }
    }
    [STAThread]static int Main(string[] args){
        try{Application.EnableVisualStyles();
            if(args.Length==2&&args[0]=="--verify-package"){string root=Path.GetFullPath(args[1]);Extract(root);File.WriteAllText(Path.Combine(root,"verification.json"),Json.Serialize(new{success=true,version=BundleVersion.Version,installer=true}));return 0;}
            if((args.Length==3||args.Length==5)&&args[0]=="--test-install"){if(args.Length==5&&args[3]=="--migrate-data")MigrationData=args[4];TestPipe="LyricIsland.Settings.test.installer."+Guid.NewGuid().ToString("N");Install(args[1],false,false);File.WriteAllText(args[2],Json.Serialize(new{success=true,installed=File.Exists(Path.Combine(args[1],"Installed.json")),registered=false,launched=false}));return 0;}
            if(args.Length==2&&args[0]=="--uninstall-worker"){Uninstall(args[1]);return 0;}
            if(args.Length==1&&args[0]=="--uninstall"){string self=Assembly.GetExecutingAssembly().Location;string copy=Path.Combine(Path.GetTempPath(),"DotIsle.Uninstall."+Guid.NewGuid().ToString("N")+".exe");File.Copy(self,copy);Process.Start(copy,"--uninstall-worker "+Quote(Path.GetDirectoryName(self)));return 0;}
            for(int i=0;i+1<args.Length;i++)if(args[i]=="--migrate-data")MigrationData=Path.GetFullPath(args[i+1]);
            using(var mutex=new Mutex(false,"Local.DotIsle.Setup")){if(!mutex.WaitOne(0))return 0;try{Application.Run(new Wizard());}finally{mutex.ReleaseMutex();}}return 0;
        }catch(Exception e){if(args.Length>0&&(args[0]=="--verify-package"||args[0]=="--test-install")){Console.Error.WriteLine(e);return 1;}MessageBox.Show(e.Message,"点屿安装",MessageBoxButtons.OK,MessageBoxIcon.Error);return 1;}
    }
}
