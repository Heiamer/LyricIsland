// A clock-first layout. The dial is vector painted at the window's native scale.
fn clock_dial(progress:f64, accent:gpui::Rgba)->AnyElement {
    let track=t::track();let tick=t::hairline_bright();
    canvas(|_,_,_|{},move |bounds,_,window,_|{
        let center=point(bounds.origin.x+bounds.size.width/2.,bounds.origin.y+bounds.size.height/2.);
        let radius=108f32;
        for (fraction,ink,width) in [(1.,track,5.),(progress.clamp(0.,1.) as f32,accent,5.)] {
            if fraction<=0. {continue}
            let steps=(240.*fraction).ceil() as usize;
            let mut path=PathBuilder::stroke(px(width));
            for i in 0..=steps {let a=-std::f32::consts::FRAC_PI_2+std::f32::consts::TAU*fraction*i as f32/steps as f32;
                let p=point(center.x+px(radius*a.cos()),center.y+px(radius*a.sin()));if i==0{path.move_to(p)}else{path.line_to(p)}
            }
            if let Ok(path)=path.build(){window.paint_path(path,ink);}
        }
        for i in 0..60 {let angle=i as f32/60.*std::f32::consts::TAU;let inner=if i%5==0{88.}else{92.};let mut path=PathBuilder::stroke(px(if i%5==0{1.6}else{1.}));
            path.move_to(point(center.x+px(inner*angle.sin()),center.y-px(inner*angle.cos())));
            path.line_to(point(center.x+px(97.*angle.sin()),center.y-px(97.*angle.cos())));
            if let Ok(path)=path.build(){window.paint_path(path,tick);}
        }
        let a=-std::f32::consts::FRAC_PI_2+std::f32::consts::TAU*progress as f32;
        window.paint_quad(quad(Bounds::new(point(center.x+px(radius*a.cos()-4.),center.y+px(radius*a.sin()-4.)),size(px(8.),px(8.))),px(4.),accent,px(2.),t::surface(),BorderStyle::Solid));
    }).w_full().h(px(248.)).into_any_element()
}
fn timer_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
    let clocks=state["timers"].as_array().cloned().unwrap_or_default();
    let active=text(state,"activeComponent","").strip_prefix("timer:").unwrap_or("");
    let current=if view.timer_new{None}else{clocks.iter().find(|c|c["id"].as_str()==Some(active)&&c["mode"].as_str()==Some(&view.timer_mode)).or_else(||clocks.iter().find(|c|c["mode"].as_str()==Some(&view.timer_mode)))};
    let rest=current.is_some_and(|c|c["phase"]=="rest"&&view.timer_mode=="pomodoro");
    let accent=t::parse(if rest{"#87CBB0"}else if view.timer_mode=="pomodoro"{"#E7A185"}else if view.timer_mode=="countdown"{"#8EBCE1"}else{"#89C8B3"});
    let mut modes=div().flex().w_full().gap_1().p_1().rounded(px(16.)).bg(t::field());
    for(mode,label)in[("pomodoro","番茄钟"),("countdown","倒计时"),("stopwatch","秒表")]{let selected=view.timer_mode==mode;
        modes=modes.child(div().id(SharedString::from(format!("timer-mode-{mode}"))).flex_1().flex().justify_center().py_2().rounded(px(12.)).bg(if selected{t::surface_hi()}else{t::field()}).text_color(if selected{t::text()}else{t::text_dim()}).font_weight(if selected{FontWeight::SEMIBOLD}else{FontWeight::NORMAL}).text_size(px(13.)).cursor_pointer().active(|s|s.opacity(0.7)).child(label).on_click(cx.listener(move|v,_,_,cx|{v.timer_mode=mode.into();v.timer_new=false;v.timer_adjust_open=false;cx.notify();})));
    }
    let seconds=if view.timer_mode=="stopwatch"{0}else{let field=if view.timer_mode=="pomodoro"{&view.timer_focus}else{&view.timer_duration};field.read(cx).text().trim().parse::<u64>().unwrap_or(0)};
    let clock=current.and_then(|c|c["clock"].as_str()).map(str::to_owned).unwrap_or_else(||if seconds>=3600{format!("{:02}:{:02}:{:02}",seconds/3600,seconds/60%60,seconds%60)}else{format!("{:02}:{:02}",seconds/60,seconds%60)});
    let subtitle=current.and_then(|c|c["subtitle"].as_str()).unwrap_or(if view.timer_mode=="pomodoro"{"准备专注"}else if view.timer_mode=="stopwatch"{"每一秒，都算数"}else{"准备开始"}).to_owned();
    let p=current.and_then(|c|c["progress"].as_f64()).unwrap_or(0.);
    let running=current.is_some_and(|c|c["running"]==true);
    let id=current.and_then(|c|c["id"].as_str()).map(str::to_owned);
    let name=current.and_then(|c|c["name"].as_str()).unwrap_or(if view.timer_mode=="pomodoro"{"留一点时间，专注当下"}else if view.timer_mode=="countdown"{"为下一件事留好时间"}else{"从此刻开始记录"});
    let mut hero=div().flex().flex_col().items_center().p_5().gap_3().rounded(px(26.)).bg(t::surface()).child(modes)
        .child(div().mt_2().text_size(px(13.)).text_color(t::text_dim()).child(name.to_owned()))
        .child(div().relative().w_full().h(px(248.)).child(clock_dial(p,accent)).child(div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap_2().child(div().text_size(px(if clock.len()>5{38.}else{50.})).font_weight(FontWeight::LIGHT).text_color(t::text()).child(clock)).child(div().text_size(px(11.)).text_color(t::text_dim()).child(subtitle))));
    if view.timer_mode=="pomodoro"{let round=current.and_then(|c|c["round"].as_u64()).unwrap_or(1);let rounds=current.and_then(|c|c["rounds"].as_u64()).unwrap_or_else(||view.timer_rounds.read(cx).text().parse().unwrap_or(4)).min(16);let mut dots=div().flex().gap_2().mb_1();for i in 1..=rounds{dots=dots.child(div().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(if i<=round{accent}else{t::track()}));}hero=hero.child(dots);}
    let reset=id.clone();let toggle=id.clone();
    hero=hero.child(div().flex().items_center().justify_center().gap_4()
        .child(div().id("clock-reset").w(px(72.)).h(px(44.)).rounded(px(22.)).bg(t::field()).flex().items_center().justify_center().text_size(px(12.)).text_color(t::text_dim()).child("重置").cursor_pointer().active(|s|s.opacity(0.6)).on_click(cx.listener(move|v,_,_,cx|{if let Some(id)=&reset{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"reset"}),cx)}})))
        .child(div().id("timer-start").w(px(150.)).h(px(46.)).rounded(px(23.)).bg(accent).flex().items_center().justify_center().font_weight(FontWeight::SEMIBOLD).text_size(px(14.)).text_color(rgb(0x24312F)).child(if running{"Ⅱ  暂停"}else if current.is_some_and(|c|c["finished"]==true){"▷  重新开始"}else if id.is_some(){"▷  继续"}else{"▷  开始"}).cursor_pointer().active(|s|s.opacity(0.7)).on_click(cx.listener(move|v,_,_,cx|{if let Some(id)=&toggle{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"toggle"}),cx)}else{v.create_timer(cx)}}))));
    if id.is_none()&&view.timer_mode!="stopwatch"{let mut presets=div().flex().justify_center().gap_2().mt_2();let options=if view.timer_mode=="pomodoro"{[25,45,60]}else{[5,10,15]};for minutes in options{presets=presets.child(div().id(("clock-preset",minutes as usize)).px_3().py_2().rounded(px(14.)).bg(t::field()).text_size(px(11.)).text_color(t::text_dim()).child(format!("{minutes} 分钟")).cursor_pointer().on_click(cx.listener(move|v,_,_,cx|{let field=if v.timer_mode=="pomodoro"{v.timer_focus.clone()}else{v.timer_duration.clone()};field.update(cx,|f,cx|f.set_text(&(minutes*60).to_string(),cx));cx.notify();})));}hero=hero.child(presets);}
    hero=hero.child(div().flex().justify_center().gap_4().mt_2().child(small_button("clock-adjust".into(),if view.timer_adjust_open{"收起调整  ⌃"}else{"调整  ⌄"},cx.listener(|v,_,_,cx|{v.timer_adjust_open=!v.timer_adjust_open;cx.notify();}))).child(small_button("clock-new".into(),"＋ 新计时",cx.listener(|v,_,_,cx|{v.timer_new=true;cx.notify();}))));
    let mut rows=vec![hero.into_any_element()];
    if view.timer_adjust_open{let mut fields=div().flex().flex_col().gap_3().p_5().rounded(px(20.)).bg(t::surface()).child(div().text_size(px(12.)).text_color(t::text_dim()).child("调整用于新计时，不会改变正在运行的项目。" )).child(field_line("名称",300.,view.timer_name.clone()));if view.timer_mode=="countdown"{fields=fields.child(field_line("倒计时（秒，最多 24 小时）",250.,view.timer_duration.clone()));}else if view.timer_mode=="pomodoro"{fields=fields.child(div().flex().flex_wrap().gap_3().child(field_line("专注（秒）",140.,view.timer_focus.clone())).child(field_line("休息（秒）",140.,view.timer_rest.clone())).child(field_line("轮数",100.,view.timer_rounds.clone())));}rows.push(fields.child(small_button("clock-apply".into(),"使用这些设置新建",cx.listener(|v,_,_,cx|{v.timer_new=true;v.timer_adjust_open=false;cx.notify();}))).into_any_element());}
    if !clocks.is_empty(){rows.push(section("我的计时"));}
    for timer in &clocks{let id=timer["id"].as_str().unwrap_or("").to_owned();let show=id.clone();let remove=id.clone();let mode=timer["mode"].as_str().unwrap_or("countdown").to_owned();let selected=Some(id.as_str())==current.and_then(|c|c["id"].as_str());
        rows.push(div().flex().items_center().gap_3().p_4().rounded(px(18.)).bg(t::surface()).border_1().border_color(if selected{t::hairline_bright()}else{t::hairline()})
            .child(div().id(SharedString::from(format!("clock-select-{id}"))).flex_1().flex().items_center().gap_4().cursor_pointer().child(div().w(px(4.)).h(px(32.)).rounded(px(2.)).bg(if timer["running"]==true{accent}else{t::track()})).child(div().flex_1().flex().flex_col().gap_1().child(div().text_size(px(13.)).text_color(t::text()).child(timer["name"].as_str().unwrap_or("计时器").to_owned())).child(div().text_size(px(10.)).text_color(t::text_dim()).child(timer["subtitle"].as_str().unwrap_or("").to_owned()))).child(div().text_size(px(23.)).font_weight(FontWeight::LIGHT).text_color(t::text()).child(timer["clock"].as_str().unwrap_or("00:00").to_owned())).on_click(cx.listener(move|v,_,_,cx|{v.timer_mode=mode.clone();v.timer_new=false;send_todo(&v.shared,"islandSelect",json!(format!("timer:{show}")),cx);cx.notify();})))
            .child(small_button(SharedString::from(format!("clock-delete-{id}")),"移除",cx.listener(move|v,_,_,cx|send_todo(&v.shared,"timerAction",json!({"id":remove,"action":"delete"}),cx)))).into_any_element());
    }
    rows.push(info_row("与灵动岛同步","多个计时可同时运行","点选下方计时卡片即可显示在灵动岛。重启后保留进度并暂停。"));rows
}

