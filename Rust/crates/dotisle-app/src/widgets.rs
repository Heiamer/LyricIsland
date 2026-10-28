//! Shared Material components and a matching hit/focus tree in logical pixels.
use vello::{Scene,kurbo::{Rect,Affine},peniko::{Fill}};
use dotisle_domain::monet::Scheme;
use crate::{draw::{self,Text,Icon},model::Action};
#[derive(Clone,Debug)]pub struct Hit{pub rect:Rect,pub action:Action,pub label:String,pub enabled:bool}
pub struct Ui<'a>{pub selection_values:std::collections::HashMap<String,f64>,pub motion_targets:Vec<(String,f64)>,pub press_layers:Vec<f64>,pub switches:std::collections::HashMap<String,f64>,pub state_layers:Vec<f32>,pub scene:&'a mut Scene,pub text:&'a mut Text,pub s:Scheme,pub hits:Vec<Hit>,pub pointer:(f64,f64),pub focus:Option<usize>,pub pressed:Option<usize>,pub clip:Rect}
impl<'a> Ui<'a>{
    pub fn selection(&mut self,action:&Action,selected:bool)->f64{let key=match action{Action::NativeEdit(key,serde_json::Value::Bool(_))=>format!("switch:{key}"),_=>format!("selection:{action:?}")};let target=if selected{1.}else{0.};self.motion_targets.push((key.clone(),target));self.selection_values.get(&key).copied().unwrap_or(target).clamp(0.,1.)}
    fn press(&self,index:Option<usize>)->f64{index.and_then(|i|self.press_layers.get(i)).copied().unwrap_or(if index.is_some()&&self.pressed==index{1.}else{0.}).clamp(0.,1.)}
    fn mix(a:u32,b:u32,t:f64)->u32{let t=t.clamp(0.,1.);let c=|n:u32|{let x=((a>>n)&255)as f64;let y=((b>>n)&255)as f64;(x+(y-x)*t).round()as u32};c(16)<<16|c(8)<<8|c(0)}
    pub fn label(&mut self,text:&str,x:f64,y:f64,size:f32,rgb:u32,bold:bool,max:f64){self.text.draw(self.scene,text,x,y,size,rgb,bold,max);}
    pub fn center(&mut self,text:&str,r:Rect,size:f32,rgb:u32,bold:bool){self.text.centered(self.scene,text,r,size,rgb,bold);}
    pub fn panel(&mut self,r:Rect,rgb:u32){draw::fill(self.scene,r,28.,rgb);}
    pub fn hit(&mut self,r:Rect,label:&str,action:Action,enabled:bool)->Option<usize>{let r=r.intersect(self.clip);if r.width()<=0.||r.height()<=0.{return None}let index=self.hits.len();self.hits.push(Hit{rect:r,action,label:label.into(),enabled});Some(index)}
    pub fn state(&mut self,r:Rect,radius:f64,index:Option<usize>,foreground:u32){let Some(index)=index else{return};let hovered=r.contains(self.pointer);let target=if self.pressed==Some(index){0.12}else if self.focus==Some(index){0.10}else if hovered{0.08}else{0.};let opacity=self.state_layers.get(index).copied().unwrap_or(target).clamp(0.,0.16);if opacity>0.{self.scene.fill(Fill::NonZero,Affine::IDENTITY,draw::alpha(foreground,opacity),None,&vello::kurbo::RoundedRect::from_rect(r,radius));}if self.focus==Some(index){draw::outline(self.scene,r.inflate(3.,3.),radius+3.,self.s.primary,2.);}}
    pub fn button(&mut self,r:Rect,label:&str,icon:Option<Icon>,action:Action,tonal:bool,enabled:bool){
        let(bg,fg)=if !enabled{(self.s.high,self.s.on_surface_variant)}else if tonal{(self.s.secondary_container,self.s.on_secondary_container)}else{(self.s.primary,self.s.on_primary)};
        let i=self.hit(r,label,action,enabled);let p=if enabled{self.press(i)}else{0.};let r=r.inset((r.width()*0.015*p,r.height()*0.015*p));let radius=r.height()/2.-10.*p;draw::fill(self.scene,r,radius,bg);if enabled{self.state(r,radius,i,fg);}
        if let Some(icon)=icon{draw::icon(self.scene,icon,r.x0+18.,r.center().y-10.,20.,fg);self.center(label,Rect::new(r.x0+35.,r.y0,r.x1-8.,r.y1),14.,fg,true);}else{self.center(label,r,14.,fg,true);}
    }
    pub fn icon_button(&mut self,r:Rect,kind:Icon,label:&str,action:Action,filled:bool,enabled:bool){let shape=if filled&&matches!(kind,Icon::Play|Icon::Pause){self.selection(&action,matches!(kind,Icon::Pause))}else{0.};let fg=if filled{self.s.on_primary}else{self.s.on_surface_variant};let i=self.hit(r,label,action,enabled);let p=if enabled{self.press(i)}else{0.};let r=r.inset((r.width()*0.015*p,r.height()*0.015*p));let radius=(r.height()/2.-(r.height()*0.2)*shape-6.*p).max(8.);if filled{draw::fill(self.scene,r,radius,if enabled{self.s.primary}else{self.s.high});}if enabled{self.state(r,radius,i,fg);}draw::icon(self.scene,kind,r.center().x-12.,r.center().y-12.,24.,if enabled{fg}else{self.s.outline});}
    pub fn chip(&mut self,r:Rect,label:&str,selected:bool,action:Action){let t=self.selection(&action,selected);let i=self.hit(r,label,action,true);let p=self.press(i);let r=r.inset((1.5*p,1.*p));let radius=12.+(r.height()/2.-12.)*t-4.*p;draw::fill(self.scene,r,radius,Self::mix(self.s.container,self.s.secondary_container,t));if t<0.999{draw::outline(self.scene,r,radius,Self::mix(self.s.outline,self.s.secondary_container,t),1.);}self.state(r,radius,i,self.s.on_surface);if t>0.001&&r.width()>110.{let size=16.*(0.8+0.2*t);self.scene.push_layer(vello::peniko::Mix::Normal,t as f32,Affine::IDENTITY,&r);draw::icon(self.scene,Icon::Check,r.x0+10.,r.center().y-size/2.,size,self.s.on_secondary_container);self.scene.pop_layer();}let shift=if r.width()>110.{8.*t}else{0.};self.center(label,Rect::new(r.x0+shift,r.y0,r.x1+shift,r.y1),14.,Self::mix(self.s.on_surface_variant,self.s.on_secondary_container,t),selected);}
    pub fn toggle(&mut self,r:Rect,title:&str,detail:&str,on:bool,action:Action){
        self.label(title,r.x0,r.y0+9.,16.,self.s.on_surface,false,r.width()-100.);self.label(detail,r.x0,r.y0+36.,12.,self.s.on_surface_variant,false,r.width()-100.);
        let t=self.selection(&action,on);let i=self.hit(r,title,action,true);let p=self.press(i);let switch=draw::rect(r.x1-56.,r.center().y-16.,52.,32.);let track=Self::mix(self.s.high,self.s.primary,t);draw::fill(self.scene,switch,16.,track);if t<0.999{draw::outline(self.scene,switch,16.,Self::mix(self.s.on_surface_variant,self.s.primary,t),1.5);}
        let center=switch.x0+16.+20.*t;let radius=8.+4.*t+1.5*p;draw::circle(self.scene,center,switch.center().y,radius,Self::mix(self.s.on_surface_variant,self.s.on_primary,t));if t>0.5{self.scene.push_layer(vello::peniko::Mix::Normal,((t-0.5)*2.)as f32,Affine::IDENTITY,&switch);draw::icon(self.scene,Icon::Check,center-8.,switch.y0+8.,16.,self.s.primary);self.scene.pop_layer();}self.state(switch,16.,i,self.s.on_surface);
    }
    pub fn slider(&mut self,r:Rect,fraction:f64,enabled:bool,phase:f64){
        let middle=r.center().y;let position=r.x0+r.width()*fraction.clamp(0.,1.);let fg=self.s.primary;
        if position+8.<r.x1{draw::fill(self.scene,Rect::new((position+8.).min(r.x1),middle-4.,r.x1,middle+4.),4.,self.s.outline);}
        if position>r.x0+4.{let mut wave=vello::kurbo::BezPath::new();let length=(position-r.x0-6.).max(0.);let count=length.ceil()as usize;for i in 0..=count{let x=r.x0+i as f64;let y=middle+((i as f64/26.)*std::f64::consts::TAU-phase).sin()*2.5;if i==0{wave.move_to((x,y))}else{wave.line_to((x,y))}}self.scene.stroke(&vello::kurbo::Stroke::new(4.).with_caps(vello::kurbo::Cap::Round),Affine::IDENTITY,draw::color(fg),None,&wave);}
        draw::fill(self.scene,draw::rect(position-2.,middle-12.,4.,24.),2.,fg);draw::circle(self.scene,r.x1-4.,middle,2.,self.s.on_surface_variant);
        let i=self.hit(r,"播放进度",Action::Seek,enabled);if self.focus==i&&i.is_some(){draw::outline(self.scene,r.inflate(2.,2.),12.,self.s.primary,2.);}
    }
    pub fn clip(&mut self,r:Rect){self.clip=r;self.scene.push_clip_layer(Affine::IDENTITY,&r);}
    pub fn end_clip(&mut self,window:Rect){self.scene.pop_layer();self.clip=window;}
}
