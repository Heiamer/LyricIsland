//! Native, premultiplied-alpha composition surface. Offscreen layers retain
//! WPF's order: shadow, morph surface, clipped/scaled expanded content, capsule.
use std::{collections::HashMap,path::Path};
use windows::{core::{Interface,Result,PCWSTR,w},Win32::{Foundation::{HWND,HMODULE},Graphics::{
    Direct2D::{*,Common::*},Direct3D::*,Direct3D11::*,DirectWrite::*,Dxgi::{*,Common::*},DirectComposition::*
}}};
use windows_numerics::{Matrix3x2,Vector2};
use crate::{layout::{Rect,Layout},motion::{Frame,blend}};

pub fn color(argb:u32)->D2D1_COLOR_F{D2D1_COLOR_F{a:((argb>>24)&255)as f32/255.,r:((argb>>16)&255)as f32/255.,g:((argb>>8)&255)as f32/255.,b:(argb&255)as f32/255.}}
fn rect(r:Rect)->D2D_RECT_F{D2D_RECT_F{left:r.x as f32,top:r.y as f32,right:(r.x+r.w)as f32,bottom:(r.y+r.h)as f32}}
fn rounded(r:Rect,radius:f64)->D2D1_ROUNDED_RECT{D2D1_ROUNDED_RECT{rect:rect(r),radiusX:radius as f32,radiusY:radius as f32}}
fn point(x:f64,y:f64)->Vector2{Vector2{X:x as f32,Y:y as f32}}
fn identity()->Matrix3x2{Matrix3x2{M11:1.,M12:0.,M21:0.,M22:1.,M31:0.,M32:0.}}
fn matrix(s:f64,x:f64,y:f64)->Matrix3x2{Matrix3x2{M11:s as f32,M22:s as f32,M31:x as f32,M32:y as f32,..Default::default()}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Material{Classic,Acrylic,LiquidGlass}
impl Material{
    /// Unknown values fall back to acrylic so an upgraded config never loses the glass layer.
    pub fn from_setting(value:&str)->Self{match value{"Classic"=>Self::Classic,"LiquidGlass"=>Self::LiquidGlass,_=>Self::Acrylic}}
}
#[derive(Clone,Copy)]
pub struct Scene<'a>{
    pub mini:bool,pub classic_glass:bool,pub mini_ink:u32,pub mini_outline:u32,
    pub tool_theme:crate::theme_accent::Accent,
    pub drag_panel:Option<&'a crate::activities::Panel>,pub drag_active:bool,pub switch_outgoing:bool,pub switch_direction:f64,
    pub panel:Option<&'a crate::activities::Panel>,pub previous_panel:Option<&'a crate::activities::Panel>,pub component_count:usize,pub auxiliary_kind:u8,pub switching:bool,pub switch_offset:f64,pub swipe_offset:f64,
    pub material:Material,pub lyric_cover_color:bool,pub cover_background:bool,pub demo:bool,pub animate:bool,
    pub notification:Option<&'a crate::notifications::Notice>,pub notification_rect:Rect,pub notification_radius:f64,pub notification_mix:f64,
    pub osd:Option<&'a crate::notifications::Notice>,pub osd_rect:Rect,pub osd_radius:f64,pub osd_mix:f64,
    pub layout:Layout,pub frame:Frame,pub radius:f64,pub resizing:bool,pub time_ms:f64,
    pub lyric:&'a str,pub previous_lyric:&'a str,pub compact_secondary:&'a str,pub previous_compact_secondary:&'a str,pub lyric_age_ms:f64,pub secondary:&'a str,
    pub title:&'a str,pub artist:&'a str,pub playing:bool,pub highlight:f64,
    pub progress:f64,pub elapsed:f64,pub duration:f64,pub font_size:f64,
    pub fluid_motion:bool,pub music_layout:crate::layout::MusicLayout,pub music_mix:f64,
    pub text_color:u32,pub component_color:u32,pub progress_color:u32,
    pub hover:i32,pub pressed:i32,pub opacity:f32,
    pub transparent:bool,pub floating:bool,pub favorite_enabled:bool,pub has_track:bool,pub layout_scale:f64,
    pub activity_scales:[f64;3],pub activity_opacity:f64,pub cover_opacity:f64,pub cover_age_ms:f64,
    pub outgoing_opacity:f64,pub outgoing_y:f64,
}
pub struct Renderer{
    glass:crate::native_glass::Glass,glass_bitmap:Option<ID2D1Bitmap1>,hwnd:HWND,reading_glass:bool,reading_dark:bool,backdrop:Option<(crate::expanded_backdrop::Request,ID2D1Bitmap1)>,
    surface_brush:Option<(u32,u32,Rect,ID2D1LinearGradientBrush)>,highlight_brush:Option<(Rect,ID2D1LinearGradientBrush)>,
    cover_pixels:Option<crate::liquid_glass::Cover>,lens:Option<(u32,u32,u32,bool,ID2D1Bitmap1)>,
    cover_palette:crate::cover_color::CoverPalette,shadow_shape:Option<(Rect,f64)>,
    notification_icon:Option<ID2D1Bitmap1>,device_bitmaps:HashMap<String,ID2D1Bitmap1>,
    factory:ID2D1Factory1,pub dc:ID2D1DeviceContext,swap:IDXGISwapChain1,
    _device:ID3D11Device,_composition:IDCompositionDevice,_target:IDCompositionTarget,_visual:IDCompositionVisual,
    screen:ID2D1Bitmap1,compact:ID2D1Bitmap1,expanded:ID2D1Bitmap1,shadow:ID2D1Bitmap1,
    poster_cover:Option<ID2D1Bitmap1>,poster_blur:ID2D1Effect,blur:ID2D1Effect,shadow_fx:ID2D1Effect,brush:ID2D1SolidColorBrush,
    write:IDWriteFactory,body_fonts:IDWriteFontCollection,system_fonts:IDWriteFontCollection,fallback:IDWriteFontFallback,body_family:String,japanese_family:String,
    pub body_line_spacing:f64,pub body_baseline:f64,
    icons:HashMap<String,ID2D1TransformedGeometry>,layouts:HashMap<String,IDWriteTextLayout>,measured:HashMap<String,f64>,cover:Option<ID2D1Bitmap1>,previous_cover:Option<ID2D1Bitmap1>,cover_changed:std::time::Instant,flip_from:f64,pub width:u32,pub height:u32,pub dpi:f32,
}
impl Renderer{
    pub unsafe fn verify_font(&mut self)->Result<serde_json::Value>{
        let mut rows=vec![];self.dc.BeginDraw();
        for weight in [400,500,600]{
            let text="晚风轻轻，音乐刚好 DotIsle 0123456789 译";
            self.text(text,Rect::new(0.,0.,1000.,32.),16.,weight,0)?;
            let key=format!("{}|{}|{}|{}|{}|{}|{}|{:?}",text,1000.,32.,16.,weight,"Noto Sans SC",DWRITE_TEXT_ALIGNMENT_LEADING.0,None::<f64>);
            let runs=crate::font_probe::inspect(self.layouts.get(&key).unwrap())?;
            rows.push(serde_json::json!({"requestedWeight":weight,"runs":runs}));
        }
        let mut languages=vec![];
        for text in ["景色は移ろい瞬く間に変わる", "ひらがな カタカナ 日本語 한국어"]{
            self.text(text,Rect::new(0.,0.,1000.,32.),16.,500,0)?;
            let key=format!("{}|{}|{}|{}|{}|{}|{}|{:?}",text,1000.,32.,16.,500,"Noto Sans SC",DWRITE_TEXT_ALIGNMENT_LEADING.0,None::<f64>);
            languages.push(serde_json::json!({"text":text,"runs":crate::font_probe::inspect(self.layouts.get(&key).unwrap())?}));
        }
        self.dc.EndDraw(None,None)?;Ok(serde_json::json!({"family":self.body_family,"lineSpacing":self.body_line_spacing,"baseline":self.body_baseline,"weights":rows,"languageSamples":languages}))
    }
    pub unsafe fn new(hwnd:HWND,width:u32,height:u32,dpi:f32,font:&Path)->Result<Self>{
        let mut device=None;
        if D3D11CreateDevice(None,D3D_DRIVER_TYPE_HARDWARE,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,None,D3D11_SDK_VERSION,Some(&mut device),None,None).is_err(){
            D3D11CreateDevice(None,D3D_DRIVER_TYPE_WARP,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,None,D3D11_SDK_VERSION,Some(&mut device),None,None)?;
        }
        let device=device.unwrap();let dxgi:IDXGIDevice=device.cast()?;let adapter=dxgi.GetAdapter()?;let dxgi_factory:IDXGIFactory2=adapter.GetParent()?;
        let desc=DXGI_SWAP_CHAIN_DESC1{Width:width,Height:height,Format:DXGI_FORMAT_B8G8R8A8_UNORM,SampleDesc:DXGI_SAMPLE_DESC{Count:1,Quality:0},BufferUsage:DXGI_USAGE_RENDER_TARGET_OUTPUT,BufferCount:2,Scaling:DXGI_SCALING_STRETCH,SwapEffect:DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,AlphaMode:DXGI_ALPHA_MODE_PREMULTIPLIED,..Default::default()};
        let swap=dxgi_factory.CreateSwapChainForComposition(&device,&desc,None)?;
        let composition:IDCompositionDevice=DCompositionCreateDevice(&dxgi)?;let target=composition.CreateTargetForHwnd(hwnd,true)?;let visual=composition.CreateVisual()?;visual.SetContent(&swap)?;target.SetRoot(&visual)?;
        let factory:ID2D1Factory1=D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,None)?;
        let d2device=factory.CreateDevice(&dxgi)?;let dc=d2device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;dc.SetDpi(dpi,dpi);dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let surface:IDXGISurface=swap.GetBuffer(0)?;
        let props=D2D1_BITMAP_PROPERTIES1{pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED},dpiX:dpi,dpiY:dpi,bitmapOptions:D2D1_BITMAP_OPTIONS_TARGET|D2D1_BITMAP_OPTIONS_CANNOT_DRAW,..Default::default()};
        let screen=dc.CreateBitmapFromDxgiSurface(&surface,Some(&props))?;
        let props=D2D1_BITMAP_PROPERTIES1{bitmapOptions:D2D1_BITMAP_OPTIONS_TARGET,..props};
        let bitmap=||dc.CreateBitmap(D2D_SIZE_U{width,height},None,0,&props);
        let compact=bitmap()?;let expanded=bitmap()?;let shadow=bitmap()?;
        let brush=dc.CreateSolidColorBrush(&color(0xffffffff),None)?;
        let write:IDWriteFactory=DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fw:IDWriteFactory5=write.cast()?;let builder=fw.CreateFontSetBuilder()?;
        let font_path=font.to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let file=write.CreateFontFileReference(PCWSTR(font_path.as_ptr()),None)?;builder.AddFontFile(&file)?;
        let japanese_path=font.with_file_name("NotoSansJP.ttf").to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        builder.AddFontFile(&write.CreateFontFileReference(PCWSTR(japanese_path.as_ptr()),None)?)?;
        let body_fonts:IDWriteFontCollection=fw.CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?.cast()?;
        let poster_blur=dc.CreateEffect(&CLSID_D2D1GaussianBlur)?;
        poster_blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,D2D1_PROPERTY_TYPE_FLOAT,&12_f32.to_ne_bytes())?;
        poster_blur.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,D2D1_PROPERTY_TYPE_ENUM,&(D2D1_BORDER_MODE_HARD.0 as u32).to_ne_bytes())?;
        let blur=dc.CreateEffect(&CLSID_D2D1GaussianBlur)?;let shadow_fx=dc.CreateEffect(&CLSID_D2D1Shadow)?;
        dc.SetTarget(&screen);let mut system_fonts=None;write.GetSystemFontCollection(&mut system_fonts,false)?;let system_fonts=system_fonts.unwrap();let fallback=write.cast::<IDWriteFactory2>()?.GetSystemFontFallback()?;
        let mut index=0;let mut body_family=String::new();
        for name in ["Noto Sans SC","Noto Sans SC Thin"]{let encoded=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();let mut found=windows::core::BOOL(0);body_fonts.FindFamilyName(PCWSTR(encoded.as_ptr()),&mut index,&mut found)?;if found.as_bool(){body_family=name.into();break}}
        if body_family.is_empty(){return Err(windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32),"随包中文字体不是 Noto Sans SC"))}
        let mut japanese_family=String::new();
        for name in ["Noto Sans JP","Noto Sans JP Thin"]{let encoded=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();let mut i=0;let mut found=windows::core::BOOL(0);body_fonts.FindFamilyName(PCWSTR(encoded.as_ptr()),&mut i,&mut found)?;if found.as_bool(){japanese_family=name.into();break}}
        if japanese_family.is_empty(){return Err(windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32),"随包日文字体不是 Noto Sans JP"))}
        let face=body_fonts.GetFontFamily(index)?.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STRETCH_NORMAL,DWRITE_FONT_STYLE_NORMAL)?.CreateFontFace()?;
        let mut metrics=DWRITE_FONT_METRICS::default();face.GetMetrics(&mut metrics);
        let body_line_spacing=(metrics.ascent as f64+metrics.descent as f64+metrics.lineGap as f64)/metrics.designUnitsPerEm as f64;
        let body_baseline=metrics.ascent as f64/metrics.designUnitsPerEm as f64;
        dc.SetTarget(&screen);dc.BeginDraw();dc.Clear(Some(&color(0)));dc.EndDraw(None,None)?;swap.Present(0,DXGI_PRESENT(0)).ok()?;composition.Commit()?;
        Ok(Self{glass:Default::default(),glass_bitmap:None,hwnd,reading_glass:false,reading_dark:false,backdrop:None,cover_pixels:None,lens:None,factory,dc,swap,_device:device,_composition:composition,_target:target,_visual:visual,screen,compact,expanded,shadow,brush,write,body_fonts,system_fonts,fallback,body_family,japanese_family,body_line_spacing,body_baseline,icons:HashMap::new(),layouts:HashMap::new(),measured:HashMap::new(),cover:None,previous_cover:None,cover_changed:std::time::Instant::now()-std::time::Duration::from_secs(1),flip_from:0.,surface_brush:None,highlight_brush:None,cover_palette:Default::default(),shadow_shape:None,notification_icon:None,device_bitmaps:HashMap::new(),width,height,dpi,poster_cover:None,poster_blur,blur,shadow_fx})
    }
    pub fn preview_palette(&self)->(u32,u32){(self.cover_palette.background,self.cover_palette.accent)}
    pub fn preview_art(&self,path:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
        if let Some(c)=&self.cover_pixels{
            let mut pixels=c.pixels.clone();for p in pixels.chunks_exact_mut(4){p.swap(0,2);}
            if let Some(parent)=path.parent(){std::fs::create_dir_all(parent)?;}
            image::save_buffer_with_format(path,&pixels,c.width,c.height,image::ColorType::Rgba8,image::ImageFormat::Png)?;
        }Ok(())
    }
    pub unsafe fn set_cover(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{self.load_cover(bytes,true)}
    /// `theme` runs the Monet palette; notification icons skip it so they never recolour the music card.
    unsafe fn load_cover(&mut self,bytes:Option<&[u8]>,theme:bool)->std::result::Result<(),Box<dyn std::error::Error>>{
        use windows::Win32::{Graphics::Imaging::*,System::Com::{CoCreateInstance,CLSCTX_INPROC_SERVER}};
        let phase=(self.flip_from+(1.-self.flip_from)*self.cover_changed.elapsed().as_secs_f64()/0.28).clamp(0.,1.);
        if phase>=0.5||self.previous_cover.is_none(){self.previous_cover=self.cover.clone();self.flip_from=1.-phase;}else{self.flip_from=phase;}
        self.cover_changed=std::time::Instant::now();
        self.cover=None;self.cover_palette=Default::default();if theme{self.cover_pixels=None;self.lens=None;}if let Some(bytes)=bytes{
            let factory:IWICImagingFactory=CoCreateInstance(&CLSID_WICImagingFactory,None,CLSCTX_INPROC_SERVER)?;
            let stream=factory.CreateStream()?;stream.InitializeFromMemory(bytes)?;
            let decoder=factory.CreateDecoderFromStream(&stream,std::ptr::null(),WICDecodeMetadataCacheOnLoad)?;let frame=decoder.GetFrame(0)?;
            let (mut width,mut height)=(0,0);frame.GetSize(&mut width,&mut height)?;
            if width==0||height==0{return Err("封面尺寸无效".into())}
            let scale=192./width.max(height)as f64;
            let scaled_width=(width as f64*scale).round().clamp(1.,192.)as u32;
            let scaled_height=(height as f64*scale).round().clamp(1.,192.)as u32;
            let scaler=factory.CreateBitmapScaler()?;scaler.Initialize(&frame,scaled_width,scaled_height,WICBitmapInterpolationModeFant)?;
            let converter=factory.CreateFormatConverter()?;converter.Initialize(&scaler,&GUID_WICPixelFormat32bppPBGRA,WICBitmapDitherTypeNone,None,0.,WICBitmapPaletteTypeCustom)?;
            let mut pixels=vec![0u8;scaled_width as usize*scaled_height as usize*4];converter.CopyPixels(std::ptr::null(),scaled_width*4,&mut pixels)?;
            if theme{self.cover_palette=crate::cover_color::palette(&pixels);self.cover_pixels=Some(crate::liquid_glass::Cover{pixels,width:scaled_width,height:scaled_height});}
            self.cover=Some(self.dc.CreateBitmapFromWicBitmap(&converter,None)?);
            // Restart the flip clock after decode + palette so their cost never eats the animation's first frames.
            self.cover_changed=std::time::Instant::now();
        }Ok(())
    }
    pub fn glass_status(&self)->serde_json::Value{serde_json::json!({"frames":self.glass.frames,"available":self.glass.available,"captureExcluded":self.glass.available,"error":self.glass.error})}
    pub unsafe fn set_expanded_backdrop(&mut self,frame:crate::expanded_backdrop::Frame)->Result<()>{
        let q=frame.request;let w=(q.bounds[2]-q.bounds[0])as u32;let h=(q.bounds[3]-q.bounds[1])as u32;
        self.reading_dark=false;let bitmap=self.glass.fixture(&self._device,&self.dc,&frame.pixels,w,h,self.dpi,q.radius,q.tint,q.tint==0x10141c)?;self.backdrop=Some((q,bitmap));Ok(())
    }
    unsafe fn expanded_glass(&mut self,_s:&Scene<'_>,shape:Rect,radius:f64,opacity:f32,_compact:bool)->Result<()>{
        self.clip(shape,radius,opacity)?;
        if let Some(b)=self.glass_bitmap.as_ref().or_else(||self.backdrop.as_ref().map(|(_,b)|b)){self.dc.DrawBitmap(b,Some(&rect(shape)),1.,D2D1_INTERPOLATION_MODE_LINEAR,None,None);}
        self.dc.PopLayer();Ok(())
    }
    fn contrast_outline(c:u32)->u32{let l=((c>>16)&255)*3+((c>>8)&255)*6+(c&255);if l>1280{0x88000000}else{0x88ffffff}}
    unsafe fn liquid_lens(&mut self,s:&Scene<'_>,shape:Rect,radius:f64,opacity:f32)->Result<()>{
        // Cache in final card space: morphing stretches one texture and never resamples per frame.
        let w=s.layout.card.w.round().clamp(1.,768.)as u32;let h=s.layout.card.h.round().clamp(1.,768.)as u32;
        let r=s.radius.round().clamp(1.,128.)as u32;
        if !self.lens.as_ref().is_some_and(|v|v.0==w&&v.1==h&&v.2==r&&v.3==s.cover_background){
            let pixels=crate::liquid_glass::texture(w,h,r as f64,if s.cover_background{self.cover_pixels.as_ref()}else{None},if s.cover_background{self.cover_palette.background}else{0x1b1f25});
            let props=D2D1_BITMAP_PROPERTIES1{pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED},dpiX:96.,dpiY:96.,..Default::default()};
            let bitmap=self.dc.CreateBitmap(D2D_SIZE_U{width:w,height:h},Some(pixels.as_ptr()as *const _),w*4,&props)?;
            self.lens=Some((w,h,r,s.cover_background,bitmap));
        }
        self.clip(shape,radius,opacity)?;
        self.dc.DrawBitmap(&self.lens.as_ref().unwrap().4,Some(&rect(shape)),1.,D2D1_INTERPOLATION_MODE_LINEAR,None,None);
        self.dc.PopLayer();Ok(())
    }
    unsafe fn fill(&self,r:Rect,radius:f64,c:u32){self.brush.SetColor(&color(c));self.dc.FillRoundedRectangle(&rounded(r,radius),&self.brush);}
    unsafe fn line(&self,a:(f64,f64),b:(f64,f64),c:u32,width:f32){if self.reading_glass{self.brush.SetColor(&color(Self::contrast_outline(c)));self.dc.DrawLine(point(a.0,a.1),point(b.0,b.1),&self.brush,width+1.2,None);}self.brush.SetColor(&color(c));self.dc.DrawLine(point(a.0,a.1),point(b.0,b.1),&self.brush,width,None);}
    unsafe fn clip(&self,r:Rect,radius:f64,opacity:f32)->Result<()>{
        let geometry=self.factory.CreateRoundedRectangleGeometry(&rounded(r,radius))?;
        let mut params=D2D1_LAYER_PARAMETERS1{contentBounds:D2D_RECT_F{left:-100000.,top:-100000.,right:100000.,bottom:100000.},geometricMask:std::mem::ManuallyDrop::new(Some(geometry.cast()?)),maskAntialiasMode:D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,maskTransform:identity(),opacity,opacityBrush:std::mem::ManuallyDrop::new(None),layerOptions:D2D1_LAYER_OPTIONS1_NONE};
        self.dc.PushLayer(&params,None);std::mem::ManuallyDrop::drop(&mut params.geometricMask);Ok(())
    }
    /// True glyph-run width for one line, in layout units.
    ///
    /// `compact_content` runs every frame, so the DirectWrite layout is cached by
    /// text and size instead of being rebuilt per frame. The family, weight and
    /// fallback chain match `text_family` exactly, so a measured width is the
    /// same width the glyphs are later drawn at.
    pub unsafe fn measure_line(&mut self,text:&str,size:f64)->Result<f64>{
        let key=format!("{text}|{size}");if let Some(width)=self.measured.get(&key){return Ok(*width)}
        let family=self.language_family(text);let name=family.encode_utf16().chain(Some(0)).collect::<Vec<_>>();let format=self.write.CreateTextFormat(PCWSTR(name.as_ptr()),if family==self.body_family||family==self.japanese_family{&self.body_fonts}else{&self.system_fonts},DWRITE_FONT_WEIGHT_MEDIUM,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size as f32,w!("zh-CN"))?;format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;let chars=text.encode_utf16().collect::<Vec<_>>();let layout=self.write.CreateTextLayout(&chars,&format,32768.,200.)?;layout.cast::<IDWriteTextLayout2>()?.SetFontFallback(&self.fallback)?;let mut metrics=DWRITE_TEXT_METRICS::default();layout.GetMetrics(&mut metrics)?;let width=metrics.widthIncludingTrailingWhitespace as f64;
        if self.measured.len()>256{self.measured.clear()}self.measured.insert(key,width);Ok(width)
    }
    unsafe fn text(&mut self,text:&str,r:Rect,size:f64,weight:i32,c:u32)->Result<()>{
        self.text_family(text,r,size,weight,c,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,None)
    }
    unsafe fn language_family(&self,text:&str)->String{
        if text.chars().any(|c|matches!(c as u32,0x3040..=0x30ff|0x31f0..=0x31ff|0xff66..=0xff9f)){return self.japanese_family.clone()}
        let choices:&[&str]=if text.chars().any(|c|matches!(c as u32,0xac00..=0xd7af|0x1100..=0x11ff|0x3130..=0x318f)){&["Malgun Gothic"]}else{&[]};
        for name in choices{let utf16=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();let mut index=0;let mut exists=windows::core::BOOL(0);if self.system_fonts.FindFamilyName(PCWSTR(utf16.as_ptr()),&mut index,&mut exists).is_ok()&&exists.as_bool(){return name.to_string()}}
        self.body_family.clone()
    }
    unsafe fn baseline_ratio(&self,family:&str)->f64{
        if family==self.body_family{return self.body_baseline/self.body_line_spacing}
        let collection=if family==self.japanese_family{&self.body_fonts}else{&self.system_fonts};
        let name=family.encode_utf16().chain(Some(0)).collect::<Vec<_>>();let mut index=0;let mut exists=windows::core::BOOL(0);
        let result=(||->Result<f64>{collection.FindFamilyName(PCWSTR(name.as_ptr()),&mut index,&mut exists)?;let face=collection.GetFontFamily(index)?.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STRETCH_NORMAL,DWRITE_FONT_STYLE_NORMAL)?.CreateFontFace()?;let mut metrics=DWRITE_FONT_METRICS::default();face.GetMetrics(&mut metrics);Ok(metrics.ascent as f64/(metrics.ascent as f64+metrics.descent as f64+metrics.lineGap as f64).max(1.))})();
        result.unwrap_or(self.body_baseline/self.body_line_spacing)
    }
    unsafe fn text_family(&mut self,text:&str,r:Rect,size:f64,weight:i32,c:u32,family:&str,system:bool,alignment:DWRITE_TEXT_ALIGNMENT,line_height:Option<f64>)->Result<()>{
        let c=if self.reading_glass&&self.reading_dark&&(c&255)>128&&((c>>8)&255)>128&&((c>>16)&255)>128{(c&0xff000000)|0x17202c}else{c};
        if text.is_empty()||r.w<=0.{return Ok(())}let key=format!("{}|{}|{}|{}|{}|{}|{}|{:?}",text,r.w,r.h,size,weight,family,alignment.0,line_height);
        if !self.layouts.contains_key(&key){
            if self.layouts.len()>192{self.layouts.clear()}
            let resolved=if system{family.to_string()}else{self.language_family(text)};let private=!system&&(resolved==self.body_family||resolved==self.japanese_family);let baseline=self.baseline_ratio(&resolved);
            let family=resolved.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let format=self.write.CreateTextFormat(PCWSTR(family.as_ptr()),if private{&self.body_fonts}else{&self.system_fonts},DWRITE_FONT_WEIGHT(weight),DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size as f32,if resolved==self.japanese_family||resolved.starts_with("Yu ")||resolved=="Meiryo"{w!("ja-JP")}else{w!("zh-CN")})?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            if !system&&line_height!=Some(-1.){format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;let height=line_height.unwrap_or(size*self.body_line_spacing);format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,height as f32,(height*baseline)as f32)?;}
            format.SetTextAlignment(alignment)?;
            let ellipsis=self.write.CreateEllipsisTrimmingSign(&format)?;format.SetTrimming(&DWRITE_TRIMMING{granularity:DWRITE_TRIMMING_GRANULARITY_CHARACTER,..Default::default()},&ellipsis)?;
            let utf16=text.encode_utf16().collect::<Vec<_>>();let layout=self.write.CreateTextLayout(&utf16,&format,r.w as f32,if line_height==Some(-1.){r.h as f32}else{r.h.max(size*self.body_line_spacing)as f32})?;
            if !system{layout.cast::<IDWriteTextLayout2>()?.SetFontFallback(&self.fallback)?;layout.cast::<IDWriteTextLayout4>()?.SetFontAxisValues(&[DWRITE_FONT_AXIS_VALUE{axisTag:DWRITE_FONT_AXIS_TAG_WEIGHT,value:weight as f32}],DWRITE_TEXT_RANGE{startPosition:0,length:utf16.len()as u32})?;}self.layouts.insert(key.clone(),layout);
        }
        let layout=self.layouts.get(&key).unwrap();let (dx,dy)=if line_height==Some(-1.){let overhang=layout.GetOverhangMetrics()?;((overhang.left-overhang.right)as f64/2.,(overhang.top-overhang.bottom)as f64/2.)}else{(0.,0.)};
        if self.reading_glass{self.brush.SetColor(&color(Self::contrast_outline(c)));for (x,y) in [(0.,0.5)]{self.dc.DrawTextLayout(point(r.x+dx+x,r.y+dy+y),layout,&self.brush,D2D1_DRAW_TEXT_OPTIONS_CLIP);}}
        self.brush.SetColor(&color(c));self.dc.DrawTextLayout(point(r.x+dx,r.y+dy),layout,&self.brush,D2D1_DRAW_TEXT_OPTIONS_CLIP);Ok(())
    }
    /// Cache actual glyph outlines so both icons share one visible height and
    /// physical-pixel center, independently of font line metrics and hinting.
    unsafe fn centered_icon(&mut self,text:&str,r:Rect,size:f64,c:u32,family:&str,body:bool)->Result<()>{
        let key=format!("{text}|{family}|{size}|{}|{}|{}|{}",r.x,r.y,r.w,r.h);
        if !self.icons.contains_key(&key){
            let collection=if body{&self.body_fonts}else{&self.system_fonts};
            let mut index=0;let mut exists=windows::core::BOOL(0);
            for name in [if body{self.body_family.as_str()}else{family},"Segoe MDL2 Assets"]{
                let encoded=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();collection.FindFamilyName(PCWSTR(encoded.as_ptr()),&mut index,&mut exists)?;if exists.as_bool(){break}
            }
            if !exists.as_bool(){return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32)))}
            let mut face=collection.GetFontFamily(index)?.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STRETCH_NORMAL,DWRITE_FONT_STYLE_NORMAL)?.CreateFontFace()?;
            if body{face=face.cast::<IDWriteFontFace5>()?.GetFontResource()?.CreateFontFace(DWRITE_FONT_SIMULATIONS_NONE,&[DWRITE_FONT_AXIS_VALUE{axisTag:DWRITE_FONT_AXIS_TAG_WEIGHT,value:400.}])?.cast()?;}
            let codepoints=text.chars().map(|c|c as u32).collect::<Vec<_>>();let mut glyphs=vec![0u16;codepoints.len()];face.GetGlyphIndices(codepoints.as_ptr(),codepoints.len()as u32,glyphs.as_mut_ptr())?;
            let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;face.GetGlyphRunOutline(64.,glyphs.as_ptr(),None,None,glyphs.len()as u32,false,false,&sink)?;sink.Close()?;
            let ink=path.GetBounds(None)?;let height=(ink.bottom-ink.top).max(0.001);let scale=size as f32/height;
            let dpi=self.dpi as f64/96.;let cx=((r.x+r.w/2.)*dpi).round()/dpi;let cy=((r.y+r.h/2.)*dpi).round()/dpi;
            let transform=Matrix3x2{M11:scale,M22:scale,M31:cx as f32-(ink.left+ink.right)*0.5*scale,M32:cy as f32-(ink.top+ink.bottom)*0.5*scale,..Default::default()};
            let geometry=self.factory.CreateTransformedGeometry(&path,&transform)?;self.icons.insert(key.clone(),geometry);
        }
        if self.reading_glass{self.brush.SetColor(&color(Self::contrast_outline(c)));self.dc.DrawGeometry(self.icons.get(&key).unwrap(),&self.brush,1.3,None);}
        self.brush.SetColor(&color(c));self.dc.FillGeometry(self.icons.get(&key).unwrap(),&self.brush,None);Ok(())
    }
    pub unsafe fn icon_bounds(&self)->Result<serde_json::Value>{
        let mut rows=vec![];for (key,geometry) in &self.icons{let r=geometry.GetBounds(None)?;rows.push(serde_json::json!({"glyph":key.split('|').next(),"top":r.top,"bottom":r.bottom,"centerY":(r.top+r.bottom)/2.,"height":r.bottom-r.top}));}Ok(serde_json::json!(rows))
    }
    unsafe fn cover_with_opacity(&mut self,x:f64,y:f64,size:f64,opacity:f32,demo:bool)->Result<()>{
        let r=Rect::new(x,y,size,size);self.clip(r,size/6.,1.)?;self.fill(r,0.,if demo{0xff26324a}else if size>24.{0xff244b42}else{0xff242a31});
        if let Some(b)=&self.cover {let s=b.GetSize();let edge=s.width.min(s.height);let source=D2D_RECT_F{left:(s.width-edge)/2.,top:(s.height-edge)/2.,right:(s.width+edge)/2.,bottom:(s.height+edge)/2.};self.dc.DrawBitmap(b,Some(&rect(r)),opacity,D2D1_INTERPOLATION_MODE_LINEAR,Some(&source),None);}
        else if demo{
            let center=point(x+size*0.5,y+size*0.5);let disc=D2D1_ELLIPSE{point:center,radiusX:(size*0.36)as f32,radiusY:(size*0.36)as f32};
            self.brush.SetColor(&color(0xff101723));self.dc.FillEllipse(&disc,&self.brush);
            self.brush.SetColor(&color(0x804e617c));
            for radius in [0.30,0.25,0.20]{let groove=D2D1_ELLIPSE{point:center,radiusX:(size*radius)as f32,radiusY:(size*radius)as f32};self.dc.DrawEllipse(&groove,&self.brush,(size*0.008).max(0.45)as f32,None);}
            self.brush.SetColor(&color(0xffb1e8d1));self.dc.FillEllipse(&D2D1_ELLIPSE{point:center,radiusX:(size*0.105)as f32,radiusY:(size*0.105)as f32},&self.brush);
            self.brush.SetColor(&color(0xff1a2638));self.dc.FillEllipse(&D2D1_ELLIPSE{point:center,radiusX:(size*0.025)as f32,radiusY:(size*0.025)as f32},&self.brush);
        }
        else{self.text_family("♫",r,size*13./24.,400,0xffc5f0dd,"Segoe UI Symbol",true,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}
        self.dc.PopLayer();Ok(())
    }
    unsafe fn flipping_cover(&mut self,r:Rect,s:&Scene<'_>)->Result<()>{
        let t=if s.animate{(self.flip_from+(1.-self.flip_from)*s.cover_age_ms/280.).clamp(0.,1.)}else{1.};
        if t>=1.||self.previous_cover.is_none(){return self.cover_with_opacity(r.x,r.y,r.w,s.cover_opacity as f32,s.demo)}
        let sx=(std::f64::consts::PI*t).cos().abs().max(0.018);let sy=1.-0.06*(std::f64::consts::PI*t).sin();
        let draw=Rect::new(r.x+r.w*(1.-sx)/2.,r.y+r.h*(1.-sy)/2.,r.w*sx,r.h*sy);
        let bitmap=if t<0.5{self.previous_cover.clone()}else{self.cover.clone()};
        self.clip(draw,6.*sx.min(sy),s.cover_opacity as f32)?;self.fill(draw,6.*sx,0xff1b2026);
        if let Some(bitmap)=bitmap{let size=bitmap.GetSize();let edge=size.width.min(size.height);let source=D2D_RECT_F{left:(size.width-edge)/2.,top:(size.height-edge)/2.,right:(size.width+edge)/2.,bottom:(size.height+edge)/2.};self.dc.DrawBitmap(&bitmap,Some(&rect(draw)),1.,D2D1_INTERPOLATION_MODE_LINEAR,Some(&source),None);}
        self.fill(draw,6.*sx,(((1.-sx)*60.)as u32)<<24);self.dc.PopLayer();Ok(())
    }
    unsafe fn button(&mut self,id:i32,x:f64,y:f64,size:f64,scene:&Scene<'_>){
        let disabled=(id==2&&!scene.floating)||(id==9&&!scene.favorite_enabled)||([3,4,5].contains(&id)&&!scene.has_track);
        let with_alpha=|c:u32|if disabled{0x4d000000|(c&0xffffff)}else{c};
        if id==4{self.fill(Rect::new(x,y,size,size),size/2.,with_alpha(scene.component_color))}
        if scene.hover==id{self.fill(Rect::new(x,y,size,size),size/2.,0x18ffffff)}
        let scale=if scene.pressed==id&&!disabled{0.94}else{1.};let ox=x+(size-16.*scale)/2.;let oy=y+(size-16.*scale)/2.;let c=with_alpha(if id==4{if crate::cover_color::contrast(scene.component_color,0xffffff)>crate::cover_color::contrast(scene.component_color,0){0xffffffff}else{0xff090f18}}else{scene.component_color});
        let l=|a:(f64,f64),b:(f64,f64)|self.line((ox+a.0*scale,oy+a.1*scale),(ox+b.0*scale,oy+b.1*scale),c,if id==4{2.}else{1.4});
        match id{
            2=>{l((8.,1.),(8.,15.));l((1.,8.),(15.,8.));for (a,b) in [((5.,4.),(8.,1.)),((8.,1.),(11.,4.)),((4.,5.),(1.,8.)),((1.,8.),(4.,11.)),((5.,12.),(8.,15.)),((8.,15.),(11.,12.)),((12.,5.),(15.,8.)),((15.,8.),(12.,11.))]{l(a,b)}},
            3=>{l((3.,2.),(3.,14.));l((14.,2.),(5.,8.));l((5.,8.),(14.,14.));l((14.,14.),(14.,2.));},
            4=>{if scene.playing{l((5.,2.),(5.,14.));l((11.,2.),(11.,14.));}else{l((4.,2.),(13.,8.));l((13.,8.),(4.,14.));l((4.,14.),(4.,2.));}},
            5=>{l((13.,2.),(13.,14.));l((2.,2.),(11.,8.));l((11.,8.),(2.,14.));l((2.,14.),(2.,2.));},
            6=>{let _=self.centered_icon("译",Rect::new(x,y,size,size),13.,c,"Noto Sans SC",true);},
            7=>{let _=self.centered_icon("\u{e713}",Rect::new(x,y,size,size),13.,c,"Segoe Fluent Icons",false);},
            9=>{let result=(||->Result<()>{let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;let p=|x:f64,y:f64|point(ox+x*scale,oy+y*scale);sink.BeginFigure(p(8.,14.),D2D1_FIGURE_BEGIN_HOLLOW);for v in [[4.,11.,1.,8.,1.,5.],[1.,1.,6.,0.,8.,4.],[10.,0.,15.,1.,15.,5.],[15.,8.,12.,11.,8.,14.]]{sink.AddBezier(&D2D1_BEZIER_SEGMENT{point1:p(v[0],v[1]),point2:p(v[2],v[3]),point3:p(v[4],v[5])});}sink.EndFigure(D2D1_FIGURE_END_CLOSED);sink.Close()?;self.brush.SetColor(&color(c));self.dc.DrawGeometry(&path,&self.brush,1.4,None);Ok(())})();let _=result;},_=>{}
        }
    }
    /// Height of the collapsed lyric group and the secondary line size for a
    /// given font size. Delegates to `compact` so the shell sizes the capsule
    /// with exactly the numbers used to place the two lines.
    pub fn compact_metrics(&self,font_size:f64,double:bool,scale:f64)->(f64,f64,f64){
        crate::compact::group_metrics(font_size,self.body_line_spacing,scale,double)
    }
    /// Capsule height that fits the collapsed group with symmetric padding.
    pub fn compact_height(&self,font_size:f64,double:bool,scale:f64)->f64{
        crate::compact::capsule_height(font_size,self.body_line_spacing,scale,double)
    }
    unsafe fn compact_content(&mut self,s:&Scene<'_>)->Result<()>{
        let c=s.layout.capsule;let cover=s.layout.compact_cover(s.layout_scale);self.cover_with_opacity(cover.x,cover.y,cover.w,s.cover_opacity as f32,s.demo)?;
        let round=|v:f64|(v*s.layout_scale).round_ties_even()/s.layout_scale;
        let double=!s.compact_secondary.is_empty();let(group_h,line_h,secondary_h)=self.compact_metrics(s.font_size,double,s.layout_scale);let secondary_size=(s.font_size*0.78).clamp(9.,11.5);
        let line_y=c.y+round((c.h-group_h)/2.);
        let viewport=Rect::new(c.x+round(7.)+30.+round(5.),c.y,c.w-14.-30.-22.-10.,c.h);self.dc.PushAxisAlignedClip(&rect(viewport),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        let t=(s.lyric_age_ms/180.).clamp(0.,1.);let ease=1.-(1.-t).powi(3);let dy=(1.-ease)*5.;let alignment=if double{DWRITE_TEXT_ALIGNMENT_CENTER}else{DWRITE_TEXT_ALIGNMENT_LEADING};
        if t<1.&&!s.previous_lyric.is_empty(){self.text_family(s.previous_lyric,Rect::new(viewport.x,line_y+s.outgoing_y+(-5.-s.outgoing_y)*ease,viewport.w,line_h),s.font_size,500,((255.*s.outgoing_opacity*(1.-t))as u32)<<24|0x00f1f3f5,"Noto Sans SC",false,alignment,None)?;}
        let a=(255.*t)as u32;
        let measured=if double{self.measure_line(s.lyric,s.font_size)?.min(viewport.w)}else{viewport.w};let text_x=if double{viewport.x+(viewport.w-measured)/2.}else{viewport.x};let boundary=((text_x+measured*s.highlight.clamp(0.,1.))*s.layout_scale).round()/s.layout_scale;
        for (clip,c) in [(Rect::new(viewport.x,viewport.y,(boundary-viewport.x).max(0.),viewport.h),s.progress_color&0xffffff),(Rect::new(boundary,viewport.y,(viewport.x+viewport.w-boundary).max(0.),viewport.h),0x00f1f3f5)]{
            if clip.w>0.{self.dc.PushAxisAlignedClip(&rect(clip),D2D1_ANTIALIAS_MODE_ALIASED);self.text_family(s.lyric,Rect::new(viewport.x,line_y+dy,viewport.w,line_h),s.font_size,500,(a<<24)|c,"Noto Sans SC",false,alignment,None)?;self.dc.PopAxisAlignedClip();}
        }
        if double{if t<1.&&!s.previous_compact_secondary.is_empty(){self.text_family(s.previous_compact_secondary,Rect::new(viewport.x,line_y+line_h+1.-5.*ease,viewport.w,secondary_h),secondary_size,400,((160.*(1.-t))as u32)<<24|0x00ced4dc,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}self.text_family(s.compact_secondary,Rect::new(viewport.x,line_y+line_h+1.+dy,viewport.w,secondary_h),secondary_size,400,((190.*t)as u32)<<24|0x00ced4dc,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}
        self.dc.PopAxisAlignedClip();
        for (i,h) in [10.,14.,8.].iter().enumerate(){let scale=s.activity_scales[i];self.fill(Rect::new(c.x+c.w-22.+i as f64*4.,c.y+c.h/2.-h*scale/2.,2.,h*scale),1.,((255.*s.activity_opacity)as u32)<<24|(s.progress_color&0xffffff));}
        Ok(())
    }
    unsafe fn expanded_content(&mut self,s:&Scene<'_>)->Result<()>{
        let w=s.layout.card.w;
        let g=crate::layout::music_geometry(w,s.music_layout,s.music_mix,s.layout_scale,s.font_size);
        let lyric_width=self.measure_line(s.lyric,s.font_size+2.)?.min(g.lyric.w);
        // One anchor for every layout: 0 leading, 0.5 centred, 1 trailing. The settings
        // preview reads the same number, so both places place the line identically.
        let lyric_x=g.lyric.x+(g.lyric.w-lyric_width)*g.align;
        let sub_width=self.measure_line(s.secondary,10.5)?.min(g.secondary.w);
        let sub_x=g.secondary.x+(g.secondary.w-sub_width)*g.align;
        let secondary=0xb8000000|(s.text_color&0xffffff);
        // Artwork, text, status, timeline and controls all share one geometry with hit testing.
        self.flipping_cover(g.cover,s)?;
        self.text(s.title,Rect::new(g.title.x,g.title.y,g.title.w,g.title.h),16.,600,s.text_color)?;
        self.text(s.artist,Rect::new(g.artist.x,g.artist.y,g.artist.w,g.artist.h),10.,400,secondary)?;
        let lyric_t=(s.lyric_age_ms/180.).clamp(0.,1.);let lyric_y=4.*(1.-lyric_t).powi(3);
        let boundary=lyric_x+lyric_width*s.highlight.clamp(0.,1.);
        for (clip,ink) in [(Rect::new(lyric_x,g.lyric.y,(boundary-lyric_x).max(0.),g.lyric.h+5.),s.progress_color),(Rect::new(boundary,g.lyric.y,(lyric_x+lyric_width-boundary).max(0.),g.lyric.h+5.),s.text_color)]{if clip.w>0.{self.dc.PushAxisAlignedClip(&rect(clip),D2D1_ANTIALIAS_MODE_ALIASED);self.text_family(s.lyric,Rect::new(lyric_x,g.lyric.y+lyric_y,lyric_width,g.lyric.h),s.font_size+2.,500,((255.*lyric_t)as u32)<<24|(ink&0xffffff),"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,Some(g.lyric.h))?;self.dc.PopAxisAlignedClip();}}

        self.text(s.secondary,Rect::new(sub_x,g.secondary.y,sub_width,g.secondary.h),10.5,400,secondary)?;
        let breath=if s.playing&&s.animate{0.68+0.32*(s.time_ms/2600.*std::f64::consts::TAU).sin()}else{1.};
        let dot=4.+if s.playing&&s.animate{0.8*breath}else{0.};
        // Use the secondary lyric's actual baseline on the side layout, including language fallback.
        let secondary_line=10.5*self.body_line_spacing;let split=g.split.clamp(0.,1.);let status_line=secondary_line;
        let secondary_baseline=g.secondary.y+secondary_line*self.baseline_ratio(&self.language_family(s.secondary));
        let title_baseline=g.title.y+16.*self.body_line_spacing*self.baseline_ratio(&self.language_family(s.title));
        let mut status_rect=g.status_text;
        status_rect.y=secondary_baseline*(1.-split)+title_baseline*split-status_line*self.baseline_ratio(&self.body_family);
        // The dot rides the status line box instead of the static geometry, so its centre stays
        // on the words even though the baseline is recomputed per layout and per language.
        let cx=g.status_dot.x+g.status_dot.w/2.;let cy=status_rect.y+status_line*0.57;
        self.fill(Rect::new(cx-dot/2.,cy-dot/2.,dot,dot),dot/2.,((255.*breath)as u32)<<24|(s.progress_color&0xffffff));
        self.text_family(if !s.has_track{"等待播放"}else if s.playing{"正在播放"}else{"已暂停"},status_rect,9.,400,secondary,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,Some(status_line))?;
        let track=g.track;let progress=track.w*s.progress.clamp(0.,1.);
        self.fill(track,1.5,if s.cover_background&&self.cover.is_some(){0xff000000|crate::cover_color::mix(self.cover_palette.background,s.component_color,0.18)}else{0xff343b45});
        self.fill(Rect::new(track.x,track.y,progress,track.h),1.5,s.component_color);
        let size=if s.pressed==10{13.}else if s.hover==10{11.}else{7.};
        self.fill(Rect::new(track.x+progress-size/2.,track.y+track.h/2.-size/2.,size,size),size/2.,s.component_color);
        self.text(&time(s.elapsed),g.elapsed,9.,400,secondary)?;
        let total=if s.duration>0.{time(s.duration)}else{"--:--".into()};
        self.text_family(&total,g.total,9.,400,secondary,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
        for (id,r) in g.buttons{self.button(id,r.x,r.y,r.w,s);}
        Ok(())
    }
    unsafe fn draw_layer(&self,bitmap:&ID2D1Bitmap1,blur:f64,opacity:f32,transform:Matrix3x2,clip:(Rect,f64))->Result<()>{
        self.dc.SetTransform(&identity());self.clip(clip.0,clip.1,opacity)?;self.dc.SetTransform(&transform);
        if blur>0.05{self.blur.SetInput(0,bitmap,true);self.blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,D2D1_PROPERTY_TYPE_FLOAT,&((blur/3.)as f32).to_ne_bytes())?;self.dc.DrawImage(&self.blur.GetOutput()?,None,None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);}
        else{self.dc.DrawImage(bitmap,None,None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);}
        self.dc.PopLayer();self.dc.SetTransform(&identity());Ok(())
    }
    pub unsafe fn render(&mut self,s:&Scene<'_>,present:bool)->Result<()>{
        let mut themed=*s;if s.panel.is_some()&&s.material==Material::Classic{themed.cover_background=false;}if self.cover.is_some(){let accent=0xff000000|self.cover_palette.accent;if themed.cover_background{themed.component_color=accent;}if s.lyric_cover_color{themed.progress_color=accent;}}let s=&themed;
        let (base_shape,base_radius)=s.layout.morph_shape(s.frame.card_reveal,s.radius,s.fluid_motion);let p=s.frame.card_reveal;
        let (shape,radius)=if s.notification.is_some(){(s.notification_rect,s.notification_radius)}else if s.osd.is_some(){(s.osd_rect,s.osd_radius)}else{(base_shape,base_radius)};
        let music_alpha=if s.notification.is_some(){1.-s.notification_mix}else if s.osd.is_some(){1.-s.osd_mix}else{1.};
        let black_capsule=!s.mini&&s.classic_glass&&p<=0.05&&s.notification.is_none()&&s.osd.is_none();
        let live_glass=(s.material==Material::LiquidGlass&&p>0.05||black_capsule)&&s.notification.is_none()&&s.osd.is_none()&&s.opacity>0.01;
        if present&&live_glass{self.dc.SetTarget(None);self.glass_bitmap=self.glass.render(&self._device,&self.dc,self.hwnd,shape,radius,self.dpi,if black_capsule{0x10141c}else if s.cover_background{self.cover_palette.background}else{0x151b24},black_capsule,!s.mini,s.time_ms);self.reading_dark=false;}
        else if present{self.glass.stop();self.glass_bitmap=None;}

        if (1.-blend(p,0.06,0.46))*music_alpha>0.001 {self.dc.SetTarget(&self.compact);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));let width=s.layout.capsule.w;
            if s.drag_active{let mut neighbor=*s;neighbor.panel=s.drag_panel;self.dc.SetTransform(&matrix(1.,s.swipe_offset-s.swipe_offset.signum()*width,0.));self.compact_view(&neighbor)?;}
            if s.switching&&s.switch_outgoing{let mut old=*s;old.panel=s.previous_panel;self.dc.SetTransform(&matrix(1.,s.switch_offset-s.switch_direction*width+s.swipe_offset,0.));self.compact_view(&old)?;}
            self.dc.SetTransform(&matrix(1.,s.switch_offset+s.swipe_offset,0.));self.compact_view(s)?;self.dc.EndDraw(None,None)?;}
        if s.frame.content_opacity*blend(p,0.25,0.88)*music_alpha>0.001 {self.dc.SetTarget(&self.expanded);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));let width=s.layout.card.w;
            if s.drag_active{let mut neighbor=*s;neighbor.panel=s.drag_panel;self.dc.SetTransform(&matrix(1.,s.swipe_offset-s.swipe_offset.signum()*width,s.frame.content_offset_y));self.expanded_view(&neighbor)?;}
            if s.switching&&s.switch_outgoing{let mut old=*s;old.panel=s.previous_panel;self.dc.SetTransform(&matrix(1.,s.switch_offset-s.switch_direction*width,s.frame.content_offset_y));self.expanded_view(&old)?;}
            self.dc.SetTransform(&matrix(1.,s.switch_offset+s.swipe_offset,s.frame.content_offset_y));self.expanded_view(s)?;self.dc.EndDraw(None,None)?;}
        if self.shadow_shape!=Some((shape,radius)){self.dc.SetTarget(&self.shadow);self.dc.BeginDraw();self.dc.SetTransform(&identity());self.dc.Clear(Some(&color(0)));self.fill(shape,radius,0x01000000);self.brush.SetColor(&color(0xff000000));self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.brush,1.,None);self.dc.EndDraw(None,None)?;self.shadow_shape=Some((shape,radius));}
        self.dc.SetTarget(&self.screen);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));self.dc.SetTransform(&identity());
        // One opacity layer for pause/show fades. Geometry remains fixed.
        self.clip(Rect::new(0.,0.,s.layout.host.w,s.layout.host.h),0.,s.opacity)?;
        self.shadow_fx.SetInput(0,&self.shadow,true);self.shadow_fx.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,D2D1_PROPERTY_TYPE_FLOAT,&(22_f32/3.).to_ne_bytes())?;
        let shadow_color=[0_f32,0.,0.,if s.mini&&p<0.01{0.04}else{0.22}];let bytes=std::slice::from_raw_parts(shadow_color.as_ptr()as *const u8,16);self.shadow_fx.SetValue(D2D1_SHADOW_PROP_COLOR.0 as u32,D2D1_PROPERTY_TYPE_VECTOR4,bytes)?;
        self.dc.DrawImage(&self.shadow_fx.GetOutput()?,Some(&point(0.,5.)),None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);
        let clear=s.material==Material::LiquidGlass&&s.notification.is_none()&&s.osd.is_none();
        // Only the reading surface gains density as it expands. A collapsed Mini
        // leaves the browser tab strip visible through its low-opacity meniscus.
        let black_capsule=!s.mini&&s.classic_glass&&p<=0.05&&s.notification.is_none()&&s.osd.is_none();
        let expanded_clear=clear&&p>0.05;let optical_mix=if clear{blend(p,0.05,0.85)}else{0.};
        let glass_ready=self.glass_bitmap.is_some()||self.backdrop.is_some();
        let alpha=if expanded_clear&&!glass_ready{0xf5000000}else if black_capsule&&!glass_ready{0xfa000000}else if clear{(((if s.mini{22.+174.*p.clamp(0.,1.)}else{245.-49.*p.clamp(0.,1.)})*(1.-optical_mix)+8.*optical_mix)as u32)<<24}else if black_capsule{0x60000000}else if s.transparent{0xf5000000}else{0xff000000};
        let top=alpha|crate::cover_color::mix(0x1b1f25,self.cover_palette.background,if s.cover_background{p*music_alpha}else{0.});
        let bottom=alpha|crate::cover_color::mix(0x0c0f14,self.cover_palette.background,if s.cover_background{p*music_alpha*0.65}else{0.});
        if !self.surface_brush.as_ref().is_some_and(|(a,b,r,_)|*a==top&&*b==bottom&&*r==shape){
            let stops=[D2D1_GRADIENT_STOP{position:0.,color:color(top)},D2D1_GRADIENT_STOP{position:1.,color:color(bottom)}];
            let collection=self.dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops,D2D1_GAMMA_2_2,D2D1_EXTEND_MODE_CLAMP)?;
            let brush=self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES{startPoint:point(shape.x,shape.y),endPoint:point(shape.x,shape.y+shape.h)},None,&collection)?;
            self.surface_brush=Some((top,bottom,shape,brush));
        }
        self.dc.FillRoundedRectangle(&rounded(shape,radius),&self.surface_brush.as_ref().unwrap().3);
        // Frost the cover, never the text. This is a self-contained poster acrylic,
        // not desktop capture; the classic material and compact capsule stay intact.
        let glass=if clear&&s.mini{1.}else if s.material!=Material::Classic&&s.notification.is_none()&&s.osd.is_none(){blend(p,if s.material==Material::LiquidGlass{0.11}else{0.16},0.92)}else{0.};
        if glass>0.001&&s.material==Material::Acrylic{
            self.clip(shape,radius,glass as f32)?;
            if s.cover_background{
                if let Some(cover)=&self.cover{
                    
                    if self.poster_cover.as_ref()!=Some(cover){self.poster_blur.SetInput(0,cover,true);self.poster_cover=Some(cover.clone());}
                    // Effect bounds, unlike bitmap DIPs, stay correct across render-target DPI.
                    let output=self.poster_blur.GetOutput()?;let bounds=self.dc.GetImageLocalBounds(&output)?;
                    let width=(bounds.right-bounds.left)as f64;let height=(bounds.bottom-bounds.top)as f64;
                    let scale=(shape.w/width.max(1.)).max(shape.h/height.max(1.));
                    self.dc.SetTransform(&matrix(scale,shape.x+(shape.w-width*scale)/2.-bounds.left as f64*scale,shape.y+(shape.h-height*scale)/2.-bounds.top as f64*scale));
                    self.dc.DrawImage(&output,None,None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);
                    self.dc.SetTransform(&identity());
                }
            }
            // An opaque dark scrim bounds the brightest possible cover for legibility. Liquid
            // glass keeps a little more of the poster, but never enough to threaten white text.
            let scrim=if s.material==Material::LiquidGlass{0xbc000000}else{0xc9000000};
            self.fill(shape,radius,scrim|crate::cover_color::mix(0x10171a,self.cover_palette.background,0.4));
            self.dc.PopLayer();
        }
        if expanded_clear{self.expanded_glass(s,shape,radius,optical_mix as f32,false)?;}
        if black_capsule{self.expanded_glass(s,shape,radius,1.,true)?;}
        if clear&&!expanded_clear&&!black_capsule&&glass>0.001{self.liquid_lens(s,shape,radius,(glass*(if s.mini{0.025+0.255*p.clamp(0.,1.)}else{0.28}))as f32)?;}
        if !expanded_clear&&!black_capsule{
        let edge=if clear&&s.mini&&p<0.01{0x28ffffff}else if s.material==Material::LiquidGlass{0x5affffff}else if glass>0.001{0x42ffffff}else{0x22ffffff};
        self.brush.SetColor(&color(edge));self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.brush,1.,None);
        if !self.highlight_brush.as_ref().is_some_and(|(r,_)|*r==shape){
            let stops=[D2D1_GRADIENT_STOP{position:0.,color:color(0x44ffffff)},D2D1_GRADIENT_STOP{position:0.32,color:color(0x00ffffff)}];
            let collection=self.dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops,D2D1_GAMMA_2_2,D2D1_EXTEND_MODE_CLAMP)?;
            let brush=self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES{startPoint:point(shape.x,shape.y),endPoint:point(shape.x,shape.y+shape.h)},None,&collection)?;
            self.highlight_brush=Some((shape,brush));
        }
        self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.highlight_brush.as_ref().unwrap().1,1.,None);
        if s.material==Material::LiquidGlass&&glass>0.001{
            // A bright outer meniscus and a darker inset give the bevel optical thickness.
            self.clip(shape,radius,glass as f32)?;
            self.brush.SetColor(&color(if s.mini&&p<0.01{0x28ffffff}else{0x55ffffff}));self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.brush,1.25,None);
            let inset=Rect::new(shape.x+2.,shape.y+2.,(shape.w-4.).max(1.),(shape.h-4.).max(1.));
            self.brush.SetColor(&color(if s.mini&&p<0.01{0x14000000}else{0x48000000}));self.dc.DrawRoundedRectangle(&rounded(inset,(radius-2.).max(1.)),&self.brush,0.8,None);
            self.dc.PopLayer();
        }
        }
        let content_scale=(base_shape.w/s.layout.card.w).min(base_shape.h/s.layout.card.h);
        let tx=base_shape.x+(base_shape.w-s.layout.card.w*content_scale)/2.;let ty=base_shape.y+(base_shape.h-s.layout.card.h*content_scale)/2.;
        let alpha=s.frame.content_opacity*blend(p,0.25,0.88)*music_alpha;
        if alpha>0.001{self.draw_layer(&self.expanded,if s.resizing{s.frame.card_blur}else{0.},alpha as f32,matrix(content_scale,tx,ty),(shape,radius))?;}
        let alpha=(1.-blend(p,0.06,0.46))*music_alpha;
        if alpha>0.001{self.draw_layer(&self.compact,if s.resizing{3.*blend(p,0.06,0.4)}else{0.},alpha as f32,matrix(1.,base_shape.x-s.layout.capsule.x,base_shape.y-s.layout.capsule.y),(shape,radius))?;}
        if !s.mini&&s.component_count>1&&s.frame.card_reveal<0.06&&s.notification.is_none()&&s.osd.is_none(){self.satellite(s)?;}
        if let Some(notice)=s.notification{self.clip(shape,radius,s.notification_mix as f32)?;self.notification_content(notice,shape)?;self.dc.PopLayer();}
        if let Some(osd)=s.osd{self.clip(shape,radius,s.osd_mix as f32)?;self.osd_content(osd,shape)?;self.dc.PopLayer();}
        self.dc.PopLayer();self.dc.EndDraw(None,None)?;
        if present{self.swap.Present(1,DXGI_PRESENT(0)).ok()?;}Ok(())
    }
    pub unsafe fn set_notification_icon(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{
        let music_cover=self.cover.take();let previous=self.previous_cover.take();let changed=self.cover_changed;let flip_from=self.flip_from;let palette=self.cover_palette;let result=self.load_cover(bytes,false);self.notification_icon=self.cover.take();self.cover=music_cover;self.previous_cover=previous;self.cover_changed=changed;self.flip_from=flip_from;self.cover_palette=palette;result
    }
    unsafe fn notification_layout(&mut self,text:&str,r:Rect,size:f64,weight:i32)->Result<IDWriteTextLayout>{
        let key=format!("notice|{text}|{}|{}|{size}|{weight}",r.w,r.h);
        if !self.layouts.contains_key(&key){
            let resolved=self.language_family(text);let private=resolved==self.body_family||resolved==self.japanese_family;let baseline=self.baseline_ratio(&resolved);let family=resolved.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let format=self.write.CreateTextFormat(PCWSTR(family.as_ptr()),if private{&self.body_fonts}else{&self.system_fonts},DWRITE_FONT_WEIGHT(weight),DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size as f32,if resolved==self.japanese_family{w!("ja-JP")}else{w!("zh-CN")})?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,(size*1.32)as f32,(size*1.32*baseline)as f32)?;
            let ellipsis=self.write.CreateEllipsisTrimmingSign(&format)?;format.SetTrimming(&DWRITE_TRIMMING{granularity:DWRITE_TRIMMING_GRANULARITY_CHARACTER,..Default::default()},&ellipsis)?;
            let chars=text.encode_utf16().collect::<Vec<_>>();let layout=self.write.CreateTextLayout(&chars,&format,r.w as f32,r.h as f32)?;
            layout.cast::<IDWriteTextLayout2>()?.SetFontFallback(&self.fallback)?;layout.cast::<IDWriteTextLayout4>()?.SetFontAxisValues(&[DWRITE_FONT_AXIS_VALUE{axisTag:DWRITE_FONT_AXIS_TAG_WEIGHT,value:weight as f32}],DWRITE_TEXT_RANGE{startPosition:0,length:chars.len()as u32})?;
            if self.layouts.len()>192{self.layouts.clear()}self.layouts.insert(key.clone(),layout);
        }
        Ok(self.layouts.get(&key).unwrap().clone())
    }
    pub unsafe fn notification_height(&mut self,n:&crate::notifications::Notice,width:f64)->Result<f64>{
        if n.body.trim().is_empty(){return Ok(64.)}
        let layout=self.notification_layout(&n.body,Rect::new(0.,0.,width-100.,34.),12.5,400)?;
        let mut metrics=DWRITE_TEXT_METRICS::default();layout.GetMetrics(&mut metrics)?;
        Ok(if metrics.lineCount<=1{80.}else{96.})
    }
    unsafe fn notification_text(&mut self,text:&str,r:Rect,size:f64,weight:i32,c:u32)->Result<()>{
        let layout=self.notification_layout(text,r,size,weight)?;
        self.brush.SetColor(&color(c));self.dc.DrawTextLayout(point(r.x,r.y),&layout,&self.brush,D2D1_DRAW_TEXT_OPTIONS_CLIP);Ok(())
    }
    unsafe fn notification_content(&mut self,n:&crate::notifications::Notice,r:Rect)->Result<()>{
        let x=r.x;let y=r.y;let icon=Rect::new(x+18.,y+(r.h-40.)/2.,40.,40.);self.clip(icon,10.,1.)?;
        if n.app_id.starts_with("LyricIsland.Device"){self.device_icon(icon,n.app_id.strip_prefix("LyricIsland.Device.").unwrap_or(""))?;}else if let Some(bitmap)=&self.notification_icon{self.dc.DrawBitmap(bitmap,Some(&rect(icon)),1.,D2D1_INTERPOLATION_MODE_LINEAR,None,None);}
        else{let initial=n.app_name.chars().next().unwrap_or('•').to_string();self.fill(icon,10.,0xff3188ee);self.text(&initial,Rect::new(icon.x+10.,icon.y+4.,24.,32.),22.,600,0xffffffff)?;}
        self.dc.PopLayer();
        self.notification_text(&n.app_name,Rect::new(x+76.,y+10.,r.w-160.,17.),11.5,500,0xffb6bdc8)?;
        self.notification_text("现在",Rect::new(x+r.w-70.,y+12.,32.,14.),10.,400,0xff8f99a8)?;
        self.notification_text(&n.title,Rect::new(x+76.,y+27.,r.w-100.,21.),14.,600,0xfff4f6f8)?;
        self.notification_text(&n.body,Rect::new(x+76.,y+49.,r.w-100.,34.),12.5,400,0xffd2d7df)?;
        self.line((x+r.w-24.,y+14.),(x+r.w-17.,y+21.),0xffa6afbc,1.2);self.line((x+r.w-17.,y+14.),(x+r.w-24.,y+21.),0xffa6afbc,1.2);Ok(())
    }
    pub unsafe fn snapshot(&self,path:&Path)->std::result::Result<(),Box<dyn std::error::Error>>{
        let cpu=self.dc.CreateBitmap(D2D_SIZE_U{width:self.width,height:self.height},None,0,&D2D1_BITMAP_PROPERTIES1{pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED},dpiX:self.dpi,dpiY:self.dpi,bitmapOptions:D2D1_BITMAP_OPTIONS_CPU_READ|D2D1_BITMAP_OPTIONS_CANNOT_DRAW,..Default::default()})?;
        cpu.CopyFromBitmap(None,&self.screen,None)?;let map=cpu.Map(D2D1_MAP_OPTIONS_READ)?;let mut bytes=vec![0u8;(self.width*self.height*4)as usize];
        for y in 0..self.height as usize{let row=std::slice::from_raw_parts(map.bits.add(y*map.pitch as usize),self.width as usize*4);for x in 0..self.width as usize{let src=&row[x*4..x*4+4];let dst=&mut bytes[(y*self.width as usize+x)*4..][..4];let a=src[3]as u32;for (j,k) in [2,1,0].iter().enumerate(){dst[j]=if a>0{((src[*k]as u32*255+a/2)/a).min(255)as u8}else{0};}dst[3]=src[3];}}
        cpu.Unmap()?;image::save_buffer(path,&bytes,self.width,self.height,image::ColorType::Rgba8)?;Ok(())
    }
}
fn time(s:f64)->String{let s=s.max(0.)as u64;format!("{:02}:{:02}",s/60,s%60)}






include!("tool_renderer.rs");
include!("mini_renderer.rs");

impl Renderer{
 pub unsafe fn render_bottom_osd(&mut self,n:&crate::notifications::Notice,opacity:f64,offset:f64,present:bool)->Result<()>{
  self.dc.SetTarget(&self.screen);self.dc.SetTransform(&identity());self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));
  let r=Rect::new(16.,12.+offset,240.,44.);self.clip(Rect::new(0.,0.,272.,80.),0.,opacity as f32)?;
  self.fill(Rect::new(r.x,r.y+2.,r.w,r.h),22.,0x25000000);self.fill(r,22.,0xff1a1d22);
  self.brush.SetColor(&color(0x28ffffff));self.dc.DrawRoundedRectangle(&rounded(r,22.),&self.brush,0.7,None);
  self.osd_content(n,r)?;self.dc.PopLayer();self.dc.EndDraw(None,None)?;
  if present{self.swap.Present(1,DXGI_PRESENT(0)).ok()?;self._composition.Commit()?;}Ok(())
 }
}
