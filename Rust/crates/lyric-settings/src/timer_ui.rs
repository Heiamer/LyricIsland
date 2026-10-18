// Digital clocks share a restrained visual hierarchy; elapsed time is native-authoritative.
struct TimerSample{revision:u64,elapsed:u64,phase:String,observed:Instant,displayed:f64}
impl SettingsApp{
 fn timer_elapsed(&self,timer:&Value)->f64{
  let id=timer["id"].as_str().unwrap_or("").to_owned();let elapsed=timer["elapsedMs"].as_u64().unwrap_or(0);let revision=timer["revision"].as_u64().unwrap_or(0);let phase=timer["phase"].as_str().unwrap_or("");
  let mut samples=self.timer_motion.borrow_mut();let sample=samples.entry(id).or_insert_with(||TimerSample{revision,elapsed,phase:phase.into(),observed:Instant::now(),displayed:elapsed as f64});
  if sample.revision!=revision||sample.phase!=phase{*sample=TimerSample{revision,elapsed,phase:phase.into(),observed:Instant::now(),displayed:elapsed as f64};}
  else if sample.elapsed!=elapsed{sample.elapsed=elapsed;sample.observed=Instant::now();}
  let connected=self.shared.lock().map(|s|!s.connection_error).unwrap_or(false);
  if timer["running"]==true&&connected{sample.displayed=sample.displayed.max(elapsed as f64+sample.observed.elapsed().as_secs_f64().min(0.9)*1000.);}else{sample.displayed=elapsed as f64;}
  sample.displayed
 }
}
fn clock_ink()->gpui::Rgba{t::accent()}
fn precise_clock(ms:u64)->String{let seconds=ms/1000;format!("{:02}:{:02}.{:02}",seconds/60,seconds%60,ms/10%100)}
fn display_clock(seconds:f64,count_up:bool)->String{let n=if count_up{seconds.floor()}else{seconds.ceil()}.max(0.)as u64;if n>=3600{format!("{:02}:{:02}:{:02}",n/3600,n/60%60,n%60)}else{format!("{:02}:{:02}",n/60,n%60)}}
// A quiet digital readout, shared by all clocks; hierarchy comes from type and spacing.
fn digital_clock(stopwatch:bool,elapsed:f64,remaining:f64,progress:f64)->AnyElement{
 let mut face=div().w_full().h(px(200.)).flex().flex_col().justify_center().gap(px(20.));
 face=face.child(div().flex().items_center().gap(px(7.)).child(div().w(px(5.)).h(px(5.)).rounded_full().bg(clock_ink())).child(div().text_size(px(12.)).text_color(t::text_dim()).child(if stopwatch{"累计时间"}else{"剩余时间"})));
 if stopwatch{let value=precise_clock(elapsed as u64);let decimal=value.find('.').unwrap_or(value.len());let len=value.len();let font=if value.len()>8{36.}else{45.};face=face.child(div().text_size(px(font)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(StyledText::new(value).with_highlights([(decimal..len,HighlightStyle{color:Some(clock_ink().into()),..Default::default()})]))).child(div().text_size(px(11.)).text_color(t::text_faint()).child("分 : 秒 . 百分秒"));}
 else{let clock=display_clock(remaining,false);face=face.child(div().text_size(px(if clock.len()>5{40.}else{55.})).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(clock)).child(div().flex().flex_col().gap(px(9.)).child(div().w_full().h(px(4.)).rounded_full().bg(t::surface_hi()).child(div().w(px((286.*progress.clamp(0.,1.))as f32)).h(px(4.)).rounded_full().bg(clock_ink()))).child(div().flex().justify_between().text_size(px(11.)).text_color(t::text_faint()).child(format!("已过 {}",display_clock(elapsed/1000.,true))).child("到时提醒")));}
 face.into_any_element()
}
fn focus_direction_switch(state:&Value,current:Option<&Value>,cx:&mut Context<SettingsApp>)->AnyElement{
 let up=current.and_then(|t|t["countUp"].as_bool()).unwrap_or(setting(state,"FocusCountUp").as_bool().unwrap_or(false));let id=current.and_then(|t|t["id"].as_str()).map(str::to_owned);let revision=current.and_then(|t|t["revision"].as_u64()).unwrap_or(0);
 let thumb=div().absolute().top(px(3.)).w(px(68.)).h(px(30.)).rounded(px(10.)).bg(t::surface()).with_spring("focus-direction-thumb",motion::phase(motion::snappy(),up),|e,p|e.left(px(3.+68.*motion::alpha(p))));
 let mut control=div().relative().flex().w(px(142.)).h(px(36.)).p(px(3.)).rounded(px(12.)).bg(t::surface_hi()).child(thumb);
 for(index,label)in["倒计时","正计时"].iter().enumerate(){let selected=up==(index==1);let id=id.clone();control=control.child(div().id(("focus-direction",index)).relative().w(px(68.)).h(px(30.)).flex().items_center().justify_center().text_size(px(12.)).text_color(if selected{t::text()}else{t::text_dim()}).cursor_pointer().active(|s|s.opacity(0.7)).child(*label).on_click(cx.listener(move|v,_,_,cx|{if selected{return}let next=index==1;let _=send(&v.shared,"FocusCountUp",if next{"true"}else{"false"},cx);if let Some(id)=&id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"direction","countUp":next,"revision":revision}),cx);}cx.notify();})));}
 control.into_any_element()
}
fn focus_style_switch(view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{
 let flow=view.timer_mode=="workflow";
 let thumb=div().absolute().top(px(3.)).w(px(122.)).h(px(30.)).rounded(px(10.)).bg(t::surface()).shadow_sm().with_spring("focus-style-thumb",motion::phase(motion::snappy(),flow),|e,p|e.left(px(3.+122.*motion::alpha(p))));
 let mut control=div().relative().flex().w(px(250.)).h(px(36.)).p(px(3.)).rounded(px(12.)).bg(t::surface_hi()).child(thumb);
 for(index,label)in["单次番茄钟","流程番茄钟"].iter().enumerate(){let selected=flow==(index==1);control=control.child(div().id(("focus-style",index)).relative().w(px(122.)).h(px(30.)).flex().items_center().justify_center().text_size(px(12.5)).font_weight(if selected{FontWeight::MEDIUM}else{FontWeight::NORMAL}).text_color(if selected{t::text()}else{t::text_dim()}).cursor_pointer().active(|s|s.opacity(0.7)).child(*label).on_click(cx.listener(move|v,_,_,cx|{v.timer_mode=if index==1{"workflow"}else{"pomodoro"}.into();v.timer_new=false;v.timer_adjust_open=false;cx.notify();})));}
 control.into_any_element()
}
fn timer_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let clocks=state["timers"].as_array().cloned().unwrap_or_default();let active=text(state,"activeComponent","").strip_prefix("timer:").unwrap_or("");
 let current=if view.timer_new{None}else{clocks.iter().find(|t|t["id"].as_str()==Some(active)&&t["mode"].as_str()==Some(view.timer_mode.as_str())).or_else(||clocks.iter().find(|t|t["mode"].as_str()==Some(view.timer_mode.as_str())))};
 let mode=if matches!(view.timer_mode.as_str(),"workflow"|"pomodoro"){0}else if view.timer_mode=="countdown"{1}else{2};
 let mut rows=vec![tab_row("timer-mode",&["番茄钟","计时器","秒表"],mode,cx,|v,index|{v.timer_mode=match index{0=>"workflow",1=>"countdown",_=>"stopwatch"}.into();v.timer_new=false;v.timer_adjust_open=false;})];
 if mode==0{rows.push(div().flex().items_center().justify_between().gap(px(12.)).child(focus_style_switch(view,cx)).child(focus_direction_switch(state,current,cx)).into_any_element());}
 let workflow=view.timer_mode=="workflow";let stopwatch=view.timer_mode=="stopwatch";
 let running=current.is_some_and(|t|t["running"]==true);let finished=current.is_some_and(|t|t["finished"]==true);let waiting=current.is_some_and(|t|t["workflow"]["waiting"]==true);
 let id=current.and_then(|t|t["id"].as_str()).map(str::to_owned);let revision=current.and_then(|t|t["revision"].as_u64()).unwrap_or(0);
 let elapsed=current.map(|t|view.timer_elapsed(t)).unwrap_or(0.);let field=if view.timer_mode=="pomodoro"{&view.timer_focus}else{&view.timer_duration};
 let duration=current.map(|t|if workflow{t["workflow"]["stages"].as_array().and_then(|stages|stages.get(t["workflow"]["stageIndex"].as_u64().unwrap_or(0)as usize)).and_then(|s|s["seconds"].as_u64()).unwrap_or(1)}else if view.timer_mode=="pomodoro"{t[if t["phase"]=="rest"{"rest"}else{"focus"}].as_u64().unwrap_or(1500)}else{t["duration"].as_u64().unwrap_or(300)}).unwrap_or_else(||field.read(cx).text().parse::<u64>().unwrap_or(300)) as f64;
 let remaining=(duration-elapsed/1000.).max(0.);let progress=if duration>0.{(elapsed/1000./duration).clamp(0.,1.)}else{0.};
 let status=if waiting{"等待下一阶段"}else if running{"正在计时"}else if finished{"已完成"}else if id.is_some(){"已暂停"}else{"准备开始"};
 let title=current.and_then(|t|t["name"].as_str()).unwrap_or(if workflow{"给自己一段专注时间"}else if stopwatch{"记录每一段投入"}else if mode==0{"一次，只做一件事"}else{"为下一件事留好时间"}).to_owned();
 let primary=if workflow&&waiting{"下一阶段"}else if running{"暂停"}else if finished{"重新开始"}else if id.is_some(){"继续"}else{"开始"};
 let action=if waiting{"next"}else if running{"pause"}else if finished{if workflow{"restart"}else{"reset"}}else{"resume"};
 if workflow&&id.is_none(){
  let mut blocks=div().flex().items_center().justify_center().gap(px(10.));
  for(kind,label,minutes)in[(0,"整理思路","5 分"),(1,"专注","25 分"),(2,"休息","5 分")]{blocks=blocks.child(div().w(px(116.)).p(px(13.)).rounded(px(14.)).bg(flow_wash(kind)).flex().flex_col().gap(px(10.)).child(icons::tinted(FLOW_MODULES[kind].1,rgb(FLOW_MODULES[kind].2))).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(label)).child(div().text_size(px(12.)).text_color(t::text_dim()).child(minutes)));}
  rows.push(div().w_full().p(px(26.)).rounded(px(20.)).bg(t::surface()).flex().flex_col().items_center().gap(px(18.)).child(div().text_size(px(21.)).font_weight(FontWeight::MEDIUM).child(title)).child(div().text_size(px(12.5)).text_color(t::text_dim()).child("拖入模块，自定时长。阶段完成后，由你决定何时继续。")).child(blocks).child(pill_button_icon("clock-start-flow".into(),Some("plus"),"编排流程",true,cx.listener(|v,_,_,cx|v.open_workflow_form(None,cx)))).into_any_element());
 }else{
  let mut left=div().w(px(286.)).flex_shrink_0().flex().flex_col().items_center().gap(px(18.));
  if stopwatch||view.timer_mode=="countdown"{left=left.child(digital_clock(stopwatch,elapsed,remaining,progress));}
  else{left=left.child(div().h(px(208.)).w_full().flex().flex_col().items_center().justify_center().gap(px(12.)).child(div().text_size(px(12.)).text_color(t::text_dim()).child(if workflow{"当前阶段"}else{"专注时间"})).child(div().text_size(px(53.)).font_weight(FontWeight::MEDIUM).text_color(clock_ink()).child(display_clock(if current.and_then(|t|t["countUp"].as_bool()).unwrap_or(setting(state,"FocusCountUp").as_bool().unwrap_or(false)){elapsed/1000.}else{remaining},current.and_then(|t|t["countUp"].as_bool()).unwrap_or(setting(state,"FocusCountUp").as_bool().unwrap_or(false))))).child(div().text_size(px(12.)).text_color(t::text_dim()).child(status)).child(div().w(px(180.)).h(px(3.)).rounded(px(2.)).bg(t::surface_hi()).child(div().w(px((180.*progress)as f32)).h(px(3.)).rounded(px(2.)).bg(clock_ink()))));}
  let (reset_id,toggle_id,lap_id)=(id.clone(),id.clone(),id.clone());
  let mut actions=div().flex().items_center().justify_center().gap(px(22.));
  if stopwatch&&running{actions=actions.child(pill_button_icon("clock-lap".into(),Some("plus"),"计次",false,cx.listener(move|v,_,_,cx|{if let Some(id)=&lap_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"lap","revision":revision}),cx)}})));}
  else{actions=actions.child(pill_button_icon("clock-reset".into(),Some("rotate-ccw"),"重置",false,cx.listener(move|v,_,_,cx|{if let Some(id)=&reset_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"reset","revision":revision}),cx)}})));}
  actions=actions.child(div().id("timer-start").w(px(112.)).h(px(44.)).rounded(px(14.)).gap(px(8.)).bg(clock_ink()).text_color(gpui::white()).flex().items_center().justify_center().cursor_pointer().hover(|s|s.opacity(0.93)).active(|s|s.opacity(0.75)).child(icons::tinted(if running{"pause"}else{"play"},gpui::white()).w(px(18.)).h(px(18.))).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(primary)).on_click(cx.listener(move|v,_,_,cx|{if let Some(id)=&toggle_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":action,"revision":revision}),cx)}else{v.create_timer(cx)}})));
  left=left.child(actions);
  let mut detail=div().flex_1().min_w(px(256.)).max_w(px(350.)).flex().flex_col().gap(px(14.)).pt(px(12.)).child(div().text_size(px(20.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(title)).child(div().text_size(px(12.)).text_color(t::text_dim()).child(if stopwatch{"计次独立记录，暂停不会清空。"}else{status}));
  if stopwatch{
   let laps=current.and_then(|t|t["laps"].as_array()).cloned().unwrap_or_default();
   let head=div().flex().text_size(px(11.)).text_color(t::text_faint()).pb(px(8.)).child(div().w(px(40.)).child("计次")).child(div().flex_1().text_right().child("单次时间")).child(div().w(px(104.)).text_right().child("总时间"));
   let mut list=div().id("stopwatch-laps").max_h(px(172.)).overflow_y_scroll().flex().flex_col().child(head);
   if laps.is_empty(){list=list.child(div().py(px(20.)).text_size(px(12.)).text_color(t::text_dim()).child("开始后点击“计次”，留下每一段记录。"));}
   for index in (0..laps.len()).rev(){let total=laps[index].as_u64().unwrap_or(0);let previous=if index==0{0}else{laps[index-1].as_u64().unwrap_or(0)};list=list.child(div().flex().items_center().py(px(11.)).border_t_1().border_color(t::hairline()).text_size(px(13.)).child(div().w(px(40.)).child(format!("{:02}",index+1))).child(div().flex_1().text_right().child(format!("+{}",precise_clock(total.saturating_sub(previous))))).child(div().w(px(104.)).text_right().child(precise_clock(total))));}
   detail=detail.child(list);
  }else if workflow{
   let stages=current.and_then(|t|t["workflow"]["stages"].as_array()).cloned().unwrap_or_default();let stage_index=current.and_then(|t|t["workflow"]["stageIndex"].as_u64()).unwrap_or(0)as usize;
   let mut list=div().id("running-workflow-stages").max_h(px(178.)).overflow_y_scroll().flex().flex_col().gap(px(6.));
   for(index,stage)in stages.iter().enumerate(){let active=index==stage_index;list=list.child(div().flex().items_center().gap(px(9.)).p(px(10.)).rounded(px(11.)).bg(if active{t::green_soft()}else{t::base()}).child(div().text_size(px(10.)).text_color(t::text_faint()).child(format!("{:02}",index+1))).child(div().flex_1().min_w_0().text_size(px(12.)).child(stage["name"].as_str().unwrap_or("阶段").to_owned())).child(div().text_size(px(11.)).text_color(t::text_dim()).child(format!("{} 分",stage["seconds"].as_u64().unwrap_or(0)/60))));}
   detail=detail.child(list);
   let(edit,skip,extend,end)=(id.clone(),id.clone(),id.clone(),id.clone());
   detail=detail.child(div().flex().flex_wrap().gap(px(6.)).child(small_button("clock-edit-flow".into(),"编辑副本",cx.listener(move|v,_,_,cx|v.open_workflow_form(edit.clone(),cx)))).child(small_button("clock-skip".into(),"跳过",cx.listener(move|v,_,_,cx|{if let Some(id)=&skip{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"skip","revision":revision}),cx)}}))).child(small_button("clock-extend".into(),"延长5分",cx.listener(move|v,_,_,cx|{if let Some(id)=&extend{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"extend5","revision":revision}),cx)}}))).child(small_button("clock-end".into(),"结束",cx.listener(move|v,_,_,cx|{if let Some(id)=&end{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"end","revision":revision}),cx)}}))));
  }else{
   detail=detail.child(div().text_size(px(12.)).text_color(t::text_dim()).child(if mode==0{"单次模式完成后结束；已有多轮番茄钟继续按原配置运行。"}else{"到时通过灵动岛和音效提醒，可同时运行多个计时器。"}));
   let mut presets=div().flex().gap(px(7.));for minutes in [5u64,15,25,45]{let field=field.clone();presets=presets.child(small_button(SharedString::from(format!("clock-preset-{minutes}")),&format!("{minutes}分"),cx.listener(move|_,_,_,cx|{field.update(cx,|f,cx|f.set_text(&(minutes*60).to_string(),cx));cx.notify();})));}detail=detail.child(presets);
  }
  if !workflow{detail=detail.child(div().flex().gap(px(8.)).child(pill_button_icon("clock-adjust".into(),Some("settings-2"),"调整",false,cx.listener(|v,_,_,cx|{v.timer_adjust_open=!v.timer_adjust_open;cx.notify();}))).child(pill_button_icon("clock-new".into(),Some("plus"),"新计时",false,cx.listener(|v,_,_,cx|{v.timer_new=true;cx.notify();}))));}
  rows.push(div().flex().flex_wrap().justify_center().items_start().gap(px(30.)).w_full().p(px(22.)).rounded(px(20.)).bg(t::surface()).child(left).child(detail).into_any_element());
 }
 if view.timer_adjust_open&&!workflow{let mut fields=div().flex().flex_col().gap(px(12.)).p(px(18.)).rounded(px(16.)).bg(t::surface()).child(div().text_size(px(12.)).text_color(t::text_dim()).child("仅用于下一次新建，不修改运行中的计时。")).child(field_line("名称",280.,view.timer_name.clone()));if view.timer_mode=="countdown"{fields=fields.child(field_line("计时时长（秒）",220.,view.timer_duration.clone()));}else if view.timer_mode=="pomodoro"{fields=fields.child(div().flex().gap(px(12.)).child(field_line("专注（秒）",130.,view.timer_focus.clone())).child(field_line("休息（秒）",130.,view.timer_rest.clone())).child(field_line("轮数（单次用1）",130.,view.timer_rounds.clone())));}rows.push(fields.into_any_element());}
 if !clocks.is_empty(){let mut cards=vec![];for timer in &clocks{let id=timer["id"].as_str().unwrap_or("").to_owned();let(show,remove)=(id.clone(),id.clone());let mode=timer["mode"].as_str().unwrap_or("countdown").to_owned();let rev=timer["revision"].as_u64().unwrap_or(0);cards.push(div().flex().items_center().gap(px(12.)).p(px(14.)).rounded(px(13.)).bg(t::surface()).child(div().id(SharedString::from(format!("clock-select-{id}"))).flex_1().flex().items_center().gap(px(12.)).cursor_pointer().child(div().flex_1().min_w_0().flex().flex_col().gap_1().child(div().text_size(px(13.)).child(timer["name"].as_str().unwrap_or("计时器").to_owned())).child(div().text_size(px(11.)).text_color(t::text_dim()).child(timer["subtitle"].as_str().unwrap_or("").to_owned()))).child(minitime(timer["clock"].as_str().unwrap_or("00:00"))).on_click(cx.listener(move|v,_,_,cx|{v.timer_mode=mode.clone();v.timer_new=false;send_todo(&v.shared,"islandSelect",json!(format!("timer:{show}")),cx);cx.notify();}))).child(small_button(SharedString::from(format!("clock-delete-{id}")),"移除",cx.listener(move|v,_,_,cx|send_todo(&v.shared,"timerAction",json!({"id":remove,"action":"delete","revision":rev}),cx)))).into_any_element());}rows.push(section_block("我的计时",None,card_stack(cards),cx));}
 rows
}


/// The 流程与专注 gear drawer: where the reminder sound lives and how timers behave when the app closes.
fn timer_settings_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let running=state["timers"].as_array().map(|list|list.iter().filter(|timer|timer["running"]==true).count()).unwrap_or(0);
 vec![
  info_row("计时器状态",&format!("{running} 个正在运行"),"多个计时可同时运行；关闭点屿后计时暂停，重新打开按保存时间恢复。",view,cx),
  toggle_row("FocusPauseOnLock","锁屏时暂停专注","锁屏后自动暂停正在进行的番茄钟专注；解锁时可选择继续。默认关闭。",setting(state,"FocusPauseOnLock").as_bool()==Some(true),view, cx),
  button_row("timer-sound-link","提醒音量与音色","计时完成与番茄钟阶段切换的音效，集中在设置 · 音效里调整。","前往音效",cx,|v,cx|v.go_sound(cx)),
  info_row("提醒方式","到时播放音效并在灵动岛提醒","提醒历史可在通知页回看，记录保留时长由通知历史设置决定。",view,cx),
 ]
}

impl SettingsApp{
 /// Open the 流程 editor, optionally prefilled from an existing workflow instance.
 pub(crate) fn open_workflow_form(&mut self,id:Option<String>,cx:&mut Context<Self>){
  let timer=id.and_then(|id|self.shared.lock().ok().and_then(|shared|shared.state["timers"].as_array().and_then(|list|list.iter().find(|timer|timer["id"].as_str()==Some(id.as_str())).cloned())));
  let stages=timer.as_ref().and_then(|timer|timer["workflow"]["stages"].as_array()).cloned().unwrap_or_default();
  let name=timer.as_ref().and_then(|timer|timer["name"].as_str()).unwrap_or("").to_owned();
  let template=timer.as_ref().and_then(|timer|timer["workflow"]["templateId"].as_str()).unwrap_or("").to_owned();
  self.workflow_name.update(cx,|field,cx|field.set_text(&name,cx));
  self.workflow_count=if stages.is_empty(){0}else{stages.len().clamp(2,FLOW_STAGE_CAP)};
  self.workflow_template=template;
  for(index,(name_field,minutes_field,kind))in self.workflow_stages.iter_mut().enumerate(){
   let(stage_name,minutes,stage_kind)=if index<self.workflow_count{stages.get(index).map(|stage|{let name=stage["name"].as_str().unwrap_or("").to_owned();let kind=infer_flow_kind(&name);(name,(stage["seconds"].as_u64().unwrap_or(0)/60).max(1),kind)}).unwrap_or((String::new(),25,5))}else{(String::new(),25,5)};
   name_field.update(cx,|field,cx|field.set_text(&stage_name,cx));
   minutes_field.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));
   *kind=stage_kind;
  }
  self.workflow_selected=0;self.workflow_generation+=1;self.workflow_busy=false;self.workflow_pending=None;self.workflow_stage_editing=None;self.workflow_drop=None;
  self.drawer=Some(Drawer::WorkflowEditor);
  cx.notify();
 }
 /// Copy a saved template into the editor's fields.
 fn load_workflow_template(&mut self,template:&Value,cx:&mut Context<Self>){
  let stages=template["stages"].as_array().cloned().unwrap_or_default();
  self.workflow_count=stages.len().clamp(2,FLOW_STAGE_CAP);self.workflow_selected=0;self.workflow_generation+=1;self.workflow_pending=None;self.workflow_stage_editing=None;self.workflow_drop=None;
  self.workflow_template=template["id"].as_str().unwrap_or("").to_owned();
  self.workflow_name.update(cx,|field,cx|field.set_text(template["name"].as_str().unwrap_or(""),cx));
  for(index,(name_field,minutes_field,kind))in self.workflow_stages.iter_mut().enumerate(){
   let(stage_name,minutes,stage_kind)=if index<self.workflow_count{stages.get(index).map(|stage|{let name=stage["name"].as_str().unwrap_or("").to_owned();let kind=infer_flow_kind(&name);(name,(stage["seconds"].as_u64().unwrap_or(0)/60).max(1),kind)}).unwrap_or((String::new(),25,5))}else{(String::new(),25,5)};
   name_field.update(cx,|field,cx|field.set_text(&stage_name,cx));
   minutes_field.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));
   *kind=stage_kind;
  }
  cx.notify();
 }
 fn workflow_stages_json(&self,cx:&Context<Self>)->Result<Value,String>{
  let mut stages=Vec::new();
  for(index,(name_field,minutes_field,_))in self.workflow_stages.iter().enumerate().take(self.workflow_count){
   let minutes=minutes_field.read(cx).text().trim().parse::<u64>().ok().filter(|minutes|(1..=240).contains(minutes)).ok_or_else(||format!("第 {} 个阶段需要 1 到 240 分钟",index+1))?;
   stages.push(json!({"name":name_field.read(cx).text().trim(),"seconds":minutes*60}));
  }
  if stages.len()<2{return Err("至少加入两个阶段".into())}
  if stages.iter().filter_map(|stage|stage["seconds"].as_u64()).sum::<u64>()>86400{return Err("流程总时长不能超过24小时".into())}
  if stages.iter().any(|stage|stage["name"].as_str().unwrap_or("").chars().count()>40){return Err("阶段名称不能超过40字".into())}
  Ok(json!(stages))
 }
 fn submit_workflow(&mut self,cx:&mut Context<Self>){self.commit_flow(false,cx)}
 fn commit_flow(&mut self,template:bool,cx:&mut Context<Self>){
  if self.workflow_busy{return}
  let stages=match self.workflow_stages_json(cx){Ok(stages)=>stages,Err(message)=>{if let Ok(mut shared)=self.shared.lock(){shared.message=message;}cx.notify();return}};
  let name=self.workflow_name.read(cx).text().trim().to_owned();
  if template&&(name.is_empty()||name.chars().count()>60){if let Ok(mut shared)=self.shared.lock(){shared.message="请输入1–60字的流程名称".into();}cx.notify();return}
  let expected_stages=stages.clone();let expected_name=name.clone();
  self.workflow_busy=true;let generation=self.workflow_generation;let shared=Rc::clone(&self.shared);let weak=cx.entity().downgrade();
  let command=if template{"workflowTemplateSave"}else{"timerCreate"};
  let value=if template{json!({"id":self.workflow_template,"name":name,"stages":stages})}else{json!({"mode":"workflow","name":name,"stages":stages,"templateId":self.workflow_template,"countUp":setting(&self.state(),"FocusCountUp").as_bool().unwrap_or(false)})};
  let job=cx.background_executor().spawn(async move{pipe::command(command,value)});
  cx.spawn(async move|_,cx|{let result=job.await;let mut success=false;if let Ok(mut shared)=shared.lock(){match result{Ok((reply,state))=>{success=reply["success"]==true;shared.state=state;shared.message=if success{if template{"模板已保存".into()}else{String::new()}}else{reply["message"].as_str().unwrap_or("流程保存失败，草稿已保留").into()};},Err(e)=>shared.message=e}}
   let _=weak.update(cx,|view,cx|{view.workflow_busy=false;if success&&!template&&view.workflow_generation==generation&&view.workflow_stages_json(cx).ok()==Some(expected_stages)&&view.workflow_name.read(cx).text().trim()==expected_name{view.drawer=None;}cx.notify();});}).detach();cx.notify();
 }
 fn flow_drop(&mut self,payload:&FlowDrag,slot:usize,cx:&mut Context<Self>){
  self.workflow_drop=None;
  if self.workflow_busy||payload.generation!=self.workflow_generation{return}
  if let Some(index)=payload.source{
   if index>=self.workflow_count{return}
   self.finish_stage_rename(true,cx);
   let stage=self.workflow_stages.remove(index);let slot=if slot>index{slot-1}else{slot}.min(self.workflow_count-1);self.workflow_stages.insert(slot,stage);self.workflow_selected=slot;
  }else if let Some(module_id)=&payload.module{
   if self.workflow_count>=FLOW_STAGE_CAP{if let Ok(mut shared)=self.shared.lock(){shared.message=format!("流程最多 {FLOW_STAGE_CAP} 个阶段");}cx.notify();return}
   let Some(module)=self.workflow_library.iter().find(|module|&module.id==module_id) else{return};
   let minutes=match module.minutes.read(cx).text().trim().parse::<u64>(){Ok(m)if(1..=240).contains(&m)=>m,_=>{if let Ok(mut shared)=self.shared.lock(){shared.message="模块时长需要1–240分钟".into();}cx.notify();return}};
   let(name,kind)=(module.name.clone(),module.kind);
   self.finish_stage_rename(true,cx);
   let fields=self.workflow_stages.remove(self.workflow_count);
   fields.0.update(cx,|f,cx|f.set_text(&name,cx));fields.1.update(cx,|f,cx|f.set_text(&minutes.to_string(),cx));let slot=slot.min(self.workflow_count);self.workflow_stages.insert(slot,(fields.0,fields.1,kind));self.workflow_count+=1;self.workflow_selected=slot;
  }else{return}
  self.workflow_generation+=1;cx.notify();
 }
 fn flow_remove(&mut self,index:usize,cx:&mut Context<Self>){if self.workflow_busy||index>=self.workflow_count{return}self.finish_stage_rename(true,cx);self.workflow_drop=None;let removed=self.workflow_stages.remove(index);self.workflow_stages.push(removed);self.workflow_count-=1;self.workflow_selected=self.workflow_selected.min(self.workflow_count.saturating_sub(1));self.workflow_generation+=1;cx.notify();}
 fn flow_duplicate(&mut self,cx:&mut Context<Self>){let index=self.workflow_selected;if self.workflow_busy||self.workflow_count>=FLOW_STAGE_CAP||index>=self.workflow_count{return}self.finish_stage_rename(true,cx);let name=self.workflow_stages[index].0.read(cx).text().to_owned();let minutes=self.workflow_stages[index].1.read(cx).text().to_owned();let kind=self.workflow_stages[index].2;let fields=self.workflow_stages.remove(self.workflow_count);fields.0.update(cx,|f,cx|f.set_text(&name,cx));fields.1.update(cx,|f,cx|f.set_text(&minutes,cx));self.workflow_stages.insert(index+1,(fields.0,fields.1,kind));self.workflow_count+=1;self.workflow_selected=index+1;self.workflow_generation+=1;cx.notify();}

 fn next_module_id(&mut self)->String{self.workflow_module_serial+=1;format!("user-{}-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),self.workflow_module_serial)}
 /// A new module is a real library entry immediately; the user can rename it inline next.
 fn library_add(&mut self,cx:&mut Context<Self>){
  if self.workflow_library.len()>=FLOW_LIBRARY_CAP{if let Ok(mut shared)=self.shared.lock(){shared.message=format!("模块库最多 {FLOW_LIBRARY_CAP} 个，请先删除不用的模块");}cx.notify();return}
  let id=self.next_module_id();let value=json!({"id":id.clone(),"name":format!("新模块 {}",self.workflow_library.len()+1),"minutes":25,"kind":5});
  let weak=cx.entity().downgrade();let module=new_library_module(&value,&weak,cx);
  self.workflow_library.push(module);self.workflow_module_selected=Some(id);self.workflow_stage_editing=None;self.save_workflow_library(cx);cx.notify();
 }
 fn library_duplicate(&mut self,id:&str,cx:&mut Context<Self>){
  if self.workflow_library.len()>=FLOW_LIBRARY_CAP{if let Ok(mut shared)=self.shared.lock(){shared.message=format!("模块库最多 {FLOW_LIBRARY_CAP} 个，请先删除不用的模块");}cx.notify();return}
  let Some(index)=self.workflow_library.iter().position(|module|module.id==id) else{return};
  let minutes=self.workflow_library[index].minutes.read(cx).text().trim().parse::<u64>().ok().filter(|minutes|(1..=240).contains(minutes));
  let Some(minutes)=minutes else{if let Ok(mut shared)=self.shared.lock(){shared.message="先把这个模块的时长改成 1–240 分钟，再复制".into();}cx.notify();return};
  let kind=self.workflow_library[index].kind;let name=self.workflow_library[index].name.clone();
  let mut copy_name=format!("{name} 副本");if copy_name.chars().count()>40{copy_name=copy_name.chars().take(40).collect();}
  let copy_id=self.next_module_id();let value=json!({"id":copy_id.clone(),"name":copy_name,"minutes":minutes,"kind":kind});
  let weak=cx.entity().downgrade();let module=new_library_module(&value,&weak,cx);
  self.workflow_library.insert(index+1,module);self.workflow_module_selected=Some(copy_id);self.save_workflow_library(cx);cx.notify();
 }
 fn library_delete(&mut self,id:&str,cx:&mut Context<Self>){
  if let Some(index)=self.workflow_library.iter().position(|module|module.id==id){self.workflow_library.remove(index);}
  if self.workflow_module_selected.as_deref()==Some(id){self.workflow_module_selected=None;}
  self.save_workflow_library(cx);cx.notify();
 }
 fn library_cycle_kind(&mut self,id:&str,cx:&mut Context<Self>){
  if let Some(module)=self.workflow_library.iter_mut().find(|module|module.id==id){module.kind=(module.kind+1)%FLOW_MODULES.len();}
  self.save_workflow_library(cx);cx.notify();
 }
 fn library_add_to_flow(&mut self,id:&str,cx:&mut Context<Self>){
  let Some(module)=self.workflow_library.iter().find(|module|module.id==id) else{return};
  let payload=FlowDrag{source:None,module:Some(id.to_owned()),kind:module.kind,generation:self.workflow_generation,title:module.name.clone()};
  self.flow_drop(&payload,self.workflow_count,cx);
 }
 fn begin_library_rename(&mut self,id:&str,window:&mut Window,cx:&mut Context<Self>){
  let Some(index)=self.workflow_library.iter().position(|module|module.id==id) else{return};
  self.finish_stage_rename(true,cx);
  self.finish_library_renames(Some(id),cx);
  let name=self.workflow_library[index].name.clone();
  self.workflow_library[index].renaming=true;self.workflow_library[index].rename_backup=name.clone();
  self.workflow_library[index].name_field.update(cx,|field,cx|field.edit(&name,cx));
  let handle=self.workflow_library[index].name_field.read(cx).focus_handle(cx);window.focus(&handle,cx);
  self.workflow_module_selected=Some(id.to_owned());self.workflow_stage_editing=None;cx.notify();
 }
 fn library_rename_commit(&mut self,id:&str,text:&str,cx:&mut Context<Self>){
  let Some(index)=self.workflow_library.iter().position(|module|module.id==id) else{return};
  let name=text.trim().to_owned();
  if name.is_empty()||name.chars().count()>40||name.chars().any(char::is_control){if let Ok(mut shared)=self.shared.lock(){shared.message="模块名称需要 1–40 个字".into();}cx.notify();return}
  self.workflow_library[index].name=name.clone();self.workflow_library[index].renaming=false;self.workflow_library[index].rename_backup.clear();
  self.workflow_library[index].name_field.update(cx,|field,cx|field.set_text(&name,cx));
  self.save_workflow_library(cx);cx.notify();
 }
 fn library_rename_cancel(&mut self,id:&str,cx:&mut Context<Self>){
  let Some(index)=self.workflow_library.iter().position(|module|module.id==id) else{return};
  let name=if self.workflow_library[index].rename_backup.is_empty(){self.workflow_library[index].name.clone()}else{self.workflow_library[index].rename_backup.clone()};
  self.workflow_library[index].renaming=false;
  self.workflow_library[index].rename_backup.clear();
  self.workflow_library[index].name_field.update(cx,|field,cx|field.set_text(&name,cx));
  cx.notify();
 }
 /// Enter in a module's minute field commits the value; an invalid one stays in the draft so it can be fixed.
 fn library_minutes_commit(&mut self,id:&str,text:&str,cx:&mut Context<Self>){
  let Some(index)=self.workflow_library.iter().position(|module|module.id==id) else{return};
  let Some(minutes)=text.trim().parse::<u64>().ok().filter(|minutes|(1..=240).contains(minutes)) else{
   if let Ok(mut shared)=self.shared.lock(){shared.message="模块时长需要 1–240 分钟".into();}
   cx.notify();return;
  };
  self.workflow_library[index].minutes.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));
  self.save_workflow_library(cx);cx.notify();
 }
 /// Commit any in-place library rename except `except`, so only one name is edited at a time.
 fn finish_library_renames(&mut self,except:Option<&str>,cx:&mut Context<Self>){
  let ids:Vec<String>=self.workflow_library.iter().filter(|module|module.renaming&&Some(module.id.as_str())!=except).map(|module|module.id.clone()).collect();
  for id in ids{
   let text=self.workflow_library.iter().find(|module|module.id==id).map(|module|module.name_field.read(cx).text().to_owned()).unwrap_or_default();
   self.library_rename_commit(&id,&text,cx);
  }
 }
 fn begin_stage_rename(&mut self,index:usize,window:&mut Window,cx:&mut Context<Self>){
  if index>=self.workflow_count{return}
  self.finish_stage_rename(true,cx);
  self.finish_library_renames(None,cx);
  let name=self.workflow_stages[index].0.read(cx).text().to_owned();
  self.workflow_stage_backup=name.clone();
  self.workflow_stages[index].0.update(cx,|field,cx|field.edit(&name,cx));
  let handle=self.workflow_stages[index].0.read(cx).focus_handle(cx);window.focus(&handle,cx);
  self.workflow_stage_editing=Some(index);self.workflow_selected=index;self.workflow_module_selected=None;cx.notify();
 }
 fn finish_stage_rename(&mut self,commit:bool,cx:&mut Context<Self>){
  let Some(index)=self.workflow_stage_editing.take() else{return};
  if index>=self.workflow_count{self.workflow_stage_backup.clear();cx.notify();return}
  let backup=self.workflow_stage_backup.clone();let field=self.workflow_stages[index].0.clone();
  if !commit{field.update(cx,|f,cx|f.set_text(&backup,cx));}
  else{
   let name=field.read(cx).text().trim().to_owned();
   if name.chars().count()>40||name.chars().any(char::is_control){self.workflow_stage_editing=Some(index);if let Ok(mut shared)=self.shared.lock(){shared.message="阶段名称不能超过40字或包含无效字符".into();}cx.notify();return}
   field.update(cx,|f,cx|f.set_text(&name,cx));
  }
  self.workflow_stage_backup.clear();cx.notify();
 }
 fn workflow_modules_json(&self,cx:&Context<Self>)->Result<Value,String>{
  let mut modules=Vec::with_capacity(self.workflow_library.len());
  for module in &self.workflow_library{
   let name=module.name.trim();
   if name.is_empty()||name.chars().count()>40||name.chars().any(char::is_control){return Err(format!("模块“{}”的名称需要 1–40 个字",if name.is_empty(){"未命名"}else{name}))}
   let minutes=module.minutes.read(cx).text().trim().parse::<u64>().ok().filter(|minutes|(1..=240).contains(minutes)).ok_or_else(||format!("模块“{name}”的时长需要 1–240 分钟"))?;
   modules.push(json!({"id":module.id,"name":name,"minutes":minutes,"kind":module.kind}));
  }
  if modules.len()>FLOW_LIBRARY_CAP{return Err(format!("模块库最多 {FLOW_LIBRARY_CAP} 个"))}
  Ok(json!(modules))
 }
 /// Persist the library. The draft stays exactly as typed on failure, so the user can fix and retry.
 fn save_library_edits(&mut self,cx:&mut Context<Self>){
  self.finish_library_renames(None,cx);
  if self.workflow_library.iter().any(|module|module.renaming){return}
  self.save_workflow_library(cx);
 }
 fn save_workflow_library(&mut self,cx:&mut Context<Self>){
  if std::env::args().any(|argument|argument=="--smoke-ui"){return}
  let modules=match self.workflow_modules_json(cx){Ok(modules)=>modules,Err(message)=>{if let Ok(mut shared)=self.shared.lock(){shared.message=message;}cx.notify();return}};
  self.workflow_library_generation+=1;let generation=self.workflow_library_generation;self.workflow_library_busy=true;
  let shared=Rc::clone(&self.shared);let weak=cx.entity().downgrade();
  let job=cx.background_executor().spawn(async move{pipe::command("update",json!({"key":"WorkflowModules","value":modules}))});
  cx.spawn(async move|_,cx|{let result=job.await;let _=weak.update(cx,|view,cx|{if view.workflow_library_generation!=generation{return}view.workflow_library_busy=false;if let Ok(mut shared)=shared.lock(){match result{Ok((reply,state))=>{if reply["success"]==true{shared.state=state;shared.message=String::new();shared.notice="模块库已保存".into();}else{shared.message=reply["message"].as_str().unwrap_or("模块库保存失败，编辑内容已保留").into();shared.notice=shared.message.clone();}},Err(error)=>{shared.message=format!("模块库保存失败：{error}");shared.notice=shared.message.clone();}}}cx.notify();});}).detach();
  cx.notify();
 }

}


const FLOW_MODULES:[(&str,&str,u32);6]=[("整理思路","layout-dashboard",0x9fac83),("专注","timer",0x86a875),("休息","pause",0xc2a487),("复盘","check",0xa398ba),("活动一下","play",0x83aaa4),("自定义","plus",0x9ba8b8)];
/// Stable ids for the six modules every install starts with, so renaming one never breaks a draft.
const FLOW_MODULE_IDS:[&str;6]=["builtin-thinking","builtin-focus","builtin-rest","builtin-review","builtin-move","builtin-custom"];
const FLOW_BUILTIN_MINUTES:[u64;6]=[5,25,5,10,3,10];
const FLOW_LIBRARY_CAP:usize=24;
const FLOW_STAGE_CAP:usize=12;

/// One entry in the 模块库. The two fields are entities created once and reused, so an in-place
/// edit keeps its focus, caret and IME composition instead of being rebuilt every frame; `name` is
/// the last committed value.
struct LibraryModule{id:String,name:String,kind:usize,name_field:Entity<TextField>,minutes:Entity<TextField>,renaming:bool,rename_backup:String}

/// A stage or a library module on its way to the canvas. `source` and `module` are mutually
/// exclusive: one reorders an existing stage, the other inserts an independent snapshot.
#[derive(Clone)]struct FlowDrag{source:Option<usize>,module:Option<String>,kind:usize,generation:u64,title:String}
impl Render for FlowDrag{fn render(&mut self,_:&mut Window,_:&mut Context<Self>)->impl IntoElement{
 div().w(px(214.)).p(px(13.)).rounded(px(13.)).bg(t::surface()).border_1().border_color(t::accent()).shadow_md().text_color(t::text()).flex().items_center().gap(px(9.))
  .child(flow_badge(self.kind,false))
  .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().text_size(px(12.5)).child(self.title.clone()))
  .child(icons::tinted("plus",t::accent()))
}}
fn infer_flow_kind(name:&str)->usize{FLOW_MODULES.iter().position(|(label,_,_)|name.starts_with(label)).unwrap_or(5)}
fn flow_wash(kind:usize)->gpui::Rgba{let a=t::surface();let b=rgb(FLOW_MODULES[kind.min(5)].2);let mix=if t::base().r<0.3{0.16}else{0.09};gpui::Rgba{r:a.r+(b.r-a.r)*mix,g:a.g+(b.g-a.g)*mix,b:a.b+(b.b-a.b)*mix,a:1.}}
fn flow_badge(kind:usize,large:bool)->AnyElement{let d=if large{40.}else{30.};div().w(px(d)).h(px(d)).flex_shrink_0().rounded(px(if large{12.}else{9.})).bg(flow_wash(kind)).flex().items_center().justify_center().child(icons::tinted(FLOW_MODULES[kind].1,rgb(FLOW_MODULES[kind].2)).w(px(if large{21.}else{16.})).h(px(if large{21.}else{16.}))).into_any_element()}

/// A double-click is the rename gesture on both the canvas and the library; a single click selects.
fn flow_double_click(event:&ClickEvent)->bool{match event{ClickEvent::Mouse(mouse)=>mouse.up.click_count>=2,ClickEvent::Touch(touch)=>touch.tap_count>=2,ClickEvent::Keyboard(_)=>false}}

/// Where a drop at `y` lands: before the first card whose midpoint is still below the pointer.
fn flow_drop_index(bounds:&[Bounds<Pixels>],y:Pixels)->usize{let mut index=0;for bound in bounds{if y>bound.center().y{index+=1}else{break}}index}

/// The inserted line is drawn over an existing gap, so receiving feedback never shifts the cards
/// under the pointer — a layout-shifting marker makes the target flicker between two slots.
fn flow_marker_offset(bounds:&[Bounds<Pixels>],index:usize)->Option<Pixels>{
 if bounds.is_empty(){return None}
 let first=bounds[0].origin.y;
 let y=if index<bounds.len(){bounds[index].origin.y-px(2.)}else{let last=bounds[bounds.len()-1];last.origin.y+last.size.height+px(2.)};
 // The marker is positioned in `content`, whose 4px top padding sits above the first card; adding
 // it back keeps slot 0 inside the viewport instead of being clipped by the scroll container.
 Some(y-first+px(4.))
}
fn flow_marker(top:Pixels)->AnyElement{div().absolute().top(top).left(px(0.)).right(px(0.)).h(px(2.)).child(div().w_full().h(px(2.)).rounded_full().bg(t::accent())).into_any_element()}

fn builtin_modules()->Vec<Value>{FLOW_MODULES.iter().enumerate().map(|(kind,(name,_,_))|json!({"id":FLOW_MODULE_IDS[kind],"name":name,"minutes":FLOW_BUILTIN_MINUTES[kind],"kind":kind})).collect()}
fn flow_module_from_value(item:&Value)->Option<Value>{
 let id=item["id"].as_str()?;
 if id.is_empty()||id.len()>64||!id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'||c=='_'){return None}
 let name=item["name"].as_str()?.trim();
 if name.is_empty()||name.chars().count()>40||name.chars().any(char::is_control){return None}
 let minutes=item["minutes"].as_u64().filter(|minutes|(1..=240).contains(minutes))?;
 let kind=item["kind"].as_u64().filter(|kind|*kind<=5)?;
 Some(json!({"id":id,"name":name,"minutes":minutes,"kind":kind}))
}
/// A missing key means an install from before the library existed: seed the six built-ins. An
/// explicitly empty array is a deliberate deletion and stays empty.
fn workflow_module_seed(state:&Value)->Vec<Value>{
 let Some(list)=state["settings"]["WorkflowModules"].as_array() else{return builtin_modules()};
 let modules:Vec<Value>=list.iter().filter_map(flow_module_from_value).collect();
 if modules.is_empty()&&!list.is_empty(){return builtin_modules()}
 modules
}
fn new_library_module(value:&Value,weak:&WeakEntity<SettingsApp>,cx:&mut Context<SettingsApp>)->LibraryModule{
 let id=value["id"].as_str().unwrap_or("module").to_owned();let name=value["name"].as_str().unwrap_or("模块").to_owned();
 let minutes=value["minutes"].as_u64().unwrap_or(25).clamp(1,240);let kind=value["kind"].as_u64().unwrap_or(5).min(5)as usize;
 let name_id=id.clone();let name_commit=id.clone();let name_cancel=id.clone();let commit=weak.clone();let cancel=weak.clone();let initial_name=name.clone();
 let name_field=cx.new(move|cx|{
  let mut field=TextField::new(format!("flow-library-name-{name_id}"),"模块名称",initial_name.clone(),Rc::new(move|text,app|{let _=commit.update(app,|view,cx|view.library_rename_commit(&name_commit,text,cx));}),cx);
  field.set_cancel_handler(Some(Rc::new(move|app|{let _=cancel.update(app,|view,cx|view.library_rename_cancel(&name_cancel,cx));})));
  field
 });
 let minutes_id=id.clone();let minutes_commit=id.clone();let minutes_weak=weak.clone();
 let minutes_field=cx.new(move|cx|TextField::new(format!("flow-library-minutes-{minutes_id}"),"分钟",minutes.to_string(),Rc::new(move|text,app|{let _=minutes_weak.update(app,|view,cx|view.library_minutes_commit(&minutes_commit,text,cx));}),cx));
 LibraryModule{id,name,kind,name_field,minutes:minutes_field,renaming:false,rename_backup:String::new()}
}

/// The draggable body of one library card. The minute field and its two buttons opt out of the
/// drag by swallowing mouse-down, so selecting text there never turns into a module drag.
fn flow_library_card(module:&LibraryModule,selected:bool,generation:u64,cx:&mut Context<SettingsApp>)->AnyElement{
 let id=module.id.clone();let name=module.name.clone();let kind=module.kind;let renaming=module.renaming;
 let payload=FlowDrag{source:None,module:Some(id.clone()),kind,generation,title:name.clone()};
 let weak=cx.entity().downgrade();
 let mut card=div().id(SharedString::from(format!("flow-library-{id}"))).p(px(9.)).rounded(px(13.)).bg(flow_wash(kind)).border_1().border_color(if selected{rgb(FLOW_MODULES[kind].2)}else{t::hairline()}).flex().flex_col().gap(px(8.));
 if !renaming{
  let start=weak.clone();
  card=card.on_drag(payload,move|payload,_,_,app|{let _=start.update(app,|view,cx|{if view.workflow_drop.take().is_some(){cx.notify();}});app.new(|_|payload.clone())});
 }
 let select_id=id.clone();let rename_id=id.clone();
 let name_area=if renaming{div().flex_1().min_w_0().child(module.name_field.clone()).into_any_element()}else{
  div().flex_1().min_w_0().child(div().id(SharedString::from(format!("flow-library-name-{id}"))).w_full().min_w_0().overflow_hidden().text_ellipsis().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).cursor_pointer().child(name.clone()).on_click(cx.listener(move|v,event:&ClickEvent,window,cx|{
   if flow_double_click(event){cx.stop_propagation();v.begin_library_rename(&rename_id,window,cx);}
   else{v.workflow_module_selected=Some(select_id.clone());v.workflow_stage_editing=None;cx.notify();}
  }))).into_any_element()
 };
 let cycle_id=id.clone();
 let header=div().flex().items_center().gap(px(7.))
  .child(flow_badge(kind,false))
  .child(name_area)
  .child(div().id(SharedString::from(format!("flow-library-kind-{id}"))).w(px(23.)).h(px(23.)).flex_shrink_0().rounded(px(8.)).flex().items_center().justify_center().cursor_pointer().hover(|style|style.bg(t::surface_hi())).child(icons::tinted("shuffle",rgb(FLOW_MODULES[kind].2)).w(px(13.)).h(px(13.))).on_click(cx.listener(move|v,_,_,cx|{cx.stop_propagation();v.library_cycle_kind(&cycle_id,cx);})));
 let duplicate_id=id.clone();let delete_id=id.clone();let add_id=id.clone();
 let footer=div().flex().items_center().gap(px(4.)).on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
  .child(div().w(px(48.)).child(module.minutes.clone()))
  .child(div().text_size(px(10.5)).text_color(t::text_dim()).child("分"))
  .child(div().flex_1())
  .child(div().id(SharedString::from(format!("flow-library-add-{id}"))).w(px(23.)).h(px(23.)).flex().items_center().justify_center().rounded(px(7.)).cursor_pointer().hover(|style|style.bg(t::surface_hi())).child(icons::tinted("arrow-right",t::accent()).w(px(12.)).h(px(12.))).on_click(cx.listener(move|v,_,_,cx|v.library_add_to_flow(&add_id,cx))))
  .child(div().id(SharedString::from(format!("flow-library-copy-{id}"))).w(px(23.)).h(px(23.)).flex().items_center().justify_center().rounded(px(7.)).cursor_pointer().text_color(t::text_dim()).hover(|style|style.bg(t::surface_hi())).child(icons::tinted("repeat",t::text_dim()).w(px(12.)).h(px(12.))).on_click(cx.listener(move|v,_,_,cx|v.library_duplicate(&duplicate_id,cx))))
  .child(div().id(SharedString::from(format!("flow-library-delete-{id}"))).w(px(23.)).h(px(23.)).flex().items_center().justify_center().rounded(px(7.)).cursor_pointer().text_color(t::text_dim()).hover(|style|style.bg(t::surface_hi())).child(icons::tinted("trash-2",t::text_dim()).w(px(12.)).h(px(12.))).on_click(cx.listener(move|v,_,_,cx|v.library_delete(&delete_id,cx))));
 card.child(header).child(footer).into_any_element()
}

fn flow_library_panel(view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{
 let generation=view.workflow_generation;
 let mut list=div().id("flow-library-list").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(8.)).pr(px(2.));
 if view.workflow_library.is_empty(){
  list=list.child(div().w_full().py(px(26.)).flex().flex_col().items_center().gap(px(6.)).text_color(t::text_dim()).child(icons::tinted("plus",t::text_faint()).w(px(20.)).h(px(20.))).child(div().text_size(px(11.5)).child("模块库是空的")).child(div().text_size(px(10.)).text_color(t::text_faint()).child("点上面的「新模块」添加")));
 }
 for module in &view.workflow_library{
  let selected=view.workflow_module_selected.as_deref()==Some(module.id.as_str());
  list=list.child(flow_library_card(module,selected,generation,cx));
 }
 div().w(px(208.)).flex_shrink_0().h(px(456.)).p(px(14.)).rounded(px(18.)).bg(t::base()).flex().flex_col().gap(px(9.))
  .child(div().flex().items_center().justify_between().child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("模块库")).child(div().text_size(px(10.)).text_color(t::text_dim()).child(format!("{}/{}",view.workflow_library.len(),FLOW_LIBRARY_CAP))))
  .child(div().text_size(px(10.)).text_color(t::text_dim()).child("拖动加入流程 · 双击名称改名"))
  .child(div().id("flow-library-add").w_full().h(px(30.)).flex_shrink_0().rounded(px(9.)).border_1().border_color(t::hairline()).bg(t::surface()).flex().items_center().justify_center().gap(px(5.)).cursor_pointer().hover(|style|style.bg(t::surface_hi())).child(icons::tinted("plus",t::text_dim()).w(px(12.)).h(px(12.))).child(div().text_size(px(11.5)).text_color(t::text_dim()).child("新模块")).on_click(cx.listener(|v,_,_,cx|v.library_add(cx))))
  .child(list)
  .child(div().id("flow-library-save").w_full().h(px(32.)).flex_shrink_0().rounded(px(9.)).bg(t::accent()).text_color(gpui::white()).flex().items_center().justify_center().cursor_pointer().active(|style|style.opacity(0.8)).child(div().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).child(if view.workflow_library_busy{"正在保存…"}else{"保存模块库"})).on_click(cx.listener(|v,_,_,cx|v.save_library_edits(cx))))
  .into_any_element()
}

/// One stage card. The whole card drags and double-clicks to rename; while the name field is open
/// the card gives up the drag so selecting text inside it cannot start a reorder.
fn flow_stage_card(view:&SettingsApp,index:usize,cx:&mut Context<SettingsApp>)->AnyElement{
 let(name_field,minutes_field,kind)=(view.workflow_stages[index].0.clone(),view.workflow_stages[index].1.clone(),view.workflow_stages[index].2);
 let name=name_field.read(cx).text().to_owned();let minutes=minutes_field.read(cx).text().to_owned();
 let editing=view.workflow_stage_editing==Some(index);let selected=view.workflow_selected==index;
 let title=if name.is_empty(){format!("阶段 {}",index+1)}else{name.clone()};
 let payload=FlowDrag{source:Some(index),module:None,kind,generation:view.workflow_generation,title:title.clone()};
 let weak=cx.entity().downgrade();
 let mut card=div().id(("flow-stage",index)).min_h(px(74.)).flex_shrink_0().flex().items_center().gap(px(10.)).px(px(12.)).py(px(10.)).rounded(px(14.)).border_1().border_color(if selected{rgb(FLOW_MODULES[kind].2)}else{t::hairline()}).bg(flow_wash(kind)).cursor_pointer()
  .on_click(cx.listener(move|v,event:&ClickEvent,window,cx|{
   if flow_double_click(event){v.begin_stage_rename(index,window,cx);}
   else{v.workflow_selected=index;v.workflow_module_selected=None;window.focus(&v.workflow_focus,cx);cx.notify();}
  }));
 if !editing{
  card=card.cursor_move().on_drag(payload,move|payload,_,_,app|{let _=weak.update(app,|view,cx|{if view.workflow_drop.take().is_some(){cx.notify();}});app.new(|_|payload.clone())});
 }
 let mut body=div().flex_1().min_w_0().flex().flex_col().gap(px(5.));
 if editing{body=body.child(div().on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation()).child(name_field.clone()));}
 else{body=body.child(div().w_full().min_w_0().overflow_hidden().text_ellipsis().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(title));}
 body=body.child(div().text_size(px(10.)).text_color(t::text_dim()).child(format!("{:02} · 完成后等待确认",index+1)));
 card.child(div().id(("flow-handle",index)).flex_shrink_0().p(px(3.)).text_color(t::text_dim()).child("⠿"))
  .child(flow_badge(kind,false))
  .child(body)
  .child(div().flex_shrink_0().flex().flex_col().items_end().gap(px(3.)).child(div().text_size(px(18.)).font_weight(FontWeight::MEDIUM).text_color(rgb(FLOW_MODULES[kind].2)).child(minutes)).child(div().text_size(px(10.)).text_color(t::text_dim()).child("分")))
  .into_any_element()
}

fn flow_detail_panel(view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{
 let mut detail=div().w(px(210.)).flex_shrink_0().h(px(456.)).p(px(14.)).rounded(px(18.)).bg(t::base()).flex().flex_col().gap(px(10.)).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("阶段详情"));
 let selected=view.workflow_selected;
 if selected<view.workflow_count{
  let(name_field,minutes_field,kind)=(view.workflow_stages[selected].0.clone(),view.workflow_stages[selected].1.clone(),view.workflow_stages[selected].2);
  let name=name_field.read(cx).text().to_owned();
  detail=detail.child(div().flex().items_center().gap(px(9.)).child(flow_badge(kind,true)).child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().w_full().min_w_0().overflow_hidden().text_ellipsis().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(if name.is_empty(){format!("阶段 {}",selected+1)}else{name})).child(div().text_size(px(10.)).text_color(t::text_faint()).child(format!("第 {} 段",selected+1)))));
  if view.workflow_stage_editing==Some(selected){
   detail=detail.child(div().p(px(10.)).rounded(px(10.)).bg(t::surface()).text_size(px(10.5)).text_color(t::text_dim()).child("正在原位改名：Enter 保存，Esc 取消。"));
  }else{
   detail=detail.child(field_line("阶段名称",182.,name_field));
  }
  detail=detail.child(field_line("持续时间（分钟）",182.,minutes_field.clone()));
  let mut presets=div().flex().gap(px(4.));
  for minutes in [5u64,15,25,45]{let field=minutes_field.clone();presets=presets.child(small_button(SharedString::from(format!("flow-detail-preset-{minutes}")),&format!("{minutes}"),cx.listener(move|_,_,_,cx|{field.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));cx.notify();})));}
  detail=detail.child(presets).child(div().text_size(px(10.)).text_color(t::text_dim()).child("只修改这一段，不影响模块库和其他阶段。"));
  detail=detail
   .child(div().flex().flex_wrap().gap(px(6.))
    .child(small_button("flow-rename".into(),"改名",cx.listener(move|v,_,window,cx|v.begin_stage_rename(selected,window,cx))))
    .child(small_button("flow-copy".into(),"复制",cx.listener(|v,_,_,cx|v.flow_duplicate(cx))))
    .child(small_button("flow-delete".into(),"删除",cx.listener(move|v,_,_,cx|v.flow_remove(selected,cx)))))
   .child(div().flex().flex_wrap().gap(px(6.))
    .child(small_button("flow-up".into(),"上移",cx.listener(|v,_,_,cx|{let index=v.workflow_selected;if index>0{let kind=v.workflow_stages[index].2;v.flow_drop(&FlowDrag{source:Some(index),module:None,kind,generation:v.workflow_generation,title:String::new()},index-1,cx);}})))
    .child(small_button("flow-down".into(),"下移",cx.listener(|v,_,_,cx|{let index=v.workflow_selected;if index+1<v.workflow_count{let kind=v.workflow_stages[index].2;v.flow_drop(&FlowDrag{source:Some(index),module:None,kind,generation:v.workflow_generation,title:String::new()},index+2,cx);}}))));
 }else{
  detail=detail.child(div().pt(px(24.)).text_size(px(12.)).text_color(t::text_dim()).child("选中画布中的阶段，编辑名称和时间。"));
 }
 detail.into_any_element()
}

pub(crate)fn workflow_editor_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let total=view.workflow_stages.iter().take(view.workflow_count).filter_map(|(_,field,_)|field.read(cx).text().parse::<u64>().ok()).sum::<u64>();
 let mut content=div().relative().flex().flex_col().gap(px(8.)).pt(px(4.)).pb(px(12.));
 let marker_top=if cx.has_active_drag()&&view.workflow_count>0{view.workflow_drop.and_then(|index|flow_marker_offset(view.workflow_stage_bounds.borrow().as_slice(),index))}else{None};
 // Prepaint refreshes every live slot; dropping stale tail entries keeps removed stages out of hit-testing.
 view.workflow_stage_bounds.borrow_mut().truncate(view.workflow_count);
 if let Some(top)=marker_top{content=content.child(flow_marker(top));}
 for index in 0..view.workflow_count{
  let bounds_cell=view.workflow_stage_bounds.clone();
  content=content.child(div().on_children_prepainted(move|children,_,_|{
   if let Some(bound)=children.first(){let mut list=bounds_cell.borrow_mut();if list.len()<=index{list.resize(index+1,Bounds::new(point(px(0.),px(0.)),size(px(0.),px(0.))));}list[index]=*bound;}
  }).id(("flow-stage-wrap",index)).flex_shrink_0().child(flow_stage_card(view,index,cx)));
 }
 if view.workflow_count==0{
  content=content.child(div().id("flow-empty").w_full().h(px(300.)).flex().flex_col().items_center().justify_center().gap(px(12.)).child(icons::tinted("plus",t::text_faint()).w(px(24.)).h(px(24.))).child(div().text_size(px(13.)).text_color(t::text_dim()).child("把第一块时间放到这里")).child(div().text_size(px(11.)).text_color(t::text_faint()).child("从左侧模块库拖入，或点卡片上的箭头加入")));
 }
 let stage_scroll=div().id("flow-canvas-scroll").flex_1().min_h_0().w_full().overflow_y_scroll().track_scroll(&view.workflow_scroll).child(content);
 let viewport_cell=view.workflow_canvas_bounds.clone();
 let drag_canvas=view.workflow_canvas_bounds.clone();
 let drag_bounds=view.workflow_stage_bounds.clone();
 let drag_scroll=view.workflow_scroll.clone();
 let weak=cx.entity().downgrade();
 let body=div().on_children_prepainted(move|children,_,_|{if let Some(bound)=children.first(){viewport_cell.set(*bound);}})
  .id("flow-canvas-body").flex_1().min_h_0().w_full().flex().flex_col().rounded(px(14.)).bg(t::surface()).border_1().border_color(if view.workflow_drop.is_some()&&cx.has_active_drag(){t::accent()}else{t::hairline()})
  .on_drag_move::<FlowDrag>(move|event,_window,cx|{
   let pointer=event.event.position;let canvas=drag_canvas.get();
   let inside=canvas.size.width>px(0.)&&canvas.size.height>px(0.)&&canvas.contains(&pointer);
   let drag=event.drag(cx);
   let allowed=drag.source.is_some()||drag_bounds.borrow().len()<FLOW_STAGE_CAP;
   let mut scrolled=false;
   if inside{let edge=px(30.);let mut offset=drag_scroll.offset();if pointer.y<canvas.origin.y+edge{offset.y+=px(12.);}else if pointer.y>canvas.origin.y+canvas.size.height-edge{offset.y-=px(12.);}if offset!=drag_scroll.offset(){drag_scroll.set_offset(offset);scrolled=true;}}
   let next=if inside&&allowed{Some(flow_drop_index(drag_bounds.borrow().as_slice(),pointer.y))}else{None};
   let _=weak.update(cx,|view,cx|{if view.workflow_drop!=next{view.workflow_drop=next;cx.notify();}});
   if scrolled{cx.refresh_windows();}
  })
  .child(stage_scroll)
  .on_drop(cx.listener(move|v,payload:&FlowDrag,_,cx|{if let Some(slot)=v.workflow_drop.take(){v.flow_drop(payload,slot,cx);}}));
 let canvas=div().flex_1().min_w(px(300.)).h(px(456.)).p(px(16.)).rounded(px(18.)).bg(t::base()).flex().flex_col().gap(px(10.))
  .child(div().flex().items_center().justify_between().gap(px(8.)).child(div().text_size(px(10.5)).text_color(t::text_dim()).child(format!("我的流程 · {} 个阶段 · {} 分钟",view.workflow_count,total))).child(div().text_size(px(10.)).text_color(t::text_faint()).child(if view.workflow_drop.is_some(){"松手放下"}else{"拖动卡片调整顺序"})))
  .child(div().flex().flex_col().gap(px(4.)).child(div().text_size(px(10.)).text_color(t::text_faint()).child("流程名称")).child(view.workflow_name.clone()))
  .child(body);
 let mut rows=vec![div().flex().items_start().gap(px(14.)).w_full().text_color(t::text()).child(flow_library_panel(view,cx)).child(canvas).child(flow_detail_panel(view,cx)).into_any_element()];
 if let Some(template)=view.workflow_pending.clone(){
  rows.push(div().flex().items_center().gap(px(10.)).child(div().flex_1().text_size(px(12.)).child("载入模板将替换当前草稿，运行中的流程不受影响。"))
   .child(small_button("flow-confirm-load".into(),"替换草稿",cx.listener(move|v,_,_,cx|v.load_workflow_template(&template,cx))))
   .child(small_button("flow-cancel-load".into(),"保留草稿",cx.listener(|v,_,_,cx|{v.workflow_pending=None;cx.notify();}))).into_any_element());
 }
 if let Some(templates)=state["templates"].as_array().filter(|templates|!templates.is_empty()){
  let mut list=div().flex().flex_wrap().gap(px(8.));
  for template in templates{let payload=template.clone();let id=template["id"].as_str().unwrap_or("").to_owned();list=list.child(div().flex().items_center().gap(px(4.)).child(pill_button(SharedString::from(format!("load-{id}")),template["name"].as_str().unwrap_or("流程模板"),false,cx.listener(move|v,_,_,cx|{v.workflow_pending=Some(payload.clone());cx.notify();}))).child(small_button(SharedString::from(format!("template-delete-{id}")),"删除",cx.listener(move|v,_,_,cx|send_todo(&v.shared,"workflowTemplateDelete",json!(id),cx)))));}
  rows.push(section_block("我的模板",None,list.into_any_element(),cx));
 }
 rows
}
fn workflow_footer(view:&SettingsApp,cx:&mut Context<SettingsApp>)->AnyElement{
 div().flex().items_center().justify_between().gap(px(12.)).px(px(21.)).py(px(16.)).border_t_1().border_color(t::hairline()).bg(t::surface()).child(div().text_size(px(11.)).text_color(t::text_dim()).child("2–12段 · 总时长≤24小时 · 阶段完成后等待确认"))
  .child(div().flex().gap(px(8.)).child(pill_button("flow-template-save".into(),if view.workflow_busy{"处理中…"}else{"保存为模板"},false,cx.listener(|v,_,_,cx|v.commit_flow(true,cx))))
   .child(pill_button("flow-submit".into(),"创建并开始",true,cx.listener(|v,_,_,cx|v.submit_workflow(cx))))).into_any_element()
}
