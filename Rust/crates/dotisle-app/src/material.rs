//! DotIsle 2.2.3 materials: cool glass planes, lit edges and restrained accent washes.
use vello::{Scene,kurbo::{Rect,RoundedRect,Affine,Stroke,Line},peniko::{Color,Fill,Gradient}};
use dotisle_domain::monet::Scheme;
fn c(rgb:u32,alpha:f32)->Color{crate::draw::alpha(rgb,alpha)}
fn mix(a:u32,b:u32,t:f64)->u32{let ch=|s:u32|{let x=((a>>s)&255)as f64;let y=((b>>s)&255)as f64;(x+(y-x)*t).round()as u32};ch(16)<<16|ch(8)<<8|ch(0)}
pub fn window(scene:&mut Scene,r:Rect,s:Scheme){let dark=s.surface<0x404040;scene.fill(Fill::NonZero,Affine::IDENTITY,c(s.surface,0.80),None,&r);let glow=if dark{0.10}else{0.08};scene.draw_blurred_rounded_rect(Affine::IDENTITY,Rect::new(170.,48.,530.,178.),c(s.primary,glow),60.,70.);}
pub fn rail(scene:&mut Scene,r:Rect,s:Scheme){let dark=s.surface<0x404040;scene.fill(Fill::NonZero,Affine::IDENTITY,c(if dark{0x141b27}else{0xffffff},if dark{0.44}else{0.34}),None,&r);scene.stroke(&Stroke::new(0.8),Affine::IDENTITY,c(if dark{0xbdd0ef}else{0x8ea1bd},if dark{0.13}else{0.24}),None,&Line::new((r.x1,r.y0),(r.x1,r.y1)));}
pub fn panel(scene:&mut Scene,r:Rect,radius:f64,base:u32,s:Scheme){let dark=s.surface<0x404040;let top=mix(base,if dark{0x667b9c}else{0xffffff},if dark{0.23}else{0.66});let bottom=mix(base,if dark{0x101928}else{0xf6f9ff},if dark{0.18}else{0.24});let shape=RoundedRect::from_rect(r,radius);
    scene.draw_blurred_rounded_rect(Affine::translate((0.,3.)),r,c(if dark{0}else{0x0f172a},if dark{0.20}else{0.07}),radius,9.);
    let gradient=Gradient::new_linear((r.x0,r.y0),(r.x0,r.y1)).with_stops([c(top,0.94),c(bottom,0.94)]);scene.fill(Fill::NonZero,Affine::IDENTITY,&gradient,None,&shape);
    scene.stroke(&Stroke::new(0.8),Affine::IDENTITY,c(if dark{0xbdd0ef}else{0x8ea1bd},if dark{0.17}else{0.24}),None,&shape);
    scene.push_clip_layer(Affine::IDENTITY,&shape);scene.stroke(&Stroke::new(0.8),Affine::IDENTITY,c(0xffffff,if dark{0.20}else{0.94}),None,&Line::new((r.x0+radius,r.y0+0.8),(r.x1-radius,r.y0+0.8)));scene.pop_layer();
}
