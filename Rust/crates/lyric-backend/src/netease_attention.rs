//! Clear only attention caused by an in-flight background seek. No global preference changes.
use std::sync::{Arc,OnceLock,atomic::{AtomicIsize,AtomicU32,AtomicU64,Ordering},mpsc};
use std::time::{Duration,Instant};
use windows::{core::w,Win32::{Foundation::{HWND,LPARAM,LRESULT,WPARAM},UI::WindowsAndMessaging::*}};
#[derive(Default)]struct State{root:AtomicIsize,pid:AtomicU32,until:AtomicU64,shell:AtomicU32,requests:AtomicU64,cleared:AtomicU64}
static STATE:OnceLock<Arc<State>>=OnceLock::new();
static START:OnceLock<Result<(),String>>=OnceLock::new();
fn now()->u64{static BASE:OnceLock<Instant>=OnceLock::new();BASE.get_or_init(Instant::now).elapsed().as_millis()as u64+1}
fn matches(root:isize,pid:u32,expected:isize,owner:u32,until:u64,at:u64)->bool{root!=0&&root==expected&&pid!=0&&pid==owner&&at<=until}
unsafe extern "system" fn window(hwnd:HWND,message:u32,wp:WPARAM,lp:LPARAM)->LRESULT{
 if let Some(s)=STATE.get(){if message==s.shell.load(Ordering::Acquire)&&wp.0==0x8006{
  let target=HWND(lp.0 as *mut _);let mut owner=0;GetWindowThreadProcessId(target,Some(&mut owner));
  if matches(lp.0,owner,s.root.load(Ordering::Acquire),s.pid.load(Ordering::Acquire),s.until.load(Ordering::Acquire),now()){
   s.requests.fetch_add(1,Ordering::Relaxed);
   // FLASHW_STOP returns the old active state, not a success/failure flag.
   let info=FLASHWINFO{cbSize:std::mem::size_of::<FLASHWINFO>()as u32,hwnd:target,dwFlags:FLASHW_STOP,uCount:0,dwTimeout:0};let _=FlashWindowEx(&info);s.cleared.fetch_add(1,Ordering::Release);
  }
 }}DefWindowProcW(hwnd,message,wp,lp)
}
pub fn arm(root:HWND,pid:u32)->Result<(),String>{
 let s=STATE.get_or_init(||Arc::new(State::default())).clone();
 START.get_or_init(||{
  let(tx,rx)=mpsc::sync_channel(1);let state=s.clone();
  std::thread::Builder::new().name("netease-attention".into()).spawn(move||unsafe{
   let run=(||->Result<(),String>{
    let class=WNDCLASSW{lpfnWndProc:Some(window),lpszClassName:w!("DotIsle.NetEase.SeekAttention.v1"),..Default::default()};if RegisterClassW(&class)==0{return Err("无法初始化后台操作注意力保护".into())}
    let hwnd=CreateWindowExW(WINDOW_EX_STYLE(0),class.lpszClassName,w!(""),WINDOW_STYLE(0),0,0,0,0,None,None,None,None).map_err(|e|e.to_string())?;
    let shell=RegisterWindowMessageW(w!("SHELLHOOK"));if shell==0||!RegisterShellHookWindow(hwnd).as_bool(){let _=DestroyWindow(hwnd);return Err("无法监听后台操作注意力".into())}state.shell.store(shell,Ordering::Release);let _=tx.send(Ok(()));
    let mut message=MSG::default();while GetMessageW(&mut message,None,0,0).as_bool(){let _=TranslateMessage(&message);DispatchMessageW(&message);}let _=DeregisterShellHookWindow(hwnd);let _=DestroyWindow(hwnd);Ok(())
   })();if let Err(e)=run{let _=tx.send(Err(e));}
  }).map_err(|e|e.to_string())?;
  rx.recv_timeout(Duration::from_millis(700)).map_err(|_|"后台注意力保护启动超时".to_string())?
 }).clone()?;
 // Serialization in netease_window ensures one verified player/seek at a time.
 s.until.store(0,Ordering::Release);s.root.store(root.0 as isize,Ordering::Release);s.pid.store(pid,Ordering::Release);s.until.store(now()+600,Ordering::Release);Ok(())
}
pub fn diagnostics()->serde_json::Value{STATE.get().map_or_else(||serde_json::json!({"requests":0,"cleared":0}),|s|serde_json::json!({"requests":s.requests.load(Ordering::Acquire),"cleared":s.cleared.load(Ordering::Acquire)}))}
#[cfg(test)]mod tests{use super::*;
 #[test]fn attention_lease_rejects_other_windows_processes_and_expired_requests(){assert!(matches(42,7,42,7,600,599));for(root,pid,at)in[(43,7,599),(42,8,599),(42,7,601),(0,7,599)]{assert!(!matches(root,pid,42,7,600,at));}}
}
