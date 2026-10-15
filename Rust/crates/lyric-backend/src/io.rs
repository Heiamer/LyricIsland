//! Native configuration, logs and IPC client used by Slint and the WPF bridge.
use std::{path::{Path,PathBuf},io::{Read,Write},time::{Instant,Duration},sync::{Mutex,OnceLock}};
use windows::{core::PCWSTR,Win32::{Foundation::HANDLE,System::Pipes::{WaitNamedPipeW,PeekNamedPipe}}};
use serde_json::Value;
pub fn data_directory()->PathBuf{PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("LyricIsland")}
pub fn read_settings(path:&Path)->Value{
    if std::fs::metadata(path).is_ok_and(|m|m.len()<=1024*1024){if let Ok(file)=std::fs::File::open(path){if let Ok(value)=serde_json::from_reader(file){return value}}}
    serde_json::json!({})
}
pub fn save_settings(path:&Path,settings:&Value)->Result<(),String>{crate::storage::atomic_write(path,&serde_json::to_vec_pretty(settings).map_err(|e|e.to_string())?)}
pub fn pipe_request(pipe:&str,request:&Value)->Result<Value,String>{
    use std::os::windows::io::AsRawHandle;
    let path=format!("\\\\.\\pipe\\{pipe}");let wide=path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // Retry transient gaps before sending bytes; never replay a mutation.
    let connect_until=Instant::now()+Duration::from_secs(2);
    let mut file=loop{
        if unsafe{WaitNamedPipeW(PCWSTR(wide.as_ptr()),100).as_bool()}{
            match std::fs::OpenOptions::new().read(true).write(true).open(&path){Ok(file)=>break file,Err(error) if !matches!(error.raw_os_error(),Some(2|231|535))=>return Err(error.to_string()),_=>{}}
        }
        if Instant::now()>=connect_until{return Err("灵动岛未连接，请先启动主程序。".into())}
        std::thread::sleep(Duration::from_millis(15));
    };
    let mut request=request.to_string();request.push('\n');file.write_all(request.as_bytes()).map_err(|e|e.to_string())?;
    let deadline=Instant::now()+Duration::from_secs(8);let mut response=vec![];
    loop{
        if Instant::now()>deadline{return Err("设置连接超时。".into())}let mut available=0;
        unsafe{PeekNamedPipe(HANDLE(file.as_raw_handle()),None,0,None,Some(&mut available),None)}.map_err(|e|e.to_string())?;
        if available==0{std::thread::sleep(Duration::from_millis(10));continue}
        let mut buffer=vec![0u8;available.min(65536) as usize];let read=file.read(&mut buffer).map_err(|e|e.to_string())?;
        if read==0{return Err("连接已关闭。".into())}response.extend_from_slice(&buffer[..read]);if response.len()>1024*1024{return Err("设置响应过大。".into())}
        if response.contains(&b'\n'){return serde_json::from_slice(&response).map_err(|e|e.to_string())}
    }
}
static LOG_LOCK:OnceLock<Mutex<()>>=OnceLock::new();
pub fn log(directory:&Path,category:&str,message:&str){
    let Ok(_guard)=LOG_LOCK.get_or_init(||Mutex::new(())).lock()else{return};let _=std::fs::create_dir_all(directory);
    let time=unsafe{windows::Win32::System::SystemInformation::GetLocalTime()};let today=format!("{:04}-{:02}-{:02}",time.wYear,time.wMonth,time.wDay);
    let prefix=if category=="ERROR"{"app_error"}else{"app_debug"};
    let path=(0..10000).map(|i|directory.join(format!("{prefix}-{today}{}.log",if i==0{String::new()}else{format!(".{i}")}))).find(|p|std::fs::metadata(p).map(|m|m.len()<5*1024*1024).unwrap_or(true));
    if let Some(path)=path{if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(path){if file.metadata().map(|m|m.len()==0).unwrap_or(false){let _=file.write_all(&[0xEF,0xBB,0xBF]);}
        let _=writeln!(file,"[{} {:02}:{:02}:{:02}.{:03}] [{}] {}",today,time.wHour,time.wMinute,time.wSecond,time.wMilliseconds,category,message);}}
    // Only this application's dated log files are eligible for retention cleanup.
    if let Ok(files)=std::fs::read_dir(directory){for file in files.flatten(){let name=file.file_name().to_string_lossy().to_string();if (name.starts_with("app_debug-")||name.starts_with("app_error-"))&&name.ends_with(".log")&&file.metadata().ok().and_then(|m|m.modified().ok()).and_then(|t|t.elapsed().ok()).is_some_and(|age|age>Duration::from_secs(7*86400)){let _=std::fs::remove_file(file.path());}}}
}

