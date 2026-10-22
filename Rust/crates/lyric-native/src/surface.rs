//! Shared material geometry for expanded cards, notification banners and system hints.
use crate::{cover_color::{self,CoverPalette},motion::blend,renderer::Material};

#[derive(Clone,Copy)]
pub struct Surface{
    /// Material of the expanded card.
    pub material:Material,
    /// Material of the collapsed strip. Kept separate so the classic capsule can be
    /// acrylic (or classic, or glass) independently of what the card opens into.
    pub compact:Material,
    pub reveal:f64,pub overlay:f64,
    pub mini:bool,pub transparent:bool,pub cover_background:bool,
}
/// `amount` is the live expansion progress: 0 collapsed, 1 fully expanded.
pub struct Paint{pub top:u32,pub bottom:u32,pub reflection:f64,pub frost:f64,pub amount:f64}
impl Surface{
    pub fn expanded(material:Material,cover_background:bool)->Self{
        Self{material,compact:material,cover_background,reveal:1.,overlay:0.,mini:false,transparent:false}
    }
    pub fn paint(self,palette:CoverPalette)->Paint{
        // A transient hint can appear while the music card is collapsed. Its own reveal,
        // rather than the music card's reveal, must bring in the selected expanded material.
        let amount=blend(self.reveal,0.05,0.92).max(self.overlay.clamp(0.,1.));
        // The strip is painted from `compact` and hands over to `material` as it opens,
        // so both ends stay exact while everything in between is a plain blend.
        let compact_alpha=match self.compact{
            Material::Glass=>if self.mini{104.}else{196.},
            _ if self.transparent=>245.,
            _=>255.,
        };
        let expanded_alpha=if self.material==Material::Glass{224.}else if self.transparent{245.}else{255.};
        let alpha=((compact_alpha+(expanded_alpha-compact_alpha)*amount).round()as u32)<<24;
        let tint=if self.cover_background{amount}else{0.};
        let top=cover_color::mix(0x1b1f25,palette.background,tint);
        let bottom=cover_color::mix(0x0c0f14,palette.background,tint*0.65);
        let reflection=(if self.compact==Material::Glass{1.-amount}else{0.})+(if self.material==Material::Glass{amount}else{0.});
        let top=cover_color::mix(top,0xdfefff,0.08*reflection);
        // Acrylic frosts the whole plane, so a capsule set to acrylic reads exactly like an
        // acrylic card instead of falling back to the flat classic strip.
        let frost=match self.material{Material::Classic=>0.,Material::Acrylic=>amount,Material::Glass=>amount*0.28}
            +(if self.compact==Material::Acrylic{1.-amount}else{0.});
        Paint{top:alpha|top,bottom:alpha|bottom,reflection,frost,amount}
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn hints_use_the_same_material_as_the_expanded_card_even_when_music_is_collapsed(){
        for mini in [false,true]{for material in [Material::Classic,Material::Acrylic,Material::Glass]{
            let expanded=Surface::expanded(material,true).paint(CoverPalette::default());
            let hint=Surface{mini,reveal:0.,overlay:1.,compact:Material::Acrylic,..Surface::expanded(material,true)}.paint(CoverPalette::default());
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
        for mini in [false,true]{for compact in [Material::Classic,Material::Acrylic,Material::Glass]{
            let surfaces=[Material::Classic,Material::Acrylic,Material::Glass].map(|material|Surface{material,compact,mini,reveal:0.,..Surface::expanded(material,true)}.paint(CoverPalette::default()));
            for surface in &surfaces[1..]{assert_eq!((surface.top,surface.bottom,surface.frost),(surfaces[0].top,surfaces[0].bottom,surfaces[0].frost));}
        }}
    }
    #[test]fn an_acrylic_strip_reads_like_the_acrylic_card(){
        // The collapsed strip must borrow the expanded card's acrylic treatment, not invent
        // a second look: same opaque plane, same full-strength frost, no glass reflection.
        let strip=Surface{compact:Material::Acrylic,reveal:0.,..Surface::expanded(Material::Classic,false)}.paint(CoverPalette::default());
        let card=Surface::expanded(Material::Acrylic,false).paint(CoverPalette::default());
        let classic=Surface{compact:Material::Classic,reveal:0.,..Surface::expanded(Material::Classic,false)}.paint(CoverPalette::default());
        assert_eq!(strip.frost,card.frost);
        assert!(strip.frost>classic.frost);
        assert_eq!((strip.top,strip.bottom,strip.reflection),(card.top,card.bottom,card.reflection));
    }
}
