//! HSV colour state and the modal picker. The pad, hue strip and hex field edit one value;
//! nothing reaches the native process until the user saves.
use vello::{kurbo::{Affine,RoundedRect},peniko::Fill};
use crate::{draw::{self,rect,Icon},model::{Action,Model},widgets::Ui};

#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Hsv{pub h:f64,pub s:f64,pub v:f64}
impl Hsv{
    pub fn from_rgb(rgb:u32)->Self{
        let r=((rgb>>16)&255)as f64/255.;let g=((rgb>>8)&255)as f64/255.;let b=(rgb&255)as f64/255.;
        let max=r.max(g).max(b);let min=r.min(g).min(b);let delta=max-min;
        let h=if delta<=f64::EPSILON{0.}else if max==r{((g-b)/delta).rem_euclid(6.)/6.}else if max==g{(((b-r)/delta)+2.)/6.}else{(((r-g)/delta)+4.)/6.};
        Self{h:h.rem_euclid(1.),s:if max<=f64::EPSILON{0.}else{delta/max},v:max}
    }
    pub fn rgb(self)->u32{
        let h=self.h.rem_euclid(1.)*6.;let s=self.s.clamp(0.,1.);let v=self.v.clamp(0.,1.);
        let sector=h.floor();let f=h-sector;let p=v*(1.-s);let q=v*(1.-s*f);let t=v*(1.-s*(1.-f));
        let(r,g,b)=match sector as i32%6{0=>(v,t,p),1=>(q,v,p),2=>(p,v,t),3=>(p,q,v),4=>(t,p,v),_=>(v,p,q)};
        ((r*255.).round()as u32)<<16|((g*255.).round()as u32)<<8|((b*255.).round()as u32)
    }
    pub fn hex(self)->String{format!("#{:06X}",self.rgb())}
}
pub fn parse_hex(value:&str)->Option<u32>{let v=value.trim();let v=v.strip_prefix('#').unwrap_or(v);if v.len()==6&&v.bytes().all(|b|b.is_ascii_hexdigit()){u32::from_str_radix(v,16).ok()}else{None}}
fn ink(rgb:u32)->u32{let f=|v:u32|{let c=v as f64/255.;if c<=0.04045{c/12.92}else{((c+0.055)/1.055).powf(2.4)}};let l=0.2126*f((rgb>>16)&255)+0.7152*f((rgb>>8)&255)+0.0722*f(rgb&255);if l>0.4{0x141814}else{0xFFFFFF}}
const PRESETS:[u32;8]=[0xFFFFFF,0xF4F6F8,0x000000,0xB1E8D1,0xA9E6CD,0xFFD166,0xFF8FA3,0x8ECAE6];

#[derive(Clone,Debug)]
pub struct ColorPicker{pub key:&'static str,pub title:&'static str,pub hsv:Hsv,pub original:u32,pub default:u32,pub hex:String,pub cursor:usize,pub error:String}
impl ColorPicker{
    pub fn new(key:&'static str,title:&'static str,current:Option<u32>,default:u32)->Self{
        let rgb=current.unwrap_or(default);
        Self{key,title,hsv:Hsv::from_rgb(rgb),original:rgb,default,hex:format!("#{rgb:06X}"),cursor:7,error:String::new()}
    }
    pub fn set_rgb(&mut self,rgb:u32){self.hsv=Hsv::from_rgb(rgb);self.hex=format!("#{rgb:06X}");self.cursor=self.hex.len();self.error.clear();}
    pub fn sync_hex(&mut self){self.hex=self.hsv.hex();self.cursor=self.hex.len();self.error.clear();}
    pub fn input(&mut self,text:&str){
        let accept=text.chars().filter(|c|c.is_ascii_hexdigit()||*c=='#').take(9).collect::<String>();
        if accept.is_empty(){return}
        self.cursor=self.cursor.min(self.hex.len());self.hex.insert_str(self.cursor,&accept);self.cursor+=accept.len();
        if self.hex.chars().count()>9{self.hex.truncate(9);self.cursor=self.cursor.min(9);}
        if let Some(rgb)=parse_hex(&self.hex){self.hsv=Hsv::from_rgb(rgb);self.error.clear();}
    }
    pub fn backspace(&mut self){
        self.cursor=self.cursor.min(self.hex.len());
        if let Some((i,_))=self.hex[..self.cursor].char_indices().last(){self.hex.replace_range(i..self.cursor,"");self.cursor=i;}
        if let Some(rgb)=parse_hex(&self.hex){self.hsv=Hsv::from_rgb(rgb);}
        self.error.clear();
    }
    pub fn edit_key(&mut self,key:&str){
        self.cursor=self.cursor.min(self.hex.len());
        match key{
            "Home"=>self.cursor=0,
            "End"=>self.cursor=self.hex.len(),
            "Left"=>self.cursor=self.hex[..self.cursor].char_indices().last().map(|(i,_)|i).unwrap_or(0),
            "Right"=>self.cursor=self.hex[self.cursor..].chars().next().map(|c|self.cursor+c.len_utf8()).unwrap_or(self.cursor),
            "Delete"=>{if self.cursor<self.hex.len(){let c=self.hex[self.cursor..].chars().next().unwrap_or('\0');self.hex.replace_range(self.cursor..self.cursor+c.len_utf8(),"");if let Some(rgb)=parse_hex(&self.hex){self.hsv=Hsv::from_rgb(rgb);}}}
            _=>{}
        }
    }
}
impl Model{
    pub fn open_picker(&mut self,key:&'static str,title:&'static str){
        let default=match key{"IslandDynamicSeedColor"=>0xB1E8D1,"SettingsLightAccentColor"=>0x2563EB,"SettingsDarkAccentColor"=>0x6EA8FF,"ExpandedTextColor"=>0xF4F6F8,"ExpandedComponentColor"=>0xB1E8D1,"LyricProgressColor"=>0xA9E6CD,_=>0xF4F6F8};
        let current=self.setting(key).as_str().and_then(parse_hex);
        self.dialog=None;self.palette=false;self.picker=Some(ColorPicker::new(key,title,current,default));
    }
    pub fn color_pad(&mut self,s:f64,v:f64){if let Some(p)=&mut self.picker{p.hsv.s=s.clamp(0.,1.);p.hsv.v=v.clamp(0.,1.);p.sync_hex();}}
    pub fn color_hue(&mut self,h:f64){if let Some(p)=&mut self.picker{p.hsv.h=h.rem_euclid(1.);p.sync_hex();}}
    pub fn picker_input(&mut self,text:&str){if let Some(p)=&mut self.picker{p.input(text);}}
    pub fn picker_backspace(&mut self){if let Some(p)=&mut self.picker{p.backspace();}}
    pub fn picker_edit_key(&mut self,key:&str){if let Some(p)=&mut self.picker{p.edit_key(key);}}
    pub fn save_picker(&mut self){
        let Some(p)=self.picker.as_ref()else{return};
        let Some(rgb)=parse_hex(&p.hex)else{if let Some(p)=&mut self.picker{p.error="请输入 6 位十六进制颜色，例如 #A9E6CD。".into();}return};
        let key=p.key;let hex=format!("#{rgb:06X}");if key=="IslandDynamicSeedColor"{self.native_state["settings"]["IslandColorSource"]=serde_json::json!("Custom");self.native_update("IslandColorSource",serde_json::json!("Custom"));}
        self.picker=None;self.native_state["settings"][key]=serde_json::json!(hex.clone());self.native_update(key,serde_json::json!(hex));self.message("颜色已更新");
    }
}

pub fn panel(ui:&mut Ui,m:&Model,width:f64,height:f64){
    let Some(p)=m.picker.as_ref()else{return};let s=ui.s;ui.hits.clear();
    ui.scene.fill(Fill::NonZero,Affine::IDENTITY,draw::alpha(0,0.48),None,&rect(0.,0.,width,height));
    let w=680.;let h=560.;let x=(width-w)/2.;let y=(height-h)/2.;ui.panel(rect(x,y,w,h),s.surface);
    ui.label(&format!("选择颜色 · {}",p.title),x+28.,y+20.,24.,s.on_surface,true,w-110.);
    ui.icon_button(rect(x+w-64.,y+16.,44.,44.),Icon::Close,"取消取色",Action::ColorClose,false,true);
    ui.label("饱和度 / 明度",x+28.,y+72.,12.,s.on_surface_variant,false,200.);
    let pad=rect(x+28.,y+96.,360.,248.);draw::fill(ui.scene,pad,14.,s.high);
    ui.scene.push_clip_layer(Affine::IDENTITY,&RoundedRect::from_rect(pad,14.));
    let cols=40;let rows=24;
    for iy in 0..rows{for ix in 0..cols{
        let cell=rect(pad.x0+pad.width()*ix as f64/cols as f64,pad.y0+pad.height()*iy as f64/rows as f64,pad.width()/cols as f64+0.5,pad.height()/rows as f64+0.5);
        draw::fill(ui.scene,cell,0.,Hsv{h:p.hsv.h,s:(ix as f64+0.5)/cols as f64,v:1.-(iy as f64+0.5)/rows as f64}.rgb());
    }}
    ui.scene.pop_layer();draw::outline(ui.scene,pad,14.,s.outline,1.);
    let kx=pad.x0+p.hsv.s*pad.width();let ky=pad.y0+(1.-p.hsv.v)*pad.height();
    draw::circle(ui.scene,kx,ky,11.,0xFFFFFF);draw::circle(ui.scene,kx,ky,8.,p.hsv.rgb());
    ui.hit(pad,"饱和度与明度",Action::ColorPad([pad.x0,pad.y0,pad.width(),pad.height()]),true);
    ui.label("色相",x+28.,y+356.,12.,s.on_surface_variant,false,100.);
    let hue=rect(x+28.,y+380.,360.,22.);
    ui.scene.push_clip_layer(Affine::IDENTITY,&RoundedRect::from_rect(hue,11.));
    for i in 0..48{let a=i as f64/48.;draw::fill(ui.scene,rect(hue.x0+hue.width()*i as f64/48.,hue.y0,hue.width()/48.+0.5,hue.height()),0.,Hsv{h:a,s:1.,v:1.}.rgb());}
    ui.scene.pop_layer();
    let hx=hue.x0+p.hsv.h*hue.width();draw::circle(ui.scene,hx,hue.center().y,9.,0xFFFFFF);draw::circle(ui.scene,hx,hue.center().y,6.,Hsv{h:p.hsv.h,s:1.,v:1.}.rgb());
    ui.hit(hue,"色相",Action::ColorHue([hue.x0,hue.y0,hue.width(),hue.height()]),true);
    ui.label("常用色",x+28.,y+422.,12.,s.on_surface_variant,false,100.);
    for(i,rgb)in PRESETS.iter().enumerate(){
        let r=rect(x+28.+i as f64*44.,y+446.,34.,34.);draw::fill(ui.scene,r,10.,*rgb);draw::outline(ui.scene,r,10.,s.outline,1.);
        if *rgb==p.hsv.rgb(){draw::icon(ui.scene,Icon::Check,r.x0+7.,r.y0+7.,20.,ink(*rgb));}
        let index=ui.hit(r,"常用色",Action::ColorPreset(*rgb),true);ui.state(r,10.,index,s.on_surface);
    }
    let rx=x+420.;let rw=w-448.;let half=(rw-8.)/2.;
    ui.label("原本",rx,y+72.,11.,s.on_surface_variant,false,half);ui.label("新的",rx+half+8.,y+72.,11.,s.on_surface_variant,false,half);
    let old=rect(rx,y+96.,half,72.);draw::fill(ui.scene,old,12.,p.original);draw::outline(ui.scene,old,12.,s.outline,1.);
    let new=rect(rx+half+8.,y+96.,half,72.);draw::fill(ui.scene,new,12.,p.hsv.rgb());draw::outline(ui.scene,new,12.,s.outline,1.);
    ui.label("HEX",rx,y+190.,11.,s.primary,true,60.);
    let field=rect(rx,y+210.,rw,64.);draw::fill(ui.scene,field,12.,s.high);draw::outline(ui.scene,field,12.,s.primary,2.);
    let caret=p.cursor.min(p.hex.len());let text=format!("{}│{}",&p.hex[..caret],&p.hex[caret..]);
    ui.label(&text,rx+14.,y+230.,19.,s.on_surface,true,rw-28.);ui.hit(field,"HEX 输入",Action::ColorField,true);
    ui.label("拖动取色，或输入 HEX",rx,y+288.,11.,s.on_surface_variant,false,rw);ui.label("保存后应用到灵动岛。",rx,y+306.,11.,s.on_surface_variant,false,rw);
    if !p.error.is_empty(){ui.label(&p.error,x+28.,y+h-108.,12.,s.error,true,w-56.);}
    ui.button(rect(x+28.,y+h-66.,140.,44.),"恢复默认",None,Action::ColorReset,true,true);
    ui.button(rect(x+w-248.,y+h-66.,96.,44.),"取消",None,Action::ColorClose,true,true);
    ui.button(rect(x+w-140.,y+h-66.,112.,44.),"保存",None,Action::ColorSave,false,true);
}

#[cfg(test)]
mod tests{
    use super::*;
    fn close(a:f64,b:f64)->bool{(a-b).abs()<0.006}
    #[test]fn primaries_and_greys_map_to_expected_hsv(){
        assert!(close(Hsv::from_rgb(0xFF0000).h,0.));assert!(close(Hsv::from_rgb(0x00FF00).h,1./3.));assert!(close(Hsv::from_rgb(0x0000FF).h,2./3.));
        let grey=Hsv::from_rgb(0x808080);assert!(close(grey.s,0.));assert!(close(grey.v,0.502));
    }
    #[test]fn every_colour_survives_the_hsv_round_trip(){
        for hex in [0x000000,0xFFFFFF,0xA9E6CD,0xB1E8D1,0xF4F6F8,0xFF6699,0x123456]{assert_eq!(Hsv::from_rgb(hex).rgb(),hex,"{hex:06X}");}
    }
    #[test]fn hue_wraps_back_to_red(){let red=Hsv{h:1.,s:1.,v:1.}.rgb();assert_eq!(red,0xFF0000);assert_eq!(Hsv{h:-0.25,s:1.,v:1.}.rgb(),Hsv{h:0.75,s:1.,v:1.}.rgb());}
    #[test]fn hex_parser_accepts_an_optional_hash_and_rejects_partials(){
        assert_eq!(parse_hex(" a9e6cd "),Some(0xA9E6CD));assert_eq!(parse_hex("#A9E6CD"),Some(0xA9E6CD));
        for v in ["#fff","#12345G","1234567",""]{assert!(parse_hex(v).is_none(),"{v}");}
    }
    #[test]fn picker_commits_only_normalised_hex(){let mut p=ColorPicker::new("LyricProgressColor","歌词进度高亮",Some(0xA9E6CD),0xA9E6CD);assert_eq!(p.hex,"#A9E6CD");p.set_rgb(0xFF6699);assert_eq!(p.hex,"#FF6699");p.hex="#12".into();assert!(parse_hex(&p.hex).is_none());}
}
