//! Apply the saved window level to the visual owner and its input window together.
use serde_json::{json,Value};
use windows::{core::Result,Win32::{Foundation::HWND,UI::WindowsAndMessaging::*}};

pub fn creation_style(topmost:bool)->WINDOW_EX_STYLE{
    if topmost{WS_EX_TOPMOST}else{WINDOW_EX_STYLE(0)}
}

pub unsafe fn apply(visual:HWND,input:HWND,topmost:bool,show:bool)->Result<()>{
    // An in-memory App has no windows yet; its first real create applies the setting.
    if visual.0.is_null()||input.0.is_null(){return Ok(())}
    let after=if topmost{HWND_TOPMOST}else{HWND_NOTOPMOST};
    let mut flags=SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE;
    if show{flags|=SWP_SHOWWINDOW;}
    // The visual is the owner. Put it in the requested band first, then keep the
    // input window above it. Showing and ordering happen in the same operation,
    // independent of the launcher's first-ShowWindow STARTUPINFO override.
    SetWindowPos(visual,Some(after),0,0,0,0,flags)?;
    SetWindowPos(input,Some(after),0,0,0,0,flags)?;
    Ok(())
}

pub unsafe fn snapshot(visual:HWND,input:HWND)->Value{
    let topmost=|hwnd:HWND|!hwnd.0.is_null()&&(GetWindowLongPtrW(hwnd,GWL_EXSTYLE)as u32&WS_EX_TOPMOST.0)!=0;
    json!({"visualTopmost":topmost(visual),"inputTopmost":topmost(input)})
}

#[cfg(test)]
mod tests{
    use super::{apply,creation_style,snapshot};
    use windows::{core::w,Win32::{Foundation::{HINSTANCE,HWND},UI::WindowsAndMessaging::*}};
    struct Pair{visual:HWND,input:HWND}
    impl Pair{
        unsafe fn new(topmost:bool)->Self{
            let style=WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|creation_style(topmost);
            let visual=CreateWindowExW(style,w!("STATIC"),w!("DotIsle level test"),WS_POPUP,0,0,1,1,None,None,Some(HINSTANCE::default()),None).unwrap();
            let input=CreateWindowExW(style,w!("STATIC"),w!("DotIsle input test"),WS_POPUP,0,0,1,1,Some(visual),None,Some(HINSTANCE::default()),None).unwrap();
            Self{visual,input}
        }
        unsafe fn check(&self,topmost:bool){
            let state=snapshot(self.visual,self.input);
            assert_eq!(state["visualTopmost"],topmost);
            assert_eq!(state["inputTopmost"],topmost);
        }
    }
    impl Drop for Pair{fn drop(&mut self){unsafe{let _=DestroyWindow(self.input);let _=DestroyWindow(self.visual);}}}

    #[test]fn saved_level_applies_before_and_after_the_first_show(){unsafe{
        for topmost in [true,false]{
            let pair=Pair::new(topmost);pair.check(topmost);
            let foreground=GetForegroundWindow();
            apply(pair.visual,pair.input,topmost,true).unwrap();pair.check(topmost);
            assert!(IsWindowVisible(pair.visual).as_bool()&&IsWindowVisible(pair.input).as_bool());
            assert_eq!(GetForegroundWindow(),foreground,"initial display must not steal focus");
            assert_eq!(GetWindow(pair.input,GW_OWNER).unwrap(),pair.visual);
        }
    }}
    #[test]fn startup_reapply_restores_both_windows_after_a_layer_change(){unsafe{
        let pair=Pair::new(true);apply(pair.visual,pair.input,true,true).unwrap();
        SetWindowPos(pair.visual,Some(HWND_NOTOPMOST),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE).unwrap();
        pair.check(false);
        apply(pair.visual,pair.input,true,false).unwrap();pair.check(true);
        apply(pair.visual,pair.input,false,false).unwrap();pair.check(false);
    }}
}
