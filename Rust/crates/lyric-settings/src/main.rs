//! Lyric Island settings — GPUI frontend.
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
const PAGES: [(&str, &str); 7] = [
    ("显示与外观", "让歌词与音乐保持在眼前。调整会自动保存。"),
    ("动画与反馈", "自然响应，连续运动。"),
    ("歌词与同步", "让每一句歌词跟上当前播放。"),
    ("播放器白名单", "选择哪些播放器可以唤出灵动岛。"),
    ("关于与许可", "版本与开源许可。"),
    ("更新日志", "从歌词岛独立版开始，按版本倒序查看历次变化。"),
    ("通知提醒", "重要消息，安静出现。"),
];

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
    calibrate: Entity<TextField>,
    custom_sources: Entity<TextField>,
    notification_apps: Entity<TextField>,
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
        Self {
            shared,
            page: smoke_page(),
            picker,
            slider: None,
            pad_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            hue_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            scroll: ScrollHandle::new(),
            applied_dark: None,
            smoke_bounds: Rc::new(Cell::new(Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0))))),
            calibrate,
            custom_sources,
            notification_apps,
        }
    }
}

impl SettingsApp {
    fn state(&self) -> Value {
        self.shared.lock().map(|s| s.state.clone()).unwrap_or_else(|_| json!({}))
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

        let mut sidebar = div().flex().flex_col().w(px(196.0)).h_full().p_3().gap_1().bg(t::sidebar());
        for (index, (label, _)) in PAGES.iter().enumerate() {
            sidebar = sidebar.child(nav_item(index, label, index == page, cx));
        }
        sidebar = sidebar.child(div().flex_1()).child(
            div().px_3().py_2().text_size(px(10.5)).text_color(t::text_faint()).child("音乐在前，界面在后。"),
        );

        // The scroll container's child must be content-sized: giving it `h_full` pinned it to the
        // viewport, so anything past the fold was clipped instead of becoming scrollable.
        let mut column = div().flex().flex_col().px_7().py_6()
            .child(div().text_size(px(26.0)).text_color(t::text()).child(title.to_owned()))
            .child(div().mt_1().text_size(px(13.0)).text_color(t::text_dim()).child(subtitle.to_owned()));
        column = column.child(div().mt_6().flex().flex_col().gap_3().children(page_rows(page, &state, self, cx)));
        if !message.is_empty() {
            column = column.child(div().mt_4().text_size(px(12.0)).text_color(t::danger()).child(message));
        }
        let footer = div().mt_6().pt_3().text_size(px(11.0)).text_color(t::text_faint())
            .child(if notice.is_empty() { "正在连接灵动岛…".to_owned() } else { notice });

        div().flex().size_full().bg(t::base()).text_color(t::text()).font_family(t::FONT)
            .child(sidebar)
            .child(div().w(px(1.0)).h_full().bg(t::hairline()))
            .child(
                div().flex().flex_col().flex_1().h_full()
                    .child(div().id("content").flex_1().overflow_y_scroll().track_scroll(&self.scroll).child(column))
                    .child(hairline())
                    .child(div().px_7().py_3().child(footer)),
            )
    }
}

/// Sidebar row: a filled pill when selected, a wash under the pointer.
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

fn page_rows(page: usize, state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Vec<AnyElement> {
    match page {
        0 => {
            let mut rows = vec![
                theme_row(state, cx),
                toggle_row("AutoStart", "开机自动启动", "登录 Windows 后驻留系统托盘。", flag(state, "AutoStart", false), view),
                toggle_row("AlwaysOnTop", "始终置顶", "显示在其他软件上方，不抢占键盘焦点。", flag(state, "AlwaysOnTop", true), view),
                toggle_row("WordHighlight", "歌词进度涂色", "按照演唱进度高亮歌词。", flag(state, "WordHighlight", true), view),
                toggle_row("HideWhenPaused", "暂停后隐藏", "暂停时收起淡出，恢复播放时显示胶囊。", flag(state, "HideWhenPaused", true), view),
                toggle_row("HoverExpand", "悬停展开", "点击封面可保持展开；移开鼠标自动收起。", flag(state, "HoverExpand", false), view),
                toggle_row("ClickAnywhereExpand", "点击任意区域展开", "开启后点击收起状态的任意位置即可展开；关闭时仅封面响应。", flag(state, "ClickAnywhereExpand", false), view),
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
            rows.push(info_row("胶囊材质", "深色渐变表面", "使用单一默认材质，兼容旧配置；已移除亚克力和云母。"));
            rows.push(action_row("resetAppearance", "恢复紧凑尺寸", "恢复宽度、字号与固定位置。", "恢复紧凑尺寸", view, cx));
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
                flag(state, "OnlineSongInfo", false),
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
            info_row("Lyric Island · 歌词岛 1.1", "原生胶囊 · GPUI 设置 · Rust 后台",
                     "保留完整 OPPO Sans 4.0，并内置 Noto Sans JP 日文字体。"),
            info_row("界面组件", "GPUI 0.2.2",
                     "设置界面使用 GPUI，支持主题切换、即时调色与自动保存。"),
            info_row("开源来源与许可", "按实际使用保留版权和许可声明",
                     "TaskbarLyrics、AF-Media-Bar、NetEase-Cloud-Music-DiscordRPC 的移植来源继续保留。\nLyricify-Lyrics-Helper 的协议移植按 Apache-2.0 保留归属。\n详情见发行目录中的 LICENSE、THIRD-PARTY-NOTICES.md 与 licenses/。"),
        ],
        5 => history::rows(),
        _ => {
            let notifications = &state["notificationState"];
            let mut rows = vec![
                toggle_row("NotificationsEnabled", "通知提醒", "仅显示通知白名单内的应用；不会改变 Windows 自身的提醒设置。", flag(state, "NotificationsEnabled", false), view),
                toggle_row("FollowWindowsDnd", "跟随 Windows 勿扰模式", "开启时，系统勿扰期间暂停横幅；无法读取勿扰状态时也暂停。关闭后只按通知总开关和白名单提醒。", flag(state, "FollowWindowsDnd", true), view),
                info_row(
                    "通知访问",
                    notifications["access"].as_str().unwrap_or("未连接"),
                    notifications["message"].as_str().unwrap_or("请开启通知提醒，并授权 Windows 通知访问。"),
                ),
            ];
            for (command, title, detail, label) in [
                ("notificationSupport", "安装通知支持", "首次使用：为当前用户注册通知能力，需要在安装窗口确认，然后重新启动歌词岛。", "安装"),
                ("notificationAccess", "授权系统通知", "由 Windows 询问通知访问权限。首次使用需通知安装版。", "授权"),
                ("notificationSettings", "Windows 通知权限", "授权被拒绝时，可在系统设置中重新允许。", "打开设置"),
                ("notificationPreview", "预览通知横幅", "使用本地示例检查形变、字体和显示样式，不读取真实通知。", "预览"),
            ] {
                rows.push(action_row(command, title, detail, label, view, cx));
            }
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
            rows
        }
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
            cx.notify();
        }))
        .into_any_element()
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
            if view.picker.as_ref().is_some_and(|picker| picker.drag != Drag::None) {
                view.drag_to(event.position);
                cx.notify();
            }
        }))
        .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| {
            view.end_drag(cx);
            cx.notify();
        }))
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
                .child(div().text_size(px(15.0)).text_color(t::text()).child(hex.clone()))
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
    for name in ["OPPOSans.ttf", "NotoSansJP.ttf"] {
        if let Ok(bytes) = std::fs::read(directory.join(name)) {
            fonts.push(Cow::Owned(bytes));
        }
    }
    if !fonts.is_empty() {
        // A rejected font would only mean a fallback face, so the result is deliberately ignored.
        let _ = cx.text_system().add_fonts(fonts);
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
                ..Default::default()
            },
            |window, cx| {window.set_window_title("歌词岛 · 设置 1.1");cx.new(|cx| SettingsApp::new(Rc::clone(&shared), smoke_picker, cx))},
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

/// The 750 ms refresh loop, mirroring the Slint frontend so the native side keeps one client.
fn poll_pipe(cx: &mut App, window: WindowHandle<SettingsApp>, shared: Rc<Mutex<Shared>>) {
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_millis(750)).await;
        let reply = cx.background_executor().spawn(async {pipe::request("state", Value::Null)}).await;
        if let Ok(mut state) = shared.lock() {
            match reply {
                Ok(value) => {
                    state.state = value;
                    state.notice = "已连接 · 调整会自动保存".into();
                    if state.message_is_connection_error{state.message.clear();state.message_is_connection_error=false;}
                    state.connection_error=false;
                }
                Err(error) => {state.notice=error;state.connection_error=true;},
            }
        }
        if window.update(cx, |_, _, cx| cx.notify()).is_err() {
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
            if window.update(cx, |view, _, cx| { view.page = page; cx.notify(); }).is_err() {
                return;
            }
            cx.background_executor().timer(Duration::from_millis(240)).await;
        }
        let connected = pipe::request("state", Value::Null).is_ok_and(|value| value["success"] == json!(true));
        let report = json!({
            "success": connected,
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
        let report=json!({"success":offline&&recovered&&unmodified&&saved,"offlineErrorRecorded":offline,"connectionRecovered":recovered,"staleErrorCleared":recovered,"failedEditNotReplayed":unmodified,"nativeSettingSaved":saved,"settings":"GPUI","version":"1.1"});
        let _=std::fs::write(directory.join("report.json"),report.to_string());
        std::process::exit(if report["success"]==true{0}else{1});
    }).detach();
}
