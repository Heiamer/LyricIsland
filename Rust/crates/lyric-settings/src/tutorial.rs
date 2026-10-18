//! First-use demonstrations are local simulations; they never operate the real tray or player.
use crate::*;

fn steps(state: &Value) -> Vec<(u8, &'static str, &'static str)> {
    let mut s = Vec::new();
    if flag(state, "ClickAnywhereExpand", true) {
        s.push((0, "轻点展开，再点收起", "点击胶囊空白处展开，点击卡片外收起。按钮与进度条保留各自的操作。"));
    } else if flag(state, "HoverExpand", false) {
        s.push((0, "悬停展开", "将鼠标移到胶囊上展开，移开后收起。"));
    }
    if flag(state, "SwipeActivities", true) {
        s.push((1, "左右拖动，切换活动", "按住空白处向左或向右拖动，在音乐、计时器与待办之间切换。有多个活动时可用。"));
    }
    if flag(state, "Floating", false) {
        s.push((2, "拖动把手，调整位置", "使用卡片左下角的移动按钮，再拖到你喜欢的位置。"));
    }
    if flag(state, "SwipeUpHide", true) {
        s.push((3, "向上拖动，暂时隐藏", "按住胶囊或展开卡片的空白区域向上拖动，松手隐藏。短距离拖动会回到原处，不会停止音乐和计时。"));
    }
    s.push((4, "随时找回点屿", "右键任务栏托盘中的点屿图标，再选择「显示 / 隐藏灵动岛」。试着点击上方演示中的图标和菜单；教程也可在灵动岛设置中再次查看。"));
    s
}

fn finish(view: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if send(&view.shared, "GestureTutorialSeen", "true", cx).is_ok() {
        view.tutorial_step = None;
    }
    cx.notify();
}

pub fn overlay(state: &Value, view: &SettingsApp, cx: &mut Context<SettingsApp>) -> Option<AnyElement> {
    let index = view.tutorial_step?;
    let list = steps(state);
    let index = index.min(list.len() - 1);
    let (kind, title, detail) = list[index];
    let last = index + 1 == list.len();
    let demo = div().relative().h(px(210.)).w_full().rounded(px(16.)).overflow_hidden().bg(t::base());
    let demo = if kind == 4 {
        let entity = cx.entity().downgrade();
        let mode = view.tutorial_recovery;
        if motion::enabled() {
            let animation = Animation::new(Duration::from_millis(if mode == 0 { 6000 } else { 260 })).with_max_fps(30.);
            let animation = if mode == 0 { animation.repeat() } else { animation };
            demo.with_animation(
                SharedString::from(format!("tray-demo-{}-{mode}", view.tutorial_epoch)), animation,
                move |element, progress| recovery_scene(element, progress, mode, entity.clone()),
            ).into_any_element()
        } else {
            recovery_scene(demo, if mode == 0 { 0. } else { 1. }, mode, entity).into_any_element()
        }
    } else if motion::enabled() {
        demo.with_animation(
            SharedString::from(format!("gesture-demo-{}-{index}", view.tutorial_epoch)),
            Animation::new(Duration::from_millis(3200)).repeat().with_max_fps(30.),
            move |element, progress| element.child(drawing(kind, progress)),
        ).into_any_element()
    } else {
        demo.child(drawing(kind, 0.6)).into_any_element()
    };
    Some(div().id("gesture-tutorial").absolute().inset_0().occlude().track_focus(&view.music_focus)
        .bg(t::scrim()).flex().items_center().justify_center()
        .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" { finish(view, cx); cx.stop_propagation(); }
        }))
        .child(div().w(px(480.)).max_w_full().p(px(24.)).rounded(px(22.)).border_1()
            .border_color(t::hairline()).bg(t::surface()).flex().flex_col().gap(px(16.))
            .child(div().flex().justify_between().items_center()
                .child(div().text_size(px(12.)).text_color(t::text_dim()).child(format!("认识你的灵动岛 · {} / {}", index + 1, list.len())))
                .child(small_button("tutorial-skip".into(), "跳过", cx.listener(|v, _, _, cx| finish(v, cx)))))
            .child(demo).child(div().text_size(px(22.)).font_weight(FontWeight::MEDIUM).child(title))
            .child(div().min_h(px(62.)).text_size(px(13.)).text_color(t::text_dim()).child(detail))
            .child(div().flex().items_center().justify_between()
                .child(small_button("tutorial-back".into(), if index == 0 { "重播" } else { "上一步" }, cx.listener(move |v, _, _, cx| {
                    v.tutorial_epoch += 1; v.tutorial_recovery = 0;
                    v.tutorial_step = Some(index.saturating_sub(1)); cx.notify();
                })))
                .child(pill_button("tutorial-next".into(), if last { "开始使用" } else { "下一步" }, true, cx.listener(move |v, _, _, cx| {
                    if last { finish(v, cx) } else {
                        v.tutorial_epoch += 1; v.tutorial_recovery = 0; v.tutorial_step = Some(index + 1); cx.notify();
                    }
                })))))
        .into_any_element())
}

// The same causal stages drive the autoplay and the interactive simulation.
fn recovery_stage(p: f32, mode: u8) -> u8 {
    if mode != 0 { mode } else if p < 0.28 { 0 } else if p < 0.58 { 1 } else { 2 }
}
fn set_recovery(entity: &WeakEntity<SettingsApp>, mode: u8, cx: &mut App) {
    let _ = entity.update(cx, |view, cx| {
        view.tutorial_recovery = mode; view.tutorial_epoch += 1; cx.notify();
    });
}
fn recovery_scene(element: Div, p: f32, mode: u8, entity: WeakEntity<SettingsApp>) -> Div {
    let stage = recovery_stage(p, mode);
    let replay = entity.clone();
    let click = entity.clone();
    let right_click = entity.clone();
    let mut element = element
        .child(div().absolute().top(px(12.)).left(px(14.)).text_size(px(11.)).text_color(t::text_dim())
            .child(match stage { 0 => "① 点击任务栏中的点屿图标", 1 => "② 点击菜单，找回灵动岛", _ => "③ 灵动岛已显示" }))
        .child(div().id("tutorial-demo-replay").absolute().top(px(12.)).right(px(14.))
            .cursor_pointer().text_size(px(11.)).text_color(t::accent()).child("重播演示")
            .on_click(move |_, _, cx| set_recovery(&replay, 0, cx)))
        .child(div().absolute().left(px(12.)).right(px(12.)).bottom(px(8.)).h(px(30.))
            .rounded(px(8.)).bg(t::surface()).border_1().border_color(t::hairline())
            .flex().items_center().justify_between().px(px(10.))
            .child(div().flex().items_center().gap(px(6.)).text_size(px(10.)).text_color(t::text_dim())
                .child(icons::icon("layout-dashboard").size(px(13.))).child("任务栏"))
            .child(div().flex().items_center().gap(px(5.))
                .child(icons::icon("chevron-up").size(px(12.)))
                .child(div().id("tutorial-tray-icon").w(px(38.)).h(px(26.)).rounded(px(5.))
                    .cursor_pointer().bg(if stage == 1 { t::base() } else { t::surface() })
                    .flex().items_center().justify_center().child(brand_pill())
                    .on_click(move |_, _, cx| set_recovery(&click, 1, cx))
                    .on_mouse_down(MouseButton::Right, move |_, _, cx| set_recovery(&right_click, 1, cx))))) ;
    if stage == 1 {
        let appear = if mode == 0 { motion::ease_out(((p - 0.28) / 0.035).clamp(0., 1.)) } else { motion::ease_out(p) };
        element = element.child(div().id("tutorial-tray-menu").absolute().right(px(14.))
            .bottom(px(43. + 5. * appear)).w(px(190.)).p(px(6.)).rounded(px(10.))
            .bg(t::surface()).border_1().border_color(t::hairline()).opacity(appear)
            .child(div().px(px(8.)).py(px(4.)).text_size(px(10.)).text_color(t::text_dim()).child("点屿 · 托盘菜单"))
            .child(div().id("tutorial-show-island").h(px(32.)).px(px(8.)).rounded(px(6.))
                .bg(t::base()).cursor_pointer().flex().items_center().gap(px(6.)).text_size(px(12.))
                .child(icons::icon("pill").size(px(14.))).child("显示 / 隐藏灵动岛")
                .on_click(move |_, _, cx| set_recovery(&entity, 2, cx))));
    }
    if stage == 2 {
        let appear = if mode == 0 { motion::ease_out(((p - 0.58) / 0.05).clamp(0., 1.)) * (1. - ((p - 0.95) / 0.05).clamp(0., 1.)) } else { motion::ease_out(p) };
        element = element.child(recovery_island(appear));
    }
    if mode == 0 && p < 0.68 && motion::enabled() {
        element = element.child(canvas(|_, _, _| {}, move |bounds, _, window, _| {
            let w = f32::from(bounds.size.width); let h = f32::from(bounds.size.height);
            let tray = (w - 43., h - 23.); let menu = (w - 116., h - 70.);
            let (x, y) = if p < 0.28 {
                let e = motion::ease_out((p / 0.22).clamp(0., 1.));
                (w * 0.66 + (tray.0 - w * 0.66) * e, h - 65. + 42. * e)
            } else {
                let e = motion::ease_out(((p - 0.32) / 0.18).clamp(0., 1.));
                (tray.0 + (menu.0 - tray.0) * e, tray.1 + (menu.1 - tray.1) * e)
            };
            pointer(window, bounds.origin, x, y, (0.22..0.28).contains(&p) || (0.52..0.58).contains(&p));
        }).absolute().inset_0());
    }
    element
}
fn recovery_island(appear: f32) -> AnyElement {
    canvas(|_, _, _| {}, move |bounds, _, window, _| {
        let w = f32::from(bounds.size.width); let h = f32::from(bounds.size.height);
        let width = 166. + 18. * appear; let height = 30. + 12. * appear;
        let x = (w - width) / 2.; let y = (h - height) / 2.;
        let mut ink = rgb(0x222829); ink.a = appear;
        paint_rect(window, bounds.origin, x, y, width, height, height / 2., ink);
        let mut cover = rgb(0x617B70); cover.a = appear;
        paint_rect(window, bounds.origin, x + 14., y + 12., 18., 18., 5., cover);
        let mut text = rgb(0xC7D4CC); text.a = appear;
        paint_rect(window, bounds.origin, x + 42., y + 14., 114., 5., 2.5, text);
        paint_rect(window, bounds.origin, x + 42., y + 25., 68., 3., 1.5, cover);
    }).absolute().inset_0().into_any_element()
}

fn pill_geometry(kind: u8, p: f32, w: f32, h: f32) -> (f32, f32, f32, f32, f32) {
    let e = motion::ease_out(((p - 0.16) / 0.36).clamp(0., 1.));
    let back = 1. - motion::ease_out(((p - 0.78) / 0.20).clamp(0., 1.));
    let e = e * back;
    let (ww, hh) = if kind == 0 { (184. + 64. * e, 42. + 58. * e) } else { (184., 42.) };
    let (dx, dy) = match kind { 2 => (42. * e, 16. * e), 3 => (0., -65. * e), _ => (0., 0.) };
    ((w - ww) / 2. + dx, (h - hh) / 2. + dy, ww, hh, e)
}
fn drawing(kind: u8, p: f32) -> AnyElement {
    canvas(|_, _, _| {}, move |bounds, _, window, _| {
        let w = f32::from(bounds.size.width); let h = f32::from(bounds.size.height);
        let (x, y, ww, hh, e) = pill_geometry(kind, p, w, h);
        let alpha = if kind == 3 { 1. - e } else { 1. };
        let mut rect = |x, y, ww, hh, rad, mut color: gpui::Rgba| {
            color.a *= alpha; paint_rect(window, bounds.origin, x, y, ww, hh, rad, color);
        };
        rect(x, y, ww, hh, 21., rgb(0x222829));
        if kind == 1 {
            // Clip each activity to the capsule's inner bounds. Its shell never moves.
            for (offset, color, width) in [(-e * 162., 0x617B70, 105.), ((1. - e) * 162., 0xBDA77C, 78.)] {
                for (xx, yy, width, height, radius, ink) in [
                    (x + 14. + offset, y + 12., 18., 18., 5.0_f32, color),
                    (x + 42. + offset, y + 14., width, 5., 2.5, 0xC7D4CC),
                    (x + 42. + offset, y + 25., width * 0.6, 3., 1.5, color),
                ] {
                    let left = xx.max(x + 12.); let right = (xx + width).min(x + ww - 12.);
                    if right > left { rect(left, yy, right - left, height, radius.min((right - left) / 2.), rgb(ink)); }
                }
            }
        } else {
            rect(x + 14., y + 12., 18., 18., 5., rgb(0x617B70));
            rect(x + 42., y + 14., ww - 62., 5., 2.5, rgb(0xC7D4CC));
            if kind == 0 && e > 0.62 {
                let mut ink = rgb(0xA3B9AC); ink.a = e;
                rect(x + 16., y + 48., ww - 58., 5., 2.5, ink);
                rect(x + 16., y + 62., ww - 32., 2., 1., ink);
            }
        }
        let pressed = (0.16..0.62).contains(&p);
        let (cx, cy) = match kind {
            1 => (w / 2. + 42. - 84. * e, h / 2. + 10.),
            2 => (x + 30., y + hh - 12.),
            3 => (w / 2., h / 2. + 10. - 65. * e),
            _ => (w / 2. + 20., h / 2. + 12.),
        };
        pointer(window, bounds.origin, cx, cy, pressed);
    }).size_full().into_any_element()
}
fn paint_rect(window: &mut Window, origin: Point<Pixels>, x: f32, y: f32, w: f32, h: f32, radius: f32, color: gpui::Rgba) {
    window.paint_quad(quad(Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h))), px(radius), color, px(0.), transparent_black(), BorderStyle::Solid));
}
fn pointer(window: &mut Window, origin: Point<Pixels>, x: f32, y: f32, pressed: bool) {
    if pressed {
        let mut accent = t::accent(); accent.a = 0.20;
        paint_rect(window, origin, x - 10., y - 10., 20., 20., 10., accent);
    }
    let mut path = PathBuilder::fill();
    for (i, (dx, dy)) in [(0., 0.), (0., 17.), (4.5, 12.), (8., 20.), (11., 18.5), (7.5, 11.), (14., 11.)].into_iter().enumerate() {
        let p = point(origin.x + px(x + dx), origin.y + px(y + dy));
        if i == 0 { path.move_to(p); } else { path.line_to(p); }
    }
    path.close();
    if let Ok(path) = path.build() { window.paint_path(path, rgb(0x187E67)); }
}

/// Exercised by the existing GPUI smoke runner, without adding a test binary to this crate.
pub fn regression() -> bool {
    let centered = [0., 0.16, 0.4, 0.6, 0.8, 1.].into_iter().all(|p| {
        [0, 1].into_iter().all(|kind| {
            let (x, y, w, h, _) = pill_geometry(kind, p, 432., 210.);
            (x + w / 2. - 216.).abs() < 0.01 && (y + h / 2. - 105.).abs() < 0.01
        })
    });
    let causal = [(0., 0), (0.27, 0), (0.28, 1), (0.57, 1), (0.58, 2), (0.94, 2)]
        .into_iter().all(|(p, expected)| recovery_stage(p, 0) == expected);
    let manual = [0., 0.5, 1.].into_iter().all(|p| recovery_stage(p, 1) == 1 && recovery_stage(p, 2) == 2);
    let defaults = steps(&json!({"settings": {}}));
    let reduced = steps(&json!({"settings": {"ClickAnywhereExpand": false, "SwipeActivities": false, "SwipeUpHide": false}}));
    centered && causal && manual && defaults.last().is_some_and(|s| s.0 == 4) && reduced.len() == 1
}
