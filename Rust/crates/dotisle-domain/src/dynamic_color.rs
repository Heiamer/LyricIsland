//! One source-resolution and Material Dynamic Color role pipeline for every Vello surface.
use std::cell::RefCell;
use material_colors::{color::Argb,dynamic_color::{DynamicScheme,Variant}};
use crate::monet::Scheme;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Source{Content,System,Custom}
impl Source{pub fn parse(v:&str)->Self{match v{"System"=>Self::System,"Custom"=>Self::Custom,_=>Self::Content}}pub fn key(self)->&'static str{match self{Self::Content=>"Content",Self::System=>"System",Self::Custom=>"Custom"}}}
fn rgb(c:Argb)->u32{(c.red as u32)<<16|(c.green as u32)<<8|c.blue as u32}
macro_rules! roles{($($name:ident),*$(,)?)=>{
 #[derive(Clone,Copy,Debug,PartialEq)]pub struct Roles{$(pub $name:u32,)*}
 impl Roles{fn from(s:&DynamicScheme)->Self{Self{$($name:rgb(s.$name()),)*}}
 pub fn mix(self,other:Self,t:f64)->Self{Self{$($name:mix_rgb(self.$name,other.$name,t),)*}}
}
}}
roles!(primary,on_primary,primary_container,on_primary_container,secondary,on_secondary,secondary_container,on_secondary_container,tertiary,on_tertiary,tertiary_container,on_tertiary_container,error,on_error,error_container,on_error_container,surface,on_surface,surface_dim,surface_bright,surface_container_lowest,surface_container_low,surface_container,surface_container_high,surface_container_highest,on_surface_variant,outline,outline_variant,inverse_surface,inverse_on_surface,inverse_primary,scrim,surface_tint,primary_fixed,primary_fixed_dim,on_primary_fixed,on_primary_fixed_variant,secondary_fixed,secondary_fixed_dim,on_secondary_fixed,on_secondary_fixed_variant,tertiary_fixed,tertiary_fixed_dim,on_tertiary_fixed,on_tertiary_fixed_variant);
impl Roles{pub fn ui(self)->Scheme{Scheme{primary:self.primary,on_primary:self.on_primary,primary_container:self.primary_container,on_primary_container:self.on_primary_container,secondary_container:self.secondary_container,on_secondary_container:self.on_secondary_container,tertiary_container:self.tertiary_container,on_tertiary_container:self.on_tertiary_container,surface:self.surface,container:self.surface_container_low,high:self.surface_container_high,on_surface:self.on_surface,on_surface_variant:self.on_surface_variant,outline:self.outline,outline_variant:self.outline_variant,error:self.error}}}
#[derive(Clone,Copy,Debug)]pub struct Resolved{pub requested:Source,pub used:Source,pub seed:u32,pub fallback:bool,pub roles:Roles}
thread_local!{static CACHE:RefCell<Vec<((u32,bool,bool,bool),Roles)>>=const{RefCell::new(Vec::new())};}
pub fn roles(seed:u32,dark:bool,content:bool,high_contrast:bool)->Roles{
 let key=(seed&0xffffff,dark,content,high_contrast);
 CACHE.with(|cache|{let mut cache=cache.borrow_mut();if let Some((_,roles))=cache.iter().find(|(k,_)|*k==key){return *roles}
  let s=DynamicScheme::by_variant(Argb::from_u32(0xff000000|seed),&if content{Variant::Content}else{Variant::TonalSpot},dark,Some(if high_contrast{0.5}else{0.}));let r=Roles::from(&s);if cache.len()>=24{cache.remove(0);}cache.push((key,r));r
 })
}
pub fn resolve(requested:Source,content:Option<u32>,system:Option<u32>,custom:u32,dark:bool,high_contrast:bool)->Resolved{
 // Missing/neutral artwork falls back to the user's own saved colour, never an unrelated hue.
 let(seed,used)=match requested{Source::Content=>content.map(|c|(c,Source::Content)).unwrap_or((custom,Source::Custom)),Source::System=>system.map(|c|(c,Source::System)).unwrap_or((custom,Source::Custom)),Source::Custom=>(custom,Source::Custom)};
 Resolved{requested,used,seed:seed&0xffffff,fallback:requested!=used,roles:roles(seed,dark,used==Source::Content,high_contrast)}
}
/// Centre surfaces use restrained Tonal Spot roles; artwork hue stays recognizable.
/// Content fidelity belongs to artwork/lyrics, not large application panels.
pub fn resolve_center(requested:Source,content:Option<u32>,system:Option<u32>,custom:u32,dark:bool,high:bool)->Resolved{
 let mut r=resolve(requested,content,system,custom,dark,high);
 if r.used==Source::Content{r.roles=roles(r.seed,dark,false,high);}r
}
pub fn scheme(seed:u32,dark:bool)->Scheme{roles(seed,dark,false,false).ui()}
/// Interruptible 200 ms source transition. Roles are regenerated together at every sampled seed.
#[derive(Default,Clone,Copy,Debug)]pub struct SeedMotion{from:u32,target:u32,at:f64,ready:bool}
impl SeedMotion{
 pub fn update(&mut self,target:u32,now:f64,animate:bool)->bool{if !self.ready||!animate{self.from=target;self.target=target;self.at=now-1.;self.ready=true;return false}if target!=self.target{self.from=self.sample(now);self.target=target;self.at=now;}self.moving(now)}
 pub fn moving(&self,now:f64)->bool{self.ready&&self.from!=self.target&&now-self.at<0.2}
 pub fn sample(&self,now:f64)->u32{if self.ready&&now<=self.at{return self.from}if !self.moving(now){return self.target}let t=lyric_core::motion_curve::sample(lyric_core::motion_curve::EMPHASIZED,((now-self.at)/0.2).clamp(0.,1.)).0;let a=material_colors::hct::Hct::new(Argb::from_u32(0xff000000|self.from));let b=material_colors::hct::Hct::new(Argb::from_u32(0xff000000|self.target));let dh=(b.get_hue()-a.get_hue()+540.).rem_euclid(360.)-180.;rgb(material_colors::hct::Hct::from(a.get_hue()+dh*t,a.get_chroma()+(b.get_chroma()-a.get_chroma())*t,a.get_tone()+(b.get_tone()-a.get_tone())*t).into())}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn all_sources_resolve_with_explicit_stable_fallbacks(){let a=resolve(Source::Content,None,Some(0xff0000),0x426b83,false,false);assert_eq!(a.used,Source::Custom);assert_eq!(a.seed,0x426b83);assert!(a.fallback);let b=resolve(Source::System,Some(0x123456),Some(0x654321),0x426b83,false,false);assert_eq!(b.seed,0x654321);assert!(!b.fallback);}
 #[test]fn dynamic_role_pairs_keep_readable_contrast(){for seed in [0,0xffffff,0xffff00,0xff0000,0x00ff00,0x0000ff,0x808080,0x496b55]{for dark in [false,true]{for content in [false,true]{for high in [false,true]{let r=roles(seed,dark,content,high);assert!(crate::monet::contrast(r.outline,r.surface)>=2.95);for(a,b)in [(r.primary,r.on_primary),(r.primary_container,r.on_primary_container),(r.surface,r.on_surface),(r.surface_container_high,r.on_surface_variant),(r.error,r.on_error)]{assert!(crate::monet::contrast(a,b)>=4.45,"{seed:06x}/{dark}/{content}/{high}: {a:06x}/{b:06x}");}}}}}}
 #[test]fn complete_roles_are_cached_consistently(){let a=roles(0x345678,true,true,true);let b=roles(0x345678,true,true,true);assert_eq!(a.primary,b.primary);assert_ne!(a.surface_container_lowest,a.surface_container_highest);}
 #[test]fn colour_motion_is_interruptible_and_reduced_motion_is_immediate(){let mut m=SeedMotion::default();m.update(0x426b83,0.,false);m.update(0xa26942,1.,true);let mid=m.sample(1.08);assert_ne!(mid,0x426b83);assert_ne!(mid,0xa26942);m.update(0x6860a5,1.08,true);assert_eq!(m.sample(1.08),mid);m.update(0x496b55,1.1,false);assert_eq!(m.sample(1.1),0x496b55);assert!(!m.moving(1.1));}
}

// Interpolate already-resolved roles. HCT solving must never run once per frame.
fn mix_rgb(a:u32,b:u32,t:f64)->u32{let c=|shift:u32|{let a=((a>>shift)&255)as f64;let b=((b>>shift)&255)as f64;(a+(b-a)*t).round().clamp(0.,255.)as u32};(c(16)<<16)|(c(8)<<8)|c(0)}
#[derive(Default)]pub struct RoleMotion{from:Option<Roles>,target:Option<Roles>,at:f64}
impl RoleMotion{
 pub fn update(&mut self,roles:Roles,now:f64,animate:bool)->bool{
  if self.target.is_none()||!animate||self.target.is_some_and(|old|crate::monet::contrast(old.on_surface,roles.on_surface)>4.){self.from=Some(roles);self.target=Some(roles);self.at=now-1.;return false}
  let old=self.target.unwrap();
  if old.primary!=roles.primary||old.surface!=roles.surface||old.secondary!=roles.secondary||old.on_surface!=roles.on_surface||old.outline!=roles.outline{self.from=Some(self.sample(now));self.target=Some(roles);self.at=now;}
  self.moving(now)
 }
 pub fn sample(&self,now:f64)->Roles{let target=self.target.expect("roles initialized");if !self.moving(now){return target}let t=((now-self.at)/0.2).clamp(0.,1.);self.from.unwrap_or(target).mix(target,1.-(1.-t).powi(3))}
 pub fn moving(&self,now:f64)->bool{now-self.at<0.2&&self.target.is_some()}
}

#[cfg(test)]mod role_motion_tests{
 use super::*;
 #[test]fn retargets_the_live_roles_without_rebuilding_a_scheme(){let mut m=RoleMotion::default();let a=roles(0x536da5,true,false,false);let b=roles(0xa45c37,true,false,false);let c=roles(0x39a46a,true,false,false);m.update(a,0.,false);m.update(b,1.,true);let mid=m.sample(1.08);m.update(c,1.08,true);assert_eq!(m.sample(1.08),mid);assert_eq!(m.sample(2.),c);}
 #[test]fn theme_change_cannot_crossfade_foreground_through_the_background(){let mut m=RoleMotion::default();m.update(roles(0x536da5,false,false,false),0.,false);let dark=roles(0x536da5,true,false,false);m.update(dark,1.,true);assert_eq!(m.sample(1.01),dark);assert!(crate::monet::contrast(m.sample(1.01).surface,m.sample(1.01).on_surface)>=4.5);}
}

#[cfg(test)]mod centre_tests{use super::*;
 #[test]fn artwork_panels_use_restrained_roles_and_readable_pairs(){for seed in [0xff00ff,0xff0000,0x00ff00,0x0000ff,0xffff00]{for dark in [false,true]{let r=resolve_center(Source::Content,Some(seed),None,0x345678,dark,false);assert_eq!(r.seed,seed);assert_eq!(r.roles,roles(seed,dark,false,false));let h=material_colors::hct::Hct::new(Argb::from_u32(0xff000000|r.roles.primary_container));assert!(h.get_chroma()<43.);assert!(crate::monet::contrast(r.roles.primary_container,r.roles.on_primary_container)>=4.5);}}}
 #[test]fn explicit_colours_and_island_fidelity_remain_independent(){let seed=0xe600ff;let manual=resolve_center(Source::Custom,None,None,seed,false,false);assert_eq!(manual.roles,resolve(Source::Custom,None,None,seed,false,false).roles);assert_ne!(resolve(Source::Content,Some(seed),None,seed,false,false).roles,resolve_center(Source::Content,Some(seed),None,seed,false,false).roles);}
}
