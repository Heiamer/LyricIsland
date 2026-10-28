//! All visible pixels, including text, artwork, icons and focus rings, are Vello scenes.
use std::{collections::HashMap,sync::Arc};
use parley::{FontContext,LayoutContext,Layout,StyleProperty,FontStack,FontWeight,PositionedLayoutItem,Alignment,AlignmentOptions};
use vello::{Scene,Glyph,kurbo::{Affine,Rect,RoundedRect,Circle,BezPath,Stroke,Line},peniko::{Color,Fill,FontData,ImageData}};

/// A read-only memory mapping of a font file. The multi-megabyte CJK fonts stay
/// file-backed: the pages are shareable and the memory manager can evict them
/// under pressure, instead of pinning a private heap copy for the whole session.
struct FontFile{view:windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,len:usize,mapping:windows::Win32::Foundation::HANDLE,file:windows::Win32::Foundation::HANDLE}
unsafe impl Send for FontFile{}
unsafe impl Sync for FontFile{}
impl std::convert::AsRef<[u8]> for FontFile{fn as_ref(&self)->&[u8]{unsafe{if self.len==0{&[]}else{std::slice::from_raw_parts(self.view.Value as *const u8,self.len)}}}}
impl Drop for FontFile{fn drop(&mut self){unsafe{if !self.view.Value.is_null(){let _=windows::Win32::System::Memory::UnmapViewOfFile(self.view);}let _=windows::Win32::Foundation::CloseHandle(self.mapping);let _=windows::Win32::Foundation::CloseHandle(self.file);}}}
impl FontFile{
    fn map(path:&std::path::Path)->std::result::Result<Self,String>{
        use std::os::windows::ffi::OsStrExt;
        use windows::{core::PCWSTR,Win32::{Storage::FileSystem::{CreateFileW,FILE_ATTRIBUTE_NORMAL,FILE_SHARE_READ,OPEN_EXISTING},System::Memory::{CreateFileMappingW,MapViewOfFile,FILE_MAP_READ,PAGE_READONLY}}};
        unsafe{
            let wide:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
            let file=CreateFileW(PCWSTR(wide.as_ptr()),windows::Win32::Foundation::GENERIC_READ.0,FILE_SHARE_READ,None,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,None).map_err(|e|e.to_string())?;
            let len=std::fs::metadata(path).map(|m|m.len()as usize).unwrap_or(0);
            let mapping=match CreateFileMappingW(file,None,PAGE_READONLY,0,0,PCWSTR::null()){Ok(m)=>m,Err(e)=>{let _=windows::Win32::Foundation::CloseHandle(file);return Err(e.to_string())}};
            let view=MapViewOfFile(mapping,FILE_MAP_READ,0,0,0);
            if view.Value.is_null()&&len>0{let _=windows::Win32::Foundation::CloseHandle(mapping);let _=windows::Win32::Foundation::CloseHandle(file);return Err("font map view failed".into());}
            Ok(Self{view,len,mapping,file})
        }
    }
}

pub fn color(rgb:u32)->Color{Color::from_rgb8((rgb>>16)as u8,(rgb>>8)as u8,rgb as u8)}
pub fn alpha(rgb:u32,a:f32)->Color{Color::from_rgba8((rgb>>16)as u8,(rgb>>8)as u8,rgb as u8,(a.clamp(0.,1.)*255.)as u8)}
pub fn rect(x:f64,y:f64,w:f64,h:f64)->Rect{Rect::new(x,y,x+w,y+h)}
/// Google Sans Flex (SIL OFL) covers Latin and digits; CJK falls through to Noto.
pub const LATIN_FONTS:[&str;4]=["GoogleSansFlex-Regular.ttf","GoogleSansFlex-Medium.ttf","GoogleSansFlex-SemiBold.ttf","GoogleSansFlex-Bold.ttf"];
pub struct Text{
    system_fonts:bool,registered:Vec<fontique::Blob<u8>>,family:String,fonts:FontContext,context:LayoutContext<()>,cache:HashMap<(String,u32,u16,bool),Layout<()>>,
    resources:HashMap<(u64,u32),FontData>,ink_cache:HashMap<(String,u32,u16,bool),(f64,f64)>,
    /// Japanese fallback is registered only when text contains Japanese kana.
    japanese:Option<std::path::PathBuf>,
}
impl Text{
    /// No bundled fonts: callers that must not depend on the executable's folder register their own.
    pub fn empty()->Self{Self{system_fonts:false,registered:vec![],family:"Google Sans Flex".into(),fonts:FontContext{collection:fontique::Collection::new(fontique::CollectionOptions{system_fonts:false,..Default::default()}),source_cache:Default::default()},context:LayoutContext::new(),cache:HashMap::new(),resources:HashMap::new(),ink_cache:HashMap::new(),japanese:None}}
    pub fn new()->Self{let mut text=Self::empty();let dir=Self::font_dir().join("Assets/Fonts");for name in LATIN_FONTS.iter().chain(["NotoSansSC.ttf","NotoSansJP.ttf"].iter()){let _=text.register(&dir.join(name));}text}
    /// Register the primary fonts now; layout registers Japanese on first use.
    pub fn new_lazy_japanese()->Self{let mut text=Self::empty();let base=Self::font_dir();for name in LATIN_FONTS.iter().chain(["NotoSansSC.ttf"].iter()){let _=text.register(&base.join("Assets/Fonts").join(name));}text.japanese=Some(base.join("Assets/Fonts/NotoSansJP.ttf"));text}
    fn font_dir()->std::path::PathBuf{std::env::current_exe().ok().and_then(|p|p.parent().map(std::path::Path::to_path_buf)).filter(|p|p.join("Assets/Fonts").is_dir()).unwrap_or_else(||std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))}
    /// Load the deferred Japanese fallback. Safe to call repeatedly; a missing file
    /// only leaves the system-font fallback in place.
    pub fn defer_japanese(&mut self,path:std::path::PathBuf){self.japanese=Some(path);}
    pub fn warm_japanese(&mut self){if let Some(path)=self.japanese.take(){let _=self.register(&path);}}
    pub fn draw(&mut self,scene:&mut Scene,text:&str,x:f64,y:f64,size:f32,rgb:u32,bold:bool,max:f64){self.draw_argb(scene,text,x,y,size,0xff000000|rgb,bold,max);}
    pub fn draw_argb(&mut self,scene:&mut Scene,text:&str,x:f64,y:f64,size:f32,argb:u32,bold:bool,max:f64){self.draw_style(scene,text,x,y,size,argb,if bold{600}else{400},max,false);}
    fn draw_style(&mut self,scene:&mut Scene,text:&str,x:f64,y:f64,size:f32,argb:u32,weight:u16,max:f64,display:bool){
        if text.is_empty()||max<=0.{return}
        let mut key=self.layout_key(text,size,weight,display);
        if self.cache[&key].width()as f64>max{
            let chars=text.chars().collect::<Vec<_>>();let(mut low,mut high)=(0,chars.len());
            while low<high{let mid=(low+high).div_ceil(2);let candidate=format!("{}…",chars[..mid].iter().collect::<String>());if self.measure_weighted(&candidate,size,weight)<=max{low=mid}else{high=mid-1}}
            let fitted=format!("{}…",chars[..low].iter().collect::<String>());if self.measure_weighted(&fitted,size,weight)>max{return}key=self.layout_key(&fitted,size,weight,display);
        }
        let layout=&self.cache[&key];
        scene.push_clip_layer(Affine::IDENTITY,&rect(x,y-2.,max,size as f64*1.7));
        for line in layout.lines(){for item in line.items(){if let PositionedLayoutItem::GlyphRun(g)=item{
            let run=g.run();let font=run.font();let id=(font.data.id(),font.index);
            // Blob is an Arc share (both parley and vello re-export the same
            // linebender FontData), so no per-font file copy is kept in memory.
            let font=self.resources.entry(id).or_insert_with(||FontData::new(font.data.clone(),font.index));
            let mut gx=g.offset();let gy=g.baseline();
            scene.draw_glyphs(font).font_size(run.font_size()).normalized_coords(run.normalized_coords()).transform(Affine::translate((x,y))).brush(alpha(argb,(argb>>24)as f32/255.)).draw(Fill::NonZero,g.glyphs().map(|glyph|{let out=Glyph{id:glyph.id as u32,x:gx+glyph.x,y:gy-glyph.y};gx+=glyph.advance;out}));
        }}}
        if layout.width()as f64>max{scene.fill(Fill::NonZero,Affine::IDENTITY,alpha(argb,(argb>>24)as f32/255.),None,&Circle::new((x+max-3.,y+size as f64),1.));}
        scene.pop_layer();
    }
    pub fn line(&mut self,scene:&mut Scene,text:&str,r:Rect,size:f32,argb:u32,bold:bool,align:f64){
        if text.is_empty()||r.width()<=0.||r.height()<=0.{return}
        let width=self.measure(text,size,bold).min(r.width());
        let key=(text.to_string(),size.to_bits(),if bold{600}else{400},false);
        let line_height=self.cache.get(&key).map(|l|l.height()as f64).unwrap_or(size as f64*1.45);
        let y=r.y0+(r.height()-line_height)/2.;
        scene.push_clip_layer(Affine::IDENTITY,&r);
        self.draw_argb(scene,text,r.x0+(r.width()-width)*align,y,size,argb,bold,r.width());
        scene.pop_layer();
    }
    fn enable_system_fonts(&mut self){if !self.system_fonts{self.system_fonts=true;let mut fonts=FontContext::new();for blob in &self.registered{fonts.collection.register_fonts(blob.clone(),None);}self.fonts=fonts;self.cache.clear();self.ink_cache.clear();}}
    pub fn set_family(&mut self,family:&str){let family=match family{"Microsoft YaHei UI"=>"Microsoft YaHei UI","Noto Serif SC"=>"Noto Serif SC",_=>"Google Sans Flex"};if self.family!=family{self.family=family.into();if family!="Google Sans Flex"{self.enable_system_fonts();}self.cache.clear();self.ink_cache.clear();}}
    pub fn register(&mut self,path:&std::path::Path)->Result<(),String>{
        // Prefer a file mapping so the bytes are shareable and evictable; a plain
        // read is only the fallback for filesystems that cannot be mapped.
        let bytes:Arc<dyn AsRef<[u8]>+Send+Sync>=match FontFile::map(path){Ok(mapped)=>Arc::new(mapped),Err(_)=>Arc::new(std::fs::read(path).map_err(|e|e.to_string())?)};
        let blob=fontique::Blob::new(bytes);self.fonts.collection.register_fonts(blob.clone(),None);self.registered.push(blob);self.cache.clear();self.ink_cache.clear();Ok(())}
    fn layout_key(&mut self,text:&str,size:f32,weight:u16,display:bool)->(String,u32,u16,bool){
        if self.japanese.is_some()&&text.chars().any(|c|matches!(c as u32,0x3040..=0x30ff|0x31f0..=0x31ff|0xff66..=0xff9d)){self.warm_japanese();}
        let key=(text.to_string(),size.to_bits(),weight,display);
        if !self.cache.contains_key(&key){
            if self.cache.len()>512{self.cache.clear();self.ink_cache.clear();}
            let mut builder=self.context.ranged_builder(&mut self.fonts,text,1.,true);
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(StyleProperty::FontStack(FontStack::Source(format!("'{}', 'Noto Sans SC', 'Noto Sans JP', sans-serif",self.family).into())));
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight as f32)));
            let mut layout=builder.build(text);layout.break_all_lines(None);layout.align(None,Alignment::Start,AlignmentOptions::default());
            let missing=!self.system_fonts&&layout.lines().any(|line|line.items().any(|item|if let PositionedLayoutItem::GlyphRun(g)=item{g.glyphs().any(|v|v.id==0)}else{false}));
            if missing{self.enable_system_fonts();return self.layout_key(text,size,weight,display)}
            self.cache.insert(key.clone(),layout);
        }
        key
    }
    pub fn measure(&mut self,text:&str,size:f32,bold:bool)->f64{self.measure_weighted(text,size,if bold{600}else{400})}
    pub fn measure_weighted(&mut self,text:&str,size:f32,weight:u16)->f64{if text.is_empty(){return 0.}let key=self.layout_key(text,size,weight,false);self.cache[&key].width()as f64}
    pub fn line_weighted(&mut self,scene:&mut Scene,text:&str,r:Rect,size:f32,argb:u32,weight:u16,align:f64){
        if text.is_empty()||r.width()<=0.||r.height()<=0.{return}
        let width=self.measure_weighted(text,size,weight).min(r.width());let key=self.layout_key(text,size,weight,false);let h=self.cache[&key].height()as f64;
        scene.push_clip_layer(Affine::IDENTITY,&r);self.draw_style(scene,text,r.x0+(r.width()-width)*align,r.y0+(r.height()-h)/2.,size,argb,weight,r.width(),false);scene.pop_layer();
    }
    /// Optical centre uses the same shaped glyph bounds and baseline as line().
    pub fn ink_center_y(&mut self,text:&str,r:Rect,size:f32,bold:bool)->f64{
        use skrifa::{FontRef,MetadataProvider,instance::{Size,LocationRef},GlyphId};
        if text.is_empty(){return r.center().y}let key=self.layout_key(text,size,if bold{600}else{400},false);let layout=&self.cache[&key];let y=r.y0+(r.height()-layout.height()as f64)/2.;if let Some((top,bottom))=self.ink_cache.get(&key){return y+(top+bottom)/2.}let(mut top,mut bottom)=(f64::INFINITY,f64::NEG_INFINITY);
        for line in layout.lines(){for item in line.items(){if let PositionedLayoutItem::GlyphRun(g)=item{let run=g.run();let font=run.font();if let Ok(f)=FontRef::from_index(font.data.as_ref(),font.index){let coords=run.normalized_coords().iter().map(|v|skrifa::instance::NormalizedCoord::from_bits(*v)).collect::<Vec<_>>();let metrics=f.glyph_metrics(Size::new(run.font_size()),LocationRef::new(&coords));for glyph in g.glyphs(){if let Some(b)=metrics.bounds(GlyphId::new(glyph.id as u32)){if b.y_max>b.y_min{let baseline=g.baseline()as f64-glyph.y as f64;top=top.min(baseline-b.y_max as f64);bottom=bottom.max(baseline-b.y_min as f64);}}}}}}}
        if top.is_finite()&&bottom.is_finite(){self.ink_cache.insert(key,(top,bottom));y+(top+bottom)/2.}else{r.center().y}
    }
    pub fn centered(&mut self,scene:&mut Scene,text:&str,r:Rect,size:f32,rgb:u32,bold:bool){
        // The same shaping cache determines centering, including Chinese fallback runs.
        let _=self.measure(text,size,bold);
        let width=self.cache.get(&(text.to_string(),size.to_bits(),if bold{600}else{400},false)).map(|l|l.width()as f64).unwrap_or(0.);
        self.draw(scene,text,r.x0+(r.width()-width.min(r.width()-8.))/2.,r.y0+(r.height()-size as f64*1.35)/2.,size,rgb,bold,r.width()-8.);
    }
}
pub fn fill(scene:&mut Scene,r:Rect,radius:f64,rgb:u32){scene.fill(Fill::NonZero,Affine::IDENTITY,color(rgb),None,&RoundedRect::from_rect(r,radius));}
pub fn outline(scene:&mut Scene,r:Rect,radius:f64,rgb:u32,width:f64){scene.stroke(&Stroke::new(width),Affine::IDENTITY,color(rgb),None,&RoundedRect::from_rect(r,radius));}
pub fn circle(scene:&mut Scene,x:f64,y:f64,r:f64,rgb:u32){scene.fill(Fill::NonZero,Affine::IDENTITY,color(rgb),None,&Circle::new((x,y),r));}
pub fn line(scene:&mut Scene,a:(f64,f64),b:(f64,f64),rgb:u32,width:f64){scene.stroke(&Stroke::new(width),Affine::IDENTITY,color(rgb),None,&Line::new(a,b));}
pub fn image(scene:&mut Scene,img:&ImageData,r:Rect,radius:f64){scene.push_clip_layer(Affine::IDENTITY,&RoundedRect::from_rect(r,radius));let scale=(r.width()/img.width as f64).max(r.height()/img.height as f64);let tr=Affine::translate((r.center().x-img.width as f64*scale/2.,r.center().y-img.height as f64*scale/2.))*Affine::scale(scale);scene.draw_image(&vello::peniko::ImageBrush::from(img.clone()),tr);scene.pop_layer();}
pub fn ring(scene:&mut Scene,c:(f64,f64),radius:f64,progress:f64,rgb:u32,width:f64){
    let mut p=BezPath::new();let n=(progress.clamp(0.,1.)*180.).ceil()as usize;if n==0{return}
    for i in 0..=n{let a=-std::f64::consts::FRAC_PI_2+progress*std::f64::consts::TAU*i as f64/n as f64;let pt=(c.0+a.cos()*radius,c.1+a.sin()*radius);if i==0{p.move_to(pt)}else{p.line_to(pt)}}
    scene.stroke(&Stroke::new(width).with_caps(vello::kurbo::Cap::Round),Affine::IDENTITY,color(rgb),None,&p);
}
#[derive(Clone,Copy,Debug)]pub enum Icon{Home,Music,Focus,Todo,Settings,Play,Pause,Next,Prev,Plus,Check,Close,Arrow,Island,Sun,Moon,Back,Trash,Reset,Search,Volume,Folder,Import,Leaf,Stop,Minimize,Maximize,Restore,Translate,Heart,Move}
pub fn icon(scene:&mut Scene,kind:Icon,x:f64,y:f64,size:f64,rgb:u32){if crate::material_symbols::icon(scene,kind,x,y,size,rgb){return}legacy_icon(scene,kind,x,y,size,rgb)}
pub fn apple_transport(scene:&mut Scene,kind:Icon,x:f64,y:f64,size:f64,rgb:u32){legacy_icon(scene,kind,x,y,size,rgb)}
fn legacy_icon(scene:&mut Scene,kind:Icon,x:f64,y:f64,size:f64,rgb:u32){
    let tr=Affine::translate((x,y))*Affine::scale(size/24.);let mut p=BezPath::new();
    match kind{
        Icon::Home=>{p.move_to((3.,10.));p.line_to((12.,3.));p.line_to((21.,10.));p.move_to((5.,9.));p.line_to((5.,21.));p.line_to((10.,21.));p.line_to((10.,15.));p.line_to((14.,15.));p.line_to((14.,21.));p.line_to((19.,21.));p.line_to((19.,9.));},
        Icon::Music=>{p.move_to((10.,17.));p.line_to((10.,5.));p.line_to((20.,3.));p.line_to((20.,15.));p.move_to((10.,9.));p.line_to((20.,7.));},
        Icon::Focus=>{p.move_to((9.,2.));p.line_to((15.,2.));p.move_to((12.,6.));p.line_to((12.,13.));p.line_to((15.,15.));},
        Icon::Todo=>{p.move_to((4.,6.));p.line_to((6.,8.));p.line_to((9.,4.));p.move_to((12.,6.));p.line_to((21.,6.));p.move_to((4.,13.));p.line_to((6.,15.));p.line_to((9.,11.));p.move_to((12.,13.));p.line_to((21.,13.));p.move_to((4.,21.));p.line_to((21.,21.));},
        Icon::Settings=>{for i in 0..32{let angle=(i as f64+0.5)*std::f64::consts::TAU/32.;let r=if i%4<2{8.}else{10.};let pt=(12.+angle.cos()*r,12.+angle.sin()*r);if i==0{p.move_to(pt)}else{p.line_to(pt)}}p.close_path();},
        Icon::Translate=>{p.move_to((3.,5.));p.line_to((15.,5.));p.move_to((9.,2.));p.line_to((9.,5.));p.move_to((12.,5.));p.curve_to((11.,11.),(7.,14.),(3.,16.));p.move_to((5.,8.));p.curve_to((7.,12.),(10.,15.),(13.,16.));p.move_to((13.,21.));p.line_to((17.5,10.));p.line_to((22.,21.));p.move_to((15.,17.));p.line_to((20.,17.));},
        Icon::Heart=>{p.move_to((12.,21.));p.curve_to((7.,17.),(2.,13.),(2.,8.));p.curve_to((2.,2.),(9.,1.),(12.,6.));p.curve_to((15.,1.),(22.,2.),(22.,8.));p.curve_to((22.,13.),(17.,17.),(12.,21.));p.close_path();},
        Icon::Move=>{for(a,b)in [((12.,2.),(12.,22.)),((2.,12.),(22.,12.)),((8.,6.),(12.,2.)),((12.,2.),(16.,6.)),((8.,18.),(12.,22.)),((12.,22.),(16.,18.)),((6.,8.),(2.,12.)),((2.,12.),(6.,16.)),((18.,8.),(22.,12.)),((22.,12.),(18.,16.))]{p.move_to(a);p.line_to(b);}},
        Icon::Play=>{p.move_to((8.5,4.7));p.curve_to((7.,3.8),(6.5,4.3),(6.5,6.));p.line_to((6.5,18.));p.curve_to((6.5,19.7),(7.,20.2),(8.5,19.3));p.line_to((19.,13.3));p.curve_to((20.6,12.4),(20.6,11.6),(19.,10.7));p.close_path();scene.fill(Fill::NonZero,tr,color(rgb),None,&p);return},
        Icon::Pause=>{for x in [6.5,14.]{scene.fill(Fill::NonZero,tr,color(rgb),None,&RoundedRect::from_rect(rect(x,4.,3.5,16.),1.2));}return},
        Icon::Next|Icon::Prev=>{
            let flip=matches!(kind,Icon::Prev);let f=|x:f64|if flip{24.-x}else{x};
            for x in [2.5,12.]{let mut q=BezPath::new();q.move_to((f(x+1.),5.2));q.curve_to((f(x),4.5),(f(x),4.8),(f(x),6.));q.line_to((f(x),18.));q.curve_to((f(x),19.2),(f(x),19.5),(f(x+1.),18.8));q.line_to((f(x+8.9),13.));q.curve_to((f(x+10.2),12.2),(f(x+10.2),11.8),(f(x+8.9),11.));q.close_path();scene.fill(Fill::NonZero,tr,color(rgb),None,&q);}return
        },
        Icon::Plus=>{p.move_to((12.,5.));p.line_to((12.,19.));p.move_to((5.,12.));p.line_to((19.,12.));},
        Icon::Check=>{p.move_to((5.,12.));p.line_to((10.,17.));p.line_to((19.,7.));},
        Icon::Close=>{p.move_to((6.,6.));p.line_to((18.,18.));p.move_to((18.,6.));p.line_to((6.,18.));},
        Icon::Arrow=>{p.move_to((4.,12.));p.line_to((20.,12.));p.move_to((14.,6.));p.line_to((20.,12.));p.line_to((14.,18.));},
        Icon::Back=>{p.move_to((20.,12.));p.line_to((4.,12.));p.move_to((10.,6.));p.line_to((4.,12.));p.line_to((10.,18.));},
        Icon::Island=>{p.move_to((8.,6.));p.line_to((16.,6.));p.curve_to((24.,6.),(24.,18.),(16.,18.));p.line_to((8.,18.));p.curve_to((0.,18.),(0.,6.),(8.,6.));p.close_path();},
        Icon::Sun=>{for i in 0..8{let a=i as f64*std::f64::consts::FRAC_PI_4;p.move_to((12.+a.cos()*8.,12.+a.sin()*8.));p.line_to((12.+a.cos()*10.,12.+a.sin()*10.));}},
        Icon::Moon=>{let q=(73f64/2.).sqrt();let top=(14.-q,10.-q);let bottom=(14.+q,10.+q);let a=(top.1-12.).atan2(top.0-12.);let b=(bottom.1-12.).atan2(bottom.0-12.);for i in 0..=48{let angle=a-(std::f64::consts::TAU-(b-a))*i as f64/48.;let pt=(12.+9.*angle.cos(),12.+9.*angle.sin());if i==0{p.move_to(pt)}else{p.line_to(pt)}}let a=(bottom.1-8.).atan2(bottom.0-16.);let b=(top.1-8.).atan2(top.0-16.)+std::f64::consts::TAU;for i in 1..=48{let angle=a+(b-a)*i as f64/48.;p.line_to((16.+9.*angle.cos(),8.+9.*angle.sin()));}p.close_path();scene.fill(Fill::NonZero,tr,color(rgb),None,&p);return},
        Icon::Trash=>{p.move_to((4.,6.));p.line_to((20.,6.));p.move_to((9.,3.));p.line_to((15.,3.));p.move_to((6.,6.));p.line_to((7.,21.));p.line_to((17.,21.));p.line_to((18.,6.));p.move_to((10.,10.));p.line_to((10.,17.));p.move_to((14.,10.));p.line_to((14.,17.));},
        Icon::Reset=>{p.move_to((4.,4.));p.line_to((4.,10.));p.line_to((10.,10.));p.move_to((4.,10.));p.curve_to((9.,-1.),(23.,4.),(21.,14.));p.curve_to((20.,22.),(8.,25.),(4.,17.));},
        Icon::Search=>{p.move_to((16.,16.));p.line_to((21.,21.));},
        Icon::Volume=>{p.move_to((3.,9.));p.line_to((7.,9.));p.line_to((12.,5.));p.line_to((12.,19.));p.line_to((7.,15.));p.line_to((3.,15.));p.close_path();p.move_to((16.,8.));p.curve_to((20.,10.),(20.,14.),(16.,16.));p.move_to((19.,4.));p.curve_to((26.,8.),(26.,16.),(19.,20.));},
        Icon::Folder=>{p.move_to((3.,6.));p.line_to((10.,6.));p.line_to((12.,9.));p.line_to((21.,9.));p.line_to((21.,20.));p.line_to((3.,20.));p.close_path();},
        Icon::Import=>{p.move_to((12.,3.));p.line_to((12.,15.));p.move_to((7.,10.));p.line_to((12.,15.));p.line_to((17.,10.));p.move_to((4.,16.));p.line_to((4.,21.));p.line_to((20.,21.));p.line_to((20.,16.));},
        Icon::Leaf=>{p.move_to((5.,19.));p.curve_to((0.,8.),(14.,2.),(21.,3.));p.curve_to((22.,15.),(16.,22.),(5.,19.));p.move_to((4.,21.));p.line_to((15.,10.));},
        Icon::Stop=>{p.move_to((6.,6.));p.line_to((18.,6.));p.line_to((18.,18.));p.line_to((6.,18.));p.close_path();},
        Icon::Minimize=>{p.move_to((5.,12.));p.line_to((19.,12.));},
        Icon::Maximize=>{p.move_to((6.,6.));p.line_to((18.,6.));p.line_to((18.,18.));p.line_to((6.,18.));p.close_path();},
        Icon::Restore=>{p.move_to((5.,9.));p.line_to((9.,9.));p.line_to((9.,5.));p.line_to((19.,5.));p.line_to((19.,15.));p.line_to((15.,15.));p.move_to((9.,9.));p.line_to((9.,19.));p.line_to((19.,19.));p.line_to((19.,9.));},
    }
    scene.stroke(&Stroke::new(1.8).with_caps(vello::kurbo::Cap::Round).with_join(vello::kurbo::Join::Round),tr,color(rgb),None,&p);
    let mut round=|cx,cy,r|scene.stroke(&Stroke::new(1.8),tr,color(rgb),None,&Circle::new((cx,cy),r));
    match kind{Icon::Music=>{round(6.5,17.5,3.5);round(16.5,15.5,3.5)},Icon::Focus=>round(12.,14.,8.),Icon::Sun=>round(12.,12.,4.),Icon::Settings=>round(12.,12.,3.4),Icon::Search=>round(10.5,10.5,6.5),_=>{}}
}
