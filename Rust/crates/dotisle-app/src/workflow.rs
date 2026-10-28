//! 流程 timer UI: stage drafts, the module library, saved templates and the waiting panel.
//! Editing is pure application state; the native process still owns clocks and persistence.
use serde_json::{Value,json};
use vello::kurbo::{Affine,BezPath,Stroke};
use crate::{draw::{self,Icon,rect},model::{Model,Action,Dialog,DialogKind,Field},widgets::Ui};

pub const STAGE_MIN:usize=2;
pub const STAGE_MAX:usize=12;
pub const STAGE_MAX_MINUTES:u32=240;
pub const STAGE_STEP:u32=5;
const TOTAL_MAX_MINUTES:u32=24*60;
const TEMPLATE_MAX:usize=16;

/// One editable stage of the draft flow. Persisted timers store seconds; the draft edits minutes.
#[derive(Clone,Debug,PartialEq)]pub struct DraftStage{pub name:String,pub minutes:u32}
#[derive(Clone,Debug,PartialEq)]pub struct FlowTemplate{pub id:String,pub name:String,pub stages:Vec<DraftStage>}
/// Everything the 流程 editor needs that is not already owned by the domain timer store.
#[derive(Clone,Debug)]pub struct WorkflowState{pub stages:Vec<DraftStage>,pub template_id:Option<String>,pub template_name:String,pub templates:Vec<FlowTemplate>}
impl Default for WorkflowState{fn default()->Self{Self{stages:default_stages(),template_id:None,template_name:String::new(),templates:Vec::new()}}}
pub fn default_stages()->Vec<DraftStage>{vec![DraftStage{name:"专注".into(),minutes:25},DraftStage{name:"休息".into(),minutes:5},DraftStage{name:"复盘".into(),minutes:10}]}
#[derive(Clone,Debug,PartialEq)]pub struct WorkflowModule{pub id:String,pub name:String,pub minutes:u32,pub kind:u8}

/// The module library is native-user data. A missing key means a pre-library install, so the six
/// built-ins are shown; an explicitly empty library stays empty.
pub fn modules(m:&Model)->Vec<WorkflowModule>{
    let Some(list)=m.native_state["settings"]["WorkflowModules"].as_array()else{return builtin_modules()};
    let mut out=Vec::with_capacity(list.len());
    for item in list{
        let(Some(id),Some(name),Some(minutes))=(item["id"].as_str(),item["name"].as_str(),item["minutes"].as_u64())else{continue};
        if id.is_empty()||name.is_empty(){continue}
        out.push(WorkflowModule{id:id.into(),name:name.into(),minutes:minutes.clamp(1,STAGE_MAX_MINUTES as u64)as u32,kind:item["kind"].as_u64().unwrap_or(5).min(5)as u8});
    }
    out
}
fn builtin_modules()->Vec<WorkflowModule>{
    [("builtin-thinking","整理思路",5,0),("builtin-focus","专注",25,1),("builtin-rest","休息",5,2),("builtin-review","复盘",10,3),("builtin-move","活动一下",3,4),("builtin-custom","自定义",10,5)]
        .iter().map(|(id,name,minutes,kind)|WorkflowModule{id:(*id).into(),name:(*name).into(),minutes:*minutes,kind:*kind}).collect()
}
pub fn total_minutes(stages:&[DraftStage])->u32{stages.iter().map(|s|s.minutes).sum()}
pub fn summary(stages:&[DraftStage])->String{format!("共 {} 分钟 · {} 个阶段",total_minutes(stages),stages.len())}
pub fn draft_clock(m:&Model)->String{format_time(total_minutes(&m.workflow.stages)as u64*60)}
fn format_time(seconds:u64)->String{const H:u64=3600;if seconds>=H{format!("{:02}:{:02}:{:02}",seconds/H,seconds/60%60,seconds%60)}else{format!("{:02}:{:02}",seconds/60,seconds%60)}}

/// Normalise the draft into the stages payload shared by `timerCreate` and template saves.
pub fn stage_payload(stages:&[DraftStage])->Result<Value,String>{
    if !(STAGE_MIN..=STAGE_MAX).contains(&stages.len()){return Err(format!("流程需要 {} 到 {} 个阶段",STAGE_MIN,STAGE_MAX))}
    let mut out=Vec::with_capacity(stages.len());let mut total=0u32;
    for(index,stage)in stages.iter().enumerate(){
        if !(1..=STAGE_MAX_MINUTES).contains(&stage.minutes){return Err(format!("第 {} 个阶段需要 1–240 分钟",index+1))}
        total+=stage.minutes;
        let name=stage.name.trim();
        if name.chars().count()>40||name.chars().any(char::is_control){return Err("阶段名称不能超过 40 字".into())}
        out.push(json!({"name":if name.is_empty(){format!("阶段 {}",index+1)}else{name.to_owned()},"seconds":stage.minutes as u64*60}));
    }
    if total>TOTAL_MAX_MINUTES{return Err("流程总时长不能超过 24 小时".into())}
    Ok(json!(out))
}
/// Payload for `timerCreate` in workflow mode.
pub fn start_payload(m:&Model)->Result<Value,String>{
    let stages=stage_payload(&m.workflow.stages)?;
    let name: String=if m.workflow.template_name.trim().is_empty(){"流程编排".into()}else{m.workflow.template_name.trim().chars().take(60).collect()};
    let mut payload=json!({"mode":"workflow","name":name,"stages":stages});
    if let Some(id)=&m.workflow.template_id{payload["templateId"]=json!(id);}
    Ok(payload)
}

pub fn add_stage(w:&mut WorkflowState,stage:DraftStage)->Result<(),String>{
    if w.stages.len()>=STAGE_MAX{return Err(format!("流程最多 {} 个阶段",STAGE_MAX))}
    if total_minutes(&w.stages)+stage.minutes.clamp(1,STAGE_MAX_MINUTES)>TOTAL_MAX_MINUTES{return Err("流程总时长不能超过 24 小时".into())}
    w.stages.push(DraftStage{name:stage.name,minutes:stage.minutes.clamp(1,STAGE_MAX_MINUTES)});Ok(())
}
pub fn delete_stage(w:&mut WorkflowState,index:usize)->Result<(),String>{
    if index>=w.stages.len(){return Err("阶段不存在".into())}
    if w.stages.len()<=STAGE_MIN{return Err(format!("流程至少需要 {} 个阶段",STAGE_MIN))}
    w.stages.remove(index);Ok(())
}
pub fn move_stage(w:&mut WorkflowState,index:usize,delta:i32)->Result<(),String>{
    if index>=w.stages.len(){return Err("阶段不存在".into())}
    let target=index as i32+delta;
    if target<0||target>=w.stages.len()as i32{return Err(if delta<0{"已经是第一个阶段".into()}else{"已经是最后一个阶段".to_string()})}
    w.stages.swap(index,target as usize);Ok(())
}
pub fn nudge_stage(w:&mut WorkflowState,index:usize,delta:i32)->Result<(),String>{
    let Some(stage)=w.stages.get(index)else{return Err("阶段不存在".into())};
    let next=(stage.minutes as i32+delta).clamp(1,STAGE_MAX_MINUTES as i32)as u32;
    if next==stage.minutes{return Ok(())}
    let others:u32=w.stages.iter().enumerate().filter(|(i,_)|*i!=index).map(|(_,s)|s.minutes).sum();
    if others+next>TOTAL_MAX_MINUTES{return Err("流程总时长不能超过 24 小时".into())}
    w.stages[index].minutes=next;Ok(())
}
/// Apply the custom-stage dialog. `index` edits an existing stage, `None` appends a new one.
pub fn apply_stage(w:&mut WorkflowState,index:Option<usize>,name:&str,minutes:&str)->Result<(),String>{
    let minutes:u32=minutes.trim().parse().map_err(|_|"分钟需要 1–240 的数字".to_string())?;
    if !(1..=STAGE_MAX_MINUTES).contains(&minutes){return Err("分钟需要 1–240".into())}
    let name=name.trim();
    if name.chars().count()>40||name.chars().any(char::is_control){return Err("阶段名称不能超过 40 字".into())}
    let other:u32=w.stages.iter().enumerate().filter(|(i,_)|Some(*i)!=index).map(|(_,s)|s.minutes).sum();
    if other+minutes>TOTAL_MAX_MINUTES{return Err("流程总时长不能超过 24 小时".into())}
    match index{
        Some(i)if i<w.stages.len()=>{w.stages[i].name=if name.is_empty(){format!("阶段 {}",i+1)}else{name.into()};w.stages[i].minutes=minutes;}
        _=>{if w.stages.len()>=STAGE_MAX{return Err(format!("流程最多 {} 个阶段",STAGE_MAX))}let position=w.stages.len();w.stages.push(DraftStage{name:if name.is_empty(){format!("阶段 {}",position+1)}else{name.into()},minutes});}
    }
    Ok(())
}

/// Save the draft into the local template cache (demo/preview path; native owns it in production).
pub fn save_template_local(w:&mut WorkflowState,name:&str,overwrite:bool)->Result<(),String>{
    let name=name.trim();
    if name.is_empty(){return Err("请填写模板名称".into())}
    if name.chars().count()>60||name.chars().any(char::is_control){return Err("模板名称不能超过 60 字".into())}
    stage_payload(&w.stages)?;
    if overwrite{
        if let Some(id)=&w.template_id{
            if let Some(template)=w.templates.iter_mut().find(|t|&t.id==id){template.name=name.into();template.stages=w.stages.clone();w.template_name=name.into();return Ok(())}
        }
    }
    if w.templates.len()>=TEMPLATE_MAX{return Err(format!("最多保存 {} 个流程模板",TEMPLATE_MAX))}
    let id=format!("flow-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    w.templates.push(FlowTemplate{id:id.clone(),name:name.into(),stages:w.stages.clone()});w.template_id=Some(id);w.template_name=name.into();Ok(())
}
/// Payload for `workflowTemplateSave`; `overwrite` keeps the loaded template id so the native
/// process updates that entry instead of appending another one.
pub fn template_payload(w:&WorkflowState,name:&str,overwrite:bool)->Result<Value,String>{
    let name=name.trim();
    if name.is_empty(){return Err("请填写模板名称".into())}
    if name.chars().count()>60||name.chars().any(char::is_control){return Err("模板名称不能超过 60 字".into())}
    let stages=stage_payload(&w.stages)?;
    let mut payload=json!({"name":name,"stages":stages});
    if overwrite{if let Some(id)=&w.template_id{payload["id"]=json!(id);}}
    Ok(payload)
}
pub fn delete_template_local(w:&mut WorkflowState,id:&str){w.templates.retain(|t|t.id!=id);if w.template_id.as_deref()==Some(id){w.template_id=None;w.template_name.clear();}}
/// Load a saved template as an editable copy; saving keeps asking overwrite-or-copy.
pub fn load_template(w:&mut WorkflowState,id:&str)->Result<String,String>{
    let Some(template)=w.templates.iter().find(|t|t.id==id)else{return Err("流程模板不存在".into())};
    w.stages=template.stages.clone();w.template_id=Some(template.id.clone());w.template_name=template.name.clone();Ok(template.name.clone())
}
/// A running workflow becomes a new draft starting at the current stage (remaining minutes first).
pub fn copy_stages_from_timer(m:&Model,id:&str)->Option<(String,Vec<DraftStage>)>{
    let timer=m.timers.items.iter().find(|t|t.id==id)?;
    let workflow=timer.workflow.as_ref()?;
    let mut stages=Vec::new();
    for(index,stage)in workflow.stages.iter().enumerate().skip(workflow.stage_index){
        let minutes=if index==workflow.stage_index{timer.remaining_ms().div_ceil(60_000).max(1)as u32}else{stage.seconds.div_ceil(60).max(1)as u32};
        stages.push(DraftStage{name:stage.name.clone(),minutes:minutes.min(STAGE_MAX_MINUTES)});
    }
    if stages.len()<STAGE_MIN{stages.push(DraftStage{name:"休息".into(),minutes:5});}
    Some((timer.name.clone(),stages))
}
/// Pull `templates` from the native state packet so the dialog and editor always see the owner copy.
pub fn sync(m:&mut Model){
    let Some(list)=m.native_state["templates"].as_array()else{return};
    let mut out=Vec::with_capacity(list.len());
    for item in list{
        let(Some(id),Some(name),Some(stages))=(item["id"].as_str(),item["name"].as_str(),item["stages"].as_array())else{continue};
        let stages=stages.iter().map(|s|DraftStage{name:s["name"].as_str().unwrap_or("阶段").to_owned(),minutes:s["seconds"].as_u64().unwrap_or(60).div_ceil(60).max(1)as u32}).collect();
        out.push(FlowTemplate{id:id.into(),name:name.into(),stages});
    }
    m.workflow.templates=out;
}

/// Draft, dialog and command glue used by `Model::dispatch`; remote mode still sends native commands.
impl Model{
    pub fn start_workflow(&mut self){
        match start_payload(self){
            Ok(payload)=>if self.remote.is_some(){self.native_command("timerCreate",payload);}
                else{match self.timers.create(&payload){Ok(id)=>{self.selected=Some(id);self.message("已开始流程");},Err(e)=>self.message(e)}},
            Err(e)=>self.message(e),
        }
    }
    pub fn workflow_add_module(&mut self,index:usize){
        let library=modules(self);let Some(module)=library.get(index)else{return};
        match add_stage(&mut self.workflow,DraftStage{name:module.name.clone(),minutes:module.minutes}){
            Ok(())=>self.message(format!("已加入「{}」",module.name)),
            Err(e)=>self.message(e),
        }
    }
    pub fn workflow_open_stage(&mut self,index:Option<usize>){
        let(kind,fields):(DialogKind,Vec<Field>)=match index.and_then(|i|self.workflow.stages.get(i as usize).map(|s|(i,s.clone()))){
            Some((i,stage))=>{let minutes=stage.minutes.to_string();(DialogKind::WorkflowStage(Some(i as u32)),vec![Field::new("阶段名称",stage.name,40),Field::new("分钟 · 1–240",minutes,3)])}
            None=>(DialogKind::WorkflowStage(None),vec![Field::new("阶段名称 · 留空自动命名",String::new(),40),Field::new("分钟 · 1–240","25".into(),3)]),
        };
        self.dialog=Some(Dialog{kind,fields,active:0,daily:false,error:String::new(),preedit:String::new()});
    }
    pub fn workflow_delete_stage(&mut self,index:usize){match delete_stage(&mut self.workflow,index){Ok(())=>self.message("阶段已移除"),Err(e)=>self.message(e)}}
    pub fn workflow_move_stage(&mut self,index:usize,delta:i8){if let Err(e)=move_stage(&mut self.workflow,index,delta as i32){self.message(e)}}
    pub fn workflow_nudge_stage(&mut self,index:usize,delta:i32){if let Err(e)=nudge_stage(&mut self.workflow,index,delta){self.message(e)}}
    pub fn workflow_open_save(&mut self){
        let name=if self.workflow.template_name.trim().is_empty(){"我的流程".to_string()}else{self.workflow.template_name.clone()};
        self.dialog=Some(Dialog{kind:DialogKind::WorkflowTemplate(self.workflow.template_id.clone()),fields:vec![Field::new("模板名称",name,60)],active:0,daily:false,error:String::new(),preedit:String::new()});
    }
    pub fn workflow_open_templates(&mut self){self.dialog=Some(Dialog{kind:DialogKind::WorkflowTemplates,fields:vec![],active:0,daily:false,error:String::new(),preedit:String::new()});}
    pub fn workflow_load_template(&mut self,id:&str){match load_template(&mut self.workflow,id){Ok(name)=>{self.dialog=None;self.message(format!("已加载模板「{name}」，保存时可覆盖或另存"));},Err(e)=>self.message(e)}}
    pub fn workflow_delete_template(&mut self,id:&str){
        if self.remote.is_some(){self.native_command("workflowTemplateDelete",json!(id));self.workflow.templates.retain(|t|t.id!=id);}
        else{delete_template_local(&mut self.workflow,id);}
        self.message("模板已删除");
    }
    pub fn workflow_draft_reset(&mut self){
        let templates=std::mem::take(&mut self.workflow.templates);
        self.workflow=WorkflowState{stages:default_stages(),template_id:None,template_name:String::new(),templates};self.message("已重置编排");
    }
    pub fn workflow_edit_copy(&mut self,id:&str){
        if let Some((name,stages))=copy_stages_from_timer(self,id){
            let templates=std::mem::take(&mut self.workflow.templates);
            self.workflow=WorkflowState{stages,template_id:None,template_name:String::new(),templates};
            self.selected=None;self.mode=3;self.message(format!("已从「{name}」复制当前阶段，可继续编排"));
        }else{self.message("这个计时器没有可复制的流程")}
    }
    pub fn submit_stage_dialog(&mut self)->Result<(),String>{
        let d=self.dialog.clone().ok_or("阶段对话框已关闭")?;
        let DialogKind::WorkflowStage(index)=d.kind else{return Err("阶段对话框已关闭".into())};
        apply_stage(&mut self.workflow,index.map(|i|i as usize),&d.fields[0].value,&d.fields[1].value)
    }
    pub fn submit_template_dialog(&mut self)->Result<(),String>{
        let d=self.dialog.clone().ok_or("模板对话框已关闭")?;
        let DialogKind::WorkflowTemplate(id)=d.kind else{return Err("模板对话框已关闭".into())};
        let overwrite=d.daily&&id.is_some();let name=d.fields[0].value.trim().to_owned();
        if self.remote.is_some(){
            let payload=template_payload(&self.workflow,&name,overwrite)?;
            self.workflow.template_name=name;self.native_command("workflowTemplateSave",payload);Ok(())
        }else{save_template_local(&mut self.workflow,&name,overwrite)}
    }
}

// ---------------------------------------------------------------- rendering

const ROW_H:f64=64.;

fn row_icon(ui:&mut Ui,r:vello::kurbo::Rect,kind:Icon,label:&'static str,action:Action,enabled:bool,fg:u32){
    let index=ui.hit(r,label,action,enabled);if enabled{ui.state(r,10.,index,fg);}
    draw::icon(ui.scene,kind,r.center().x-9.,r.center().y-9.,18.,if enabled{fg}else{ui.s.outline});
}
fn row_step(ui:&mut Ui,r:vello::kurbo::Rect,plus:bool,label:&'static str,action:Action,enabled:bool,fg:u32){
    let index=ui.hit(r,label,action,enabled);if enabled{ui.state(r,10.,index,fg);}
    let color=if enabled{fg}else{ui.s.outline};let(cx,cy)=(r.center().x,r.center().y);
    if plus{draw::icon(ui.scene,Icon::Plus,cx-8.,cy-8.,16.,color);}
    else{draw::line(ui.scene,(cx-6.,cy),(cx+6.,cy),color,2.);}
}
fn row_chevron(ui:&mut Ui,r:vello::kurbo::Rect,up:bool,label:&'static str,action:Action,enabled:bool,fg:u32){
    let index=ui.hit(r,label,action,enabled);if enabled{ui.state(r,8.,index,fg);}
    let color=if enabled{fg}else{ui.s.outline};let(cx,cy)=(r.center().x,r.center().y);let mut path=BezPath::new();
    if up{path.move_to((cx-4.5,cy+3.));path.line_to((cx,cy-3.));path.line_to((cx+4.5,cy+3.));}else{path.move_to((cx-4.5,cy-3.));path.line_to((cx,cy+3.));path.line_to((cx+4.5,cy-3.));}
    ui.scene.stroke(&Stroke::new(2.).with_caps(vello::kurbo::Cap::Round),Affine::IDENTITY,draw::color(color),None,&path);
}
fn hero_button(ui:&mut Ui,r:vello::kurbo::Rect,label:&str,icon:Option<Icon>,action:Action,primary:bool,enabled:bool){
    let s=ui.s;let i=ui.hit(r,label,action,enabled);let p=if enabled&&ui.pressed==i{1.}else{0.};let rr=r.inset((r.width()*0.015*p,r.height()*0.015*p));
    let(bg,fg)=if !enabled{(s.high,s.on_surface_variant)}else if primary{(s.primary,s.on_primary)}else{(s.surface,s.on_surface)};
    draw::fill(ui.scene,rr,rr.height()/2.-8.*p,bg);if enabled{ui.state(rr,rr.height()/2.,i,fg);}
    if let Some(icon)=icon{draw::icon(ui.scene,icon,rr.x0+16.,rr.center().y-9.,18.,fg);ui.center(label,vello::kurbo::Rect::new(rr.x0+32.,rr.y0,rr.x1-8.,rr.y1),13.5,fg,true);}
    else{ui.center(label,rr,13.5,fg,true);}
}
fn quick_chip(ui:&mut Ui,r:vello::kurbo::Rect,label:&str,action:Action,enabled:bool){
    let s=ui.s;draw::fill(ui.scene,r,18.,if enabled{s.container}else{s.high});
    let index=ui.hit(r,label,action,enabled);if enabled{ui.state(r,18.,index,s.on_surface);}
    draw::icon(ui.scene,Icon::Plus,r.x0+13.,r.center().y-8.,16.,if enabled{s.primary}else{s.outline});
    ui.center(label,vello::kurbo::Rect::new(r.x0+34.,r.y0,r.x1-8.,r.y1),12.5,if enabled{s.on_surface}else{s.on_surface_variant},false);
}

/// Full-width flow workspace. Planning is one vertical timeline: a summary hero, then one
/// row per stage, then quick-add modules. Everything the old right column did stays reachable.
pub fn hero_full(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
    if let Some(timer)=m.selected_timer().filter(|t|t.mode=="workflow"){running_full(ui,m,timer,x,y,w)}else{editor_full(ui,m,x,y,w)}
}

fn editor_full(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
    let s=ui.s;let stages=&m.workflow.stages;let library=modules(m);let wide=w>=860.;
    let hero_h=if wide{132.}else{196.};
    ui.panel(rect(x,y,w,hero_h),s.primary_container);
    ui.label("总时长",x+32.,y+18.,12.,s.on_primary_container,true,w-560.);
    ui.label(&draft_clock(m),x+32.,y+36.,if wide{46.}else{42.},s.on_primary_container,true,260.);
    let base=summary(stages);
    let label=if m.workflow.template_name.trim().is_empty(){base}else{format!("{} · 模板「{}」",base,m.workflow.template_name)};
    ui.label(&label,x+34.,y+if wide{96.}else{82.},12.,s.on_primary_container,false,w-72.);
    if wide{
        let cy=y+hero_h/2.-22.;let gap=8.;
        let start=136.;let add=108.;let save=108.;let lib=96.;let reset=44.;
        let mut rx=x+w-28.-reset;
        ui.icon_button(rect(rx,cy,reset,44.),Icon::Reset,"重置编排",Action::WorkflowDraftReset,false,true);
        rx-=gap+start;hero_button(ui,rect(rx,cy,start,44.),"开始流程",Some(Icon::Play),Action::StartTimer,true,stages.len()>=STAGE_MIN);
        rx-=gap+add;hero_button(ui,rect(rx,cy,add,44.),"添加阶段",Some(Icon::Plus),Action::WorkflowStageNew,false,stages.len()<STAGE_MAX);
        rx-=gap+save;hero_button(ui,rect(rx,cy,save,44.),"保存模板",None,Action::WorkflowSaveTemplate,false,true);
        rx-=gap+lib;hero_button(ui,rect(rx,cy,lib,44.),"模板库",None,Action::WorkflowTemplates,false,true);
    }else{
        let cy=y+hero_h-66.;let gap=8.;let start=(w*0.30).clamp(142.,190.);
        let bw=(w-56.-start-4.*gap)/4.;
        hero_button(ui,rect(x+28.,cy,start,44.),"开始流程",Some(Icon::Play),Action::StartTimer,true,stages.len()>=STAGE_MIN);
        hero_button(ui,rect(x+28.+start+gap,cy,bw,44.),"添加阶段",None,Action::WorkflowStageNew,false,stages.len()<STAGE_MAX);
        hero_button(ui,rect(x+28.+start+2.*gap+bw,cy,bw,44.),"保存模板",None,Action::WorkflowSaveTemplate,false,true);
        hero_button(ui,rect(x+28.+start+3.*gap+2.*bw,cy,bw,44.),"模板库",None,Action::WorkflowTemplates,false,true);
        ui.icon_button(rect(x+w-28.-44.,cy,44.,44.),Icon::Reset,"重置编排",Action::WorkflowDraftReset,false,true);
    }
    let mut cy=y+hero_h+18.;
    ui.label("流程阶段",x+4.,cy,16.,s.on_surface,true,120.);
    ui.label(&format!("{} / {}",stages.len(),STAGE_MAX),x+88.,cy+2.,12.,s.on_surface_variant,false,86.);
    ui.button(rect(x+w-152.,cy-8.,152.,38.),"管理模块库",Some(Icon::Settings),Action::WorkflowModules,true,true);
    ui.label("点阶段名称可改名称与时长",x+186.,cy+2.,11.,s.on_surface_variant,false,w-350.);
    cy+=30.;
    for(index,stage)in stages.iter().enumerate(){stage_row_full(ui,index,stage,stages.len(),x,cy,w);cy+=ROW_H+8.;}
    cy+=6.;
    ui.label("快速添加",x+4.,cy,15.,s.on_surface,true,100.);
    ui.label("点一下追加到流程末尾",x+82.,cy+2.,11.,s.on_surface_variant,false,180.);
    cy+=26.;
    let cols=if w>=880.{4}else{3};let gap=8.;let cw=(w-gap*(cols as f64-1.))/cols as f64;
    for(index,module)in library.iter().enumerate(){
        let cx=x+(index%cols)as f64*(cw+gap);let row_y=cy+(index/cols)as f64*44.;
        quick_chip(ui,rect(cx,row_y,cw,36.),&format!("{} · {} 分",module.name,module.minutes),Action::WorkflowAddModule(index),stages.len()<STAGE_MAX);
    }
    let custom=library.len();let cx=x+(custom%cols)as f64*(cw+gap);let row_y=cy+(custom/cols)as f64*44.;
    quick_chip(ui,rect(cx,row_y,cw,36.),"自定义阶段",Action::WorkflowStageNew,stages.len()<STAGE_MAX);
    cy+=(library.len()+1).div_ceil(cols)as f64*44.+6.;
    cy
}

fn stage_row_full(ui:&mut Ui,index:usize,stage:&DraftStage,count:usize,x:f64,y:f64,w:f64){
    let s=ui.s;let h=ROW_H;draw::fill(ui.scene,rect(x,y,w,h),20.,s.surface);
    draw::circle(ui.scene,x+38.,y+32.,18.,s.secondary_container);
    ui.center(&(index+1).to_string(),rect(x+20.,y+14.,36.,36.),13.,s.on_secondary_container,true);
    if index+1<count{draw::line(ui.scene,(x+38.,y+52.),(x+38.,y+h+8.),s.outline,1.4);}
    let name=if stage.name.trim().is_empty(){format!("阶段 {}",index+1)}else{stage.name.clone()};
    let edit_width=(w-296.-68.-12.).max(120.);
    let edit=ui.hit(rect(x+60.,y+4.,edit_width+56.,56.),"编辑阶段",Action::WorkflowStageEdit(index),true);
    ui.state(rect(x+60.,y+4.,edit_width+56.,56.),14.,edit,s.on_surface);
    ui.label(&name,x+68.,y+11.,16.,s.on_surface,true,edit_width);
    ui.label(&format!("{} 分钟",stage.minutes),x+68.,y+37.,12.,s.on_surface_variant,false,edit_width);
    let pill=rect(x+w-296.,y+14.,176.,36.);draw::fill(ui.scene,pill,18.,s.high);
    row_step(ui,rect(pill.x0+4.,y+16.,32.,32.),false,"减少 5 分钟",Action::WorkflowStageNudge(index,-(STAGE_STEP as i32)),stage.minutes>1,s.on_surface);
    ui.center(&format!("{} 分钟",stage.minutes),rect(pill.x0+38.,y+16.,100.,32.),12.5,s.on_surface,true);
    row_step(ui,rect(pill.x1-36.,y+16.,32.,32.),true,"增加 5 分钟",Action::WorkflowStageNudge(index,STAGE_STEP as i32),stage.minutes<STAGE_MAX_MINUTES,s.on_surface);
    row_chevron(ui,rect(x+w-108.,y+8.,28.,24.),true,"上移阶段",Action::WorkflowStageMove(index,-1),index>0,s.on_surface_variant);
    row_icon(ui,rect(x+w-44.,y+16.,32.,32.),Icon::Trash,"删除阶段",Action::WorkflowStageDelete(index),count>STAGE_MIN,s.on_surface_variant);
    // The lower chevron is redrawn as a down arrow; the upper one is the up arrow.
    row_chevron(ui,rect(x+w-108.,y+32.,28.,24.),false,"下移阶段",Action::WorkflowStageMove(index,1),index+1<count,s.on_surface_variant);
}

fn running_full(ui:&mut Ui,m:&Model,timer:&dotisle_domain::timers::Timer,x:f64,y:f64,w:f64)->f64{
    let s=ui.s;let workflow=timer.workflow.as_ref();
    let count=workflow.map(|w|w.stages.len()).unwrap_or(0);let index=workflow.map(|w|w.stage_index).unwrap_or(0);
    let waiting=workflow.is_some_and(|w|w.waiting);let stage=workflow.and_then(|w|w.stages.get(w.stage_index));
    let h=392.;
    ui.panel(rect(x,y,w,h),s.primary_container);
    ui.center("流程进行中",rect(x+24.,y+20.,w-48.,24.),13.,s.on_primary_container,true);
    ui.center(stage.map(|s|s.name.as_str()).unwrap_or("阶段"),rect(x+24.,y+48.,w-48.,40.),30.,s.on_primary_container,true);
    ui.center(&format_time(timer.remaining_ms().div_ceil(1000)),rect(x+24.,y+94.,w-48.,80.),64.,s.on_primary_container,true);
    ui.center(&format!("第 {} / {} 阶段 · 已过 {}",index+1,count,format_time(timer.elapsed_ms/1000)),rect(x+24.,y+178.,w-48.,22.),13.,s.on_primary_container,false);
    let track=rect(x+40.,y+210.,w-80.,8.);draw::fill(ui.scene,track,4.,s.secondary_container);
    draw::fill(ui.scene,rect(track.x0,track.y0,(track.width()*timer.progress()).max(0.001),track.height()),4.,s.primary);
    let banner=rect(x+40.,y+232.,w-80.,44.);draw::fill(ui.scene,banner,22.,s.secondary_container);
    let text=if timer.finished{"流程已结束，可以重新开始或编辑副本。"}else if waiting{"本阶段完成，准备进入下一阶段。"}else if timer.running{"正在专注，阶段完成后由你点「下一阶段」。"}else{"已暂停，继续后重新开始计时。"};
    ui.center(text,banner,12.5,s.on_secondary_container,true);
    let active=!timer.finished;let top=y+292.;let gap=8.;let bw=(w-80.-gap)/2.;
    ui.button(rect(x+40.,top,bw,44.),match timer.primary_action(){"pause"=>"暂停","resume"=>"继续","next"=>"进入下一阶段",_=>"重新开始"},Some(if timer.running{Icon::Pause}else{Icon::Play}),Action::Timer(timer.id.clone(),timer.primary_action()),false,active);
    hero_button(ui,rect(x+40.+bw+gap,top,bw,44.),"跳过本阶段",None,Action::Timer(timer.id.clone(),"skip"),false,active);
    let top2=top+52.;let bw4=(w-80.-3.*gap)/4.;
    hero_button(ui,rect(x+40.,top2,bw4,44.),"延长 5 分钟",None,Action::Timer(timer.id.clone(),"extend5"),false,active);
    hero_button(ui,rect(x+40.+bw4+gap,top2,bw4,44.),"结束流程",None,Action::Timer(timer.id.clone(),"end"),false,active);
    hero_button(ui,rect(x+40.+2.*(bw4+gap),top2,bw4,44.),"编辑副本",None,Action::WorkflowEditCopy(timer.id.clone()),false,true);
    hero_button(ui,rect(x+40.+3.*(bw4+gap),top2,bw4,44.),"回到编排",None,Action::Mode(3),false,true);
    let _=m;h
}

/// Compact one-line state for the dial card under the big clock; the full list lives in the panel.
pub fn run_line(timer:&dotisle_domain::timers::Timer)->String{
    if timer.finished{return "流程已完成".into()}
    let workflow=timer.workflow.as_ref();
    let(index,count,waiting)=workflow.map(|w|(w.stage_index,w.stages.len(),w.waiting)).unwrap_or((0,0,false));
    if waiting{return format!("第 {} / {} 阶段 · 准备下一阶段",index+1,count)}
    let name=workflow.and_then(|w|w.stages.get(w.stage_index)).map(|s|s.name.as_str()).unwrap_or("阶段");
    if timer.running{format!("第 {} / {} 阶段 · {name}",index+1,count)}else{format!("第 {} / {} 阶段 · {name} · 已暂停",index+1,count)}
}
pub fn draft_summary(m:&Model)->String{
    let base=summary(&m.workflow.stages);
    if m.workflow.template_name.trim().is_empty(){base}else{format!("{base} · 模板「{}」",m.workflow.template_name)}
}

#[cfg(test)]
mod tests{
    use super::*;
    use std::path::PathBuf;
    fn temp(tag:&str)->PathBuf{
        let path=std::env::temp_dir().join(format!("DotIsle.WorkflowUi.{tag}.{}.{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
        let _=std::fs::create_dir_all(&path);path
    }
    fn stages()->Vec<DraftStage>{vec![DraftStage{name:"专注".into(),minutes:25},DraftStage{name:"休息".into(),minutes:5}]}

    #[test]fn stages_add_delete_reorder_and_report_the_total(){
        let mut w=WorkflowState::default();
        assert_eq!(w.stages.len(),3);assert_eq!(total_minutes(&w.stages),40);assert_eq!(summary(&w.stages),"共 40 分钟 · 3 个阶段");
        add_stage(&mut w,DraftStage{name:"活动一下".into(),minutes:3}).unwrap();assert_eq!(w.stages.len(),4);
        move_stage(&mut w,3,-1).unwrap();assert_eq!(w.stages[2].name,"活动一下");
        assert!(move_stage(&mut w,0,-1).is_err());assert!(move_stage(&mut w,3,1).is_err());
        delete_stage(&mut w,0).unwrap();assert_eq!(w.stages.len(),3);
        delete_stage(&mut w,0).unwrap();assert_eq!(w.stages.len(),2);
        assert!(delete_stage(&mut w,0).is_err());
        for index in 2..STAGE_MAX{add_stage(&mut w,DraftStage{name:format!("阶段{index}"),minutes:1}).unwrap();}
        assert_eq!(w.stages.len(),STAGE_MAX);assert!(add_stage(&mut w,DraftStage{name:"溢出".into(),minutes:1}).is_err());
    }
    #[test]fn minute_steppers_and_custom_dialog_validate_bounds(){
        let mut w=WorkflowState::default();
        nudge_stage(&mut w,0,5).unwrap();assert_eq!(w.stages[0].minutes,30);
        for _ in 0..100{nudge_stage(&mut w,0,-5).unwrap();}
        assert_eq!(w.stages[0].minutes,1);
        apply_stage(&mut w,None,"活动","3").unwrap();assert_eq!(w.stages.len(),4);assert_eq!(w.stages[3].minutes,3);
        apply_stage(&mut w,Some(3),"活动一下","7").unwrap();assert_eq!(w.stages[3],DraftStage{name:"活动一下".into(),minutes:7});
        assert!(apply_stage(&mut w,None,"坏","0").is_err());
        assert!(apply_stage(&mut w,None,"坏","abc").is_err());
        assert!(apply_stage(&mut w,None,"坏","241").is_err());
        assert_eq!(w.stages.len(),4);
    }
    #[test]fn stage_payload_matches_the_native_contract(){
        let payload=stage_payload(&stages()).unwrap();
        assert_eq!(payload[0]["name"],"专注");assert_eq!(payload[0]["seconds"],1500);
        assert_eq!(payload[1]["seconds"],300);
        assert!(stage_payload(&[DraftStage{name:"只有一个".into(),minutes:5}]).is_err());
        let mut too_many=stages();while too_many.len()<13{too_many.push(DraftStage{name:"x".into(),minutes:1});}
        assert!(stage_payload(&too_many).is_err());
    }
    #[test]fn start_payload_carries_the_draft_name_stages_and_template_id(){
        let data=temp("payload");let mut m=Model::new(data,true);
        m.workflow.stages=stages();m.workflow.template_id=Some("flow-1".into());m.workflow.template_name="学习流程".into();
        let payload=start_payload(&m).unwrap();
        assert_eq!(payload["mode"],"workflow");assert_eq!(payload["name"],"学习流程");
        assert_eq!(payload["stages"][0]["name"],"专注");assert_eq!(payload["stages"][0]["seconds"],1500);
        assert_eq!(payload["templateId"],"flow-1");
        m.workflow.template_name.clear();
        assert_eq!(start_payload(&m).unwrap()["name"],"流程编排");
    }
    #[test]fn demo_start_creates_a_workflow_that_waits_and_advances_with_next(){
        let data=temp("run");let mut m=Model::new(data,true);
        m.mode=3;m.workflow.stages=stages();
        m.dispatch(Action::StartTimer);
        assert_eq!(m.timers.items.len(),1);
        let timer=m.timers.items[0].clone();assert_eq!(timer.mode,"workflow");assert_eq!(timer.clock(),"25:00");
        assert_eq!(m.selected.as_deref(),Some(timer.id.as_str()));
        m.timers.advance(25*60*1000);
        assert!(m.timers.items[0].workflow.as_ref().unwrap().waiting);
        assert_eq!(m.timers.items[0].primary_action(),"next");
        m.dispatch(Action::Timer(timer.id.clone(),"next"));
        assert_eq!(m.timers.items[0].workflow.as_ref().unwrap().stage_index,1);
        assert_eq!(m.timers.items[0].clock(),"05:00");
    }
    #[test]fn template_save_load_overwrite_and_delete_stay_in_sync(){
        let mut w=WorkflowState::default();
        save_template_local(&mut w,"学习流程",false).unwrap();
        assert_eq!(w.templates.len(),1);let id=w.template_id.clone().unwrap();assert_eq!(w.template_name,"学习流程");
        w.stages=stages();save_template_local(&mut w,"学习流程改",true).unwrap();
        assert_eq!(w.templates.len(),1);assert_eq!(w.templates[0].name,"学习流程改");
        let mut loaded=WorkflowState::default();loaded.templates=w.templates.clone();
        assert_eq!(load_template(&mut loaded,&id).unwrap(),"学习流程改");
        assert_eq!(loaded.stages,stages());
        delete_template_local(&mut w,&id);assert!(w.templates.is_empty());assert!(w.template_id.is_none());
        assert!(load_template(&mut w,&id).is_err());
    }
    #[test]fn edit_copy_starts_at_the_current_stage_with_remaining_minutes(){
        let data=temp("copy");let mut m=Model::new(data,true);
        m.timers.create(&json!({"mode":"workflow","name":"深度工作","stages":[{"name":"整理","seconds":600},{"name":"专注","seconds":1500},{"name":"休息","seconds":300}]})).unwrap();
        let id=m.timers.items[0].id.clone();
        m.timers.advance(300_000);
        let(name,stages)=copy_stages_from_timer(&m,&id).unwrap();
        assert_eq!(name,"深度工作");assert_eq!(stages.len(),3);
        assert_eq!(stages[0].minutes,5);assert_eq!(stages[1].minutes,25);assert_eq!(stages[2].minutes,5);
        let last=m.timers.items[0].workflow.as_ref().unwrap().stages.len()-1;
        m.timers.items[0].workflow.as_mut().unwrap().stage_index=last;
        let(_,tail)=copy_stages_from_timer(&m,&id).unwrap();assert_eq!(tail.len(),2);assert_eq!(tail[1].name,"休息");
    }
    #[test]fn native_template_packets_land_in_the_local_cache(){
        let data=temp("sync");let mut m=Model::new(data,true);
        m.workflow.templates.clear();
        m.native_state=json!({"templates":[{"id":"flow-9","name":"晨间","stages":[{"name":"整理","seconds":300},{"name":"专注","seconds":1500}]}]});
        sync(&mut m);
        assert_eq!(m.workflow.templates.len(),1);assert_eq!(m.workflow.templates[0].stages[1].minutes,25);
    }
    #[test]fn builtin_module_library_matches_the_native_defaults(){
        let data=temp("modules");let m=Model::new(data,true);
        let library=modules(&m);assert_eq!(library.len(),6);
        assert_eq!(library[1].name,"专注");assert_eq!(library[1].minutes,25);
        assert_eq!(library[4].minutes,3);assert_eq!(library[5].kind,5);
    }
}
