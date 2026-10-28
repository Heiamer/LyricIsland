//! Pixel regressions for retained commands, group alpha and image atlas reuse.
use std::{path::Path,sync::Arc};
use vello::{Scene,kurbo::{Affine,Rect},peniko::{Blob,Color,Fill,ImageData,ImageFormat,ImageAlphaType}};
use crate::gpu::Gpu;
fn pixel(path:&Path,x:u32,y:u32,expected:[u8;4])->Result<(),String>{
    let image=image::open(path).map_err(|e|e.to_string())?.to_rgba8();let actual=image.get_pixel(x,y).0;
    if actual.iter().zip(expected).any(|(a,b)|a.abs_diff(b)>2){return Err(format!("{} pixel ({x},{y}): {actual:?}, expected {expected:?}",path.display()))}Ok(())
}
pub fn run(gpu:&mut Gpu,out:&Path)->Result<(),String>{
    let bounds=Rect::new(0.,0.,32.,32.);let mut scene=Scene::new();
    scene.push_layer(vello::peniko::Mix::Normal,0.5,Affine::IDENTITY,&bounds);
    scene.fill(Fill::NonZero,Affine::IDENTITY,Color::from_rgb8(255,0,0),None,&Rect::new(0.,0.,24.,24.));
    scene.fill(Fill::NonZero,Affine::IDENTITY,Color::from_rgb8(0,0,255),None,&Rect::new(8.,8.,32.,32.));scene.pop_layer();
    let path=out.join("hybrid-group-alpha.png");gpu.capture(&scene,32,32,&path)?;
    pixel(&path,4,4,[128,0,0,128])?;pixel(&path,12,12,[0,0,128,128])?;
    let mut child=Scene::new();child.fill(Fill::NonZero,Affine::IDENTITY,Color::from_rgb8(255,0,0),None,&Rect::new(0.,0.,4.,4.));
    let mut parent=Scene::new();parent.append(&child,Some(Affine::translate((4.,4.))*Affine::scale(2.)));child.reset();
    let path=out.join("hybrid-append.png");gpu.capture(&parent,32,32,&path)?;pixel(&path,7,7,[255,0,0,255])?;pixel(&path,1,1,[0,0,0,0])?;
    let blob=Blob::new(Arc::new(vec![64u8,0,16,128]));let mut scene=Scene::new();
    for (x,format,alpha_type) in [(0.,ImageFormat::Rgba8,ImageAlphaType::Alpha),(10.,ImageFormat::Bgra8,ImageAlphaType::Alpha),(20.,ImageFormat::Rgba8,ImageAlphaType::AlphaPremultiplied)]{
        let image=ImageData{data:blob.clone(),format,alpha_type,width:1,height:1};
        scene.draw_image(&vello::peniko::ImageBrush::from(image),Affine::translate((x,0.))*Affine::scale(8.));
    }
    let path=out.join("hybrid-image-formats.png");gpu.capture(&scene,32,32,&path)?;
    pixel(&path,4,4,[32,0,8,128])?;pixel(&path,14,4,[8,0,32,128])?;pixel(&path,24,4,[64,0,16,128])?;
    for _ in 0..4{gpu.capture(&Scene::new(),32,32,&out.join("hybrid-empty.png"))?;}
    let mut scene=Scene::new();let image=ImageData{data:Blob::new(Arc::new(vec![0u8,255,0,255])),format:ImageFormat::Rgba8,alpha_type:ImageAlphaType::Alpha,width:1,height:1};
    scene.draw_image(&vello::peniko::ImageBrush::from(image),Affine::scale(16.));let path=out.join("hybrid-image-reuse.png");gpu.capture(&scene,32,32,&path)?;pixel(&path,4,4,[0,255,0,255])?;
    for i in 0..36u8{
        let mut scene=Scene::new();let image=ImageData{data:Blob::new(Arc::new(vec![i,255-i,0,255])),format:ImageFormat::Rgba8,alpha_type:ImageAlphaType::Alpha,width:1,height:1};
        scene.draw_image(&vello::peniko::ImageBrush::from(image),Affine::scale(16.));
        let path=out.join("hybrid-image-churn.png");gpu.capture(&scene,32,32,&path)?;pixel(&path,4,4,[i,255-i,0,255])?;
    }
    for (width,height) in [(2560,1440),(640,480),(1200,800)]{
        let mut scene=Scene::new();scene.fill(Fill::NonZero,Affine::IDENTITY,Color::from_rgb8(31,127,63),None,&Rect::new(0.,0.,width as f64,height as f64));
        let path=out.join("hybrid-resize.png");gpu.capture(&scene,width,height,&path)?;pixel(&path,width-3,height-3,[31,127,63,255])?;
    }
    let mut scene=Scene::new();let mut text=crate::draw::Text::new_lazy_japanese();
    for (i,sample) in ["中文 ABC 123","君に届け ハウル","ภาษาไทย","العربية","한글","🎵"].iter().enumerate(){text.draw(&mut scene,sample,8.,8.+i as f64*35.,20.,0xffffff,false,760.);}
    gpu.capture(&scene,800,240,&out.join("hybrid-multilingual.png"))?;Ok(())
}
