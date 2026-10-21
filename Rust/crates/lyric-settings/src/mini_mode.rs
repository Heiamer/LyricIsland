//! A two-position slider: direct pointer tracking, then a spring to the chosen detent.
use crate::*;
pub fn row(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{slider_row(state,view,cx,"IslandSizeMode","Mini","island-size-mode","灵动岛大小","经典保留完整卡片；Mini 使用顶部细条与紧凑卡片，保留左右滑动切换与上滑隐藏手势。","经典","Mini")}
pub fn capsule_material(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{if setting_str(state,"IslandSizeMode","Classic")=="Mini"{return div().id("classic-capsule-folded").into_any_element()}slider_row(state,view,cx,"ClassicCapsuleMaterial","LiquidGlass","classic-capsule-material","经典灵动岛材质","收起时可选经典深色或黑色液态玻璃；展开卡片材质独立设置。","经典","黑色液态玻璃")}
fn slider_row(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>,key:&'static str,second:&str,id:&'static str,title:&str,description:&str,left:&'static str,right:&'static str)->AnyElement{
    let mini=setting_str(state,key,"Classic")==second;
    let dragging=view.slider.filter(|d|d.key==key);
    let progress=dragging.map(|d|d.value as f32).unwrap_or(if mini{1.}else{0.});
    let bounds=Rc::new(Cell::new(Bounds::new(point(px(0.),px(0.)),size(px(0.),px(0.)))));
    let cell=bounds.clone();
    let draw=move|p:f32|{
        let cell=cell.clone();
        div().relative().size_full().rounded(px(11.)).bg(t::field()).border_1().border_color(t::hairline())
            .child(canvas(move|r,_,_|{cell.set(Bounds::new(point(r.origin.x+px(62.),r.origin.y),size(r.size.width-px(124.),r.size.height)));},move|_,_,_,_|{}).absolute().size_full())
            .child(div().absolute().top(px(3.)).left(px(3.+124.*p.clamp(0.,1.))).w(px(122.)).h(px(30.)).rounded(px(8.)).bg(t::accent()))
            .child(div().absolute().inset_0().flex().items_center()
                .child(div().w_1_2().text_center().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(if p<0.5{t::on_accent()}else{t::text_dim()}).child(left))
                .child(div().w_1_2().text_center().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(if p>=0.5{t::on_accent()}else{t::text_dim()}).child(right)))
    };
    let control=if dragging.is_some(){draw(progress).into_any_element()}else{draw(progress).with_spring(id,SpringAnimation::new(motion::snappy()).to(progress),move|_,p|draw(p)).into_any_element()};
    card(id,title,description)
        .child(div().id(id).w(px(252.)).h(px(38.)).cursor_pointer().child(control)
            .on_mouse_down(MouseButton::Left,cx.listener(move|v,e:&MouseDownEvent,_,cx|{v.slider_begin(key,0.,1.,bounds.get(),e.position);cx.notify();})))
        .on_mouse_move(cx.listener(move|v,e:&MouseMoveEvent,_,cx|{if v.slider.is_some_and(|d|d.key==key){v.slider_move(e.position);cx.notify();}}))
        .on_mouse_up(MouseButton::Left,cx.listener(|v,_:&MouseUpEvent,_,cx|{v.slider_commit(cx);cx.notify();}))
        .on_mouse_up_out(MouseButton::Left,cx.listener(|v,_:&MouseUpEvent,_,cx|{v.slider_commit(cx);cx.notify();})).into_any_element()
}
pub fn layout(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{
    let mini=setting_str(state,"IslandSizeMode","Classic")=="Mini";
    if mini{if setting_str(state,"MiniMaterial","LiquidGlass")=="LiquidGlass"{return toggle_row("MiniAutoContrast","自动文字对比","浅色背景用深字，深色背景用浅字；即时采样细条边缘，不保存屏幕图像。",flag(state,"MiniAutoContrast",true),view,cx)}return div().id("mini-layout-folded").into_any_element()}
    music_layout_row(state,view,cx)
}
pub fn material(state:&Value,cx:&mut Context<SettingsApp>)->AnyElement{
    if setting_str(state,"IslandSizeMode","Classic")=="Mini"{return segmented_row("mini-material","MiniMaterial","Mini 材质","透明液态玻璃或经典深色；歌词居中并保留清晰描边，展开时沿用莫奈取色。",&[("Classic","经典"),("LiquidGlass","液态玻璃")],setting_str(state,"MiniMaterial","LiquidGlass"),cx)}
    segmented_row("island-material","IslandMaterial","展开卡片材质","经典保留纯色平面；亚克力使用磨砂与细噪点；液态玻璃使用透镜反光与通透边缘；封面色彩仍由莫奈取色。默认亚克力。",&[("Classic","经典"),("Acrylic","亚克力"),("LiquidGlass","液态玻璃")],setting_str(state,"IslandMaterial","Acrylic"),cx)
}
