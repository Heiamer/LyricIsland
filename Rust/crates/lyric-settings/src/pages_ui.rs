// Overview and notification history. Both read only from the state snapshot, so they work while
// the pipe is reconnecting and never hold a lock across a frame.
const NOTICE_FILTERS:[(&str,&str);5]=[("all","全部"),("notice","应用消息"),("todo","待办提醒"),("device","设备"),("automation","应用提醒")];
fn kind_label(kind:&str)->&'static str{match kind{"notice"=>"应用消息","todo"=>"待办提醒","timer"=>"计时","device"=>"设备","automation"=>"应用提醒",_=>"消息"}}
fn kind_icon(kind:&str)->&'static str{match kind{"notice"=>"message-circle","todo"=>"bell-ring","timer"=>"timer","device"=>"usb","automation"=>"bell-ring",_=>"bell"}}

/// 概览：the glance view — next reminder, running timer and the two newest messages.
fn overview_rows(state:&Value,_view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let mut rows=Vec::new();
 rows.extend(quick_ui::focus_resume_rows(state,cx));
 let add=pill_button_icon(SharedString::from("overview-quick-open"),Some("plus"),"添加",false,cx.listener(|v,_,_,cx|{v.drawer=Some(Drawer::QuickReminder);cx.notify();}));
 rows.push(div().w_full().mb(px(22.)).child(
  raised(18.0).p(px(14.)).flex().flex_row().items_center().gap(px(12.))
   .child(div().w(px(34.)).h(px(34.)).flex_shrink_0().rounded(px(11.)).bg(t::green_soft()).flex().items_center().justify_center().child(icons::tinted("plus",t::accent())))
   .child(div().flex().flex_col().gap(px(2.)).flex_1().min_w_0()
    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child("一句话快速提醒"))
    .child(div().text_size(px(11.5)).text_color(t::text_dim()).child("例如：20 分钟后取快递 · 明天 9:00 交稿。")))
   .child(add)
 ).into_any_element());
 let today=text(state,"today","");
 let next=state["todos"].as_array().and_then(|list|list.iter().filter(|todo|todo["completed"]!=true).min_by_key(|todo|format!("{} {}",todo["date"].as_str().unwrap_or(""),todo["time"].as_str().unwrap_or("")))).cloned();
 let body=match next{
  Some(todo)=>{
   let id=todo["id"].as_str().unwrap_or("").to_owned();
   let date=todo["date"].as_str().unwrap_or("");
   let badge=if today.is_empty(){date}else if date<today{"已过期"}else if date==today||todo["daily"]==true{"今天"}else{date};
   let time=todo["time"].as_str().unwrap_or("");
   let (done,later)=(id.clone(),id.clone());
   let actions=div().flex().flex_wrap().gap_2()
    .child(pill_button(SharedString::from("overview-done"),"完成",false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoComplete",json!({"id":done.clone(),"completed":true}),cx);cx.notify();})))
    .child(pill_button(SharedString::from("overview-later"),"10 分钟后",false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoSnooze",json!({"id":later.clone(),"minutes":10}),cx);cx.notify();})))
    .into_any_element();
   let trailing=if badge.is_empty(){None}else{Some(pill(badge))};
   let line=taskcheck(symbol("bell-ring",t::surface(),t::text()),todo["title"].as_str().unwrap_or("待办"),&format!("{time} · 待办提醒"),trailing);
   inset_card(true,line,Some(actions))
  }
  None=>{
   let line=div().flex().flex_col().gap_1().min_w_0()
    .child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child("暂时没有待处理的事"))
    .child(div().text_size(px(12.)).text_color(t::text_dim()).child("需要时，再安排下一条提醒。"));
   inset_card(false,line.into_any_element(),None)
  }
 };
 rows.push(section_block("接下来",Some(("overview-next-link","管理待办",Route::Todos)),body,cx));
 let timers=state["timers"].as_array().cloned().unwrap_or_default();
 let current=timers.iter().find(|timer|timer["running"]==true&&timer["finished"]!=true).or_else(||timers.iter().find(|timer|timer["finished"]!=true));
 let body=match current{
  Some(timer)=>{
   let id=timer["id"].as_str().unwrap_or("").to_owned();
   let running=timer["running"]==true;
   let toggle=id.clone();
   let detail=timer["subtitle"].as_str().filter(|text|!text.is_empty()).unwrap_or(if running{"计时进行中"}else{"计时已暂停"}).to_owned();
   let line=taskcheck(symbol("timer",t::green_soft(),t::accent()),timer["name"].as_str().unwrap_or("计时器"),&detail,Some(minitime(timer["clock"].as_str().unwrap_or("00:00"))));
   let actions=div().flex().flex_wrap().gap_2()
    .child(pill_button_icon(SharedString::from("overview-timer-toggle"),Some(if running{"pause"}else{"play"}),if running{"暂停"}else{"继续"},false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"timerAction",json!({"id":toggle.clone(),"action":"toggle"}),cx);cx.notify();})))
    .into_any_element();
   inset_card(false,line,Some(actions))
  }
  None=>{
   let line=taskcheck(symbol("timer",t::surface_hi(),t::text_dim()),"没有正在进行的计时","流程与专注里可以开始一个番茄钟。",None);
   inset_card(false,line,None)
  }
 };
 rows.push(section_block("正在进行",Some(("overview-timer-link","打开计时器",Route::Timers)),body,cx));
 let recent=state["notificationHistory"].as_array().map(|list|list.iter().take(2).cloned().collect::<Vec<_>>()).unwrap_or_default();
 let body=if recent.is_empty(){list(vec![empty_row("没有新的通知")])}else{list(recent.iter().map(|event|notice_row(event,cx)).collect())};
 rows.push(section_block("最近通知",Some(("overview-notice-link","查看全部",Route::Notifications)),body,cx));
 rows
}

/// 通知：the history list first; its knobs live behind the gear so the page stays a reading surface.
fn notify_page_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let mut rows=vec![tab_row("notice-view",&["通知历史","应用提醒"],view.notice_view.min(1),cx,|v,index|{v.notice_view=index;v.clear_armed=false;})];
 if view.notice_view==1{rows.extend(rules_ui::rules_page_rows(state,view,cx));return rows}
 let events=state["notificationHistory"].as_array().cloned().unwrap_or_default();
 rows.push(tab_row("notice-filter",&["全部","应用消息","待办提醒","设备","应用提醒"],view.notice_filter.min(NOTICE_FILTERS.len()-1),cx,|v,index|{v.notice_filter=index;v.clear_armed=false;}));
 let kind=NOTICE_FILTERS[view.notice_filter.min(NOTICE_FILTERS.len()-1)].0;
 let shown=events.iter().filter(|event|kind=="all"||event["kind"].as_str()==Some(kind)).collect::<Vec<_>>();
 let clear_kind=if kind=="all"{String::new()}else{kind.to_owned()};
 let armed=view.clear_armed;
 let today=text(state,"today","");
 let all_today=!shown.is_empty()&&shown.iter().all(|event|event["date"].as_str()==Some(today));
 let caption=if shown.is_empty()||all_today{format!("今天 · {} 条",shown.len())}else{format!("共 {} 条",shown.len())};
 rows.push(div().flex().items_center().justify_between().gap_3().mb(px(10.0))
  .child(div().text_size(px(12.)).text_color(t::text_dim()).child(caption))
  .child(div().id("notice-clear").flex().items_center().gap(px(5.0)).text_size(px(12.)).text_color(t::accent()).cursor_pointer()
   .hover(|style|style.opacity(0.72))
   .child(icons::tinted("trash-2", t::accent()))
   .child(if armed{"确认清除"}else{"清除当前记录"})
   .on_click(cx.listener(move|v,_,_,cx|{if !v.clear_armed{v.clear_armed=true;cx.notify();return}send_todo(&v.shared,"historyClear",json!({"kind":clear_kind.clone()}),cx);v.clear_armed=false;cx.notify();})))
  .into_any_element());
 let body=if shown.is_empty(){list(vec![empty_row("没有通知记录")])}else{list(shown.iter().map(|event|notice_row(event,cx)).collect())};
 rows.push(body);
 if let Some(error)=state["historyError"].as_str().filter(|error|!error.is_empty()){rows.push(plain_group(vec![info_row("通知记录异常","已保留现有记录",error,view,cx)]));}
 rows
}

/// One history entry, as the handoff draws it: a 34px symbol, the message, then source and time.
fn notice_row(event:&Value,cx:&mut Context<SettingsApp>)->AnyElement{
 let id=event["id"].as_str().unwrap_or("").to_owned();
 let kind=event["kind"].as_str().unwrap_or("notice");
 let title=event["title"].as_str().filter(|title|!title.is_empty()).unwrap_or(if kind=="todo"{"待办提醒"}else{"消息"}).to_owned();
 let body=event["body"].as_str().unwrap_or("").to_owned();
 let app=event["appName"].as_str().filter(|name|!name.is_empty()).unwrap_or(kind_label(kind)).to_owned();
 let time=event["clock"].as_str().unwrap_or("").to_owned();
 let amber=kind=="todo";
 let open=id.clone();
 div().id(SharedString::from(format!("notice-{id}"))).flex().items_center().gap(px(12.0)).w_full().min_w_0().px(px(16.0)).py(px(15.0)).cursor_pointer()
  .hover(|style|style.bg(t::row_hover()))
  .child(symbol(kind_icon(kind),if amber{t::yellow()}else{t::surface_hi()},if amber{t::text()}else{t::text_dim()}))
  .child(div().flex().flex_col().gap_1().flex_1().min_w_0()
   .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).min_w_0().overflow_hidden().child(title))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).min_w_0().overflow_hidden().child(body)))
  .child(div().flex().flex_col().gap_1().items_end().flex_shrink_0().max_w(px(75.0))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child(app))
   .child(div().text_size(px(12.)).text_color(t::text_faint()).child(time)))
  .on_click(cx.listener(move|v,_,_,cx|{v.drawer=Some(Drawer::NoticeDetail(open.clone()));cx.notify();}))
  .into_any_element()
}

/// The 通知设置 gear drawer: a flat stack of rows, the way the handoff draws every drawer.
fn notification_settings_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let notifications=&state["notificationState"];
 let mut rows=vec![
  toggle_row("NotificationsEnabled","通知提醒","接收当前用户的新通知，无需额外安装通知组件。",flag(state,"NotificationsEnabled",true),view, cx),
  toggle_row("NotificationClickOpenApp","点击通知打开来源","点击横幅打开来源应用，关闭按钮只收起通知。",flag(state,"NotificationClickOpenApp",true),view, cx),
  toggle_row("NotificationWhitelistEnabled","启用通知白名单","关闭时不限制应用；开启后只提醒选中的应用。",flag(state,"NotificationWhitelistEnabled",false),view, cx),
 ];
 if flag(state,"NotificationWhitelistEnabled",false){
  for(id,title)in[("QQNT","QQNT"),("QQ","QQ（旧版及系统通知）")]{let allowed=notification_allowed(state,id);rows.push(check_row(SharedString::from(format!("notification-app:{id}")),title,"仅能接收该应用发送到 Windows 通知中心的消息。",allowed,view,cx));}
  if let Some(apps)=notifications["apps"].as_array(){for app in apps{let(Some(id),Some(name))=(app["id"].as_str(),app["name"].as_str())else{continue};if["QQNT","QQ"].contains(&id){continue}let allowed=notification_allowed(state,id);rows.push(check_row(SharedString::from(format!("notification-app:{id}")),name,id,allowed,view,cx));}}
  rows.push(div().flex().flex_col().gap_2().w_full().min_w_0().py(px(16.0))
   .child(div().text_size(px(14.)).text_color(t::text()).child("允许的应用"))
   .child(div().w_full().child(view.notification_apps.clone()))
   .into_any_element());
  tally();
 }
 rows.push(toggle_row("NotificationBlacklistEnabled","启用通知黑名单","屏蔽选中应用的新通知；黑名单优先于白名单，不影响点屿的待办、计时器与设备提醒。",flag(state,"NotificationBlacklistEnabled",false),view,cx));
 if flag(state,"NotificationBlacklistEnabled",false){
  let blocked=setting(state,"NotificationBlockedApps").as_array().cloned().unwrap_or_default();let mut apps=Vec::<(String,String)>::new();
  if let Some(known)=notifications["apps"].as_array(){for app in known{if let Some(id)=app["id"].as_str(){apps.push((id.into(),app["name"].as_str().unwrap_or(id).into()));}}}
  for item in &blocked{if let Some(id)=item.as_str(){if !apps.iter().any(|(key,_)|key.eq_ignore_ascii_case(id)){apps.push((id.into(),id.into()));}}}
  for(id,name)in apps{let selected=blocked.iter().any(|v|v.as_str().is_some_and(|s|s.eq_ignore_ascii_case(&id)));rows.push(check_row(SharedString::from(format!("notification-blocked:{id}")),&name,&id,selected,view,cx));}
  rows.push(div().flex().flex_col().gap_2().w_full().min_w_0().py(px(16.)).child(div().text_size(px(14.)).text_color(t::text()).child("添加需要屏蔽的应用"))
   .child(div().text_size(px(11.)).text_color(t::text_dim()).child("从已识别应用勾选，或输入完整AUMID并按Enter。关闭黑名单会保留名单；不删除已有历史记录。"))
   .child(view.notification_blocked.clone()).into_any_element());tally();
 }
 rows.push(toggle_row("PauseNotifications","暂停通知","暂停期间不弹出通知，恢复后只提醒新消息。",flag(state,"PauseNotifications",false),view, cx));
 rows.push(toggle_row("DeviceRemindersEnabled","设备接入提醒","U 盘、手机、耳机等按识别能力提醒。默认开启。",flag(state,"DeviceRemindersEnabled",true),view, cx));
 if flag(state,"DeviceRemindersEnabled",true){
  rows.push(toggle_row("HeadphoneConnectionRemindersEnabled","耳机连接提醒","有线与蓝牙耳机接入时提醒；切换默认输出不会重复提醒。",flag(state,"HeadphoneConnectionRemindersEnabled",true),view, cx));
  rows.push(toggle_row("SpeakerConnectionRemindersEnabled","音响连接提醒","音响或扬声器接入时提醒。",flag(state,"SpeakerConnectionRemindersEnabled",true),view, cx));
  rows.push(toggle_row("GamepadConnectionRemindersEnabled","手柄连接提醒","XInput 与标准 HID 手柄接入时提醒。",flag(state,"GamepadConnectionRemindersEnabled",true),view, cx));
  rows.push(toggle_row("MouseConnectionRemindersEnabled","鼠标连接提醒","默认关闭：无线鼠标休眠唤醒、KVM 切换会反复重连，每次都弹一次反而成了噪音。只识别接入与移除，不记录任何操作内容。",flag(state,"MouseConnectionRemindersEnabled",false),view, cx));
  rows.push(toggle_row("KeyboardConnectionRemindersEnabled","键盘连接提醒","默认关闭：键盘同样会因休眠或切换设备反复重连。只识别接入与移除，不影响正常输入。",flag(state,"KeyboardConnectionRemindersEnabled",false),view, cx));
  rows.push(toggle_row("DeviceDisconnectRemindersEnabled","断开提醒","设备断开时也提醒；关闭时只提醒接入。",flag(state,"DeviceDisconnectRemindersEnabled",true),view, cx));
 }
 rows.push(device_test_row(cx));
 rows.push(toggle_row("NotificationHistoryEnabled","保留通知记录","仅记录点屿实际收到的通知。",flag(state,"NotificationHistoryEnabled",true),view, cx));
 let days=setting(state,"NotificationHistoryDays").as_u64().map(|days|days.to_string()).unwrap_or_else(||"7".into());
 rows.push(segmented_row("notice-history-days","NotificationHistoryDays","记录保留时间","超出的记录自动清理；只影响点屿自己的记录。",&[("1","1 天"),("7","7 天"),("30","30 天")],&days,cx));
 rows.push(button_row("notification-preview","预览通知横幅","显示本地示例，预览通知外观。","预览",cx,|v,cx|{send_todo(&v.shared,"notificationPreview",json!({}),cx);cx.notify();}));
 rows.push(button_row("notice-audio-link","通知音效","音量与音色集中在设置 · 音效里调整。","前往音效",cx,|v,cx|v.go_sound(cx)));
 rows.push(info_row("通知接收状态",notifications["access"].as_str().unwrap_or("未开启"),notifications["message"].as_str().unwrap_or("请开启通知提醒。"),view,cx));
 rows
}

fn notice_detail_rows(id:&str,state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let Some(event)=state["notificationHistory"].as_array().and_then(|list|list.iter().find(|event|event["id"].as_str()==Some(id))).cloned() else{
  return vec![info_row("记录不存在","可能已被清理","返回通知列表重新选择一条记录。",view,cx)];
 };
 let kind=event["kind"].as_str().unwrap_or("notice").to_owned();
 let time=format!("{} {}",event["date"].as_str().unwrap_or(""),event["clock"].as_str().unwrap_or(""));
 let title=event["title"].as_str().unwrap_or("").to_owned();
 let body=event["body"].as_str().unwrap_or("").to_owned();
 let mut rows=vec![info_row(&format!("{} · {}",kind_label(&kind),time),&title,if body.is_empty(){"这条记录没有正文。"}else{body.as_str()},view,cx)];
 let app_id=event["appId"].as_str().unwrap_or("").to_owned();
 if kind=="notice"&&!app_id.is_empty(){
  let app_name=event["appName"].as_str().unwrap_or("").to_owned();
  rows.push(info_row("来源应用",if app_name.is_empty(){app_id.as_str()}else{app_name.as_str()},app_id.as_str(),view,cx));
  let open_id=app_id.clone();
  rows.push(button_row("notice-open-source","打开来源应用","跳转到发送这条消息的应用。","打开",cx,move|v,cx|{send_todo(&v.shared,"historyOpen",json!(open_id.clone()),cx);cx.notify();}));
 }
 if let Some(todo_id)=event["todoId"].as_str().map(str::to_owned).filter(|id|!id.is_empty()){
  if let Some(todo)=state["todos"].as_array().and_then(|list|list.iter().find(|todo|todo["id"].as_str()==Some(todo_id.as_str())&&todo["completed"]!=true)){
   let name=todo["title"].as_str().unwrap_or("待办").to_owned();
   let(done,later,hour)=(todo_id.clone(),todo_id.clone(),todo_id.clone());
   let mut actions=div().flex().flex_wrap().gap_2();
   actions=actions.child(pill_button(SharedString::from("notice-todo-done"),"完成",false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoComplete",json!({"id":done.clone(),"completed":true}),cx);cx.notify();})));
   actions=actions.child(pill_button(SharedString::from("notice-todo-later"),"10 分钟后",false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoSnooze",json!({"id":later.clone(),"minutes":10}),cx);cx.notify();})));
   actions=actions.child(pill_button(SharedString::from("notice-todo-hour"),"1 小时后",false,cx.listener(move|v,_,_,cx|{send_todo(&v.shared,"todoSnooze",json!({"id":hour.clone(),"minutes":60}),cx);cx.notify();})));
   rows.push(inset_card(false,div().flex().flex_col().gap_1().min_w_0()
    .child(div().text_size(px(14.)).text_color(t::text()).child("关联待办"))
    .child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(name))
    .into_any_element(),Some(actions.into_any_element())));
  }
 }
 rows
}
