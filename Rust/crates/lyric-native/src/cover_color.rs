//! Monet (Material You) cover palette: Celebi quantization → Score seed → HCT tonal palette.
//! Runs once per cover change (never per frame) on the already-scaled 192px BGRA bitmap.
use material_colors::{color::Argb,hct::Hct,palette::TonalPalette,quantize::{Quantizer,QuantizerCelebi},score::Score};
pub const NEUTRAL_BACKGROUND:u32=0x1b1f25;
const BACKGROUND_TONE:i32=24;
const ACCENT_TONE:i32=80;
const NEUTRAL_CHROMA:f64=4.;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct CoverPalette{pub background:u32,pub accent:u32}
impl Default for CoverPalette{fn default()->Self{Self{background:NEUTRAL_BACKGROUND,accent:neutral_accent()}}}
/// Material Score alone decides suitability; its neutral fallback never invents a cover hue.
fn seed(bgra:&[u8])->Option<Hct>{
    let pixels:Vec<Argb>=bgra.chunks_exact(4).filter(|px|px[3]>=200).map(|px|{let alpha=px[3]as f64/255.;let straight=|v:u8|(v as f64/alpha).min(255.).round()as u8;Argb::new(255,straight(px[2]),straight(px[1]),straight(px[0]))}).collect();
    if pixels.is_empty(){return None}
    let quantized=QuantizerCelebi::quantize(&pixels,128);
    // The quantizer only emits opaque colours, so an alpha-0 fallback unambiguously means "nothing passed the filter".
    let sentinel=Argb::new(0,0,0,0);
    let chosen=*Score::score(&quantized.color_to_count,Some(1),Some(sentinel),Some(true)).first()?;
    if chosen==sentinel{None}else{Some(Hct::new(chosen))}
}
fn rgb(argb:Argb)->u32{(argb.red as u32)<<16|(argb.green as u32)<<8|argb.blue as u32}
fn neutral_accent()->u32{rgb(TonalPalette::from_hue_and_chroma(Hct::new(Argb::from_u32(0xff000000|NEUTRAL_BACKGROUND)).get_hue(),NEUTRAL_CHROMA).tone(88))}
fn readable(palette:&TonalPalette,background:u32)->u32{
    let mut tone=ACCENT_TONE;let mut accent=rgb(palette.tone(tone));
    while contrast(accent,background)<4.5&&tone<100{tone+=2;accent=rgb(palette.tone(tone));}
    accent
}
pub fn palette(bgra:&[u8])->CoverPalette{
    let Some(hct)=seed(bgra)else{return CoverPalette::default()};
    // Preserve the seed chroma instead of forcibly saturating muted art.
    let tonal=TonalPalette::from_hue_and_chroma(hct.get_hue(),hct.get_chroma());
    let background=rgb(tonal.tone(BACKGROUND_TONE));
    CoverPalette{background,accent:readable(&tonal,background)}
}
pub fn mix(a:u32,b:u32,t:f64)->u32 {
    let t=t.clamp(0.,1.);let ch=|shift:u32|{let x=((a>>shift)&255u32)as f64;let y=((b>>shift)&255u32)as f64;((x+(y-x)*t).round()as u32)<<shift};
    ch(16)|ch(8)|ch(0)
}
pub fn luminance(c:u32)->f64{let f=|shift:u32|{let v=((c>>shift)&255u32)as f64/255.;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}};0.2126*f(16)+0.7152*f(8)+0.0722*f(0)}
pub fn contrast(a:u32,b:u32)->f64{let a=luminance(a);let b=luminance(b);(a.max(b)+0.05)/(a.min(b)+0.05)}
#[cfg(test)]mod tests{use super::*;
    fn channels(c:u32)->(u32,u32,u32){(c>>16&255,c>>8&255,c&255)}
    #[test]fn white_missing_and_transparent_art_use_neutral_fallback(){for pixels in [vec![255u8;400],vec![],[0u8,0,0,0].repeat(300),[250u8,250,250,255].repeat(300)]{let p=palette(&pixels);assert_eq!(p.background,NEUTRAL_BACKGROUND);assert!(contrast(p.accent,p.background)>=4.5);}}
    #[test]fn neutral_accent_stays_nearly_gray(){let (r,g,b)=channels(CoverPalette::default().accent);assert!(r.abs_diff(b)<12&&g.abs_diff(b)<12&&r>200);}
    #[test]fn bright_colour_is_darkened_and_preserves_hue(){let p=palette(&[10,20,240,255].repeat(20));let (r,g,b)=channels(p.background);assert!(r>g&&r>b);assert!(r>=40&&r<=130&&luminance(p.background)<0.08);let (ar,_,ab)=channels(p.accent);assert!(ar>ab);}
    #[test]fn disabled_and_collapsed_keep_original(){assert_eq!(mix(0x1b1f25,0x481818,0.),0x1b1f25);}
    #[test]fn blue_and_green_subjects_keep_their_hue(){for (px,shift)in[([220u8,90,30,255],0u32),([30,220,60,255],8)]{let p=palette(&px.repeat(40));let c=p.background;assert!(c>>shift&255>=c>>16&255&&c>>shift&255>=c>>8&255&&c>>shift&255>=c&255);assert!(contrast(p.accent,c)>=4.5);}}
    #[test]fn orange_subject_beats_a_large_flat_gray_background(){let mut pixels=[48,48,48,255].repeat(900);for i in 0..100u8{pixels.extend_from_slice(&[10,80+i,180+i/2,255]);}let p=palette(&pixels);let (r,g,b)=channels(p.background);assert!(r>g&&g>b,"{:06x}",p.background);let (ar,_,ab)=channels(p.accent);assert!(ar>ab+50);assert!(contrast(p.accent,p.background)>=4.5);}
    #[test]fn colorful_noise_does_not_tint_monochrome_art(){let mut pixels=[60,60,60,255].repeat(1000);pixels.extend([0,0,255,255].repeat(10));assert_eq!(palette(&pixels).background,NEUTRAL_BACKGROUND);}
    #[test]fn every_hue_yields_readable_accent(){for hue in (0..360).step_by(15){let tonal=TonalPalette::from_hue_and_chroma(hue as f64,48.);let bg=rgb(tonal.tone(BACKGROUND_TONE));assert!(contrast(readable(&tonal,bg),bg)>=4.5,"hue {hue}");}}
    #[test]fn small_color_subject_is_left_to_material_score(){let mut pixels=[60,60,60,255].repeat(980);pixels.extend([0,80,240,255].repeat(20));let hct=seed(&pixels).expect("a two-percent subject passes Material Score");assert!(hct.get_chroma()>40.);assert_ne!(palette(&pixels).background,NEUTRAL_BACKGROUND);}
    #[test]fn every_pixel_can_contribute_instead_of_fixed_columns(){let mut pixels=[60,60,60,255].repeat(192*8);for row in 0..8{for x in (1..192).step_by(4){let i=(row*192+x)*4;pixels[i..i+4].copy_from_slice(&[220,90,30,255]);}}let p=palette(&pixels);let (r,_,b)=channels(p.background);assert!(b>r,"striped blue subject was missed: {:06x}",p.background);}
    #[test]fn muted_cover_is_not_forced_to_chroma_thirty_six(){let source=TonalPalette::from_hue_and_chroma(30.,12.).tone(55);let input=[source.blue,source.green,source.red,255].repeat(40);let p=palette(&input);let bg=Hct::new(Argb::from_u32(0xff000000|p.background));assert!(bg.get_chroma()<20.,"muted cover was oversaturated: {}",bg.get_chroma());assert!(contrast(p.accent,p.background)>=4.5);}
    #[test]fn full_cover_quantizes_quickly(){let mut pixels=Vec::with_capacity(192*192*4);for y in 0..192u32{for x in 0..192u32{pixels.extend_from_slice(&[(x*255/191)as u8,(y*255/191)as u8,((x+y)%256)as u8,255]);}}let start=std::time::Instant::now();let p=palette(&pixels);let elapsed=start.elapsed();eprintln!("monet palette {:?} -> {:06x}/{:06x}",elapsed,p.background,p.accent);assert!(contrast(p.accent,p.background)>=4.5);}
}
