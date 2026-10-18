// F01 one-line reminders and the F02 unlock prompt. Both are drawers so the overview and the
// todo page open the exact same surface.
use gpui::{prelude::*,*};
use serde_json::{json,Value};
use crate::protocol as pipe;
use crate::theme as t;
use crate::{card,info_row,inset_card,pill,pill_button,send_todo,symbol,taskcheck,Drawer,SettingsApp};

fn quick_preview(view:&mut SettingsApp,cx:&mut Context<SettingsApp>){
 let text=view.quick_text.read(cx).text().to_owned();
 quick_preview_text(view,text,cx)
}

/// Enter hands over the committed text directly: the callback fires while the field entity is
/// still mid-update, so reading it back here would panic.
fn quick_preview_text(_view:&mut SettingsApp,text:String,cx:&mut Context<SettingsApp>){
 let job=cx.background_executor().spawn(async move{pipe::command("quickReminderPreview",json!({"text":text}))});
 let weak=cx.entity().downgrade();
 cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{match outcome{
  Ok((reply,state))=>{
   if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if reply["success"]==true{String::new()}else{reply["message"].as_str().unwrap_or("无法解析这句话").into()};}
   if reply["success"]==true{let nanos=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();view.quick_request=format!("qr-{nanos}");}
   view.quick_preview=reply;
  }
  Err(e)=>view.quick_preview=json!({"success":false,"message":e}),
 }cx.notify();});}).detach();
}

fn quick_commit(view:&mut SettingsApp,cx:&mut Context<SettingsApp>){
 let token=view.quick_preview["previewToken"].as_str().unwrap_or("").to_owned();
 let request=view.quick_request.clone();
 let job=cx.background_executor().spawn(async move{pipe::command("quickReminderCommit",json!({"previewToken":token,"clientRequestId":request}))});
 let weak=cx.entity().downgrade();
 cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{match outcome{
  Ok((reply,state))=>{
   let success=reply["success"]==true;
   if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("创建提醒失败").into()};shared.notice=reply["message"].as_str().unwrap_or(if success{"已创建提醒"}else{"创建提醒失败"}).into();}
   if success{view.drawer=None;view.quick_preview=Value::Null;view.quick_text.update(cx,|field,cx|field.set_text("",cx));}
  }
  Err(e)=>{if let Ok(mut shared)=view.shared.lock(){shared.message=e;}view.quick_preview=json!({"success":false,"message":"连接失败"});}
 }cx.notify();});}).detach();
}

/// The 一句话提醒 drawer: native parses first, the user confirms a preview, and only the commit
/// writes a todo. The preview token expires after 120 seconds on the native side.
pub(crate) fn quick_reminder_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let mut rows=vec![
  info_row("一句话快速提醒","例如：20分钟后取快递","支持 N分钟后 · N小时后 · 今天H:mm · 明天H:mm · YYYY-MM-DD H:mm，最长 7 天。",view,cx),
  div().flex().flex_col().gap(px(6.)).py(px(14.))
   .child(div().text_size(px(14.)).text_color(t::text()).child("提醒内容"))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child("按 Enter 解析；先确认预览，再创建提醒。"))
   .child(div().mt(px(4.)).w_full().child(view.quick_text.clone()))
   .into_any_element(),
  div().flex().items_center().gap_2().child(pill_button("quick-parse".into(),"解析并预览",true,cx.listener(|v,_,_,cx|quick_preview(v,cx)))).into_any_element(),
 ];
 let preview=&view.quick_preview;
 if preview["success"]==true{
  let title=preview["title"].as_str().unwrap_or("提醒").to_owned();
  let when=format!("{} {}",preview["date"].as_str().unwrap_or(""),preview["time"].as_str().unwrap_or(""));
  let line=taskcheck(symbol("alarm-clock",t::yellow(),t::text()),&title,&when,Some(pill("待创建")));
  let actions=div().flex().flex_wrap().gap_2()
   .child(pill_button("quick-create".into(),"创建提醒",true,cx.listener(|v,_,_,cx|quick_commit(v,cx))))
   .child(pill_button("quick-reset-preview".into(),"重新输入",false,cx.listener(|v,_,_,cx|{v.quick_preview=Value::Null;cx.notify();})));
  rows.push(inset_card(true,line,Some(actions.into_any_element())));
  rows.push(info_row("预览有效期","120 秒","过期后重新解析即可；创建成功后重复提交不会产生第二条待办。",view,cx));
 }else if preview["success"]==false{
  rows.push(info_row("无法创建提醒",preview["message"].as_str().unwrap_or("换一种写法试试"),"检查日期是否有效、时间是否已经过去。",view,cx));
 }
 let hotkey=state["quickHotkey"].as_str().unwrap_or("");
 rows.push(info_row("全局快捷键",if hotkey.is_empty(){"未绑定"}else{hotkey},"在 设置 · 常规 中开启；默认关闭。",view,cx));
 rows
}

/// Settings · 常规: the optional global hotkey. A rejected binding keeps the previous one.
pub(crate) fn quick_hotkey_rows(state:&Value,_view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let current=state["quickHotkey"].as_str().unwrap_or("").to_owned();
 let mut segments=div().flex().flex_wrap().gap(px(6.));
 for (value,label) in [("","关闭"),("Ctrl+Alt+R","Ctrl+Alt+R"),("Ctrl+Shift+R","Ctrl+Shift+R"),("Ctrl+Alt+N","Ctrl+Alt+N")]{
  let selected=current==value;
  segments=segments.child(div().id(SharedString::from(format!("hotkey-{value}"))).px(px(12.)).py(px(7.)).rounded(px(10.)).cursor_pointer().bg(if selected{t::surface_hi()}else{t::surface()}).border_1().border_color(if selected{t::accent()}else{t::hairline()}).text_size(px(13.)).text_color(if selected{t::text()}else{t::text_dim()}).font_weight(if selected{FontWeight::MEDIUM}else{FontWeight::NORMAL}).child(label).on_click(cx.listener(move|v,_,_,cx|send_todo(&v.shared,"update",json!({"key":"QuickReminderHotkey","value":value}),cx))));
 }
 vec![
  card("row-QuickReminderHotkey","快速提醒快捷键","在任何程序里按下即可打开一句话提醒。被其他程序占用时自动保留原绑定。").child(segments).into_any_element(),
  card("row-quick-open","一句话提醒","从概览或待办页也能打开同一个面板。").child(pill_button("quick-open".into(),"打开一句话提醒",false,cx.listener(|v,_,_,cx|{v.drawer=Some(Drawer::QuickReminder);cx.notify();}))).into_any_element(),
 ]
}

/// The unlock prompt. Nothing resumes automatically: 全部继续 is an explicit click, and a
/// reminder that has already been answered disappears from the next state snapshot.
pub(crate) fn focus_resume_rows(state:&Value,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let Some(resume)=state["focusResume"].as_object()else{return vec![]};
 let generation=resume.get("generation").and_then(Value::as_u64).unwrap_or(0);
 let ids=resume.get("ids").and_then(Value::as_array).map(|list|list.iter().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default();
 if ids.is_empty(){return vec![]}
 let timers=state["timers"].as_array().cloned().unwrap_or_default();
 let title=format!("继续刚才的 {} 项专注？",ids.len());
 let all=ids.clone();
 let actions=div().flex().flex_wrap().gap_2()
  .child(pill_button("resume-all".into(),"全部继续",true,cx.listener(move|v,_,_,cx|send_todo(&v.shared,"focusResume",json!({"generation":generation,"ids":all.clone()}),cx))))
  .child(pill_button("resume-later".into(),"暂不",false,cx.listener(|v,_,_,cx|send_todo(&v.shared,"focusResumeDismiss",json!({}),cx))));
 let mut rows=vec![inset_card(true,taskcheck(symbol("play",t::green_soft(),t::accent()),&title,"锁屏时自动暂停，解锁不会自动继续。",None),Some(actions.into_any_element()))];
 for id in ids{
  let timer=timers.iter().find(|timer|timer["id"].as_str()==Some(id.as_str()));
  let name=timer.and_then(|timer|timer["name"].as_str()).unwrap_or("专注").to_owned();
  let clock=timer.and_then(|timer|timer["clock"].as_str()).unwrap_or("").to_owned();
  let resume=id.clone();
  rows.push(card(SharedString::from(format!("resume-{id}")),&name,"单独继续这一项。").child(pill_button(SharedString::from(format!("resume-one-{id}")),&format!("继续 · {clock}"),false,cx.listener(move|v,_,_,cx|send_todo(&v.shared,"focusResume",json!({"generation":generation,"ids":[resume.clone()]}),cx)))).into_any_element());
 }
 rows
}

impl SettingsApp{
 /// Enter in the quick field parses through the same path as the button.
 pub(crate) fn preview_quick_text(&mut self,text:String,cx:&mut Context<Self>){quick_preview_text(self,text,cx)}
}

