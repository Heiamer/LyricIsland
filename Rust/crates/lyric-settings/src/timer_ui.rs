// A clock-first layout. The dial is vector painted at the window's native scale: a soft track with
// the green arc on top, and the read-out living on an inner disc. The arc shows what is *left* for
// 番茄钟 and 倒计时 (a full ring before starting) and the time recorded for 秒表.
fn clock_dial(ring:f32,accent:gpui::Rgba,size:f32)->AnyElement{
 let track=t::surface_hi();
 let radius=(size-7.)/2.;
 canvas(|_,_,_|{},move |bounds,_,window,_|{
  let center=point(bounds.origin.x+bounds.size.width/2.,bounds.origin.y+bounds.size.height/2.);
  for(fraction,ink)in[(1.,track),(ring.clamp(0.,1.),accent)]{if fraction<=0.{continue}
   let steps=(240.*fraction).ceil() as usize;let mut path=PathBuilder::stroke(px(7.));
   for i in 0..=steps{let a=-std::f32::consts::FRAC_PI_2+std::f32::consts::TAU*fraction*i as f32/steps as f32;
    let p=point(center.x+px(radius*a.cos()),center.y+px(radius*a.sin()));if i==0{path.move_to(p)}else{path.line_to(p)}}
   if let Ok(path)=path.build(){window.paint_path(path,ink);}}
 }).w(px(size)).h(px(size)).into_any_element()
}
const TIMER_MODES:[(&str,&str);4]=[("workflow","流程"),("pomodoro","番茄钟"),("countdown","倒计时"),("stopwatch","秒表")];
fn timer_seconds(clock:&str)->f64{let parts=clock.split(':').map(|part|part.parse::<f64>().unwrap_or(0.)).collect::<Vec<_>>();match parts.len(){3=>parts[0]*3600.+parts[1]*60.+parts[2],2=>parts[0]*60.+parts[1],_=>0.}}
fn timer_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let clocks=state["timers"].as_array().cloned().unwrap_or_default();
 let active=text(state,"activeComponent","").strip_prefix("timer:").unwrap_or("");
 let current=if view.timer_new{None}else{clocks.iter().find(|c|c["id"].as_str()==Some(active)&&c["mode"].as_str()==Some(&view.timer_mode)).or_else(||clocks.iter().find(|c|c["mode"].as_str()==Some(&view.timer_mode)))};
 let mode_index=TIMER_MODES.iter().position(|(mode,_)|*mode==view.timer_mode.as_str()).unwrap_or(0);
 let mut rows=vec![tab_row("timer-mode",&["流程","番茄钟","倒计时","秒表"],mode_index,cx,|v,index|{v.timer_mode=TIMER_MODES[index].0.to_owned();v.timer_new=false;v.timer_adjust_open=false;})];
 let seconds=if matches!(view.timer_mode.as_str(),"stopwatch"|"workflow"){0}else{let field=if view.timer_mode=="pomodoro"{&view.timer_focus}else{&view.timer_duration};field.read(cx).text().trim().parse::<u64>().unwrap_or(0)};
 let clock=current.and_then(|c|c["clock"].as_str()).map(str::to_owned).unwrap_or_else(||if seconds>=3600{format!("{:02}:{:02}:{:02}",seconds/3600,seconds/60%60,seconds%60)}else{format!("{:02}:{:02}",seconds/60,seconds%60)});
 let running=current.is_some_and(|c|c["running"]==true);
 let finished=current.is_some_and(|c|c["finished"]==true);
 let progress=current.and_then(|c|c["progress"].as_f64()).unwrap_or(0.);
 let ring=if view.timer_mode=="stopwatch"{(timer_seconds(&clock)/3600.) as f32}else if current.is_some(){(1.-progress).clamp(0.,1.) as f32}else{1.};
 let id=current.and_then(|c|c["id"].as_str()).map(str::to_owned);
 let revision=current.and_then(|c|c["revision"].as_u64()).unwrap_or(0);
 let stages=current.and_then(|c|c["workflow"]["stages"].as_array()).cloned().unwrap_or_default();
 let stage_index=current.and_then(|c|c["workflow"]["stageIndex"].as_u64()).unwrap_or(0) as usize;
 let waiting=current.is_some_and(|c|c["workflow"]["waiting"]==true);
 let stage_name=stages.get(stage_index).and_then(|s|s["name"].as_str()).map(str::to_owned);
 let total_minutes=stages.iter().filter_map(|s|s["seconds"].as_u64()).sum::<u64>()/60;
 let name=if view.timer_mode=="workflow"{stage_name.clone().unwrap_or_else(||if id.is_some(){"流程已结束".into()}else{"把一段流程拆成可执行的阶段".into()})}else{current.and_then(|c|c["name"].as_str()).filter(|name|!name.is_empty()).unwrap_or(match view.timer_mode.as_str(){"pomodoro"=>"留一点时间，专注当下","countdown"=>"为下一件事留好时间",_=>"从此刻开始记录"}).to_owned()};
 let detail=if view.timer_mode=="workflow"{if id.is_some(){format!("流程 · 共 {} 个阶段 · {} 分钟",stages.len(),if total_minutes==0{1}else{total_minutes})}else{"每个阶段结束后手动进入下一阶段".into()}}else{current.and_then(|c|c["subtitle"].as_str()).filter(|text|!text.is_empty()).unwrap_or(match view.timer_mode.as_str(){"pomodoro"=>"结束后提醒确认待办进展",_=>"运行时可在灵动岛查看"}).to_owned()};
 let note=if id.is_some(){if waiting{"等待下一阶段"}else if running{"进行中"}else if finished{"已完成"}else{"已暂停"}}else{"准备开始"};
 let status=match view.timer_mode.as_str(){
  "pomodoro"=>{let rounds=current.and_then(|c|c["rounds"].as_u64()).unwrap_or_else(||view.timer_rounds.read(cx).text().parse().unwrap_or(4)).clamp(1,16);let round=current.and_then(|c|c["round"].as_u64()).unwrap_or(1);format!("专注 · 第 {round} / {rounds} 轮")}
  "countdown"=>{let left=if current.is_some(){timer_seconds(&clock).round() as u64}else{seconds};format!("还剩 {} 分钟",(left+59)/60)}
  "workflow"=>if id.is_some(){format!("第 {} / {} 阶段",stage_index+1,stages.len())}else{"多阶段流程".to_owned()},
  _=>"记录经过的时间".to_owned(),
 };
 let(reset_id,toggle_id)=(id.clone(),id.clone());
 let workflow=view.timer_mode=="workflow";
 let primary_label=if workflow&&waiting{"下一阶段"}else if workflow&&id.is_none(){"新建流程"}else if running{"暂停"}else if finished{"重新开始"}else if id.is_some(){"继续"}else{"开始"};
 let primary_icon=if (workflow&&waiting)||(workflow&&id.is_none()){"arrow-right"}else if running{"pause"}else{"play"};
 let primary_action=if id.is_none(){"new"}else if workflow&&waiting{"next"}else if finished{if workflow{"restart"}else{"reset"}}else if running{"pause"}else{"resume"};
 // A running workflow adds its own stage-action row; shrink the dial so the bottom controls
 // still fit above the footer without scrolling.
 let compact=workflow&&id.is_some();
 let dial:f32=if compact{164.}else{196.};
 let disc:f32=(dial-14.).max(0.);
 let hero=div().flex().flex_col().items_center().gap(px(if compact{12.}else{16.})).w_full().p(px(18.)).rounded(px(16.)).bg(t::surface()).border_1().border_color(t::hairline())
  .child(pill(&status))
  .child(div().relative().w(px(dial)).h(px(dial)).flex_shrink_0().child(clock_dial(ring,t::accent(),dial))
   .child(div().absolute().left(px(7.)).top(px(7.)).w(px(disc)).h(px(disc)).rounded(px(disc/2.)).bg(t::surface()).flex().flex_col().items_center().justify_center().gap(px(7.))
    .child(div().text_size(px(if clock.len()>5{32.}else{40.})).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(clock.clone()))
    .child(div().text_size(px(12.)).text_color(t::text_dim()).child(note))))
  .child(div().flex().flex_col().items_center().gap_1().min_w_0()
   .child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).child(name))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child(detail)))
  .child(div().flex().items_center().justify_center().gap(px(12.)).children(reset_id.as_ref().map(|id|{let id=id.clone();pill_button_icon("clock-reset".into(),Some("rotate-ccw"),"重置",false,cx.listener(move|v,_,_,cx|send_todo(&v.shared,"timerAction",json!({"id":id,"action":"reset","revision":revision}),cx))) }))
   .child(div().id("timer-start").flex().items_center().justify_center().gap(px(7.)).min_w(px(110.)).min_h(px(36.)).px(px(13.)).py(px(8.)).rounded(px(10.)).cursor_pointer()
    .bg(t::accent()).text_size(px(13.5)).font_weight(FontWeight::MEDIUM).text_color(t::on_accent())
    .hover(|style|style.opacity(0.92)).active(|style|style.opacity(0.8))
    .child(icons::tinted(primary_icon, t::on_accent())).child(primary_label)
    .on_click(cx.listener(move|v,_,_,cx|{if let Some(id)=&toggle_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":primary_action,"revision":revision}),cx)}else if v.timer_mode=="workflow"{v.open_workflow_form(None,cx)}else{v.create_timer(cx)}}))));
 let mut controls=div().flex().items_center().justify_center().gap(px(6.));
 if matches!(view.timer_mode.as_str(),"pomodoro"|"countdown"){
  for minutes in [15u64,25,45]{let field=if view.timer_mode=="pomodoro"{view.timer_focus.clone()}else{view.timer_duration.clone()};
   controls=controls.child(div().id(("clock-preset",minutes as usize)).px(px(12.)).py(px(7.)).rounded(px(10.)).bg(t::surface()).text_size(px(13.)).text_color(t::text_dim()).cursor_pointer().hover(|style|style.bg(t::surface_hi())).child(format!("{minutes} 分钟")).on_click(cx.listener(move|_,_,_,cx|{field.update(cx,|field,cx|field.set_text(&(minutes*60).to_string(),cx));cx.notify();})));}
 }
 let(edit_id,next_id,skip_id,extend_id,end_id)=(id.clone(),id.clone(),id.clone(),id.clone(),id.clone());
 let hero=if workflow&&id.is_some()&&!finished{
  let mut bar=div().flex().items_center().justify_center().gap(px(6.));
  if waiting{bar=bar.child(small_button("clock-next".into(),"下一阶段",cx.listener(move|v,_,_,cx|{if let Some(id)=&next_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"next","revision":revision}),cx)}})));}
  bar=bar.child(small_button("clock-skip".into(),"跳过",cx.listener(move|v,_,_,cx|{if let Some(id)=&skip_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"skip","revision":revision}),cx)}})))
   .child(small_button("clock-extend".into(),"延长 5 分钟",cx.listener(move|v,_,_,cx|{if let Some(id)=&extend_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"extend5","revision":revision}),cx)}})))
   .child(small_button("clock-end".into(),"结束",cx.listener(move|v,_,_,cx|{if let Some(id)=&end_id{send_todo(&v.shared,"timerAction",json!({"id":id,"action":"end","revision":revision}),cx)}})));
  hero.child(bar)
 }else{hero};
 if workflow{
  if let Some(id)=&edit_id{let edit=id.clone();controls=controls.child(pill_button_icon("clock-edit-flow".into(),Some("repeat"),"编辑流程",false,cx.listener(move|v,_,_,cx|v.open_workflow_form(Some(edit.clone()),cx))));
   controls=controls.child(pill_button_icon("clock-new".into(),Some("plus"),"新建流程",false,cx.listener(|v,_,_,cx|v.open_workflow_form(None,cx))));}
 }else{
  controls=controls.child(pill_button_icon("clock-adjust".into(),Some(if view.timer_adjust_open{"chevron-up"}else{"chevron-down"}),"调整",false,cx.listener(|v,_,_,cx|{v.timer_adjust_open=!v.timer_adjust_open;cx.notify();})));
  controls=controls.child(pill_button_icon("clock-new".into(),Some("plus"),"新计时",false,cx.listener(|v,_,_,cx|{v.timer_new=true;cx.notify();})));
 }
 rows.push(hero.child(controls).into_any_element());
 if view.timer_adjust_open&&!workflow{
  let mut fields=div().flex().flex_col().gap(px(14.)).w_full().p(px(18.)).rounded(px(16.)).bg(t::surface()).border_1().border_color(t::hairline())
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child("调整用于新计时，不会改变正在运行的项目。"))
   .child(field_line("名称",300.,view.timer_name.clone()));
  if view.timer_mode=="countdown"{fields=fields.child(field_line("倒计时（秒，最多 24 小时）",250.,view.timer_duration.clone()));}
  else if view.timer_mode=="pomodoro"{fields=fields.child(div().flex().flex_wrap().gap(px(14.)).child(field_line("专注（秒）",140.,view.timer_focus.clone())).child(field_line("休息（秒）",140.,view.timer_rest.clone())).child(field_line("轮数",100.,view.timer_rounds.clone())));}
  rows.push(fields.child(div().flex().child(pill_button_icon("clock-apply".into(),Some("plus"),"使用这些设置新建",true,cx.listener(|v,_,_,cx|{v.timer_new=true;v.timer_adjust_open=false;cx.notify();})))).into_any_element());
 }
 if !clocks.is_empty(){
  let mut cards=Vec::new();
  for timer in &clocks{let id=timer["id"].as_str().unwrap_or("").to_owned();let(show,remove)=(id.clone(),id.clone());let mode=timer["mode"].as_str().unwrap_or("countdown").to_owned();let selected=Some(id.as_str())==current.and_then(|c|c["id"].as_str());let rev=timer["revision"].as_u64().unwrap_or(0);
   cards.push(div().id(SharedString::from(format!("timer-{id}"))).flex().items_center().gap(px(13.)).w_full().min_w_0().px(px(17.)).py(px(15.)).rounded(px(15.)).bg(t::surface()).border_1().border_color(if selected{t::hairline_bright()}else{t::hairline()})
    .child(div().w(px(4.)).h(px(36.)).flex_shrink_0().rounded(px(3.)).bg(if timer["running"]==true{t::accent()}else{t::surface_hi()}))
    .child(div().id(SharedString::from(format!("clock-select-{id}"))).flex().items_center().gap(px(13.)).flex_1().min_w_0().cursor_pointer()
     .child(div().flex().flex_col().gap_1().flex_1().min_w_0()
      .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(t::text()).min_w_0().child(timer["name"].as_str().unwrap_or("计时器").to_owned()))
      .child(div().text_size(px(12.)).text_color(t::text_dim()).min_w_0().child(timer["subtitle"].as_str().unwrap_or("").to_owned())))
     .child(minitime(timer["clock"].as_str().unwrap_or("00:00")))
     .on_click(cx.listener(move|v,_,_,cx|{v.timer_mode=mode.clone();v.timer_new=false;send_todo(&v.shared,"islandSelect",json!(format!("timer:{show}")),cx);cx.notify();})))
    .child(small_button(SharedString::from(format!("clock-delete-{id}")),"移除",cx.listener(move|v,_,_,cx|send_todo(&v.shared,"timerAction",json!({"id":remove,"action":"delete","revision":rev}),cx)))).into_any_element());
  }
  rows.push(section_block("我的计时",None,card_stack(cards),cx));
 }
 rows.push(plain_group(vec![info_row("与灵动岛同步","多个计时可同时运行","点选下方计时卡片即可显示在灵动岛。重启后保留进度并暂停。",view,cx)]));
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
  self.workflow_count=if stages.is_empty(){0}else{stages.len().clamp(2,12)};
  self.workflow_template=template;
  for(index,(name_field,minutes_field))in self.workflow_stages.iter().enumerate(){
   let(stage_name,minutes)=if index<self.workflow_count{stages.get(index).map(|stage|(stage["name"].as_str().unwrap_or("").to_owned(),(stage["seconds"].as_u64().unwrap_or(0)/60).max(1))).unwrap_or((String::new(),25))}else{(String::new(),25)};
   name_field.update(cx,|field,cx|field.set_text(&stage_name,cx));
   minutes_field.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));
  }
  self.workflow_selected=0;self.workflow_generation+=1;self.workflow_busy=false;self.workflow_pending=None;
  self.drawer=Some(Drawer::WorkflowEditor);
  cx.notify();
 }
 /// Copy a saved template into the editor's fields.
 fn load_workflow_template(&mut self,template:&Value,cx:&mut Context<Self>){
  let stages=template["stages"].as_array().cloned().unwrap_or_default();
  self.workflow_count=stages.len().clamp(2,12);self.workflow_selected=0;self.workflow_generation+=1;self.workflow_pending=None;
  self.workflow_template=template["id"].as_str().unwrap_or("").to_owned();
  self.workflow_name.update(cx,|field,cx|field.set_text(template["name"].as_str().unwrap_or(""),cx));
  for(index,(name_field,minutes_field))in self.workflow_stages.iter().enumerate(){
   let(stage_name,minutes)=if index<self.workflow_count{stages.get(index).map(|stage|(stage["name"].as_str().unwrap_or("").to_owned(),(stage["seconds"].as_u64().unwrap_or(0)/60).max(1))).unwrap_or((String::new(),25))}else{(String::new(),25)};
   name_field.update(cx,|field,cx|field.set_text(&stage_name,cx));
   minutes_field.update(cx,|field,cx|field.set_text(&minutes.to_string(),cx));
  }
  cx.notify();
 }
 fn workflow_stages_json(&self,cx:&Context<Self>)->Result<Value,String>{
  let mut stages=Vec::new();
  for(index,(name_field,minutes_field))in self.workflow_stages.iter().enumerate().take(self.workflow_count){
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
  let value=if template{json!({"id":self.workflow_template,"name":name,"stages":stages})}else{json!({"mode":"workflow","name":name,"stages":stages,"templateId":self.workflow_template})};
  let job=cx.background_executor().spawn(async move{pipe::command(command,value)});
  cx.spawn(async move|_,cx|{let result=job.await;let mut success=false;if let Ok(mut shared)=shared.lock(){match result{Ok((reply,state))=>{success=reply["success"]==true;shared.state=state;shared.message=if success{if template{"模板已保存".into()}else{String::new()}}else{reply["message"].as_str().unwrap_or("流程保存失败，草稿已保留").into()};},Err(e)=>shared.message=e}}
   let _=weak.update(cx,|view,cx|{view.workflow_busy=false;if success&&!template&&view.workflow_generation==generation&&view.workflow_stages_json(cx).ok()==Some(expected_stages)&&view.workflow_name.read(cx).text().trim()==expected_name{view.drawer=None;}cx.notify();});}).detach();cx.notify();
 }
 fn flow_drop(&mut self,payload:&FlowDrag,slot:usize,cx:&mut Context<Self>){
  if self.workflow_busy||payload.generation!=self.workflow_generation{return}
  if let Some(index)=payload.source{if index>=self.workflow_count{return}let stage=self.workflow_stages.remove(index);let slot=if slot>index{slot-1}else{slot}.min(self.workflow_count-1);self.workflow_stages.insert(slot,stage);self.workflow_selected=slot;}
  else{if self.workflow_count>=12{return}let minutes=match self.workflow_library[payload.kind].read(cx).text().trim().parse::<u64>(){Ok(m)if(1..=240).contains(&m)=>m,_=>{if let Ok(mut shared)=self.shared.lock(){shared.message="模块时长需要1–240分钟".into();}cx.notify();return}};
   let fields=self.workflow_stages.remove(self.workflow_count);fields.0.update(cx,|f,cx|f.set_text(FLOW_MODULES[payload.kind].0,cx));fields.1.update(cx,|f,cx|f.set_text(&minutes.to_string(),cx));let slot=slot.min(self.workflow_count);self.workflow_stages.insert(slot,fields);self.workflow_count+=1;self.workflow_selected=slot;
  }
  self.workflow_generation+=1;cx.notify();
 }
 fn flow_remove(&mut self,index:usize,cx:&mut Context<Self>){if self.workflow_busy||index>=self.workflow_count{return}let removed=self.workflow_stages.remove(index);self.workflow_stages.push(removed);self.workflow_count-=1;self.workflow_selected=self.workflow_selected.min(self.workflow_count.saturating_sub(1));self.workflow_generation+=1;cx.notify();}
 fn flow_duplicate(&mut self,cx:&mut Context<Self>){let index=self.workflow_selected;if self.workflow_busy||self.workflow_count>=12||index>=self.workflow_count{return}let name=self.workflow_stages[index].0.read(cx).text().to_owned();let minutes=self.workflow_stages[index].1.read(cx).text().to_owned();let fields=self.workflow_stages.remove(self.workflow_count);fields.0.update(cx,|f,cx|f.set_text(&name,cx));fields.1.update(cx,|f,cx|f.set_text(&minutes,cx));self.workflow_stages.insert(index+1,fields);self.workflow_count+=1;self.workflow_selected=index+1;self.workflow_generation+=1;cx.notify();}

}


const FLOW_MODULES:[(&str,&str,u32);6]=[("整理思路","list",0x9fac83),("专注","clock",0x86a875),("休息","pause",0xc2a487),("复盘","check",0xa398ba),("活动一下","play",0x83aaa4),("自定义","plus",0x9ba8b8)];
#[derive(Clone)]struct FlowDrag{source:Option<usize>,kind:usize,generation:u64,title:String}
impl Render for FlowDrag{fn render(&mut self,_:&mut Window,_:&mut Context<Self>)->impl IntoElement{div().w(px(220.)).p(px(16.)).rounded(px(14.)).bg(t::surface()).border_1().border_color(t::accent()).shadow_md().text_color(t::text()).child(self.title.clone())}}
fn flow_slot(index:usize,generation:u64,cx:&Context<SettingsApp>)->AnyElement{div().id(("flow-slot",index)).w_full().h(px(12.)).rounded(px(6.)).bg(t::surface_hi()).drag_over::<FlowDrag>(|style,_,_,_|style.bg(t::accent())).on_drop(cx.listener(move|view,payload:&FlowDrag,_,cx|{if view.workflow_generation==generation{view.flow_drop(payload,index,cx);}})).into_any_element()}
pub(crate) fn workflow_editor_rows(state:&Value,view:&SettingsApp,cx:&mut Context<SettingsApp>)->Vec<AnyElement>{
 let generation=view.workflow_generation;
 let mut library=div().id("flow-library-scroll").h(px(390.)).overflow_y_scroll().w(px(164.)).flex_shrink_0().flex().flex_col().gap(px(8.)).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("模块库"));
 for (index,(name,_,color))in FLOW_MODULES.iter().enumerate(){let payload=FlowDrag{source:None,kind:index,generation,title:name.to_string()};
  library=library.child(div().id(("flow-library",index)).p(px(10.)).rounded(px(12.)).bg(t::surface_hi()).flex().flex_col().gap(px(6.))
   .child(div().id(("flow-library-handle",index)).cursor_move().text_color(rgb(*color)).child(format!("⠿  {name}")).on_drag(payload,|p,_,_,cx|cx.new(|_|p.clone())))
   .child(div().flex().items_center().gap(px(6.)).child(div().w(px(60.)).child(view.workflow_library[index].clone())).child(div().text_size(px(11.)).child("分钟"))
    .child(small_button(SharedString::from(format!("flow-add-{index}")),"＋",cx.listener(move|v,_,_,cx|{let payload=FlowDrag{source:None,kind:index,generation:v.workflow_generation,title:String::new()};v.flow_drop(&payload,v.workflow_count,cx);})))));
 }
 let mut stages=div().id("flow-canvas-scroll").flex().flex_col().gap(px(4.)).h(px(350.)).overflow_y_scroll().track_scroll(&view.workflow_scroll);
 let scroll=view.workflow_scroll.clone();stages=stages.on_drag_move::<FlowDrag>(move|event,_,cx|{let y=event.event.position.y;let top=event.bounds.origin.y;let bottom=top+event.bounds.size.height;let mut offset=scroll.offset();if y<top+px(32.){offset.y+=px(9.);}else if y>bottom-px(32.){offset.y-=px(9.);}else{return}scroll.set_offset(offset);cx.refresh_windows();});
 for index in 0..view.workflow_count{let name=view.workflow_stages[index].0.read(cx).text().to_owned();let minutes=view.workflow_stages[index].1.read(cx).text().to_owned();let payload=FlowDrag{source:Some(index),kind:0,generation,title:name.clone()};
  stages=stages.child(flow_slot(index,generation,cx)).child(div().id(("flow-stage",index)).flex().items_center().gap(px(10.)).p(px(12.)).rounded(px(12.)).border_1().border_color(if view.workflow_selected==index{t::accent()}else{t::hairline()}).bg(t::surface_hi())
   .child(div().id(("flow-handle",index)).p(px(6.)).cursor_move().child("⠿").on_drag(payload,|p,_,_,cx|cx.new(|_|p.clone())))
   .child(div().flex_1().min_w_0().text_size(px(13.)).child(if name.is_empty(){format!("阶段 {}",index+1)}else{name}))
   .child(div().text_size(px(12.)).text_color(t::text_dim()).child(format!("{minutes} 分")))
   .drag_over::<FlowDrag>(|style,_,_,_|style.border_color(t::accent()))
   .on_drop(cx.listener(move|v,payload:&FlowDrag,_,cx|v.flow_drop(payload,index,cx)))
   .on_click(cx.listener(move|v,_,window,cx|{v.workflow_selected=index;window.focus(&v.workflow_focus,cx);cx.notify();})));
 }
 stages=stages.child(flow_slot(view.workflow_count,generation,cx));
 if view.workflow_count==0{stages=stages.child(div().p(px(22.)).text_size(px(12.)).text_color(t::text_dim()).child("从左侧拖入模块，或点击＋添加。每一段都能独立调整。"));}
 let total=view.workflow_stages.iter().take(view.workflow_count).filter_map(|(_,f)|f.read(cx).text().parse::<u64>().ok()).sum::<u64>();
 let canvas=div().id("flow-canvas").track_focus(&view.workflow_focus).on_key_down(cx.listener(|v,event:&KeyDownEvent,_,cx|{if !event.keystroke.modifiers.alt{return}let index=v.workflow_selected;let slot=match event.keystroke.key.as_str(){"up" if index>0=>index-1,"down" if index+1<v.workflow_count=>index+2,_=>return};let payload=FlowDrag{source:Some(index),kind:0,generation:v.workflow_generation,title:String::new()};v.flow_drop(&payload,slot,cx);cx.stop_propagation();})).flex_1().min_w(px(240.)).flex().flex_col().gap(px(12.)).child(div().text_size(px(13.)).child(format!("流程画布 · {} 段 · {} 分钟",view.workflow_count,total))).child(div().child(view.workflow_name.clone())).child(stages);
 let mut detail=div().w(px(184.)).flex_shrink_0().flex().flex_col().gap(px(12.)).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("阶段详情"));
 if let Some((name,minutes))=view.workflow_stages.get(view.workflow_selected).filter(|_|view.workflow_selected<view.workflow_count){let selected=view.workflow_selected;detail=detail.child(field_line("名称",180.,name.clone())).child(field_line("分钟（1–240）",180.,minutes.clone()))
  .child(small_button("flow-copy".into(),"复制阶段",cx.listener(|v,_,_,cx|v.flow_duplicate(cx))))
  .child(small_button("flow-delete".into(),"删除阶段",cx.listener(move|v,_,_,cx|v.flow_remove(selected,cx))))
  .child(div().flex().gap(px(6.)).child(small_button("flow-up".into(),"上移",cx.listener(move|v,_,_,cx|{if selected>0{let p=FlowDrag{source:Some(selected),kind:0,generation:v.workflow_generation,title:String::new()};v.flow_drop(&p,selected-1,cx);}}))).child(small_button("flow-down".into(),"下移",cx.listener(move|v,_,_,cx|{if selected+1<v.workflow_count{let p=FlowDrag{source:Some(selected),kind:0,generation:v.workflow_generation,title:String::new()};v.flow_drop(&p,selected+2,cx);}}))))
  .child(div().text_size(px(11.)).text_color(t::text_dim()).child("修改只影响这一段；阶段结束等待点击下一阶段。"));}
 let mut rows=vec![div().flex().gap(px(20.)).text_color(t::text()).child(library).child(canvas).child(detail).into_any_element()];
 rows.push(div().flex().items_center().justify_between().gap(px(12.)).child(div().text_size(px(11.)).text_color(t::text_dim()).child("2–12段 · 总时长≤24小时 · 拖入复制模块，拖动⠿排序"))
  .child(div().flex().gap(px(8.)).child(pill_button("flow-template-save".into(),if view.workflow_busy{"处理中…"}else{"保存为模板"},false,cx.listener(|v,_,_,cx|v.commit_flow(true,cx))))
   .child(pill_button("flow-submit".into(),"创建并开始",true,cx.listener(|v,_,_,cx|v.submit_workflow(cx))))).into_any_element());
 if let Some(error)=state["workflowTemplateError"].as_str().filter(|s|!s.is_empty()){rows.push(info_row("模板文件异常",error,"请修复文件后再保存；原文件保留。",view,cx));}
 if let Some(template)=view.workflow_pending.clone(){rows.push(div().flex().items_center().gap(px(10.)).child(div().flex_1().child("载入模板将替换当前草稿，正在运行的流程不受影响。"))
  .child(small_button("flow-confirm-load".into(),"替换草稿",cx.listener(move|v,_,_,cx|v.load_workflow_template(&template,cx))))
  .child(small_button("flow-cancel-load".into(),"保留草稿",cx.listener(|v,_,_,cx|{v.workflow_pending=None;cx.notify();}))).into_any_element());}
 let templates=state["templates"].as_array().cloned().unwrap_or_default();
 for template in templates{let payload=template.clone();let id=template["id"].as_str().unwrap_or("").to_owned();rows.push(div().flex().items_center().gap(px(12.)).child(div().flex_1().child(template["name"].as_str().unwrap_or("流程模板").to_owned())).child(small_button(SharedString::from(format!("load-{id}")),"载入副本",cx.listener(move|v,_,_,cx|{v.workflow_pending=Some(payload.clone());cx.notify();}))).child(small_button(SharedString::from(format!("remove-{id}")),"删除",cx.listener(move|v,_,_,cx|send_todo(&v.shared,"workflowTemplateDelete",json!(id),cx)))).into_any_element());}
 rows
}
