//! Expressive timer tools: a compact shared player and distinct mode configuration.
use crate::{widgets::Ui,model::{Model,Action},draw::{self,rect,Icon}};
pub fn render(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
 let s=ui.s;let tabs=["番茄钟","倒计时","秒表","流程"];let tw=(w-12.)/4.;
 for(i,label)in tabs.iter().enumerate(){let r=rect(x+i as f64*(tw+4.),y,tw,48.);draw::fill(ui.scene,r,if i==0{24.}else{14.},if m.mode as usize==i{s.primary}else{s.container});let hit=ui.hit(r,label,Action::Mode(i as u8),true);ui.state(r,14.,hit,s.on_surface);ui.center(label,r,15.,if m.mode as usize==i{s.on_primary}else{s.on_surface_variant},m.mode as usize==i);}
 let y=y+64.;if m.mode==3{return crate::workflow::hero_full(ui,m,x,y,w)}
 let t=m.selected_timer().filter(|t|t.mode==match m.mode{0=>"pomodoro",1=>"countdown",_=>"stopwatch"});
 let gap=16.;let left=((w-gap)*0.52).max(304.);let rx=x+left+gap;let right=w-left-gap;
 let right_h=288.;
 ui.panel(rect(rx,y,right,right_h),s.container);ui.label(if m.mode==2{"记录每一段投入"}else{"设定你的节奏"},rx+20.,y+18.,20.,s.on_surface,true,right-40.);
 if m.mode==2{draw::icon(ui.scene,Icon::Focus,rx+right/2.-26.,y+72.,52.,s.primary);ui.center("向前计时，随时暂停",rect(rx+16.,y+142.,right-32.,24.),14.,s.on_surface_variant,false);ui.center("可同时运行多个计时器",rect(rx+16.,y+178.,right-32.,24.),12.,s.on_surface_variant,false);}
 else{ui.center(&if m.timer_seconds.is_some_and(|s|s%60!=0){dotisle_domain::timers::format_time(m.timer_seconds.unwrap()as u64)}else{m.minutes.to_string()},rect(rx+64.,y+54.,right-128.,64.),if m.timer_seconds.is_some_and(|s|s%60!=0){36.}else{48.},s.on_surface,true);ui.icon_button(rect(rx+16.,y+66.,44.,44.),Icon::Back,"减少一分钟",Action::Preset(m.minutes.saturating_sub(1).max(1)),false,m.minutes>1);ui.icon_button(rect(rx+right-60.,y+66.,44.,44.),Icon::Plus,"增加一分钟",Action::Preset((m.minutes+1).min(240)),false,m.minutes<240);ui.center(if m.timer_seconds.is_some_and(|s|s%60!=0){"分 : 秒"}else{"分钟"},rect(rx+20.,y+118.,right-40.,24.),12.,s.on_surface_variant,false);
 let cw=(right-44.)/2.;for(i,min)in [5,15,25,45].iter().enumerate(){ui.chip(rect(rx+18.+(i%2)as f64*(cw+8.),y+154.+(i/2)as f64*48.,cw,40.),&format!("{min} 分钟"),m.minutes==*min,Action::Preset(*min));}}
 if m.mode<2{ui.button(rect(rx+right-132.,y+248.,112.,32.),"调整参数",None,Action::TimerPreferences,true,true);ui.label(&if m.mode==0{format!("{} 轮 · 休息 {} 秒",m.timer_rounds,m.rest_seconds)}else{"到时提醒".into()},rx+20.,y+256.,12.,s.on_surface_variant,false,right-164.);}else{if let Some(timer)=t{ui.button(rect(rx+20.,y+212.,right-40.,40.),"记录分段",None,Action::Timer(timer.id.clone(),"lap"),true,timer.running);}ui.label("暂停后可接着记录",rx+20.,y+264.,12.,s.on_surface_variant,false,right-40.);}
 // Both cards start on the same line; the dial no longer floats against a taller editor.
 let left_h=288.;let left_y=y;
 ui.panel(rect(x,left_y,left,left_h),s.primary_container);
 let title=t.map(|t|t.name.as_str()).unwrap_or(tabs[m.mode.min(3)as usize]);ui.label(title,x+24.,left_y+18.,16.,s.on_primary_container,true,left-48.);
 let clock=t.map(|t|t.clock()).unwrap_or_else(||if m.mode==2{"00:00".into()}else{dotisle_domain::timers::format_time(m.timer_seconds.unwrap_or(m.minutes*60)as u64)});
 ui.center(&clock,rect(x+20.,left_y+56.,left-40.,76.),if clock.len()>5{44.}else{60.},s.on_primary_container,true);
 let subtitle=t.map(|t|t.subtitle()).unwrap_or_else(||"准备开始".into());ui.center(&subtitle,rect(x+20.,left_y+136.,left-40.,24.),12.,s.on_primary_container,false);
 let progress=t.map(|t|t.progress()).unwrap_or(0.);let track=rect(x+24.,left_y+176.,left-48.,12.);draw::fill(ui.scene,track,6.,s.outline);
 let fill=(track.width()*progress.clamp(0.,1.)).max(0.);if fill>0.{draw::fill(ui.scene,rect(track.x0,track.y0,fill,track.height()),6.,s.primary);}draw::circle(ui.scene,track.x1-5.,track.center().y,2.,s.on_primary_container);
 let label=t.map(|t|match t.primary_action(){"pause"=>"暂停","resume"=>"继续","next"=>"下一阶段",_=>"重新开始"}).unwrap_or("开始计时");let action=t.map(|t|Action::Timer(t.id.clone(),t.primary_action())).unwrap_or(Action::StartTimer);
 ui.button(rect(x+24.,left_y+214.,left-104.,52.),label,Some(if t.is_some_and(|t|t.running){Icon::Pause}else{Icon::Play}),action,false,true);
 ui.icon_button(rect(x+left-68.,left_y+218.,44.,44.),Icon::Reset,"重置计时",t.map(|t|Action::Timer(t.id.clone(),"reset")).unwrap_or(Action::Preset(25)),false,t.is_some());
 let bottom=y+right_h.max(left_h);

 let mut cy=bottom+24.;if m.mode==2{if let Some(timer)=t{if !timer.laps.is_empty(){ui.label("分段记录",x+4.,cy,20.,s.on_surface,true,w-8.);cy+=40.;for(index,lap)in timer.laps.iter().enumerate().rev().take(10){let previous=if index==0{0}else{timer.laps[index-1]};ui.panel(rect(x,cy,w,48.),s.container);ui.label(&format!("第 {} 段",index+1),x+20.,cy+13.,14.,s.on_surface,false,120.);ui.label(&format!("{} · 总计 {}",dotisle_domain::timers::format_time((lap-previous)/1000),dotisle_domain::timers::format_time(lap/1000)),x+160.,cy+13.,14.,s.on_surface_variant,false,w-180.);cy+=56.;}cy+=12.;}}}ui.label("计时列表",x+4.,cy+8.,20.,s.on_surface,true,w-176.);ui.button(rect(x+w-148.,cy,148.,44.),"新建计时",Some(Icon::Plus),Action::StartTimer,true,m.timers.items.len()<8);cy+=60.;
 for t in &m.timers.items{ui.panel(rect(x,cy,w,88.),s.container);let select=rect(x,cy,w-132.,88.);let hit=ui.hit(select,&t.name,Action::SelectTimer(t.id.clone()),true);ui.state(select,28.,hit,s.on_surface);draw::fill(ui.scene,rect(x+16.,cy+20.,4.,48.),2.,if t.running{s.primary}else{s.outline});ui.label(&t.name,x+36.,cy+15.,16.,s.on_surface,true,w-340.);ui.label(&t.subtitle(),x+36.,cy+49.,12.,s.on_surface_variant,false,w-340.);ui.label(&t.clock(),x+w-300.,cy+25.,26.,s.primary,true,160.);ui.icon_button(rect(x+w-120.,cy+22.,44.,44.),if t.running{Icon::Pause}else{Icon::Play},"切换计时状态",Action::Timer(t.id.clone(),t.primary_action()),false,true);ui.icon_button(rect(x+w-64.,cy+22.,44.,44.),Icon::Trash,"移除计时器",Action::Timer(t.id.clone(),"delete"),false,true);cy+=100.;}
 if m.timers.items.is_empty(){ui.label("还没有计时，开始一段属于自己的时间。",x+4.,cy+8.,13.,s.on_surface_variant,false,w);cy+=44.;}cy
}
