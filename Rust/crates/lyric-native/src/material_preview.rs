//! Deterministic screenshots of the shared painter, including every system-hint state.
use std::path::Path;
use serde_json::{json,Value};
use crate::{App,renderer::{Renderer,Material},layout::Rect,motion::Frame,osd::{Hint,Kind},surface::Surface};

unsafe fn save(r:&Renderer,dir:&Path,name:String,shape:Rect,rows:&mut Vec<Value>)->Result<(),Box<dyn std::error::Error>>{
    r.snapshot(&dir.join(&name))?;
    let scale=r.dpi as f64/96.;
    rows.push(json!({"file":name,"sample":[((shape.x+10.)*scale).round()as u32,((shape.y+shape.h/2.)*scale).round()as u32],"bounds":shape}));Ok(())
}
pub unsafe fn surfaces(a:&mut App,dir:&Path)->Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(dir)?;a.opacity=1.;a.lyric="晚风轻轻，音乐刚好".into();
    let mut r=a.renderer.take().unwrap();r.set_cover(Some(&std::fs::read(a.base.join("Assets/LyricIsland-v2.png"))?))?;
    let palette=r.preview_palette();let mut rows=vec![];
    for mini in [false,true]{
        a.settings["IslandSizeMode"]=json!(if mini{"Mini"}else{"Classic"});
        a.settings["MiniMaterial"]=json!("Classic");a.settings["ClassicCapsuleMaterial"]=json!("Classic");
        a.layout.card.w=if mini{420.}else{400.};a.layout.card.h=if mini{120.}else{220.};a.layout.card.x=(a.layout.host.w-a.layout.card.w)/2.;
        for (label,material) in [("Classic",Material::Classic),("Acrylic",Material::Acrylic),("Glass",Material::Glass)]{
            a.settings["IslandMaterial"]=json!(label);a.settings["MiniExpandedMaterial"]=json!(label);a.frame=Frame::expanded();
            let prefix=format!("{}-{label}",if mini{"mini"}else{"classic"});
            let mut scene=a.scene();scene.lyric_age_ms=1000.;r.render(&scene,false)?;save(&r,dir,format!("{prefix}-card.png"),a.layout.card,&mut rows)?;
            a.frame=Frame::hidden(a.options());
            let notice=crate::notifications::Notice::example(&a.base);r.set_notification_icon(notice.icon.as_deref())?;
            let notification=Rect::new((a.layout.host.w-440.)/2.,42.,440.,90.);
            let mut scene=a.scene();scene.notification=Some(&notice);scene.notification_rect=notification;scene.notification_radius=24.;scene.notification_mix=1.;
            r.render(&scene,false)?;save(&r,dir,format!("{prefix}-notification.png"),notification,&mut rows)?;
            for kind in [Kind::Caps,Kind::Num,Kind::Mic,Kind::Touchpad]{for on in [false,true]{
                let notice=Hint{kind,on}.notice();let hint=Rect::new((a.layout.host.w-240.)/2.,42.,240.,44.);
                let mut scene=a.scene();scene.osd=Some(&notice);scene.osd_rect=hint;scene.osd_radius=22.;scene.osd_mix=1.;
                r.render(&scene,false)?;save(&r,dir,format!("{prefix}-island-{}-{on}.png",kind.key()),hint,&mut rows)?;
                r.render_bottom_osd(&notice,1.,0.,Surface::expanded(material,true),false)?;
                save(&r,dir,format!("{prefix}-bottom-{}-{on}.png",kind.key()),Rect::new(16.,12.,240.,44.),&mut rows)?;
            }}
        }
    }
    std::fs::write(dir.join("surfaces.json"),serde_json::to_vec_pretty(&json!({"background":format!("#{:06X}",palette.0),"images":rows}))?)?;Ok(())
}

pub unsafe fn cover_radii(a:&mut App,dir:&Path)->Result<(),Box<dyn std::error::Error>>{
    std::fs::create_dir_all(dir)?;a.opacity=1.;a.frame=Frame::expanded();a.lyric="晚风轻轻，音乐刚好".into();
    let mut r=a.renderer.take().unwrap();let cover=std::fs::read(a.base.join("Assets/LyricIsland-v2.png"))?;
    r.set_cover(Some(&cover))?;std::thread::sleep(std::time::Duration::from_millis(300));r.set_cover(Some(&cover))?;let mut rows=vec![];
    for mini in [false,true]{for radius in [8.,16.,30.,48.]{
        a.settings["IslandSizeMode"]=json!(if mini{"Mini"}else{"Classic"});a.settings["IslandMaterial"]=json!("Glass");a.settings["MiniExpandedMaterial"]=json!("Glass");a.settings["Motion"]["Radius"]=json!(radius);
        a.layout.card.w=if mini{420.}else{400.};a.layout.card.x=(a.layout.host.w-a.layout.card.w)/2.;
        for layout in 0..if mini{1}else{4}{
            a.music_mix=layout as f64;a.layout.card.h=if mini{120.}else{crate::layout::music_height(layout as f64,12.5)};
            let cover=if mini{Rect::new(20.,8.,40.,40.)}else{crate::layout::music_geometry(400.,crate::layout::MusicLayout::SideBySide,layout as f64,r.dpi as f64/96.,12.5).cover};
            for age in [0.,70.,280.]{
                let file=format!("{}-layout-{layout}-radius-{radius:.0}-age-{age:.0}.png",if mini{"mini"}else{"classic"});
                let mut scene=a.scene();scene.cover_age_ms=age;scene.lyric_age_ms=1000.;r.render(&scene,false)?;r.snapshot(&dir.join(&file))?;
                let scale=r.dpi as f64/96.;rows.push(json!({"file":file,"mini":mini,"layout":layout,"radius":radius,"coverRadius":crate::layout::cover_corner_radius(radius,cover.w,cover.h),"age":age,"cover":[(a.layout.card.x+cover.x)*scale,(a.layout.card.y+cover.y)*scale,cover.w*scale,cover.h*scale],"scale":scale}));
            }
        }
    }}std::fs::write(dir.join("cover-radii.json"),serde_json::to_vec_pretty(&json!({"images":rows}))?)?;Ok(())
}
