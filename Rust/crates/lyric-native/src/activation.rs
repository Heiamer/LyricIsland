//! Activate only the OS-provided source identity, never notification payload text.
use windows::{core::{PCWSTR,PWSTR,BOOL},Win32::{Foundation::*,System::{Threading::*,Com::*},Storage::Packaging::Appx::GetApplicationUserModelId,UI::{WindowsAndMessaging::*,Shell::*}}};
fn valid(id:&str)->bool{!id.trim().is_empty()&&id.len()<=512&&!id.chars().any(|c|c.is_control()||matches!(c,'"'|'<'|'>'|'|'))}
fn executable(id:&str)->Option<String>{
    let id=id.to_lowercase();
    let known=match id.as_str(){"qqnt"|"tencent.qq.nt"|"qq"|"tencent.qq"=>Some("qq.exe"),"netease"=>Some("cloudmusic.exe"),"qqmusic"=>Some("qqmusic.exe"),"spotify"=>Some("spotify.exe"),_=>None};
    known.map(str::to_owned).or_else(||{let file=id.rsplit(['\\','/']).next()?;if file.ends_with(".exe")&&!file.contains([':','!']){Some(file.into())}else{None}})
}
pub fn open(id:&str)->Result<(),String>{
    if !valid(id){return Err("没有可用的来源应用标识".into())}
    struct Search<'a>{id:&'a str,exe:Option<String>,found:Option<HWND>}
    unsafe extern "system" fn find(hwnd:HWND,param:LPARAM)->BOOL{
        let s=&mut *(param.0 as *mut Search<'_>);
        if !IsWindowVisible(hwnd).as_bool()||GetWindowTextLengthW(hwnd)==0{return BOOL(1)}
        let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
        if let Ok(process)=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid){
            let mut buf=[0u16;32768];let mut len=buf.len()as u32;
            let identity=GetApplicationUserModelId(process,&mut len,Some(PWSTR(buf.as_mut_ptr())))==ERROR_SUCCESS&&String::from_utf16_lossy(&buf[..len.saturating_sub(1)as usize]).eq_ignore_ascii_case(s.id);
            let mut len=buf.len()as u32;
            let file=s.exe.as_ref().is_some_and(|exe|QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,PWSTR(buf.as_mut_ptr()),&mut len).is_ok()&&std::path::Path::new(&String::from_utf16_lossy(&buf[..len as usize])).file_name().is_some_and(|p|p.to_string_lossy().eq_ignore_ascii_case(exe)));
            let _=CloseHandle(process);if identity||file{s.found=Some(hwnd);return BOOL(0)}
        }BOOL(1)
    }
    unsafe{
        let mut search=Search{id,exe:executable(id),found:None};let _=EnumWindows(Some(find),LPARAM(&mut search as *mut _ as isize));
        if let Some(hwnd)=search.found{if IsIconic(hwnd).as_bool(){let _=ShowWindow(hwnd,SW_RESTORE);}if SetForegroundWindow(hwnd).as_bool(){return Ok(())}return Err("Windows 暂时无法将来源应用切换到前台，请手动打开后重试".into())}
        // Windows validates registered application identities. No executable path or arguments are passed.
        let wide=id.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        if let Ok(manager)=CoCreateInstance::<_,IApplicationActivationManager>(&ApplicationActivationManager,None,CLSCTX_LOCAL_SERVER){if manager.ActivateApplication(PCWSTR(wide.as_ptr()),PCWSTR::null(),AO_NONE).is_ok(){return Ok(())}}
        // Desktop notification AUMIDs may be registered only in the Shell's AppsFolder.
        let path=format!("shell:AppsFolder\\{id}").encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let _:IShellItem=SHCreateItemFromParsingName(PCWSTR(path.as_ptr()),None).map_err(|_|"未找到来源应用窗口或可启动的应用注册信息".to_owned())?;
        let result=ShellExecuteW(None,windows::core::w!("open"),PCWSTR(path.as_ptr()),PCWSTR::null(),PCWSTR::null(),SW_SHOWNORMAL);
        if result.0 as isize>32{Ok(())}else{Err("Windows 无法打开来源应用".into())}

    }
}
#[cfg(test)]mod tests{use super::*;#[test]fn source_identity_is_not_a_command(){for bad in ["","\n","app\0evil","app\" --run"]{assert!(!valid(bad));}assert_eq!(executable("C:\\Music\\cloudmusic.exe").as_deref(),Some("cloudmusic.exe"));assert_eq!(executable("Tencent.QQ.NT").as_deref(),Some("qq.exe"));assert!(executable("Example_123!App").is_none());}}
