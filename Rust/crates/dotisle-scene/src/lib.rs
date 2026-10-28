//! Retained drawing commands replayed into Vello Hybrid at the physical viewport size.
//! Keeping commands independent of a GPU lets UI layout and text shaping stay unchanged.
use std::{collections::HashMap, sync::Arc};
pub use vello_common::{kurbo, peniko};
pub use wgpu;
pub use glifo::Glyph;
use kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use peniko::{BlendMode, Brush, BrushRef, Fill, FontData, ImageBrush, ImageBrushRef};
use vello_common::paint::{ImageSource, PaintType};

#[derive(Clone, Debug)]
enum Geometry { Rect(Rect), Path(BezPath) }
impl Geometry {
    fn new(shape: &impl Shape) -> Self {
        shape.as_rect().map_or_else(||Self::Path(shape.to_path(0.1)),Self::Rect)
    }
}
#[derive(Clone, Debug)]
enum Command {
    Fill(Fill, Affine, Brush, Option<Affine>, Geometry),
    Stroke(Stroke, Affine, Brush, Option<Affine>, BezPath),
    Layer(BlendMode, f32, Affine, BezPath),
    Pop,
    Append(Scene, Affine),
    Blur(Affine, Rect, peniko::Color, f64, f64),
    Glyphs { font:FontData,size:f32,coords:Vec<i16>,transform:Affine,brush:Brush,fill:Fill,glyphs:Vec<Glyph> },
}

#[derive(Clone, Debug, Default)]
pub struct Scene { commands:Arc<Vec<Command>> }
impl Scene {
    pub fn new() -> Self { Self::default() }
    fn push(&mut self, c:Command) { Arc::make_mut(&mut self.commands).push(c); }
    pub fn reset(&mut self) { Arc::make_mut(&mut self.commands).clear(); }
    pub fn fill<'a>(&mut self,fill:Fill,transform:Affine,brush:impl Into<BrushRef<'a>>,brush_transform:Option<Affine>,shape:&impl Shape) {
        self.push(Command::Fill(fill,transform,brush.into().to_owned(),brush_transform,Geometry::new(shape)));
    }
    pub fn stroke<'a>(&mut self,stroke:&Stroke,transform:Affine,brush:impl Into<BrushRef<'a>>,brush_transform:Option<Affine>,shape:&impl Shape) {
        self.push(Command::Stroke(stroke.clone(),transform,brush.into().to_owned(),brush_transform,shape.to_path(0.1)));
    }
    pub fn push_layer(&mut self,blend:impl Into<BlendMode>,alpha:f32,transform:Affine,shape:&impl Shape) {
        self.push(Command::Layer(blend.into(),alpha.clamp(0.,1.),transform,shape.to_path(0.1)));
    }
    pub fn push_clip_layer(&mut self,transform:Affine,shape:&impl Shape) { self.push_layer(BlendMode::default(),1.,transform,shape); }
    pub fn pop_layer(&mut self) { self.push(Command::Pop); }
    pub fn append(&mut self,scene:&Scene,transform:Option<Affine>) {
        if !scene.commands.is_empty() { self.push(Command::Append(scene.clone(),transform.unwrap_or(Affine::IDENTITY))); }
    }
    pub fn draw_image<'a>(&mut self,image:impl Into<ImageBrushRef<'a>>,transform:Affine) {
        let image=image.into().to_owned();
        let rect=Rect::new(0.,0.,image.image.width as f64,image.image.height as f64);
        self.push(Command::Fill(Fill::NonZero,transform,Brush::Image(image),None,Geometry::Rect(rect)));
    }
    pub fn draw_blurred_rounded_rect(&mut self,transform:Affine,rect:Rect,color:peniko::Color,radius:f64,std_dev:f64) {
        self.push(Command::Blur(transform,rect,color,radius,std_dev));
    }
    pub fn draw_glyphs<'a>(&'a mut self,font:&FontData)->GlyphBuilder<'a> {
        GlyphBuilder{scene:self,font:font.clone(),size:16.,coords:vec![],transform:Affine::IDENTITY,brush:Brush::Solid(peniko::Color::BLACK)}
    }
}
pub struct GlyphBuilder<'a>{scene:&'a mut Scene,font:FontData,size:f32,coords:Vec<i16>,transform:Affine,brush:Brush}
impl GlyphBuilder<'_> {
    pub fn font_size(mut self,size:f32)->Self{self.size=size;self}
    pub fn normalized_coords(mut self,coords:&[i16])->Self{self.coords.extend_from_slice(coords);self}
    pub fn transform(mut self,transform:Affine)->Self{self.transform=transform;self}
    pub fn brush<'b>(mut self,brush:impl Into<BrushRef<'b>>)->Self{self.brush=brush.into().to_owned();self}
    pub fn draw(self,fill:Fill,glyphs:impl IntoIterator<Item=Glyph>){
        self.scene.push(Command::Glyphs{font:self.font,size:self.size,coords:self.coords,transform:self.transform,brush:self.brush,fill,glyphs:glyphs.into_iter().collect()});
    }
}

struct CachedImage { source:ImageSource,last_used:u64 }
type ImageKey=(u64,u32,u32,std::mem::Discriminant<peniko::ImageFormat>,std::mem::Discriminant<peniko::ImageAlphaType>);
fn image_key(i:&peniko::ImageData)->ImageKey{(i.data.id(),i.width,i.height,std::mem::discriminant(&i.format),std::mem::discriminant(&i.alpha_type))}
#[derive(Default)]
struct PaintCache { images:HashMap<ImageKey,CachedImage>,frame:u64 }
impl PaintCache {
    fn prepare(&mut self,commands:&[Command],renderer:&mut vello_hybrid::Renderer,resources:&mut vello_hybrid::Resources,device:&wgpu::Device,queue:&wgpu::Queue,encoder:&mut wgpu::CommandEncoder){
        for command in commands {
            let brush=match command {
                Command::Fill(_,_,b,_,_)|Command::Stroke(_,_,b,_,_)|Command::Glyphs{brush:b,..}=>b,
                Command::Append(child,_)=>{self.prepare(&child.commands,renderer,resources,device,queue,encoder);continue},
                _=>continue,
            };
            if let Brush::Image(i)=brush {
                let key=image_key(&i.image);
                if let Some(image)=self.images.get_mut(&key){image.last_used=self.frame;continue}
                let ImageSource::Pixmap(pixmap)=ImageSource::from_peniko_image_data(&i.image) else {unreachable!()};
                let id=renderer.upload_image(resources,device,queue,encoder,&pixmap);
                self.images.insert(key,CachedImage{source:ImageSource::opaque_id_with_transparency_hint(id,pixmap.may_have_transparency()),last_used:self.frame});
            }
        }
    }
    fn resolve(&mut self,brush:&Brush)->PaintType {
        match brush {
            Brush::Solid(c)=>PaintType::Solid(*c),
            Brush::Gradient(g)=>PaintType::Gradient(g.clone()),
            Brush::Image(i)=>{
                let key=image_key(&i.image);
                let cached=self.images.get_mut(&key).expect("image must be uploaded before replay");
                cached.last_used=self.frame;
                PaintType::Image(ImageBrush{image:cached.source.clone(),sampler:i.sampler})
            }
        }
    }
    fn maintain(&mut self){
        self.frame=self.frame.wrapping_add(1);
    }
    fn retire(&mut self,renderer:&mut vello_hybrid::Renderer,resources:&mut vello_hybrid::Resources,encoder:&mut wgpu::CommandEncoder){
        // Clear freed slots after this frame's draws. Reusing a slot earlier would let
        // an encoded clear overwrite the queue's newly uploaded pixels.
        let stale:Vec<_>=self.images.iter().filter(|(_,v)|self.frame.wrapping_sub(v.last_used)>2).map(|(k,_)|*k).collect();
        for key in stale{if let Some(CachedImage{source:ImageSource::OpaqueId{id,..},..})=self.images.remove(&key){renderer.destroy_image(resources,encoder,id);}}
    }
}

fn replay(commands:&[Command],root:Affine,scene:&mut vello_hybrid::Scene,resources:&mut vello_hybrid::Resources,cache:&mut PaintCache,layers:&mut Vec<bool>){
    for command in commands {
        match command {
            Command::Fill(fill,tr,brush,bt,shape)=>{
                scene.set_transform(root * *tr);scene.set_paint_transform(bt.unwrap_or(Affine::IDENTITY));
                scene.set_paint(cache.resolve(brush));scene.set_fill_rule(*fill);
                match shape {Geometry::Rect(r)=>scene.fill_rect(r),Geometry::Path(p)=>scene.fill_path(p)}
            }
            Command::Stroke(stroke,tr,brush,bt,path)=>{
                scene.set_transform(root * *tr);scene.set_paint_transform(bt.unwrap_or(Affine::IDENTITY));
                scene.set_paint(cache.resolve(brush));scene.set_stroke(stroke.clone());scene.stroke_path(path);
            }
            Command::Layer(blend,alpha,tr,path)=>{
                scene.set_transform(root * *tr);scene.set_fill_rule(Fill::NonZero);
                // A plain clip needs no isolated color texture. Opacity groups still
                // use a real layer so overlapping children receive group alpha once.
                let clip=*alpha==1.&&*blend==BlendMode::default();
                if clip{scene.push_clip_path(path)}else{scene.push_layer(Some(path),Some(*blend),Some(*alpha),None,None);}
                layers.push(clip);
            }
            Command::Pop=>{if layers.pop()==Some(true){scene.pop_clip_path()}else{scene.pop_layer()}},
            Command::Append(child,tr)=>replay(&child.commands,root * *tr,scene,resources,cache,layers),
            Command::Blur(tr,rect,color,radius,std_dev)=>{
                scene.set_transform(root * *tr);scene.reset_paint_transform();scene.set_paint(*color);
                scene.fill_blurred_rounded_rect(rect,*radius as f32,*std_dev as f32,false);
            }
            Command::Glyphs{font,size,coords,transform,brush,fill,glyphs}=>{
                scene.set_transform(root * *transform);scene.reset_paint_transform();scene.set_paint(cache.resolve(brush));scene.set_fill_rule(*fill);
                // Match the previous renderer's unhinted outlines, including variable fonts.
                scene.glyph_run(resources,font).font_size(*size).normalized_coords(coords).hint(false).fill_glyphs(glyphs.iter().copied());
            }
        }
    }
}

/// Persistent CPU preparation and GPU resources; no compute rasterizer is constructed.
pub struct Renderer { renderer:vello_hybrid::Renderer,resources:vello_hybrid::Resources,scene:vello_hybrid::Scene,paints:PaintCache }
impl Renderer {
    pub fn new(device:&wgpu::Device,format:wgpu::TextureFormat,width:u32,height:u32)->Self {
        let settings=vello_hybrid::RenderSettings{
            memory_settings:vello_hybrid::MemorySettings{
                // Covers are decoded to thumbnail sizes by the callers. Allocate image pages lazily.
                image_atlas_config:vello_hybrid::AtlasConfig{atlas_size:(1024,1024),..Default::default()},
                layers_config:vello_hybrid::LayersConfig{min_texture_size:vello_hybrid::SizeU16::new(256),..Default::default()},
            },..Default::default()
        };
        let(renderer,resources)=vello_hybrid::Renderer::new_with(device,&vello_hybrid::RenderTargetConfig{format,width,height},settings);
        Self{renderer,resources,scene:vello_hybrid::Scene::new(width as u16,height as u16),paints:PaintCache::default()}
    }
    pub fn render(&mut self,device:&wgpu::Device,queue:&wgpu::Queue,encoder:&mut wgpu::CommandEncoder,scene:&Scene,scale:f64,width:u32,height:u32,view:&wgpu::TextureView)->Result<(),String>{
        let w=u16::try_from(width).map_err(|_|"render width exceeds 65535")?;
        let h=u16::try_from(height).map_err(|_|"render height exceeds 65535")?;
        self.scene.reset_and_resize(w,h);
        self.paints.maintain();
        self.paints.prepare(&scene.commands,&mut self.renderer,&mut self.resources,device,queue,encoder);
        replay(&scene.commands,Affine::scale(scale),&mut self.scene,&mut self.resources,&mut self.paints,&mut Vec::new());
        self.renderer.render(&self.scene,&mut self.resources,device,queue,encoder,&vello_hybrid::RenderSize{width,height},view,&vello_hybrid::TextureBindings::new()).map_err(|e|e.to_string())?;
        self.paints.retire(&mut self.renderer,&mut self.resources,encoder);
        Ok(())
    }
}
