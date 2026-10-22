//! DotIsle control center — GPUI frontend.
//!
//! The native capsule still owns the pipe server, so this binary is a pure client and keeps the
//! existing contract (see `protocol.rs`). Navigation uses semantic routes rather than sidebar
//! indices, and the shared gear drawers reuse the exact setting keys the settings page edits.
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
mod icons;
mod motion;
mod protocol;
mod text_field;
mod theme;
mod value;
mod quick_ui;
mod rules_ui;
pub const DISPLAY_VERSION:&str="2.1.6";
mod tutorial;
mod mini_mode;
mod row_edges;
mod music_preview;
mod frame_probe;

use text_field::TextField;
use value::{format_value, position_to_value};

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Mutex,
    time::{Duration, Instant},
};

use color::Hsv;
use gpui::{prelude::*, *};
use protocol as pipe;
use serde_json::{json, Value};
use theme as t;

/// The tiny island mark used by the onboarding tutorial's simulated tray.
fn brand_pill()->AnyElement{canvas(|_,_,_|{},|bounds,_,window,_|{let x=bounds.origin.x;let y=bounds.origin.y;window.paint_quad(quad(Bounds::new(point(x+px(4.0),y),size(px(27.0),px(17.0))),px(8.5),rgb(0x222829),px(0.),transparent_black(),BorderStyle::Solid));window.paint_quad(quad(Bounds::new(point(x+px(22.4),y+px(6.1)),size(px(4.8),px(4.8))),px(2.4),rgb(0xF4C34D),px(0.),transparent_black(),BorderStyle::Solid));}).w(px(35.)).h(px(17.)).into_any_element()}

/// Sidebar width, straight from the 1.5 handoff's `--di-sidebar`.
const SIDEBAR: f32 = 208.0;

/// The control centre is a fixed canvas.
///
/// Every page scrolls inside this box and the layout was designed at exactly this size, so letting
/// it be dragged around only ever bought a way to break the design. These are **logical** pixels,
/// not physical: on a 200% display the window is twice as many pixels and the interface is the same
/// size, which is what "supports several DPIs" has to mean.
const WINDOW_WIDTH: f32 = 1180.0;
const WINDOW_HEIGHT: f32 = 760.0;

/// The canvas for the display the window is currently on.
///
/// A fixed window still has to fit: a 200% laptop offers only 960×540 of usable logical space, so
/// the canvas shrinks to the work area instead of hanging off the screen. It never grows past the
/// design size — the window is fixed, not elastic.
fn canvas_size(window: &Window, cx: &App) -> Size<Pixels> {
    let design = size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
    match window.display(cx) {
        Some(display) => design.min(&display.visible_bounds().size),
        None => design,
    }
}

/// One page of the control center. The order here is the walking order for `--smoke-ui`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Island,
    Overview,
    Notifications,
    Todos,
    Timers,
    Settings,
    About,
}

impl Route {
    const ALL: [Route; 7] = [
        Route::Island,
        Route::Overview,
        Route::Notifications,
        Route::Todos,
        Route::Timers,
        Route::Settings,
        Route::About,
    ];

    fn index(self) -> usize {
        Route::ALL.iter().position(|route| *route == self).unwrap_or(0)
    }

    fn heading(self) -> (&'static str, &'static str) {
        match self {
            Route::Island => ("灵动岛", "需要时出现，处理后收起。"),
            Route::Overview => ("概览", "重要的事，一眼可见。"),
            Route::Notifications => ("通知", "回看已收到的消息，及时处理自己的提醒。"),
            Route::Todos => ("待办清单", "把重要的小事，安排妥当。"),
            Route::Timers => ("流程与专注", "一次只关注眼前这一件事。"),
            Route::Settings => ("设置", "调整点屿的共同偏好。"),
            Route::About => ("关于与更新", "点屿 · DotIsle"),
        }
    }

    /// The sidebar label can differ from the page title: the docked island wears its product name
    /// (灵动岛, the same one the tray menu and the capsule use) and the todo page drops the trailing
    /// 清单 where the group already frames it.
    fn nav_label(self) -> &'static str {
        match self {
            Route::Island => "灵动岛",
            Route::Todos => "待办",
            other => other.heading().0,
        }
    }

    /// The icon name from the handoff's nav table, served by `icons::icon`.
    fn nav_icon(self) -> &'static str {
        match self {
            Route::Island => "pill",
            Route::Overview => "layout-dashboard",
            Route::Notifications => "bell",
            Route::Todos => "circle-check",
            Route::Timers => "timer",
            Route::Settings => "settings-2",
            Route::About => "circle-help",
        }
    }

    /// The stable name the native side stores as the start page.
    fn key(self) -> &'static str {
        match self {
            Route::Island => "island",
            Route::Overview => "overview",
            Route::Notifications => "notifications",
            Route::Todos => "todos",
            Route::Timers => "timers",
            Route::Settings => "settings",
            Route::About => "about",
        }
    }

    fn from_key(key: &str) -> Option<Route> {
        Route::ALL.into_iter().find(|route| route.key() == key)
    }
}

/// A panel that slides over the right side of the content area. Gear settings open the same drawer
/// from both the tool page and anywhere else, so a field only ever has one definition.
#[derive(Clone, PartialEq)]
enum Drawer {
    TodoEditor,
    TodoSettings,
    Snooze(String),
    Focus(String),
    NoticeDetail(String),
    NotificationSettings,
    TimerSettings,
    Changelog,
    Licenses,
    QuickReminder,
    WorkflowEditor,
    RuleEditor(Option<String>),
}

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
    route: Route,
    /// The single open overlay panel, if any. One slot on purpose: an editor and a gear drawer
    /// must never stack.
    drawer: Option<Drawer>,
    /// Where a "前往音效" jump came from, so the settings page can offer the way back.
    return_context: Option<(Route, Option<Drawer>)>,
    picker: Option<Picker>,
    slider: Option<SliderDrag>,
    /// Where the pad and the hue strip were actually painted, in window coordinates. Recorded during
    /// prepaint because pointer events only carry a window position.
    pad_bounds: Rc<Cell<Bounds<Pixels>>>,
    hue_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Keyboard focus for the music-layout blocks, so Left/Right/Space work after a click.
    music_focus: FocusHandle,
    /// Scroll state of the content column. One scroll handle per route, so switching routes always
    /// starts where that route was left. The drawer keeps its own offset as well.
    scrolls: Vec<ScrollHandle>,
    drawer_scroll: ScrollHandle,
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
    notification_blocked:Entity<TextField>,
    /// Todo form fields. Created once and reused, exactly like the fields above: rebuilding them
    /// inside `render` would drop focus and the half-typed text on every frame.
    todo_title: Entity<TextField>,
    todo_content: Entity<TextField>,
    todo_date: Entity<TextField>,
    todo_time: Entity<TextField>,
    /// `Some(id)` while an existing todo is being edited, `None` when composing a new one.
    todo_editing: Option<String>,
    todo_daily: bool,
    todo_filter:usize,todo_completed_open:bool,todo_color:String,
    /// Notification-history page state: the active kind filter and the two-step clear confirmation.
    notice_filter:usize,clear_armed:bool,
    /// Which section of the settings page is open: 常规 / 外观与动画 / 音效.
    settings_tab:usize,
    /// Minutes preset for the "开始专注" drawer.
    focus_minutes:u32,
    timer_motion:RefCell<std::collections::HashMap<String,TimerSample>>,
    timer_mode:String,timer_new:bool,timer_adjust_open:bool,timer_name:Entity<TextField>,timer_duration:Entity<TextField>,timer_focus:Entity<TextField>,timer_rest:Entity<TextField>,timer_rounds:Entity<TextField>,
    notice_view: usize,
    quick_text: Entity<TextField>,
    quick_preview: Value,
    quick_request: String,
    workflow_name: Entity<TextField>,
    workflow_stages: Vec<(Entity<TextField>, Entity<TextField>, usize)>,
    workflow_count: usize,
    workflow_template: String,
    workflow_selected:usize,workflow_generation:u64,workflow_busy:bool,
    workflow_library:Vec<LibraryModule>,workflow_module_selected:Option<String>,workflow_module_serial:u64,
    workflow_library_generation:u64,workflow_library_busy:bool,
    workflow_stage_editing:Option<usize>,workflow_stage_backup:String,
    tutorial_step:Option<usize>,tutorial_epoch:usize,tutorial_recovery:u8,tutorial_checked:bool,
    workflow_drop:Option<usize>,workflow_stage_bounds:Rc<RefCell<Vec<Bounds<Pixels>>>>,workflow_canvas_bounds:Rc<Cell<Bounds<Pixels>>>,
    workflow_scroll:ScrollHandle,workflow_focus:FocusHandle,workflow_pending:Option<Value>,
    /// The 自定义快捷键 recorder. `hotkey_pressed` accumulates modifiers for the session so a
    /// combination typed one key at a time still reads as one chord.
    hotkey_focus:FocusHandle,hotkey_active:bool,hotkey_pressed:gpui::Modifiers,hotkey_candidate:Option<String>,
    rule_editing: Option<String>,
    rule_trigger: String,
    rule_name: Entity<TextField>,
    rule_path: Entity<TextField>,
    rule_message: Entity<TextField>,
    rule_cooldown: u32,
    rule_threshold: u32,
    rule_enabled: bool,
    /// Which select-style control is open, plus the window rectangles its menu can hang from —
    /// one entry per painted control, because a page may carry several.
    select: Option<&'static str>,
    select_bounds: Rc<RefCell<Vec<(&'static str, Bounds<Pixels>)>>>,
    /// Bumped on a route change. Part of the entrance animation's element id, so the page's rows
    /// cascade in again every time a different page replaces them.
    route_epoch: u32,
    /// Bumped whenever the *content* under a page's tab bar is replaced, including tab and filter
    /// switches. The tab bar itself keeps using `route_epoch`, so it never blinks when a filter is
    /// clicked — only the rows below it cross over.
    content_epoch: u32,
    /// Bumped when a select menu opens, so its menu animates in from scratch every time.
    menu_epoch: u32,
    /// The drawer that is still travelling out. `drawer` clears the instant the user dismisses it,
    /// but the panel has to stay mounted for the length of its exit spring or it would vanish
    /// mid-gesture; this keeps it alive until that spring settles.
    closing: Option<Drawer>,
    /// The colour picker's exit twin. Same idea, smaller surface.
    closing_picker: Option<Picker>,
    /// The palette this window is drawing with right now, and the one it is travelling from while
    /// an appearance change eases. Keeping the *displayed* palette (rather than the stored theme) is
    /// what lets an interrupted theme switch start from where the screen actually is.
    shown: t::Palette,
    theme_from: t::Palette,
    theme_key: Option<(String, bool)>,
    theme_started: Option<Instant>,
    /// The live controls the pointer is over, outermost first. Controls nest — a button sits inside
    /// a row — so the stack keeps the outer one too: leaving the button resumes the row's own wash
    /// without waiting for a hover event that already happened.
    hover: Vec<SharedString>,
    /// The control that had a button pressed on it while the pointer was over it. One at a time, and
    /// cleared by the root, so a press released outside the window never stays sunk.
    press: Option<SharedString>,
}

impl SettingsApp {
    /// Whether this control is the innermost live one under the pointer.
    fn hovered(&self, key: &str) -> bool {
        matches!(self.hover.last(), Some(live) if live.as_str() == key)
    }

    /// Whether the pointer is down on this control.
    fn pressed(&self, key: &str) -> bool {
        matches!(self.press.as_ref(), Some(live) if live.as_str() == key)
    }

    /// Where the pointer has left this control standing: rest, hover, or press.
    fn liveness(&self, key: &str) -> f32 {
        motion::liveness(self.hovered(key), self.pressed(key))
    }

    /// The innermost live control takes the press. Read at the root, after the hover stack has
    /// already been settled by the move that preceded this one, so a control that was never tracked
    /// simply never appears here.
    fn press_began(&mut self) {
        self.press = self.hover.last().cloned();
    }

    fn press_ended(&mut self) {
        self.press = None;
    }

    fn new(shared: Rc<Mutex<Shared>>, picker: Option<Picker>, cx: &mut Context<Self>) -> Self {
        let calibrate = {
            let shared = Rc::clone(&shared);
            cx.new(|cx| {
                TextField::new(
                    "field-calibrate",
                    "秒数",
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
        let timer_rounds=cx.new(|cx|TextField::new("timer-rounds","轮数","1",Rc::new(|_,_|{}),cx));
        let quick_text={let weak=weak.clone();cx.new(|cx|TextField::new("field-quick","例如：20分钟后取快递","",Rc::new(move|text,app|{let text=text.to_owned();let _=weak.update(app,move|view,cx|view.preview_quick_text(text,cx));}),cx))};
        let workflow_name=cx.new(|cx|TextField::new("flow-name","流程名称","",Rc::new(|_,_|{}),cx));
        let mut workflow_stages=Vec::new();
        for index in 0..12{let commit=weak.clone();let cancel=weak.clone();let name=cx.new(move|cx|{let mut field=TextField::new(format!("flow-stage-name-{index}"),"阶段名称","",Rc::new(move|_,app|{let _=commit.update(app,|view,cx|view.finish_stage_rename(true,cx));}),cx);field.set_cancel_handler(Some(Rc::new(move|app|{let _=cancel.update(app,|view,cx|view.finish_stage_rename(false,cx));})));field});let minutes=cx.new(move|cx|TextField::new(format!("flow-stage-minutes-{index}"),"分钟","",Rc::new(|_,_|{}),cx));workflow_stages.push((name,minutes,5));}
        let mut workflow_library=Vec::new();
        let module_seed={let state=shared.lock().map(|shared|shared.state.clone()).unwrap_or_else(|_|json!({}));workflow_module_seed(&state)};
        for value in &module_seed{workflow_library.push(new_library_module(value,&weak,cx));}
        let workflow_module_serial=workflow_library.len() as u64;
        let rule_name=cx.new(|cx|TextField::new("rule-name","规则名称","",Rc::new(|_,_|{}),cx));
        let rule_path=cx.new(|cx|TextField::new("rule-path","C:/Program Files/App/App.exe","",Rc::new(|_,_|{}),cx));
        let rule_message=cx.new(|cx|TextField::new("rule-message","提醒内容","",Rc::new(|_,_|{}),cx));
        // Self-check pages win, then the user's own 打开时显示的页面 choice.
        let music_focus = cx.focus_handle();
        let start_route = smoke_page().unwrap_or_else(|| {
            let state = shared.lock().map(|shared| shared.state.clone()).unwrap_or_else(|_| json!({}));
            Route::from_key(setting_str(&state, "ControlCenterStartPage", "overview")).unwrap_or(Route::Overview)
        });
        Self {
            timer_motion:RefCell::new(std::collections::HashMap::new()),timer_name,timer_duration,timer_focus,timer_rest,timer_rounds,timer_mode:std::env::var("LYRICISLAND_SMOKE_TIMER_MODE").ok().filter(|mode| ["workflow","pomodoro","countdown","stopwatch"].contains(&mode.as_str())).unwrap_or_else(|| "workflow".into()),timer_new:false,timer_adjust_open:false,todo_filter:0,todo_completed_open:false,todo_color:"#F3D79B".into(),
            shared:Rc::clone(&shared),
            route: start_route,
            drawer: smoke_drawer(),
            return_context: None,
            // The smoke picker lives in 设置 → 外观与动画, so opening one brings that tab up too.
            settings_tab: std::env::var("LYRICISLAND_SMOKE_TAB").ok().and_then(|value| value.parse::<usize>().ok()).map(|tab| tab.min(2)).unwrap_or(if picker.is_some() || smoke_slider().is_some_and(|key| key.starts_with("Motion") || key.starts_with("Spring")) { 1 } else { 0 }),
            notice_filter: 0,
            clear_armed: false,
            focus_minutes: 25,
            picker,
            slider: None,
            pad_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            hue_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            music_focus,
            scrolls: (0..Route::ALL.len()).map(|_| ScrollHandle::new()).collect(),
            drawer_scroll: ScrollHandle::new(),
            applied_dark: None,
            smoke_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            hex_field:picker.map(|p|make_hex_field(&t::to_hex(p.hsv.to_rgb()),cx)),
            calibrate,
            custom_sources,
            notification_apps,
            notification_blocked:{let shared=Rc::clone(&shared);cx.new(|cx|TextField::new("field-notification-blocked","输入完整应用标识，分号分隔；按Enter添加","",Rc::new(move|value:&str,app:&mut App|{let _=send(&shared,"notificationBlockedApps",value,app);}),cx))},
            todo_title,
            todo_content,
            todo_date,
            todo_time,
            todo_editing: None,
            todo_daily: false,
            select: None,
            select_bounds: Rc::new(RefCell::new(Vec::new())),
            route_epoch: 0,
            content_epoch: 0,
            menu_epoch: 0,
            closing: None,
            closing_picker: None,
            shown: t::DARK,
            theme_from: t::DARK,
            theme_key: None,
            theme_started: None,
            hover: Vec::new(),
            press: None,
            notice_view:0,quick_text,quick_preview:Value::Null,quick_request:String::new(),
            workflow_name,workflow_stages,workflow_count:2,workflow_template:String::new(),workflow_selected:0,workflow_generation:0,workflow_busy:false,
            workflow_library,workflow_module_selected:None,workflow_module_serial,workflow_library_generation:0,workflow_library_busy:false,
            tutorial_step:if std::env::args().any(|a|a=="--tutorial"){Some(0)}else{None},tutorial_epoch:0,tutorial_recovery:0,tutorial_checked:false,workflow_stage_editing:None,workflow_stage_backup:String::new(),workflow_drop:None,workflow_stage_bounds:Rc::new(RefCell::new(Vec::new())),workflow_canvas_bounds:Rc::new(Cell::new(Bounds::new(point(px(0.0),px(0.0)),size(px(0.0),px(0.0))))),
            workflow_scroll:ScrollHandle::new(),workflow_focus:cx.focus_handle(),workflow_pending:None,
            hotkey_focus:cx.focus_handle(),hotkey_active:false,hotkey_pressed:gpui::Modifiers::default(),hotkey_candidate:None,
            rule_editing:None,rule_trigger:"enter".into(),rule_name,rule_path,rule_message,rule_cooldown:30,rule_threshold:60,rule_enabled:true,
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
        cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{match outcome{Ok((reply,state))=>{let success=reply["success"]==true;if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("保存失败").into()};}if success{view.drawer=None;view.todo_editing=None;}},Err(e)=>{if let Ok(mut shared)=view.shared.lock(){shared.message=e;}}}cx.notify();});}).detach();
    }

    /// Open the editor drawer, either empty (defaulting to today) or loaded from an existing todo.
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
        self.drawer = Some(Drawer::TodoEditor);
        self.todo_daily = daily;
        self.todo_title.update(cx, |field, cx| field.set_text(&title, cx));
        self.todo_content.update(cx, |field, cx| field.set_text(&content, cx));
        self.todo_date.update(cx, |field, cx| field.set_text(&date, cx));
        self.todo_time.update(cx, |field, cx| field.set_text(&time, cx));
        cx.notify();
    }

    fn close_todo_form(&mut self, cx: &mut Context<Self>) {
        self.drawer = None;
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

/// The label the 打开时显示的页面 select shows for the stored start page.
fn start_page_label(state: &Value) -> &'static str {
    match setting_str(state, "ControlCenterStartPage", "overview") {
        "island" => "灵动岛",
        "notifications" => "通知",
        "todos" => "待办清单",
        "timers" => "流程与专注",
        "settings" => "设置",
        "about" => "关于与更新",
        _ => "概览",
    }
}

/// The label the 界面主题 select shows for the stored theme.
fn theme_label(state: &Value) -> &'static str {
    match setting_str(state, "SettingsTheme", "System") {
        "Light" => "浅色",
        "Dark" => "深色",
        _ => "跟随系统",
    }
}

/// A 1px wash used as a separator. Cheaper to reason about than a side-specific border.
fn hairline() -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(t::hairline())
}

thread_local! {
    /// Counts rows materialised during a self-check. Sections are wrapped in panels now, so the
    /// number of top-level elements no longer says how many rows a page really builds.
    static ROW_TALLY: Cell<usize> = const { Cell::new(0) };
}

fn tally_reset() {
    ROW_TALLY.with(|count| count.set(0));
}

fn tally_count() -> usize {
    ROW_TALLY.with(Cell::get)
}

fn tally() {
    ROW_TALLY.with(|count| count.set(count.get() + 1));
}

/// Rows stacked with a hairline between neighbours — the rhythm every surface shares.
fn stacked(rows: Vec<AnyElement>, rounded_panel: bool) -> Div {
    let count = rows.len();
    let mut body = div().flex().flex_col().min_w_0();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            // Same bleed as the rows: a divider narrower than the fill it separates reads as
            // a line drawn inside the highlight rather than as the boundary between two rows.
            body = body.child(div().h(px(1.0)).mx(px(-PANEL_PAD)).bg(t::hairline()));
        }
        body = body.child(row_edges::wrap(row, rounded_panel && index == 0, rounded_panel && index + 1 == count));
    }
    body
}

/// A raised group, as the handoff draws it: one bordered panel holding every row of a section,
/// with hairlines inserted *between* rows — never at the edges.
///
/// Rows bleed to the panel border. Only its first and last rows inherit the outer radius;
/// interior seams remain straight, including in drawers. RowEdges scopes that topology while
/// lazy animation elements materialise their row styles during request_layout.
const PANEL_PAD: f32 = 18.0;
const PANEL_RADIUS: f32 = 16.0;

/// The raised sheet every group, list and card is cut from.
///
/// Three quiet layers do the work: a vertical wash a hair lighter at the top, a one-pixel inset
/// sheen along the top edge, and a shadow that hugs the outline. The border is kept so the panel
/// still separates from the base when the shadow is invisible (light mode, low-contrast displays).
fn raised(radius: f32) -> Div {
    div().w_full().min_w_0().rounded(px(radius))
        .border_1().border_color(t::hairline())
        .bg(linear_gradient(
            180.0,
            linear_color_stop(t::surface(), 0.0),
            linear_color_stop(t::surface(), 1.0),
        ))
        .shadow(vec![
            BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset(),
            BoxShadow::new(px(0.0), px(1.0), t::shadow().into()).blur_radius(px(2.0)),
            BoxShadow::new(px(0.0), px(10.0), t::shadow().opacity(0.30).into())
                .blur_radius(px(24.0)).spread_radius(px(-16.0)),
        ])
}

fn panel(rows: Vec<AnyElement>) -> AnyElement {
    raised(PANEL_RADIUS).overflow_hidden().px(px(PANEL_PAD)).py(px(0.0)).child(stacked(rows, true)).into_any_element()
}

/// A borderless panel whose rows carry their own horizontal padding (notification and timer lists).
fn list(rows: Vec<AnyElement>) -> AnyElement {
    raised(PANEL_RADIUS).overflow_hidden().p(px(8.0)).child(stacked(rows, false)).into_any_element()
}

/// A 34px icon tile (`di-symbol`): notification rows and task lines wear one.
fn symbol(icon_name: &'static str, bg: gpui::Rgba, fg: gpui::Rgba) -> AnyElement {
    div().w(px(34.0)).h(px(34.0)).flex_shrink_0().rounded(px(11.0)).bg(bg).flex().items_center().justify_center().text_color(fg)
        .child(icons::tinted(icon_name, fg)).into_any_element()
}

/// A small status word on a soft surface (`di-pill`).
fn pill(label: &str) -> AnyElement {
    div().flex_shrink_0().px(px(9.0)).py(px(3.0)).rounded(px(9.0)).bg(t::surface_hi()).text_size(px(12.0)).text_color(t::text_dim()).child(label.to_owned()).into_any_element()
}

/// A big read-out at the end of a task line (`di-minitime`).
fn minitime(label: &str) -> AnyElement {
    div().flex_shrink_0().text_size(px(24.0)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(label.to_owned()).into_any_element()
}

/// The empty state inside a list (`di-empty`).
fn empty_row(label: &str) -> AnyElement {
    div().w_full().px(px(18.0)).py(px(45.0)).flex().justify_center().text_size(px(12.0)).text_color(t::text_dim()).child(label.to_owned()).into_any_element()
}

/// One `di-taskcheck` line: a symbol, a growing title/detail block and an optional trailing slot.
fn taskcheck(sym: AnyElement, title: &str, detail: &str, trailing: Option<AnyElement>) -> AnyElement {
    let mut body = div().flex().items_center().gap(px(10.0)).w_full().min_w_0()
        .child(sym)
        .child(div().flex().flex_col().gap_1().flex_1().min_w_0()
            .child(div().text_size(px(15.0)).font_weight(FontWeight::MEDIUM).text_color(t::text()).min_w_0().child(title.to_owned()))
            .child(div().text_size(px(12.0)).text_color(t::text_dim()).min_w_0().child(detail.to_owned())));
    if let Some(trailing) = trailing {
        body = body.child(trailing);
    }
    body.into_any_element()
}

/// A card holding one content block and, below a hairline, an inset action strip (`di-inset`).
fn inset_card(highlight: bool, body: AnyElement, actions: Option<AnyElement>) -> AnyElement {
    let card = if highlight {
        div().w_full().min_w_0().rounded(px(PANEL_RADIUS))
            .bg(linear_gradient(180.0, linear_color_stop(t::yellow(), 0.0), linear_color_stop(motion::mix(0.5, t::yellow(), t::surface()), 1.0)))
            .border_1().border_color(t::yellow_line().opacity(0.35))
    } else {
        raised(PANEL_RADIUS)
    };
    let card = card.child(div().px(px(18.0)).pt(px(18.0)).pb(px(if actions.is_some() { 14.0 } else { 18.0 })).child(body));
    match actions {
        Some(actions) => card.child(div().mx(px(18.0)).pt(px(15.0)).pb(px(18.0)).border_t_1().border_color(t::hairline()).child(actions)).into_any_element(),
        None => card.into_any_element(),
    }
}

/// One section: a 14px heading with its accent tick, an optional green jump link, and one body
/// below. The 22px section rhythm lives here, so the page column itself carries no gap.
fn section_block(title: &str, link: Option<(&'static str, &str, Route)>, body: AnyElement, cx: &mut Context<SettingsApp>) -> AnyElement {
    let head = div().flex().items_center().justify_between().gap_3().mb(px(11.0))
        .child(div().flex().items_center().gap(px(9.0))
            .child(div().w(px(3.0)).h(px(14.0)).rounded(px(1.5)).bg(t::accent()))
            .child(div().text_size(px(14.5)).font_weight(FontWeight::SEMIBOLD).text_color(t::text()).child(title.to_owned())));
    let head = match link {
        Some((id, label, route)) => head.child(link_button(id, label, route, cx)),
        None => head,
    };
    div().flex().flex_col().mb(px(22.0)).child(head).child(body).into_any_element()
}

/// A column of standalone cards (the todo stack): 10px apart like `di-todos`.
fn card_stack(rows: Vec<AnyElement>) -> AnyElement {
    div().w_full().min_w_0().flex().flex_col().gap(px(10.0)).children(rows).into_any_element()
}

/// A green jump link with the ↗ icon, used on section heads.
fn link_button(id: &'static str, label: &str, route: Route, cx: &mut Context<SettingsApp>) -> AnyElement {
    div().id(id).flex().items_center().gap(px(5.0)).text_size(px(12.0)).text_color(t::accent()).cursor_pointer()
        .hover(|style| style.opacity(0.72))
        .child(label.to_owned())
        .child(icons::tinted("arrow-up-right", t::accent()))
        .on_click(cx.listener(move |view, _, _, cx| {
            view.go(route);
            cx.notify();
        }))
        .into_any_element()
}

/// One section: a 16px heading, an optional green jump link, and a single panel below.
fn section_group(title: &str, link: Option<(&'static str, &str, Route)>, rows: Vec<AnyElement>, cx: &mut Context<SettingsApp>) -> AnyElement {
    section_block(title, link, panel(rows), cx)
}

/// A row of `di-tab` pills: the page-level filters (待办筛选、通知分类、计时模式、设置分页).
///
/// The pills sit inside a recessed track, so the row reads as one segmented control instead of a
/// scatter of buttons. The active pill keeps its own spring fill — the track stays put while the
/// selection travels, which is the same "one object changed" story the nav rail tells.
fn tab_row(id_prefix: &'static str, labels: &[&'static str], selected: usize, cx: &mut Context<SettingsApp>, pick: impl Fn(&mut SettingsApp, usize) + Clone + 'static) -> AnyElement {
    let mut bar = div().flex().items_center().flex_wrap().gap(px(2.0)).p(px(3.0)).rounded(px(13.0))
        .bg(t::field()).border_1().border_color(t::hairline());
    for (index, label) in labels.iter().enumerate() {
        let text: &'static str = *label;
        let active = index == selected;
        let pick = pick.clone();
        let click = cx.listener(move |view, _, _, cx| {
            pick(view, index);
            // The rows below the bar are about to be replaced: restart their cascade so the new
            // content arrives instead of silently swapping.
            if id_prefix!="timer-mode"{view.pulse();}
            cx.notify();
        });
        let id = SharedString::from(format!("tab-{id_prefix}-{index}"));
        bar = bar.child(
            div().id((id.clone(), 1usize)).with_spring(
                (id.clone(), 0usize),
                motion::phase(motion::snappy(), active),
                move |_, phase| {
                    let lift = motion::alpha(phase);
                    div().id(id.clone()).px(px(15.0)).py(px(7.0)).rounded(px(10.0))
                        .bg(motion::blend_color(phase, transparent_black().into(), t::accent()))
                        .text_size(px(12.5))
                        .text_color(motion::blend_color(phase, t::text_dim(), t::on_accent()))
                        .font_weight(if lift > 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                        .shadow(vec![BoxShadow::new(px(0.0), px(1.0), t::shadow().opacity(lift).into()).blur_radius(px(2.0))])
                        .cursor_pointer().active(|style| style.opacity(0.72)).child(text.to_owned())
                        .on_click(click)
                },
            ),
        );
    }
    div().flex().w_full().mb(px(16.0)).child(bar).into_any_element()
}

/// A select-style control: the handoff's `di-select` at rest. The dropdown itself is painted by
/// `select_overlay` at the window level, so a scroll container can never clip it.
fn select_control(id: &'static str, value_label: &str, open: bool, width: Option<f32>, bounds: Rc<RefCell<Vec<(&'static str, Bounds<Pixels>)>>>, cx: &mut Context<SettingsApp>) -> AnyElement {
    let cell = bounds;
    let probe = canvas(move |painted, _, _| {
        if let Ok(mut map) = cell.try_borrow_mut() {
            match map.iter_mut().find(|(name, _)| *name == id) {
                Some(entry) => entry.1 = painted,
                None => map.push((id, painted)),
            }
        }
    }, |_, _, _, _| {}).absolute().inset_0();
    let control = div().id(id).relative().flex().items_center().justify_between().gap_2()
        .min_h(px(34.0))
        .px(px(11.0)).py(px(8.0)).rounded(px(10.0))
        .bg(t::field()).border_1().border_color(if open { t::accent() } else { t::hairline() })
        .text_size(px(12.5)).text_color(t::text()).cursor_pointer()
        .shadow(vec![BoxShadow::new(px(0.0), px(1.0), t::shadow().opacity(0.7).into()).blur_radius(px(2.0)).inset()])
        .hover(|style| style.bg(t::surface_hi()))
        .child(probe)
        .child(div().min_w_0().overflow_hidden().child(value_label.to_owned()))
        .child(icons::tinted("chevron-down", if open { t::accent() } else { t::text_dim() }))
        .on_click(cx.listener(move |view, _, _, cx| {
            view.select = if view.select == Some(id) { None } else { Some(id) };
            // A fresh epoch per opening: the menu drops in every time, even when it is reopened
            // from the same control.
            view.menu_epoch = view.menu_epoch.wrapping_add(1);
            cx.notify();
        }));
    match width {
        Some(width) => control.w(px(width)).max_w(px(width)).into_any_element(),
        None => control.w_full().into_any_element(),
    }
}

/// One setting row with a select on the right (`di-settinglabel` + `di-select`).
fn select_row(id: &'static str, title: &str, detail: &str, value_label: &str, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let row = card(SharedString::from(format!("row-{id}")), title, detail)
        .child(select_control(id, value_label, view.select == Some(id), Some(175.0), Rc::clone(&view.select_bounds), cx));
    live_shell(row, SharedString::from(id), view, cx, |row, liveness| row_wash(row, liveness, false))
}

/// A headedless group: one panel on the 22px section rhythm (the settings tabs use these).
fn plain_group(rows: Vec<AnyElement>) -> AnyElement {
    div().flex().flex_col().mb(px(22.0)).child(panel(rows)).into_any_element()
}

/// The open select, painted under the root so it floats above the content and its scrim swallows
/// the next click anywhere else.
fn select_overlay(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let Some(key) = view.select else {
        return div().into_any_element();
    };
    let anchor = view.select_bounds.borrow().iter().find(|(name, _)| *name == key).map(|(_, bounds)| *bounds)
        .unwrap_or_else(|| Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))));
    let options: &[(&str, &str)] = match key {
        "osd-placement" => &[("Island","灵动岛内"),("Bottom","屏幕下方")],
        "startPage" => &[("overview", "概览"), ("notifications", "通知"), ("todos", "待办清单"), ("timers", "流程与专注"), ("island", "灵动岛"), ("settings", "设置"), ("about", "关于与更新")],
        "todo-color" | "todo-default-color" => &[("#F3D79B", "柔黄"), ("#B7DBCB", "薄荷"), ("#C9BCE5", "浅紫")],
        _ => &[("System", "跟随系统"), ("Light", "浅色"), ("Dark", "深色")],
    };
    let current = match key {
        "osd-placement" => setting_str(state,"SystemStateHintPlacement","Island").to_owned(),
        "startPage" => setting_str(state, "ControlCenterStartPage", "overview").to_owned(),
        "todo-color" => view.todo_color.clone(),
        "todo-default-color" => setting_str(state, "TodoDefaultColor", "#F3D79B").to_owned(),
        _ => setting_str(state, "SettingsTheme", "System").to_owned(),
    };
    let menu_height=options.len()as f32*36.+12.;let menu_left=f32::from(anchor.origin.x).clamp(8.,WINDOW_WIDTH-218.);
    let below=f32::from(anchor.origin.y+anchor.size.height)+6.;
    let menu_top=if below+menu_height<=WINDOW_HEIGHT-12.{below}else{(f32::from(anchor.origin.y)-menu_height-6.).max(48.)};
    let mut menu = div().flex().flex_col().gap(px(1.0)).p(px(6.0));
    for (value, label) in options {
        let active = current == *value;
        let value = value.to_string();
        let mut option = div().id(SharedString::from(format!("select-option-{key}-{value}"))).flex().items_center().justify_between().gap_3()
            .px(px(11.0)).py(px(8.0)).rounded(px(9.0)).text_size(px(12.5)).text_color(t::text()).cursor_pointer();
        if active {
            option = option.bg(t::accent_dim()).text_color(t::accent());
        }
        menu = menu.child(
            option
                .hover(move |style| if active { style } else { style.bg(t::surface_hi()) })
                .child((*label).to_owned())
                .child(if active { icons::tinted("check", t::accent()).into_any_element() } else { div().into_any_element() })
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.select = None;
                    match key {
                        "todo-color" => view.todo_color = value.clone(),
                        "todo-default-color" => {let shared = Rc::clone(&view.shared);let _ = send(&shared, "TodoDefaultColor", &value, cx);}
                        "osd-placement" => {let shared=Rc::clone(&view.shared);let _=send(&shared,"SystemStateHintPlacement",&value,cx);}
                        "startPage" => {let shared = Rc::clone(&view.shared);let _ = send(&shared, "ControlCenterStartPage", &value, cx);}
                        _ => {let shared = Rc::clone(&view.shared);let _ = send(&shared, "SettingsTheme", &value, cx);}
                    }
                    cx.notify();
                })),
        );
    }
    div().id("select-layer").absolute().inset_0().occlude()
        .child(div().id("select-scrim").absolute().inset_0().on_click(cx.listener(|view, _, _, cx| {view.select = None;cx.notify();})))
        .child(
            div().absolute().left(px(menu_left)).w(px(210.0))
                .rounded(px(14.0)).border_1().border_color(t::hairline())
                .bg(linear_gradient(180.0, linear_color_stop(t::panel_top(), 0.0), linear_color_stop(t::panel_bottom(), 1.0)))
                .shadow(vec![
                    BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset(),
                    BoxShadow::new(px(0.0), px(14.0), t::shadow().opacity(0.9).into()).blur_radius(px(30.0)),
                ])
                .child(menu)
                // The menu grows out of the control that owns it and travels the short distance to
                // its resting place, so the eye never has to search for where it came from.
                .with_animation(("select-menu", view.menu_epoch), motion::enter(170, 0), move |element, progress| {
                    element.opacity(progress).top(px(menu_top + 7.0 * (1.0 - progress)))
                }),
        )
        .into_any_element()
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Windows delivers a DPI change as a new window rect that keeps the *physical* size, which
        // silently changes the logical canvas the layout was built for: drag the window from a 100%
        // to a 200% monitor and the whole interface would come out half size. Re-assert the canvas
        // every frame — it is a no-op unless something else moved it.
        let canvas = canvas_size(window, cx);
        let current = window.bounds().size;
        if (f32::from(current.width) - f32::from(canvas.width)).abs() > 0.5
            || (f32::from(current.height) - f32::from(canvas.height)).abs() > 0.5
        {
            window.resize(canvas);
        }
        let state = self.state();
        if !self.tutorial_checked&&state["success"]==true{self.tutorial_checked=true;if !flag(&state,"GestureTutorialSeen",false)&&std::env::var_os("LYRICISLAND_TEST_PIPE").is_none_or(|p|p=="LyricIsland.Native.Settings.v1"){self.tutorial_step=Some(0);}}
        if self.route==Route::Timers&&self.drawer.is_none()&&!self.timer_new&&self.shared.lock().map(|s|!s.connection_error).unwrap_or(false){
            if let Some(items)=state["timers"].as_array(){let active=state["activeComponent"].as_str().unwrap_or("").strip_prefix("timer:").unwrap_or("");let selected=items.iter().find(|t|t["mode"].as_str()==Some(self.timer_mode.as_str())&&t["id"].as_str()==Some(active)).or_else(||items.iter().find(|t|t["mode"].as_str()==Some(self.timer_mode.as_str())));
                if let Some(timer)=selected.filter(|t|t["running"]==true){let id=timer["id"].as_str().unwrap_or("");let samples=self.timer_motion.borrow();if samples.get(id).map(|s|s.observed.elapsed().as_secs_f64()<0.92||s.elapsed!=timer["elapsedMs"].as_u64().unwrap_or(0)).unwrap_or(true){window.request_animation_frame();}}
            }
        }
        let route = self.route;
        let (title, subtitle) = route.heading();
        // Screenshots need both appearances without touching the user's stored theme; the smoke
        // hook is read only when an automated run sets it.
        let theme = std::env::var("LYRICISLAND_SMOKE_THEME").ok()
            .filter(|value| value == "Light" || value == "Dark")
            .unwrap_or_else(|| setting_str(&state, "SettingsTheme", "System").to_owned());
        let system_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let want_dark = match theme.as_str() {
            "Light" => false,
            "Dark" => true,
            _ => system_dark,
        };
        // 界面动画 is the application's own switch, not Windows': folding it into gpui's
        // reduce-motion flag makes every spring and every one-shot below obey it without each call
        // site checking. Reduced motion still gets the end state — it is a gentler equivalent, not
        // a dead interface.
        let animate = flag(&state, "Animation", true);
        motion::set_enabled(animate);
        if cx.reduce_motion() != !animate {
            cx.set_reduce_motion(!animate);
        }
        // Appearance is resolved before anything is built: `theme` holds the palette for this frame.
        // A change of appearance eases between the two palettes instead of snapping — a sudden
        // jump between light and dark is exactly the brightness flash the skill warns about.
        let target = t::customize(t::select(&theme, system_dark), setting_str(&state,"SettingsLightAccentColor","#2563EB"), setting_str(&state,"SettingsDarkAccentColor","#6EA8FF"), want_dark);
        let theme_key = (theme.clone(), system_dark);
        match self.theme_key.as_ref() {
            // First frame: adopt the palette outright; there is nothing to travel from.
            None => {
                self.theme_key = Some(theme_key);
                self.shown = target;
            }
            Some(current) if *current != theme_key => {
                self.theme_key = Some(theme_key);
                self.theme_from = self.shown;
                self.theme_started = animate.then(Instant::now);
            }
            Some(_) => {}
        }
        let shown = match self.theme_started {
            Some(started) => {
                let progress = (started.elapsed().as_secs_f32() / THEME_BLEND).min(1.0);
                if progress >= 1.0 {
                    self.theme_started = None;
                    target
                } else {
                    // One more frame, please: the blend is driven by wall time, not by a spring.
                    window.request_animation_frame();
                    t::blend(self.theme_from, target, motion::ease_in_out(progress))
                }
            }
            None => target,
        };
        // `shown` is what the next appearance change travels *from*, so an interrupted switch
        // resumes at the colour that is actually on screen.
        self.shown = shown;
        t::set_current(shown);
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

        // 桌面入口 sits alone at the top; the tools group below; help stays pinned at the bottom.
        let online = notice.starts_with("已连接");
        let mut sidebar = div().flex().flex_col().w(px(SIDEBAR)).flex_shrink_0().h_full().pt(px(20.)).px(px(14.)).pb(px(14.)).gap(px(4.)).bg(t::sidebar());
        sidebar = sidebar.child(group_label("桌面", "01")).child(nav_item(Route::Island, route, cx));
        sidebar = sidebar.child(sidebar_rule());
        sidebar = sidebar.child(group_label("工作台", "02"));
        for item in [Route::Overview, Route::Notifications, Route::Todos, Route::Timers] {
            sidebar = sidebar.child(nav_item(item, route, cx));
        }
        sidebar = sidebar
            .child(div().flex_1().min_h(px(24.)))
            .child(sidebar_rule())
            .child(group_label("系统", "03"))
            .child(nav_item(Route::Settings, route, cx))
            .child(nav_item(Route::About, route, cx))
            .child(sidebar_notes());

        // The scroll container's child must be content-sized: giving it `h_full` pinned it to the
        // viewport, so anything past the fold was clipped instead of becoming scrollable.
        let number = format!("{:02}", route.index() + 1);
        let head = div().flex().items_center().justify_between().gap_4().mb(px(26.))
            .child(
                div().flex().flex_col().flex_1().min_w_0()
                    .child(div().flex().items_center().gap(px(8.0))
                        .child(div().font(t::mono_font()).text_size(px(10.0)).font_weight(FontWeight::MEDIUM).text_color(t::accent()).child(format!("控制中心 · {number}")))
                        .child(div().w(px(20.0)).h(px(1.0)).rounded(px(0.5)).bg(t::accent_dim())))
                    .child(div().mt(px(9.)).font(t::display_font()).text_size(px(30.0)).font_weight(FontWeight::SEMIBOLD).text_color(t::text()).child(title.to_owned()))
                    .child(div().mt(px(7.)).text_size(px(12.5)).text_color(t::text_dim()).child(subtitle.to_owned())),
            )
            .child(div().flex_shrink_0().child(head_actions(route, &state, self, cx)));
        // The heading and the rows enter together, the rows one after another. Both are keyed off
        // the route epoch, so a new page always starts its cascade from zero rather than
        // inheriting whatever the previous page's animations had already finished.
        let head = head.with_animation(("page-head", self.route_epoch), motion::enter(260, 0), |element, progress| {
            motion::rise(element, progress, 10.0)
        });
        let rows=page_rows(route,&state,self,cx);let rows=if route==Route::Timers{rows}else{motion::cascade(rows,self.route_epoch,self.content_epoch,22)};
        let mut column = div().flex().flex_col().w_full().min_w_0().pt(px(26.)).px(px(26.)).pb(px(26.)).child(head);
        column = column.child(div().flex().flex_col().children(rows));

        // Each route owns its offset, so switching routes always starts where that route was left.
        let scroll = self.scrolls[route.index().min(self.scrolls.len() - 1)].clone();
        // While the island is not answering, the status dot breathes. It is the one looping
        // animation in the window: it says "still trying" without asking for attention, and it
        // stops the moment the pipe connects.
        let dot = div().w(px(6.0)).h(px(6.0)).flex_shrink_0().rounded(px(3.0));
        let dot: AnyElement = if online || !motion::enabled() {
            dot.bg(t::accent()).into_any_element()
        } else {
            dot.bg(t::text_faint())
                .with_animation(
                    "status-breath",
                    Animation::new(Duration::from_millis(1400)).repeat_synced(),
                    |element, progress| {
                        let breath = 0.5 - 0.5 * (progress * std::f32::consts::TAU).cos();
                        element.opacity(0.35 + 0.65 * breath)
                    },
                )
                .into_any_element()
        };
        let footer = div().flex().items_center().gap_2().text_size(px(11.5)).text_color(t::text_dim())
            .child(dot)
            .child(if notice.is_empty() { "正在连接灵动岛…".to_owned() } else { notice })
            .child(div().flex_1())
            .child(div().flex().items_center().gap(px(5.0)).text_size(px(10.5)).text_color(t::text_faint())
                .child(icons::tinted("circle-check", t::text_faint()))
                .child("所有更改即时保存"));
        let drawer = drawer_panel(&state, self, cx);
        if self.tutorial_step.is_some(){window.focus(&self.music_focus,cx);}

        div().relative().flex().flex_col().size_full().bg(t::base()).text_color(t::text()).text_size(px(13.0)).font(t::ui_font())
            // The press is taken at the root, not per control: whatever the hover stack says is
            // innermost when the button goes down *is* what got pressed, and releasing it anywhere —
            // including outside the window — has to let it back up.
            .on_mouse_down(MouseButton::Left, cx.listener(|view, _: &MouseDownEvent, _, cx| {
                view.press_began();
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {
                view.press_ended();
                cx.notify();
            }))
            .on_mouse_exit(cx.listener(|view, _: &MouseExitEvent, _, cx| {
                view.press_ended();
                view.hover.clear();
                cx.notify();
            }))
            .child(settings_titlebar())
            .child(div().relative().flex().flex_1().min_h_0().min_w_0()
            .child(sidebar)
            .child(div().w(px(1.0)).flex_shrink_0().h_full().bg(t::hairline()))
            .child(
                div().relative().flex().flex_col().flex_1().min_w_0().h_full()
                    // The glow sits behind the scroll surface; pages paint no background of their
                    // own, so it reads as light falling into the top-right of the content column.
                    .child(page_halo(self.route_epoch))
                    .child(div().id("content").flex_1().min_w_0().overflow_y_scroll().track_scroll(&scroll).child(column))
                    .child(hairline())
                    .child(div().px(px(26.)).pt(px(9.)).pb(px(12.)).min_w_0()
                        .children(if message.is_empty(){None}else{Some(
                            // A fresh id per message: each new line slides in on its own, and an
                            // identical repeat of the last one does not restart it.
                            div().mb_1().text_size(px(12.)).text_color(if message.contains("失败")||message.contains("错误"){t::danger()}else{t::text()})
                                .with_animation(SharedString::from(format!("message-{message}")), motion::enter(200, 0), |element, progress| motion::rise(element, progress, 5.0))
                                .child(message)
                        )})
                        .child(footer)),
            )
            .child(drawer))
            // Popovers are window-level and must paint after the drawer, never behind its surface.
            .child(select_overlay(&state, self, cx))
            .children(tutorial::overlay(&state,self,cx))
    }
}

/// The 2.0 brand mark: one graphite capsule with an amber state dot inside its right half.
fn settings_titlebar()->AnyElement {
    let mut controls=div().flex().items_center().h_full().pr_2().gap_1();
    // No maximize button: the window is a fixed canvas (see `WINDOW_WIDTH`), so the control would
    // be dead. Windows also stops offering the resize border and the titlebar double-click once the
    // window is created non-resizable, so nothing here has to defend the size.
    for (id,area,glyph) in [("window-minimize",WindowControlArea::Min,"\u{e921}"),("window-close",WindowControlArea::Close,"\u{e8bb}")] {
        let close=area==WindowControlArea::Close;
        controls=controls.child(div().id(id).window_control_area(area).w(px(36.)).h(px(28.)).rounded(px(8.)).flex().items_center().justify_center().font_family("Segoe Fluent Icons").text_size(px(10.)).text_color(t::text_dim()).hover(move|s|if close{s.bg(t::danger()).text_color(rgb(0xffffff))}else{s.bg(t::surface_hi()).text_color(t::text())}).child(glyph));
    }
    // The brand mark: a jade island capsule with the amber state dot, lifted off the chrome by its
    // own light. It is the same mark the capsule wears, so the control centre reads as the same app.
    let brand = div().relative().w(px(26.0)).h(px(26.0)).rounded(px(8.0))
        .bg(linear_gradient(145.0, linear_color_stop(t::accent(), 0.0), linear_color_stop(t::accent_deep(), 1.0)))
        .shadow(vec![
            BoxShadow::new(px(0.0), px(5.0), t::accent().opacity(0.28).into()).blur_radius(px(12.0)),
            BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset(),
        ])
        .flex().items_center().justify_center()
        .child(div().flex().items_center().gap(px(2.0))
            .child(div().w(px(11.0)).h(px(4.5)).rounded(px(2.25)).bg(t::on_accent()))
            .child(div().w(px(4.0)).h(px(4.0)).rounded(px(2.0)).bg(t::yellow_line())));
    // The bar centres its children vertically: the version chip used to stretch to the full
    // 52 px height, which left its label pinned to the top of the pill.
    div().flex().items_center().w_full().h(px(52.)).flex_shrink_0().bg(t::chrome()).border_b_1().border_color(t::hairline())
        .child(div().id("window-brand-drag").window_control_area(WindowControlArea::Drag).flex_1().min_w_0().h_full().pl(px(22.)).flex().items_center().gap(px(11.))
            .child(brand)
            .child(div().flex().items_center().gap(px(8.)).text_size(px(13.5))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(t::text()).child("点屿"))
                .child(div().px(px(6.0)).py(px(1.5)).rounded(px(5.0)).bg(t::surface_hi()).text_size(px(9.5)).text_color(t::text_dim()).child("控制中心"))))
        .child(div().mr(px(6.0)).px(px(7.0)).py(px(3.0)).rounded(px(6.0)).bg(t::field()).border_1().border_color(t::hairline())
            .font(t::mono_font()).text_size(px(9.5)).text_color(t::text_faint()).child(format!("v{}", DISPLAY_VERSION)))
        .child(controls).into_any_element()
}

fn nav_item(route: Route, current: Route, cx: &mut Context<SettingsApp>) -> AnyElement {
    let selected = route == current;
    let label = route.nav_label();
    let icon = route.nav_icon();
    let id = SharedString::from(format!("nav-{}", route.index()));
    let click = cx.listener(move |view, _, _, cx| {
        view.go(route);
        cx.notify();
    });
    // The rail's selection is a spring rather than a class: the raised surface, the text, the icon
    // and the accent bar all travel together, and picking another row mid-flight retargets them
    // from wherever they are instead of cutting.
    div()
        .id(SharedString::from(format!("nav-shell-{}", route.index())))
        .w_full()
        .with_spring(
            (SharedString::from("nav-phase"), route.index()),
            motion::phase(motion::standard(), selected),
            move |_, phase| {
                let progress = motion::alpha(phase);
                let text = motion::blend_color(phase, t::text_dim(), t::text());
                let icon_color = motion::blend_color(phase, t::text_faint(), t::accent());
                let mut row = div()
                    .id(id.clone())
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .min_h(px(42.0))
                    .w_full()
                    .px(px(12.0))
                    .py(px(9.0))
                    .rounded(px(12.0))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .text_color(text)
                    
                    .font_weight(if progress > 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                    // No drop shadow on the rail rows: the raised surface, the border and the
                    // accent bar already separate the selected row, and the shadow only smudged
                    // the flat white rail.
                    // The accent bar grows out of the rail as the row is picked: motion that
                    // telegraphs the outcome instead of just interpolating to it.
                    .child(
                        div().absolute().left(px(-8.0)).top(px(12.0)).w(px(3.0))
                            .h(px(6.0 + 12.0 * progress)).rounded(px(1.5))
                            .bg(t::accent()).opacity(progress),
                    )
                    .child(icons::tinted(icon, icon_color))
                    .child(label.to_owned())
                    .on_click(click);
                if !selected {
                    row = row.hover(|style| style.text_color(t::accent()));
                }
                row
            },
        )
        .into_any_element()
}

/// A numbered group label in the rail. The monospaced index is the only Latin the rail carries,
/// and it gives the three groups an unambiguous order without a second heading level.
fn group_label(label: &str, index: &'static str) -> AnyElement {
    div().flex().items_center().gap(px(7.0)).px(px(12.0)).pt(px(6.0)).pb(px(7.0))
        .child(div().font(t::mono_font()).text_size(px(9.0)).text_color(t::text_faint()).child(index.to_owned()))
        .child(div().text_size(px(11.0)).font_weight(FontWeight::MEDIUM).text_color(t::text_dim()).child(label.to_owned()))
        .into_any_element()
}

fn sidebar_rule() -> AnyElement {
    div().mx(px(12.0)).my(px(10.0)).h(px(1.0)).bg(t::hairline()).into_any_element()
}

/// The pinned card at the foot of the rail: a reassurance, not a control.
fn sidebar_notes() -> AnyElement {
    div().mt(px(16.0)).w_full().rounded(px(14.0)).p(px(12.0))
        .bg(t::surface().opacity(0.55)).border_1().border_color(t::hairline())
        .flex().flex_col().gap(px(5.0))
        .child(div().flex().items_center().gap(px(7.0))
            .child(icons::tinted("circle-check", t::accent()))
            .child(div().text_size(px(11.0)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child("修改即时保存")))
        .child(div().text_size(px(10.0)).text_color(t::text_faint()).child("所有设置写入本机，无需确认。"))
        .into_any_element()
}

/// The light in the top-right of the content column.
///
/// Six translucent discs stack into a soft falloff. A single tinted rectangle would show its edge;
/// layered hairline steps of alpha read as a glow, and the window clips the rest.
fn page_halo(epoch: u32) -> AnyElement {
    let accent = t::glow();
    let halo = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let right = f32::from(bounds.origin.x + bounds.size.width) - 30.0;
            let top = f32::from(bounds.origin.y) + 6.0;
            for (index, radius) in [300.0f32, 250.0, 200.0, 150.0, 100.0, 56.0].iter().enumerate() {
                let r = *radius;
                let origin = point(px(right - r), px(top - r));
                let alpha = 0.012 + 0.007 * index as f32;
                window.paint_quad(quad(
                    Bounds::new(origin, size(px(r * 2.0), px(r * 2.0))),
                    px(r),
                    accent.opacity(alpha),
                    px(0.0),
                    transparent_black(),
                    BorderStyle::Solid,
                ));
            }
        },
    )
    .absolute().top_0().right_0().w(px(420.0)).h(px(340.0));
    halo.with_animation(("page-halo", epoch), motion::enter(380, 20), |element, progress| {
        element.opacity(progress)
    }).into_any_element()
}

/// 音乐卡片: one radio group, because the four layouts are alternatives rather than features
/// that can be combined. The choices are plain blocks, not miniatures: a hand-drawn thumbnail
/// can only ever approximate the card, and the live preview above already shows the real thing.
fn music_layout_row(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let selected=setting_str(state,"MusicExpandedLayout","SideBySide");
    let focus=view.music_focus.clone();
    let options=div().flex().flex_col().gap(px(8.))
        .child(div().flex().flex_row().gap(px(8.))
            .child(music_layout_option("SideBySide","紧凑","封面与歌词并排，整体更紧凑。",selected=="SideBySide",view,cx))
            .child(music_layout_option("Separated","舒展","歌词独立成行，长句更从容。",selected=="Separated",view,cx)))
        .child(div().flex().flex_row().gap(px(8.))
            .child(music_layout_option("LyricFocus","歌词聚焦","歌词居中呈现，封面与信息收于顶部。",selected=="LyricFocus",view,cx))
            .child(music_layout_option("LyricRight","右韵","歌词靠右对齐，左侧留白，节奏更轻。",selected=="LyricRight",view,cx)))
        .track_focus(&focus)
        .on_key_down(cx.listener(move |view,event:&KeyDownEvent,window,cx|{
            let current=setting_str(&view.state(),"MusicExpandedLayout","SideBySide").to_owned();
            let values=["SideBySide","Separated","LyricFocus","LyricRight"];let count=values.len();
            let index=values.iter().position(|value|*value==current).unwrap_or(0);let next=match event.keystroke.key.as_str(){"left"=>values[(index+count-1)%count],"right"|"space"=>values[(index+1)%count],_=>return};
            if next!=current{let _=send(&view.shared,"MusicExpandedLayout",next,cx);}
            window.focus(&view.music_focus,cx);cx.notify();
        }));
    let label=match selected{"Separated"=>"舒展","LyricFocus"=>"歌词聚焦","LyricRight"=>"右韵",_=>"紧凑"};
    let footer=div().flex().items_center().gap_2().pt(px(2.))
        .child(div().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(t::accent()))
        .child(div().text_size(px(12.)).text_color(t::text_dim()).child(format!("当前选择：{label}")));
    card("row-music-layout","展开卡片布局","预览跟随当前歌曲、歌词与配色；点选方块切换布局，上方即为真实排版。")
        .flex_col().items_stretch().gap_3().child(music_preview::live(state)).child(options).child(footer).into_any_element()
}

fn music_layout_option(value: &'static str, title: &'static str, detail: &'static str, selected: bool, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let shared=Rc::clone(&view.shared);
    let radio=div().w(px(18.)).h(px(18.)).rounded(px(9.)).border_1()
        .border_color(if selected{t::accent()}else{t::hairline_bright()})
        .flex().items_center().justify_center()
        .child(if selected{div().w(px(10.)).h(px(10.)).rounded(px(5.)).bg(t::accent()).into_any_element()}else{div().into_any_element()});
    let mut tile=div().id(SharedString::from(format!("music-layout-{value}"))).flex_1().min_w_0().flex().flex_col().gap_1().p(px(12.)).rounded(px(12.))
        .border_1().border_color(if selected{t::accent()}else{t::hairline()})
        .bg(if selected{t::row_hover()}else{t::base()})
        .cursor_pointer()
        .child(div().flex().items_center().justify_between().gap_2()
            .child(div().min_w_0().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(title.to_owned()))
            .child(radio))
        .child(div().text_size(px(11.5)).text_color(t::text_dim()).child(detail.to_owned()))
        .on_click(cx.listener(move |view,_,window,cx|{
            let _=send(&shared,"MusicExpandedLayout",value,cx);
            window.focus(&view.music_focus,cx);cx.notify();
        }));
    if !selected{tile=tile.hover(|style| style.bg(t::row_hover()).border_color(t::hairline_bright()));}
    tile.into_any_element()
}

/// 灵动岛 owns the capsule itself: when it appears, how it looks, what feeds it, and the way into
/// the tool pages. Music and lyric preferences live here because only the capsule renders them.
fn island_page_rows(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    let mut sections: Vec<AnyElement> = Vec::new();
    sections.push(section_group("音乐卡片", None, vec![
        mini_mode::material(state,cx),
        mini_mode::row(state,view,cx),
        mini_mode::capsule_material(state,view,cx),
        mini_mode::layout(state,view,cx)
    ], cx));
    sections.push(section_group("系统状态提示", None, vec![
        toggle_row("SystemStateHintsEnabled","系统状态提示","统一提醒大写锁定、小键盘锁定、麦克风静音和触控板状态。",flag(state,"SystemStateHintsEnabled",true),view,cx),
        segmented_row("osd-placement","SystemStateHintPlacement","提示位置","屏幕下方使用独立浮层，不打断岛上的音乐或计时。",&[("Island","灵动岛内"),("Bottom","屏幕下方")],setting_str(state,"SystemStateHintPlacement","Island"),cx),
        system_test_row(cx),
    ],cx));
    sections.push(section_group("何时出现", None, vec![
        toggle_row("HideWhenPaused", "音乐暂停后隐藏", "暂停时收起淡出；有计时器等活动时继续显示当前组件。", flag(state, "HideWhenPaused", true), view, cx),
        toggle_row("AlwaysOnTop", "始终置顶", "显示在其他软件上方，不抢占键盘焦点。", flag(state, "AlwaysOnTop", true), view, cx),
    ], cx));
    sections.push(section_group("预览", None, vec![
        button_row("island-visibility", "显示 / 隐藏灵动岛", "与托盘菜单一致；隐藏后需要时仍会重新出现。", "切换显示", cx, |v, cx| {send_todo(&v.shared, "toggleVisibility", json!({}), cx);cx.notify();}),
        button_row("island-notice-preview", "预览通知横幅", "显示一条本地示例通知，检查横幅外观。", "预览", cx, |v, cx| {send_todo(&v.shared, "notificationPreview", json!({}), cx);cx.notify();}),
        device_test_row(cx),
    ], cx));
    let mut display = vec![
        slider_row("CompactWidth", "固定胶囊宽度", "自适应关闭时使用此宽度。", 140., 480., "px", slider_value(state, "CompactWidth", view, 260.), view, cx),
        toggle_row("AutoCompactWidth", "歌词长度自适应宽度", "按歌词实际宽度平滑伸缩。", flag(state, "AutoCompactWidth", true), view, cx),
    ];
    if flag(state, "AutoCompactWidth", true) {
        display.push(slider_row("AutoCompactMaxWidth", "自适应宽度上限", "只约束歌词自适应，不改变展开卡片尺寸。", 140., 480., "px", slider_value(state, "AutoCompactMaxWidth", view, 420.), view, cx));
    }
    display.push(slider_row("FontSize", "歌词字号", "同时应用于收起胶囊与音乐卡片。", 11., 17., "px", slider_value(state, "FontSize", view, 12.5), view, cx));
    display.push(toggle_row("WordHighlight", "歌词进度涂色", "随演唱进度突出当前歌词。", flag(state, "WordHighlight", true), view, cx));
    display.push(toggle_row("LyricColorFromCover","歌词涂色跟随封面","开启时随封面取色；关闭后使用固定的歌词进度填色。仅影响原文，不给翻译或下一句涂色。",flag(state,"LyricColorFromCover",true),view,cx));
    display.push(toggle_row("CompactTranslation", "胶囊显示翻译", "原文在上、翻译在下；没有翻译时不会留出空行。", flag(state, "CompactTranslation", true), view, cx));
    display.push(toggle_row("ChineseTwoLine", "中文双行模式", "默认开启；没有翻译时，下面显示较小的下一句。", flag(state, "ChineseTwoLine", true), view, cx));
    display.push(toggle_row("Translation", "展开卡片显示翻译", "有翻译时优先显示翻译。", flag(state, "Translation", true), view, cx));
    sections.push(section_group("尺寸与显示", None, display, cx));
    let mut palette: Vec<AnyElement> = vec![
        toggle_row("CoverBackground", "封面取色背景", "根据歌曲封面为展开卡片取色，自动压低亮度；与下方三种颜色一起管理。", flag(state, "CoverBackground", true), view, cx),
    ];
    for (key, title) in [("ExpandedTextColor", "展开卡片字体颜色"), ("ExpandedComponentColor", "展开卡片组件颜色"), ("LyricProgressColor", "歌词进度填色")] {
        palette.push(color_row(key, title, state, view, cx));
        if view.picker.as_ref().is_some_and(|picker| picker.key == key) {
            palette.push(picker_panel(view, cx));
        }
    }
    sections.push(section_group("配色", None, palette, cx));
    let mini=setting_str(state,"IslandSizeMode","Classic")=="Mini";
    let mut interaction=vec![
        button_row("gesture-tutorial","手势教程","跟着动画了解展开、切换与隐藏。","查看教程",cx,|v,cx|{v.tutorial_epoch+=1;v.tutorial_recovery=0;v.tutorial_step=Some(0);cx.notify();}),
        toggle_row("HoverExpand", "悬停展开", "鼠标经过时展开当前组件。", flag(state, "HoverExpand", false), view, cx),
        toggle_row("ClickAnywhereExpand", "点击任意区域展开", "展开当前选中的组件。", flag(state, "ClickAnywhereExpand", true), view, cx),
        toggle_row("SwipeUpHide","上拖隐藏","按住胶囊或展开卡片的空白区域向上拖动隐藏；托盘菜单可重新显示。",flag(state,"SwipeUpHide",true),view,cx),
        toggle_row("SwipeActivities", "左右滑动胶囊切换", "按住收起胶囊或展开卡片左右拖动切换组件。", flag(state, "SwipeActivities", true), view, cx),
        toggle_row("Floating", "自由悬浮", "允许移动灵动岛位置。", flag(state, "Floating", false), view, cx),
        toggle_row("MusicClickOpenApp", "点击封面或歌名打开播放器", "仅影响音乐卡片。", flag(state, "MusicClickOpenApp", false), view, cx),
    ];
    if mini{interaction=vec![toggle_row("SwipeActivities","横向滑动切换","在细条或卡片空白处左右滑动切换组件。",flag(state,"SwipeActivities",true),view,cx),button_row("gesture-tutorial-mini","手势教程","Mini 点击展开，左右滑动切换组件，上滑隐藏。","查看教程",cx,|v,cx|{v.tutorial_epoch+=1;v.tutorial_step=Some(0);cx.notify();}),toggle_row("SwipeUpHide","上拖隐藏","拖动细条或卡片空白处向上隐藏；托盘菜单可找回。",flag(state,"SwipeUpHide",true),view,cx)];}
    sections.push(section_group("交互",None,interaction,cx));
    let position = state["position"].as_f64().unwrap_or(0.);
    sections.push(section_group("歌词与同步", None, vec![
        info_row("当前来源", text(state, "lyricSource", "等待获取"), text(state, "timeline", "等待播放器"), view, cx),
        slider_row("OffsetMilliseconds", "歌词偏移", "正数提前，负数延后。", -5000., 5000., "ms", slider_value(state, "OffsetMilliseconds", view, 0.), view, cx),
        calibrate_row(position, view, cx),
        toggle_row("OnlineSongInfo", "在线补全网易云歌曲信息", "本地信息缺失时直连 music.163.com 补全封面和歌名，不发送账号或令牌。", flag(state, "OnlineSongInfo", true), view, cx),
    ], cx));
    // Everything responds by default, so this page is a grid of exclusions rather than a checklist
    // of permissions: the common case needs no action at all, and a player that is not on the grid
    // still works. The four groups follow how people describe their own players, not session ids.
    let mut grid = div().w_full().min_w_0().rounded(px(16.0)).border_1().border_color(t::hairline()).bg(t::surface())
        .p(px(PANEL_PAD)).flex().flex_col().gap(px(16.0));
    for (label, players) in pipe::PLAYER_GROUPS {
        grid = grid.child(player_group(label, players, state, view, cx));
    }
    grid = grid.child(div().flex().items_center().gap_2()
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(t::accent()))
        .child(div().text_size(px(12.0)).text_color(t::text_dim()).child("默认响应所有播放器；点选后不再唤出灵动岛。浏览器媒体默认不响应。")));
    sections.push(section_block("播放器与来源", None, grid.into_any_element(), cx));
    let mut extras: Vec<AnyElement> = Vec::new();
    extras.push(card("row-custom-add", "其它程序", "输入程序名或媒体会话标识，不支持通配符和启动参数。按 Enter 添加。").child(div().w(px(300.0)).child(view.custom_sources.clone())).into_any_element());
    let custom = custom_sources(state);
    if !custom.is_empty() {
        let mut chips = div().flex().flex_row().flex_wrap().gap_2();
        for id in &custom {
            let owned = id.clone();
            chips = chips.child(
                div().id(SharedString::from(format!("custom-{id}"))).flex().flex_row().items_center().gap_2().px_2().py_1().rounded(px(7.0)).bg(t::field()).border_1().border_color(t::hairline()).text_size(px(11.5)).text_color(t::text_dim())
                    .child(id.clone())
                    .child(div().id(SharedString::from(format!("custom-remove-{id}"))).cursor_pointer().text_color(t::danger()).hover(|style| style.text_color(t::text())).child("×").on_click(cx.listener(move |view, _, _, cx| {
                        let shared = Rc::clone(&view.shared);
                        let remaining: Vec<String> = custom_sources(&view.state()).into_iter().filter(|kept| *kept != owned).collect();
                        let _ = send(&shared, "customSources", &remaining.join("; "), cx);
                        cx.notify();
                    }))),
            );
        }
        extras.push(card("row-custom-list", "已加入的程序", "这些程序不在上面的列表里，同样不会唤出灵动岛。").child(chips).into_any_element());
    }
    extras.push(info_row("当前播放信息", text(state, "player", "等待播放器"), &format!("{}\n{}", text(state, "title", "暂无播放"), text(state, "artist", "")), view, cx));
    sections.push(section_group("其它程序", None, extras, cx));
    sections.push(section_group("组件", None, vec![
        page_link("link-todos", "待办清单", "提醒与清单在控制中心管理。", "打开", Route::Todos, cx),
        page_link("link-timers", "流程与专注", "番茄钟、倒计时与秒表。", "打开", Route::Timers, cx),
        page_link("link-notifications", "通知", "消息回看与通知偏好。", "打开", Route::Notifications, cx),
    ], cx));
    sections
}

fn page_link(id: &'static str, title: &str, detail: &str, label: &str, route: Route, cx: &mut Context<SettingsApp>) -> AnyElement {
    button_row(id, title, detail, label, cx, move |view, cx| {
        view.go(route);
        cx.notify();
    })
}

/// 设置 keeps the preferences that are not about one tool: startup, appearance and sound.
fn settings_page_rows(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    let mut rows: Vec<AnyElement> = Vec::new();
    if view.return_context.is_some() {
        rows.push(button_row("sound-back", "返回", "回到刚才跳转过来的页面。", "返回", cx, |v, cx| v.return_to_context(cx)));
    }
    rows.push(tab_row("settings-tab", &["常规", "外观与动画", "音效"], view.settings_tab, cx, |view, index| {
        view.settings_tab = index;
        // The picker belongs to a row on the tab being left, so it leaves with it rather than
        // animating out under a page it no longer matches.
        view.picker = None;
        view.closing_picker = None;
        view.hex_field = None;
        view.select = None;
    }));
    match view.settings_tab {
        1 => {
            rows.push(plain_group(vec![
                select_row("theme", "界面主题", "跟随 Windows，或固定为浅色 / 深色。", theme_label(state), view, cx),
                toggle_row("Animation", "界面动画", "连续形变可以随时反向操作，由软件开关控制，不跟随 Windows 动画设置。", flag(state, "Animation", true), view, cx),
                toggle_row("IslandFluidMotion", "灵动岛流体动画", "胶囊轻抛后舒展成卡片，收起沿原路回到胶囊；途中反向也保持连续。关闭后使用经典曲线。", flag(state, "IslandFluidMotion", true), view, cx),
            ]));
            let mut motion: Vec<AnyElement> = Vec::new();
            for (key, title, detail, min, max, default, unit) in [
                ("MotionGap", "内容间距", "调整展开后的内容留白。", 0., 24., 8., "px"),
                ("MotionDuration", "展开响应", "调整展开速度。", 250., 1200., 480., "ms"),
                ("MotionCollapseDuration", "收起响应", "调整收起速度。", 150., 700., 440., "ms"),
                ("MotionRadius", "卡片圆角", "胶囊形变后平滑落到这个圆角。", 8., 48., 30., "px"),
                ("SpringStiffness", "弹簧刚度", "越高响应越快。", 100., 1000., 420., ""),
                ("SpringDamping", "弹簧阻尼", "越高回弹越少。", 10., 80., 30., ""),
                ("SpringMass", "弹簧质量", "调整运动的重量感。", 0.3, 2., 0.8, ""),
            ] {
                motion.push(slider_row(key, title, detail, min, max, unit, slider_value(state, key, view, default), view, cx));
            }
            motion.push(action_row("resetMotion", "还原动效", "恢复展开、收起、弹簧与圆角的默认值，并开启界面与流体动画。", "还原动效", view, cx));
            motion.push(action_row("resetAppearance", "恢复紧凑尺寸", "恢复宽度、字号与固定位置。", "恢复紧凑尺寸", view, cx));
            rows.push(section_group("动效", None, motion, cx));
        }
        2 => {
            rows.push(section_group("通知音效", None, sound_rows("Notification", state, view, cx), cx));
            rows.push(section_group("待办与计时器", None, sound_rows("Todo", state, view, cx), cx));
            rows.push(section_group("设备连接", None, sound_rows("Device", state, view, cx), cx));
            rows.push(plain_group(vec![info_row("音量说明", "内置短音已提高基础响度", "以下滑块只调整点屿，不修改 Windows 系统音量。0% 为静音，可单独试听。", view, cx)]));
        }
        _ => {
            // One panel, not two: nothing titles either half, so a gap between them reads as a
            // crack in the page rather than as a section break.
            rows.push(plain_group(vec![
                select_row("startPage", "打开时显示的页面", "下次打开控制中心时直接进入这一页。", start_page_label(state), view, cx),
                select_row("theme", "界面主题", "跟随 Windows，或固定为浅色 / 深色。", theme_label(state), view, cx),
                toggle_row("AutoStart", "开机启动", "默认开启，登录 Windows 后驻留托盘；也可右键托盘图标切换。", flag(state, "AutoStart", true), view, cx),
                toggle_row("AlwaysOnTop", "灵动岛保持置顶", "显示在其他软件上方，不抢占键盘焦点。", flag(state, "AlwaysOnTop", true), view, cx),
                info_row("偏好保存在本机", "调整会自动保存", "设置写入点屿的数据目录；重装或版本更新不会重置。", view, cx),
                info_row("数据与隐私", "默认不离开本机", "待办、计时与通知记录只保存在本机；唯一的外部请求是在线补全歌曲信息。", view, cx),
            ]));
            let mut colors=Vec::new();
            for (key,title) in [("SettingsLightAccentColor","浅色主题色"),("SettingsDarkAccentColor","深色主题色")]{
                colors.push(color_row(key,title,state,view,cx));
                if view.picker.as_ref().is_some_and(|p|p.key==key){colors.push(picker_panel(view,cx));}
            }
            colors.push(action_row("resetSettingsColors","还原主题配色","恢复浅色与深色主题原有的蓝色。","还原配色",view,cx));
            rows.push(section_group("主题配色",None,colors,cx));
            rows.push(section_group("快速提醒", None, quick_ui::quick_hotkey_rows(state, view, cx), cx));
        }
    }
    rows
}

fn about_page_rows(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    let _ = state;
    vec![
        section_group("版本", None, vec![
            info_row(&format!("DotIsle · 点屿 {}", DISPLAY_VERSION), "原生胶囊 · GPUI 控制中心 · Rust 后台", "内置完整 Noto Sans SC 与 Noto Sans JP，均采用 SIL OFL 1.1 许可。", view, cx),
            toggle_row("AutoCheckUpdatesEnabled","自动检查更新","默认开启，只检查并在灵动岛提醒；下载和安装由你确认。",flag(state,"AutoCheckUpdatesEnabled",true),view,cx),
            info_row("更新状态",state["softwareUpdate"]["phase"].as_str().unwrap_or("idle"),state["softwareUpdate"]["message"].as_str().unwrap_or("发现新版时将在灵动岛提醒。"),view,cx),
            action_row("checkUpdate", "在线更新", "同时请求国内源和 GitHub，检查最新正式版本。", "检查更新", view, cx),
            action_row("showUpdate","岛上更新进度","重新打开当前更新任务，查看下载、校验与待安装状态。","查看进度",view,cx),
            action_row("installUpdate", "下载更新", "下载并校验后，在灵动岛选择重启并安装；旧单文件版将打开安装目录选择向导。", "下载更新", view, cx),
        ], cx),
        section_group("诊断", None, vec![
            action_row("collectLogs", "抓取日志", "汇总版本、最近错误、日志中的错误行与日志尾部，生成一个文本文件并在资源管理器中选中，反馈问题时直接附上。", "抓取日志", view, cx),
        ], cx),
        section_group("文档", None, vec![
            button_row("about-changelog", "更新日志", "每个版本的用户可见变化。", "查看", cx, |v, cx| {v.drawer = Some(Drawer::Changelog);cx.notify();}),
            button_row("about-licenses", "开源许可", "第三方组件与字体的许可信息。", "查看", cx, |v, cx| {v.drawer = Some(Drawer::Licenses);cx.notify();}),
        ], cx),
        section_group("链接", None, vec![
            action_row("openGitee", "国内发布页", "点屿的 Gitee 国内镜像与下载。", "Gitee", view, cx),
            action_row("openGitHub", "项目主页", "查看源码、发行版本与反馈问题。", "GitHub", view, cx),
        ], cx),
        plain_group(vec![info_row("更新来源", "Gitee 国内源 + GitHub", "同时检查两个源；一个不可用时使用另一个。下载校验完成后才替换原程序。", view, cx)]),
    ]
}

fn page_rows(route: Route, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    match route {
        Route::Island => island_page_rows(state, view, cx),
        Route::Overview => overview_rows(state, view, cx),
        Route::Notifications => notify_page_rows(state, view, cx),
        Route::Todos => new_todo_rows(state, view, cx),
        Route::Timers => timer_rows(state, view, cx),
        Route::Settings => settings_page_rows(state, view, cx),
        Route::About => about_page_rows(state, view, cx),
    }
}

/// The buttons that sit beside the heading. Only routes with real actions grow a head.
fn head_actions(route: Route, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let mut row = div().flex().items_center().gap_2().flex_shrink_0();
    match route {
        Route::Todos => {
            row = row
                .child(pill_button_icon("todo-quick".into(), Some("alarm-clock"), "一句话提醒", false, cx.listener(|view, _, _, cx| { view.drawer = Some(Drawer::QuickReminder); cx.notify(); })))
                .child(pill_button_icon("todo-new".into(), Some("plus"), "新建", true, cx.listener(|view, _, _, cx| view.open_todo_form(None, cx))))
                .child(gear_button("todo-gear", Drawer::TodoSettings, cx));
        }
        Route::Notifications => {
            if view.notice_view == 1 {
                row = row
                    .child(pill_button_icon("rule-new".into(), Some("plus"), "新建规则", true, cx.listener(|view, _, _, cx| view.open_rule_form(None, cx))))
                    .child(gear_button("notice-gear", Drawer::NotificationSettings, cx));
            } else {
                let paused = flag(state, "PauseNotifications", false);
                row = row
                    .child(pill_button_icon(SharedString::from("notice-pause"), Some(if paused { "play" } else { "pause" }), if paused { "恢复通知" } else { "暂停通知" }, paused, cx.listener(|view, _, _, cx| {send_todo(&view.shared, "toggleNotifications", json!({}), cx);cx.notify();})))
                    .child(gear_button("notice-gear", Drawer::NotificationSettings, cx));
            }
        }
        Route::Timers => {
            row = row.child(gear_button("timer-gear", Drawer::TimerSettings, cx));
        }
        _ => {}
    }
    row.into_any_element()
}

/// The handoff's `di-icon-button`: a 36px circle on the panel surface.
fn gear_button(id: &'static str, drawer: Drawer, cx: &mut Context<SettingsApp>) -> AnyElement {
    div()
        .id(id)
        .w(px(36.0))
        .h(px(36.0))
        .flex_shrink_0()
        .rounded(px(11.0))
        .bg(t::surface())
        .border_1()
        .border_color(t::hairline())
        .shadow(vec![BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset()])
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(t::text_dim())
        .hover(|style| style.bg(t::surface_hi()).border_color(t::hairline_bright()).text_color(t::text()))
        .child(icons::tinted("settings-2", t::text()))
        .on_click(cx.listener(move |view, _, _, cx| {view.drawer = Some(drawer.clone());cx.notify();}))
        .into_any_element()
}

fn drawer_heading(drawer: &Drawer) -> (&'static str, &'static str) {
    match drawer {
        Drawer::TodoEditor => ("待办编辑", "题目必填；按 Enter 保存。"),
        Drawer::TodoSettings => ("待办设置", "列表偏好与提醒行为。"),
        Drawer::Snooze(_) => ("稍后提醒", "到点后再提醒这条待办。"),
        Drawer::Focus(_) => ("开始专注", "用一个番茄钟专注这件事。"),
        Drawer::NoticeDetail(_) => ("消息详情", "来自通知历史。"),
        Drawer::NotificationSettings => ("通知设置", "应用通知、历史记录与设备提醒。"),
        Drawer::TimerSettings => ("计时设置", "保存方式与提醒音效入口。"),
        Drawer::Changelog => ("更新日志", "每个版本的用户可见变化。"),
        Drawer::Licenses => ("开源许可", "点屿使用的开源组件与字体。"),
        Drawer::QuickReminder => ("一句话提醒", "用一句话安排下一件小事。"),
        Drawer::WorkflowEditor => ("新建流程", "2–12 个阶段，每个阶段等待你点击下一阶段。"),
        Drawer::RuleEditor(_) => ("应用提醒规则", "进入软件或前台累计达到时间时提醒。"),
    }
}

fn drawer_rows(drawer: &Drawer, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    match drawer {
        Drawer::TodoEditor => {
            let mut rows = vec![todo_form(view, cx)];
            if let Some(id) = view.todo_editing.clone() {
                rows.push(button_row("todo-delete-edit", "移除待办", "删除此条待办。", "删除", cx, move |v, cx| {send_todo(&v.shared, "todoDelete", json!({"id": id.clone()}), cx);v.drawer = None;v.todo_editing = None;cx.notify();}));
            }
            rows
        }
        Drawer::TodoSettings => todo_settings_rows(state, view, cx),
        Drawer::Snooze(id) => snooze_rows(id, state, view, cx),
        Drawer::Focus(id) => focus_rows(id, state, view, cx),
        Drawer::NoticeDetail(id) => notice_detail_rows(id, state, view, cx),
        Drawer::NotificationSettings => notification_settings_rows(state, view, cx),
        Drawer::TimerSettings => timer_settings_rows(state, view, cx),
        Drawer::Changelog => history::rows(),
        Drawer::Licenses => licenses_rows(view, cx),
        Drawer::QuickReminder => quick_ui::quick_reminder_rows(state, view, cx),
        Drawer::WorkflowEditor => workflow_editor_rows(state, view, cx),
        Drawer::RuleEditor(_) => rules_ui::rule_editor_rows(state, view, cx),
    }
}

fn licenses_rows(view: &SettingsApp, cx: &Context<SettingsApp>) -> Vec<AnyElement> {
    vec![
        info_row("开源来源与许可", "按实际使用保留版权和许可声明", "TaskbarLyrics、AF-Media-Bar、NetEase-Cloud-Music-DiscordRPC 的移植来源继续保留。\nLyricify-Lyrics-Helper 的协议移植按 Apache-2.0 保留归属。\n详情见发行目录中的 LICENSE、THIRD-PARTY-NOTICES.md 与 licenses/。", view, cx),
        info_row("内置字体", "Noto Sans SC / Noto Sans JP", "均采用 SIL OFL 1.1 许可，随安装包完整内置。", view, cx),
        info_row("界面组件", "GPUI 0.3.9", "控制中心使用 GPUI，支持主题切换、即时调色与自动保存。", view, cx),
    ]
}

/// A stable small number per drawer kind, used as the spring's identity: switching between two
/// drawers starts a fresh spring, reopening the same one picks up where it left off.
fn drawer_tag(drawer: &Drawer) -> usize {
    match drawer {
        Drawer::TodoEditor => 0,
        Drawer::TodoSettings => 1,
        Drawer::Snooze(_) => 2,
        Drawer::Focus(_) => 3,
        Drawer::NoticeDetail(_) => 4,
        Drawer::NotificationSettings => 5,
        Drawer::TimerSettings => 6,
        Drawer::Changelog => 7,
        Drawer::Licenses => 8,
        Drawer::QuickReminder => 9,
        Drawer::WorkflowEditor => 10,
        Drawer::RuleEditor(_) => 11,
    }
}

/// The single overlay: a scrim over the body plus a 385px panel on the right. One drawer at a
/// time, never stacked, and the same panel serves every entry point.
fn drawer_panel(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    // A dismissed drawer stays mounted until its exit spring settles, so it can travel back out
    // the way it came in. Without this the panel would be gone on the very frame the click landed,
    // which is the one transition the skill calls out as feeling like a cut (§7).
    let (drawer, open) = match (&view.drawer, &view.closing) {
        (Some(drawer), _) => (drawer.clone(), true),
        (None, Some(drawer)) => (drawer.clone(), false),
        (None, None) => return div().into_any_element(),
    };
    let tag = drawer_tag(&drawer);
    let (title, subtitle) = match &drawer {
        Drawer::TodoEditor if view.todo_editing.is_some() => ("编辑待办", "改动会覆盖原来的内容；按 Enter 保存。"),
        Drawer::TodoEditor => ("新建待办", "题目必填；日期与时间使用本地时区。"),
        other => drawer_heading(other),
    };
    let rows = motion::cascade(drawer_rows(&drawer, state, view, cx), tag as u32, tag as u32, 16);
    let scroll = view.drawer_scroll.clone();
    // A drawer is a sheet: a little bounce as it lands, because it was thrown in from the edge.
    // The scrim is a different thing and gets a different spring — see `motion::scrim`.
    let spring = || motion::phase_from(motion::sheet(), open, false);
    let scrim = || motion::phase_from(motion::scrim(), open, false);
    let panel = div().id("drawer-panel").w(px(if matches!(drawer,Drawer::WorkflowEditor){1000.}else{385.0})).max_w_full().flex_1().min_h_0().flex_shrink_0().flex().flex_col()
        .rounded(px(18.0)).border_1().border_color(t::hairline())
        .bg(linear_gradient(180.0, linear_color_stop(t::panel_top(), 0.0), linear_color_stop(t::panel_bottom(), 1.0)))
        .shadow(vec![
            BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset(),
            BoxShadow::new(px(-10.0), px(0.0), t::shadow().opacity(0.9).into()).blur_radius(px(40.0)),
        ])
        .overflow_hidden()
        .child(
            div().flex().items_start().justify_between().gap(px(12.0)).px(px(23.0)).pt(px(23.0)).pb(px(25.0))
                .child(
                    div().flex().flex_col().min_w_0()
                        .child(div().font(t::display_font()).text_size(px(19.0)).font_weight(FontWeight::SEMIBOLD).text_color(t::text()).child(title.to_owned()))
                        .child(div().mt(px(5.0)).text_size(px(12.0)).text_color(t::text_dim()).child(subtitle.to_owned())),
                )
                .child(
                    div().id("drawer-close").w(px(34.0)).h(px(34.0)).flex_shrink_0().rounded(px(10.0)).flex().items_center().justify_center().cursor_pointer().text_color(t::text_dim())
                        .bg(t::field()).border_1().border_color(t::hairline())
                        .hover(|style| style.bg(t::surface_hi()).text_color(t::text()))
                        .child(icons::icon("x"))
                        .on_click(cx.listener(|view, _, _, cx| view.close_drawer(cx))),
                ),
        )
        .child(div().id("drawer-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&scroll).px(px(if matches!(drawer,Drawer::WorkflowEditor){23.}else{PANEL_PAD})).pb(px(23.0)).child(stacked(rows, false)));
    let panel=if matches!(drawer,Drawer::WorkflowEditor){panel.child(workflow_footer(view,cx))}else{panel};
    // GPUI routes a wheel event to every scroll container under the cursor, so without an
    // occluding hitbox the page behind keeps scrolling while the drawer scrolls.
    div().id("drawer").absolute().inset_0().flex().flex_row().occlude()
        .child(
            div().id("drawer-scrim").flex_1().h_full().bg(t::scrim())
                .on_click(cx.listener(|view, _, _, cx| view.close_drawer(cx)))
                .with_spring(("drawer-scrim", tag), scrim(), |element, phase| {
                    element.opacity(motion::alpha(phase))
                }),
        )
        // The sheet floats clear of the window's edges: a 10px gutter all round tells the eye it
        // was thrown on top of the page rather than docked into it.
        .child(
            div().flex().flex_col().h_full().py(px(10.0)).pr(px(10.0))
                .child(panel.with_spring(("drawer-panel", tag), spring(), |element, phase| {
                    motion::slide_in(element, phase.0, 44.0)
                })),
        )
        .into_any_element()
}

/// How long the window takes to travel between two palettes. Long enough to read as a change of
/// material, short enough that it never feels like a loading state.
const THEME_BLEND: f32 = 0.26;
/// How long a dismissed surface stays mounted while its exit spring runs. The springs themselves
/// settle sooner; this is only the upper bound that keeps a panel alive long enough to leave.
const EXIT_MS: u64 = 380;

impl SettingsApp {
    /// The rows under a page's tab bar were replaced: restart their entrance cascade.
    fn pulse(&mut self) {
        self.content_epoch = self.content_epoch.wrapping_add(1);
    }

    /// Navigate. Every entry point goes through here, so the page transition, the overlay teardown
    /// and the entrance cascade can never drift apart.
    fn go(&mut self, route: Route) {
        if self.route != route {
            self.route = route;
            self.route_epoch = self.route_epoch.wrapping_add(1);
            self.pulse();
        }
        self.drawer = None;
        self.closing = None;
        self.picker = None;
        self.closing_picker = None;
        self.return_context = None;
        self.select = None;
    }

    /// Close the drawer, but keep it mounted for one exit spring so it leaves along the path it
    /// arrived on. A panel that disappears the instant the click lands reads as a cut, not a close.
    fn close_drawer(&mut self, cx: &mut Context<Self>) {
        if let Some(drawer) = self.drawer.take() {
            self.closing = Some(drawer);
            let weak = cx.entity().downgrade();
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_millis(EXIT_MS)).await;
                let _ = weak.update(cx, |view, cx| {
                    view.closing = None;
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// The picker grew out of its row, so it shrinks back into it rather than blinking away.
    fn close_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.take() {
            self.closing_picker = Some(picker);
            let weak = cx.entity().downgrade();
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_millis(EXIT_MS)).await;
                let _ = weak.update(cx, |view, cx| {
                    view.closing_picker = None;
                    cx.notify();
                });
            })
            .detach();
        }
        self.hex_field = None;
        cx.notify();
    }

    /// Jump to 设置 · 音效, remembering where the jump came from so the way back stays visible.
    fn go_sound(&mut self, cx: &mut Context<Self>) {
        let back = (self.route != Route::Settings).then(|| (self.route, self.drawer.take()));
        self.go(Route::Settings);
        if let Some(context) = back {
            self.return_context = Some(context);
        }
        self.settings_tab = 2;
        cx.notify();
    }

    fn return_to_context(&mut self, cx: &mut Context<Self>) {
        if let Some((route, drawer)) = self.return_context.take() {
            self.go(route);
            self.drawer = drawer;
            self.settings_tab = 0;
            cx.notify();
        }
    }
}

/// Whether a built-in player is currently kept silent. Absent the setting, only the browser is:
/// a video in a tab is the one source that would otherwise take the island over uninvited.
fn source_excluded(state: &Value, id: &str) -> bool {
    setting(state, "IgnoredSources")
        .as_array()
        .map(|list| list.iter().any(|entry| entry.as_str().is_some_and(|entry| entry.eq_ignore_ascii_case(id))))
        .unwrap_or_else(|| id.eq_ignore_ascii_case("Browser"))
}

/// Custom (non built-in) entries from the exclusion list.
fn custom_sources(state: &Value) -> Vec<String> {
    setting(state, "IgnoredSources")
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter(|id| !pipe::player_ids().any(|known| known.eq_ignore_ascii_case(id)))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// One group of player tiles under its own 12px heading.
///
/// The grid wraps instead of stretching: a row of five tiles has to look the same as a row of
/// three, and `flex_1` would silently make the short row's tiles wider than the long row's.
fn player_group(label: &str, players: &[(&'static str, &'static str)], state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let mut row = div().flex().flex_row().flex_wrap().gap(px(8.0));
    for &(id, name) in players {
        row = row.child(player_tile(id, name, source_excluded(state, id), view, cx));
    }
    div().flex().flex_col().gap(px(8.0))
        .child(div().text_size(px(12.0)).text_color(t::text_dim()).child(label.to_owned()))
        .child(row)
        .into_any_element()
}

/// One player, as a tile rather than a full-width row.
///
/// The trailing mark carries the state twice over — filled tick for responding, hollow cross for
/// silent — because a page where nearly everything is "on" cannot rely on colour alone to show the
/// few that are not. Pressing it is the whole interaction, so the tile washes on hover and keeps its
/// border; nothing here needs a switch, which would add a second hit target to a 54px box.
fn player_tile(id: &'static str, name: &'static str, excluded: bool, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let shared = Rc::clone(&view.shared);
    let mark = div().w(px(18.0)).h(px(18.0)).flex_shrink_0().rounded(px(9.0))
        .flex().items_center().justify_center().text_size(px(11.0))
        .border_1().border_color(if excluded { t::hairline_bright() } else { t::accent() })
        .bg(if excluded { t::base() } else { t::accent() })
        .text_color(if excluded { t::text_faint() } else { t::on_accent() })
        .child(if excluded { "×" } else { "✓" });
    // 144 + 8 gaps is what lets the widest group (six tiles) hold one line inside the 915px panel:
    // a group that wrapped to a lone tile on its own row would read as a mistake rather than a grid.
    let tile = div().id(SharedString::from(format!("player-{id}")))
        .w(px(144.0)).flex_shrink_0().flex().items_center().justify_between().gap_2()
        .px(px(10.0)).py(px(10.0)).rounded(px(12.0))
        .border_1().border_color(t::hairline()).bg(t::base())
        .cursor_pointer()
        .child(div().min_w_0().overflow_hidden().text_size(px(13.0)).text_color(if excluded { t::text_dim() } else { t::text() }).child(name))
        .child(mark)
        .hover(|style| style.bg(t::row_hover()).border_color(t::hairline_bright()))
        .on_click(cx.listener(move |_view, _, _, cx| {
            let _ = send(&shared, &format!("source:{id}"), if excluded { "false" } else { "true" }, cx);
            cx.notify();
        }));
    tile.into_any_element()
}

fn notification_allowed(state: &Value, id: &str) -> bool {
    setting(state, "NotificationAllowedApps")
        .as_array()
        .map(|list| list.iter().any(|entry| entry.as_str().is_some_and(|entry| entry.eq_ignore_ascii_case(id))))
        .unwrap_or(matches!(id, "QQNT" | "QQ"))
}

/// A row whose whole surface toggles a membership (players, notification apps).
///
/// Goes through the same liveness shell as every other row rather than styling `.hover()`
/// itself: these sit directly above the toggle rows on the 通知 page, and a row that snapped
/// while the one beside it sprang read as two different interfaces.
fn check_row(id: SharedString, title: &str, detail: &str, checked: bool, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let key = id.to_string();
    let mark = SharedString::from(format!("check-{key}"));
    let spring_key = SharedString::from(format!("check-{key}"));
    let row = card(id, title, detail)
        .cursor_pointer()
        .child(
            // The circle fills and the tick fades in together on one spring, so a source that is
            // switched on reads as a single gesture rather than two separate repaints.
            div().with_spring(mark, motion::phase(motion::snappy(), checked), |_, phase| {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(11.0))
                    .bg(motion::blend_color(phase, t::track(), t::accent()))
                    .text_size(px(12.0))
                    .text_color(motion::blend_color(phase, t::text_faint(), t::on_accent()))
                    .child(div().opacity(motion::alpha(phase)).child("✓"))
            }),
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            let shared = Rc::clone(&view.shared);
            let _ = send(&shared, &key, if checked { "false" } else { "true" }, cx);
            cx.notify();
        }));
    live_shell(row, spring_key, view, cx, |row, liveness| row_wash(row, liveness, true))
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
                .bg(t::field())
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

/// A small secondary button, used inside rows and drawers.
fn small_button(
    id: SharedString,
    label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .px(px(11.0))
        .py(px(6.0))
        .rounded(px(9.0))
        .cursor_pointer()
        .bg(t::surface())
        .border_1()
        .border_color(t::hairline())
        .text_size(px(12.5))
        .text_color(t::text())
        .hover(|style| style.bg(t::surface_hi()))
        .active(|style| style.opacity(0.85))
        .child(label.to_owned())
        .on_click(on_click)
}

/// The handoff's `di-button`: 36px tall, 10px radius, panel surface with a hairline border, or the
/// green primary fill. The height is explicit so a button never stretches to a sibling's height.
fn pill_button(
    id: SharedString,
    label: &str,
    filled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    pill_button_icon(id, None, label, filled, on_click)
}

fn pill_button_icon(
    id: SharedString,
    icon: Option<&'static str>,
    label: &str,
    filled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let foreground = if filled { t::on_accent() } else { t::text() };
    let border = if filled { t::accent_deep() } else { t::hairline() };
    let mut button = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(7.0))
        .min_h(px(36.0))
        .px(px(13.0))
        .py(px(8.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(border)
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(foreground)
        .cursor_pointer();
    if filled {
        button = button
            .bg(linear_gradient(180.0, linear_color_stop(t::accent(), 0.0), linear_color_stop(t::accent_deep(), 1.0)))
            .shadow(vec![
                BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset(),
                BoxShadow::new(px(0.0), px(5.0), t::accent().opacity(0.30).into()).blur_radius(px(12.0)),
            ])
            .hover(|style| style.opacity(0.93))
            .active(|style| style.opacity(0.82));
    } else {
        button = button
            .bg(t::surface())
            .shadow(vec![BoxShadow::new(px(0.0), px(1.0), t::sheen().into()).blur_radius(px(0.0)).inset()])
            .hover(|style| style.bg(t::surface_hi()).border_color(t::hairline_bright()))
            .active(|style| style.opacity(0.82));
    }
    button
        .children(icon.map(|name| icons::tinted(name, foreground)))
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

/// A labelled segmented control in the handoff's `di-tab` dress, used for the chime choice.
fn segmented_row(
    id_prefix: &'static str,
    key: &'static str,
    title: &str,
    detail: &str,
    choices: &'static [(&'static str, &'static str)],
    current: &str,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let mut segments = div().flex().flex_row().gap(px(2.0)).p(px(3.0)).rounded(px(11.0))
        .bg(t::field()).border_1().border_color(t::hairline());
    for (value, label) in choices {
        let value: &'static str = *value;
        let label: &'static str = *label;
        let selected = current == value;
        let click = cx.listener(move |view, _, _, cx| {
            let shared = Rc::clone(&view.shared);
            let _ = send(&shared, key, value, cx);
            cx.notify();
        });
        segments = segments.child(
            div().id((SharedString::from(format!("{id_prefix}-{value}")), 1usize)).with_spring(
                (SharedString::from(format!("{id_prefix}-{value}")), 0usize),
                motion::phase(motion::snappy(), selected),
                move |_, phase| {
                    let lift = motion::alpha(phase);
                    div()
                        .id(SharedString::from(format!("{id_prefix}-{value}")))
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(8.0))
                        .cursor_pointer()
                        .bg(motion::blend_color(phase, transparent_black().into(), t::accent()))
                        .text_size(px(12.5))
                        .text_color(motion::blend_color(phase, t::text_dim(), t::on_accent()))
                        .font_weight(if lift > 0.5 { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                        .shadow(vec![BoxShadow::new(px(0.0), px(1.0), t::shadow().opacity(lift).into()).blur_radius(px(2.0))])
                        .active(|style| style.opacity(0.72))
                        .child(label.to_owned())
                        .on_click(click)
                },
            ),
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
    state: &Value,
    view: &SettingsApp,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let todo=prefix=="Todo";
    let (enabled_key,kind_key,volume_key,path_key,detail)=match prefix{
        "Todo"=>("TodoSoundEnabled","TodoSound","TodoVolume","TodoSoundPath","待办到期、计时完成或番茄钟切换阶段时播放。"),
        "Device"=>("DeviceSoundEnabled","DeviceSound","DeviceVolume","DeviceSoundPath","设备连接或移除横幅出现时播放。"),
        _=>("NotificationSoundEnabled","NotificationSound","NotificationVolume","NotificationSoundPath","收到应用通知时播放。"),
    };
    let default_kind = if prefix=="Device"{"Connect"}else if todo { "Bell" } else { "Soft" };
    let default_volume = if todo { 45.0 } else { 35.0 };
    let current = setting_str(state, kind_key, default_kind).to_owned();
    let mut rows = vec![
        toggle_row(enabled_key, "启用提醒音效", detail, flag(state, enabled_key, true), view, cx),
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

/// The add / edit form, drawn as the handoff's flat stack of labelled fields. The entities are
/// owned by the settings view, so re-rendering never costs the user their typing.
fn todo_form(view: &SettingsApp, cx: &mut Context<SettingsApp>) -> AnyElement {
    let editing = view.todo_editing.is_some();
    let daily = view.todo_daily;
    let labelled = |label: &str, body: AnyElement| div()
        .flex()
        .flex_col()
        .gap(px(7.0))
        .w_full()
        .child(div().text_size(px(14.0)).text_color(t::text()).child(label.to_owned()))
        .child(body);
    div()
        .id("row-todo-form")
        .flex()
        .flex_col()
        .gap(px(17.0))
        .w_full()
        .child(labelled("题目", view.todo_title.clone().into_any_element()))
        .child(labelled("内容", view.todo_content.clone().into_any_element()))
        .child(labelled("提醒日期与时间", div()
            .flex()
            .flex_row()
            .gap_3()
            .child(div().flex_1().min_w_0().child(view.todo_date.clone()))
            .child(div().w(px(110.0)).flex_shrink_0().child(view.todo_time.clone()))
            .into_any_element()))
        .child(labelled("颜色", select_control("todo-color", &color_name(&view.todo_color), view.select == Some("todo-color"), None, Rc::clone(&view.select_bounds), cx)))
        .child(
            div()
                .id("todo-daily")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .pt(px(16.0))
                .pb(px(16.0))
                .border_b_1()
                .border_color(t::hairline())
                .cursor_pointer()
                .child(div().text_size(px(14.0)).text_color(t::text()).child("每日重复"))
                .child(switch("todo-daily", daily))
                .on_click(cx.listener(|view, _, _, cx| {
                    view.todo_daily = !view.todo_daily;
                    cx.notify();
                })),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .pt(px(14.0))
                .child(pill_button(
                    "todo-cancel".into(),
                    "取消",
                    false,
                    cx.listener(|view, _, _, cx| view.close_todo_form(cx)),
                ))
                .child(pill_button(
                    "todo-save".into(),
                    if editing { "保存修改" } else { "保存" },
                    true,
                    cx.listener(|view, _, _, cx| view.submit_todo(cx)),
                )),
        )
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
                .bg(t::field())
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
    card(
        "row-calibrate",
        "播放位置校准",
        "输入播放器上显示的秒数，按 Enter 校准；右侧按钮每次微调 1 或 5 秒。仅在备用本地计时时使用。",
    )
    .child(
        // Wraps instead of clipping: the field used to be a fixed 228 DIP wide with a 20-character
        // placeholder, so the instruction was cut mid-word and the only way to read it was to
        // guess. The instruction now lives in the row's detail text where it can be any length.
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap_2()
            .child(div().text_size(px(12.0)).text_color(t::accent()).child(format!("{position:.0} 秒")))
            .child(div().w(px(112.0)).child(view.calibrate.clone()))
            .child(stepper),
    )
        .into_any_element()
}

/// One labelled row inside a panel: title, optional 12px detail, and the control on the right. The
/// surrounding panel owns the surface and the dividers, so the row itself is only rhythm.
///
/// The negative margin cancels the panel's padding and the padding puts it back, so the content
/// stays exactly where it was while the row's own box — and therefore its hover fill — reaches the
/// panel's border. That is the whole point: a highlight that stops short of the frame reads as a
/// shadow that missed. Both numbers come from [`PANEL_PAD`], so they cannot drift.
fn card(id: impl Into<ElementId>, title: &str, detail: &str) -> Stateful<Div> {
    tally();
    let mut body = div().flex().flex_col().gap_1().flex_1().min_w_0()
        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(title.to_owned()));
    if !detail.is_empty() {
        body = body.child(div().text_size(px(11.5)).text_color(t::text_dim()).child(detail.to_owned()));
    }
    div().id(id).flex().flex_row().items_center().justify_between().gap(px(15.0)).py(px(15.0)).min_w_0()
        .mx(px(-PANEL_PAD)).px(px(PANEL_PAD))
        .child(body)
}

/// Paint one control that springs toward the pointer.
///
/// The shell owns the listeners and the spring; `build` draws the control at the current liveness,
/// so the wash, the bar and the sink are one gesture rather than three class swaps. Pressing while
/// already hovering retargets the *same* spring from wherever the wash got to, which a style swap
/// cannot do — that carry-through is the whole reason this exists instead of another `.hover()`.
///
/// The hover listener is reported from both edges: gpui fires it on the way in *and* the way out,
/// and also when a layout change moves a control under a stationary mouse — precisely the case a
/// hand-rolled hit test gets wrong, and why the pointer state is delegated rather than rebuilt from
/// recorded bounds.
fn live_on(
    shell: Div,
    key: &'static str,
    view: &SettingsApp,
    cx: &Context<SettingsApp>,
    build: impl Fn(f32) -> AnyElement + 'static,
) -> AnyElement {
    let id = SharedString::from(format!("live-{key}"));
    let tracked = SharedString::from(key);
    shell
        .id((id.clone(), 1usize))
        .on_hover(cx.listener(move |this, on: &bool, _, cx| {
            if *on {
                if !this.hover.contains(&tracked) {
                    this.hover.push(tracked.clone());
                }
            } else {
                let Some(at) = this.hover.iter().position(|live| *live == tracked) else {
                    return;
                };
                this.hover.remove(at);
            }
            cx.notify();
        }))
        .with_spring((id, 0usize), motion::level(motion::snappy(), view.liveness(key)), move |shell, liveness| shell.child(build(liveness)))
        .into_any_element()
}

/// [`live_on`] for a row: a block that spans the panel it lives in.
fn live(key: &'static str, view: &SettingsApp, cx: &Context<SettingsApp>, build: impl Fn(f32) -> AnyElement + 'static) -> AnyElement {
    live_on(div().w_full().min_w_0(), key, view, cx, build)
}

/// The pointer response every panel row shares: a wash that springs in, and an accent bar that grows
/// out of the panel's own edge at the row's vertical centre.
///
/// `interactive` gates only the press step — a row that is clickable deepens while the button is
/// down, a row that merely *carries* controls (a select, a slider) only washes, because sinking it
/// would claim a click that has nowhere to go.
fn row_wash(row: Stateful<Div>, liveness: f32, interactive: bool) -> Stateful<Div> {
    let wash = motion::wash(liveness);
    let press = if interactive { motion::sink(liveness) } else { 0.0 };
    row.relative().rounded_t(px(if row_edges::current().0 { PANEL_RADIUS } else { 0. }))
        .rounded_b(px(if row_edges::current().1 { PANEL_RADIUS } else { 0. }))
        .bg(motion::mix(press, t::row_hover().opacity(wash), t::surface_hi()))
        .child(accent_bar(liveness))
}

/// A box stretched over the whole row, so the bar is centred by the flex rather than by a measured
/// offset — rows come in two heights and one `top` cannot centre both. The box already reaches the
/// panel's inner edge, because `card` bleeds its own margin out to it; a negative offset would land
/// outside and be clipped.
fn accent_bar(liveness: f32) -> AnyElement {
    let wash = motion::wash(liveness);
    div().absolute().left_0().right_0().top_0().bottom_0().flex().items_center()
        .child(div().w(px(3.0)).h(px(26.0 * wash)).rounded(px(1.5)).bg(t::accent()).opacity(wash)
            .shadow(vec![BoxShadow::new(px(0.0), px(0.0), t::accent().opacity(0.35 * wash).into()).blur_radius(px(7.0))]))
        .into_any_element()
}

/// Wrap a row that was built with its own `Context`-dependent children.
///
/// [`live_on`] rebuilds its content from a liveness value, which needs every child to be
/// constructible inside a `'static` closure. A row holding a select or a slider track is not, so
/// here the row is built once per frame as usual and the spring only restyles it — same motion, one
/// fewer rebuild.
fn live_shell(
    row: Stateful<Div>,
    key: SharedString,
    view: &SettingsApp,
    cx: &Context<SettingsApp>,
    animate: impl FnOnce(Stateful<Div>, f32) -> Stateful<Div> + 'static,
) -> AnyElement {
    let id = SharedString::from(format!("spr-{key}"));
    let tracked = key.clone();
    row.on_hover(cx.listener(move |this, on: &bool, _, cx| {
            if *on {
                if !this.hover.contains(&tracked) {
                    this.hover.push(tracked.clone());
                }
            } else if let Some(at) = this.hover.iter().position(|live| *live == tracked) {
                this.hover.remove(at);
            }
            cx.notify();
        }))
        .with_spring(id, motion::level(motion::snappy(), view.liveness(&key)), animate)
        .into_any_element()
}

/// A panel row that answers to the pointer: the wash springs in, an accent bar grows out of the
/// panel's edge, and the row deepens a further step under a press.
///
/// `trailing` is drawn at the same liveness as the wash, so a control can ride its row — a switch
/// knob stretching while its row is held down, a slider knob growing as the pointer arrives.
fn live_card(
    key: &'static str,
    title: &str,
    detail: &str,
    trailing: impl Fn(f32) -> AnyElement + 'static,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    view: &SettingsApp,
    cx: &Context<SettingsApp>,
) -> AnyElement {
    tally();
    let (title, detail) = (title.to_owned(), detail.to_owned());
    // Shared, because the row is redrawn on every frame of its spring and each redraw needs the
    // same handler; a plain closure would be consumed by the first one.
    let click = Rc::new(click);
    live(key, view, cx, move |liveness| {
        let click = Rc::clone(&click);
        row_wash(
            card(SharedString::from(format!("row-{key}")), &title, &detail).cursor_pointer(),
            liveness,
            true,
        )
        .child(trailing(liveness))
        .on_click(move |event, window, app| click(event, window, app))
        .into_any_element()
    })
}

/// Paint one slider track at `progress` (0..1), with the knob lifted by `grab` (0 at rest, 1 while
/// the gesture is live).
///
/// A free function so the very same track can be painted from inside a spring: the animated value
/// is just a different `progress`, which keeps the drag path and the animated path pixel-identical.
fn slider_track(
    cell: Rc<Cell<Bounds<Pixels>>>,
    smoke: Rc<Cell<Bounds<Pixels>>>,
    watched: bool,
    progress: f32,
    grab: f32,
) -> impl IntoElement {
    let filled = t::accent();
    let empty = t::track();
    let knob = t::knob();
    canvas(
        move |painted, _window, _cx| {
            cell.set(painted);
            if watched {
                smoke.set(painted);
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
                Bounds::new(line.origin, size(painted.size.width * progress, thickness)),
                px(2.0),
                filled,
                px(0.0),
                transparent_black(),
                BorderStyle::Solid,
            ));
            let radius = 7.0 + 2.0 * grab;
            window.paint_quad(quad(
                Bounds::new(
                    point(painted.origin.x + painted.size.width * progress - px(radius), painted.origin.y + painted.size.height / 2.0 - px(radius)),
                    size(px(radius * 2.0), px(radius * 2.0)),
                ),
                px(radius),
                knob,
                px(1.0),
                t::hairline_bright(),
                BorderStyle::Solid,
            ));
        },
    )
    .size_full()
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
    let watched = smoke_slider() == Some(key);
    let dragging = view.slider.is_some_and(|drag| drag.key == key);
    let cell = Rc::clone(&bounds);
    let smoke_cell = Rc::clone(&view.smoke_bounds);
    // Direct manipulation wins: while the pointer is on the track the fill must follow it one to
    // one, and a spring there would read as lag. Every other change — a preset, a reset, a value
    // arriving from the native side — glides instead of jumping.
    let track: AnyElement = if dragging {
        slider_track(Rc::clone(&cell), Rc::clone(&smoke_cell), watched, fraction, 1.0).into_any_element()
    } else {
        let paint = move |progress: f32| slider_track(Rc::clone(&cell), Rc::clone(&smoke_cell), watched, progress, 0.0);
        paint(fraction)
            .with_spring(
                SharedString::from(format!("slider-{key}")),
                SpringAnimation::new(motion::standard()).to(fraction).with_epsilon(0.0008),
                move |_, progress| paint(progress),
            )
            .into_any_element()
    };

    let readout = format_value(value, min, max, unit);
    let row = card(SharedString::from(format!("row-{key}")), title, detail)
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
        }));
    live_shell(row, SharedString::from(key), view, cx, |row, liveness| row_wash(row, liveness, false))
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

fn toggle_row(key: &'static str, title: &str, detail: &str, checked: bool, view: &SettingsApp, cx: &Context<SettingsApp>) -> AnyElement {
    let shared = Rc::clone(&view.shared);
    live_card(
        key,
        title,
        detail,
        move |liveness| switch_at(key, checked, liveness).into_any_element(),
        move |_, _, cx| {
            let _ = send(&shared, key, if checked { "false" } else { "true" }, cx);
        },
        view,
        cx,
    )
}

fn default_color(key: &str) -> &'static str {
    match key {
        "SettingsLightAccentColor" => "#2563EB",
        "SettingsDarkAccentColor" => "#6EA8FF",
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
    let row = card(SharedString::from(format!("row-{key}")), title, "点击打开调色盘；拖动色相与明暗即可取色。")
        .cursor_pointer()
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
            if already_open {
                view.close_picker(cx);
            } else {
                view.closing_picker = None;
                view.picker = Some(Picker { key, hsv: Hsv::from_rgb(t::parse(&opening)), drag: Drag::None });
                view.hex_field = Some(make_hex_field(&opening, cx));
                cx.notify();
            }
        }));
    live_shell(row, SharedString::from(key), view, cx, |row, liveness| row_wash(row, liveness, true))
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
    // Same trick as the drawer: a closing picker keeps painting until its exit spring settles, so
    // the surface shrinks back into the row that opened it instead of blinking out.
    let (picker, open) = match (view.picker.as_ref(), view.closing_picker.as_ref()) {
        (Some(picker), _) => (*picker, true),
        (None, Some(picker)) => (*picker, false),
        (None, None) => return div().into_any_element(),
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
            // The knob swells under the finger: feedback on the press, and it settles back the
            // moment the gesture ends.
            div().with_spring(
                ("pad-knob", 0usize),
                motion::phase(motion::snappy(), picker.drag == Drag::Pad),
                move |_, phase| {
                    let size = KNOB + 4.0 * motion::alpha(phase);
                    div()
                        .absolute()
                        .left(px(hsv.s * PAD_WIDTH - size / 2.0))
                        .top(px((1.0 - hsv.v) * PAD_HEIGHT - size / 2.0))
                        .w(px(size))
                        .h(px(size))
                        .rounded(px(size / 2.0))
                        .bg(hsv.to_rgb())
                        .border_2()
                        .border_color(gpui::white())
                },
            ),
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
            div().with_spring(
                ("hue-knob", 0usize),
                motion::phase(motion::snappy(), picker.drag == Drag::Hue),
                move |_, phase| {
                    let size = KNOB + 4.0 * motion::alpha(phase);
                    div()
                        .absolute()
                        .left(px(hsv.h * PAD_WIDTH - size / 2.0))
                        .top(px(-(size - HUE_HEIGHT) / 2.0))
                        .w(px(size))
                        .h(px(size))
                        .rounded(px(size / 2.0))
                        .bg(Hsv::new(hsv.h, 1.0, 1.0).to_rgb())
                        .border_2()
                        .border_color(gpui::white())
                },
            ),
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
                                .on_click(cx.listener(|view, _, _, cx| view.close_picker(cx))),
                        ),
                ),
        )
        // The picker is a surface that grew out of a row, so it arrives from there and leaves the
        // same way — the row it belongs to stays visible the whole time, which is what makes the
        // spatial relationship obvious.
        .with_spring(("picker-phase", 0usize), motion::phase_from(motion::sheet(), open, false), |element, phase| {
            motion::rise(element, motion::alpha(phase), 14.0)
        })
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

fn info_row(title: &str, detail: &str, body: &str, view: &SettingsApp, cx: &Context<SettingsApp>) -> AnyElement {
    tally();
    let row = div().id(SharedString::from(format!("row-info-{title}")))
        .flex().flex_col().py(px(16.0)).mx(px(-PANEL_PAD)).px(px(PANEL_PAD)).min_w_0()
        .child(div().text_size(px(14.0)).text_color(t::text()).child(title.to_owned()))
        .child(div().mt_1().text_size(px(12.0)).text_color(t::text_dim()).child(detail.to_owned()))
        .child(div().mt_3().text_size(px(12.5)).text_color(t::text_dim()).child(body.to_owned()));
    live_shell(row, SharedString::from(title), view, cx, |row, liveness| row_wash(row, liveness, false))
}

/// The switch: a capsule track with a knob, straight from the handoff's `di-switch`.
///
/// Track and knob ride one spring, so flipping it half way and changing your mind carries the
/// velocity into the return rather than hitting a wall — the thing a CSS transition cannot do.
fn switch(key: &'static str, checked: bool) -> impl IntoElement {
    switch_at(key, checked, 0.0)
}

/// [`switch`] with the row's pointer state folded in.
///
/// `liveness` arrives already interpolated by the row's own spring, so the knob deforms *with* the
/// press instead of needing a second spring of its own — one element, one spring, two readings of
/// it. Held down, the knob stretches toward the end it is about to travel to and flattens slightly,
/// which is what makes a toggle read as pushed rather than merely clicked.
fn switch_at(key: &'static str, checked: bool, liveness: f32) -> impl IntoElement {
    let wash = motion::wash(liveness);
    let press = motion::sink(liveness);
    div().with_spring(
        SharedString::from(format!("switch-{key}")),
        motion::phase(motion::snappy(), checked),
        move |_, phase| {
            let progress = motion::alpha(phase);
            // Stretching eats travel, so the knob still lands inside the track at both ends.
            let width = 17.0 + 5.0 * press;
            let travel = 16.0 - 5.0 * press;
            div().relative().flex_shrink_0().w(px(40.0)).h(px(24.0)).rounded(px(12.0))
                .bg(motion::blend_color(phase, t::track(), t::accent()))
                .border_1()
                .border_color(motion::mix(0.45 * wash, motion::blend_color(phase, t::hairline(), t::accent()), t::accent()))
                .shadow(vec![BoxShadow::new(px(0.0), px(2.0), t::accent().opacity(0.28 * progress).into()).blur_radius(px(8.0))])
                .child(
                    div().absolute().top(px(2.5 + press)).left(px(2.5 + travel * progress))
                        .w(px(width)).h(px(18.0 - 2.5 * press)).rounded(px(9.0))
                        .bg(motion::blend_color(phase, t::knob(), t::on_accent()))
                        .shadow(vec![BoxShadow::new(px(0.0), px(1.0 + wash), t::shadow().into()).blur_radius(px(2.0 + 3.0 * wash))]),
                )
        },
    )
}

/// Send one edit and fold the reply back into the shared state.
fn send(shared: &Rc<Mutex<Shared>>, key: &str, value: &str, cx: &mut App) -> Result<(), String> {
    if key=="openGitee" {cx.open_url("https://gitee.com/heiamer/abc");return Ok(())}
    if key=="openGitHub" {cx.open_url("https://github.com/Heiamer/LyricIsland");return Ok(())}
    if key=="collectLogs" {
        if let Ok(mut s)=shared.lock(){s.message="正在收集日志…".into();}
        let shared=Rc::clone(shared);
        let job=cx.background_executor().spawn(async move { pipe::request("collectLogs", Value::Null) });
        cx.spawn(async move |cx|{
            let result=job.await;
            if let Ok(mut s)=shared.lock(){
                match result{
                    Ok(reply)=>{let success=reply["success"].as_bool()==Some(true);s.message=reply["message"].as_str().unwrap_or(if success{"已生成诊断日志"}else{"生成诊断日志失败"}).to_owned();s.connection_error=false;s.message_is_connection_error=false;},
                    Err(error)=>{s.message=error;s.connection_error=true;s.message_is_connection_error=true;}
                }
            }
            let _=cx.update(|cx|cx.refresh_windows());
        }).detach();
        return Ok(())
    }
    if key=="checkUpdate"||key=="installUpdate" {send_todo(shared,if key=="checkUpdate"{"checkUpdate"}else{"updateDownload"},Value::Null,cx);return Ok(())}
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
        let key = ["ExpandedTextColor", "ExpandedComponentColor", "LyricProgressColor", "SettingsLightAccentColor", "SettingsDarkAccentColor"]
            .into_iter()
            .find(|key| *key == requested)?;
        let state = shared.lock().ok()?.state.clone();
        let hex = setting(&state, key).as_str().map(str::to_owned).unwrap_or_else(|| default_color(key).to_owned());
        log("SMOKE", &format!("starting with the {key} picker open ({hex})"));
        Some(Picker { key, hsv: Hsv::from_rgb(t::parse(&hex)), drag: Drag::None })
    });

    let platform = gpui_windows::WindowsPlatform::new(false).expect("failed to initialize the Windows platform");
    Application::with_platform(Rc::new(platform)).with_assets(icons::IconAssets).run(move |cx: &mut App| {
        register_fonts(cx);

        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Blurred,
                titlebar: Some(TitlebarOptions{title:Some("点屿 · 控制中心".into()),appears_transparent:true,..Default::default()}),
                // Fixed canvas: no resize border, no maximize, no drag-to-maximize. The layout is
                // built for one size and every page scrolls inside it.
                is_resizable: false,
                ..Default::default()
            },
            |window, cx| {window.set_window_title("点屿 · 控制中心");cx.new(|cx| SettingsApp::new(Rc::clone(&shared), smoke_picker, cx))},
        ) {
            Ok(window) => window,
            Err(error) => {
                log("GPUI", &error.to_string());
                cx.quit();
                return;
            }
        };

        cx.on_window_closed(|cx, _window_id| {
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
                if let Ok(fraction) = std::env::var("LYRICISLAND_SMOKE_DRAWER_BOTTOM") {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_millis(900)).await;
                        let _ = window.update(cx, |view, _, cx| {
                            let fraction = fraction.parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0);
                            let max = view.drawer_scroll.max_offset();
                            view.drawer_scroll.set_offset(point(px(0.0), -max.y * fraction));
                            cx.notify();
                        });
                    })
                    .detach();
                }
                if let Ok(fraction) = std::env::var("LYRICISLAND_SMOKE_PAGE_SCROLL") {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_millis(900)).await;
                        let _ = window.update(cx, |view, _, cx| {
                            let fraction = fraction.parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
                            let index = view.route.index().min(view.scrolls.len() - 1);
                            let max = view.scrolls[index].max_offset();
                            view.scrolls[index].set_offset(point(px(0.0), -max.y * fraction));
                            cx.notify();
                        });
                    })
                    .detach();
                }
                if let Some(path)=std::env::var_os("LYRICISLAND_SMOKE_RECONNECT"){run_reconnect_smoke(cx,window,PathBuf::from(path),Rc::clone(&shared));}
                // Bring the picker into view after the first frame. This doubles as a check that a
                // late update really does repaint — dragging depends on it.
                if smoke_picker.is_some() || smoke_slider().is_some() || std::env::var_os("LYRICISLAND_SMOKE_FIELD").is_some() {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_millis(900)).await;
                        let outcome = window.update(cx, |view, _, cx| {
                            let max = view.scrolls[view.route.index().min(view.scrolls.len() - 1)].max_offset();
                            // Scroll to the end so the picker under the last colour row is visible.
                            view.scrolls[view.route.index().min(view.scrolls.len() - 1)].set_offset(point(px(0.0), -max.y));
                            cx.notify();
                            f32::from(max.y)
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
        if let Some(path)=std::env::var_os("LYRICISLAND_FRAME_PROBE"){
            let _=window.update(cx,|_,window,cx|frame_probe::start(window,cx,PathBuf::from(path)));
        }
        cx.activate(true);
    });
}

fn ui_state(state:&Value)->Value {let mut state=state.clone();if let Some(map)=state.as_object_mut(){for key in ["renderFrames"]{map.remove(key);}}state}

/// The 750 ms refresh loop, mirroring the Slint frontend so the native side keeps one client.
fn poll_pipe(cx: &mut App, window: WindowHandle<SettingsApp>, shared: Rc<Mutex<Shared>>) {
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_millis(250)).await;
        let reply = cx.background_executor().spawn(async {
            let value = pipe::request("state", Value::Null)?;
            // Clicking a reminder banner asks this window to show the todo page. Acknowledge it in
            // the same round trip so the native side stops repeating the request.
            if value["pendingPage"].as_str().is_some() {
                let _ = pipe::page_seen();
            }
            let quick = value["pendingAction"].as_str() == Some("quickReminder");
            if quick { let _ = pipe::request("pendingActionSeen", Value::Null); }
            Ok::<(Value, bool), String>((value, quick))
        }).await;
        let mut jump = None;
        let mut quick = false;
        let mut changed=true;
        if let Ok(mut state) = shared.lock() {
            let before=(ui_state(&state.state),state.notice.clone(),state.message.clone(),state.connection_error);
            match reply {
                Ok((value, quick_action)) => {
                    quick = quick_action;
                    jump = match value["pendingPage"].as_str() {
                        Some("todos")=>Some(Route::Todos),Some("timers")=>Some(Route::Timers),Some("notifications")=>Some(Route::Notifications),
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
                if view.route != target { view.route = target; view.drawer=None; view.return_context=None; }
                cx.notify();
            }).is_err() { break; }
        }
        if quick && window.update(cx, |view, _, cx| {
            view.route = Route::Overview;
            view.drawer = Some(Drawer::QuickReminder);
            view.return_context = None;
            cx.notify();
        }).is_err() { break; }
        if changed && window.update(cx, |_, _, cx| cx.notify()).is_err() {
            break;
        }
    })
    .detach();
}

/// `--smoke-ui <dir>`: walk every route, render every drawer once, then report the regressions.
fn run_smoke(cx: &mut App, window: WindowHandle<SettingsApp>, directory: PathBuf) {
    let _ = std::fs::create_dir_all(&directory);
    cx.spawn(async move |cx| {
        for route in Route::ALL {
            // The todo route is walked with its editor drawer open, so the walk paints the fields
            // and the daily-repeat control rather than only the list.
            if window.update(cx, |view, _, cx| {
                view.route = route;
                view.drawer = None;
                view.picker = None;
                view.settings_tab = 0;
                if route == Route::Todos {
                    view.open_todo_form(None, cx);
                }
                cx.notify();
            }).is_err() {
                return;
            }
            cx.background_executor().timer(Duration::from_millis(240)).await;
            // 设置 has three tabs behind one sidebar entry, so each tab is painted in turn.
            if route == Route::Settings {
                for tab in 1..3 {
                    if window.update(cx, |view, _, cx| { view.settings_tab = tab; cx.notify(); }).is_err() {
                        return;
                    }
                    cx.background_executor().timer(Duration::from_millis(240)).await;
                }
            }
        }
        // Every drawer must build without a window: a missing arm fails the walk loudly.
        let drawer_walk = window.update(cx, |view, _, cx| {
            let state = view.shared.lock().unwrap().state.clone();
            let todo = state["todos"].as_array().and_then(|list| list.first()).and_then(|todo| todo["id"].as_str()).map(str::to_owned);
            let notice = state["notificationHistory"].as_array().and_then(|list| list.first()).and_then(|event| event["id"].as_str()).map(str::to_owned);
            let mut drawers = vec![Drawer::TodoEditor, Drawer::TodoSettings, Drawer::NotificationSettings, Drawer::TimerSettings, Drawer::Changelog, Drawer::Licenses, Drawer::QuickReminder, Drawer::WorkflowEditor, Drawer::RuleEditor(None)];
            let rule = state["appRules"].as_array().and_then(|list| list.first()).and_then(|rule| rule["id"].as_str()).map(str::to_owned);
            if let Some(id) = rule {
                drawers.push(Drawer::RuleEditor(Some(id)));
            }
            if let Some(id) = todo {
                drawers.push(Drawer::Snooze(id.clone()));
                drawers.push(Drawer::Focus(id));
            }
            if let Some(id) = notice {
                drawers.push(Drawer::NoticeDetail(id));
            }
            view.drawer = None;
            for drawer in drawers {
                if drawer_rows(&drawer, &state, view, cx).is_empty() {
                    return false;
                }
            }
            true
        }).unwrap_or(false);
        // Every tab of the settings page must build from the same snapshot, and the sound tab is
        // the largest of the three. Rows are counted individually now that sections are panels.
        let settings_tabs_regression=window.update(cx,|view,_,cx|{
            let state=view.shared.lock().unwrap().state.clone();
            let mut sizes=Vec::new();
            for tab in 0..3 { view.settings_tab=tab; tally_reset(); let _=settings_page_rows(&state,view,cx); sizes.push(tally_count()); }
            view.settings_tab=0;
            sizes.iter().all(|size|*size>=2)&&sizes[2]>sizes[0]
        }).unwrap_or(false);
        // The drawer is a stack of panels now, so the number of top-level elements says nothing
        // about how many rows it builds; count the rows themselves.
        let whitelist_regression=window.update(cx,|view,_,cx|{
            let mut state=view.shared.lock().unwrap().state.clone();
            state["settings"]["NotificationWhitelistEnabled"]=json!(false);
            tally_reset();let _=notification_settings_rows(&state,view,cx);let collapsed=tally_count();
            state["settings"]["NotificationWhitelistEnabled"]=json!(true);
            tally_reset();let _=notification_settings_rows(&state,view,cx);let expanded=tally_count();
            collapsed>=9&&expanded>=collapsed+3
        }).unwrap_or(false);
        // Opening the editor must fill the fields from the snapshot, and closing it must not change
        // what the todo page renders — the form lives in the one shared drawer now.
        let todo_regression=window.update(cx,|view,_,cx|{
            view.drawer=None;view.todo_editing=None;view.todo_filter=0;view.todo_completed_open=false;
            let state=view.shared.lock().unwrap().state.clone();
            let collapsed=page_rows(Route::Todos,&state,view,cx).len();
            view.open_todo_form(None,cx);
            let editor=view.drawer==Some(Drawer::TodoEditor);
            let drawer=view.drawer.clone().map(|drawer|drawer_rows(&drawer,&state,view,cx).len()).unwrap_or(0);
            let filled=view.todo_date.read(cx).text()==state["today"].as_str().unwrap_or_default()&&!view.todo_daily;
            let after=page_rows(Route::Todos,&state,view,cx).len();
            view.close_todo_form(cx);
            collapsed>=3&&editor&&drawer>=1&&filled&&after==collapsed&&view.drawer.is_none()
        }).unwrap_or(false);
        // Jumping to the shared sound settings and coming back must land on the page (and drawer)
        // that started the jump.
        let shared_nav_regression=window.update(cx,|view,_,cx|{
            view.route=Route::Notifications;view.drawer=Some(Drawer::NotificationSettings);view.return_context=None;
            view.go_sound(cx);
            let forwarded=view.route==Route::Settings&&view.settings_tab==2&&view.return_context.is_some()&&view.drawer.is_none();
            view.return_to_context(cx);
            let returned=view.route==Route::Notifications&&view.drawer==Some(Drawer::NotificationSettings)&&view.return_context.is_none();
            view.drawer=None;
            forwarded&&returned
        }).unwrap_or(false);
        // Every route must own its scroll offset. One shared handle made a page open already scrolled
        // to wherever the previous page had been left, so the bottom of one page was the bottom of all.
        let scroll_regression=window.update(cx,|view,_,_|{
            if view.scrolls.len()!=Route::ALL.len(){return false}
            let first=view.scrolls[Route::Todos.index()].clone();let second=view.scrolls[Route::Notifications.index()].clone();
            second.set_offset(point(px(0.0),px(0.0)));
            first.set_offset(point(px(0.0),px(-320.0)));
            let moved=first.offset().y==px(-320.0)&&second.offset().y==px(0.0);
            first.set_offset(point(px(0.0),px(0.0)));
            moved
        }).unwrap_or(false);
        // New surfaces: the quick drawer, the 流程 editor and the 应用提醒 view must all build from
        // a live snapshot, and a fresh 流程 form must start with two valid 25-minute stages.
        let feature_regression=window.update(cx,|view,window,cx|{
            let state=view.shared.lock().unwrap().state.clone();view.drawer=None;view.open_workflow_form(None,cx);
            let empty=view.workflow_count==0&&view.workflow_stages_json(cx).is_err();
            let payload=FlowDrag{source:None,module:Some("builtin-focus".into()),kind:1,generation:view.workflow_generation,title:String::new()};view.flow_drop(&payload,0,cx);
            let independent=view.workflow_count==1&&view.workflow_stages[0].1.read(cx).text()=="25";
            view.workflow_stages[0].1.update(cx,|f,cx|f.set_text("17",cx));
            let library_minutes=view.workflow_library.iter().find(|module|module.id=="builtin-focus").is_some_and(|module|module.minutes.read(cx).text()=="25");
            view.library_add_to_flow("builtin-rest",cx);
            let valid=view.workflow_stages_json(cx).is_ok();let entity=view.workflow_stages[0].0.entity_id();
            let payload=FlowDrag{source:Some(0),module:None,kind:0,generation:view.workflow_generation,title:String::new()};view.flow_drop(&payload,2,cx);
            let reordered=view.workflow_stages[1].0.entity_id()==entity;
            view.workflow_selected=1;view.flow_duplicate(cx);let duplicated=view.workflow_count==3&&view.workflow_stages[2].0.entity_id()!=entity;
            view.flow_remove(2,cx);let removed=view.workflow_count==2;
            view.workflow_stages[0].1.update(cx,|f,cx|f.set_text("0",cx));let invalid=view.workflow_stages_json(cx).is_err();
            let original_kind=view.workflow_stages[0].2;view.workflow_stages[0].2=3;
            view.begin_stage_rename(0,window,cx);view.workflow_stages[0].0.update(cx,|f,cx|f.set_text("  改名阶段  ",cx));view.finish_stage_rename(true,cx);
            let renamed=view.workflow_stages[0].2==3&&view.workflow_stages[0].0.read(cx).text()=="改名阶段";view.workflow_stages[0].2=original_kind;
            // The library is a real draft: add, rename, cycle the badge, duplicate, then clean up.
            let library_before=view.workflow_library.len();view.library_add(cx);
            let temp_id=view.workflow_module_selected.clone();
            let added_library=temp_id.is_some()&&view.workflow_library.len()==library_before+1;
            let renamed_library=temp_id.as_ref().is_some_and(|id|{view.library_rename_commit(id,"临时模块",cx);view.workflow_library.iter().find(|module|&module.id==id).is_some_and(|module|module.name=="临时模块")});
            let rejected_library=temp_id.as_ref().is_some_and(|id|{view.library_rename_commit(id,"",cx);view.workflow_library.iter().find(|module|&module.id==id).is_some_and(|module|module.name=="临时模块")});
            let cycled_library=temp_id.as_ref().is_some_and(|id|{let before=view.workflow_library.iter().find(|module|&module.id==id).map(|module|module.kind).unwrap_or(0);view.library_cycle_kind(id,cx);view.workflow_library.iter().find(|module|&module.id==id).is_some_and(|module|module.kind==(before+1)%FLOW_MODULES.len())});
            let duplicated_library=temp_id.as_ref().is_some_and(|id|{let before=view.workflow_library.len();view.library_duplicate(id,cx);view.workflow_library.len()==before+1});
            let temp_ids:Vec<String>=view.workflow_library.iter().filter(|module|module.name.starts_with("临时模块")).map(|module|module.id.clone()).collect();
            for id in &temp_ids{view.library_delete(id,cx);}
            let library_clean=!temp_ids.is_empty()&&view.workflow_library.len()==library_before;
            view.workflow_module_selected=None;
            let quick=quick_ui::quick_reminder_rows(&state,view,cx).len()>=2;let rules=rules_ui::rules_page_rows(&state,view,cx).len()>=2;
            view.drawer=None;empty&&independent&&library_minutes&&valid&&reordered&&duplicated&&removed&&invalid&&renamed&&added_library&&renamed_library&&rejected_library&&cycled_library&&duplicated_library&&library_clean&&quick&&rules
        }).unwrap_or(false);
        let clock_regression=window.update(cx,|view,_,cx|{
            let mut state=view.shared.lock().unwrap().state.clone();let old_mode=view.timer_mode.clone();let old_new=view.timer_new;
            state["timers"]=json!([]);view.timer_new=true;
            for mode in ["workflow","pomodoro","countdown","stopwatch"]{view.timer_mode=mode.into();if timer_rows(&state,view,cx).is_empty(){return false}}
            let _=focus_direction_switch(&state,None,cx);state["settings"]["FocusCountUp"]=json!(true);let _=focus_direction_switch(&state,None,cx);
            let sample=json!({"id":"clock-smoke","mode":"pomodoro","countUp":true,"elapsedMs":0,"revision":1,"phase":"focus","running":false});let _=focus_direction_switch(&state,Some(&sample),cx);
            view.timer_mode=old_mode;view.timer_new=old_new;
            precise_clock(3830)=="00:03.83"&&display_clock(3.2,true)=="00:03"&&display_clock(3.2,false)=="00:04"&&display_clock(3600.,false)=="01:00:00"
        }).unwrap_or(false);
        let picker_regression=if std::env::var_os("LYRICISLAND_SMOKE_REGRESSIONS").is_some(){
            let result=window.update(cx,|view,_,cx|{
                view.route=Route::Settings;view.settings_tab=1;view.drawer=None;
                view.picker=Some(Picker{key:"ExpandedComponentColor",hsv:Hsv::new(0.3,0.5,0.7),drag:Drag::None});
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
        let sound_selection = cx.background_executor().spawn(async { pipe::sound_selection_smoke() }).await;
        let sound_selection = match sound_selection {
            Ok(report) => report,
            Err(error) => json!({"success": false, "error": error}),
        };
        let material_selection = cx.background_executor().spawn(async { pipe::material_selection_smoke() }).await;
        let material_selection = match material_selection {
            Ok(report) => report,
            Err(error) => json!({"success": false, "error": error}),
        };
        let connected = pipe::request("state", Value::Null).is_ok_and(|value| value["success"] == json!(true));
        let report = json!({
            "success": t::accent_regression()&&connected&&sound_selection["success"]==true&&material_selection["success"]==true&&picker_regression.unwrap_or(true)&&whitelist_regression&&settings_tabs_regression&&todo_regression&&shared_nav_regression&&drawer_walk&&scroll_regression&&feature_regression&&clock_regression&&tutorial::regression()&&row_edges::regression()&&changelog::entries().first().is_some_and(|entry|entry.title.ends_with(DISPLAY_VERSION)&&entry.body.lines().count()<=5&&entry.body.chars().count()<300),"changelogSummaryRegression":changelog::entries().first().is_some_and(|entry|entry.title.ends_with(DISPLAY_VERSION)&&entry.body.lines().count()<=5&&entry.body.chars().count()<300),"accentContrastRegression":t::accent_regression(),"rowEdgesRegression":row_edges::regression(),"tutorialRegression":tutorial::regression(),"digitalClockRegression":clock_regression,"soundSelection":sound_selection,"materialSelection":material_selection,"whitelistExpandCollapse":whitelist_regression,"pickerRegression":picker_regression,"settingsTabs":settings_tabs_regression,"todoRegression":todo_regression,"sharedSettingsNav":shared_nav_regression,"drawerWalk":drawer_walk,"scrollPerPage":scroll_regression,"featureRegression":feature_regression,
            "pages": Route::ALL.len(),
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

/// Self-check hook: open straight onto a given route so each one can be photographed without a
/// human clicking through the sidebar. `None` falls back to the stored start page.
fn smoke_page() -> Option<Route> {
    if let Some(route) = std::env::var("LYRICISLAND_SMOKE_PAGE").ok().and_then(|value| value.parse::<usize>().ok()).and_then(|index| Route::ALL.get(index)) {
        return Some(*route);
    }
    if std::env::var_os("LYRICISLAND_SMOKE_PICKER").is_some() || smoke_slider().is_some_and(|key| key.starts_with("Motion") || key.starts_with("Spring")) {
        return Some(Route::Settings);
    }
    None
}
/// Self-check hook: open a drawer without a click so its rows can be photographed.
fn smoke_drawer() -> Option<Drawer> {
    match std::env::var("LYRICISLAND_SMOKE_DRAWER").ok().as_deref() {
        Some("NotificationSettings") => Some(Drawer::NotificationSettings),
        Some("TimerSettings") => Some(Drawer::TimerSettings),
        Some("TodoSettings") => Some(Drawer::TodoSettings),
        Some("TodoEditor") => Some(Drawer::TodoEditor),
        Some("Changelog") => Some(Drawer::Changelog),
        Some("Licenses") => Some(Drawer::Licenses),
        Some("QuickReminder") => Some(Drawer::QuickReminder),
        Some("WorkflowEditor") => Some(Drawer::WorkflowEditor),
        Some("RuleEditor") => Some(Drawer::RuleEditor(None)),
        _ => None,
    }
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
        for route in Route::ALL{
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let _=window.update(cx,|view,_,cx|{view.route=route;view.drawer=None;cx.notify();});
        }
        cx.background_executor().timer(Duration::from_millis(600)).await;
        let offline=shared.lock().is_ok_and(|s|s.connection_error);
        let _=std::fs::write(directory.join("offline-ready.json"),json!({"offlineErrorRecorded":offline,"pagesNavigatedWhileDisconnected":Route::ALL.len()}).to_string());
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
        let report=json!({"success":offline&&recovered&&unmodified&&saved,"offlineErrorRecorded":offline,"connectionRecovered":recovered,"staleErrorCleared":recovered,"failedEditNotReplayed":unmodified,"nativeSettingSaved":saved,"settings":"GPUI","version":"1.5"});
        let _=std::fs::write(directory.join("report.json"),report.to_string());
        std::process::exit(if report["success"]==true{0}else{1});
    }).detach();
}

include!("tools_ui.rs");
include!("pages_ui.rs");
