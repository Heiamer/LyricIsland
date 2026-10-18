//! Native startup registration and Windows media-shell commands.
use std::path::Path;
use windows::{core::{PCWSTR,BOOL},Win32::{Foundation::{HWND,LPARAM},UI::{WindowsAndMessaging::*,Input::KeyboardAndMouse::*},System::Registry::*}};
use serde_json::{Value,json};
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
fn error(e:impl std::fmt::Display)->String{e.to_string()}
const RUN:&str="Software\\Microsoft\\Windows\\CurrentVersion\\Run";
struct Key(HKEY);impl Drop for Key{fn drop(&mut self){unsafe{let _=RegCloseKey(self.0);}}}
unsafe fn open(path:&str,write:bool)->Result<Key,String>{let path=wide(path);let mut key=HKEY::default();
    if write{RegCreateKeyExW(HKEY_CURRENT_USER,PCWSTR(path.as_ptr()),None,None,REG_OPTION_NON_VOLATILE,KEY_READ|KEY_WRITE,None,&mut key,None).ok().map_err(error)?}
    else{RegOpenKeyExW(HKEY_CURRENT_USER,PCWSTR(path.as_ptr()),None,KEY_READ,&mut key).ok().map_err(error)?}Ok(Key(key))
}
unsafe fn read_value(key:&Key)->Option<(REG_VALUE_TYPE,Vec<u8>)>{let name=wide("LyricIsland");let mut kind=REG_VALUE_TYPE::default();let mut length=0;
    RegQueryValueExW(key.0,PCWSTR(name.as_ptr()),None,Some(&mut kind),None,Some(&mut length)).ok().ok()?;
    if length>65536{return None}let mut bytes=vec![0u8;length as usize];RegQueryValueExW(key.0,PCWSTR(name.as_ptr()),None,Some(&mut kind),Some(bytes.as_mut_ptr()),Some(&mut length)).ok().ok()?;Some((kind,bytes))
}
/// Accepts both the extracted runtime executable and the single-file launcher
/// (`DotIsle-<version>.exe`), which is the path login startup registers.
fn launcher_name(name:&str)->bool{
    name.eq_ignore_ascii_case("LyricIsland.exe")||(name.len()>12&&name.get(..8).is_some_and(|p|p.eq_ignore_ascii_case("DotIsle-"))&&name.to_ascii_lowercase().ends_with(".exe"))
}
pub fn parse_command(command:&str)->Option<String>{
    if !command.starts_with('"'){return None}let end=command[1..].find('"')?+1;if command[end+1..].trim()!="--startup"{return None}
    let path=&command[1..end];if !Path::new(path).is_absolute()||!launcher_name(&Path::new(path).file_name()?.to_string_lossy()){return None}Some(path.into())
}
pub fn startup_info(path:Option<&str>)->Value{
    let registered=unsafe{open(path.unwrap_or(RUN),false).ok().and_then(|k|read_value(&k)).and_then(|(kind,bytes)|{
        if kind!=REG_SZ{return None}let wide=bytes.chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).collect::<Vec<_>>();let text=String::from_utf16_lossy(&wide);parse_command(text.trim_end_matches('\0'))
    })};let enabled=registered.as_ref().is_some_and(|p|Path::new(p).is_file());
    json!({"registered":registered.is_some(),"enabled":enabled,"executable":registered,"status":if enabled{"已注册登录启动；没有音乐播放时驻留系统托盘。"}else if registered.is_some(){"启动路径已失效，请重新开启以修复。"}else{"尚未启用登录启动。"}})
}
pub fn set_startup(enabled:bool,exe:&str,settings_path:&Path,settings:&Value,key_path:Option<&str>)->Result<(),String>{unsafe{
    if enabled&&(!Path::new(exe).is_file()||parse_command(&format!("\"{exe}\" --startup")).is_none()||exe.contains('"')){return Err("启动程序路径无效。".into())}
    let command=format!("\"{exe}\" --startup");if enabled&&command.encode_utf16().count()>260{return Err("程序路径太长，无法注册登录启动。".into())}
    let key=open(key_path.unwrap_or(RUN),true)?;let previous=read_value(&key);let name=wide("LyricIsland");
    let result=(||{if enabled{let bytes=wide(&command).iter().flat_map(|c|c.to_le_bytes()).collect::<Vec<_>>();RegSetValueExW(key.0,PCWSTR(name.as_ptr()),None,REG_SZ,Some(&bytes)).ok().map_err(error)?}else{let result=RegDeleteValueW(key.0,PCWSTR(name.as_ptr()));if result.0!=0&&result.0!=2{return Err(format!("Registry error {}",result.0))}}
        crate::io::save_settings(settings_path,settings)
    })();if result.is_err(){if let Some((kind,bytes))=previous{let _=RegSetValueExW(key.0,PCWSTR(name.as_ptr()),None,kind,Some(&bytes));}else{let _=RegDeleteValueW(key.0,PCWSTR(name.as_ptr()));}}result
}}
/// Declare per-monitor DPI awareness for this process.
///
/// The tray menu is positioned with coordinates taken from the parent (a per-monitor aware WPF
/// process), i.e. physical pixels. A DPI-unaware process has those reinterpreted as logical
/// pixels, so on a 150% display the menu would open at 1.5x the intended position — far away
/// from the cursor, and off-screen when the tray sits at the right edge. Must be called before
/// any window or menu is created.
pub fn ensure_per_monitor_dpi_awareness(){
    use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext,DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    unsafe{let _=SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);}
}
pub fn tray_menu(x:i32,y:i32)->Result<(),String>{unsafe{
    let class=wide("STATIC");let title=wide("DotIsle");let owner=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_LAYERED,PCWSTR(class.as_ptr()),PCWSTR(title.as_ptr()),WS_POPUP,x,y,1,1,None,None,None,None).map_err(error)?;
    let _=SetLayeredWindowAttributes(owner,windows::Win32::Foundation::COLORREF(0),0,LWA_ALPHA);let _=ShowWindow(owner,SW_SHOWNOACTIVATE);let _=SetForegroundWindow(owner);
    let menu=CreatePopupMenu().map_err(error)?;
    for (id,label) in [(1,"展开灵动岛"),(2,"显示 / 隐藏灵动岛"),(3,"设置"),(4,"退出")]{let label=wide(label);AppendMenuW(menu,MF_STRING,id,PCWSTR(label.as_ptr())).map_err(error)?}
    let chosen=TrackPopupMenuEx(menu,(TPM_RETURNCMD|TPM_RIGHTBUTTON).0,x,y,owner,None).0;
    let _=DestroyMenu(menu);let _=DestroyWindow(owner);
    let command=match chosen{1=>"expand",2=>"toggleVisibility",3=>"settings",4=>"exit",_=>return Ok(())};
    crate::io::pipe_request("LyricIsland.Settings.v3",&json!({"version":1,"command":command}))?;Ok(())
}}
pub fn favorite(title:&str,source:&str,invoke:bool)->Value{
    if source!="Netease"{return json!({"success":false,"message":"当前播放器暂不支持喜欢操作"})}
    struct Search{title:String,found:Option<HWND>}
    unsafe extern "system" fn find(hwnd:HWND,param:LPARAM)->BOOL{
        let search=&mut *(param.0 as *mut Search);if !IsWindowVisible(hwnd).as_bool(){return BOOL(1)}
        let mut name=[0u16;1024];let length=GetWindowTextW(hwnd,&mut name);let name=String::from_utf16_lossy(&name[..length.max(0) as usize]);
        if !name.to_lowercase().contains(&search.title.to_lowercase()){return BOOL(1)}
        let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
        use windows::Win32::System::{Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION,QueryFullProcessImageNameW,PROCESS_NAME_WIN32}};
        if let Ok(process)=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid){let mut path=[0u16;32768];let mut size=path.len() as u32;let ok=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,windows::core::PWSTR(path.as_mut_ptr()),&mut size).is_ok();let _=windows::Win32::Foundation::CloseHandle(process);
            if ok&&Path::new(&String::from_utf16_lossy(&path[..size as usize])).file_name().is_some_and(|p|p.to_string_lossy().eq_ignore_ascii_case("cloudmusic.exe")){search.found=Some(hwnd);return BOOL(0)}
        }
        BOOL(1)
    }
    let mut search=Search{title:title.into(),found:None};unsafe{let _=EnumWindows(Some(find),LPARAM(&mut search as *mut _ as isize));}
    let Some(window)=search.found else{return json!({"success":false,"message":"没有找到网易云当前歌曲窗口，请打开网易云后重试"})};
    if !invoke{return json!({"success":true,"message":"网易云当前歌曲可通过原生 Ctrl+L 喜欢命令调用"})}
    unsafe{
        if [0x10,0x11,0x12].iter().any(|k|GetAsyncKeyState(*k)&i16::MIN!=0){return json!({"success":false,"message":"请松开 Ctrl、Alt、Shift 后重试喜欢操作"})}
        let previous=GetForegroundWindow();let minimized=IsIconic(window).as_bool();if minimized{let _=ShowWindow(window,SW_RESTORE);}
        if !SetForegroundWindow(window).as_bool(){if minimized{let _=ShowWindow(window,SW_MINIMIZE);}return json!({"success":false,"message":"无法连接网易云窗口，请打开后重试"})}
        std::thread::sleep(std::time::Duration::from_millis(90));let mut text=[0u16;1024];let count=GetWindowTextW(window,&mut text);
        let success=GetForegroundWindow()==window&&String::from_utf16_lossy(&text[..count.max(0) as usize]).to_lowercase().contains(&title.to_lowercase());
        let key=|code,up|INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{ki:KEYBDINPUT{wVk:VIRTUAL_KEY(code),dwFlags:if up{KEYEVENTF_KEYUP}else{KEYBD_EVENT_FLAGS(0)},..Default::default()}}};
        let sent=if success{SendInput(&[key(0x11,false),key(0x4c,false),key(0x4c,true),key(0x11,true)],std::mem::size_of::<INPUT>() as i32)}else{0};
        if sent>0&&sent<4{SendInput(&[key(0x4c,true),key(0x11,true)],std::mem::size_of::<INPUT>() as i32);}
        if sent==4{std::thread::sleep(std::time::Duration::from_millis(140));}if minimized{let _=ShowWindow(window,SW_MINIMIZE);}if previous.0!=std::ptr::null_mut()&&previous!=window{let _=SetForegroundWindow(previous);}
        json!({"success":sent==4,"message":if sent==4{"已向网易云提交喜欢 / 取消喜欢操作"}else{"歌曲或窗口已切换，未执行喜欢操作"}})
    }
}
#[cfg(test)]mod tests{
use super::*;
#[test]fn only_scoped_startup_commands_are_accepted(){assert!(parse_command("\"C:\\Apps\\LyricIsland.exe\" --startup").is_some());assert!(parse_command("\"C:\\Apps\\DotIsle-2.0.0.exe\" --startup").is_some());assert!(parse_command("\"C:\\Apps\\DotIsle-2.0.0.exe\" --startup").as_deref()==Some("C:\\Apps\\DotIsle-2.0.0.exe"));assert!(parse_command("\"C:\\Apps\\Other.exe\" --startup").is_none());assert!(parse_command("\"C:\\Apps\\DotIsle-.exe\" --startup").is_none());assert!(parse_command("\"C:\\Apps\\LyricIsland.exe\" --startup --extra").is_none());}
/// Removes the scratch key and directory even when an assertion fails.
struct Scratch{key:String,dir:std::path::PathBuf}
impl Drop for Scratch{fn drop(&mut self){unsafe{let path=wide(&self.key);let _=RegDeleteKeyW(HKEY_CURRENT_USER,PCWSTR(path.as_ptr()));}let _=std::fs::remove_dir_all(&self.dir);}}
fn seed(key_path:&str,command:&str){unsafe{let key=open(key_path,true).unwrap();let name=wide("LyricIsland");let bytes=wide(command).iter().flat_map(|c|c.to_le_bytes()).collect::<Vec<_>>();RegSetValueExW(key.0,PCWSTR(name.as_ptr()),None,REG_SZ,Some(&bytes)).ok().unwrap();}}
/// Exercises the real registry transaction against a scratch key, so the user's Run entry is untouched.
/// The tray menu is placed in physical pixels; if this process stayed DPI-unaware the coordinates
/// would be reinterpreted and the menu would open far from the cursor.
#[test]fn tray_menu_process_becomes_per_monitor_dpi_aware(){
    use windows::Win32::UI::HiDpi::{GetAwarenessFromDpiAwarenessContext,GetThreadDpiAwarenessContext,DPI_AWARENESS_PER_MONITOR_AWARE};
    ensure_per_monitor_dpi_awareness();
    unsafe{let context=GetThreadDpiAwarenessContext();assert_eq!(GetAwarenessFromDpiAwarenessContext(context),DPI_AWARENESS_PER_MONITOR_AWARE,"the tray menu would be placed with logical-pixel coordinates");}
}
#[test]fn startup_registration_commits_and_rolls_back(){
    let dir=std::env::temp_dir().join(format!("li-startup-{}",std::process::id()));let _=std::fs::remove_dir_all(&dir);std::fs::create_dir_all(&dir).unwrap();
    let scratch=Scratch{key:format!("Software\\LyricIsland-Test-{}",std::process::id()),dir:dir.clone()};
    let exe=dir.join("LyricIsland.exe");std::fs::write(&exe,b"").unwrap();let exe=exe.to_str().unwrap().to_string();
    let settings_path=dir.join("settings.json");let settings=json!({"AutoStart":true});

    // Enabling writes the Run command and persists settings as one transaction.
    set_startup(true,&exe,&settings_path,&settings,Some(&scratch.key)).unwrap();
    let info=startup_info(Some(&scratch.key));
    assert_eq!(info["registered"],json!(true),"{info}");assert_eq!(info["enabled"],json!(true),"{info}");
    assert_eq!(info["executable"],json!(exe.clone()),"{info}");
    assert!(settings_path.is_file(),"settings were not persisted");

    // A failing settings write must restore the previous registry value, not leave the new one.
    seed(&scratch.key,"\"C:\\Old\\LyricIsland.exe\" --startup");
    let blocked=dir.join("blocked");std::fs::write(&blocked,b"not a directory").unwrap();
    let failed=set_startup(true,&exe,&blocked.join("settings.json"),&settings,Some(&scratch.key));
    assert!(failed.is_err(),"a failing settings write has to surface as an error");
    assert_eq!(startup_info(Some(&scratch.key))["executable"],json!("C:\\Old\\LyricIsland.exe"),"rollback did not restore the previous value");

    // Disabling removes the value again.
    set_startup(false,&exe,&settings_path,&settings,Some(&scratch.key)).unwrap();
    assert_eq!(startup_info(Some(&scratch.key))["registered"],json!(false),"disable left the value behind");

    // An executable that is not a LyricIsland.exe file is refused before any write happens.
    assert!(set_startup(true,"C:\\definitely\\missing\\LyricIsland.exe",&settings_path,&settings,Some(&scratch.key)).is_err());
    assert_eq!(startup_info(Some(&scratch.key))["registered"],json!(false),"a refused enable still wrote the value");
}}

