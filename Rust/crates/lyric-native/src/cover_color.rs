//! Sample once per cover; ignore near-white/transparent pixels and keep white text legible.
pub fn dominant(bgra:&[u8])->u32 {
    let mut bins=[[0u64;4];512];
    for px in bgra.chunks_exact(4).step_by(4) {
        if px[3]<200{continue}
        let (r,g,b)=(px[2] as u32,px[1] as u32,px[0] as u32);
        if r.min(g).min(b)>210 || r.max(g).max(b)<24 {continue}
        let bin=((r>>5)*64+(g>>5)*8+(b>>5))as usize;
        bins[bin][0]+=1;bins[bin][1]+=r as u64;bins[bin][2]+=g as u64;bins[bin][3]+=b as u64;
    }
    let bin=bins.iter().max_by_key(|b|b[0]).unwrap();
    if bin[0]==0{return 0x1b1f25}
    let c=[bin[1]/bin[0],bin[2]/bin[0],bin[3]/bin[0]];
    let scale=72.0/(*c.iter().max().unwrap()).max(1)as f64;
    let ch=|n:u64|((n as f64*scale.min(1.0)).round()as u32).max(12);
    ch(c[0])<<16|ch(c[1])<<8|ch(c[2])
}
pub fn mix(a:u32,b:u32,t:f64)->u32 {
    let t=t.clamp(0.,1.);let ch=|shift:u32|{let x=((a>>shift)&255u32)as f64;let y=((b>>shift)&255u32)as f64;((x+(y-x)*t).round()as u32)<<shift};
    ch(16)|ch(8)|ch(0)
}
pub fn luminance(c:u32)->f64{let f=|shift:u32|{let v=((c>>shift)&255u32)as f64/255.;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}};0.2126*f(16)+0.7152*f(8)+0.0722*f(0)}
pub fn contrast(a:u32,b:u32)->f64{let a=luminance(a);let b=luminance(b);(a.max(b)+0.05)/(a.min(b)+0.05)}
pub fn accent(background:u32)->u32{let mut c=mix(background,0xffffff,0.72);while contrast(c,background)<4.5&&c!=0xffffff{c=mix(c,0xffffff,0.12);}c}
#[cfg(test)]mod tests{use super::*;
    #[test]fn white_and_missing_art_use_dark_fallback(){assert_eq!(dominant(&[255;400]),0x1b1f25);assert_eq!(dominant(&[]),0x1b1f25);}
    #[test]fn bright_colour_is_darkened_and_preserves_hue(){let c=dominant(&[10,20,240,255].repeat(20));assert_eq!(c>>16,72);assert!((c&255u32)<30);}
    #[test]fn disabled_and_collapsed_keep_original(){assert_eq!(mix(0x1b1f25,0x481818,0.),0x1b1f25);}
}

#[cfg(test)]mod palette_tests{use super::*;#[test]fn components_remain_readable_for_every_dark_cover_bin(){for r in (12u32..=72).step_by(12){for g in (12u32..=72).step_by(12){for b in (12u32..=72).step_by(12){let bg=r<<16|g<<8|b;assert!(contrast(accent(bg),bg)>=4.5);}}}}}
