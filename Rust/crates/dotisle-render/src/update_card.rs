//! Shared update foreground for the island and the out-of-process Vello takeover window.
use vello::{Scene,kurbo::{Affine,BezPath,Stroke},peniko::Fill};
use crate::draw::{self,Text,Icon,rect};
#[derive(Clone,Copy)]pub struct Style{pub primary:u32,pub on_primary:u32,pub on_surface:u32,pub muted:u32,pub outline:u32}
pub struct Presentation<'a>{pub title:&'a str,pub value:&'a str,pub subtitle:&'a str,pub progress:f64,pub time_ms:f64,pub animate:bool,pub close:bool,pub action:Option<&'a str>,pub feedback:f32}
pub fn primary(w:f64,h:f64)->vello::kurbo::Rect{let mini=h<180.;rect(20.,h-if mini{48.}else{56.},w-40.,if mini{36.}else{40.})}
pub fn close(w:f64)->vello::kurbo::Rect{rect(w-60.,8.,44.,44.)}
/// Centre and radius of the progress ring for a card of height `h`.
pub fn ring_center(h:f64)->((f64,f64),f64){if h<180.{((42.,38.),18.)}else{((58.,100.),28.)}}
pub fn ring(scene:&mut Scene,c:(f64,f64),r:f64,p:f64,time:f64,animate:bool,track:u32,green:u32,width:f64){
 draw::ring(scene,c,r,1.,track,width);let fraction=if p<0.{0.22}else{p.clamp(0.,1.)};if fraction<=0.{return}let start=if p<0.&&animate{time/1200.*std::f64::consts::TAU}else{-std::f64::consts::FRAC_PI_2};let mut path=BezPath::new();let n=(fraction*120.).ceil().max(2.)as usize;for i in 0..=n{let a=start+fraction*std::f64::consts::TAU*i as f64/n as f64;let pt=(c.0+r*a.cos(),c.1+r*a.sin());if i==0{path.move_to(pt)}else{path.line_to(pt)}}scene.stroke(&Stroke::new(width).with_caps(vello::kurbo::Cap::Round),Affine::IDENTITY,draw::color(green),None,&path);
}
fn line(text:&mut Text,scene:&mut Scene,value:&str,x:f64,y:f64,size:f32,color:u32,bold:bool,width:f64){text.draw(scene,value,x,y,size,color,bold,width);}
pub fn paint(scene:&mut Scene,text:&mut Text,w:f64,h:f64,style:Style,p:&Presentation<'_>){
 let mini=h<180.;let green=style.primary;let(centre,radius)=ring_center(h);
 if p.close{let r=close(w);draw::icon(scene,Icon::Close,r.center().x-10.,r.center().y-10.,20.,style.muted);}
 if mini{ring(scene,centre,radius,p.progress,p.time_ms,p.animate,style.outline,green,3.);if p.progress>=0.{text.centered(scene,p.value,rect(24.,28.,36.,20.),10.,green,true);}line(text,scene,p.title,78.,16.,13.,style.on_surface,true,w-142.);line(text,scene,p.subtitle,78.,44.,11.,style.muted,false,w-100.);}
 else{
  let header=text.measure("软件更新",14.,true);let group=20.+8.+header;let gx=(w-group)/2.;draw::icon(scene,Icon::Island,gx,18.,20.,style.primary);line(text,scene,"软件更新",gx+28.,18.,14.,style.on_surface,true,header+2.);
  let cx=w/2.;let cy=88.;let radius=28.;ring(scene,(cx,cy),radius,p.progress,p.time_ms,p.animate,style.outline,green,3.);
  if p.progress>=0.{text.centered(scene,p.value,rect(cx-40.,cy-12.,80.,24.),13.,green,true);}
  text.centered(scene,p.title,rect(24.,128.,w-48.,22.),15.,style.on_surface,true);
  let mut lines=vec![];let mut value=String::new();for c in p.subtitle.chars(){let mut next=value.clone();next.push(c);if !value.is_empty()&&(c=='\n'||text.measure(&next,12.,false)>w-64.){lines.push(std::mem::take(&mut value));}if c!='\n'{value.push(c)}}if !value.is_empty(){lines.push(value)}for(i,value)in lines.iter().take(2).enumerate(){text.centered(scene,value,rect(24.,152.+i as f64*18.,w-48.,18.),12.,style.muted,false);}
 }
 if let Some(action)=p.action{let r=primary(w,h);draw::fill(scene,r,r.height()/2.,style.primary);if p.feedback>0.{scene.fill(Fill::NonZero,Affine::IDENTITY,draw::alpha(style.on_primary,p.feedback),None,&vello::kurbo::RoundedRect::from_rect(r,r.height()/2.));}text.centered(scene,action,r,if mini{12.}else{13.},style.on_primary,true);}
}
