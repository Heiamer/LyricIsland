// F05/F06: application rules share one list because the two triggers only differ in when the
// same normalized full-path match fires. The editor is a drawer, like every other form here.
use gpui::{prelude::*,*};
use serde_json::{json,Value};
use crate::protocol as pipe;
use crate::theme as t;
use crate::{empty_row,field_line,flag,info_row,inset_card,list,plain_group,pill,pill_button,send_todo,small_button,switch,symbol,taskcheck,toggle_row,Drawer,SettingsApp};

fn rule_action(id:&str,action:&str,revision:u64,cx:&mut Context<SettingsApp>){
 let value=json!({"id":id,"action":action,"revision":revision});
 let job=cx.background_executor().spawn(async move{pipe::command("appRuleAction",value)});
 let weak=cx.entity().downgrade();
 cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{
  match outcome{Ok((reply,state))=>{let success=reply["success"]==true;if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("规则操作失败").into()};}},Err(e)=>{if let Ok(mut shared)=view.shared.lock(){shared.message=e;}}}
  cx.notify();
 });}).detach();
}

pub(crate) fn rules_page_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let rules=state["appRules"].as_array().cloned().unwrap_or_default();
 let entry=flag(state,"AppEntryRemindersEnabled",false);
 let usage=flag(state,"AppUsageRemindersEnabled",false);
 let mut rows=vec![
  toggle_row("AppEntryRemindersEnabled","进入软件时提醒","切到指定程序时提醒一次；同一程序在冷却时间内不重复。",entry,view, cx),
  toggle_row("AppUsageRemindersEnabled","前台累计时长提醒","只在真正使用该程序时累计；锁屏与睡眠不计入。",usage,view, cx),
 ];
 let active=rules.iter().filter(|rule|rule["enabled"]==true).count();
 rows.push(div().flex().items_center().justify_between().gap_3().mt(px(6.)).mb(px(10.))
  .child(div().text_size(px(12.)).text_color(t::text_dim()).child(format!("{} 条规则 · {} 条已开启",rules.len(),active)))
  .child(div().id("rules-clear-usage").text_size(px(12.)).text_color(t::accent()).cursor_pointer().hover(|style|style.opacity(0.72)).child("清空今日统计").on_click(cx.listener(|v,_,_,cx|send_todo(&v.shared,"appRuleClearUsage",json!({}),cx))))
  .into_any_element());
 if !entry&&!usage{
  rows.push(inset_card(false,div().flex().flex_col().gap_1().min_w_0()
   .child(div().text_size(px(14.)).text_color(t::text()).child("两个开关都关闭"))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child("规则会保留，但不会触发任何提醒。"))
   .into_any_element(),None));
 }
 if rules.is_empty(){rows.push(list(vec![empty_row("还没有应用提醒规则")]));}
 for rule in &rules{rows.push(rule_card(rule,cx));}
 if let Some(error)=state["appRuleError"].as_str().filter(|error|!error.is_empty()){rows.push(plain_group(vec![info_row("应用提醒异常","已保留现有规则",error,view,cx)]));}
 rows
}

fn rule_card(rule:&Value,cx:&mut Context<SettingsApp>)->AnyElement{
 let id=rule["id"].as_str().unwrap_or("").to_owned();
 let name=rule["name"].as_str().unwrap_or("应用提醒").to_owned();
 let path=rule["executablePath"].as_str().unwrap_or("").to_owned();
 let enabled=rule["enabled"]==true;
 let trigger=rule["trigger"].as_str().unwrap_or("enter");
 let revision=rule["revision"].as_u64().unwrap_or(0);
 let cooldown=rule["cooldownMinutes"].as_u64().unwrap_or(30);
 let threshold=rule["thresholdMinutes"].as_u64().unwrap_or(60);
 let today_minutes=(rule["todayMs"].as_u64().unwrap_or(0)+59999)/60000;
 let meta=match trigger{
  "enter"=>format!("进入时 · 冷却 {cooldown} 分钟{}",if rule["disabledToday"]==true{" · 今天已停"}else{""}),
  _=>format!("前台累计 {today_minutes} / {threshold} 分钟{}{}",if rule["firedToday"]==true{" · 今天已提醒"}else{""},if rule["snoozeTargetMs"].as_u64().unwrap_or(0)>0{" · 已稍后"}else{""}),
 };
 let line=taskcheck(symbol("bell-ring",if enabled{t::green_soft()}else{t::surface_hi()},if enabled{t::accent()}else{t::text_dim()}),&name,&format!("{meta}
{path}"),Some(pill(if enabled{"已开启"}else{"已关闭"})));
 let mut actions=div().flex().flex_wrap().gap_2();
 let toggle=id.clone();
 actions=actions.child(pill_button(SharedString::from(format!("rule-toggle-{id}")),if enabled{"关闭"}else{"开启"},false,cx.listener(move|_,_,_,cx|rule_action(&toggle.clone(),if enabled{"disable"}else{"enable"},revision,cx))));
 let edit=id.clone();
 actions=actions.child(pill_button(SharedString::from(format!("rule-edit-{id}")),"编辑",false,cx.listener(move|v,_,_,cx|v.open_rule_form(Some(edit.clone()),cx))));
 let extra=id.clone();
 if trigger=="enter"{
  actions=actions.child(pill_button(SharedString::from(format!("rule-today-{id}")),"今天不再提醒",false,cx.listener(move|v,_,_,cx|{let _=v;rule_action(&extra.clone(),"disableToday",revision,cx)})));
 }else{
  actions=actions.child(pill_button(SharedString::from(format!("rule-snooze-{id}")),"10 分钟后再提醒",false,cx.listener(move|v,_,_,cx|{let _=v;rule_action(&extra.clone(),"snoozeForeground10",revision,cx)})));
 }
 let remove=id.clone();
 actions=actions.child(pill_button(SharedString::from(format!("rule-delete-{id}")),"移除",false,cx.listener(move|v,_,_,cx|{let _=v;rule_action(&remove.clone(),"delete",revision,cx)})));
 inset_card(false,line,Some(actions.into_any_element()))
}

impl SettingsApp{
 pub(crate) fn open_rule_form(&mut self,id:Option<String>,cx:&mut Context<Self>){
  let state=self.state();
  let rule=id.as_ref().and_then(|id|state["appRules"].as_array()?.iter().find(|rule|rule["id"].as_str()==Some(id.as_str())).cloned()).unwrap_or_else(||json!({}));
  let foreground=state["foreground"]["path"].as_str().unwrap_or("");
  self.rule_trigger=rule["trigger"].as_str().unwrap_or("enter").into();
  self.rule_name.update(cx,|field,cx|field.set_text(rule["name"].as_str().unwrap_or(""),cx));
  self.rule_path.update(cx,|field,cx|field.set_text(rule["executablePath"].as_str().filter(|path|!path.is_empty()).unwrap_or(foreground),cx));
  self.rule_message.update(cx,|field,cx|field.set_text(rule["message"].as_str().unwrap_or(""),cx));
  self.rule_cooldown=rule["cooldownMinutes"].as_u64().unwrap_or(30)as u32;
  self.rule_threshold=rule["thresholdMinutes"].as_u64().unwrap_or(60)as u32;
  self.rule_enabled=rule["enabled"].as_bool().unwrap_or(true);
  self.rule_editing=id.clone();
  self.drawer=Some(Drawer::RuleEditor(id));
  cx.notify();
 }
}

fn save_rule(view:&mut SettingsApp,cx:&mut Context<SettingsApp>){
 let payload=json!({"id":view.rule_editing.clone().unwrap_or_default(),"name":view.rule_name.read(cx).text(),"executablePath":view.rule_path.read(cx).text(),"message":view.rule_message.read(cx).text(),"trigger":view.rule_trigger,"cooldownMinutes":view.rule_cooldown,"thresholdMinutes":view.rule_threshold,"enabled":view.rule_enabled});
 let job=cx.background_executor().spawn(async move{pipe::command("appRuleSave",payload)});
 let weak=cx.entity().downgrade();
 cx.spawn(async move|_,cx|{let outcome=job.await;let _=weak.update(cx,|view,cx|{match outcome{
  Ok((reply,state))=>{let success=reply["success"]==true;if let Ok(mut shared)=view.shared.lock(){shared.state=state;shared.message=if success{String::new()}else{reply["message"].as_str().unwrap_or("保存规则失败").into()};if success{view.drawer=None;view.rule_editing=None;}}},
  Err(e)=>{if let Ok(mut shared)=view.shared.lock(){shared.message=e;}}
 }cx.notify();});}).detach();
}

/// A drawer-width section: title and hint stack above the controls. The shared `card` is a
/// row built for full-width pages and would crush its label beside four pills here.
fn drawer_section(id:&'static str,title:&str,detail:&str,controls:Div)->AnyElement{
 div().id(id).flex().flex_col().gap_2().py(px(16.)).min_w_0()
  .child(div().flex().flex_col().gap_1().min_w_0()
   .child(div().text_size(px(14.)).text_color(t::text()).child(title.to_owned()))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child(detail.to_owned())))
  .child(controls)
  .into_any_element()
}

pub(crate) fn rule_editor_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let foreground=state["foreground"]["path"].as_str().unwrap_or("").to_owned();
 let foreground_name=state["foreground"]["name"].as_str().unwrap_or("").to_owned();
 let(picked_path,picked_name)=(foreground.clone(),foreground_name.clone());
 let mut rows=vec![div().flex().flex_col().gap(px(17.)).w(px(331.))
  .child(field_line("规则名称",331.,view.rule_name.clone()))
  .child(field_line("程序完整路径（.exe）",331.,view.rule_path.clone()))
  .child(div().flex().items_center().gap_2()
   .child(small_button("rule-use-foreground".into(),"使用当前前台程序",cx.listener(move|v,_,_,cx|{v.rule_path.update(cx,|field,cx|field.set_text(&picked_path,cx));if v.rule_name.read(cx).text().trim().is_empty(){v.rule_name.update(cx,|field,cx|field.set_text(&picked_name,cx));}cx.notify();})))
   .child(div().text_size(px(11.)).text_color(t::text_faint()).min_w_0().child(if foreground.is_empty(){"当前没有可识别的前台程序".to_owned()}else{foreground.clone()})))
  .child(field_line("提醒内容",331.,view.rule_message.clone()))
  .into_any_element()];
 let mut triggers=div().flex().flex_wrap().gap(px(6.));
 for (value,label) in [("enter","进入软件时"),("foregroundDaily","前台累计时长")]{
  let selected=view.rule_trigger==value;
  triggers=triggers.child(div().id(SharedString::from(format!("rule-trigger-{value}"))).px(px(12.)).py(px(7.)).rounded(px(10.)).cursor_pointer().bg(if selected{t::accent_dim()}else{t::surface()}).border_1().border_color(if selected{t::accent()}else{t::hairline()}).text_size(px(13.)).text_color(if selected{t::accent()}else{t::text_dim()}).child(label).on_click(cx.listener(move|v,_,_,cx|{v.rule_trigger=value.into();cx.notify();})));
 }
 rows.push(drawer_section("rule-trigger","触发方式","进入软件：每次切到前台时判断冷却；前台累计：每天达到阈值时提醒一次。",triggers));
 let mut cooldowns=div().flex().flex_wrap().gap(px(6.));
 for minutes in [5u32,15,30,60]{
  let selected=view.rule_cooldown==minutes;
  cooldowns=cooldowns.child(div().id(("rule-cooldown",minutes as usize)).px(px(12.)).py(px(7.)).rounded(px(10.)).cursor_pointer().bg(if selected{t::accent_dim()}else{t::surface()}).border_1().border_color(if selected{t::accent()}else{t::hairline()}).text_size(px(13.)).text_color(if selected{t::accent()}else{t::text_dim()}).child(format!("{minutes} 分钟")).on_click(cx.listener(move|v,_,_,cx|{v.rule_cooldown=minutes;cx.notify();})));
 }
 rows.push(drawer_section("rule-cooldown","进入提醒冷却","同一程序在冷却时间内不会重复提醒。",cooldowns));
 let mut thresholds=div().flex().flex_wrap().gap(px(6.));
 for minutes in [10u32,30,60,120,240]{
  let selected=view.rule_threshold==minutes;
  thresholds=thresholds.child(div().id(("rule-threshold",minutes as usize)).px(px(12.)).py(px(7.)).rounded(px(10.)).cursor_pointer().bg(if selected{t::accent_dim()}else{t::surface()}).border_1().border_color(if selected{t::accent()}else{t::hairline()}).text_size(px(13.)).text_color(if selected{t::accent()}else{t::text_dim()}).child(format!("{minutes} 分钟")).on_click(cx.listener(move|v,_,_,cx|{v.rule_threshold=minutes;cx.notify();})));
 }
 rows.push(drawer_section("rule-threshold","前台累计阈值","每天零点重置；锁屏、睡眠和未在前台的时间都不计入。",thresholds));
 let enabled=view.rule_enabled;
 rows.push(div().id("rule-enabled").flex().items_center().justify_between().gap_3().py(px(16.)).border_b_1().border_color(t::hairline()).cursor_pointer()
  .child(div().flex().flex_col().gap_1().child(div().text_size(px(14.)).text_color(t::text()).child("启用这条规则")).child(div().text_size(px(12.)).text_color(t::text_dim()).child("关闭后规则保留，但不会触发。")))
  .child(switch("rule-enabled", enabled))
  .on_click(cx.listener(|v,_,_,cx|{v.rule_enabled=!v.rule_enabled;cx.notify();}))
  .into_any_element());
 let editing=view.rule_editing.is_some();
 rows.push(div().flex().justify_end().gap_2().pt(px(12.))
  .child(pill_button("rule-cancel".into(),"取消",false,cx.listener(|v,_,_,cx|{v.drawer=None;v.rule_editing=None;cx.notify();})))
  .child(pill_button("rule-save".into(),if editing{"保存修改"}else{"保存规则"},true,cx.listener(|v,_,_,cx|save_rule(v,cx))))
  .into_any_element());
 rows
}
