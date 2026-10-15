//! Pipe transport stays native; the callback only dispatches GUI operations in WPF.
use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Instant,Duration},ffi::{c_char,CStr}};
use windows::{core::PCWSTR,Win32::{Foundation::{HANDLE,CloseHandle,GetLastError,ERROR_PIPE_CONNECTED},System::{Pipes::*,Com::CoTaskMemFree},Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE,PIPE_ACCESS_DUPLEX,FlushFileBuffers,ReadFile,WriteFile}}};
pub type GuiCallback=unsafe extern "C" fn(*const c_char)->*mut c_char;
pub struct Server {closed:Arc<AtomicBool>,finished:Arc<AtomicBool>}
impl Server{
    pub fn new(name:&str,callback:GuiCallback)->Self{
        let name=format!("\\\\.\\pipe\\{name}").encode_utf16().chain(Some(0)).collect::<Vec<_>>();let closed=Arc::new(AtomicBool::new(false));let worker_closed=closed.clone();
        let finished=Arc::new(AtomicBool::new(false));let done=finished.clone();
        std::thread::spawn(move||unsafe{
            while !worker_closed.load(Ordering::Acquire){
                // Local-only pipe, remote clients rejected by the pipe type. The request is buffered
                // first under a 16 KiB cap, then the client token SID is checked with
                // ImpersonateNamedPipeClient before the callback runs: nothing is acted on and no
                // data is returned until that check passes.
                let pipe=CreateNamedPipeW(PCWSTR(name.as_ptr()),PIPE_ACCESS_DUPLEX|FILE_FLAG_FIRST_PIPE_INSTANCE,PIPE_TYPE_BYTE|PIPE_READMODE_BYTE|PIPE_NOWAIT|PIPE_REJECT_REMOTE_CLIENTS,1,65536,16384,1000,None);
                if pipe.is_invalid(){std::thread::sleep(Duration::from_millis(150));continue}
                let connection=(||{
                    loop{
                        if worker_closed.load(Ordering::Acquire){return None}
                        if ConnectNamedPipe(pipe,None).is_ok(){break}
                        if GetLastError()==ERROR_PIPE_CONNECTED{break}
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    let deadline=Instant::now()+Duration::from_secs(8);let mut input=Vec::new();
                    loop{
                        if worker_closed.load(Ordering::Acquire){return None}
                        if Instant::now()>deadline{return dropped("request timeout")}
                        let mut available=0;if PeekNamedPipe(pipe,None,0,None,Some(&mut available),None).is_err(){return dropped("peek failed")}
                        if available==0{std::thread::sleep(Duration::from_millis(10));continue}
                        let mut bytes=vec![0;available.min(16384) as usize];let mut read=0;
                        if ReadFile(pipe,Some(&mut bytes),Some(&mut read),None).is_err(){return dropped("read failed")}
                        input.extend_from_slice(&bytes[..read as usize]);
                        if input.len()>16384{return dropped("request too large")}if let Some(end)=input.iter().position(|b|*b==b'\n'){input.truncate(end);break}
                    }
                    if !same_user(pipe){return dropped("client identity rejected")}
                    let Some(request)=std::ffi::CString::new(input).ok()else{return dropped("request is not a C string")};
                    let result=callback(request.as_ptr());if result.is_null(){return dropped("GUI returned no response")}
                    let mut response=CStr::from_ptr(result).to_bytes().to_vec();CoTaskMemFree(Some(result as _));response.push(b'\n');
                    if response.len()>1024*1024{return dropped("response too large")}
                    let mut written=0;if WriteFile(pipe,Some(&response),Some(&mut written),None).is_err(){return dropped("write failed")}
                    // DisconnectNamedPipe discards whatever the client has not consumed yet, so the
                    // response has to be drained first: FlushFileBuffers on a server handle returns
                    // once the client has read everything. Without this the client races the
                    // disconnect and fails with ERROR_PIPE_NOT_CONNECTED (0x800700E9).
                    // Best effort: a failure here only means the client already closed its end
                    // (nothing left to wait for), so it is not reported as a dropped request.
                    let _=FlushFileBuffers(pipe);
                    Some(())
                })();
                let _=connection;let _=DisconnectNamedPipe(pipe);let _=CloseHandle(pipe);
            }
            done.store(true,Ordering::Release);
        });Self{closed,finished}
    }
    pub fn close(&self){self.closed.store(true,Ordering::Release)}
    pub fn finished(&self)->bool{self.finished.load(Ordering::Acquire)}
}
impl Drop for Server{fn drop(&mut self){self.close()}}
/// Record why a connection was abandoned, then report it as no response.
fn dropped(reason:&str)->Option<()>{
    crate::io::log(&crate::io::data_directory().join("Logs"),"IPC_DROP",&format!("settings pipe connection dropped: {reason}"));
    None
}
unsafe fn same_user(pipe:HANDLE)->bool{
    use windows::Win32::{System::Threading::{GetCurrentProcess,GetCurrentThread,OpenProcessToken,OpenThreadToken},Security::{GetTokenInformation,TokenUser,TOKEN_QUERY,TOKEN_USER,EqualSid}};
    fn token_user(token:HANDLE)->Option<Vec<u8>>{unsafe{let mut length=0;let _=GetTokenInformation(token,TokenUser,None,0,&mut length);if length==0||length>65536{return None}let mut buffer=vec![0u8;length as usize];GetTokenInformation(token,TokenUser,Some(buffer.as_mut_ptr() as _),length,&mut length).ok()?;Some(buffer)}}
    let mut own=HANDLE::default();if OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut own).is_err(){return false}let own_data=token_user(own);let _=CloseHandle(own);
    if ImpersonateNamedPipeClient(pipe).is_err(){return false}let mut client=HANDLE::default();let opened=OpenThreadToken(GetCurrentThread(),TOKEN_QUERY,true,&mut client).is_ok();
    let data=if opened{let d=token_user(client);let _=CloseHandle(client);d}else{None};let _=windows::Win32::Security::RevertToSelf();
    match (own_data,data){(Some(a),Some(b))=>{let a=&*(a.as_ptr() as *const TOKEN_USER);let b=&*(b.as_ptr() as *const TOKEN_USER);EqualSid(a.User.Sid,b.User.Sid).is_ok()},_=>false}
}

