//! Vello takeover surface for the update helper that keeps drawing after the main app exits.
use crate::error::{Error, Result};
use crate::updater::OverlayState;
use dotisle_render::{draw::{self, Icon, Text}, surface::Surface, update_card::{self, Presentation, Style}};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use vello::{Scene, kurbo::{Affine, RoundedRect}, peniko::{Blob, Gradient, ImageAlphaType, ImageBrush, ImageData, ImageFormat, Mix}};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, MonitorFromRect, MONITOR_DEFAULTTONULL, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MA_NOACTIVATE, MSG, PeekMessageW, PM_REMOVE,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SPI_GETWORKAREA, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_PAINT,
    WNDCLASSW, WM_QUIT, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

const MAX_APPEARANCE_BYTES: usize = 64 * 1024;
const MAX_BACKGROUND_BYTES: usize = 32 * 1024 * 1024;
const EXPORT_MAX_AGE_MS: u64 = 15 * 60 * 1000;

/// Everything the takeover window needs to look exactly like the island it replaces.
#[derive(Clone)]
pub struct Look {
    pub window: RECT,
    pub scale: f64,
    pub card: (f64, f64, f64, f64),
    pub radius: f64,
    pub style: Style,
    pub font_family: String,
    pub animate: bool,
    pub background: Option<ImageData>,
    pub time_offset: f64,
    pub source: String,
}

impl Look {
    pub fn logical_size(&self) -> (f64, f64) {
        let width = self.window.right - self.window.left;
        let height = self.window.bottom - self.window.top;
        (width as f64 / self.scale.max(0.1), height as f64 / self.scale.max(0.1))
    }
    pub fn device_size(&self) -> (i32, i32) {
        (self.window.right - self.window.left, self.window.bottom - self.window.top)
    }
}

fn epoch_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as u64).unwrap_or(0)
}

fn hex_role(value: &Value, fallback: u32) -> u32 {
    let Some(text) = value.as_str() else { return fallback };
    let text = text.trim().trim_start_matches('#');
    u32::from_str_radix(text, 16).map(|value| value & 0xffffff).unwrap_or(fallback)
}

fn fallback_style() -> Style {
    Style { primary: 0x9bd5ae, on_primary: 0x00391f, on_surface: 0xe1e3df, muted: 0xc0c9c0, outline: 0x8a938b }
}

fn work_area() -> RECT {
    let mut area = RECT::default();
    unsafe { let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut area as *mut _ as *mut std::ffi::c_void), Default::default()); }
    area
}

/// The island's own geometry is unknown before the first export, so the helper falls back to a
/// neutral card placed on the work area of the primary monitor.
fn fallback_look() -> Look {
    let dpi = unsafe { GetDpiForSystem() }.max(96) as f64;
    let scale = dpi / 96.;
    let (width, height) = (448., 252.);
    let area = work_area();
    let device_width = (width * scale).round() as i32;
    let device_height = (height * scale).round() as i32;
    let x = area.left + ((area.right - area.left) - device_width) / 2;
    let y = area.top - (24. * scale).round() as i32;
    Look {
        window: RECT { left: x, top: y, right: x + device_width, bottom: y + device_height },
        scale,
        card: (24., 24., 400., 196.),
        radius: 28.,
        style: fallback_style(),
        font_family: "Noto Sans SC".into(),
        animate: true,
        background: None,
        time_offset: 0.,
        source: "fallback".into(),
    }
}

fn parse_look(value: &Value, job: Option<&str>, strict: bool) -> Result<Look> {
    if value["version"].as_u64() != Some(1) { return Err(Error("外观快照版本无效".into())); }
    if let Some(job) = job {
        if value["jobId"].as_str() != Some(job) { return Err(Error("外观快照不属于本次更新任务".into())); }
    } else if strict {
        let exported = value["exportedAtMs"].as_u64().unwrap_or(0);
        if epoch_ms().saturating_sub(exported) > EXPORT_MAX_AGE_MS { return Err(Error("外观快照已过期".into())); }
    }
    let scale = value["scale"].as_f64().unwrap_or(1.).clamp(0.5, 5.);
    let window = value["window"].clone();
    let left = window["x"].as_i64().unwrap_or(0);
    let top = window["y"].as_i64().unwrap_or(0);
    let width = window["width"].as_i64().unwrap_or(0);
    let height = window["height"].as_i64().unwrap_or(0);
    if !(32..=8192).contains(&width) || !(32..=8192).contains(&height) { return Err(Error("外观快照尺寸无效".into())); }
    if unsafe { MonitorFromRect(&RECT { left: left as i32, top: top as i32, right: left as i32 + width as i32, bottom: top as i32 + height as i32 }, MONITOR_DEFAULTTONULL) }.0.is_null() {
        return Err(Error("外观快照位置不在任何显示器上".into()));
    }
    let card = value["card"].clone();
    let card_x = card["x"].as_f64().unwrap_or(0.);
    let card_y = card["y"].as_f64().unwrap_or(0.);
    let card_w = card["width"].as_f64().unwrap_or(0.);
    let card_h = card["height"].as_f64().unwrap_or(0.);
    let (logical_w, logical_h) = (width as f64 / scale, height as f64 / scale);
    if card_w < 120. || card_h < 60. { return Err(Error("外观快照卡片尺寸无效".into())); }
    if card_x < 0. || card_y < 0. || card_x + card_w > logical_w + 0.5 || card_y + card_h > logical_h + 0.5 {
        return Err(Error("外观快照卡片超出窗口".into()));
    }
    let radius = value["radius"].as_f64().unwrap_or(0.).max(0.).min((card_w.min(card_h)) / 2.);
    let roles = value["roles"].clone();
    let style = Style {
        primary: hex_role(&roles["primary"], fallback_style().primary),
        on_primary: hex_role(&roles["onPrimary"], fallback_style().on_primary),
        on_surface: hex_role(&roles["onSurface"], fallback_style().on_surface),
        muted: hex_role(&roles["onSurfaceVariant"], fallback_style().muted),
        outline: hex_role(&roles["outline"], fallback_style().outline),
    };
    let font_family = value["fontFamily"].as_str().unwrap_or("Noto Sans SC").chars().take(64).collect::<String>();
    Ok(Look {
        window: RECT { left: left as i32, top: top as i32, right: left as i32 + width as i32, bottom: top as i32 + height as i32 },
        scale,
        card: (card_x, card_y, card_w, card_h),
        radius,
        style,
        font_family: if font_family.is_empty() { "Noto Sans SC".into() } else { font_family },
        animate: value["animate"].as_bool().unwrap_or(true),
        background: None,
        time_offset: value["timeMs"].as_f64().unwrap_or(0.),
        source: "export".into(),
    })
}

fn appearance_path(data: &Path) -> PathBuf { data.join("Cache/UpdateAppearance/appearance.json") }

fn decode_background(data: &Path, window: (i32, i32), notes: &mut Vec<String>) -> Option<ImageData> {
    let path = data.join("Cache/UpdateAppearance/background.png");
    let Ok(meta) = std::fs::metadata(&path) else { notes.push("没有找到背景快照，改用内置材质。".into()); return None; };
    if meta.len() as usize > MAX_BACKGROUND_BYTES { notes.push("背景快照过大，已忽略。".into()); return None; }
    let Ok(bytes) = std::fs::read(&path) else { return None; };
    let Ok(mut reader) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format() else { return None; };
    reader.limits(image::Limits::default());
    let Ok(decoded) = reader.decode() else { notes.push("背景快照无法解析，已忽略。".into()); return None; };
    let picture = decoded.to_rgba8();
    let (width, height) = (picture.width() as i32, picture.height() as i32);
    if (width, height) != window { notes.push("背景快照尺寸与窗口不一致，已忽略。".into()); return None; }
    Some(ImageData { data: Blob::new(Arc::new(picture.into_raw())), format: ImageFormat::Rgba8, alpha_type: ImageAlphaType::Alpha, width: width as u32, height: height as u32 })
}

fn load_look(data: &Path, job: Option<&str>, strict: bool) -> (Look, Vec<String>) {
    let mut notes = Vec::new();
    let mut look = fallback_look();
    let path = appearance_path(data);
    let Ok(bytes) = std::fs::read(&path) else { notes.push("没有找到外观快照，使用内置配色。".into()); return (look, notes); };
    if bytes.len() > MAX_APPEARANCE_BYTES { notes.push("外观快照过大，已忽略。".into()); return (look, notes); }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { notes.push("外观快照无法解析，使用内置配色。".into()); return (look, notes); };
    match parse_look(&value, job, strict) {
        Ok(parsed) => {
            let mut parsed = parsed;
            let window = parsed.device_size();
            parsed.background = decode_background(data, window, &mut notes);
            look = parsed;
        }
        Err(error) => notes.push(format!("外观快照不可用：{error}")),
    }
    (look, notes)
}

fn register_fonts(text: &mut Text, target: &Path, family: &str) -> Vec<String> {
    let mut registered = Vec::new();
    let mut names = vec!["GoogleSansFlex-Regular.ttf", "GoogleSansFlex-Medium.ttf", "GoogleSansFlex-SemiBold.ttf", "GoogleSansFlex-Bold.ttf", "NotoSansSC.ttf", "NotoSansJP.ttf"];
    if family == "Noto Serif SC" { names.push("NotoSerifSC.ttf"); }
    for name in names {
        let path = target.join("Assets/Fonts").join(name);
        if path.is_file() {
            match text.register(&path) { Ok(()) => registered.push(name.to_owned()), Err(error) => registered.push(format!("{name}: {error}")) }
        }
    }
    text.set_family(family);
    registered
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT => { let mut paint = PAINTSTRUCT::default(); let _ = BeginPaint(hwnd, &mut paint); let _ = EndPaint(hwnd, &paint); LRESULT(0) }
        WM_CLOSE => { let _ = DestroyWindow(hwnd); LRESULT(0) }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn register_class() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| unsafe {
        let instance = match GetModuleHandleW(None) { Ok(module) => module, Err(_) => return };
        let class = WNDCLASSW { lpfnWndProc: Some(overlay_proc), hInstance: instance.into(), lpszClassName: w!("DotIsleUpdateSurface"), ..Default::default() };
        let _ = RegisterClassW(&class);
    });
}

unsafe fn create_window(look: &Look) -> std::result::Result<HWND, String> {
    let instance = GetModuleHandleW(None).map_err(|error| error.message().to_string())?;
    let (width, height) = look.device_size();
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
        w!("DotIsleUpdateSurface"),
        w!("DotIsle · Update"),
        WS_POPUP,
        look.window.left, look.window.top, width, height,
        None, None, Some(instance.into()), None,
    ).map_err(|error| error.message().to_string())?;
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
    Ok(hwnd)
}

/// The card content is painted with the same helper the island uses, so the takeover keeps the
/// exact typography, ring and copy of the panel it replaces.
fn compose(scene: &mut Scene, text: &mut Text, look: &Look, state: &OverlayState, version: &str, time: f64) {
    let (card_x, card_y, card_w, card_h) = look.card;
    let phase = state.phase.as_str();
    let (title, subtitle, progress) = match phase {
        "restarting" => (format!("正在重启点屿 v{version}"), "正在启动新版并确认…".to_owned(), -1.),
        "completed" => (format!("更新完成 v{version}"), if state.message.is_empty() { "新版已启动，设置已保留。".to_owned() } else { state.message.clone() }, 1.),
        "failed" => ("更新未完成".to_owned(), if state.message.is_empty() { "正在恢复原版本…".to_owned() } else { state.message.clone() }, -1.),
        _ => (format!("正在安装更新 v{version}"), if state.message.is_empty() { "正在保存数据并安装，请勿关闭…".to_owned() } else { state.message.clone() }, -1.),
    };
    let mut content = Scene::new();
    let presentation = Presentation {
        title: &title,
        value: "",
        subtitle: &subtitle,
        progress,
        time_ms: time,
        animate: look.animate,
        close: false,
        action: None,
        feedback: 0.,
    };
    update_card::paint(&mut content, text, card_w, card_h, look.style, &presentation);
    if phase == "completed" {
        let (centre, radius) = update_card::ring_center(card_h);
        draw::icon(&mut content, Icon::Check, centre.0 - radius * 0.42, centre.1 - radius * 0.42, radius * 0.84, 0x9bd5ae);
    }
    let shape = RoundedRect::from_rect(vello::kurbo::Rect::new(card_x, card_y, card_x + card_w, card_y + card_h), look.radius);
    scene.push_layer(Mix::Normal, 1., Affine::IDENTITY, &shape);
    scene.append(&content, Some(Affine::translate((card_x, card_y))));
    scene.pop_layer();
}

/// Draws the exported island frame, or a neutral material when no export exists.
fn paint_background(scene: &mut Scene, look: &Look) {
    if let Some(image) = look.background.clone() {
        scene.draw_image(&ImageBrush::from(image), Affine::scale(1. / look.scale.max(0.1)));
        return;
    }
    let (logical_w, logical_h) = look.logical_size();
    let (card_x, card_y, card_w, card_h) = look.card;
    let card = vello::kurbo::Rect::new(card_x, card_y, card_x + card_w, card_y + card_h);
    scene.draw_blurred_rounded_rect(Affine::translate((0., 5.)), card, draw::color(0x38000000), look.radius, 22. / 3.);
    let gradient = Gradient::new_linear((card_x, card_y), (card_x, card_y + card_h)).with_stops([draw::color(0xff1b1f25), draw::color(0xff0c0f14)]);
    scene.fill(vello::peniko::Fill::NonZero, Affine::IDENTITY, &gradient, None, &RoundedRect::from_rect(card, look.radius));
    let frost = Gradient::new_linear((card_x, card_y), (card_x, card_y + card_h)).with_stops([draw::alpha(0xffffff, 15. / 255.), draw::alpha(0xffffff, 0.)]);
    scene.fill(vello::peniko::Fill::NonZero, Affine::IDENTITY, &frost, None, &RoundedRect::from_rect(card, look.radius));
    let edge = vello::kurbo::Rect::new(card_x + 0.5, card_y + 0.5, card_x + card_w - 1., card_y + card_h - 1.);
    draw::outline(scene, edge, (look.radius - 0.5).max(0.), 0x12ffffff, 0.8);
    let _ = logical_w;
    let _ = logical_h;
}

fn draw_frame(surface: &mut Surface, text: &mut Text, look: &Look, state: &OverlayState, version: &str, time: f64) -> std::result::Result<(), String> {
    let mut scene = Scene::new();
    paint_background(&mut scene, look);
    compose(&mut scene, text, look, state, version, time);
    surface.draw(&scene, look.scale, true)
}

fn draw_frame_offscreen(surface: &mut Surface, text: &mut Text, look: &Look, state: &OverlayState, version: &str, time: f64) -> std::result::Result<(), String> {
    let mut scene = Scene::new();
    paint_background(&mut scene, look);
    compose(&mut scene, text, look, state, version, time);
    surface.draw(&scene, look.scale, false)
}

pub struct Overlay {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub info: Value,
}

impl Overlay {
    /// The handshake is the contract: the caller must not let the main app exit before the
    /// takeover window has drawn its first frame.
    pub fn show(state: Arc<Mutex<OverlayState>>, target: &Path, data: &Path, version: &str, job: Option<&str>) -> Result<Self> {
        crate::ui::dpi_aware();
        register_class();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let state = state.clone();
        let target = target.to_path_buf();
        let data = data.to_path_buf();
        let version = version.to_owned();
        let job = job.map(str::to_owned);
        let (sender, receiver) = mpsc::channel::<std::result::Result<Value, String>>();
        let thread = std::thread::spawn(move || {
            let started = Instant::now();
            let (look, notes) = load_look(&data, job.as_deref(), true);
            let mut text = Text::empty();
            let fonts = register_fonts(&mut text, &target, &look.font_family);
            let result = (|| -> std::result::Result<Value, String> {
                unsafe {
                    let hwnd = create_window(&look)?;
                    let (width, height) = look.device_size();
                    let mut surface = Surface::new(hwnd, width.max(1) as u32, height.max(1) as u32)?;
                    let mut scene = Scene::new();
                    paint_background(&mut scene, &look);
                    let snapshot = state.lock().ok().map(|value| value.clone()).unwrap_or_default();
                    compose(&mut scene, &mut text, &look, &snapshot, &version, look.time_offset);
                    surface.draw(&scene, look.scale, true).ok();
                    let info = json!({
                        "success": true,
                        "source": look.source,
                        "notes": notes,
                        "fontsRegistered": fonts,
                        "geometry": {"scale": look.scale, "card": {"x": look.card.0, "y": look.card.1, "width": look.card.2, "height": look.card.3}, "radius": look.radius},
                        "window": {"x": look.window.left, "y": look.window.top, "width": look.device_size().0, "height": look.device_size().1},
                        "background": look.background.is_some(),
                        "firstFrameMs": started.elapsed().as_millis() as u64,
                    });
                    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), look.window.left, look.window.top, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    if thread_stop.load(Ordering::Relaxed) { let _ = DestroyWindow(hwnd); return Err("安装进度窗口已关闭".into()); }
                    let _ = sender.send(Ok(info));
                    let mut message = MSG::default();
                    let mut last_topmost = Instant::now();
                    while !thread_stop.load(Ordering::Relaxed) {
                        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            if message.message == WM_QUIT { break; }
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        let snapshot = state.lock().ok().map(|value| value.clone()).unwrap_or_default();
                        let time = look.time_offset + started.elapsed().as_secs_f64() * 1000.;
                        draw_frame(&mut surface, &mut text, &look, &snapshot, &version, time)?;
                        if matches!(snapshot.phase.as_str(), "restarting" | "completed" | "failed") && last_topmost.elapsed() > Duration::from_millis(250) {
                            last_topmost = Instant::now();
                            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                        }
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    let _ = DestroyWindow(hwnd);
                    Ok(Value::Null)
                }
            })();
            if let Err(error) = result { let _ = sender.send(Err(error)); }
        });
        match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(info)) => Ok(Overlay { stop, thread: Some(thread), info }),
            Ok(Err(error)) => { stop.store(true, Ordering::Relaxed); let _ = thread.join(); Err(Error(format!("安装进度窗口未就绪：{error}"))) }
            Err(_) => { stop.store(true, Ordering::Relaxed); Err(Error("安装进度窗口未就绪".into())) }
        }
    }

    pub fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) { if self.thread.is_some() { self.close(); } }
}

/// Renders every update phase into PNG files so the takeover surface can be reviewed offline.
pub fn preview(out: &Path, target: &Path, data: &Path, version: &str) -> Result<Value> {
    crate::ui::dpi_aware();
    register_class();
    std::fs::create_dir_all(out)?;
    let (look, notes) = load_look(data, None, false);
    let mut text = Text::empty();
    let fonts = register_fonts(&mut text, target, &look.font_family);
    let phases = [
        ("applying", OverlayState { phase: "applying".into(), message: "正在保存数据并安装，请勿关闭…".into(), ..Default::default() }, look.time_offset),
        ("applying-late", OverlayState { phase: "applying".into(), message: "正在保存数据并安装，请勿关闭…".into(), ..Default::default() }, look.time_offset + 900.),
        ("restarting", OverlayState { phase: "restarting".into(), message: "正在启动新版并确认…".into(), ..Default::default() }, look.time_offset + 1200.),
        ("completed", OverlayState { phase: "completed".into(), message: "新版已启动，设置已保留。".into(), ..Default::default() }, look.time_offset + 1500.),
        ("failed", OverlayState { phase: "failed".into(), message: "新版未能启动，正在恢复原版本…".into(), ..Default::default() }, look.time_offset + 1500.),
    ];
    let mut files = Vec::new();
    unsafe {
        let hwnd = create_window(&look)?;
        let (width, height) = look.device_size();
        let mut surface = Surface::new(hwnd, width.max(1) as u32, height.max(1) as u32)?;
        for (name, state, time) in &phases {
            draw_frame_offscreen(&mut surface, &mut text, &look, state, version, *time)?;
            let path = out.join(format!("overlay-{name}.png"));
            surface.snapshot(&path).map_err(|error| Error(error.to_string()))?;
            files.push(path.file_name().unwrap_or_default().to_string_lossy().to_string());
        }
        let _ = DestroyWindow(hwnd);
    }
    let report = json!({
        "success": true,
        "version": version,
        "source": look.source,
        "notes": notes,
        "fontsRegistered": fonts,
        "background": look.background.is_some(),
        "geometry": {"scale": look.scale, "card": {"x": look.card.0, "y": look.card.1, "width": look.card.2, "height": look.card.3}, "radius": look.radius},
        "files": files,
    });
    let _ = std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report).unwrap_or_default());
    Ok(report)
}

/// Shows the takeover surface on screen so it can be filmed or checked by eye.
pub fn demo(target: &Path, data: &Path, version: &str) -> Result<Value> {
    let state = Arc::new(Mutex::new(OverlayState { phase: "applying".into(), message: "正在保存数据并安装，请勿关闭…".into(), ..Default::default() }));
    let mut overlay = Overlay::show(state.clone(), target, data, version, None)?;
    let steps = [("applying", 2000u64), ("restarting", 2000), ("completed", 1400)];
    for (phase, millis) in steps {
        std::thread::sleep(Duration::from_millis(millis));
        if let Ok(mut value) = state.lock() { value.phase = phase.into(); }
    }
    overlay.close();
    Ok(overlay.info.clone())
}

/// Entry point for `LyricIsland.Updater.exe --overlay-preview|--overlay-demo <out> [target] [data] [version]`.
pub fn cli(args: &[String]) -> Option<i32> {
    let mode = args.first().map(String::as_str)?;
    if mode != "--overlay-preview" && mode != "--overlay-demo" { return None; }
    let Some(out) = args.get(1) else {
        let _ = std::fs::write("report.json", serde_json::to_vec_pretty(&json!({"success": false, "message": "缺少输出目录"})).unwrap_or_default());
        return Some(1);
    };
    let target = args.get(2).map(PathBuf::from).unwrap_or_else(|| std::env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf)).unwrap_or_default());
    let data = args.get(3).map(PathBuf::from).unwrap_or_else(|| target.join("Data"));
    let version = args.get(4).cloned().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    let out = PathBuf::from(out);
    let result = if mode == "--overlay-preview" { preview(&out, &target, &data, &version).map(|_| 0) } else { demo(&target, &data, &version).map(|_| 0) };
    match result {
        Ok(code) => Some(code),
        Err(error) => {
            let _ = std::fs::create_dir_all(&out);
            let _ = std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&json!({"success": false, "message": error.to_string()})).unwrap_or_default());
            Some(1)
        }
    }
}
