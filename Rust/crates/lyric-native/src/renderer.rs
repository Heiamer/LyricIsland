//! Vello island painter. The v2 layout, activity model, hit targets and motion are retained.
use lyric_core::shared_motion::{Spec as MotionSpec,Scheme as MotionScheme,Kind as MotionKind,Speed as MotionSpeed};
use std::{path::Path,sync::Arc,collections::HashMap,time::{Instant,Duration}};
use windows::{core::{Result,Error,HRESULT},Win32::Foundation::HWND};
use vello::{Scene as VScene,kurbo::{Affine,RoundedRect,Stroke,Circle,BezPath,Line,Cap},peniko::{Fill,Color,ImageData,ImageFormat,ImageAlphaType,Blob,Gradient,Mix}};
use dotisle_render::draw::{Text,Icon};
use crate::{layout::{Rect,Layout},motion::{Frame,blend}};
fn err(e:impl ToString)->Error{Error::new(HRESULT(0x80004005u32 as i32),e.to_string())}
fn vr(r:Rect)->vello::kurbo::Rect{vello::kurbo::Rect::new(r.x,r.y,r.x+r.w,r.y+r.h)}
pub fn color(argb:u32)->Color{Color::from_rgba8((argb>>16)as u8,(argb>>8)as u8,argb as u8,(argb>>24)as u8)}

pub struct PreparedCover{image:ImageData,bgra:Vec<u8>,width:u32,height:u32,seed:Option<u32>}
impl PreparedCover{pub fn save_preview(&self,path:&Path)->std::result::Result<(),String>{image::save_buffer(path,self.image.data.data(),self.width,self.height,image::ColorType::Rgba8).map_err(|e|e.to_string())}}
pub struct Renderer{window:HWND,glass:crate::glass::Sampler,glass_live:bool,glass_allowed:bool,glass_active:bool,
    gpu:crate::vello_surface::Surface,canvas:VScene,text:Text,
    blurred_cover:Option<(u32,ImageData)>,cover:Option<ImageData>,previous_cover:Option<ImageData>,notification_icon:Option<ImageData>,
    device_images:HashMap<String,ImageData>,cover_palette:crate::cover_color::CoverPalette,paint_palette:crate::cover_color::CoverPalette,cover_seed:Option<u32>,dynamic_resolved:Option<dotisle_domain::dynamic_color::Resolved>,
    color_motion:dotisle_domain::dynamic_color::RoleMotion,m3_cache:Option<(u32,bool,dotisle_domain::monet::Scheme)>,preview_pixels:Option<(Vec<u8>,u32,u32)>,cover_changed:Instant,flip_from:f64,flip_velocity:f64,
    pub body_line_spacing:f64,pub body_baseline:f64,pub width:u32,pub height:u32,pub dpi:f32,
}
impl Renderer{
    pub unsafe fn new(hwnd:HWND,width:u32,height:u32,dpi:f32,font:&Path)->Result<Self>{
        let gpu=crate::vello_surface::Surface::new(hwnd,width,height).map_err(err)?;let mut text=Text::empty();
        text.register(font).map_err(err)?;
        // Google Sans Flex covers Latin and digits; every weight is bundled.
        for name in dotisle_render::draw::LATIN_FONTS{let _=text.register(&font.with_file_name(name));}
        // Register the Japanese fallback only when text first requires it.
        let japanese=font.with_file_name("NotoSansJP.ttf");
        text.defer_japanese(japanese);
        Ok(Self{window:hwnd,glass:Default::default(),glass_live:false,glass_allowed:true,glass_active:false,gpu,canvas:VScene::new(),text,color_motion:Default::default(),m3_cache:None,blurred_cover:None,cover:None,previous_cover:None,notification_icon:None,device_images:HashMap::new(),cover_palette:Default::default(),paint_palette:Default::default(),cover_seed:None,dynamic_resolved:None,preview_pixels:None,cover_changed:Instant::now()-Duration::from_secs(1),flip_from:0.,flip_velocity:0.,body_line_spacing:1.448,body_baseline:1.16,width,height,dpi})
    }
    fn fill(&mut self,r:Rect,radius:f64,c:u32){if r.w>0.&&r.h>0.{self.canvas.fill(Fill::NonZero,Affine::IDENTITY,color(c),None,&RoundedRect::from_rect(vr(r),radius));}}
    fn outline(&mut self,r:Rect,radius:f64,c:u32,width:f64){self.canvas.stroke(&Stroke::new(width),Affine::IDENTITY,color(c),None,&RoundedRect::from_rect(vr(r),radius));}
    fn line(&mut self,a:(f64,f64),b:(f64,f64),c:u32,width:f64){self.canvas.stroke(&Stroke::new(width).with_caps(vello::kurbo::Cap::Round),Affine::IDENTITY,color(c),None,&Line::new(a,b));}
    fn ring(&mut self,x:f64,y:f64,r:f64,c:u32,width:f64){self.canvas.stroke(&Stroke::new(width),Affine::IDENTITY,color(c),None,&Circle::new((x,y),r));}
    /// Determinate byte progress or an indeterminate rotating arc; never a fake percentage.
    fn update_ring(&mut self,x:f64,y:f64,radius:f64,progress:f64,time:f64,animate:bool,accent:u32){
        let track=self.m3_cache.map(|(_,_,c)|c.outline).unwrap_or(0x43534c);self.ring(x,y,radius,0xff000000|track,3.);
        let fraction=if progress<0.{0.22}else{progress.clamp(0.,1.)};
        let start=if progress<0.&&animate{time/1200.*std::f64::consts::TAU}else{-std::f64::consts::FRAC_PI_2};
        if fraction>=0.9999{self.ring(x,y,radius,accent,3.);return}
        if fraction<=0.{return}
        let mut path=BezPath::new();let steps=(fraction*120.).ceil().max(2.)as usize;
        for i in 0..=steps{let angle=start+fraction*std::f64::consts::TAU*i as f64/steps as f64;let p=(x+radius*angle.cos(),y+radius*angle.sin());if i==0{path.move_to(p)}else{path.line_to(p)}}
        self.canvas.stroke(&Stroke::new(3.).with_caps(Cap::Round),Affine::IDENTITY,color(accent),None,&path);
    }
    fn clip(&mut self,r:Rect,radius:f64,alpha:f32){self.canvas.push_layer(Mix::Normal,alpha.clamp(0.,1.),Affine::IDENTITY,&RoundedRect::from_rect(vr(r),radius));}
    fn text_at(&mut self,text:&str,r:Rect,size:f64,weight:i32,c:u32,align:f64){
        self.text.line(&mut self.canvas,text,vr(r),size as f32,c,weight>=550,align);
    }
    fn text_line(&mut self,text:&str,r:Rect,size:f64,weight:i32,c:u32){self.text_at(text,r,size,weight,c,0.);}
    fn image(&mut self,image:&ImageData,r:Rect,radius:f64,opacity:f32){self.clip(r,radius,opacity);dotisle_render::draw::image(&mut self.canvas,image,vr(r),radius);self.canvas.pop_layer();}
    fn icon(&mut self,kind:Icon,r:Rect,c:u32){self.clip(r,0.,(c>>24)as f32/255.);dotisle_render::draw::icon(&mut self.canvas,kind,r.x,r.y,r.w,c&0xffffff);self.canvas.pop_layer();}
    fn collect(&mut self,draw:impl FnOnce(&mut Self))->VScene{let outer=std::mem::take(&mut self.canvas);draw(self);std::mem::replace(&mut self.canvas,outer)}
    pub fn set_font_family(&mut self,family:&str){self.text.set_family(family);}
    pub unsafe fn measure_line(&mut self,text:&str,size:f64)->Result<f64>{Ok(self.text.measure(text,size as f32,false))}
    pub fn status_indicator(&mut self,label:&str,dot:Rect,text:Rect)->Rect{let center=self.text.ink_center_y(label,vr(text),9.,false);Rect::new(dot.x,center-dot.h/2.,dot.w,dot.h)}
    pub fn compact_metrics(&self,font:f64,double:bool,scale:f64)->(f64,f64,f64){crate::compact::group_metrics(font,self.body_line_spacing,scale,double)}
    pub fn compact_height(&self,font:f64,double:bool,scale:f64)->f64{crate::compact::capsule_height(font,self.body_line_spacing,scale,double)}
    fn decode(bytes:&[u8])->std::result::Result<(ImageData,Vec<u8>,u32,u32),Box<dyn std::error::Error>>{
        let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;reader.limits(image::Limits::default());let image=reader.decode()?.thumbnail(512,512).to_rgba8();let(w,h)=image.dimensions();let rgba=image.into_raw();let mut bgra=rgba.clone();for p in bgra.chunks_exact_mut(4){let a=p[3]as u32;let red=p[0];p[0]=(p[2]as u32*a/255)as u8;p[1]=(p[1]as u32*a/255)as u8;p[2]=(red as u32*a/255)as u8;}
        Ok((ImageData{data:Blob::new(Arc::new(rgba)),format:ImageFormat::Rgba8,alpha_type:ImageAlphaType::Alpha,width:w,height:h},bgra,w,h))
    }
    pub fn prepare_cover(bytes:Option<&[u8]>)->std::result::Result<Option<PreparedCover>,String>{
        let decoded=bytes.map(Self::decode).transpose().map_err(|e|e.to_string())?;
        Ok(decoded.map(|(image,bgra,width,height)|{let small=image::RgbaImage::from_raw(width,height,image.data.data().to_vec()).map(|rgba|image::imageops::thumbnail(&rgba,64,64));let seed=small.as_ref().and_then(|p|dotisle_domain::monet::seed(p.as_raw()));PreparedCover{image,bgra,width,height,seed}}))
    }
    pub fn apply_cover(&mut self,prepared:Option<PreparedCover>){
        let(phase,velocity)=MotionSpec::token(MotionScheme::Expressive,MotionKind::Spatial,MotionSpeed::Default).sample(self.flip_from,1.,self.flip_velocity,self.cover_changed.elapsed().as_secs_f64());let phase=phase.clamp(0.,1.);
        if phase>=0.5||self.previous_cover.is_none(){self.previous_cover=self.cover.clone();self.flip_from=1.-phase;self.flip_velocity=-velocity;}else{self.flip_from=phase;self.flip_velocity=velocity;}
        self.cover=None;self.blurred_cover=None;self.preview_pixels=None;self.cover_palette=Default::default();self.cover_seed=None;
        if let Some(p)=prepared{self.cover_seed=p.seed;if let Some(seed)=p.seed{let roles=dotisle_domain::dynamic_color::roles(seed,true,true,false);self.cover_palette=crate::cover_color::CoverPalette{background:roles.surface_container_high,accent:roles.primary};}self.paint_palette=self.cover_palette;self.preview_pixels=Some((p.bgra,p.width,p.height));self.cover=Some(p.image);}
        self.cover_changed=Instant::now();
    }
    pub unsafe fn set_cover(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{let prepared=Self::prepare_cover(bytes)?;self.apply_cover(prepared);Ok(())}
    pub unsafe fn set_notification_icon(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{self.notification_icon=bytes.map(Self::decode).transpose()?.map(|d|d.0);Ok(())}
    pub fn material_theme(&self,material:Material,cover_background:bool)->MaterialTheme{MaterialTheme{glass_refraction:self.glass_allowed,roles:self.dynamic_resolved.map(|v|v.roles),material,cover_background,palette:self.paint_palette,pixels:self.preview_pixels.clone()}}
    pub unsafe fn set_material_theme(&mut self,theme:&MaterialTheme)->Result<()>{self.glass_allowed=theme.glass_refraction;if let Some(roles)=theme.roles{let mut resolved=dotisle_domain::dynamic_color::resolve(dotisle_domain::dynamic_color::Source::Custom,None,None,0x496b55,true,false);resolved.roles=roles;self.dynamic_resolved=Some(resolved);self.m3_cache=Some((resolved.seed,true,roles.ui()));}self.cover_palette=theme.palette;self.paint_palette=theme.palette;self.blurred_cover=None;self.cover=theme.pixels.as_ref().map(|(p,w,h)|{let mut rgba=p.clone();for c in rgba.chunks_exact_mut(4){let a=c[3]as u32;let blue=c[0];c[0]=if a>0{(c[2]as u32*255/a).min(255)as u8}else{0};c[1]=if a>0{(c[1]as u32*255/a).min(255)as u8}else{0};c[2]=if a>0{(blue as u32*255/a).min(255)as u8}else{0};}ImageData{data:Blob::new(Arc::new(rgba)),format:ImageFormat::Rgba8,alpha_type:ImageAlphaType::Alpha,width:*w,height:*h}});Ok(())}
    pub fn glass_tick(&mut self)->bool{self.glass_active&&self.glass_live&&(self.glass.poll()||self.glass.due())}
    pub fn glass_status(&self)->serde_json::Value{serde_json::json!({"active":self.glass_active,"samples":self.glass.captures,"enabled":self.glass_allowed})}
    pub fn glass_fixture(&mut self){self.glass.enable_fixture(self.width,self.height);}
    pub fn preview_palette(&self)->(u32,u32){(self.paint_palette.background,self.paint_palette.accent)}
    pub fn preview_art(&self,path:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{if let Some(image)=&self.cover{if let Some(p)=path.parent(){std::fs::create_dir_all(p)?;}image::save_buffer(path,image.data.data(),image.width,image.height,image::ColorType::Rgba8)?;}Ok(())}
    fn cover(&mut self,r:Rect,radius:f64,opacity:f32){if let Some(image)=self.cover.clone(){self.image(&image,r,radius,opacity);}else{let tones=self.m3_cache.map(|(_,_,c)|c).unwrap_or_else(||dotisle_domain::dynamic_color::scheme(0x496b55,true));self.fill(r,radius,0xff000000|tones.secondary_container);let size=r.w*0.56;self.icon(Icon::Music,Rect::new(r.x+(r.w-size)/2.,r.y+(r.h-size)/2.,size,size),0xff000000|tones.on_secondary_container);}}
    fn flipping_cover(&mut self,r:Rect,s:&Scene<'_>){let radius=crate::layout::cover_corner_radius(s.radius,r.w,r.h);let t=if s.animate{MotionSpec::token(s.motion_scheme,MotionKind::Spatial,MotionSpeed::Default).paced(s.motion_speed).sample(self.flip_from,1.,self.flip_velocity,s.cover_age_ms/1000.).0.clamp(0.,1.)}else{1.};if t>=1.||self.previous_cover.is_none(){self.cover(r,radius,s.cover_opacity as f32);return}
        let sx=(std::f64::consts::PI*t).cos().abs().max(0.018);let sy=1.-0.06*(std::f64::consts::PI*t).sin();let image=if t<0.5{self.previous_cover.clone()}else{self.cover.clone()};let part=self.collect(|a|{if let Some(image)=image{a.image(&image,r,radius,s.cover_opacity as f32);}else{a.cover(r,radius,s.cover_opacity as f32);}});let cx=r.x+r.w/2.;let cy=r.y+r.h/2.;self.canvas.append(&part,Some(Affine::translate((cx,cy))*Affine::scale_non_uniform(sx,sy)*Affine::translate((-cx,-cy))));
    }
    fn surface_spec(s:&Scene<'_>)->crate::surface::Surface{crate::surface::Surface{material:s.material,compact:if s.mini{s.mini_capsule}else{s.classic_capsule},reveal:s.frame.card_reveal,overlay:s.notification_mix.max(s.osd_mix),mini:s.mini,transparent:s.transparent,cover_background:s.expanded_cover_background,compact_cover_background:s.cover_background,cover_overlay:s.cover_overlay&&!s.dynamic_high_contrast,card_frost:s.acrylic_card_frost,card_opacity:if s.dynamic_high_contrast{0.}else{s.acrylic_card_opacity},capsule_frost:s.acrylic_capsule_frost,capsule_opacity:if s.dynamic_high_contrast{0.}else{s.acrylic_capsule_opacity}}}
    fn material_surface(&mut self,r:Rect,radius:f64,s:crate::surface::Surface){
        let p=s.paint(self.paint_palette);let gradient=Gradient::new_linear((r.x,r.y),(r.x,r.y+r.h)).with_stops([color(p.top),color(p.bottom)]);self.canvas.fill(Fill::NonZero,Affine::IDENTITY,&gradient,None,&RoundedRect::from_rect(vr(r),radius));
        if p.cover_alpha>0.001{if let Some(image)=self.cover.clone(){let blur=p.cover_blur.max(0.).round()as u32;if self.blurred_cover.as_ref().is_none_or(|(key,_)|*key!=blur){if let Some(rgba)=image::RgbaImage::from_raw(image.width,image.height,image.data.data().to_vec()){let small=image::imageops::thumbnail(&rgba,128,128);let ratio=small.width()as f32/image.width as f32;let blurred=image::imageops::blur(&small,blur as f32*ratio);let(w,h)=blurred.dimensions();let mut pixels=blurred.into_raw();
                    // Keep the frosted artwork a dark M3E surface: a near-white cover would otherwise lift the card past white text contrast.
                    for c in pixels.chunks_exact_mut(4){c[0]=(c[0]as u32*58/100)as u8;c[1]=(c[1]as u32*58/100)as u8;c[2]=(c[2]as u32*58/100)as u8;}
                    self.blurred_cover=Some((blur,ImageData{data:Blob::new(Arc::new(pixels)),width:w,height:h,..image.clone()}));}}let image=self.blurred_cover.as_ref().map(|(_,i)|i.clone()).unwrap_or(image);self.image(&image,r,radius,p.cover_alpha as f32);let scrim=((80.+36.*(1.-p.amount))as u32)<<24;self.fill(r,radius,scrim|crate::cover_color::mix(0x10171a,self.paint_palette.background,0.4*p.tint));}}
        if p.frost>0.001{let frost=Gradient::new_linear((r.x,r.y),(r.x,r.y+r.h)).with_stops([color(((p.frost*15.)as u32)<<24|0xffffff),color(0x00ffffff)]);self.canvas.fill(Fill::NonZero,Affine::IDENTITY,&frost,None,&RoundedRect::from_rect(vr(r),radius));}
        if self.glass_allowed&&((s.material==Material::Glass&&p.amount>0.001)||(s.compact==Material::Glass&&p.amount<0.999)){self.glass_active=true;if let Some(edge)=self.glass.image(self.window,r,radius,self.dpi,self.glass_live){self.image(&edge,r,radius,1.);}}
        let edge=Rect::new(r.x+0.5,r.y+0.5,r.w-1.,r.h-1.);self.outline(edge,(radius-0.5).max(0.),((18.+30.*p.reflection)as u32)<<24|0xffffff,0.8);
    }
    fn music_lyric(&mut self,text:&str,r:Rect,size:f64,align:f64,ink:u32,s:&Scene<'_>){let weight=if s.music_mix>3.5&&size>=20.{500}else{400};let width=self.text.measure_weighted(text,size as f32,weight).min(r.w);let x=r.x+(r.w-width)*align;let phase=if s.animate{MotionSpec::token(s.motion_scheme,MotionKind::Effects,MotionSpeed::Fast).paced(s.motion_speed).sample(0.,1.,0.,s.lyric_age_ms/1000.).0.clamp(0.,1.)}else{1.};let y=r.y+4.*(1.-if s.animate{MotionSpec::token(s.motion_scheme,MotionKind::Spatial,MotionSpeed::Fast).paced(s.motion_speed).sample(0.,1.,0.,s.lyric_age_ms/1000.).0}else{1.});let boundary=x+width*s.highlight.clamp(0.,1.);
        for (clip,c) in [(Rect::new(r.x,r.y,(boundary-r.x).max(0.),r.h+5.),s.progress_color),(Rect::new(boundary,r.y,(r.x+r.w-boundary).max(0.),r.h+5.),ink)]{if clip.w>0.{self.clip(clip,0.,phase as f32);self.text.line_weighted(&mut self.canvas,text,vr(Rect::new(x,y,width,r.h)),size as f32,c,weight,0.);self.canvas.pop_layer();}}
    }
    fn compact_music(&mut self,s:&Scene<'_>){let tones=self.m3(s);let c=s.layout.capsule;let cover=s.layout.compact_cover(s.layout_scale);self.cover(cover,6.,s.cover_opacity as f32);let double=!s.compact_secondary.is_empty();let(h,line,second)=self.compact_metrics(s.font_size,double,s.layout_scale);let y=c.y+(c.h-h)/2.;let viewport=Rect::new(c.x+42.,y,c.w-76.,line);let phase=if s.animate{MotionSpec::token(s.motion_scheme,MotionKind::Effects,MotionSpeed::Fast).paced(s.motion_speed).sample(0.,1.,0.,s.lyric_age_ms/1000.).0.clamp(0.,1.)}else{1.};
        if phase<1.&&!s.previous_lyric.is_empty(){self.clip(Rect::new(viewport.x,c.y,viewport.w,c.h),0.,(1.-phase)as f32);self.text_at(s.previous_lyric,Rect::new(viewport.x,y-5.*phase,viewport.w,line),s.font_size,500,0xff000000|tones.on_surface,if double{0.5}else{0.});self.canvas.pop_layer();}
        self.music_lyric(s.lyric,viewport,s.font_size,if double{0.5}else{0.},0xff000000|tones.on_surface,s);
        if double{self.text_at(s.compact_secondary,Rect::new(viewport.x,y+line+1.,viewport.w,second),(s.font_size*0.78).clamp(9.,11.5),400,0xff000000|tones.on_surface_variant,0.5);}
        for(i,h)in [10.,14.,8.].iter().enumerate(){let scale=s.activity_scales[i];self.fill(Rect::new(c.x+c.w-22.+i as f64*4.,c.y+c.h/2.-h*scale/2.,2.,h*scale),1.,((255.*s.activity_opacity)as u32)<<24|(s.progress_color&0xffffff));}
    }
    fn compact_view(&mut self,s:&Scene<'_>){let mut compact=*s;compact.expressive_dark=true;let s=&compact;if s.mini{let tones=self.m3(s);let c=s.layout.capsule;let accent=if s.panel.is_some(){0xff000000|tones.primary}else{s.progress_color};let moving=s.panel.map(|p|p.running).unwrap_or(s.playing);let breath=if moving&&s.animate{0.68+0.32*(s.time_ms/2800.*std::f64::consts::TAU).sin()}else{0.8};let dot=3.2+breath*1.2;self.fill(Rect::new(c.x+11.-dot/2.,c.y+(c.h-dot)/2.,dot,dot),dot/2.,((255.*breath)as u32)<<24|(accent&0xffffff));let label=s.panel.map(|p|format!("{}  {}",p.title,p.value)).unwrap_or_else(||s.lyric.to_string());self.text_at(&label,Rect::new(c.x+22.,c.y,c.w-44.,c.h),crate::compact::mini_font_size(s.font_size),500,if s.mini_glass{s.mini_ink}else{0xff000000|tones.on_surface},0.5);return}
        if let Some(p)=s.panel{let tones=self.m3(s);let c=s.layout.capsule;let cy=c.y+c.h/2.;let accent=0xff000000|tones.primary;if p.kind==3{self.update_ring(c.x+22.,cy,10.,p.progress,s.time_ms,s.animate,accent);}else{self.icon(if p.kind==1{Icon::Focus}else if p.kind==2{Icon::Check}else{Icon::Import},Rect::new(c.x+12.,cy-10.,20.,20.),accent);}if p.kind==3{self.text_at(&p.title,Rect::new(c.x+42.,cy-16.,c.w-56.,19.),11.,600,0xff000000|tones.on_surface,0.);self.text_at(&p.subtitle,Rect::new(c.x+42.,cy+4.,c.w-56.,14.),9.,400,accent,0.);}else{self.text_at(&p.value,Rect::new(c.x+40.,cy-17.,c.w-70.,19.),14.,600,0xff000000|tones.on_surface,0.5);self.text_at(&p.title,Rect::new(c.x+40.,cy+3.,c.w-70.,13.),9.,400,accent,0.5);}if p.kind!=3{let a=if p.running&&s.animate{0.7+0.3*(s.time_ms/2400.*std::f64::consts::TAU).sin()}else{0.65};self.fill(Rect::new(c.x+c.w-18.,cy-3.,6.,6.),3.,((255.*a)as u32)<<24|(accent&0xffffff));}}else{let tones=self.m3(s);let mut music=*s;music.progress_color=s.progress_color;if s.lyric_cover_color{let mut accent=s.progress_color;for _ in 0..12{if crate::cover_color::contrast(accent,tones.container)>=3.{break}accent=0xff000000|crate::cover_color::mix(accent,tones.on_surface,0.15);}music.progress_color=accent;}self.compact_music(&music);}
    }
    fn button(&mut self,id:i32,r:Rect,s:&Scene<'_>){let disabled=([3,4,5].contains(&id)&&!s.has_track)||(id==2&&!s.floating)||(id==9&&!s.favorite_enabled)||(id==6&&!s.translation_button);let mut c=s.component_color;if disabled{c=0x4d000000|(c&0xffffff);}if id==6&&s.translation_enabled&&!disabled{self.fill(r,12.,0x30000000|(c&0xffffff));}let feedback=s.feedback.get(id as usize).copied().unwrap_or(0.);if feedback>0.001&&!disabled&&![3,5].contains(&id){self.fill(r,12.,((feedback*255.)as u32)<<24|0xffffff);}let pressed=s.press.get(id as usize).copied().unwrap_or(0.)as f64;let press=1.-pressed*0.04;
        // Skip controls are unfilled; only interaction feedback paints a subtle state layer.
        if [3,5].contains(&id)&&!disabled{c=0xff000000|dotisle_domain::music_controls::on_surface(self.paint_palette.background,c);if feedback>0.001{self.fill(r,12.,((feedback*255.)as u32)<<24|(c&0xffffff));}}let size=if id==4{24.}else{if [3,5].contains(&id){22.}else{20.}}*press;let icon=Rect::new(r.x+(r.w-size)/2.,r.y+(r.h-size)/2.,size,size);
        if id==4{
            let mix=if s.animate{s.play_shape.clamp(0.,1.)}else{if s.playing{1.}else{0.}};
            let inset=pressed*1.5;let body=Rect::new(r.x+inset,r.y+inset,r.w-inset*2.,r.h-inset*2.);
            self.fill(body,body.h*(0.5-0.22*if s.animate{s.play_shape.clamp(-0.08,1.10)}else{mix}),c);
            let ink=if crate::cover_color::contrast(c,0x17201d)>crate::cover_color::contrast(c,0xffffff){0xff17201d}else{0xffffffff};
            if mix<0.999{self.icon(Icon::Play,icon,(((1.-mix)*255.)as u32)<<24|(ink&0xffffff));}
            if mix>0.001{self.icon(Icon::Pause,icon,((mix*255.)as u32)<<24|(ink&0xffffff));}
            return
        }
        match id{3=>self.icon(Icon::Prev,icon,c),4=>self.icon(if s.playing{Icon::Pause}else{Icon::Play},icon,c),5=>self.icon(Icon::Next,icon,c),7=>self.icon(Icon::Settings,icon,c),2=>self.icon(Icon::Move,icon,c),6=>{if !disabled{self.icon(Icon::Translate,icon,c);}},9=>self.icon(Icon::Heart,icon,c),_=>{}}
    }
    fn music_expanded(&mut self,s:&Scene<'_>){let w=s.layout.card.w;let g=crate::layout::music_geometry(w,s.music_layout,s.music_mix,s.layout_scale,s.font_size);let secondary=0xb8000000|(s.text_color&0xffffff);let immersion=(s.music_mix-3.).clamp(0.,1.);if g.cover.w>0.&&immersion<0.999{self.clip(g.cover,0.,(1.-immersion)as f32);self.flipping_cover(g.cover,s);self.canvas.pop_layer();}
        self.text_at(s.title,g.title,16.-2.*immersion,600,0xff000000|(s.text_color&0xffffff),g.align);if !s.mini{self.text_at(s.artist,g.artist,10.,400,secondary,g.align);}
        if immersion>0.5{
            let size=(s.font_size+2.)+(s.font_size+6.).clamp(20.,23.)*immersion-(s.font_size+2.)*immersion;
            let chars=s.lyric.chars().collect::<Vec<_>>();let mut split=chars.len();
            if self.text.measure_weighted(s.lyric,size as f32,500)>g.lyric.w{let(mut low,mut high)=(1,chars.len());while low<high{let n=(low+high+1)/2;let part=chars[..n].iter().collect::<String>();if self.text.measure_weighted(&part,size as f32,500)<=g.lyric.w{low=n}else{high=n-1}}split=low;if let Some(i)=chars[..split].iter().rposition(|c|c.is_whitespace()){if i>split/2{split=i;}}}
            let first=chars[..split].iter().collect::<String>();let second=chars[split..].iter().collect::<String>();let rows=if second.trim().is_empty(){1}else{2};let line=30.;let context=(1.-s.controls_opacity)as f32;let next=if s.next_context!=s.secondary{s.next_context}else{""};let text=lyric_core::music_layout::immersive_text_geometry(g.height,rows,!s.previous_context.is_empty(),!s.secondary.is_empty(),!next.is_empty(),s.controls_opacity);let mut band=g.lyric;band.y=text.current_y;
            if context>0.001{self.clip(Rect::new(0.,0.,w,g.height),0.,context);self.text_line(s.previous_context,Rect::new(24.,text.previous_y,w-48.,18.),12.,400,0x60000000|(s.text_color&0xffffff));self.text_line(next,Rect::new(24.,text.next_y,w-48.,22.),14.,400,0x78000000|(s.text_color&0xffffff));self.canvas.pop_layer();}
            let mut lyric=*s;if rows==1{lyric.highlight=s.highlight;self.music_lyric(&first,Rect::new(band.x,band.y,band.w,line),size,0.,s.text_color,&lyric);}
            else{let weight=split as f64/chars.len().max(1)as f64;lyric.highlight=(s.highlight/weight.max(0.001)).clamp(0.,1.);self.music_lyric(&first,Rect::new(band.x,band.y,band.w,line),size,0.,s.text_color,&lyric);lyric.highlight=((s.highlight-weight)/(1.-weight).max(0.001)).clamp(0.,1.);self.music_lyric(second.trim(),Rect::new(band.x,band.y+line+2.,band.w,line),size,0.,s.text_color,&lyric);}
            self.text_at(s.secondary,Rect::new(g.secondary.x,text.secondary_y,g.secondary.w,18.),10.5,400,secondary,0.);
        }else{self.music_lyric(s.lyric,g.lyric,s.font_size+2.,g.align,s.text_color,s);}
        if !s.mini{if immersion<=0.5{self.text_at(s.secondary,g.secondary,10.5,400,secondary,g.align);}let status=if !s.has_track{"等待播放"}else if s.playing{"正在播放"}else{"已暂停"};if g.align>=0.5{let tw=self.text.measure_weighted(status,9.,400).min(g.status_text.w);let total=4.+6.+tw;let x=g.status_text.x+(g.status_text.w-total)*0.5;let dot=self.status_indicator(status,Rect::new(x,g.status_dot.y,4.,4.),Rect::new(x+10.,g.status_text.y,tw+2.,g.status_text.h));self.fill(dot,dot.w/2.,s.progress_color);self.text_line(status,Rect::new(x+10.,g.status_text.y,tw+2.,g.status_text.h),9.,400,secondary);}else{let dot=self.status_indicator(status,g.status_dot,g.status_text);self.fill(dot,dot.w/2.,s.progress_color);self.text_line(status,g.status_text,9.,400,secondary);}}
        if s.controls_opacity<=0.001{return}self.clip(Rect::new(0.,0.,w,g.height),0.,s.controls_opacity as f32);let rail=if s.seek_offer{0x2e000000|(s.component_color&0xffffff)}else{0x48000000|(s.component_color&0xffffff)};let mid=g.track.y+g.track.h/2.;let end=g.track.x+g.track.w*s.progress.clamp(0.,1.);
        let thumb=if s.pressed==10{14.}else{12.};
        self.line(((end+5.).min(g.track.x+g.track.w),mid),(g.track.x+g.track.w,mid),rail,3.);
        if end>g.track.x{
            let amount=if s.animate{s.play_shape.clamp(0.,1.)}else{0.};let mut path=BezPath::new();let length=end-g.track.x;let count=(length/3.).ceil().max(1.)as usize;
            for i in 0..=count{let x=g.track.x+length*i as f64/count as f64;let edge=((x-g.track.x)/10.).min((end-x)/10.).clamp(0.,1.);let y=mid+1.6*amount*edge*((x-g.track.x)/24.*std::f64::consts::TAU-s.time_ms/1000.*2.).sin();if i==0{path.move_to((x,y));}else{path.line_to((x,y));}}
            self.canvas.stroke(&Stroke::new(3.).with_caps(Cap::Round),Affine::IDENTITY,color(s.component_color),None,&path);
        }
        self.fill(Rect::new(end-1.5,mid-thumb/2.,3.,thumb),1.5,s.component_color);
        self.text_line(&time(s.elapsed),g.elapsed,9.,400,secondary);self.text_at(&if s.duration>0.{format!("−{}",time((s.duration-s.elapsed).max(0.)))}else{"--:--".into()},g.total,9.,400,secondary,1.);if s.seek_offer{let tones=self.m3(s);let r=g.seek_chip;if s.seek_confirm{let(cancel,confirm)=lyric_core::music_layout::seek_confirm_pair(r);let fb_cancel=s.feedback.get(14).copied().unwrap_or(0.);let fb_confirm=s.feedback.get(15).copied().unwrap_or(0.);self.fill(cancel,cancel.h/2.,0xff000000|tones.secondary_container);if fb_cancel>0.001{self.fill(cancel,cancel.h/2.,((fb_cancel*255.)as u32)<<24|(tones.on_secondary_container&0xffffff));}self.text_at("取消",cancel,10.5,600,0xff000000|tones.on_secondary_container,0.5);self.fill(confirm,confirm.h/2.,0xff000000|tones.primary);if fb_confirm>0.001{self.fill(confirm,confirm.h/2.,((fb_confirm*255.)as u32)<<24|(tones.on_primary&0xffffff));}self.text_at("重新连接",confirm,10.5,600,0xff000000|tones.on_primary,0.5);}else{let fb=s.feedback.get(14).copied().unwrap_or(0.);self.fill(r,r.h/2.,0xff000000|tones.secondary_container);if fb>0.001{self.fill(r,r.h/2.,((fb*255.)as u32)<<24|(tones.on_secondary_container&0xffffff));}self.text_at("重新连接进度",Rect::new(r.x,r.y,r.w,r.h),11.,600,0xff000000|tones.on_secondary_container,0.5);}}for(id,r)in g.buttons{if r.w>0.{self.button(id,r,s);}}if s.translation_tooltip&&s.hover==6&&s.translation_button{if let Some((_,r))=g.buttons.iter().find(|(id,_)|*id==6){let tw=128.;let x=(r.x+r.w/2.-tw/2.).clamp(12.,w-tw-12.);let tip=Rect::new(x,r.y-38.,tw,30.);self.fill(tip,10.,0xffe4ece7);self.text_at(if s.translation_enabled{"隐藏翻译 · 已开启"}else{"显示翻译 · 已关闭"},Rect::new(tip.x,tip.y+6.,tip.w,18.),11.,500,0xff19261e,0.5);}}self.canvas.pop_layer();
    }
    fn tool_button(&mut self,id:i32,r:Rect,label:&str,s:&Scene<'_>){self.m3_button(id,r,label,s,id==30)}
    fn tool_expanded(&mut self,s:&Scene<'_>,p:&crate::activities::Panel){self.expressive_tools(s,p)}
    fn expanded_view(&mut self,s:&Scene<'_>){let mut expanded=*s;expanded.mini=false;let s=&expanded;if let Some(p)=s.panel{self.tool_expanded(s,p)}else{self.music_expanded(s)}}
    fn satellite(&mut self,s:&Scene<'_>){let mut compact=*s;compact.expressive_dark=true;let s=&compact;let r=s.layout.satellite();let reveal=(1.-s.frame.card_reveal/0.06).clamp(0.,1.);if reveal<=0.{return}self.clip(Rect::new(r.x-24.,r.y-24.,r.w+48.,r.h+48.),0.,reveal as f32);let tones=self.m3(s);let mut surface=Self::surface_spec(s);surface.reveal=0.;surface.overlay=0.;self.canvas.draw_blurred_rounded_rect(Affine::translate((0.,5.)),vr(r),color(0x38000000),r.h/2.,22./3.);self.material_surface(r,r.h/2.,surface);let cx=r.x+r.w/2.;let cy=r.y+r.h/2.;let c=0xff000000|tones.on_secondary_container;
        // Secondary activity state is supplied by the owner; the icon never guesses from the primary panel.
        match s.auxiliary_kind{3=>{if s.auxiliary_update_completed{self.icon(Icon::Check,Rect::new(cx-9.,cy-9.,18.,18.),0xff9bd5ae);}else{self.update_ring(cx,cy,9.,s.auxiliary_update_progress,s.time_ms,s.animate&&s.auxiliary_running,0xff9bd5ae);}},1=>{self.ring(cx,cy,8.,c,1.4);let angle=if s.animate&&s.auxiliary_running{-std::f64::consts::FRAC_PI_2+s.auxiliary_elapsed/60000.*std::f64::consts::TAU}else{-std::f64::consts::FRAC_PI_2};self.line((cx,cy),(cx+angle.cos()*5.,cy+angle.sin()*5.),c,1.4);self.line((cx,cy),(cx+4.,cy+2.),c,1.4);},2=>self.icon(Icon::Check,Rect::new(cx-9.,cy-9.,18.,18.),c),_=>{for(i,base)in [8.,14.,10.].iter().enumerate(){let h=if s.animate&&s.auxiliary_running{6.+(s.time_ms/270.+i as f64*1.4).sin().abs()*9.}else{*base*0.65};self.fill(Rect::new(cx-6.+i as f64*5.,cy-h/2.,2.,h),1.,c);}}}self.canvas.pop_layer();
    }
    fn notification_content(&mut self,n:&crate::notifications::Notice,r:Rect,dark:bool){let tones=self.m3_cache.map(|(_,_,c)|c).unwrap_or_else(||dotisle_domain::dynamic_color::scheme(0x496b55,dark));let icon=Rect::new(r.x+18.,r.y+(r.h-40.)/2.,40.,40.);if let Some(kind)=n.app_id.strip_prefix("LyricIsland.Device."){let tones=self.m3_cache.map(|(_,_,c)|c).unwrap_or_else(||dotisle_domain::monet::scheme(0x496b55,dark));self.fill(icon,14.,0xff000000|tones.secondary_container);self.device_glyph(Rect::new(icon.x+7.,icon.y+7.,26.,26.),kind,0xff000000|tones.on_secondary_container);}
        else if let Some(im)=self.notification_icon.clone(){self.image(&im,icon,10.,1.);}else{self.fill(icon,10.,0xff3188ee);self.text_at(&n.app_name.chars().next().unwrap_or('•').to_string(),Rect::new(icon.x,icon.y+2.,40.,34.),22.,600,0xffffffff,0.5);}
        self.text_line(&n.app_name,Rect::new(r.x+76.,r.y+10.,r.w-160.,17.),11.5,500,(0xff000000|tones.on_surface_variant));self.text_line("现在",Rect::new(r.x+r.w-70.,r.y+12.,32.,14.),10.,400,(0xff000000|tones.on_surface_variant));self.text_line(&n.title,Rect::new(r.x+76.,r.y+27.,r.w-100.,21.),14.,600,(0xff000000|tones.on_surface));let lines=self.wrap(&n.body,12.5,r.w-100.);for(i,line)in lines.iter().take(2).enumerate(){self.text_line(line,Rect::new(r.x+76.,r.y+49.+i as f64*16.,r.w-100.,18.),12.5,400,(0xff000000|tones.on_surface_variant));}self.icon(Icon::Close,Rect::new(r.x+r.w-28.,r.y+11.,16.,16.),(0xff000000|tones.on_surface_variant));
    }
    fn wrap(&mut self,text:&str,size:f64,width:f64)->Vec<String>{let mut lines=vec![];let mut line=String::new();for c in text.chars(){let mut next=line.clone();next.push(c);if !line.is_empty()&&(c=='\n'||self.text.measure(&next,size as f32,false)>width){lines.push(std::mem::take(&mut line));}if c!='\n'{line.push(c);}}if !line.is_empty(){lines.push(line);}lines}
    pub unsafe fn notification_height(&mut self,n:&crate::notifications::Notice,width:f64)->Result<f64>{Ok(if n.body.trim().is_empty(){64.}else if self.wrap(&n.body,12.5,width-100.).len()<=1{80.}else{96.})}
    fn osd_content(&mut self,n:&crate::notifications::Notice,r:Rect,dark:bool){let roles=self.dynamic_resolved.map(|v|v.roles).unwrap_or_else(||dotisle_domain::dynamic_color::roles(0x496b55,true,false,false));let icon=Rect::new(r.x+12.,r.y+(r.h-30.)/2.,30.,30.);self.fill(icon,10.,0x1affffff);let kind=n.app_id.strip_prefix("LyricIsland.System.").unwrap_or("");let on=if kind=="Mic"{n.body=="已静音"}else{n.body=="已开启"};let tint=0xff000000|if kind=="Mic"&&on{roles.error}else if on{roles.primary}else{roles.on_surface_variant};self.device_glyph(Rect::new(icon.x+5.,icon.y+5.,20.,20.),if kind=="Mic"&&on{"MicOff"}else{kind},tint);self.text_line(&n.title,Rect::new(r.x+50.,r.y+13.,70.,18.),12.5,600,(0xff000000|roles.on_surface));self.text_line(&n.body,Rect::new(r.x+122.,r.y+14.,r.w-134.,15.),10.5,500,tint);}
    pub unsafe fn render(&mut self,s:&Scene<'_>,present:bool)->Result<()>{self.glass_active=false;self.glass_live=present&&!s.resizing&&!s.drag_active;self.glass_allowed=s.glass_refraction&&!s.dynamic_high_contrast;self.text.set_family(s.font_family);self.prepare_dynamic(s);let(mut themed,compact)=cover_scenes(s,Some(self.paint_palette));if s.expanded_cover_background{if let Some(resolved)=self.dynamic_resolved{themed.text_color=0xff000000|resolved.roles.on_surface;}}let s=&themed;self.canvas=VScene::new();if let Some(background)=self.glass.background(){self.image(&background,s.layout.host,0.,1.);}let(base,base_radius)=s.layout.morph_shape_with_radii(s.frame.card_reveal,s.capsule_radius,s.radius);let p=s.frame.card_reveal;let(shape,radius)=if s.notification.is_some(){(s.notification_rect,s.notification_radius)}else if s.osd.is_some(){(s.osd_rect,s.osd_radius)}else{(base,base_radius)};let music_alpha=if s.notification.is_some(){1.-s.notification_mix}else if s.osd.is_some(){1.-s.osd_mix}else{1.};self.clip(s.layout.host,0.,s.opacity);
        self.canvas.draw_blurred_rounded_rect(Affine::translate((0.,5.)),vr(shape),color(if s.mini&&p<0.01{0x0a000000}else{0x38000000}),radius,22./3.);
        let mut surface=Self::surface_spec(s);
        let immersion=(s.music_mix-3.).clamp(0.,1.);
        if s.panel.is_none()&&s.notification.is_none()&&s.osd.is_none()&&immersion>0.&&!s.dynamic_high_contrast{
            surface.material=Material::Acrylic;surface.cover_background=s.expanded_cover_background;surface.cover_overlay=s.cover_overlay;surface.card_frost=surface.card_frost+(64.-surface.card_frost)*immersion;surface.card_opacity=surface.card_opacity+(62.-surface.card_opacity)*immersion;
        }
        let saved_palette=self.paint_palette;if s.panel.is_some()||s.notification.is_some()||s.osd.is_some(){surface.cover_overlay=false;surface.card_opacity=0.;surface.cover_background=true;if let Some(r)=self.dynamic_resolved{self.paint_palette.background=r.roles.surface_container;}}
        self.material_surface(shape,radius,surface);self.paint_palette=saved_palette;
        let alpha=s.frame.content_opacity*blend(p,0.25,0.88)*music_alpha;if alpha>0.001{let part=self.collect(|a|{let w=s.layout.card.w;if s.drag_active{let mut neighbor=*s;neighbor.panel=s.drag_panel;let layer=a.collect(|a|a.expanded_view(&neighbor));a.canvas.append(&layer,Some(Affine::translate((s.swipe_offset-s.swipe_offset.signum()*w,s.frame.content_offset_y))));}if s.switching&&s.switch_outgoing{let mut old=*s;old.panel=s.previous_panel;let layer=a.collect(|a|a.expanded_view(&old));a.canvas.append(&layer,Some(Affine::translate((s.switch_offset-s.switch_direction*w,s.frame.content_offset_y))));}let layer=a.collect(|a|a.expanded_view(s));a.canvas.append(&layer,Some(Affine::translate((s.switch_offset+s.swipe_offset,s.frame.content_offset_y))));});let scale=(base.w/s.layout.card.w).min(base.h/s.layout.card.h);let tx=base.x+(base.w-s.layout.card.w*scale)/2.;let ty=base.y+(base.h-s.layout.card.h*scale)/2.;self.clip(shape,radius,alpha as f32);self.canvas.append(&part,Some(Affine::translate((tx,ty))*Affine::scale(scale)));self.canvas.pop_layer();}
        let alpha=(1.-blend(p,0.06,0.46))*music_alpha;if alpha>0.001{let part=self.collect(|a|{let w=s.layout.capsule.w;if s.drag_active{let mut neighbor=compact;neighbor.panel=s.drag_panel;let layer=a.collect(|a|a.compact_view(&neighbor));a.canvas.append(&layer,Some(Affine::translate((s.swipe_offset-s.swipe_offset.signum()*w,0.))));}if s.switching&&s.switch_outgoing{let mut old=compact;old.panel=s.previous_panel;let layer=a.collect(|a|a.compact_view(&old));a.canvas.append(&layer,Some(Affine::translate((s.switch_offset-s.switch_direction*w+s.swipe_offset,0.))));}let layer=a.collect(|a|a.compact_view(&compact));a.canvas.append(&layer,Some(Affine::translate((s.switch_offset+s.swipe_offset,0.))));});self.clip(shape,radius,alpha as f32);self.canvas.append(&part,Some(Affine::translate((base.x-s.layout.capsule.x,base.y-s.layout.capsule.y))));self.canvas.pop_layer();}
        if !s.mini&&s.component_count>1&&s.notification.is_none()&&s.osd.is_none(){self.satellite(&compact);}        if let Some(n)=s.notification{self.clip(shape,radius,s.notification_mix as f32);self.notification_content(n,shape,s.expressive_dark);self.canvas.pop_layer();}if let Some(n)=s.osd{self.clip(shape,radius,s.osd_mix as f32);self.osd_content(n,shape,s.expressive_dark);self.canvas.pop_layer();}self.canvas.pop_layer();
        let result=self.gpu.draw(&self.canvas,self.dpi as f64/96.,present).map_err(err);

        result
    }
    pub unsafe fn render_bottom_osd(&mut self,n:&crate::notifications::Notice,opacity:f64,offset:f64,surface:crate::surface::Surface,present:bool)->Result<()>{self.glass_live=present;self.glass_active=false;self.canvas=VScene::new();let r=Rect::new(16.,12.+offset,240.,44.);self.clip(Rect::new(0.,0.,272.,80.),0.,opacity as f32);self.material_surface(r,22.,surface);self.osd_content(n,r,true);self.canvas.pop_layer();
        let result=self.gpu.draw(&self.canvas,self.dpi as f64/96.,present).map_err(err);

        result
    }
    pub fn glass_captures(&self)->u64{self.glass.captures}
    pub fn resize(&mut self,width:u32,height:u32,dpi:f32){self.gpu.resize(width,height);self.width=width;self.height=height;self.dpi=dpi;}
    pub fn wait_idle(&self){self.gpu.wait_idle();}
    pub fn colors_moving(&self,now:f64)->bool{self.color_motion.moving(now)}
    pub unsafe fn snapshot(&self,path:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{self.gpu.snapshot(path)}
    /// The takeover window cannot rebuild Material roles, so the app hands it the window-sized
    /// material plus the roles, geometry and font that painted the island card.
    pub unsafe fn capture_update_appearance(&mut self,s:&Scene<'_>,directory:&Path,window:(i32,i32,i32,i32),job:&str)->std::result::Result<(),Box<dyn std::error::Error>>{
        self.text.set_family(s.font_family);self.prepare_dynamic(s);
        self.canvas=VScene::new();
        let mut surface=Self::surface_spec(s);
        let immersion=(s.music_mix-3.).clamp(0.,1.);
        if s.panel.is_none()&&s.notification.is_none()&&s.osd.is_none()&&immersion>0.&&!s.dynamic_high_contrast{
            surface.material=Material::Acrylic;surface.cover_background=s.expanded_cover_background;surface.cover_overlay=s.cover_overlay;surface.card_frost=surface.card_frost+(64.-surface.card_frost)*immersion;surface.card_opacity=surface.card_opacity+(62.-surface.card_opacity)*immersion;
        }surface.cover_overlay=false;surface.card_opacity=0.;surface.cover_background=true;if let Some(r)=self.dynamic_resolved{self.paint_palette.background=r.roles.surface_container;}self.material_surface(s.layout.card,s.radius,surface);
        self.gpu.draw(&self.canvas,self.dpi as f64/96.,false)?;
        std::fs::create_dir_all(directory)?;
        self.gpu.snapshot(&directory.join("background.png"))?;
        let c=self.m3(s);let hex=|value:u32|format!("#{:06x}",value&0xffffff);
        let exported=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
        let appearance=serde_json::json!({"version":1,"jobId":job,"exportedAtMs":exported,
            "scale":self.dpi as f64/96.,"window":{"x":window.0,"y":window.1,"width":window.2,"height":window.3},
            "card":{"x":s.layout.card.x,"y":s.layout.card.y,"width":s.layout.card.w,"height":s.layout.card.h},
            "radius":s.radius,"fontFamily":s.font_family,"animate":true,"timeMs":0.,
            "roles":{"primary":hex(c.primary),"onPrimary":hex(c.on_primary),"onSurface":hex(c.on_surface),"onSurfaceVariant":hex(c.on_surface_variant),"outline":hex(c.outline)}});
        std::fs::write(directory.join("appearance.json"),serde_json::to_vec_pretty(&appearance)?)?;
        Ok(())
    }
    pub unsafe fn verify_font(&mut self)->Result<serde_json::Value>{let samples=["晚风轻轻，音乐刚好 DotIsle 0123456789 译","景色は移ろい瞬く間に変わる","ひらがな カタカナ 日本語 한국어"];Ok(serde_json::json!({"renderer":"Vello + Parley","family":"Noto Sans SC","lineSpacing":self.body_line_spacing,"baseline":self.body_baseline,"samples":samples.map(|s|serde_json::json!({"text":s,"width":self.text.measure(s,16.,false)}))}))}
    pub unsafe fn icon_bounds(&self)->Result<serde_json::Value>{Ok(serde_json::json!({"renderer":"Vello paths","transportSize":24,"secondarySize":16}))}
}
fn time(s:f64)->String{let s=s.max(0.)as u64;format!("{:02}:{:02}",s/60,s%60)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Material{Acrylic,Glass}
impl Material{
    /// Both capsule modes open the same card and use the same material preference.
    pub fn expanded(settings:&serde_json::Value,_mini:bool)->Self{
        Self::from_setting(settings["IslandMaterial"].as_str().unwrap_or("Acrylic"))
    }
    pub fn from_setting(value:&str)->Self{match value{"Glass"=>Self::Glass,_=>Self::Acrylic}}
}
#[derive(Clone,PartialEq)]
pub struct MaterialTheme{pub glass_refraction:bool,
    pub roles:Option<dotisle_domain::dynamic_color::Roles>,pub material:Material,pub cover_background:bool,pub palette:crate::cover_color::CoverPalette,
    pub pixels:Option<(Vec<u8>,u32,u32)>,
}
impl MaterialTheme{
    pub fn new(material:Material,cover_background:bool)->Self{Self{glass_refraction:true,roles:None,material,cover_background,palette:Default::default(),pixels:None}}
}
#[derive(Clone,Copy)]
pub struct Scene<'a>{pub motion_scheme:MotionScheme,pub motion_speed:f64,pub press:[f32;64],pub glass_refraction:bool,
    pub play_shape:f64,pub capsule_radius:f64,pub update_phase:&'a str,pub feedback:[f32;64],pub dynamic_source:&'a str,pub dynamic_system_seed:Option<u32>,pub dynamic_custom_seed:u32,pub dynamic_high_contrast:bool,pub font_family:&'a str,pub expressive_dark:bool,pub mini:bool,pub mini_glass:bool,pub classic_capsule:Material,pub mini_capsule:Material,pub mini_ink:u32,pub mini_outline:u32,
    /// Hides the expanded card's 译 button in painting and in hit testing together.
    pub translation_button:bool,pub translation_enabled:bool,pub translation_tooltip:bool,
    pub tool_theme:crate::theme_accent::Accent,
    pub drag_panel:Option<&'a crate::activities::Panel>,pub drag_active:bool,pub switch_outgoing:bool,pub switch_direction:f64,
    pub panel:Option<&'a crate::activities::Panel>,pub previous_panel:Option<&'a crate::activities::Panel>,pub component_count:usize,pub auxiliary_update_progress:f64,pub auxiliary_update_completed:bool,pub auxiliary_kind:u8,pub auxiliary_running:bool,pub auxiliary_elapsed:f64,pub switching:bool,pub switch_offset:f64,pub swipe_offset:f64,
    pub material:Material,pub lyric_cover_color:bool,pub cover_background:bool,pub expanded_cover_background:bool,pub cover_overlay:bool,pub demo:bool,pub animate:bool,
    pub acrylic_card_frost:f64,pub acrylic_card_opacity:f64,pub acrylic_capsule_frost:f64,pub acrylic_capsule_opacity:f64,
    pub notification:Option<&'a crate::notifications::Notice>,pub notification_rect:Rect,pub notification_radius:f64,pub notification_mix:f64,
    pub osd:Option<&'a crate::notifications::Notice>,pub osd_rect:Rect,pub osd_radius:f64,pub osd_mix:f64,
    pub layout:Layout,pub frame:Frame,pub radius:f64,pub resizing:bool,pub time_ms:f64,
    pub previous_context:&'a str,pub next_context:&'a str,pub controls_opacity:f64,
    pub lyric:&'a str,pub previous_lyric:&'a str,pub compact_secondary:&'a str,pub previous_compact_secondary:&'a str,pub lyric_age_ms:f64,pub secondary:&'a str,
    pub title:&'a str,pub artist:&'a str,pub playing:bool,pub highlight:f64,
    /// The player reports a live timeline but cannot be seeked; the card offers an enable flow.
    pub seek_offer:bool,
    /// The offer is expanded into a clear restart/cancel decision.
    pub seek_confirm:bool,
    pub progress:f64,pub elapsed:f64,pub duration:f64,pub font_size:f64,
    pub music_layout:crate::layout::MusicLayout,pub music_mix:f64,
    pub text_color:u32,pub component_color:u32,pub progress_color:u32,
    pub hover:i32,pub pressed:i32,pub opacity:f32,
    pub transparent:bool,pub floating:bool,pub favorite_enabled:bool,pub has_track:bool,pub layout_scale:f64,
    pub activity_scales:[f64;3],pub activity_opacity:f64,pub cover_opacity:f64,pub cover_age_ms:f64,
    pub outgoing_opacity:f64,pub outgoing_y:f64,
}

/// Lyric highlighting follows its own preference, independently of surface tint.
pub(crate) fn cover_scenes<'a>(s:&Scene<'a>,palette:Option<crate::cover_color::CoverPalette>)->(Scene<'a>,Scene<'a>){
    let mut expanded=*s;let mut compact=*s;
    if let Some(palette)=palette{let accent=0xff000000|palette.accent;
        if s.expanded_cover_background{expanded.component_color=accent;}
        if s.cover_background{compact.component_color=accent;}
        if s.lyric_cover_color{expanded.progress_color=accent;compact.progress_color=accent;}
    }
    (expanded,compact)
}

include!("expressive.rs");

include!("dynamic_painter.rs");
