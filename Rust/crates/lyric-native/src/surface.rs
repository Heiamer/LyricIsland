//! Shared material geometry for expanded cards, notification banners and system hints.
use crate::{cover_color::{self,CoverPalette},motion::blend,renderer::Material};

#[derive(Clone,Copy)]
pub struct Surface{
    pub material:Material,pub reveal:f64,pub overlay:f64,pub compact_glass:bool,
    pub mini:bool,pub transparent:bool,pub cover_background:bool,
}
pub struct Paint{pub top:u32,pub bottom:u32,pub reflection:f64,pub frost:f64}
impl Surface{
    pub fn expanded(material:Material,cover_background:bool)->Self{
        Self{material,cover_background,reveal:1.,overlay:0.,compact_glass:false,mini:false,transparent:false}
    }
    pub fn paint(self,palette:CoverPalette)->Paint{
        // A transient hint can appear while the music card is collapsed. Its own reveal,
        // rather than the music card's reveal, must bring in the selected expanded material.
        let amount=blend(self.reveal,0.05,0.92).max(self.overlay.clamp(0.,1.));
        let compact_alpha=if self.compact_glass{if self.mini{104.}else{196.}}else if self.transparent{245.}else{255.};
        let expanded_alpha=if self.material==Material::Glass{224.}else if self.transparent{245.}else{255.};
        let alpha=((compact_alpha+(expanded_alpha-compact_alpha)*amount).round()as u32)<<24;
        let tint=if self.cover_background{amount}else{0.};
        let top=cover_color::mix(0x1b1f25,palette.background,tint);
        let bottom=cover_color::mix(0x0c0f14,palette.background,tint*0.65);
        let reflection=if self.compact_glass{1.-amount}else{0.}+if self.material==Material::Glass{amount}else{0.};
        let top=cover_color::mix(top,0xdfefff,0.08*reflection);
        let frost=match self.material{Material::Classic=>0.,Material::Acrylic=>amount,Material::Glass=>amount*0.28};
        Paint{top:alpha|top,bottom:alpha|bottom,reflection,frost}
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn hints_use_the_same_material_as_the_expanded_card_even_when_music_is_collapsed(){
        for mini in [false,true]{for material in [Material::Classic,Material::Acrylic,Material::Glass]{
            let expanded=Surface::expanded(material,true).paint(CoverPalette::default());
            let hint=Surface{mini,reveal:0.,overlay:1.,compact_glass:false,..Surface::expanded(material,true)}.paint(CoverPalette::default());
            assert_eq!((hint.top,hint.bottom,hint.frost,hint.reflection),(expanded.top,expanded.bottom,expanded.frost,expanded.reflection));
        }}
    }
    #[test]fn aero_glass_has_a_denser_reading_plane_and_retains_cover_hue(){
        let glass=Surface::expanded(Material::Glass,true);
        let red=glass.paint(CoverPalette{background:0x681c12,accent:0xffb0a0});
        let blue=glass.paint(CoverPalette{background:0x123568,accent:0xa0c0ff});
        assert_eq!(red.top>>24,224);assert!(red.top>>24>168);
        assert!(red.top>>16&255>red.top&255);assert!(blue.top&255>blue.top>>16&255);
        assert!(red.frost>0.&&red.reflection>0.);
        let neutral=Surface{cover_background:false,..glass};
        assert_eq!(neutral.paint(CoverPalette{background:0x681c12,accent:0xffb0a0}).top,neutral.paint(CoverPalette{background:0x123568,accent:0xa0c0ff}).top);
    }
    #[test]fn switching_expanded_material_does_not_change_the_collapsed_strip(){
        for mini in [false,true]{for compact_glass in [false,true]{
            let surfaces=[Material::Classic,Material::Acrylic,Material::Glass].map(|material|Surface{material,mini,compact_glass,reveal:0.,..Surface::expanded(material,true)}.paint(CoverPalette::default()));
            for surface in &surfaces[1..]{assert_eq!((surface.top,surface.bottom,surface.frost),(surfaces[0].top,surfaces[0].bottom,surfaces[0].frost));}
        }}
    }
}
