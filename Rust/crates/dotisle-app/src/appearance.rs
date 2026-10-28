//! Source choice is explicit; the full colour picker remains visible in a small window.
use crate::{widgets::Ui,model::{Model,Action,Page},draw::{self,rect}};
use serde_json::json;
pub fn render(ui:&mut Ui,m:&Model,x:f64,y:f64,w:f64)->f64{
 let s=ui.s;let requested=if m.prefs.monet{m.setting("ControlCenterColorSource").as_str().unwrap_or("Content").to_owned()}else{"Custom".into()};
 ui.panel(rect(x,y,w,180.),s.container);
 ui.toggle(rect(x+24.,y+8.,w-48.,72.),"动态色彩","按所选来源生成协调配色；与灵动岛分别保存",m.prefs.monet,Action::Pref("monet"));
 let cw=(w-64.)/3.;for(i,(label,value))in [("专辑封面","Content"),("系统强调色","System"),("自选主色","Custom")].iter().enumerate(){ui.chip(rect(x+24.+i as f64*(cw+8.),y+94.,cw,42.),label,requested==*value,Action::NativeEdit("ControlCenterColorSource",json!(value)));}
 let info=match requested.as_str(){"Content"=>if m.artwork.as_ref().and_then(|a|a.seed).is_some(){"当前来源：专辑封面；换歌时配色跟随变化"}else{"未取得有效封面色，使用已保存的自选主色"},"System"=>if m.native_state["systemAccent"].is_number(){"当前来源：Windows 系统强调色"}else{"系统强调色不可用，使用已保存的自选主色"},_=>"当前来源：自选主色；不会随歌曲或系统主题改变"};
 ui.label(info,x+24.,y+150.,12.,s.on_surface_variant,false,w-48.);
 let mut cy=y+196.;ui.panel(rect(x,cy,w,92.),s.container);ui.label("自选主题色",x+24.,cy+15.,17.,s.on_surface,true,w-260.);ui.label("从完整调色盘自由选色，明暗主题分别保存",x+24.,cy+49.,12.,s.on_surface_variant,false,w-260.);
 ui.button(rect(x+w-212.,cy+24.,188.,44.),"打开完整调色盘",None,Action::PickColor(if m.prefs.dark{"SettingsDarkAccentColor"}else{"SettingsLightAccentColor"},"控制中心主题色"),false,requested=="Custom");cy+=108.;
 ui.panel(rect(x,cy,w,88.),s.container);let roles=m.dynamic_color().roles;let swatch=(w-48.)/6.;for(i,(label,c))in [("主色",roles.primary),("强调",roles.secondary),("辅助",roles.tertiary),("背景",s.surface),("层次",s.high),("文字",s.on_surface)].iter().enumerate(){let column=x+24.+i as f64*swatch;let center=column+swatch/2.;draw::circle(ui.scene,center,cy+26.,13.,s.outline_variant);draw::circle(ui.scene,center,cy+26.,12.,*c);ui.center(label,rect(column,cy+48.,swatch,22.),11.,s.on_surface_variant,false);}cy+=104.;
 ui.panel(rect(x,cy,w,248.),s.container);ui.toggle(rect(x+24.,cy+8.,w-48.,72.),"深色外观","控制中心使用深色主题，灵动岛保留自己的材质",m.prefs.dark,Action::Pref("dark"));
 let contrast=m.setting("DynamicColorHighContrast").as_bool().unwrap_or(false);ui.toggle(rect(x+24.,cy+88.,w-48.,72.),"增强颜色对比","提高文字和主要操作的辨识度",contrast,Action::NativeEdit("DynamicColorHighContrast",json!(!contrast)));
 ui.toggle(rect(x+24.,cy+168.,w-48.,72.),"减少动态效果","保留清晰的状态，关闭弹性、波浪与配色过渡",m.prefs.reduced_motion,Action::Pref("motion"));cy+=264.;
 ui.button(rect(x,cy,212.,44.),"灵动岛动态色彩与材质",None,Action::Navigate(Page::Island),true,true);cy+60.
}
