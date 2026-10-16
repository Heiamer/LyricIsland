//! DotIsle settings — GPUI frontend.
//!
//! Replaces the Slint settings window. The native capsule still owns the pipe server, so this
//! binary is a pure client and keeps the existing contract (see `protocol.rs`).
//!
//! Design follows the bundled `apple-design` skill: depth comes from layered surfaces and
//! hairlines instead of heavy borders, a restrained palette around the product's mint accent,
//! feedback lands on pointer-down rather than on release, and the user's own "界面动画" switch —
//! not the OS setting — governs motion.
#![windows_subsystem = "windows"]
// gpui's element macros expand deeply; the default limit is not enough once a test harness wraps
// the same code in `#[test]`.
#![recursion_limit = "512"]

mod changelog;
mod color;
mod history;
mod protocol;
mod text_field;
mod theme;
mod value;

use text_field::TextField;
use value::{format_value, position_to_value};

use std::{
    borrow::Cow,
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Mutex,
    time::Duration,
};

use color::Hsv;
use gpui::{prelude::*, *};
use protocol as pipe;
use serde_json::{json, Value};
use theme as t;

/// Pages in sidebar order. `--smoke-ui` walks all of them.
const PAGES: [(&str, &str); 11] = [
 ("常规","启动与窗口行为。"),("灵动岛","尺寸、展开与活动切换。"),("歌词与同步","显示每一句歌词和翻译。"),("外观与动画","配色、主题与连续运动。"),("播放器与来源","选择播放器和在线信息来源。"),("通知与设备","消息与设备接入提醒。"),("音效","分别调整通知、提醒与设备音量。"),("待办清单","把重要的小事放在眼前。"),("计时器","专注、倒计时，或记录经过的时间。"),("关于与更新","版本、在线更新与许可。"),("更新日志","查看历次版本变化。"),
];

/// Index of the todo page, so the native side can ask for it by name.
const PAGE_TODOS: usize = 7;

struct Shared {
    state: Value,
    notice: String,
    message: String,
    connection_error: bool,
    message_is_connection_error:bool,
}

/// Which surface a drag is currently driving.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    /// Saturation/value pad.
    Pad,
    /// Hue strip.
    Hue,
}

/// An open colour picker. `hsv` is the live value: it follows the pointer continuously and is only
/// sent to the native side when the gesture ends, so dragging never floods the pipe.
#[derive(Clone, Copy)]
struct Picker {
    key: &'static str,
    hsv: Hsv,
    drag: Drag,
}

/// A value being dragged. The bounds are carried along so the mapping stays exact even though the
/// pointer may leave the track.
#[derive(Clone, Copy)]
struct SliderDrag {
    key: &'static str,
    min: f64,
    max: f64,
    bounds: Bounds<Pixels>,
    value: f64,
}

/// Window state shared by every page.
struct SettingsApp {
    shared: Rc<Mutex<Shared>>,
    page: usize,
    picker: Option<Picker>,
    slider: Option<SliderDrag>,
    /// Where the pad and the hue strip were actually painted, in window coordinates. Recorded during
    /// prepaint because pointer events only carry a window position.
    pad_bounds: Rc<Cell<Bounds<Pixels>>>,
    hue_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Scroll state of the content column, so a self-check can bring the picker into view.
    scroll: ScrollHandle,
    /// Whether the last frame asked DWM for a dark (or light) non-client frame.
    applied_dark: Option<bool>,
    /// Track rectangle of the slider named by `LYRICISLAND_SMOKE_SLIDER`, published so an automated
    /// drag can aim at it.
    smoke_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Text fields are created once and reused: building them inside `render` would replace the
    /// entity every frame and the field would lose its contents and focus.
    hex_field:Option<Entity<TextField>>,
    calibrate: Entity<TextField>,
    custom_sources: Entity<TextField>,
    notification_apps: Entity<TextField>,
    /// Todo form fields. Created once and reused, exactly like the fields above: rebuilding them
    /// inside `render` would drop focus and the half-typed text on every frame.
    todo_title: Entity<TextField>,
    todo_content: Entity<TextField>,
    todo_date: Entity<TextField>,
    todo_time: Entity<TextField>,
    /// `Some(id)` while an existing todo is being edited, `None` when composing a new one.
    todo_editing: Option<String>,
    todo_form_open: bool,
    todo_daily: bool,
    todo_settings:bool,todo_filter:usize,todo_completed_open:bool,todo_color:String,
    timer_mode:String,timer_new:bool,timer_adjust_open:bool,timer_name:Entity<TextField>,timer_duration:Entity<TextField>,timer_focus:Entity<TextField>,timer_rest:Entity<TextField>,timer_rounds:Entity<TextField>,
}

impl SettingsApp {
    fn new(shared: Rc<Mutex<Shared>>, picker: Option<Picker>, cx: &mut Context<Self>) -> Self {
        let calibrate = {
            let shared = Rc::clone(&shared);
            cx.new(|cx| {
                TextField::new(
                    "field-calibrate",
                    "输入播放器显示的秒数，按 Enter 校准",
                    "",
                    Rc::new(move |value: &str, app: &mut App| {
                        let _ = send(&shared, "synchronize", value, app);
                    }),
                    cx,
                )
            })
        };
        let custom_sources = {
            let shared = Rc::clone(&shared);
            cx.new(|cx| {
                TextField::new(
                    "field-custom-sources",
                    "输入程序名，多个用分号分隔",
                    "",
                    Rc::new(move |value: &str, app: &mut App| {
                        let _ = send(&shared, "customSources", value, app);
                    }),
                    cx,
                )
            })
        };
        let notification_apps = {
            let shared = Rc::clone(&shared);
            cx.new(|cx| {
                TextField::new(
                    "field-notification-apps",
                    "输入完整应用标识（AUMID），多个用分号分隔",
                    "",
                    Rc::new(move |value: &str, app: &mut App| {
                        let _ = send(&shared, "notificationApps", value, app);
                    }),
                    cx,
                )
            })
        };
        // Enter in any todo field saves the form. The callback reaches back into the settings view
        // through a weak handle, which is why the fields can be built before `Self` exists.
        let weak = cx.entity().downgrade();
        let todo_field = |id: &'static str, placeholder: &'static str, cx: &mut Context<Self>| {
            let weak = weak.clone();
            cx.new(|cx| {
                TextField::new(
                    id,
                    placeholder,
                    "",
                    Rc::new(move |_value: &str, app: &mut App| {
                        let _ = weak.update(app, |view, cx| view.submit_todo(cx));
                    }),
                    cx,
                )
            })
        };
        let todo_title = todo_field("field-todo-title", "待办题目", cx);
        let todo_content = todo_field("field-todo-content", "内容，可留空", cx);
        let todo_date = todo_field("field-todo-date", "YYYY-MM-DD", cx);
        let todo_time = todo_field("field-todo-time", "HH:MM（24 小时制）", cx);
        let timer_name=cx.new(|cx|TextField::new("timer-name","可留空","",Rc::new(|_,_|{}),cx));
        let timer_duration=cx.new(|cx|TextField::new("timer-duration","秒数","300",Rc::new(|_,_|{}),cx));
        let timer_focus=cx.new(|cx|TextField::new("timer-focus","秒数","1500",Rc::new(|_,_|{}),cx));
        let timer_rest=cx.new(|cx|TextField::new("timer-rest","秒数","300",Rc::new(|_,_|{}),cx));
        let timer_rounds=cx.new(|cx|TextField::new("timer-rounds","轮数","4",Rc::new(|_,_|{}),cx));
        Self {
            timer_name,timer_duration,timer_focus,timer_rest,timer_rounds,timer_mode:"pomodoro".into(),timer_new:false,timer_adjust_open:false,todo_settings:false,todo_filter:0,todo_completed_open:false,todo_color:"#F3D79B".into(),
            shared,
            page: smoke_page(),
            picker,
            slider: None,
            pad_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            hue_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            scroll: ScrollHandle::new(),
            applied_dark: None,
            smoke_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            hex_field:picker.map(|p|make_hex_field(&t::to_hex(p.hsv.to_rgb()),cx)),
            calibrate,
            custom_sources,
            notification_apps,
            todo_title,
            todo_content,
            todo_date,
            todo_time,
            todo_editing: None,
            todo_form_open: false,
            todo_daily: false,
        }
    }
}

impl SettingsApp {
    fn state(&self) -> Value {
        self.shared.lock().map(|s| s.state.clone()).unwrap_or_else(|_| json!({}))
    }

    /// Read the form entities and submit them. Validation lives on the native side, so an invalid
    /// date or time comes back as a message rather than being silently dropped here.
    fn submit_todo(&mut self, cx: &mut Context<Self>) {
        let payload = json!({
            "id": self.todo_editing.clone().unwrap_or_default(),
            "title": self.todo_title.read(cx).text(),
            "content": self.todo_content.read(cx).text(),
            "date": self.todo_date.read(cx).text(),
            "time": self.todo_time.read(cx).text(),
            "daily": self.todo_daily,"color":self.todo_color,
        });
        let weak=cx.entity().downgrade();let job=cx.background_executor().spawn(async move{pipe::command("todoSave",payload)});
        cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{match outcome{Ok((reply,state))=>{let success=reply["success"]==true;if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("保存失败").into()};}if success{view.todo_form_open=false;view.todo_editing=None;}},Err(e)=>{if let Ok(mut shared)=view.shared.lock(){shared.message=e;}}}cx.notify();});}).detach();
    }

    /// Open the form, either empty (defaulting to today) or loaded from an existing todo.
    fn open_todo_form(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let state = self.state();
        let existing = id.as_ref().and_then(|id| {
            state["todos"]
                .as_array()?
                .iter()
                .find(|todo| todo["id"].as_str() == Some(id.as_str()))
                .cloned()
        });
        let (title, content, date, time, daily) = match &existing {
            Some(todo) => (
                todo["title"].as_str().unwrap_or_default().to_owned(),
                todo["content"].as_str().unwrap_or_default().to_owned(),
                todo["date"].as_str().unwrap_or_default().to_owned(),
                todo["time"].as_str().unwrap_or_default().to_owned(),
                todo["daily"].as_bool().unwrap_or(false),
            ),
            None => (
                String::new(),
                String::new(),
                state["today"].as_str().unwrap_or_default().to_owned(),
                state["now"].as_str().unwrap_or_default().to_owned(),
                false,
            ),
        };
        self.todo_color=id.as_ref().and_then(|id|state["todos"].as_array().and_then(|a|a.iter().find(|t|t["id"].as_str()==Some(id)))).and_then(|t|t["color"].as_str()).unwrap_or(setting_str(&state,"TodoDefaultColor","#F3D79B")).to_owned();
        self.todo_editing = id;
        self.todo_form_open = true;
        self.todo_daily = daily;
        self.todo_title.update(cx, |field, cx| field.set_text(&title, cx));
        self.todo_content.update(cx, |field, cx| field.set_text(&content, cx));
        self.todo_date.update(cx, |field, cx| field.set_text(&date, cx));
        self.todo_time.update(cx, |field, cx| field.set_text(&time, cx));
        cx.notify();
    }

    fn close_todo_form(&mut self, cx: &mut Context<Self>) {
        self.todo_form_open = false;
        self.todo_editing = None;
        cx.notify();
    }
}

fn setting<'a>(state: &'a Value, key: &str) -> &'a Value {
    state.get("settings").and_then(|s| s.get(key)).unwrap_or(&Value::Null)
}

fn flag(state: &Value, key: &str, default: bool) -> bool {
    setting(state, key).as_bool().unwrap_or(default)
}

fn text<'a>(state: &'a Value, key: &str, default: &'a str) -> &'a str {
    state.get(key).and_then(Value::as_str).unwrap_or(default)
}

/// A string stored in the settings object (rather than a live field of the snapshot).
fn setting_str<'a>(state: &'a Value, key: &str, default: &'a str) -> &'a str {
    setting(state, key).as_str().unwrap_or(default)
}

/// A 1px wash used as a separator. Cheaper to reason about than a side-specific border.
fn hairline() -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(t::hairline())
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state();
        let (title, subtitle) = PAGES[self.page.min(PAGES.len() - 1)];
        // Appearance is resolved before anything is built: `theme` holds the palette for this frame.
        let theme = setting_str(&state, "SettingsTheme", "System").to_owned();
        let system_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let want_dark = match theme.as_str() {
            "Light" => false,
            "Dark" => true,
            _ => system_dark,
        };
        t::set_current(t::select(&theme, system_dark));
        // The caption bar follows the app, not the system, so both stay in step.
        if self.applied_dark != Some(want_dark) {
            apply_titlebar(window, want_dark);
            self.applied_dark = Some(want_dark);
        }
        let (notice, message) = self
            .shared
            .lock()
            .map(|s| (s.notice.clone(), s.message.clone()))
            .unwrap_or_default();
        let page = self.page;

        let mut sidebar = div().flex().flex_col().w(px(196.0)).flex_shrink_0().h_full().p_3().gap_1().bg(t::sidebar());
        for (index, (label, _)) in PAGES.iter().enumerate() {
            if index==0||index==5||index==9{sidebar=sidebar.child(div().px_3().pt_3().pb_1().text_size(px(10.)).text_color(t::text_faint()).child(if index==0{"个性化"}else if index==5{"提醒与工具"}else{"帮助"}));}
            sidebar = sidebar.child(nav_item(index, label, index == page, cx));
        }
        sidebar = sidebar.child(div().flex_1()).child(
            div().px_3().py_2().text_size(px(10.5)).text_color(t::text_faint()).child("音乐在前，界面在后。"),
        );

        // The scroll container's child must be content-sized: giving it `h_full` pinned it to the
        // viewport, so anything past the fold was clipped instead of becoming scrollable.
        let mut column = div().flex().flex_col().w_full().min_w_0().px_7().py_6()
            .child(div().text_size(px(26.0)).text_color(t::text()).child(title.to_owned()))
            .child(div().mt_1().text_size(px(13.0)).text_color(t::text_dim()).child(subtitle.to_owned()));
        column = column.child(div().mt_6().flex().flex_col().gap_3().children(page_rows(page, &state, self, cx)));

        let footer = div().mt_6().pt_3().text_size(px(11.0)).text_color(t::text_faint())
            .child(if notice.is_empty() { "正在连接灵动岛…".to_owned() } else { notice });

        div().flex().flex_col().size_full().bg(t::base()).text_color(t::text()).font_family(t::FONT)
            .child(settings_titlebar(window))
            .child(div().flex().flex_1().min_h_0().min_w_0()
            .child(sidebar)
            .child(div().w(px(1.0)).flex_shrink_0().h_full().bg(t::hairline()))
            .child(
                div().flex().flex_col().flex_1().min_w_0().h_full()
                    .child(div().id("content").flex_1().min_w_0().overflow_y_scroll().track_scroll(&self.scroll).child(column))
                    .child(hairline())
                    .child(div().px_7().py_3().min_w_0().children(if message.is_empty(){None}else{Some(div().text_size(px(12.)).text_color(if message.contains("失败")||message.contains("错误"){t::danger()}else{t::text()}).child(message))}).child(footer)),
            ))
    }
}

/// Sidebar row: a filled pill when selected, a wash under the pointer.
fn brand_mark()->AnyElement{canvas(|_,_,_|{},|bounds,_,window,_|{let x=bounds.origin.x;let y=bounds.origin.y;let mut shape=|left:f32,top:f32,w:f32,h:f32,r:f32,color:gpui::Rgba|window.paint_quad(quad(Bounds::new(point(x+px(left),y+px(top)),size(px(w),px(h))),px(r),color,px(0.),transparent_black(),BorderStyle::Solid));shape(1.,1.,24.,24.,6.,rgb(0xDDF3E9));shape(4.,9.,18.,8.5,4.25,rgb(0x128F74));shape(6.2,11.4,3.7,3.7,1.85,rgb(0xF5FFFB));shape(12.,12.,6.4,0.9,0.45,rgb(0xE9FFF6));shape(12.,14.,4.1,0.8,0.4,rgb(0x92D8BE));shape(18.5,4.8,3.5,3.5,1.75,rgb(0x14967A));shape(19.7,6.,1.1,1.1,0.55,rgb(0xF5FFFB));}).w(px(26.)).h(px(26.)).into_any_element()}
fn settings_titlebar(window:&Window)->AnyElement {
    let mut controls=div().flex().items_center().h_full().pr_2().gap_1();
    for (id,area,glyph) in [("window-minimize",WindowControlArea::Min,"\u{e921}"),("window-maximize",WindowControlArea::Max,if window.is_maximized(){"\u{e923}"}else{"\u{e922}"}),("window-close",WindowControlArea::Close,"\u{e8bb}")] {
        let close=area==WindowControlArea::Close;
        controls=controls.child(div().id(id).window_control_area(area).w(px(38.)).h(px(30.)).rounded(px(9.)).flex().items_center().justify_center().font_family("Segoe Fluent Icons").text_size(px(10.)).text_color(t::text_dim()).hover(move|s|if close{s.bg(t::danger()).text_color(rgb(0xffffff))}else{s.bg(t::hover()).text_color(t::text())}).child(glyph));
    }
    div().flex().w_full().h(px(44.)).flex_shrink_0()
        .child(div().id("window-brand-drag").window_control_area(WindowControlArea::Drag).w(px(196.)).flex_shrink_0().h_full().px_5().flex().items_center().gap_2().bg(t::sidebar())
            .child(brand_mark())
            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t::text()).child("点屿")))
        .child(div().w(px(1.)).h_full().bg(t::hairline()))
        .child(div().id("window-title-drag").window_control_area(WindowControlArea::Drag).flex_1().h_full().px_7().flex().items_center().text_size(px(11.)).text_color(t::text_faint()).child("设置"))
        .child(controls).into_any_element()
}

fn nav_item(index: usize, label: &str, selected: bool, cx: &mut Context<SettingsApp>) -> AnyElement {
    div()
        .id(("nav", index))
        .px_3()
        .py_2()
        .rounded(px(8.0))
        .cursor_pointer()
        .bg(if selected { t::surface_hi() } else { t::sidebar() })
        .hover(|style| style.bg(if selected { t::surface_hi() } else { t::hover() }))
        .text_size(px(13.0))
        .text_color(if selected { t::text() } else { t::text_dim() })
        .child(label.to_owned())
        .on_click(cx.listener(move |view, _, _, cx| {
            view.page = index;
            cx.notify();
        }))
        .into_any_element()
}

fn legacy_page_rows(page: usize, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    match page {
        0 => {
            let mut rows = vec![
                theme_row(state, cx),
                toggle_row("AutoStart", "开机自动启动", "登录 Windows 后驻留系统托盘。", flag(state, "AutoStart", false), view),
                toggle_row("AlwaysOnTop", "始终置顶", "显示在其他软件上方，不抢占键盘焦点。", flag(state, "AlwaysOnTop", true), view),
                toggle_row("WordHighlight", "歌词进度涂色", "按照演唱进度高亮歌词。", flag(state, "WordHighlight", true), view),
                toggle_row("HideWhenPaused", "暂停后隐藏", "暂停时收起淡出，恢复播放时显示胶囊。", flag(state, "HideWhenPaused", true), view),
                toggle_row("HoverExpand", "悬停展开", "点击封面可保持展开；移开鼠标自动收起。", flag(state, "HoverExpand", false), view),
                toggle_row("ClickAnywhereExpand", "点击任意区域展开", "开启后点击收起状态的任意位置即可展开；关闭时仅封面响应。", flag(state, "ClickAnywhereExpand", true), view),
                slider_row("CompactWidth", "胶囊宽度", "调整收起时的歌词空间。", 140.0, 480.0, "px", slider_value(state, "CompactWidth", view, 260.0), view, cx),
                slider_row("FontSize", "歌词字号", "文字大小即时生效。", 11.0, 17.0, "px", slider_value(state, "FontSize", view, 12.5), view, cx),
                toggle_row("Floating", "自由悬浮", "按住移动按钮拖动，方向键可微调。", flag(state, "Floating", false), view),
            ];
            // The picker opens inline, directly under the row it edits, so the swatch and the pad
            // stay in the same glance.
            for (key, title) in [
                ("ExpandedTextColor", "展开卡片字体颜色"),
                ("ExpandedComponentColor", "展开卡片组件颜色"),
                ("LyricProgressColor", "歌词进度填色"),
            ] {
                rows.push(color_row(key, title, state, view, cx));
                if view.picker.as_ref().is_some_and(|picker| picker.key == key) {
                    rows.push(picker_panel(view, cx));
                }
            }
            rows.push(toggle_row("CoverBackground", "封面取色背景", "默认开启。根据歌曲封面为展开卡片取色，自动压低亮度。", flag(state, "CoverBackground", true), view));
            rows.push(action_row("resetAppearance", "恢复紧凑尺寸", "恢复宽度、字号与固定位置。", "恢复紧凑尺寸", view, cx));
            rows.push(toggle_row("MusicClickOpenApp", "点击封面或歌名跳转播放器", "默认关闭。开启后点击音乐封面或歌名打开当前播放器。", flag(state,"MusicClickOpenApp",false),view));
            rows
        }
        1 => {
            let motion = setting(state, "Motion");
            let _ = motion;
            let mut rows = vec![toggle_row(
                "Animation",
                "界面动画",
                "连续形变可以随时反向操作，由软件开关控制，不跟随 Windows 动画设置。",
                flag(state, "Animation", true),
                view,
            )];
            for (key, title, detail, min, max, default, unit) in [
                ("MotionGap", "内容间距", "调整展开后的内容留白。", 0.0, 24.0, 8.0, "px"),
                ("MotionDuration", "展开响应", "调整展开速度。", 250.0, 1200.0, 360.0, "ms"),
                ("MotionCollapseDuration", "收起响应", "调整收起速度。", 150.0, 700.0, 300.0, "ms"),
                ("MotionRadius", "卡片圆角", "胶囊形变后平滑落到这个圆角。", 8.0, 48.0, 30.0, "px"),
                ("SpringStiffness", "弹簧刚度", "越高响应越快。", 100.0, 1000.0, 420.0, ""),
                ("SpringDamping", "弹簧阻尼", "越高回弹越少。", 10.0, 80.0, 30.0, ""),
                ("SpringMass", "弹簧质量", "调整运动的重量感。", 0.3, 2.0, 0.8, ""),
            ] {
                rows.push(slider_row(key, title, detail, min, max, unit, slider_value(state, key, view, default), view, cx));
            }
            rows
        }
        2 => {
            let position = state["position"].as_f64().unwrap_or(0.0);
            let duration = state["duration"].as_f64().unwrap_or(0.0);
            vec![
                info_row(
                    "同步状态",
                    text(state, "timeline", "等待播放器"),
                    &format!(
                        "当前位置 {position:.1} 秒 / 总时长 {duration:.1} 秒\n歌词来源：{}",
                        text(state, "lyricSource", "等待获取")
                    ),
                ),
                slider_row(
                    "OffsetMilliseconds",
                    "歌词偏移",
                    "正数让歌词提前，负数让歌词延后。",
                    -5000.0,
                    5000.0,
                    "ms",
                    slider_value(state, "OffsetMilliseconds", view, 0.0),
                    view,
                    cx,
                ),
                toggle_row("Translation", "显示翻译", "展开时优先显示翻译，没有翻译时显示下一句。", flag(state, "Translation", true), view),
                toggle_row("CompactTranslation", "收起时显示翻译", "默认开启。收起状态下原文在上、翻译在下；没有翻译时不会留出空行。", flag(state, "CompactTranslation", true), view),
                toggle_row("ChineseTwoLine", "中文双行模式", "默认关闭。没有翻译时，在当前句下方以更小的字显示下一句。", flag(state, "ChineseTwoLine", false), view),
                toggle_row("AutoCompactWidth", "歌词长度自适应宽度", "默认关闭。收起状态按真实字形宽度收缩胶囊，仍保留封面与播放活动条。", flag(state, "AutoCompactWidth", false), view),
                calibrate_row(position, view, cx),
            ]
        }
        3 => {
            let mut rows = Vec::new();
            for (id, name) in pipe::PLAYERS {
                let allowed = source_allowed(state, id);
                rows.push(check_row(
                    SharedString::from(format!("source:{id}")),
                    name,
                    if id == "Browser" { "默认关闭，避免网页视频唤出胶囊。" } else { "仅响应允许的播放器。" },
                    allowed,
                    cx,
                ));
            }
            let custom = custom_sources(state);
            rows.push(
                card("row-custom-add", "添加自定义程序", "输入程序名或媒体会话标识，不支持通配符和启动参数。按 Enter 添加。")
                    .child(div().w(px(300.0)).child(view.custom_sources.clone()))
                    .into_any_element(),
            );
            if !custom.is_empty() {
                let mut list = div().flex().flex_row().flex_wrap().gap_2();
                for id in &custom {
                    let owned = id.clone();
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("custom-{id}")))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(7.0))
                            .bg(t::field())
                            .border_1()
                            .border_color(t::hairline())
                            .text_size(px(11.5))
                            .text_color(t::text_dim())
                            .child(id.clone())
                            .child(
                                div()
                                    .id(SharedString::from(format!("custom-remove-{id}")))
                                    .cursor_pointer()
                                    .text_color(t::danger())
                                    .hover(|style| style.text_color(t::text()))
                                    .child("×")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        let shared = Rc::clone(&view.shared);
                                        let remaining: Vec<String> =
                                            custom_sources(&view.state()).into_iter().filter(|kept| *kept != owned).collect();
                                        let text = remaining.join("; ");
                                        let _ = send(&shared, "customSources", &text, cx);
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                rows.push(
                    card("row-custom-list", "已添加的自定义程序", "这些程序不在内置列表里，但同样可以唤出灵动岛。")
                        .child(list)
                        .into_any_element(),
                );
            }
            rows.push(toggle_row(
                "OnlineSongInfo",
                "在线补全网易云歌曲信息",
                "本地信息缺失时直连 music.163.com 补全封面和歌名，不发送账号或令牌。",
                flag(state, "OnlineSongInfo", true),
                view,
            ));
            rows.push(info_row(
                "当前播放信息",
                text(state, "player", "等待播放器"),
                &format!("{}\n{}", text(state, "title", "暂无播放"), text(state, "artist", "")),
            ));
            rows
        }
        4 => vec![
            info_row("更新来源", "Gitee 国内源 + GitHub", "同时检查两个源；一个不可用时使用另一个。下载校验完成后才替换原程序。"),
            action_row("openGitee", "国内发布页", "点屿的 Gitee 国内镜像与下载。", "Gitee", view, cx),
            action_row("checkUpdate", "在线更新", "同时请求国内源和 GitHub，检查最新正式版本。", "检查更新", view, cx),
            action_row("installUpdate", "下载并更新", "应用内下载、校验并更新，保留设置，完成后自动重启。", "下载并更新", view, cx),
            action_row("openGitHub", "项目主页", "查看源码、发行版本与反馈问题。", "GitHub", view, cx),
            info_row("DotIsle · 点屿 1.3.1", "原生胶囊 · GPUI 设置 · Rust 后台",
                     "内置完整 Noto Sans SC 与 Noto Sans JP，均采用 SIL OFL 1.1 许可。"),
            info_row("界面组件", "GPUI 0.2.2",
                     "设置界面使用 GPUI，支持主题切换、即时调色与自动保存。"),
            info_row("开源来源与许可", "按实际使用保留版权和许可声明",
                     "TaskbarLyrics、AF-Media-Bar、NetEase-Cloud-Music-DiscordRPC 的移植来源继续保留。\nLyricify-Lyrics-Helper 的协议移植按 Apache-2.0 保留归属。\n详情见发行目录中的 LICENSE、THIRD-PARTY-NOTICES.md 与 licenses/。"),
        ],
        5 => history::rows(),
        6 => {
            let notifications = &state["notificationState"];
            let mut rows = vec![
                toggle_row("NotificationsEnabled", "通知提醒", "接收当前用户的新通知，无需额外安装通知组件。", flag(state,"NotificationsEnabled",false),view),
                toggle_row("NotificationClickOpenApp", "点击通知跳转应用", "默认开启。点击通知横幅打开消息来源应用；关闭按钮只收起通知。", flag(state,"NotificationClickOpenApp",true),view),
                toggle_row("PauseNotifications", "暂停通知", "暂停期间不弹出通知，恢复后只提醒新消息。", flag(state,"PauseNotifications",false),view),
                toggle_row("NotificationWhitelistEnabled", "启用通知白名单", "默认关闭。开启后展开应用列表，仅提醒选中的应用。", flag(state,"NotificationWhitelistEnabled",false),view),
            ];
            if flag(state,"NotificationWhitelistEnabled",false){
            for (id, title) in [("QQNT", "QQNT"), ("QQ", "QQ（旧版及系统通知）")] {
                let allowed = notification_allowed(state, id);
                rows.push(check_row(
                    SharedString::from(format!("notification-app:{id}")),
                    title,
                    "仅能接收该应用发送到 Windows 通知中心的消息。",
                    allowed,
                    cx,
                ));
            }
            if let Some(apps) = notifications["apps"].as_array() {
                for app in apps {
                    let (Some(id), Some(name)) = (app["id"].as_str(), app["name"].as_str()) else {
                        continue;
                    };
                    if ["QQNT", "QQ"].contains(&id) {
                        continue;
                    }
                    let allowed = notification_allowed(state, id);
                    rows.push(check_row(
                        SharedString::from(format!("notification-app:{id}")),
                        name,
                        id,
                        allowed,
                        cx,
                    ));
                }
            }
            rows.push(
                card("row-notification-add", "其他通知应用白名单", "填写完整应用标识（AUMID），多个以分号分隔。也可从已出现的通知应用中勾选。")
                    .child(div().w(px(300.0)).child(view.notification_apps.clone()))
                    .into_any_element(),
            );
            }
            rows.push(info_row("通知接收状态",notifications["access"].as_str().unwrap_or("未开启"),notifications["message"].as_str().unwrap_or("请开启通知提醒。")));
            rows.push(action_row("notificationPreview","预览通知横幅","显示本地示例，预览通知外观。","预览",view,cx));

            rows
        }
        7 => new_todo_rows(state, view, cx),
        _ => Vec::new(),
    }
}

/// Whether a built-in player id is currently allowed.
fn source_allowed(state: &Value, id: &str) -> bool {
    setting(state, "AllowedSources")
        .as_array()
        .map(|list| list.iter().any(|entry| entry.as_str().is_some_and(|entry| entry.eq_ignore_ascii_case(id))))
        .unwrap_or(matches!(id, "Netease" | "QQMusic" | "Kugou" | "Kuwo" | "Spotify" | "TwilightEcho"))
}

/// Custom (non built-in) entries from the source whitelist.
fn custom_sources(state: &Value) -> Vec<String> {
    setting(state, "AllowedSources")
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter(|id| !pipe::PLAYERS.iter().any(|(known, _)| known.eq_ignore_ascii_case(id)))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn notification_allowed(state: &Value, id: &str) -> bool {
    setting(state, "NotificationAllowedApps")
        .as_array()
        .map(|list| list.iter().any(|entry| entry.as_str().is_some_and(|entry| entry.eq_ignore_ascii_case(id))))
        .unwrap_or(matches!(id, "QQNT" | "QQ"))
}

/// A row whose whole surface toggles a membership (players, notification apps).
fn check_row(id: SharedString, title: &str, detail: &str, checked: bool, cx: &mut Context<SettingsApp>) -> AnyElement {
    let key = id.to_string();
    card(id, title, detail)
        .cursor_pointer()
        .hover(|style| style.bg(t::surface_hi()))
        .active(|style| style.bg(t::surface_hi()).opacity(0.9))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(22.0))
                .h(px(22.0))
                .rounded(px(11.0))
                .bg(if checked { t::accent() } else { t::track() })
                .text_size(px(12.0))
                .text_color(if checked { t::on_accent() } else { t::text_faint() })
                .child(if checked { "✓" } else { "" }),
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            let shared = Rc::clone(&view.shared);
            let _ = send(&shared, &key, if checked { "false" } else { "true" }, cx);
            cx.notify();
        }))
        .into_any_element()
}

/// A row whose button runs a local closure instead of a pipe command.
fn button_row(
    id: &'static str,
    title: &str,
    detail: &str,
    label: &str,
    cx: &mut Context<SettingsApp>,
    action: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
) -> AnyElement {
    card(SharedString::from(id), title, detail)
        .child(
            div()
                .id(SharedString::from(format!("run-{id}")))
                .px_3()
                .py_2()
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(t::surface_hi())
                .border_1()
                .border_color(t::hairline())
                .text_size(px(12.0))
                .text_color(t::text())
                .hover(|style| style.bg(t::accent()).text_color(t::on_accent()))
                .active(|style| style.opacity(0.85))
                .child(label.to_owned())
                .on_click(cx.listener(move |view, _, _, cx| {
                    action(view, cx);
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// A small secondary button used inside todo rows and the todo form.
fn small_button(
    id: SharedString,
    label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded(px(7.0))
        .cursor_pointer()
        .bg(t::field())
        .border_1()
        .border_color(t::hairline())
        .text_size(px(11.5))
        .text_color(t::text_dim())
        .hover(|style| style.bg(t::surface_hi()).text_color(t::text()))
        .active(|style| style.opacity(0.85))
        .child(label.to_owned())
        .on_click(on_click)
}

/// A labelled text field inside the todo form.
fn field_line(label: &str, width: f32, field: Entity<TextField>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .w(px(width))
        .child(div().text_size(px(11.0)).text_color(t::text_faint()).child(label.to_owned()))
        .child(field)
}

/// A labelled segmented control, used for the chime choice.
fn segmented_row(
    id_prefix: &'static str,
    key: &'static str,
    title: &str,
    detail: &str,
    choices: &'static [(&'static str, &'static str)],
    current: &str,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let mut segments = div().flex().flex_row().gap_1().p_1().rounded(px(9.0)).bg(t::field());
    for (value, label) in choices {
        let selected = current == *value;
        segments = segments.child(
            div()
                .id(SharedString::from(format!("{id_prefix}-{value}")))
                .px_3()
                .py_1()
                .rounded(px(7.0))
                .cursor_pointer()
                .bg(if selected { t::accent() } else { t::field() })
                .text_size(px(12.0))
                .text_color(if selected { t::on_accent() } else { t::text_dim() })
                .hover(|style| style.bg(if selected { t::accent() } else { t::surface_hi() }))
                .active(|style| style.opacity(0.85))
                .child((*label).to_owned())
                .on_click(cx.listener(move |view, _, _, cx| {
                    let shared = Rc::clone(&view.shared);
                    let _ = send(&shared, key, value, cx);
                    cx.notify();
                })),
        );
    }
    card(SharedString::from(format!("row-{key}")), title, detail)
        .child(segments)
        .into_any_element()
}

/// Audition button. Plays through the same worker the banner uses, but never
/// reads or fabricates a real notification.
fn preview_button(prefix: &'static str, cx: &mut Context<SettingsApp>) -> AnyElement {
    small_button(
        SharedString::from(format!("preview-{prefix}")),
        "试听",
        cx.listener(move |view, _, _, cx| {
            audition(Rc::clone(&view.shared), prefix, cx);
            cx.notify();
        }),
    )
    .into_any_element()
}

/// Play one chime through the pipe. Kept as a free function because the async
/// continuation needs `App`, not a view context.
fn audition(shared: Rc<Mutex<Shared>>, prefix: &'static str, cx: &mut App) {
    let job = cx.background_executor().spawn(async move {if prefix=="Device"{pipe::request("deviceSoundPreview",Value::Null)}else{pipe::sound_preview(prefix=="Todo")}});
    cx.spawn(async move |cx| {
        let outcome = job.await;
        if let Ok(mut shared) = shared.lock() {
            shared.message = match outcome {
                Ok(reply) if reply["success"].as_bool() == Some(true) => String::new(),
                Ok(reply) => reply["message"].as_str().unwrap_or("试听失败").to_owned(),
                Err(error) => error,
            };
        }
        let _ = cx.update(|cx| cx.refresh_windows());
    })
    .detach();
}

/// Ask for a local PCM WAV and hand the chosen path to the native side, which
/// validates it before it becomes a setting.
fn pick_sound_file(shared: Rc<Mutex<Shared>>, path_key: &'static str, cx: &mut App) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(SharedString::from("选择 WAV 音效")),
    });
    cx.spawn(async move |cx| {
        let picked = match receiver.await {
            Ok(Ok(Some(paths))) => paths.into_iter().next(),
            _ => None,
        };
        let Some(path) = picked else {
            return;
        };
        let text = path.to_string_lossy().to_string();
        let _ = cx.update(|cx| {
            let _ = send(&shared, path_key, &text, cx);
        });
    })
    .detach();
}

/// The four sound settings shared by notifications and reminders: switch, chime,
/// volume and an optional local PCM WAV. Keys are literals so the wire contract
/// stays visible at the call site.
fn sound_rows(
    prefix: &'static str,
    title: &str,
    detail: &str,
    state: &Value,
    view: &SettingsApp,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let todo=prefix=="Todo";
    let (enabled_key,kind_key,volume_key,path_key)=match prefix{"Todo"=>("TodoSoundEnabled","TodoSound","TodoVolume","TodoSoundPath"),"Device"=>("DeviceSoundEnabled","DeviceSound","DeviceVolume","DeviceSoundPath"),_=>("NotificationSoundEnabled","NotificationSound","NotificationVolume","NotificationSoundPath")};
    let default_kind = if prefix=="Device"{"Connect"}else if todo { "Bell" } else { "Soft" };
    let default_volume = if todo { 45.0 } else { 35.0 };
    let current = setting_str(state, kind_key, default_kind).to_owned();
    let mut rows = vec![
        toggle_row(enabled_key, title, detail, flag(state, enabled_key, true), view),
        segmented_row(
            kind_key,
            kind_key,
            "音色",
            "四段原创短音，可选择内置音色或自己的声音文件。",
            &[("Soft", "轻柔"), ("Glass", "清脆"), ("Bell", "铃音"), ("Connect","连接")],
            &current,
            cx,
        ),
        slider_row(
            volume_key,
            "音量",
            "0 为静音。只影响点屿自己播放的音效，不改变系统音量。",
            0.0,
            100.0,
            "%",
            slider_value(state, volume_key, view, default_volume),
            view,
            cx,
        ),
        sound_path_row(path_key, state, cx),
    ];
    rows.push(
        card(SharedString::from(format!("row-preview-{prefix}")), "试听音效", "播放当前音色与音量，不会产生或读取任何通知。")
            .child(preview_button(prefix, cx))
            .into_any_element(),
    );
    rows
}

/// Pick or clear a custom PCM WAV for one sound slot.
fn sound_path_row(
    path_key: &'static str,
    state: &Value,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let current = setting_str(state, path_key, "");
    let label = if current.is_empty() {
        "使用内置音色".to_owned()
    } else {
        Path::new(current).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| current.to_owned())
    };
    let clear = !current.is_empty();
    card(SharedString::from(format!("row-{path_key}")), "自定义音效", "选择本机的 16 位 PCM WAV（8–48 kHz、10 秒以内、不超过 8 MB）。")
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().text_size(px(11.5)).text_color(t::text_dim()).child(label))
                .child(small_button(
                    SharedString::from(format!("pick-{path_key}")),
                    "选择文件",
                    cx.listener(move |view, _, _, cx| {
                        pick_sound_file(Rc::clone(&view.shared), path_key, cx);
                        cx.notify();
                    }),
                ))
                .child(small_button(
                    SharedString::from(format!("clear-{path_key}")),
                    "清除",
                    cx.listener(move |view, _, _, cx| {
                        if !clear {
                            return;
                        }
                        let shared = Rc::clone(&view.shared);
                        let _ = send(&shared, path_key, "", cx);
                        cx.notify();
                    }),
                )),
        )
        .into_any_element()
}

/// The add / edit form. Fields are entities owned by the settings view, so
/// switching pages or re-rendering never costs the user their typing.
fn todo_form(view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let editing = view.todo_editing.is_some();
    let daily = view.todo_daily;
    div()
        .id("row-todo-form")
        .flex()
        .flex_col()
        .gap_3()
        .px_4()
        .py_3()
        .rounded(px(12.0))
        .bg(t::surface())
        .border_1()
        .border_color(t::hairline())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(13.5)).text_color(t::text()).child(if editing { "编辑待办" } else { "新建待办" }))
                .child(div().text_size(px(11.5)).text_color(t::text_dim()).child("题目必填。日期使用本地日期，时间使用 24 小时制。按 Enter 或点保存提交。")),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap_3()
                .child(field_line("题目", 220.0, view.todo_title.clone()))
                .child(field_line("内容（可留空）", 300.0, view.todo_content.clone())),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap_3()
                .child(field_line("日期", 140.0, view.todo_date.clone()))
                .child(field_line("时间", 120.0, view.todo_time.clone()))
                .child(
                    div()
                        .id("todo-daily")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .rounded(px(8.0))
                        .cursor_pointer()
                        .bg(t::field())
                        .border_1()
                        .border_color(t::hairline())
                        .hover(|style| style.bg(t::surface_hi()))
                        .child(switch(daily))
                        .child(div().text_size(px(12.0)).text_color(t::text_dim()).child("每日重复"))
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.todo_daily = !view.todo_daily;
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(small_button(
                    "todo-save".into(),
                    if editing { "保存修改" } else { "保存" },
                    cx.listener(|view, _, _, cx| view.submit_todo(cx)),
                ))
                .child(small_button(
                    "todo-cancel".into(),
                    "取消",
                    cx.listener(|view, _, _, cx| view.close_todo_form(cx)),
                )),
        )
        .child(palette_row(view,cx))
        .into_any_element()
}

/// Send a todo / sound command and fold the reply back, mirroring `send`.
fn send_todo(shared: &Rc<Mutex<Shared>>, command: &'static str, value: Value, cx: &mut App) {
    let job = cx.background_executor().spawn(async move { pipe::command(command, value) });
    let shared = Rc::clone(shared);
    cx.spawn(async move |cx| {
        let outcome = job.await;
        if let Ok(mut shared) = shared.lock() {
            match outcome {
                Ok((reply, state)) => {
                    shared.state = state;
                    shared.connection_error = false;
                    shared.message_is_connection_error = false;
                    let success = reply["success"].as_bool() == Some(true);
                    shared.message = if success { String::new() } else { reply["message"].as_str().unwrap_or("待办操作失败").to_owned() };
                    shared.notice = reply["message"].as_str().unwrap_or(if success { "已保存" } else { "待办操作失败" }).to_owned();
                }
                Err(error) => {
                    shared.message = error;
                    shared.connection_error = true;
                    shared.message_is_connection_error = true;
                }
            }
        }
        let _ = cx.update(|cx| cx.refresh_windows());
    })
    .detach();
}

/// A row that runs a command when pressed, with the row title as the button label.
fn action_row(
    command: &'static str,
    title: &str,
    detail: &str,
    label: &str,
    view: &SettingsApp,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let shared = Rc::clone(&view.shared);
    card(SharedString::from(format!("action-{command}")), title, detail)
        .child(
            div()
                .id(SharedString::from(format!("run-{command}")))
                .px_3()
                .py_2()
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(t::surface_hi())
                .border_1()
                .border_color(t::hairline())
                .text_size(px(12.0))
                .text_color(t::text())
                .hover(|style| style.bg(t::accent()).text_color(t::on_accent()))
                .active(|style| style.opacity(0.85))
                .child(label.to_owned())
                .on_click(cx.listener(move |view2, _, _, cx| {
                    let _ = view2;
                    let _ = send(&shared, command, "", cx);
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// Playback-position calibration: type the seconds the player shows, or nudge with the steppers.
fn calibrate_row(position: f64, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let mut stepper = div().flex().flex_row().items_center().gap_2();
    for step in [-5.0_f64, -1.0, 1.0, 5.0] {
        stepper = stepper.child(
            div()
                .id(SharedString::from(format!("calibrate-{step}")))
                .w(px(38.0))
                .py_1()
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(t::field())
                .border_1()
                .border_color(t::hairline())
                .text_size(px(12.0))
                .text_color(t::text_dim())
                .flex()
                .justify_center()
                .hover(|style| style.bg(t::surface_hi()).text_color(t::text()))
                .active(|style| style.opacity(0.85))
                .child(if step > 0.0 { format!("+{step:.0}") } else { format!("{step:.0}") })
                .on_click(cx.listener(move |view, _, _, cx| {
                    let current = view.state()["position"].as_f64().unwrap_or(0.0);
                    let next = (current + step).clamp(0.0, 86400.0);
                    let shared = Rc::clone(&view.shared);
                    let _ = send(&shared, "synchronize", &format!("{next:.0}"), cx);
                    cx.notify();
                })),
        );
    }
    card("row-calibrate", "播放位置校准", "输入播放器上显示的秒数并按 Enter，或用按钮微调。备用本地计时时使用。")
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(div().text_size(px(12.0)).text_color(t::accent()).child(format!("{position:.0} 秒")))
                .child(div().w(px(228.0)).child(view.calibrate.clone()))
                .child(stepper),
        )
        .into_any_element()
}

fn card(id: impl Into<ElementId>, title: &str, detail: &str) -> Stateful<Div> {
    let mut body = div().flex().flex_col().gap_1()
        .child(div().text_size(px(13.5)).text_color(t::text()).child(title.to_owned()));
    if !detail.is_empty() {
        body = body.child(div().text_size(px(11.5)).text_color(t::text_dim()).child(detail.to_owned()));
    }
    div().id(id).flex().flex_row().items_center().justify_between().gap_4()
        .px_4().py_3().rounded(px(12.0)).bg(t::surface()).border_1().border_color(t::hairline())
        .child(body)
}

/// A draggable value row.
///
/// The track follows the pointer one to one, the read-out updates continuously, and the value only
/// reaches the native side when the gesture ends — a spring setting the user is still tuning should
/// not be written on every frame.
fn slider_row(
    key: &'static str,
    title: &str,
    detail: &str,
    min: f64,
    max: f64,
    unit: &str,
    value: f64,
    view: &SettingsApp,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let bounds: Rc<Cell<Bounds<Pixels>>> =
        Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0)))));
    let span = if max > min { max - min } else { 1.0 };
    let fraction = ((value - min) / span).clamp(0.0, 1.0) as f32;
    let filled = t::accent();
    let empty = t::track();
    let knob = t::knob();

    let cell = Rc::clone(&bounds);
    let watched = smoke_slider() == Some(key);
    let smoke_cell = Rc::clone(&view.smoke_bounds);
    let track = canvas(
        move |painted, _window, _cx| {
            cell.set(painted);
            if watched {
                smoke_cell.set(painted);
            }
        },
        move |painted, _state, window, _cx| {
            let thickness = px(4.0);
            let line = Bounds::new(
                point(painted.origin.x, painted.origin.y + (painted.size.height - thickness) / 2.0),
                size(painted.size.width, thickness),
            );
            window.paint_quad(quad(line, px(2.0), empty, px(0.0), transparent_black(), BorderStyle::Solid));
            window.paint_quad(quad(
                Bounds::new(line.origin, size(painted.size.width * fraction, thickness)),
                px(2.0),
                filled,
                px(0.0),
                transparent_black(),
                BorderStyle::Solid,
            ));
            window.paint_quad(quad(
                Bounds::new(
                    point(painted.origin.x + painted.size.width * fraction - px(7.0), painted.origin.y + painted.size.height / 2.0 - px(7.0)),
                    size(px(14.0), px(14.0)),
                ),
                px(7.0),
                knob,
                px(1.0),
                t::hairline_bright(),
                BorderStyle::Solid,
            ));
        },
    )
    .size_full();

    let readout = format_value(value, min, max, unit);
    let dragging = view.slider.is_some_and(|drag| drag.key == key);
    card(SharedString::from(format!("row-{key}")), title, detail)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .w(px(268.0))
                .child(
                    div()
                        .id(SharedString::from(format!("slider-{key}")))
                        .flex_1()
                        .h(px(24.0))
                        .cursor_pointer()
                        .child(track)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            view.slider_begin(key, min, max, bounds.get(), event.position);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(px(11.5))
                        .text_color(if dragging { t::accent() } else { t::text_dim() })
                        .child(readout),
                ),
        )
        .on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
            if view.slider.is_some_and(|drag| drag.key == key && drag.bounds.size.width > px(0.0)) {
                view.slider_move(event.position);
                cx.notify();
            }
        }))
        .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {
            view.slider_commit(cx);
            cx.notify();
        }))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {
            view.slider_commit(cx);
            cx.notify();
        }))
        .into_any_element()
}

/// Current value of a slider: the live drag value while this row is being dragged, otherwise what
/// is stored. Motion settings are addressed as `MotionGap` / `SpringStiffness` on the wire but live
/// at `Motion.Gap` / `Motion.Spring.Stiffness`.
fn slider_value(state: &Value, key: &str, view: &SettingsApp, default: f64) -> f64 {
    if let Some(drag) = view.slider.filter(|drag| drag.key == key) {
        return drag.value;
    }
    let motion = setting(state, "Motion");
    let stored: Option<&Value> = if let Some(name) = key.strip_prefix("Spring") {
        motion.get("Spring").and_then(|spring| spring.get(name))
    } else if let Some(name) = key.strip_prefix("Motion") {
        motion.get(name)
    } else {
        Some(setting(state, key)).filter(|value| !value.is_null())
    };
    stored.and_then(Value::as_f64).unwrap_or(default)
}

/// Apple-style segmented control for the appearance preference: follow Windows, or pin it.
fn theme_row(state: &Value, cx: &mut Context<SettingsApp>) -> AnyElement {
    let current = setting_str(state, "SettingsTheme", "System").to_owned();
    let mut segments = div().flex().flex_row().gap_1().p_1().rounded(px(9.0)).bg(t::field());
    for (value, label) in [("System", "跟随系统"), ("Light", "浅色"), ("Dark", "深色")] {
        let selected = current == value;
        segments = segments.child(
            div()
                .id(SharedString::from(format!("theme-{value}")))
                .px_3()
                .py_1()
                .rounded(px(7.0))
                .cursor_pointer()
                .bg(if selected { t::accent() } else { t::field() })
                .text_size(px(12.0))
                .text_color(if selected { t::on_accent() } else { t::text_dim() })
                .hover(|style| style.bg(if selected { t::accent() } else { t::surface_hi() }))
                .active(|style| style.opacity(0.85))
                .child(label.to_owned())
                .on_click(cx.listener(move |view, _, _, cx| {
                    let shared = Rc::clone(&view.shared);
                    let _ = send(&shared, "SettingsTheme", value, cx);
                    cx.notify();
                })),
        );
    }
    card("row-theme", "外观", "跟随 Windows，或固定为浅色 / 深色。选择会随其他设置一起保存。")
        .child(segments)
        .into_any_element()
}

fn toggle_row(key: &'static str, title: &str, detail: &str, checked: bool, view: &SettingsApp) -> AnyElement {
    let shared = Rc::clone(&view.shared);
    card(SharedString::from(format!("row-{key}")), title, detail)
        .cursor_pointer()
        .hover(|style| style.bg(t::surface_hi()))
        .child(switch(checked))
        .on_click(move |_, _, cx| {
            let _ = send(&shared, key, if checked { "false" } else { "true" }, cx);
        })
        .into_any_element()
}

fn default_color(key: &str) -> &'static str {
    match key {
        "ExpandedComponentColor" => "#B1E8D1",
        "LyricProgressColor" => "#A9E6CD",
        "TodoDefaultColor"=>"#F3D79B",
        _ => "#F4F6F8",
    }
}

fn color_row(key: &'static str, title: &str, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let current = setting(state, key).as_str().map(str::to_owned).unwrap_or_else(|| default_color(key).to_owned());
    let open = view.picker.as_ref().is_some_and(|picker| picker.key == key);
    let opening = current.clone();
    card(SharedString::from(format!("row-{key}")), title, "点击打开调色盘；拖动色相与明暗即可取色。")
        .cursor_pointer()
        .hover(|style| style.bg(t::surface_hi()))
        // Feedback lands on the press, not on release.
        .active(|style| style.bg(t::surface_hi()).opacity(0.9))
        .child(
            div().flex().items_center().gap_2()
                .child(div().text_size(px(11.5)).text_color(t::text_dim()).child(current.clone()))
                .child(
                    div().w(px(26.0)).h(px(26.0)).rounded(px(7.0)).bg(t::parse(&current))
                        .border_1().border_color(if open { t::accent() } else { t::hairline_bright() }),
                ),
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            let already_open = view.picker.as_ref().is_some_and(|picker| picker.key == key);
            view.picker = (!already_open).then(|| Picker { key, hsv: Hsv::from_rgb(t::parse(&opening)), drag: Drag::None });
            view.hex_field=(!already_open).then(||make_hex_field(&opening,cx));
            cx.notify();
        }))
        .into_any_element()
}

fn make_hex_field(initial:&str,cx:&mut Context<SettingsApp>)->Entity<TextField>{
    let weak=cx.entity().downgrade();
    cx.new(|cx|TextField::new("field-hex","#RRGGBB，Enter 应用",initial,Rc::new(move |value,app|{
        let _=weak.update(app,|view,cx|{view.apply_hex(value,cx);});
    }),cx))
}
impl SettingsApp {
    fn apply_hex(&mut self,value:&str,cx:&mut Context<Self>)->bool{
        let Some(hex)=color::normalize_hex(value)else{
            if let Ok(mut s)=self.shared.lock(){s.message="色号格式应为 #RRGGBB，例如 #A9E6CD".into();}cx.notify();return false;
        };
        if let Some(p)=self.picker.as_mut(){p.hsv=Hsv::from_rgb(t::parse(&hex));p.drag=Drag::None;}else{return false}
        self.commit(cx);cx.notify();true
    }
}

const PAD_WIDTH: f32 = 268.0;
const PAD_HEIGHT: f32 = 150.0;
const HUE_HEIGHT: f32 = 16.0;
const KNOB: f32 = 15.0;

/// Quick choices: the product's own palette first, then neutrals.
const PRESETS: [&str; 10] = [
    "#F4F6F8", "#B1E8D1", "#A9E6CD", "#7FD1B0", "#4FB894", "#F5C86B", "#F08A7A", "#8AB4F8", "#C792EA", "#FFFFFF",
];

/// The colour picker: a saturation/value pad, a hue strip, a preview, presets and the hex value.
///
/// Direct manipulation first — the pad and strip track the pointer one to one, the value is visible
/// the whole time, and the edit only reaches the native side when the gesture ends.
fn picker_panel(view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let Some(picker) = view.picker.as_ref() else {
        return div().into_any_element();
    };
    let hsv = picker.hsv;
    let hex = t::to_hex(hsv.to_rgb());

    let pad_cell = Rc::clone(&view.pad_bounds);
    let pad_canvas = canvas(
        move |bounds, _window, _cx| pad_cell.set(bounds),
        move |bounds, _state, window, _cx| {
            // Hue base, then white towards the right, then black towards the bottom.
            window.paint_quad(fill(bounds, Hsv::new(hsv.h, 1.0, 1.0).to_rgb()));
            window.paint_quad(fill(
                bounds,
                linear_gradient(90.0, linear_color_stop(gpui::white(), 0.0), linear_color_stop(gpui::white().opacity(0.0), 1.0)),
            ));
            window.paint_quad(fill(
                bounds,
                linear_gradient(180.0, linear_color_stop(gpui::black().opacity(0.0), 0.0), linear_color_stop(gpui::black(), 1.0)),
            ));
        },
    )
    // A canvas carries no intrinsic size, so without this it collapses to nothing and paints no
    // gradients at all (and its recorded bounds would be zero, breaking the drag mapping).
    .size_full();
    // Interactive elements need a stable id: gpui keys hit-testing and hover/active state off it,
    // and without one the pad never receives a pointer event at all.
    let pad = div()
        .id("picker-pad")
        .relative()
        .w(px(PAD_WIDTH))
        .h(px(PAD_HEIGHT))
        .rounded(px(10.0))
        .overflow_hidden()
        .border_1()
        .border_color(t::hairline_bright())
        .child(pad_canvas)
        .child(
            div()
                .absolute()
                .left(px(hsv.s * PAD_WIDTH - KNOB / 2.0))
                .top(px((1.0 - hsv.v) * PAD_HEIGHT - KNOB / 2.0))
                .w(px(KNOB))
                .h(px(KNOB))
                .rounded(px(KNOB / 2.0))
                .bg(hsv.to_rgb())
                .border_2()
                .border_color(gpui::white()),
        )
        .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &MouseDownEvent, _, cx| {
            view.begin_drag(Drag::Pad, event.position);
            cx.notify();
        }));

    let hue_cell = Rc::clone(&view.hue_bounds);
    let hue_canvas = canvas(
        move |bounds, _window, _cx| hue_cell.set(bounds),
        move |bounds, _state, window, _cx| {
            // Two-stop gradients cannot express a full hue wheel, so draw the six sectors and let
            // each one interpolate.
            let sectors = 6;
            for index in 0..sectors {
                let from = index as f32 / sectors as f32;
                let to = (index + 1) as f32 / sectors as f32;
                let segment = Bounds::new(
                    point(bounds.origin.x + bounds.size.width * from, bounds.origin.y),
                    size(bounds.size.width / sectors as f32 + px(0.5), bounds.size.height),
                );
                window.paint_quad(fill(
                    segment,
                    linear_gradient(
                        90.0,
                        linear_color_stop(Hsv::new(from, 1.0, 1.0).to_rgb(), 0.0),
                        linear_color_stop(Hsv::new(to, 1.0, 1.0).to_rgb(), 1.0),
                    ),
                ));
            }
        },
    )
    .size_full();
    let hue = div()
        .id("picker-hue")
        .relative()
        .w(px(PAD_WIDTH))
        .h(px(HUE_HEIGHT))
        .rounded(px(HUE_HEIGHT / 2.0))
        .overflow_hidden()
        .child(hue_canvas)
        .child(
            div()
                .absolute()
                .left(px(hsv.h * PAD_WIDTH - KNOB / 2.0))
                .top(px(-(KNOB - HUE_HEIGHT) / 2.0))
                .w(px(KNOB))
                .h(px(KNOB))
                .rounded(px(KNOB / 2.0))
                .bg(Hsv::new(hsv.h, 1.0, 1.0).to_rgb())
                .border_2()
                .border_color(gpui::white()),
        )
        .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &MouseDownEvent, _, cx| {
            view.begin_drag(Drag::Hue, event.position);
            cx.notify();
        }));

    let mut presets = div().flex().flex_row().gap_2();
    for preset in PRESETS {
        presets = presets.child(
            div()
                .id(SharedString::from(format!("preset-{preset}")))
                .w(px(22.0))
                .h(px(22.0))
                .rounded(px(6.0))
                .bg(t::parse(preset))
                .border_1()
                .border_color(t::hairline_bright())
                .cursor_pointer()
                .hover(|style| style.border_color(gpui::white()))
                .active(|style| style.opacity(0.75))
                .on_click(cx.listener(move |view, _, _, cx| {
                    if let Some(picker) = view.picker.as_mut() {
                        picker.hsv = Hsv::from_rgb(t::parse(preset));
                    }
                    view.commit(cx);
                    cx.notify();
                })),
        );
    }

    let preview = div()
        .w(px(72.0))
        .h(px(44.0))
        .rounded(px(10.0))
        .bg(hsv.to_rgb())
        .border_1()
        .border_color(t::hairline_bright());

    div()
        .id("picker")
        .flex()
        .flex_row()
        .gap_5()
        .p_4()
        .rounded(px(14.0))
        .bg(t::field())
        .border_1()
        .border_color(t::hairline())
        // A drag may leave the pad; tracking continues on the panel and the value is clamped.
        .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
            if event.pressed_button != Some(MouseButton::Left) {view.end_drag(cx);return;}
            if view.picker.as_ref().is_some_and(|picker| picker.drag != Drag::None) {
                view.drag_to(event.position);
                cx.notify();
            }
        }))
        .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {
            view.end_drag(cx);
            cx.notify();
        }))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {view.end_drag(cx);cx.notify();}))
        .child(
            div().flex().flex_col().gap_3()
                .child(pad)
                .child(hue)
                .child(
                    div().flex().flex_row().items_center().gap_3()
                        .child(div().text_size(px(11.5)).text_color(t::text_dim()).child("色板"))
                        .child(presets),
                ),
        )
        .child(
            div().flex().flex_col().gap_3().w(px(168.0))
                .child(preview)
                .child(view.hex_field.clone().map(|field|field.into_any_element()).unwrap_or_else(||div().child(hex.clone()).into_any_element()))
                .child(div().text_size(px(11.0)).text_color(t::text_faint()).child("拖动圆点取色，松手即保存。"))
                .child(div().flex_1())
                .child(
                    div().flex().flex_row().gap_2()
                        .child(
                            div().id("picker-default").px_3().py_2().rounded(px(8.0)).cursor_pointer()
                                .bg(t::surface()).border_1().border_color(t::hairline())
                                .text_size(px(12.0)).text_color(t::text_dim())
                                .hover(|style| style.bg(t::surface_hi()))
                                .active(|style| style.opacity(0.8))
                                .child("恢复默认")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    let key = view.picker.as_ref().map(|picker| picker.key);
                                    if let (Some(key), Some(picker)) = (key, view.picker.as_mut()) {
                                        picker.hsv = Hsv::from_rgb(t::parse(default_color(key)));
                                    }
                                    view.commit(cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            div().id("picker-done").px_3().py_2().rounded(px(8.0)).cursor_pointer()
                                .bg(t::accent_dim()).border_1().border_color(t::accent())
                                .text_size(px(12.0)).text_color(t::accent())
                                .hover(|style| style.bg(t::accent()).text_color(t::base()))
                                .active(|style| style.opacity(0.85))
                                .child("完成")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.picker = None;
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .into_any_element()
}

impl SettingsApp {
    fn slider_begin(&mut self, key: &'static str, min: f64, max: f64, bounds: Bounds<Pixels>, position: Point<Pixels>) {
        self.slider = Some(SliderDrag { key, min, max, bounds, value: min });
        self.slider_move(position);
        log("SLIDER", &format!("begin {key} at {}", f32::from(position.x)));
    }

    /// Map the pointer onto the track. Clamped, so dragging past either end pins to the limit.
    fn slider_move(&mut self, position: Point<Pixels>) {
        let Some(mut drag) = self.slider else {
            return;
        };
        drag.value = position_to_value(
            drag.min,
            drag.max,
            f32::from(drag.bounds.origin.x),
            f32::from(drag.bounds.size.width),
            f32::from(position.x),
        );
        self.slider = Some(drag);
    }

    fn slider_commit(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.slider.take() else {
            return;
        };
        let shared = Rc::clone(&self.shared);
        let text = format!("{}", drag.value);
        let outcome = send(&shared, drag.key, &text, cx);
        log("SLIDER", &format!("commit {}={text} -> {outcome:?}", drag.key));
    }

    fn begin_drag(&mut self, drag: Drag, position: Point<Pixels>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.drag = drag;
        }
        self.drag_to(position);
        log("PICKER", &format!("begin drag at ({}, {})", f32::from(position.x), f32::from(position.y)));
    }

    /// Map a window position onto the pad or the strip, clamped so the knob cannot leave the track.
    fn drag_to(&mut self, position: Point<Pixels>) {
        let pad = self.pad_bounds.get();
        let hue = self.hue_bounds.get();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match picker.drag {
            Drag::Pad => {
                if f32::from(pad.size.width) <= 0.0 {
                    return;
                }
                picker.hsv.s = (f32::from(position.x - pad.origin.x) / f32::from(pad.size.width)).clamp(0.0, 1.0);
                picker.hsv.v = 1.0 - (f32::from(position.y - pad.origin.y) / f32::from(pad.size.height)).clamp(0.0, 1.0);
            }
            Drag::Hue => {
                if f32::from(hue.size.width) <= 0.0 {
                    return;
                }
                picker.hsv.h = (f32::from(position.x - hue.origin.x) / f32::from(hue.size.width)).clamp(0.0, 1.0);
            }
            Drag::None => {}
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if !self.picker.as_ref().is_some_and(|picker| picker.drag != Drag::None) {
            return;
        }
        if let Some(picker) = self.picker.as_mut() {
            picker.drag = Drag::None;
        }
        self.commit(cx);
    }

    /// Send the current colour to the native side.
    fn commit(&mut self, cx: &mut Context<Self>) {
        let Some((key, hex)) = self.picker.as_ref().map(|picker| (picker.key, t::to_hex(picker.hsv.to_rgb()))) else {
            return;
        };
        if let Some(field)=&self.hex_field {field.update(cx,|field,cx|field.set_text(&hex,cx));}
        let shared = Rc::clone(&self.shared);
        let outcome = send(&shared, key, &hex, cx);
        log("PICKER", &format!("commit {key}={hex} -> {outcome:?}"));
    }
}

fn info_row(title: &str, detail: &str, body: &str) -> AnyElement {
    div().px_4().py_3().rounded(px(12.0)).bg(t::surface()).border_1().border_color(t::hairline())
        .child(div().text_size(px(13.5)).text_color(t::text()).child(title.to_owned()))
        .child(div().mt_1().text_size(px(11.5)).text_color(t::text_dim()).child(detail.to_owned()))
        .child(div().mt_3().text_size(px(12.0)).text_color(t::text_dim()).child(body.to_owned()))
        .into_any_element()
}

/// The switch: a capsule track with a knob. Two nested rounded rectangles, no image assets.
fn switch(checked: bool) -> impl IntoElement {
    let (track, knob) = if checked { (t::accent(), t::base()) } else { (t::track(), t::knob()) };
    div().flex().items_center().w(px(42.0)).h(px(24.0)).rounded(px(12.0)).bg(track)
        .child(
            div().ml(px(if checked { 20.0 } else { 2.0 }))
                .w(px(20.0)).h(px(20.0)).rounded(px(10.0)).bg(knob),
        )
}

/// Send one edit and fold the reply back into the shared state.
fn send(shared: &Rc<Mutex<Shared>>, key: &str, value: &str, cx: &mut App) -> Result<(), String> {
    if key=="openGitee" {cx.open_url("https://gitee.com/heiamer/abc");return Ok(())}
    if key=="openGitHub" {cx.open_url("https://github.com/Heiamer/LyricIsland");return Ok(())}
    if key=="checkUpdate"||key=="installUpdate" {
        let install=key=="installUpdate";
        if let Ok(mut s)=shared.lock(){s.message="正在同时检查 Gitee 国内源和 GitHub…".into();}
        let shared=Rc::clone(shared);let job=cx.background_executor().spawn(async move {if install{lyric_backend::updates::install()}else{lyric_backend::updates::check()}});
        cx.spawn(async move |cx|{let result=job.await;if let Ok(mut s)=shared.lock(){s.message=match result{Ok(v)=>v["message"].as_str().unwrap_or("检查完成").to_owned(),Err(e)=>e};}let _=cx.update(|cx|cx.refresh_windows());}).detach();return Ok(())
    }
    let key=key.to_owned();let value=value.to_owned();
    let job=cx.background_executor().spawn(async move {pipe::edit(&key,&value)});
    let shared=Rc::clone(shared);
    cx.spawn(async move |cx| {
        let outcome=job.await;
        if let Ok(mut shared)=shared.lock(){
            match outcome{
                Ok((reply,state))=>{
                    shared.state=state;shared.connection_error=false;shared.message_is_connection_error=false;
                    let success=reply["success"].as_bool()==Some(true);
                    shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("设置更新失败").to_owned()};
                    shared.notice=reply["message"].as_str().unwrap_or(if success{"已保存"}else{"设置更新失败"}).to_owned();
                },
                Err(error)=>{shared.message=error;shared.connection_error=true;shared.message_is_connection_error=true;}
            }
        }
        let _=cx.update(|cx| cx.refresh_windows());
    }).detach();
    Ok(())
}

/// Resolve the font directory: an explicit override, then beside the executable (how the release
/// ships it), then the source tree so self-checks run straight from the build directory.
fn font_directory() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("LYRICISLAND_FONTS") {
        return Some(PathBuf::from(explicit));
    }
    let beside = std::env::current_exe().ok()?.parent()?.join("Assets/Fonts");
    if beside.is_dir() {
        return Some(beside);
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Assets/Fonts");
    source.is_dir().then_some(source)
}

fn register_fonts(cx: &mut App) {
    let Some(directory) = font_directory() else {
        return;
    };
    let mut fonts: Vec<Cow<'static, [u8]>> = Vec::new();
    for name in ["NotoSansSC.ttf", "NotoSansJP.ttf"] {
        if let Ok(bytes) = std::fs::read(directory.join(name)) {
            fonts.push(Cow::Owned(bytes));
        }
    }
    if !fonts.is_empty() {
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            log("FONT", &format!("Bundled fonts could not be loaded: {error}"));
        }
        if let Some(path) = std::env::var_os("LYRICISLAND_FONT_REPORT") {
            let names = cx.text_system().all_font_names();
            let report = json!({"chinese": names.iter().any(|name| name == t::FONT), "japanese": names.iter().any(|name| name == "Noto Sans JP"), "family": t::FONT});
            let _ = std::fs::write(path, report.to_string());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // The tray menu runs before any window exists. Its coordinates arrive as physical pixels from
    // a DPI-aware parent, so this process has to be DPI aware too.
    if args.get(1).is_some_and(|a| a == "--tray-menu") {
        lyric_backend::shell::ensure_per_monitor_dpi_awareness();
        let x = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        let y = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
        if let Err(error) = lyric_backend::shell::tray_menu(x, y) {
            log("TRAY", &error);
            std::process::exit(1);
        }
        return;
    }

    let smoke = args.get(1).filter(|a| *a == "--smoke-ui").map(|_| PathBuf::from(&args[2]));
    let initial = pipe::request("state", Value::Null)
        .unwrap_or_else(|error| json!({"success": false, "settings": {}, "error": error}));
    let notice = if initial["success"].as_bool() == Some(true) { "已连接 · 调整会自动保存" } else { "正在连接灵动岛…" };
    let shared = Rc::new(Mutex::new(Shared { state: initial, notice: notice.to_owned(), message: String::new(), connection_error:false,message_is_connection_error:false }));

    // Self-check hook: start with one picker already open so a screenshot can capture it without a
    // human clicking. Only the three real colour settings are accepted.
    let smoke_picker = std::env::var("LYRICISLAND_SMOKE_PICKER").ok().and_then(|requested| {
        let key = ["ExpandedTextColor", "ExpandedComponentColor", "LyricProgressColor"]
            .into_iter()
            .find(|key| *key == requested)?;
        let state = shared.lock().ok()?.state.clone();
        let hex = setting(&state, key).as_str().map(str::to_owned).unwrap_or_else(|| default_color(key).to_owned());
        log("SMOKE", &format!("starting with the {key} picker open ({hex})"));
        Some(Picker { key, hsv: Hsv::from_rgb(t::parse(&hex)), drag: Drag::None })
    });

    Application::new().run(move |cx: &mut App| {
        register_fonts(cx);

        let bounds = Bounds::centered(None, size(px(940.0), px(720.0)), cx);
        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions{title:Some("点屿 · 设置 1.3.1".into()),appears_transparent:true,..Default::default()}),
                ..Default::default()
            },
            |window, cx| {window.set_window_title("点屿 · 设置 1.3.1");cx.new(|cx| SettingsApp::new(Rc::clone(&shared), smoke_picker, cx))},
        ) {
            Ok(window) => window,
            Err(error) => {
                log("GPUI", &error.to_string());
                cx.quit();
                return;
            }
        };

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        // The caption bar is switched to match the theme from `Render`, once the window exists and
        // the stored preference is known.

        match smoke.clone() {
            Some(directory) => run_smoke(cx, window, directory),
            None => {
                poll_pipe(cx, window, Rc::clone(&shared));
                if let Some(path)=std::env::var_os("LYRICISLAND_SMOKE_RECONNECT"){run_reconnect_smoke(cx,window,PathBuf::from(path),Rc::clone(&shared));}
                // Bring the picker into view after the first frame. This doubles as a check that a
                // late update really does repaint — dragging depends on it.
                if smoke_picker.is_some() || smoke_slider().is_some() || std::env::var_os("LYRICISLAND_SMOKE_FIELD").is_some() {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_millis(900)).await;
                        let outcome = window.update(cx, |view, _, cx| {
                            let max = view.scroll.max_offset();
                            // Scroll to the end so the picker under the last colour row is visible.
                            view.scroll.set_offset(point(px(0.0), -max.height));
                            cx.notify();
                            f32::from(max.height)
                        });
                        log("SMOKE", &format!("late scroll: {outcome:?}"));
                        // Publish the painted pad rectangle so an automated drag can aim at it. The
                        // bounds are only final after the frame that applied the scroll above.
                        cx.background_executor().timer(Duration::from_millis(700)).await;
                        let bounds = window.update(cx, |view, _, _| {
                            let pad = view.pad_bounds.get();
                            [
                                f32::from(pad.origin.x),
                                f32::from(pad.origin.y),
                                f32::from(pad.size.width),
                                f32::from(pad.size.height),
                            ]
                        });
                        if let Ok([x, y, width, height]) = bounds {
                            log("SMOKE", &format!("pad bounds: x={x} y={y} w={width} h={height}"));
                        }
                        let slider = window.update(cx, |view, _, _| {
                            let track = view.smoke_bounds.get();
                            [
                                f32::from(track.origin.x),
                                f32::from(track.origin.y),
                                f32::from(track.size.width),
                                f32::from(track.size.height),
                            ]
                        });
                        if let Ok([x, y, width, height]) = slider {
                            if width > 0.0 {
                                log("SMOKE", &format!("slider bounds: x={x} y={y} w={width} h={height}"));
                            }
                        }
                    })
                    .detach();
                }
            }
        }
        cx.activate(true);
    });
}

fn ui_state(state:&Value)->Value {let mut state=state.clone();if let Some(map)=state.as_object_mut(){for key in ["renderFrames"]{map.remove(key);}}state}

/// The 750 ms refresh loop, mirroring the Slint frontend so the native side keeps one client.
fn poll_pipe(cx: &mut App, window: WindowHandle<SettingsApp>, shared: Rc<Mutex<Shared>>) {
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_millis(750)).await;
        let reply = cx.background_executor().spawn(async {
            let value = pipe::request("state", Value::Null)?;
            // Clicking a reminder banner asks this window to show the todo page. Acknowledge it in
            // the same round trip so the native side stops repeating the request.
            if value["pendingPage"].as_str().is_some() {
                let _ = pipe::page_seen();
            }
            Ok::<Value, String>(value)
        }).await;
        let mut jump = None;
        let mut changed=true;
        if let Ok(mut state) = shared.lock() {
            let before=(ui_state(&state.state),state.notice.clone(),state.message.clone(),state.connection_error);
            match reply {
                Ok(value) => {
                    jump = match value["pendingPage"].as_str() {
                        Some("todos") => Some(PAGE_TODOS),Some("timers")=>Some(8),
                        _ => None,
                    };
                    state.state = value;
                    state.notice = "已连接 · 调整会自动保存".into();
                    if state.message_is_connection_error{state.message.clear();state.message_is_connection_error=false;}
                    state.connection_error=false;
                }
                Err(error) => {state.notice=error;state.connection_error=true;},
            }
            changed=before!=(ui_state(&state.state),state.notice.clone(),state.message.clone(),state.connection_error);
        }
        if let Some(target) = jump {
            if window.update(cx, |view, _, cx| {
                if view.page != target { view.page = target; }
                cx.notify();
            }).is_err() { break; }
        }
        if changed && window.update(cx, |_, _, cx| cx.notify()).is_err() {
            break;
        }
    })
    .detach();
}

/// `--smoke-ui <dir>`: walk every page, then report whether the pipe answered.
fn run_smoke(cx: &mut App, window: WindowHandle<SettingsApp>, directory: PathBuf) {
    let _ = std::fs::create_dir_all(&directory);
    cx.spawn(async move |cx| {
        for page in 0..PAGES.len() {
            // The todo page is walked with its form open, so the walk paints the fields and the
            // daily-repeat control rather than only the list.
            if window.update(cx, |view, _, cx| {
                view.page = page;
                if page == PAGE_TODOS {
                    view.open_todo_form(None, cx);
                }
                cx.notify();
            }).is_err() {
                return;
            }
            cx.background_executor().timer(Duration::from_millis(240)).await;
        }
        let whitelist_regression=window.update(cx,|view,_,cx|{
            let mut state=view.shared.lock().unwrap().state.clone();state["settings"]["NotificationWhitelistEnabled"]=json!(false);let collapsed=page_rows(5,&state,view,cx).len();state["settings"]["NotificationWhitelistEnabled"]=json!(true);let expanded=page_rows(5,&state,view,cx).len();collapsed==9&&expanded>=collapsed+3
        }).unwrap_or(false);
        // The todo page must grow by exactly one row when the form opens, and the form must not be
        // built from a fresh entity each frame.
        let todo_regression=window.update(cx,|view,_,cx|{
            view.todo_form_open=false;view.todo_editing=None;
            let state=view.shared.lock().unwrap().state.clone();
            let collapsed=page_rows(PAGE_TODOS,&state,view,cx).len();
            view.open_todo_form(None,cx);
            let state=view.shared.lock().unwrap().state.clone();
            let opened=page_rows(PAGE_TODOS,&state,view,cx).len();
            let filled=view.todo_date.read(cx).text()==state["today"].as_str().unwrap_or_default()&&!view.todo_daily;
            view.close_todo_form(cx);
            collapsed>=3&&opened==collapsed+1&&filled
        }).unwrap_or(false);
        let picker_regression=if std::env::var_os("LYRICISLAND_SMOKE_REGRESSIONS").is_some(){
            let result=window.update(cx,|view,_,cx|{
                view.page=0;view.picker=Some(Picker{key:"ExpandedComponentColor",hsv:Hsv::new(0.3,0.5,0.7),drag:Drag::None});
                view.hex_field=Some(make_hex_field("#123456",cx));
                view.pad_bounds.set(Bounds::new(point(px(10.),px(10.)),size(px(268.),px(150.))));
                view.begin_drag(Drag::Pad,point(px(130.),px(70.)));view.drag_to(point(px(900.),px(900.)));view.end_drag(cx);
                let stopped=view.picker.unwrap();view.drag_to(point(px(20.),px(20.)));
                let released=stopped.drag==Drag::None&&stopped.hsv==view.picker.unwrap().hsv;
                let accepted=view.apply_hex("123456",cx);let colour=t::to_hex(view.picker.unwrap().hsv.to_rgb());
                let rejected=!view.apply_hex("#badvalue",cx)&&t::to_hex(view.picker.unwrap().hsv.to_rgb())==colour;
                released&&accepted&&rejected&&colour=="#123456"
            }).unwrap_or(false);
            cx.background_executor().timer(Duration::from_millis(1200)).await;Some(result)
        }else{None};
        let connected = pipe::request("state", Value::Null).is_ok_and(|value| value["success"] == json!(true));
        let report = json!({
            "success": connected&&picker_regression.unwrap_or(true)&&whitelist_regression&&todo_regression,"whitelistExpandCollapse":whitelist_regression,"pickerRegression":picker_regression,"todoRegression":todo_regression,
            "pages": PAGES.len(),
            "settings": "GPUI",
            "ipc": "Rust",
            "renderer": "directx",
        });
        let _ = std::fs::write(directory.join("report.json"), report.to_string());
        // A self-check owns the process: leave immediately rather than racing the event loop.
        std::process::exit(if connected { 0 } else { 1 });
    })
    .detach();
}

fn log(category: &str, message: &str) {
    lyric_backend::io::log(&lyric_backend::io::data_directory().join("Logs"), category, message);
}

/// Self-check hook: name one slider so its painted track is published for an automated drag.
fn smoke_slider() -> Option<&'static str> {
    static NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| std::env::var("LYRICISLAND_SMOKE_SLIDER").ok())
        .as_deref()
        .and_then(|name| ["CompactWidth", "FontSize", "MotionGap", "MotionDuration", "SpringStiffness"].into_iter().find(|key| *key == name))
}

/// Self-check hook: open straight onto a given page so each one can be photographed without a human
/// clicking through the sidebar.
fn smoke_page() -> usize {
    std::env::var("LYRICISLAND_SMOKE_PAGE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|page| *page < PAGES.len())
        .unwrap_or(0)
}

/// Paint the non-client frame to match the app's own appearance.
///
/// gpui picks the title bar theme from the *system* appearance, so a light-mode Windows would keep
/// a white frame over an app the user has switched to dark (and vice versa). DWM's immersive dark
/// mode fixes the frame without giving up the standard caption buttons.
fn apply_titlebar(window: &Window, dark: bool) {
    use raw_window_handle::RawWindowHandle;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

    // `Window` has an inherent `window_handle()` returning gpui's own handle, so the raw-window
    // trait method has to be named explicitly.
    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    let enabled = windows::core::BOOL::from(dark);
    // Failure is cosmetic only, so a refused attribute must not take the window down.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &enabled as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<windows::core::BOOL>() as u32,
        );
    }
}

/// Exercise the same asynchronous edit and reconnect paths as real controls.
fn run_reconnect_smoke(cx:&mut App,window:WindowHandle<SettingsApp>,directory:PathBuf,shared:Rc<Mutex<Shared>>){
    let _=std::fs::create_dir_all(&directory);
    let _=send(&shared,"CompactWidth","279",cx);
    cx.spawn(async move |cx| {
        for page in 0..7{
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let _=window.update(cx,|view,_,cx|{view.page=page;cx.notify();});
        }
        cx.background_executor().timer(Duration::from_millis(600)).await;
        let offline=shared.lock().is_ok_and(|s|s.connection_error);
        let _=std::fs::write(directory.join("offline-ready.json"),json!({"offlineErrorRecorded":offline,"pagesNavigatedWhileDisconnected":7}).to_string());
        let mut recovered=false;
        for _ in 0..60{
            cx.background_executor().timer(Duration::from_millis(250)).await;
            recovered=shared.lock().is_ok_and(|s|s.state["success"]==true&&!s.connection_error&&s.message.is_empty()&&s.notice.starts_with("已连接"));
            if recovered{break}
        }
        let unmodified=shared.lock().is_ok_and(|s|s.state["settings"]["CompactWidth"].as_f64()==Some(260.));
        let shared_for_send=Rc::clone(&shared);
        let _=cx.update(|cx|{let _=send(&shared_for_send,"CompactWidth","278",cx);});
        let mut saved=false;
        for _ in 0..40{
            cx.background_executor().timer(Duration::from_millis(150)).await;
            saved=shared.lock().is_ok_and(|s|s.state["settings"]["CompactWidth"].as_f64()==Some(278.)&&s.message.is_empty());
            if saved{break}
        }
        let report=json!({"success":offline&&recovered&&unmodified&&saved,"offlineErrorRecorded":offline,"connectionRecovered":recovered,"staleErrorCleared":recovered,"failedEditNotReplayed":unmodified,"nativeSettingSaved":saved,"settings":"GPUI","version":"1.3"});
        let _=std::fs::write(directory.join("report.json"),report.to_string());
        std::process::exit(if report["success"]==true{0}else{1});
    }).detach();
}

include!("tools_ui.rs");
