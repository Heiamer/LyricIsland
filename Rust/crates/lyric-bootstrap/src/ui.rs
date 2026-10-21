//! Native installer and update surfaces. They use Win32 directly so the bootstrap
//! binaries need no .NET runtime or WinForms assemblies.

use crate::error::{Error, Result};
use crate::installer;
use crate::payload;
use crate::updater::OverlayState;
use crate::win;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, CreatePen, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect,
    GetStockObject, InvalidateRect, LineTo, MoveToEx, SetBkMode, SetTextColor, UpdateWindow, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER, DT_LEFT,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HBRUSH, HDC, HFONT, HGDIOBJ, NULL_BRUSH, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{BST_CHECKED, DRAWITEMSTRUCT, ODS_DISABLED};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::Shell::{SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowTextW, GetWindowTextLengthW, IsDialogMessageW, LoadCursorW, PostMessageW, PostQuitMessage,
    RegisterClassW, SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowTextW, ShowWindow, SystemParametersInfoW,
    TranslateMessage, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_OWNERDRAW, ES_AUTOHSCROLL, GWLP_USERDATA, HMENU,
    IDC_ARROW, MSG, SPI_GETWORKAREA, SW_SHOW, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_KEYDOWN, WM_NCCREATE,
    WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE, WS_EX_CONTROLPARENT,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, CREATESTRUCTW,
};

pub fn dpi_aware() {
    unsafe { let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2); }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF { COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16) }

fn scale(value: i32, dpi: u32) -> i32 { (value as i64 * dpi.max(96) as i64 / 96) as i32 }

unsafe fn wide(value: &str) -> Vec<u16> { win::wide(value) }

unsafe fn set_text(hwnd: HWND, text: &str) {
    let text = wide(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(text.as_ptr()));
}

unsafe fn text_of(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let read = GetWindowTextW(hwnd, &mut buffer);
    String::from_utf16_lossy(&buffer[..read.max(0) as usize])
}

fn font(face: &str, points: i32, bold: bool, dpi: u32) -> HFONT {
    let face = win::wide(face);
    unsafe {
        CreateFontW(
            -((points * dpi as i32 + 36) / 72),
            0,
            0,
            0,
            if bold { 700 } else { 400 },
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY,
            0,
            PCWSTR(face.as_ptr()),
        )
    }
}

struct WizardState {
    uninstall: bool,
    target: PathBuf,
    data: PathBuf,
    dpi: u32,
    bg: HBRUSH,
    body: HFONT,
    title: HFONT,
    status: HWND,
    detail: HWND,
    edit: HWND,
    browse: HWND,
    primary: HWND,
    keep: HWND,
    result: Option<bool>,
    installing: bool,
    migration: Option<PathBuf>,
    error: Arc<Mutex<Option<String>>>,
}

const ID_BROWSE: u32 = 101;
const ID_PRIMARY: u32 = 102;
const ID_KEEP: u32 = 200;
const ID_CANCEL: u32 = 201;
const ID_DETAIL: u32 = 202;
const WM_FINISHED: u32 = WM_APP + 1;

unsafe fn state(hwnd: HWND) -> &'static mut WizardState {
    &mut *(windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WizardState)
}

unsafe fn create_control(
    parent: HWND,
    class: &str,
    text: &str,
    style: WINDOW_STYLE,
    exstyle: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: u32,
    handle_font: HFONT,
    dpi: u32,
) -> HWND {
    let class = wide(class);
    let text = wide(text);
    let instance = windows::Win32::Foundation::HINSTANCE(GetModuleHandleW(None).unwrap().0);
    let hwnd = CreateWindowExW(
        exstyle,
        PCWSTR(class.as_ptr()),
        PCWSTR(text.as_ptr()),
        style,
        scale(x, dpi),
        scale(y, dpi),
        scale(width, dpi),
        scale(height, dpi),
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        Some(instance),
        None,
    )
    .unwrap_or_default();
    SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(handle_font.0 as usize)), Some(LPARAM(1)));
    hwnd
}

fn register_wizard_class() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let background = CreateSolidBrush(rgb(247, 249, 253));
        let class_name = wide("DotIsleSetupWizard");
        let instance = windows::Win32::Foundation::HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class = WNDCLASSW {
            style: Default::default(),
            lpfnWndProc: Some(wizard_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: Default::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: background,
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
        };
        let _ = RegisterClassW(&class);
    });
}

unsafe fn layout_install(hwnd: HWND) {
    let state = state(hwnd);
    let dpi = state.dpi;
    let title = create_control(hwnd, "STATIC", "让重要的事，一眼可见。", WS_CHILD | WS_VISIBLE, Default::default(), 30, 30, 550, 45, 0, state.title, dpi);
    let subtitle = format!("安装点屿  {}\r\n选择安装位置；你的设置和待办会保留。", payload::VERSION);
    create_control(hwnd, "STATIC", &subtitle, WS_CHILD | WS_VISIBLE, Default::default(), 32, 88, 550, 54, 0, state.body, dpi);
    let folder = installer::default_install_folder();
    let edit = create_control(hwnd, "EDIT", &folder.to_string_lossy(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32), WS_EX_CLIENTEDGE, 32, 167, 450, 30, 0, state.body, dpi);
    let browse = create_control(hwnd, "BUTTON", "浏览…", WS_CHILD | WS_VISIBLE | WS_TABSTOP, Default::default(), 495, 165, 90, 34, ID_BROWSE, state.body, dpi);
    let primary = create_control(hwnd, "BUTTON", "安装并启动", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32), Default::default(), 430, 300, 155, 42, ID_PRIMARY, state.body, dpi);
    let status = create_control(hwnd, "STATIC", "安装到当前用户，无需管理员权限。\r\n旧单文件版数据不会被删除；已有安装数据优先，迁移冲突保留备份。", WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0), Default::default(), 32, 216, 550, 68, 0, state.body, dpi);
    state.edit = edit;
    state.browse = browse;
    state.primary = primary;
    state.status = status;
    let _ = title;
    let _ = subtitle;
    let _ = SetFocus(Some(edit));
}

unsafe fn layout_uninstall(hwnd: HWND) {
    let state = state(hwnd);
    let dpi = state.dpi;
    create_control(hwnd, "STATIC", "准备卸载点屿", WS_CHILD | WS_VISIBLE, Default::default(), 28, 25, 510, 36, 0, state.title, dpi);
    let info = format!("将关闭点屿并移除程序、快捷方式及 Windows 应用登记。\r\n程序位置：{}", state.target.display());
    create_control(hwnd, "STATIC", &info, WS_CHILD | WS_VISIBLE, Default::default(), 30, 78, 510, 60, 0, state.body, dpi);
    let keep = create_control(hwnd, "BUTTON", "保留设置、待办、流程与历史记录", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32), Default::default(), 30, 150, 510, 32, ID_KEEP, state.body, dpi);
    SendMessageW(keep, BM_SETCHECK, Some(WPARAM(BST_CHECKED.0 as usize)), None);
    let detail = create_control(hwnd, "STATIC", "", WS_CHILD | WS_VISIBLE, Default::default(), 30, 190, 510, 66, ID_DETAIL, state.body, dpi);
    create_control(hwnd, "BUTTON", "取消", WS_CHILD | WS_VISIBLE | WS_TABSTOP, Default::default(), 320, 270, 95, 36, ID_CANCEL, state.body, dpi);
    let primary = create_control(hwnd, "BUTTON", "确认卸载", WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32), Default::default(), 427, 270, 112, 36, ID_PRIMARY, state.body, dpi);
    state.keep = keep;
    state.detail = detail;
    state.primary = primary;
    refresh_uninstall_detail(hwnd);
    let _ = SetFocus(Some(primary));
}

unsafe fn refresh_uninstall_detail(hwnd: HWND) {
    let state = state(hwnd);
    let keep = SendMessageW(state.keep, BM_GETCHECK, None, None).0 != 0;
    let text = if keep { format!("数据将保留：\r\n{}", state.data.display()) } else { format!("将永久删除数据、缓存和日志：\r\n{}", state.data.display()) };
    set_text(state.detail, &text);
}

fn browse_folder(owner: HWND) -> Option<PathBuf> {
    unsafe {
        let title = wide("选择点屿安装文件夹（需为空或已有点屿安装目录）");
        let mut display = [0u16; 260];
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pszDisplayName: PWSTR(display.as_mut_ptr()),
            lpszTitle: PCWSTR(title.as_ptr()),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            ..Default::default()
        };
        let item = SHBrowseForFolderW(&info);
        if item.is_null() { return None; }
        let mut path = [0u16; 260];
        let ok = SHGetPathFromIDListW(item, &mut path).as_bool();
        CoTaskMemFree(Some(item as *const core::ffi::c_void));
        if !ok { return None; }
        let length = path.iter().position(|unit| *unit == 0).unwrap_or(path.len());
        Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
    }
}

unsafe fn begin_install(hwnd: HWND) {
    let state = state(hwnd);
    if state.installing { return; }
    let folder = text_of(state.edit);
    if folder.trim().is_empty() {
        set_text(state.status, "请选择安装位置。");
        return;
    }
    state.installing = true;
    set_text(state.status, "正在校验组件、保存数据并安装…");
    let _ = EnableWindow(state.edit, false);
    let _ = EnableWindow(state.browse, false);
    let _ = EnableWindow(state.primary, false);
    set_text(state.primary, "正在安装…");
    let errors = state.error.clone();
    let migration = state.migration.clone();
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let outcome = installer::install(folder.trim(), true, true, migration.as_deref());
        if let Ok(mut slot) = errors.lock() { *slot = outcome.err().map(|error| error.to_string()); }
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        unsafe { let _ = PostMessageW(Some(hwnd), WM_FINISHED, WPARAM(outcome_is_ok(&errors)), LPARAM(0)); }
    });
}

fn outcome_is_ok(errors: &Arc<Mutex<Option<String>>>) -> usize {
    match errors.lock() { Ok(slot) => if slot.is_none() { 1 } else { 0 }, Err(_) => 0 }
}

unsafe fn finish_install(hwnd: HWND) {
    let state = state(hwnd);
    state.installing = false;
    let message = state.error.lock().ok().and_then(|mut slot| slot.take());
    match message {
        None => { let _ = DestroyWindow(hwnd); }
        Some(error) => {
            set_text(state.status, &error);
            set_text(state.primary, "安装并启动");
            let _ = EnableWindow(state.edit, true);
            let _ = EnableWindow(state.browse, true);
            let _ = EnableWindow(state.primary, true);
        }
    }
}

unsafe extern "system" fn wizard_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            LRESULT(1)
        }
        WM_CREATE => {
            let state = state(hwnd);
            state.bg = CreateSolidBrush(rgb(247, 249, 253));
            state.body = font("Microsoft YaHei UI", 10, false, state.dpi);
            state.title = if state.uninstall { font("Microsoft YaHei UI", 18, true, state.dpi) } else { font("Microsoft YaHei UI", 19, true, state.dpi) };
            if state.uninstall { layout_uninstall(hwnd); } else { layout_install(hwnd); }
            LRESULT(0)
        }
        WM_COMMAND => {
            let state = state(hwnd);
            let id = (wparam.0 & 0xffff) as u32;
            match id {
                ID_BROWSE => { if let Some(folder) = browse_folder(hwnd) { set_text(state.edit, &folder.to_string_lossy()); } }
                ID_PRIMARY => {
                    if state.uninstall {
                        state.result = Some(SendMessageW(state.keep, BM_GETCHECK, None, None).0 != 0);
                        let _ = DestroyWindow(hwnd);
                    } else { begin_install(hwnd); }
                }
                ID_KEEP => refresh_uninstall_detail(hwnd),
                ID_CANCEL => { if !state.installing { state.result = None; let _ = DestroyWindow(hwnd); } }
                _ => {}
            }
            LRESULT(0)
        }
        WM_FINISHED => { finish_install(hwnd); LRESULT(0) }
        WM_CLOSE => {
            let state = state(hwnd);
            if state.installing { return LRESULT(0); }
            state.result = None;
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let state = state(hwnd);
            let hdc = HDC(lparam.0 as *mut core::ffi::c_void);
            let control = HWND(wparam.0 as *mut core::ffi::c_void);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, if control == state.status || control == state.detail { rgb(90, 101, 117) } else { rgb(15, 23, 42) });
            LRESULT(state.bg.0 as isize)
        }
        WM_DRAWITEM => {
            let state = state(hwnd);
            let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
            if item.CtlID == ID_PRIMARY {
                let disabled = item.itemState.0 & ODS_DISABLED.0 != 0;
                let brush = CreateSolidBrush(if disabled { rgb(186, 194, 190) } else { rgb(37, 99, 235) });
                FillRect(item.hDC, &item.rcItem, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));
                SetBkMode(item.hDC, TRANSPARENT);
                SetTextColor(item.hDC, rgb(255, 255, 255));
                let old = windows::Win32::Graphics::Gdi::SelectObject(item.hDC, HGDIOBJ(state.body.0));
                let mut text: Vec<u16> = text_of(item.hwndItem).encode_utf16().collect();
                let mut rect = item.rcItem;
                DrawTextW(item.hDC, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
                windows::Win32::Graphics::Gdi::SelectObject(item.hDC, old);
            }
            LRESULT(1)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn run_wizard(uninstall: bool, target: &Path, data: &Path, migration: Option<PathBuf>) -> Result<Option<bool>> {
    dpi_aware();
    register_wizard_class();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dpi = GetDpiForSystem().max(96);
        let (client_width, client_height) = if uninstall { (570, 330) } else { (620, 370) };
        let style = WS_CAPTION | WS_SYSMENU;
        let exstyle = WS_EX_CONTROLPARENT;
        let mut rect = RECT { left: 0, top: 0, right: scale(client_width, dpi), bottom: scale(client_height, dpi) };
        let _ = AdjustWindowRectEx(&mut rect, style, false, exstyle);
        let outer_width = rect.right - rect.left;
        let outer_height = rect.bottom - rect.top;
        let mut area = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut area as *mut _ as *mut core::ffi::c_void), Default::default());
        let x = area.left + ((area.right - area.left) - outer_width) / 2;
        let y = area.top + ((area.bottom - area.top) - outer_height) / 2;
        let mut state = Box::new(WizardState {
            uninstall,
            target: target.to_path_buf(),
            data: data.to_path_buf(),
            dpi,
            bg: HBRUSH::default(),
            body: HFONT::default(),
            title: HFONT::default(),
            status: HWND::default(),
            detail: HWND::default(),
            edit: HWND::default(),
            browse: HWND::default(),
            primary: HWND::default(),
            keep: HWND::default(),
            result: None,
            installing: false,
            migration,
            error: Arc::new(Mutex::new(None)),
        });
        let pointer = &mut *state as *mut WizardState;
        let class_name = win::wide("DotIsleSetupWizard");
        let title = if uninstall { "卸载点屿 · DotIsle".to_owned() } else { format!("安装点屿 · DotIsle {}", payload::VERSION) };
        let title = win::wide(&title);
        let instance = windows::Win32::Foundation::HINSTANCE(GetModuleHandleW(None)?.0);
        let hwnd = CreateWindowExW(
            exstyle,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            style,
            x,
            y,
            outer_width,
            outer_height,
            None,
            None,
            Some(instance),
            Some(pointer as *const core::ffi::c_void),
        )?;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_KEYDOWN {
                if message.wParam.0 == VK_ESCAPE.0 as usize {
                    let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(ID_CANCEL as usize), LPARAM(0));
                    continue;
                }
                if message.wParam.0 == VK_RETURN.0 as usize {
                    let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(ID_PRIMARY as usize), LPARAM(0));
                    continue;
                }
            }
            if IsDialogMessageW(hwnd, &message).as_bool() { continue; }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let outcome = state.result;
        if !state.bg.is_invalid() { let _ = DeleteObject(HGDIOBJ(state.bg.0)); }
        if !state.body.is_invalid() { let _ = DeleteObject(HGDIOBJ(state.body.0)); }
        if !state.title.is_invalid() { let _ = DeleteObject(HGDIOBJ(state.title.0)); }
        CoUninitialize();
        drop(Box::from_raw(pointer));
        Ok(outcome)
    }
}

pub fn install_wizard(migration: Option<PathBuf>) -> Result<()> {
    run_wizard(false, Path::new(""), Path::new(""), migration).map(|_| ())
}

pub fn confirm_uninstall(target: &Path, data: &Path) -> Option<bool> {
    run_wizard(true, target, data, None).ok().flatten()
}

// ---- Update island -------------------------------------------------------------------------

static OVERLAY_STATE: Mutex<Option<Arc<Mutex<OverlayState>>>> = Mutex::new(None);

fn register_overlay_class() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let class_name = wide("DotIsleUpdateOverlay");
        let instance = windows::Win32::Foundation::HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class = WNDCLASSW {
            style: Default::default(),
            lpfnWndProc: Some(overlay_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: Default::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
        };
        let _ = RegisterClassW(&class);
    });
}

fn progress_points(cx: i32, cy: i32, radius: i32, start: f64, sweep: f64) -> Vec<(i32, i32)> {
    let steps = ((sweep.abs() / 4.0).ceil() as i32).max(2);
    (0..=steps)
        .map(|index| {
            let angle = (start + sweep * index as f64 / steps as f64).to_radians();
            (cx + (radius as f64 * angle.cos()) as i32, cy + (radius as f64 * angle.sin()) as i32)
        })
        .collect()
}

unsafe fn draw_overlay(hwnd: HWND) {
    let snapshot = OVERLAY_STATE.lock().ok().and_then(|slot| slot.as_ref().map(|state| state.lock().ok().map(|state| state.clone()))).flatten().unwrap_or_default();
    let mut paint = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut paint);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let background = CreateSolidBrush(rgb(21, 26, 30));
    FillRect(hdc, &client, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
    let ring_left = scale(24, dpi);
    let ring_top = scale(35, dpi);
    let ring_size = scale(54, dpi);
    let width = scale(4, dpi).max(1);
    let track = CreatePen(PS_SOLID, width, rgb(42, 56, 81));
    let old_pen = windows::Win32::Graphics::Gdi::SelectObject(hdc, HGDIOBJ(track.0));
    let old_brush = windows::Win32::Graphics::Gdi::SelectObject(hdc, GetStockObject(NULL_BRUSH));
    let _ = Ellipse(hdc, ring_left, ring_top, ring_left + ring_size, ring_top + ring_size);
    windows::Win32::Graphics::Gdi::SelectObject(hdc, old_brush);
    windows::Win32::Graphics::Gdi::SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(track.0));
    let accent = CreatePen(PS_SOLID, width, rgb(110, 168, 255));
    let old_pen = windows::Win32::Graphics::Gdi::SelectObject(hdc, HGDIOBJ(accent.0));
    let centre = ring_left + ring_size / 2;
    let radius = ring_size / 2 - width / 2;
    let fraction = snapshot.length.filter(|length| *length > 0).map(|length| (snapshot.received as f64 / length as f64).clamp(0., 1.));
    let (start, sweep) = if snapshot.phase == "downloading" {
        match fraction { Some(value) => (-90.0, (value * 359.9).max(0.1)), None => { let tick = windows::Win32::System::SystemInformation::GetTickCount64() % 1200; (-90.0 + (tick as f64 / 1200.0) * 360.0, 80.0) } }
    } else { (-90.0 + ((windows::Win32::System::SystemInformation::GetTickCount64() % 1200) as f64 / 1200.0) * 360.0, 80.0) };
    for (index, (x, y)) in progress_points(centre, centre, radius, start, sweep).into_iter().enumerate() {
        if index == 0 { let _ = MoveToEx(hdc, x, y, None); } else { let _ = LineTo(hdc, x, y); }
    }
    windows::Win32::Graphics::Gdi::SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(accent.0));

    SetBkMode(hdc, TRANSPARENT);
    let title = match snapshot.phase.as_str() { "completed" => "更新已完成", "restarting" => "正在启动新版", _ => "正在安装更新" };
    let title_font = font("Microsoft YaHei UI", 13, true, dpi);
    let body_font = font("Microsoft YaHei UI", 9, false, dpi);
    let old_font = windows::Win32::Graphics::Gdi::SelectObject(hdc, HGDIOBJ(title_font.0));
    SetTextColor(hdc, rgb(255, 255, 255));
    let mut title: Vec<u16> = title.encode_utf16().collect();
    let mut rect = RECT { left: scale(104, dpi), top: scale(33, dpi), right: scale(392, dpi), bottom: scale(60, dpi) };
    DrawTextW(hdc, &mut title, &mut rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    windows::Win32::Graphics::Gdi::SelectObject(hdc, HGDIOBJ(body_font.0));
    SetTextColor(hdc, rgb(169, 181, 191));
    let message = if snapshot.message.is_empty() { "正在保留设置并替换应用文件…".to_owned() } else { snapshot.message.clone() };
    let mut message: Vec<u16> = message.encode_utf16().collect();
    let mut rect = RECT { left: scale(104, dpi), top: scale(66, dpi), right: scale(392, dpi), bottom: scale(112, dpi) };
    DrawTextW(hdc, &mut message, &mut rect, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
    windows::Win32::Graphics::Gdi::SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(title_font.0));
    let _ = DeleteObject(HGDIOBJ(body_font.0));
    let _ = EndPaint(hwnd, &paint);
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_PAINT => { draw_overlay(hwnd); LRESULT(0) }
        WM_TIMER => { let _ = InvalidateRect(Some(hwnd), None, false); LRESULT(0) }
        WM_CLOSE => { let _ = DestroyWindow(hwnd); LRESULT(0) }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

pub struct Overlay {
    hwnd: HWND,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Overlay {
    pub fn show(state: Arc<Mutex<OverlayState>>) -> Result<Self> {
        dpi_aware();
        register_overlay_class();
        if let Ok(mut slot) = OVERLAY_STATE.lock() { *slot = Some(state); }
        let (sender, receiver) = std::sync::mpsc::channel::<std::result::Result<isize, String>>();
        let thread = std::thread::spawn(move || unsafe {
            let dpi = GetDpiForSystem().max(96);
            let width = scale(420, dpi);
            let height = scale(128, dpi);
            let mut area = RECT::default();
            let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut area as *mut _ as *mut core::ffi::c_void), Default::default());
            let x = area.left + ((area.right - area.left) - width) / 2;
            let y = area.top + scale(28, dpi);
            let class_name = win::wide("DotIsleUpdateOverlay");
            let title = win::wide("点屿更新");
            let instance = match GetModuleHandleW(None) { Ok(module) => windows::Win32::Foundation::HINSTANCE(module.0), Err(error) => { let _ = sender.send(Err(error.message().to_string())); return; } };
            let hwnd = match CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(error) => { let _ = sender.send(Err(error.message().to_string())); return; }
            };
            let region = windows::Win32::Graphics::Gdi::CreateRoundRectRgn(0, 0, width + 1, height + 1, scale(48, dpi), scale(48, dpi));
            windows::Win32::Graphics::Gdi::SetWindowRgn(hwnd, Some(region), true);
            let _ = SetTimer(Some(hwnd), 1, 33, None);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(hwnd);
            let _ = sender.send(Ok(hwnd.0 as isize));
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), 1);
        });
        match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(raw)) => Ok(Overlay { hwnd: HWND(raw as *mut core::ffi::c_void), thread: Some(thread) }),
            Ok(Err(error)) => Err(Error(format!("安装进度窗口未就绪：{error}"))),
            Err(_) => Err(Error("安装进度窗口未就绪".into())),
        }
    }

    pub fn close(&mut self) {
        unsafe { let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)); }
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
        if let Ok(mut slot) = OVERLAY_STATE.lock() { *slot = None; }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) { if self.thread.is_some() { self.close(); } }
}
