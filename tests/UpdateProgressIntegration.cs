using System;
using System.IO;
using System.IO.Pipes;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Net;
using System.Net.Sockets;
using System.Collections.Generic;
using System.Web.Script.Serialization;
internal static class UpdateProgressIntegration{
 static JavaScriptSerializer Json=new JavaScriptSerializer();static string Pipe;
 static object Call(string name,params object[] args){try{return typeof(Updater).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);}catch(TargetInvocationException e){throw e.InnerException;}}
 static void Set(string name,object value){typeof(Updater).GetField(name,BindingFlags.NonPublic|BindingFlags.Static).SetValue(null,value);}
 static Dictionary<string,object> Request(string command){using(var p=new NamedPipeClientStream(".",Pipe,PipeDirection.InOut)){p.Connect(5000);var bytes=Encoding.UTF8.GetBytes(Json.Serialize(new{version=1,command=command})+"\n");p.Write(bytes,0,bytes.Length);p.Flush();return Json.Deserialize<Dictionary<string,object>>(new StreamReader(p).ReadLine());}}
 static string Server(bool known){var server=new TcpListener(IPAddress.Loopback,0);server.Start();int port=((IPEndPoint)server.LocalEndpoint).Port;new Thread(()=>{try{using(var client=server.AcceptTcpClient())using(var stream=client.GetStream()){var buffer=new byte[4096];stream.Read(buffer,0,buffer.Length);string header="HTTP/1.1 200 OK\r\nConnection: close\r\n"+(known?"Content-Length: 8192\r\n":"")+"\r\n";var bytes=Encoding.ASCII.GetBytes(header);stream.Write(bytes,0,bytes.Length);stream.Write(new byte[8192],0,8192);}}finally{server.Stop();}}){IsBackground=true}.Start();return "http://127.0.0.1:"+port+"/fixture";}
 static int Main(string[] args){Pipe=args[0];string root=args[1],report=args[2];Directory.CreateDirectory(root);try{
  Request("updateTestPrepare");Set("Authenticated",true);Set("Job","integration-test-job");Set("Nonce","integration-test-nonce");Set("Pipe",Pipe);Set("UpdateVersion","2.0.1");
  Call("Download",Server(true),Path.Combine(root,"update.zip"),1024L*1024);var known=(Dictionary<string,object>)Request("state")["softwareUpdate"];if(Convert.ToString(known["phase"])!="verifying"||Convert.ToInt32(known["percent"])!=100)throw new Exception("known byte progress failed");
  Request("updateTestPrepare");Call("Download",Server(false),Path.Combine(root,"update.zip"),1024L*1024);var unknown=(Dictionary<string,object>)Request("state")["softwareUpdate"];if(unknown["percent"]!=null||Convert.ToInt64(unknown["receivedBytes"])!=8192)throw new Exception("unknown length invented percent");
  Call("Report","ready",0L,null,"ready",true);Request("updateApply");string directive=(string)Call("Report","ready",0L,null,"ready",true);if(directive!="apply")throw new Exception("explicit apply handshake failed");
  Request("updateTestPrepare");Request("updateCancel");bool cancelled=false;try{Call("Report","downloading",0L,null,"cancel",true);}catch(OperationCanceledException){cancelled=true;}if(!cancelled)throw new Exception("cancel not honoured");
  File.WriteAllText(report,Json.Serialize(new{success=true,realStreamByteProgress=true,unknownLengthNoPercent=true,explicitApplyHandshake=true,cancel=true}));return 0;
 }catch(Exception e){Console.Error.WriteLine(e);return 1;}}
}
