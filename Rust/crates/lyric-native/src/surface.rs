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
    pub mini:bool,pub transparent:bool,pub cover_background:bool,pub compact_cover_background:bool,pub cover_overlay:bool,
    /// Frosted artwork layer for acrylic, per surface: frost strength and layer opacity in
    /// percent. The card and the collapsed strip keep independent pairs; the strip defaults to
    /// zero so the island body looks unchanged until the sliders are moved.
    pub card_frost:f64,pub card_opacity:f64,pub capsule_frost:f64,pub capsule_opacity:f64,
}
/// `amount` is the live expansion progress: 0 collapsed, 1 fully expanded.
pub struct Paint{pub top:u32,pub bottom:u32,pub reflection:f64,pub frost:f64,pub amount:f64,pub cover_alpha:f64,pub cover_blur:f64,pub tint:f64}
impl Surface{
    pub fn expanded(material:Material,cover_background:bool)->Self{
        Self{material,compact:material,cover_background,compact_cover_background:false,cover_overlay:true,reveal:1.,overlay:0.,mini:false,transparent:false,card_frost:0.,card_opacity:0.,capsule_frost:0.,capsule_opacity:0.}
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
        let tint=(if self.cover_background{amount}else{0.})+(if self.compact_cover_background{1.-amount}else{0.});
        let top=cover_color::mix(0x1b1f25,palette.background,tint);
        let bottom=cover_color::mix(0x0c0f14,palette.background,tint*0.65);
        let reflection=(if self.compact==Material::Glass{1.-amount}else{0.})+(if self.material==Material::Glass{amount}else{0.});
        let top=cover_color::mix(top,0xdfefff,0.08*reflection);
        // Acrylic frosts the whole plane, so a capsule set to acrylic reads exactly like an
        // acrylic card instead of falling back to the flat classic strip.
        let frost=match self.material{Material::Acrylic=>amount,Material::Glass=>amount*0.28}
            +(if self.compact==Material::Acrylic{1.-amount}else{0.});
        // The frosted artwork layer only exists for acrylic surfaces, and it hands over from the
        // strip's pair to the card's pair exactly like the rest of the plane.
        let card=if self.material==Material::Acrylic{amount}else{0.};
        let strip=if self.compact==Material::Acrylic{1.-amount}else{0.};
        let weight=(card+strip).max(1e-6);
        // Artwork is independent from Monet tint. The capsule still obeys its own opt-in.
        let cover_alpha=((if self.cover_overlay{card*self.card_opacity}else{0.})
            +(if self.compact_cover_background{strip*self.capsule_opacity}else{0.}))/100.;
        let strength=((card*self.card_frost+strip*self.capsule_frost)/weight/100.).clamp(0.,1.);
        // Keep useful detail across the whole slider instead of saturating a 192px image early.
        let cover_blur=30.*strength.powf(1.4);
        Paint{top:alpha|top,bottom:alpha|bottom,reflection,frost,amount,cover_alpha:cover_alpha.clamp(0.,1.),cover_blur,tint}
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn hints_use_the_same_material_as_the_expanded_card_even_when_music_is_collapsed(){
        for mini in [false,true]{for material in [Material::Acrylic,Material::Glass]{
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
        for mini in [false,true]{for compact in [Material::Acrylic,Material::Glass]{
            let surfaces=[Material::Acrylic,Material::Glass].map(|material|Surface{material,compact,mini,reveal:0.,..Surface::expanded(material,true)}.paint(CoverPalette::default()));
            for surface in &surfaces[1..]{assert_eq!((surface.top,surface.bottom,surface.frost),(surfaces[0].top,surfaces[0].bottom,surfaces[0].frost));}
        }}
    }
    #[test]fn an_acrylic_strip_reads_like_the_acrylic_card(){
        // The collapsed strip must borrow the expanded card's acrylic treatment, not invent
        // a second look: same opaque plane, same full-strength frost, no glass reflection.
        let strip=Surface{compact:Material::Acrylic,reveal:0.,..Surface::expanded(Material::Acrylic,false)}.paint(CoverPalette::default());
        let card=Surface::expanded(Material::Acrylic,false).paint(CoverPalette::default());
        
        assert_eq!(strip.frost,card.frost);
        assert!(strip.frost>0.);
        assert_eq!((strip.top,strip.bottom,strip.reflection),(card.top,card.bottom,card.reflection));
    }
    #[test]fn the_frosted_artwork_layer_only_exists_for_acrylic(){
        let card=Surface{card_frost:40.,card_opacity:60.,capsule_frost:40.,capsule_opacity:0.,..Surface::expanded(Material::Acrylic,true)};
        let full=card.paint(CoverPalette::default());
        assert!((full.cover_alpha-0.6).abs()<0.01);
        assert!(full.cover_blur>0.&&full.cover_blur<30.);
        // The collapsed strip keeps its own pair, so the default 0% leaves it exactly as it was.
        let strip=Surface{reveal:0.,..card}.paint(CoverPalette::default());
        assert!(strip.cover_alpha<0.05);
        // Turning off Monet must retain the independently enabled lyric artwork layer.
        let monet_off=Surface{cover_background:false,..card}.paint(CoverPalette::default());
        assert_eq!(monet_off.cover_alpha,full.cover_alpha);
        assert_eq!(Surface{cover_overlay:false,..card}.paint(CoverPalette::default()).cover_alpha,0.);
        // Glass never samples the artwork through this layer.
        for material in [Material::Glass]{
            let other=Surface{material,compact:material,..card}.paint(CoverPalette::default());
            assert_eq!(other.cover_alpha,0.);
        }
    }
}

#[cfg(test)]mod independent_appearance_tests{
    use super::*;
    fn surface()->Surface{Surface{card_frost:40.,card_opacity:60.,capsule_frost:40.,capsule_opacity:80.,..Surface::expanded(Material::Acrylic,true)}}
    #[test]fn card_preferences_cannot_recolour_or_overlay_a_disabled_capsule(){
        let palette=CoverPalette{background:0x662233,accent:0xffaabb};
        for mini in [false,true]{for cover_background in [false,true]{for cover_overlay in [false,true]{
            let paint=Surface{mini,reveal:0.,cover_background,cover_overlay,compact_cover_background:false,..surface()}.paint(palette);
            assert_eq!(paint.tint,0.);assert_eq!(paint.cover_alpha,0.);assert_eq!(paint.top&0xffffff,0x1b1f25);
        }}}
    }
    #[test]fn capsule_preferences_cannot_change_expanded_music(){
        let palette=CoverPalette{background:0x662233,accent:0xffaabb};
        let card=surface().paint(palette);
        let changed=Surface{compact_cover_background:true,capsule_frost:100.,capsule_opacity:100.,..surface()}.paint(palette);
        assert_eq!((card.top,card.bottom,card.cover_alpha,card.cover_blur),(changed.top,changed.bottom,changed.cover_alpha,changed.cover_blur));
    }
    #[test]fn overlay_and_monet_can_be_toggled_in_every_combination(){
        let palette=CoverPalette{background:0x662233,accent:0xffaabb};
        for tint in [false,true]{for overlay in [false,true]{
            let paint=Surface{cover_background:tint,cover_overlay:overlay,..surface()}.paint(palette);
            assert_eq!(paint.tint,if tint{1.}else{0.});assert_eq!(paint.cover_alpha,if overlay{0.6}else{0.});
        }}
    }
    #[test]fn acrylic_controls_have_distinct_endpoints_and_a_monotonic_range(){
        let palette=CoverPalette::default();let mut previous=-1.;
        for frost in [0.,10.,20.,40.,60.,80.,100.]{
            let paint=Surface{card_frost:frost,..surface()}.paint(palette);
            assert!(paint.cover_blur>previous);previous=paint.cover_blur;
        }
        assert_eq!(Surface{card_frost:0.,card_opacity:0.,..surface()}.paint(palette).cover_alpha,0.);
        assert_eq!(Surface{card_frost:100.,card_opacity:100.,..surface()}.paint(palette).cover_alpha,1.);
        let mid=Surface{reveal:0.5,compact_cover_background:true,..surface()}.paint(palette);
        assert!(mid.cover_alpha>=0.6&&mid.cover_alpha<=0.8&&mid.cover_blur.is_finite());
    }
}
