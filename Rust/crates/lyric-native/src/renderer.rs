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
pub struct Scene<'a>{
    pub notification:Option<&'a crate::notifications::Notice>,pub notification_rect:Rect,pub notification_radius:f64,pub notification_mix:f64,
    pub layout:Layout,pub frame:Frame,pub radius:f64,pub resizing:bool,pub time_ms:f64,
    pub lyric:&'a str,pub previous_lyric:&'a str,pub lyric_age_ms:f64,pub secondary:&'a str,
    pub title:&'a str,pub artist:&'a str,pub playing:bool,pub highlight:f64,
    pub progress:f64,pub elapsed:f64,pub duration:f64,pub font_size:f64,
    pub text_color:u32,pub component_color:u32,pub progress_color:u32,
    pub hover:i32,pub pressed:i32,pub opacity:f32,
    pub transparent:bool,pub floating:bool,pub favorite_enabled:bool,pub has_track:bool,pub layout_scale:f64,
    pub activity_scales:[f64;3],pub activity_opacity:f64,pub cover_opacity:f64,
    pub outgoing_opacity:f64,pub outgoing_y:f64,
}
pub struct Renderer{
    notification_icon:Option<ID2D1Bitmap1>,
    factory:ID2D1Factory1,pub dc:ID2D1DeviceContext,swap:IDXGISwapChain1,
    _device:ID3D11Device,_composition:IDCompositionDevice,_target:IDCompositionTarget,_visual:IDCompositionVisual,
    screen:ID2D1Bitmap1,compact:ID2D1Bitmap1,expanded:ID2D1Bitmap1,shadow:ID2D1Bitmap1,
    blur:ID2D1Effect,shadow_fx:ID2D1Effect,brush:ID2D1SolidColorBrush,
    write:IDWriteFactory,body_fonts:IDWriteFontCollection,system_fonts:IDWriteFontCollection,fallback:IDWriteFontFallback,body_family:String,japanese_family:String,
    pub body_line_spacing:f64,pub body_baseline:f64,
    layouts:HashMap<String,IDWriteTextLayout>,cover:Option<ID2D1Bitmap1>,pub width:u32,pub height:u32,pub dpi:f32,
}
impl Renderer{
    pub unsafe fn verify_font(&mut self)->Result<serde_json::Value>{
        let mut rows=vec![];self.dc.BeginDraw();
        for weight in [400,500,600]{
            let text="每一段旋律，都有自己的光 Lyric Island 0123456789 译";
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
        let composition:IDCompositionDevice=DCompositionCreateDevice(&dxgi)?;let target=composition.CreateTargetForHwnd(hwnd,true)?;let visual=composition.CreateVisual()?;visual.SetContent(&swap)?;target.SetRoot(&visual)?;composition.Commit()?;
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
        Ok(Self{factory,dc,swap,_device:device,_composition:composition,_target:target,_visual:visual,screen,compact,expanded,shadow,brush,write,body_fonts,system_fonts,fallback,body_family,japanese_family,body_line_spacing,body_baseline,layouts:HashMap::new(),cover:None,notification_icon:None,width,height,dpi,blur,shadow_fx})
    }
    pub unsafe fn set_cover(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{
        use windows::Win32::{Graphics::Imaging::*,System::Com::{CoCreateInstance,CLSCTX_INPROC_SERVER}};
        self.cover=None;if let Some(bytes)=bytes{
            let factory:IWICImagingFactory=CoCreateInstance(&CLSID_WICImagingFactory,None,CLSCTX_INPROC_SERVER)?;
            let stream=factory.CreateStream()?;stream.InitializeFromMemory(bytes)?;
            let decoder=factory.CreateDecoderFromStream(&stream,std::ptr::null(),WICDecodeMetadataCacheOnLoad)?;let frame=decoder.GetFrame(0)?;
            let (mut width,mut height)=(0,0);frame.GetSize(&mut width,&mut height)?;
            if width==0||height==0{return Err("封面尺寸无效".into())}
            let scaled_height=((height as f64/width as f64)*192.).round().clamp(1.,16384.)as u32;
            let scaler=factory.CreateBitmapScaler()?;scaler.Initialize(&frame,192,scaled_height,WICBitmapInterpolationModeFant)?;
            let converter=factory.CreateFormatConverter()?;converter.Initialize(&scaler,&GUID_WICPixelFormat32bppPBGRA,WICBitmapDitherTypeNone,None,0.,WICBitmapPaletteTypeCustom)?;
            self.cover=Some(self.dc.CreateBitmapFromWicBitmap(&converter,None)?);
        }Ok(())
    }
    unsafe fn fill(&self,r:Rect,radius:f64,c:u32){self.brush.SetColor(&color(c));self.dc.FillRoundedRectangle(&rounded(r,radius),&self.brush);}
    unsafe fn line(&self,a:(f64,f64),b:(f64,f64),c:u32,width:f32){self.brush.SetColor(&color(c));self.dc.DrawLine(point(a.0,a.1),point(b.0,b.1),&self.brush,width,None);}
    unsafe fn clip(&self,r:Rect,radius:f64,opacity:f32)->Result<()>{
        let geometry=self.factory.CreateRoundedRectangleGeometry(&rounded(r,radius))?;
        let mut params=D2D1_LAYER_PARAMETERS1{contentBounds:D2D_RECT_F{left:-100000.,top:-100000.,right:100000.,bottom:100000.},geometricMask:std::mem::ManuallyDrop::new(Some(geometry.cast()?)),maskAntialiasMode:D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,maskTransform:identity(),opacity,opacityBrush:std::mem::ManuallyDrop::new(None),layerOptions:D2D1_LAYER_OPTIONS1_NONE};
        self.dc.PushLayer(&params,None);std::mem::ManuallyDrop::drop(&mut params.geometricMask);Ok(())
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
        if text.is_empty()||r.w<=0.{return Ok(())}let key=format!("{}|{}|{}|{}|{}|{}|{}|{:?}",text,r.w,r.h,size,weight,family,alignment.0,line_height);
        if !self.layouts.contains_key(&key){
            if self.layouts.len()>192{self.layouts.clear()}
            let resolved=if system{family.to_string()}else{self.language_family(text)};let private=!system&&(resolved==self.body_family||resolved==self.japanese_family);let baseline=self.baseline_ratio(&resolved);
            let family=resolved.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let format=self.write.CreateTextFormat(PCWSTR(family.as_ptr()),if private{&self.body_fonts}else{&self.system_fonts},DWRITE_FONT_WEIGHT(weight),DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size as f32,if resolved==self.japanese_family||resolved.starts_with("Yu ")||resolved=="Meiryo"{w!("ja-JP")}else{w!("zh-CN")})?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            if !system{format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;let height=line_height.unwrap_or(size*self.body_line_spacing);format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,height as f32,(height*baseline)as f32)?;}
            format.SetTextAlignment(alignment)?;
            let ellipsis=self.write.CreateEllipsisTrimmingSign(&format)?;format.SetTrimming(&DWRITE_TRIMMING{granularity:DWRITE_TRIMMING_GRANULARITY_CHARACTER,..Default::default()},&ellipsis)?;
            let utf16=text.encode_utf16().collect::<Vec<_>>();let layout=self.write.CreateTextLayout(&utf16,&format,r.w as f32,r.h.max(size*self.body_line_spacing)as f32)?;
            if !system{layout.cast::<IDWriteTextLayout2>()?.SetFontFallback(&self.fallback)?;layout.cast::<IDWriteTextLayout4>()?.SetFontAxisValues(&[DWRITE_FONT_AXIS_VALUE{axisTag:DWRITE_FONT_AXIS_TAG_WEIGHT,value:weight as f32}],DWRITE_TEXT_RANGE{startPosition:0,length:utf16.len()as u32})?;}self.layouts.insert(key.clone(),layout);
        }
        self.brush.SetColor(&color(c));self.dc.DrawTextLayout(point(r.x,r.y),self.layouts.get(&key).unwrap(),&self.brush,D2D1_DRAW_TEXT_OPTIONS_CLIP);Ok(())
    }
    /// Center the visible glyph, rather than the font's line box. The text
    /// translation icon and Fluent gear have different ascent/descent metrics.
    unsafe fn centered_icon(&mut self,text:&str,r:Rect,size:f64,c:u32,family:&str,body:bool)->Result<()>{
        let key=format!("icon|{text}|{family}|{size}");
        if !self.layouts.contains_key(&key){
            let name=if body{self.body_family.as_str()}else{family}.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let format=self.write.CreateTextFormat(PCWSTR(name.as_ptr()),if body{&self.body_fonts}else{&self.system_fonts},DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size as f32,w!("en-US"))?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let text=text.encode_utf16().collect::<Vec<_>>();let layout=self.write.CreateTextLayout(&text,&format,32.,32.)?;
            if body{layout.cast::<IDWriteTextLayout2>()?.SetFontFallback(&self.fallback)?;}
            let ink=layout.GetOverhangMetrics()?;let height=32.+ink.top+ink.bottom;
            if height>0.{layout.SetFontSize(size as f32*13./height,DWRITE_TEXT_RANGE{startPosition:0,length:text.len()as u32})?;}
            self.layouts.insert(key.clone(),layout);
        }
        let layout=self.layouts.get(&key).unwrap();let ink=layout.GetOverhangMetrics()?;
        let center_x=(-ink.left+32.+ink.right)/2.;let center_y=(-ink.top+32.+ink.bottom)/2.;
        self.brush.SetColor(&color(c));self.dc.DrawTextLayout(point(r.x+r.w/2.-center_x as f64,r.y+r.h/2.-center_y as f64),layout,&self.brush,D2D1_DRAW_TEXT_OPTIONS_NONE);Ok(())
    }
    unsafe fn cover_with_opacity(&mut self,x:f64,y:f64,size:f64,opacity:f32)->Result<()>{
        let r=Rect::new(x,y,size,size);self.clip(r,size/6.,1.)?;self.fill(r,0.,if size>24.{0xff244b42}else{0xff242a31});
        if let Some(b)=&self.cover {let s=b.GetSize();let edge=s.width.min(s.height);let source=D2D_RECT_F{left:(s.width-edge)/2.,top:(s.height-edge)/2.,right:(s.width+edge)/2.,bottom:(s.height+edge)/2.};self.dc.DrawBitmap(b,Some(&rect(r)),opacity,D2D1_INTERPOLATION_MODE_LINEAR,Some(&source),None);}
        else{self.text_family("♫",r,size*13./24.,400,0xffc5f0dd,"Segoe UI Symbol",true,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}
        self.dc.PopLayer();Ok(())
    }
    unsafe fn button(&mut self,id:i32,x:f64,y:f64,size:f64,scene:&Scene<'_>){
        let disabled=(id==2&&!scene.floating)||(id==9&&!scene.favorite_enabled)||([3,4,5].contains(&id)&&!scene.has_track);
        let with_alpha=|c:u32|if disabled{0x4d000000|(c&0xffffff)}else{c};
        if id==4{self.fill(Rect::new(x,y,size,size),size/2.,with_alpha(scene.component_color))}
        if scene.hover==id{self.fill(Rect::new(x,y,size,size),size/2.,0x18ffffff)}
        let scale=if scene.pressed==id&&!disabled{0.94}else{1.};let ox=x+(size-16.*scale)/2.;let oy=y+(size-16.*scale)/2.;let c=with_alpha(if id==4{0xff12251d}else if id==8{0xff8c99a8}else{scene.component_color});
        let l=|a:(f64,f64),b:(f64,f64)|self.line((ox+a.0*scale,oy+a.1*scale),(ox+b.0*scale,oy+b.1*scale),c,if id==4{2.}else{1.4});
        match id{
            2=>{l((8.,1.),(8.,15.));l((1.,8.),(15.,8.));for (a,b) in [((5.,4.),(8.,1.)),((8.,1.),(11.,4.)),((4.,5.),(1.,8.)),((1.,8.),(4.,11.)),((5.,12.),(8.,15.)),((8.,15.),(11.,12.)),((12.,5.),(15.,8.)),((15.,8.),(12.,11.))]{l(a,b)}},
            3=>{l((3.,2.),(3.,14.));l((14.,2.),(5.,8.));l((5.,8.),(14.,14.));l((14.,14.),(14.,2.));},
            4=>{if scene.playing{l((5.,2.),(5.,14.));l((11.,2.),(11.,14.));}else{l((4.,2.),(13.,8.));l((13.,8.),(4.,14.));l((4.,14.),(4.,2.));}},
            5=>{l((13.,2.),(13.,14.));l((2.,2.),(11.,8.));l((11.,8.),(2.,14.));l((2.,14.),(2.,2.));},
            6=>{let _=self.centered_icon("译",Rect::new(x,y,size,size),12.,c,"Noto Sans SC",true);},
            7=>{let _=self.centered_icon("\u{e713}",Rect::new(x,y,size,size),13.,c,"Segoe Fluent Icons",false);},
            8=>{l((3.,10.),(8.,5.));l((8.,5.),(13.,10.));},
            9=>{let result=(||->Result<()>{let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;let p=|x:f64,y:f64|point(ox+x*scale,oy+y*scale);sink.BeginFigure(p(8.,14.),D2D1_FIGURE_BEGIN_HOLLOW);for v in [[4.,11.,1.,8.,1.,5.],[1.,1.,6.,0.,8.,4.],[10.,0.,15.,1.,15.,5.],[15.,8.,12.,11.,8.,14.]]{sink.AddBezier(&D2D1_BEZIER_SEGMENT{point1:p(v[0],v[1]),point2:p(v[2],v[3]),point3:p(v[4],v[5])});}sink.EndFigure(D2D1_FIGURE_END_CLOSED);sink.Close()?;self.brush.SetColor(&color(c));self.dc.DrawGeometry(&path,&self.brush,1.4,None);Ok(())})();let _=result;},_=>{}
        }
    }
    unsafe fn compact_content(&mut self,s:&Scene<'_>)->Result<()>{
        let c=s.layout.capsule;let cover=s.layout.compact_cover(s.layout_scale);self.cover_with_opacity(cover.x,cover.y,cover.w,s.cover_opacity as f32)?;
        let round=|v:f64|(v*s.layout_scale).round_ties_even()/s.layout_scale;
        let line_h=round(s.font_size*self.body_line_spacing);let line_y=c.y+round((40.-line_h)/2.);
        let viewport=Rect::new(c.x+round(7.)+30.+round(5.),line_y,c.w-14.-30.-22.-10.,line_h);self.dc.PushAxisAlignedClip(&rect(viewport),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        let t=(s.lyric_age_ms/180.).clamp(0.,1.);let ease=1.-(1.-t).powi(3);let dy=(1.-ease)*4.;
        if t<1.&&!s.previous_lyric.is_empty(){self.text(s.previous_lyric,Rect::new(viewport.x,viewport.y+s.outgoing_y+(-4.-s.outgoing_y)*ease,viewport.w,line_h),s.font_size,500,((255.*s.outgoing_opacity*(1.-t))as u32)<<24|0x00f1f3f5)?;}
        let a=(255.*t)as u32;
        let boundary=((viewport.x+viewport.w*s.highlight.clamp(0.,1.))*s.layout_scale).round()/s.layout_scale;
        // Disjoint color regions: never paint antialiased glyph edges twice.
        for (clip,c) in [(Rect::new(viewport.x,viewport.y,(boundary-viewport.x).max(0.),line_h),s.progress_color&0xffffff),(Rect::new(boundary,viewport.y,(viewport.x+viewport.w-boundary).max(0.),line_h),0x00f1f3f5)]{
            if clip.w>0.{self.dc.PushAxisAlignedClip(&rect(clip),D2D1_ANTIALIAS_MODE_ALIASED);self.text(s.lyric,Rect::new(viewport.x,viewport.y+dy,viewport.w,line_h),s.font_size,500,(a<<24)|c)?;self.dc.PopAxisAlignedClip();}
        }
        self.dc.PopAxisAlignedClip();
        for (i,h) in [10.,14.,8.].iter().enumerate(){let scale=s.activity_scales[i];self.fill(Rect::new(c.x+c.w-22.+i as f64*4.,c.y+20.-h*scale/2.,2.,h*scale),1.,((255.*s.activity_opacity)as u32)<<24|(s.component_color&0xffffff));}
        Ok(())
    }
    unsafe fn expanded_content(&mut self,s:&Scene<'_>)->Result<()>{
        let round=|v:f64|(v*s.layout_scale).round_ties_even()/s.layout_scale;
        let title_h=round(16.*self.body_line_spacing);let artist_h=round(12.5*self.body_line_spacing);
        let title_y=16.+round((42.-title_h-artist_h-4.)/2.);let artist_y=title_y+title_h+4.;
        let elapsed_y=134.+round((10.-round(10.*self.body_line_spacing))/2.);
        let secondary_h=round(11.5*self.body_line_spacing);let lyric_y_base=64.+round((46.-20.-secondary_h-4.)/2.);let status_h=round(10.*self.body_line_spacing);let status_y=110.-status_h;
        let w=s.layout.card.w;let secondary=0xc2000000|(s.text_color&0xffffff);self.cover_with_opacity(18.,16.,80.,s.cover_opacity as f32)?;
        self.fill(Rect::new(18.,status_y+(status_h-4.)/2.,4.,4.),2.,s.progress_color);self.text(if !s.has_track{"等待播放"}else if s.playing{"正在播放"}else{"已暂停"},Rect::new(28.,status_y,72.,status_h),10.,400,secondary)?;
        self.text(s.title,Rect::new(114.,title_y,w-164.,title_h),16.,600,s.text_color)?;self.text(s.artist,Rect::new(114.,artist_y,w-164.,artist_h),12.5,400,secondary)?;
        let lyric_t=(s.lyric_age_ms/180.).clamp(0.,1.);let lyric_y=5.*(1.-lyric_t).powi(3);
        self.text_family(s.lyric,Rect::new(114.,lyric_y_base+lyric_y,w-132.,20.),s.font_size+3.,500,((255.*lyric_t)as u32)<<24|(s.text_color&0xffffff),"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,Some(20.))?;self.text(s.secondary,Rect::new(114.,lyric_y_base+24.,w-132.,secondary_h),11.5,400,secondary)?;
        self.fill(Rect::new(18.,122.,w-36.,4.),2.,0xff343b45);let p=(w-36.)*s.progress.clamp(0.,1.);self.fill(Rect::new(18.,122.,p,4.),2.,s.component_color);let size=if s.pressed==10{14.}else if s.hover==10{12.}else{10.};self.fill(Rect::new(18.+p-size/2.,124.-size/2.,size,size),size/2.,s.component_color);
        self.text(&time(s.elapsed),Rect::new(18.,elapsed_y,70.,status_h),10.,400,secondary)?;let total=if s.duration>0.{time(s.duration)}else{"--:--".into()};self.text_family(&total,Rect::new(w-90.,elapsed_y,72.,status_h),10.,400,secondary,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
        for (id,r) in crate::layout::buttons(w,s.layout_scale){self.button(id,r.x,r.y,r.w,s);}
        Ok(())
    }
    unsafe fn draw_layer(&self,bitmap:&ID2D1Bitmap1,blur:f64,opacity:f32,transform:Matrix3x2,clip:(Rect,f64))->Result<()>{
        self.dc.SetTransform(&identity());self.clip(clip.0,clip.1,opacity)?;self.dc.SetTransform(&transform);
        if blur>0.05{self.blur.SetInput(0,bitmap,true);self.blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,D2D1_PROPERTY_TYPE_FLOAT,&((blur/3.)as f32).to_ne_bytes())?;self.dc.DrawImage(&self.blur.GetOutput()?,None,None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);}
        else{self.dc.DrawImage(bitmap,None,None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);}
        self.dc.PopLayer();self.dc.SetTransform(&identity());Ok(())
    }
    pub unsafe fn render(&mut self,s:&Scene<'_>,present:bool)->Result<()>{
        let (base_shape,base_radius)=s.layout.shape(s.frame.card_reveal,s.radius);let p=s.frame.card_reveal;
        let (shape,radius)=if s.notification.is_some(){(s.notification_rect,s.notification_radius)}else{(base_shape,base_radius)};
        let music_alpha=if s.notification.is_some(){1.-s.notification_mix}else{1.};
        self.dc.SetTarget(&self.compact);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));self.dc.SetTransform(&identity());self.compact_content(s)?;self.dc.EndDraw(None,None)?;
        self.dc.SetTarget(&self.expanded);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));self.dc.SetTransform(&matrix(1.,0.,s.frame.content_offset_y));self.expanded_content(s)?;self.dc.EndDraw(None,None)?;
        self.dc.SetTarget(&self.shadow);self.dc.BeginDraw();self.dc.SetTransform(&identity());self.dc.Clear(Some(&color(0)));self.fill(shape,radius,0x01000000);self.brush.SetColor(&color(0xff000000));self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.brush,1.,None);self.dc.EndDraw(None,None)?;
        self.dc.SetTarget(&self.screen);self.dc.BeginDraw();self.dc.Clear(Some(&color(0)));self.dc.SetTransform(&identity());
        // One opacity layer for pause/show fades. Geometry remains fixed.
        self.clip(Rect::new(0.,0.,s.layout.host.w,s.layout.host.h),0.,s.opacity)?;
        self.shadow_fx.SetInput(0,&self.shadow,true);self.shadow_fx.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,D2D1_PROPERTY_TYPE_FLOAT,&(22_f32/3.).to_ne_bytes())?;
        let shadow_color=[0_f32,0.,0.,0.3];let bytes=std::slice::from_raw_parts(shadow_color.as_ptr()as *const u8,16);self.shadow_fx.SetValue(D2D1_SHADOW_PROP_COLOR.0 as u32,D2D1_PROPERTY_TYPE_VECTOR4,bytes)?;
        self.dc.DrawImage(&self.shadow_fx.GetOutput()?,Some(&point(0.,5.)),None,D2D1_INTERPOLATION_MODE_LINEAR,D2D1_COMPOSITE_MODE_SOURCE_OVER);
        let alpha=if s.transparent{0xf5000000}else{0xff000000};
        let stops=[D2D1_GRADIENT_STOP{position:0.,color:color(alpha|0x1b1f25)},D2D1_GRADIENT_STOP{position:1.,color:color(alpha|0x0c0f14)}];
        let collection=self.dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops,D2D1_GAMMA_2_2,D2D1_EXTEND_MODE_CLAMP)?;
        let gradient=self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES{startPoint:point(shape.x,shape.y),endPoint:point(shape.x,shape.y+shape.h)},None,&collection)?;
        self.dc.FillRoundedRectangle(&rounded(shape,radius),&gradient);self.brush.SetColor(&color(0x22ffffff));self.dc.DrawRoundedRectangle(&rounded(shape,radius),&self.brush,1.,None);
        let stops=[D2D1_GRADIENT_STOP{position:0.,color:color(0x44ffffff)},D2D1_GRADIENT_STOP{position:0.32,color:color(0x00ffffff)}];let collection=self.dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops,D2D1_GAMMA_2_2,D2D1_EXTEND_MODE_CLAMP)?;let highlight=self.dc.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES{startPoint:point(shape.x,shape.y),endPoint:point(shape.x,shape.y+shape.h)},None,&collection)?;self.dc.DrawRoundedRectangle(&rounded(shape,radius),&highlight,1.,None);
        let content_scale=(base_shape.w/s.layout.card.w).min(base_shape.h/s.layout.card.h);
        let tx=base_shape.x+(base_shape.w-s.layout.card.w*content_scale)/2.;let ty=base_shape.y+(base_shape.h-s.layout.card.h*content_scale)/2.;
        let alpha=s.frame.content_opacity*blend(p,0.25,0.88)*music_alpha;
        if alpha>0.001{self.draw_layer(&self.expanded,if s.resizing{s.frame.card_blur}else{0.},alpha as f32,matrix(content_scale,tx,ty),(shape,radius))?;}
        let alpha=(1.-blend(p,0.06,0.46))*music_alpha;
        if alpha>0.001{self.draw_layer(&self.compact,if s.resizing{3.*blend(p,0.06,0.4)}else{0.},alpha as f32,identity(),(s.layout.capsule,20.))?;}
        if let Some(notice)=s.notification{self.clip(shape,radius,s.notification_mix as f32)?;self.notification_content(notice,shape)?;self.dc.PopLayer();}
        self.dc.PopLayer();self.dc.EndDraw(None,None)?;
        if present{self.swap.Present(1,DXGI_PRESENT(0)).ok()?;}Ok(())
    }
    pub unsafe fn set_notification_icon(&mut self,bytes:Option<&[u8]>)->std::result::Result<(),Box<dyn std::error::Error>>{
        let music_cover=self.cover.take();let result=self.set_cover(bytes);self.notification_icon=self.cover.take();self.cover=music_cover;result
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
        if let Some(bitmap)=&self.notification_icon{self.dc.DrawBitmap(bitmap,Some(&rect(icon)),1.,D2D1_INTERPOLATION_MODE_LINEAR,None,None);}
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





