//! Transport colours are paired against their actual filled target, including state layers.
use crate::monet::contrast;
#[derive(Clone,Copy,Debug)]pub struct SkipColors{pub background:u32,pub foreground:u32}
fn mix(a:u32,b:u32,t:f64)->u32{let c=|s:u32|{let a=((a>>s)&255)as f64;let b=((b>>s)&255)as f64;(a+(b-a)*t).round()as u32};c(16)<<16|c(8)<<8|c(0)}
pub fn skip_colors(surface:u32,preferred:u32)->SkipColors{
 let background=mix(surface,preferred,0.14);
 let worst=|ink:u32|[0.,0.08,0.12,0.16].into_iter().map(|a|contrast(ink,mix(background,ink,a))).fold(f64::INFINITY,f64::min);
 let foreground=if contrast(preferred,background)>=4.5&&worst(preferred)>=3.{preferred&0xffffff}else if contrast(0xffffff,background)>=4.5&&(contrast(0,background)<4.5||worst(0xffffff)>worst(0)){0xffffff}else{0};
 SkipColors{background,foreground}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn screenshot_regressions_and_custom_colours_stay_readable(){for(bg,ink)in[(0x1d2e33,0x41494c),(0x7d2b85,0x514158),(0x351a35,0xf6a9fb),(0xf5e5dc,0xffeeee),(0x050505,0x101010)]{let c=skip_colors(bg,ink);for state in [0.,0.08,0.12,0.16]{assert!(contrast(c.foreground,mix(c.background,c.foreground,state))>=3.,"{bg:06x}/{ink:06x}/{state}");}assert!(contrast(c.foreground,c.background)>=4.5);}}
 #[test]fn material_and_cover_colours_keep_legible_skip_targets(){for seed in [0,0xffffff,0xff00ff,0xffff00,0x00ff00,0x0000ff,0x1d2e33,0x9135a7]{for dark in [false,true]{for content in [false,true]{let r=crate::dynamic_color::roles(seed,dark,content,false);let c=skip_colors(r.primary_container,r.on_primary_container);assert!(contrast(c.foreground,c.background)>=4.5,"{seed:06x}/{dark}/{content}");for state in [0.,0.08,0.12,0.16]{assert!(contrast(c.foreground,mix(c.background,c.foreground,state))>=3.);}}}}}
}
