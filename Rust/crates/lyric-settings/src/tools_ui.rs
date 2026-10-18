// Organised settings, reminder cards and clocks. Shares the existing native IPC contract.
fn mix_ui(a:gpui::Rgba,b:gpui::Rgba,k:f32)->gpui::Rgba{gpui::Rgba{r:a.r+(b.r-a.r)*k,g:a.g+(b.g-a.g)*k,b:a.b+(b.b-a.b)*k,a:1.}}
/// The three card colours by name, so a select can speak 柔黄 / 薄荷 / 浅紫.
fn color_name(hex:&str)->String{match hex.to_ascii_uppercase().as_str(){"#F3D79B"=>"柔黄".into(),"#B7DBCB"=>"薄荷".into(),"#C9BCE5"=>"浅紫".into(),other=>other.to_owned()}}
/// The todo gear drawer. It edits the exact same keys the settings page would — there is only one
/// definition of each preference, and 前往音效 forwards to the settings page with a way back.
fn todo_settings_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{vec![select_row("todo-default-color","新事项默认颜色","",color_name(setting_str(state,"TodoDefaultColor","#F3D79B")).as_str(),view,cx),toggle_row("TodoRemindersEnabled","到期提醒","独立于应用通知总开关；暂停通知会暂时抑制提醒。",flag(state,"TodoRemindersEnabled",true),view, cx),toggle_row("TodoIslandEnabled","在灵动岛显示待办组件","有未完成项目时，可切换到待办胶囊。",flag(state,"TodoIslandEnabled",true),view, cx),button_row("todo-audio-link","提醒音效","与全局音效设置共用。","前往音效",cx,|v,cx|v.go_sound(cx)),div().py(px(16.)).text_size(px(12.)).text_color(t::text_dim()).child("单个事项的日期、重复和内容，在事项编辑中调整。").into_any_element()]}
fn ordinal(s:&str)->Option<i64>{let p=s.split('-').map(|p|p.parse::<i64>().ok()).collect::<Option<Vec<_>>>()?;if p.len()!=3{return None}let(y,m,d)=(p[0],p[1],p[2]);if !(1..=12).contains(&m)||!(1..=31).contains(&d){return None}let before=y-1;let mut days=before*365+before/4-before/100+before/400;let months=[31,28,31,30,31,30,31,31,30,31,30,31];for i in 0..(m-1)as usize{days+=months[i];}if m>2&&(y%400==0||(y%4==0&&y%100!=0)){days+=1;}Some(days+d)}
fn todo_matches(todo:&Value,filter:usize,state:&Value)->bool{let done=todo["completed"].as_bool().unwrap_or(false);if done{return false}let date=todo["date"].as_str().unwrap_or("");let today=text(state,"today","");let time=todo["time"].as_str().unwrap_or("");let now=text(state,"now","");let daily=todo["daily"].as_bool().unwrap_or(false);let overdue=if daily{date<=today&&time<now}else{date<today||(date==today&&time<now)};match filter{1=>!overdue,2=>overdue,3=>date==today||(daily&&date<=today),4=>daily&&date<=today||ordinal(date).zip(ordinal(today)).is_some_and(|(d,t)|d>=t&&d<t+7),_=>true}}
/// One todo card, as the handoff draws it: a colour bar, a clickable title block, the two green
/// quick actions and the round complete button. `brief` drops the actions for the done list.
fn todo_card(todo:&Value,brief:bool,state:&Value,cx:&mut Context<SettingsApp>)->AnyElement{
 let id=todo["id"].as_str().unwrap_or("").to_owned();
 let (edit,toggle)=(id.clone(),id.clone());
 let done=todo["completed"].as_bool().unwrap_or(false);
 let color=todo["color"].as_str().unwrap_or("#F3D79B");
 let (background,bar)=match color.to_ascii_uppercase().as_str(){
  "#B7DBCB"=>(t::mint(),t::accent()),
  "#C9BCE5"=>(t::lilac(),t::lilac_bar()),
  "#F3D79B"=>(t::yellow(),t::yellow_line()),
  other=>(mix_ui(t::surface(),t::parse(other),if t::base().r<0.3{0.28}else{0.35}),t::parse(other)),
 };
 let (background,bar)=if done{(t::field(),t::yellow_line())}else{(background,bar)};
 let date=todo["date"].as_str().unwrap_or("");let date=if date.len()>=10&&date.as_bytes()[4]==b'-'{&date[5..]}else{date};
 let mut meta=format!("{} {}{}",date,todo["time"].as_str().unwrap_or(""),if todo["daily"]==true{" · 每天"}else{""});
 if done{meta.push_str(" · 已完成");}else if todo_matches(todo,2,state){meta.push_str(" · 已过期");}
 let mut title_row=div().id(SharedString::from(format!("todo-open-{id}"))).flex().flex_col().w_full().cursor_pointer()
  .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(if done{t::text_dim()}else{t::text()}).child(todo["title"].as_str().unwrap_or("待办").to_owned()));
 if let Some(content)=todo["content"].as_str().filter(|s|!s.is_empty()){title_row=title_row.child(div().mt(px(4.)).text_size(px(12.)).text_color(t::text_dim()).child(content.to_owned()));}
 let mut grow=div().flex().flex_col().flex_1().min_w_0()
  .child(title_row.child(div().mt(px(4.)).text_size(px(12.)).text_color(t::text_dim()).child(meta)).on_click(cx.listener(move|v,_,_,cx|v.open_todo_form(Some(edit.clone()),cx))));
 if !brief&&!done{
  let (focus,snooze)=(id.clone(),id.clone());
  grow=grow.child(div().flex().items_center().gap(px(10.)).mt(px(9.))
   .child(div().id(SharedString::from(format!("todo-focus-{id}"))).flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(t::accent()).cursor_pointer().hover(|s|s.opacity(0.72)).child(icons::tinted("play", t::accent())).child("开始专注").on_click(cx.listener(move|v,_,_,cx|{v.drawer=Some(Drawer::Focus(focus.clone()));cx.notify();})))
   .child(div().id(SharedString::from(format!("todo-snooze-{id}"))).flex().items_center().gap(px(5.)).text_size(px(12.)).text_color(t::accent()).cursor_pointer().hover(|s|s.opacity(0.72)).child(icons::tinted("alarm-clock", t::accent())).child("稍后提醒").on_click(cx.listener(move|v,_,_,cx|{v.drawer=Some(Drawer::Snooze(snooze.clone()));cx.notify();}))));
 }
 div().id(SharedString::from(format!("todo-{id}"))).flex().items_center().gap(px(13.)).w_full().min_w_0().px(px(17.)).py(px(15.)).rounded(px(15.)).bg(background)
  .child(div().w(px(4.)).h(px(36.)).flex_shrink_0().rounded(px(3.)).bg(bar))
  .child(grow)
  .child(div().id(SharedString::from(format!("todo-toggle-{id}"))).w(px(25.)).h(px(25.)).flex_shrink_0().rounded(px(13.)).border_1().border_color(if done{t::accent()}else{t::text_faint()}).bg(if done{t::accent()}else{background}).flex().items_center().justify_center().text_color(if done{t::on_accent()}else{t::text_faint()}).cursor_pointer().active(|s|s.opacity(0.6)).child(if done{icons::tinted("check", t::on_accent()).into_any_element()}else{div().into_any_element()}).on_click(cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoComplete",json!({"id":toggle,"completed":!done}),cx);cx.notify();}))).into_any_element()
}
/// The list itself: the five filters, the tally, the cards and the fold of done items. 新建 and
/// the gear live in the page head; the editor lives in the shared drawer.
fn new_todo_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let todos=state["todos"].as_array().cloned().unwrap_or_default();
 let remaining=todos.iter().filter(|t|t["completed"]!=true).count();
 let completed=todos.len()-remaining;
 let mut rows=vec![tab_row("todo-filter",&["全部","进行中","已过期","今天","最近 7 天"],view.todo_filter.min(4),cx,|v,index|v.todo_filter=index)];
 rows.push(div().flex().items_center().justify_between().gap_3().mb(px(10.0)).child(div().text_size(px(12.)).text_color(t::text_dim()).child(format!("{remaining} 项待完成"))).into_any_element());
 let mut list=todos.iter().filter(|t|todo_matches(t,view.todo_filter,state)).collect::<Vec<_>>();
 list.sort_by_key(|t|format!("{} {}",t["date"],t["time"]));
 if list.is_empty(){rows.push(panel(vec![empty_row("这个分类下没有待办")]));}else{rows.push(card_stack(list.iter().map(|t|todo_card(t,false,state,cx)).collect()));}
 if completed>0{
  let open=view.todo_completed_open;
  rows.push(div().flex().items_center().justify_between().gap_3().mt(px(24.0)).mb(px(10.0))
   .child(div().id("todo-completed-link").flex().items_center().gap(px(4.)).text_size(px(12.)).text_color(t::accent()).cursor_pointer().hover(|style|style.opacity(0.72))
    .child(format!("已完成 · {completed} 项"))
    .child(icons::tinted(if open{"chevron-up"}else{"chevron-down"}, t::accent()))
    .on_click(cx.listener(|v,_,_,cx|{v.todo_completed_open=!v.todo_completed_open;cx.notify();})))
   .into_any_element());
  if open{rows.push(card_stack(todos.iter().filter(|t|t["completed"]==true).map(|t|todo_card(t,true,state,cx)).collect()));}
 }
 if let Some(error)=state["todoError"].as_str(){rows.push(info_row("待办存储异常","原文件已保留",error,view,cx));}
 rows
}
/// 稍后提醒: postpone the next banner without touching the todo itself.
fn snooze_rows(id:&str,state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{let todo=state["todos"].as_array().and_then(|list|list.iter().find(|todo|todo["id"].as_str()==Some(id)));let title=todo.and_then(|todo|todo["title"].as_str()).unwrap_or("这条待办").to_owned();let daily=todo.is_some_and(|todo|todo["daily"]==true);let first=id.to_owned();let second=id.to_owned();let mut rows=vec![info_row("稍后提醒",title.as_str(),"从当前时刻起推迟这一次提醒；到点后会再次提醒。",view,cx),button_row("snooze-10","10 分钟后提醒","适合手头这件小事马上就好时。","10 分钟后",cx,move|v,cx|{send_todo(&v.shared,"todoSnooze",json!({"id":first.clone(),"minutes":10}),cx);v.drawer=None;cx.notify();}),button_row("snooze-60","1 小时后提醒","适合需要先专注一段时间再处理时。","1 小时后",cx,move|v,cx|{send_todo(&v.shared,"todoSnooze",json!({"id":second.clone(),"minutes":60}),cx);v.drawer=None;cx.notify();})];if daily{rows.push(info_row("每日待办","只推迟今天这一次","明天会重新按原定时刻提醒。",view,cx));}rows}
/// 开始专注: start (or re-show) a pomodoro linked to this todo through `todoId`.
fn focus_rows(id:&str,state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{let todo=state["todos"].as_array().and_then(|list|list.iter().find(|todo|todo["id"].as_str()==Some(id)));let title=todo.and_then(|todo|todo["title"].as_str()).unwrap_or("这条待办").to_owned();let existing=state["timers"].as_array().and_then(|list|list.iter().find(|timer|timer["todoId"].as_str()==Some(id)&&timer["finished"]!=true)).and_then(|timer|timer["id"].as_str()).map(str::to_owned);let minutes=view.focus_minutes;let mut rows=vec![info_row("开始专注",title.as_str(),if existing.is_some(){"这个待办已经有正在进行的计时，直接回到它。"}else{"创建一个番茄钟；专注结束不会自动完成待办。"},view,cx)];
 let mut pills=div().flex().gap_2();for option in [25u32,45,60]{let selected=minutes==option;pills=pills.child(div().id(("focus-preset",option as usize)).px_3().py_2().rounded(px(14.)).bg(if selected{t::accent_dim()}else{t::field()}).border_1().border_color(if selected{t::accent()}else{t::hairline()}).text_size(px(12.)).text_color(if selected{t::accent()}else{t::text_dim()}).cursor_pointer().child(format!("{option} 分钟")).on_click(cx.listener(move|v,_,_,cx|{v.focus_minutes=option;cx.notify();})));}rows.push(card("focus-presets","专注时长","到点后提醒一次，然后进入 5 分钟休息。").child(pills).into_any_element());
 match existing{Some(timer)=>rows.push(button_row("focus-existing","继续已有的专注","这个待办已经有正在进行的计时。","显示在灵动岛",cx,move|v,cx|{send_todo(&v.shared,"islandSelect",json!(timer.clone()),cx);v.drawer=None;cx.notify();})),None=>{let todo_id=id.to_owned();rows.push(button_row("focus-start","开始专注",&format!("{} 分钟的番茄钟，结束时提醒一次。",minutes),"开始",cx,move|v,cx|{send_todo(&v.shared,"timerCreate",json!({"mode":"pomodoro","todoId":todo_id.clone(),"name":"","duration":minutes*60,"focus":minutes*60,"rest":300,"rounds":4}),cx);v.drawer=None;cx.notify();}));}}
 rows.push(info_row("结束后","手动勾选完成","计时结束时只提醒，不会改变待办状态。",view,cx));rows}
include!("timer_ui.rs");
impl SettingsApp{fn create_timer(&mut self,cx:&mut Context<Self>){let number=|f:&Entity<TextField>|f.read(cx).text().trim().parse::<u64>().ok();let (Some(duration),Some(focus),Some(rest),Some(rounds))=(number(&self.timer_duration),number(&self.timer_focus),number(&self.timer_rest),number(&self.timer_rounds))else{if let Ok(mut s)=self.shared.lock(){s.message="请使用整数填写计时时长与轮数".into();}cx.notify();return};self.timer_new=false;send_todo(&self.shared,"timerCreate",json!({"mode":self.timer_mode,"name":self.timer_name.read(cx).text(),"duration":duration,"focus":focus,"rest":rest,"rounds":rounds,"countUp":self.timer_mode=="pomodoro"&&setting(&self.state(),"FocusCountUp").as_bool().unwrap_or(false)}),cx);}}
#[cfg(test)]mod tools_ui_tests{use super::{ordinal,todo_matches};use serde_json::json;#[test]fn date_ranges_cross_year_and_leap_day(){assert_eq!(ordinal("2027-01-01").unwrap()-ordinal("2026-12-31").unwrap(),1);assert_eq!(ordinal("2028-03-01").unwrap()-ordinal("2028-02-28").unwrap(),2);}#[test]fn daily_tasks_can_appear_today_but_completed_items_do_not(){let s=json!({"today":"2026-10-16","now":"12:00"});let mut t=json!({"date":"2026-01-01","time":"09:00","daily":true,"completed":false});assert!(todo_matches(&t,3,&s));assert!(todo_matches(&t,2,&s));t["completed"]=json!(true);assert!(!todo_matches(&t,0,&s));}}

/// Many compact buttons need the full row width; in the drawer the shared `card` row squeezes its
/// text column to nothing, so these two stack the buttons under the copy instead.
fn test_card(id:&'static str,title:&'static str,detail:&'static str,buttons:Div)->AnyElement{
 div().id(id).flex().flex_col().items_start().gap_2().py(px(16.0)).min_w_0().mx(px(-18.0)).px(px(18.0))
  .child(div().text_size(px(14.)).text_color(t::text()).child(title))
  .child(div().text_size(px(12.)).text_color(t::text_dim()).child(detail))
  .child(buttons.w_full().min_w_0())
  .into_any_element()
}
fn device_test_row(cx:&mut Context<SettingsApp>)->AnyElement{let mut buttons=div().flex().flex_wrap().gap_2();for(kind,connected,label)in[("usb",true,"U 盘连接"),("phone",true,"手机连接"),("headphones",true,"耳机连接"),("speaker",true,"音响连接"),("gamepad",true,"手柄连接"),("mouse",true,"鼠标连接"),("keyboard",true,"键盘连接"),("usb",false,"设备移除")]{buttons=buttons.child(small_button(SharedString::from(format!("device-test-{kind}-{connected}")),label,cx.listener(move|v,_,_,cx|send_todo(&v.shared,"devicePreview",json!({"kind":kind,"connected":connected}),cx))));}test_card("device-tests","模拟连接提醒","显示示例弹窗，不是真实硬件检测；无需插入设备。音效可在音效页试听。",buttons)}
fn system_test_row(cx:&mut Context<SettingsApp>)->AnyElement{let mut buttons=div().flex().flex_wrap().gap_2();for(kind,on,label)in[("caps",true,"大写开启"),("caps",false,"大写关闭"),("num",true,"小键盘开启"),("num",false,"小键盘关闭"),("mic",true,"麦克风静音"),("mic",false,"麦克风取消静音"),("touchpad",true,"触控板开启"),("touchpad",false,"触控板关闭")]{buttons=buttons.child(small_button(SharedString::from(format!("osd-test-{kind}-{on}")),label,cx.listener(move|v,_,_,cx|send_todo(&v.shared,"osdPreview",json!({"kind":kind,"on":on}),cx))));}test_card("osd-tests","模拟系统提示","模拟八种状态，不是硬件检测；真实状态来自锁定键、默认麦克风与触控板配置。",buttons)}
