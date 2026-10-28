//! Material You: Celebi -> Score -> HCT -> Material 3 Tonal Spot scheme.
use material_colors::{color::Argb, quantize::{Quantizer, QuantizerCelebi}, score::Score};
#[derive(Clone, Copy, Debug)]
pub struct Scheme {
    pub primary:u32, pub on_primary:u32, pub primary_container:u32, pub on_primary_container:u32,
    pub secondary_container:u32, pub on_secondary_container:u32, pub tertiary_container:u32,
    pub on_tertiary_container:u32, pub surface:u32, pub container:u32, pub high:u32,
    pub on_surface:u32, pub on_surface_variant:u32, pub outline:u32, pub outline_variant:u32, pub error:u32,
}
fn rgb(c:Argb)->u32{(c.red as u32)<<16|(c.green as u32)<<8|c.blue as u32}
pub fn seed(rgba:&[u8])->Option<u32>{
    let pixels:Vec<Argb>=rgba.chunks_exact(4).filter(|p|p[3]>=200).map(|p|Argb::new(255,p[0],p[1],p[2])).collect();
    if pixels.is_empty(){return None}
    let quantized=QuantizerCelebi::quantize(&pixels,128);
    let sentinel=Argb::new(0,0,0,0);
    let chosen=*Score::score(&quantized.color_to_count,Some(1),Some(sentinel),Some(true)).first()?;
    (chosen!=sentinel).then(||rgb(chosen))
}
pub use crate::dynamic_color::scheme;
pub fn contrast(a:u32,b:u32)->f64{fn l(c:u32)->f64{let f=|shift:u32|{let v=((c>>shift)&255u32)as f64/255.;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}};f(16)*0.2126+f(8)*0.7152+f(0)*0.0722}let(a,b)=(l(a),l(b));(a.max(b)+0.05)/(a.min(b)+0.05)}
#[cfg(test)] mod tests {use super::*;
    #[test] fn transparent_and_neutral_covers_fall_back(){assert_eq!(seed(&[]),None);assert_eq!(seed(&[255,255,255,0].repeat(100)),None);assert_eq!(seed(&[240,240,240,255].repeat(100)),None);}
    #[test] fn artwork_hue_drives_both_themes_with_readable_pairs(){let red=seed(&[240,40,20,255].repeat(100)).unwrap();let blue=seed(&[20,60,240,255].repeat(100)).unwrap();assert_ne!(red,blue);for seed in [red,blue,0x496b55,0x8f5d7a,0xa46c36]{for dark in [true,false]{let s=scheme(seed,dark);for(a,b)in [(s.primary,s.on_primary),(s.primary_container,s.on_primary_container),(s.surface,s.on_surface),(s.container,s.on_surface_variant)]{assert!(contrast(a,b)>=4.5,"{a:x}/{b:x}");}}}}
}

