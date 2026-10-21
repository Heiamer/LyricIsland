//! DXGI desktop duplication and GPU-only, dispersive glass composition.
//! Shader sources are adapted from MIT-licensed liquidDX11; see shaders/.
use std::time::{Duration,Instant};
use windows::{core::{Interface,Result},Win32::{Foundation::{HWND,RECT},Graphics::{Direct2D::{ID2D1Bitmap1,ID2D1DeviceContext,D2D1_BITMAP_PROPERTIES1},Direct2D::Common::*,Direct3D::*,Direct3D11::*,Dxgi::{*,Common::*}},UI::WindowsAndMessaging::*}};

struct Texture{texture:ID3D11Texture2D,srv:ID3D11ShaderResourceView,rtv:ID3D11RenderTargetView,w:u32,h:u32}
impl Texture{
    unsafe fn new(device:&ID3D11Device,w:u32,h:u32)->Result<Self>{
        let desc=D3D11_TEXTURE2D_DESC{Width:w,Height:h,MipLevels:1,ArraySize:1,Format:DXGI_FORMAT_B8G8R8A8_UNORM,SampleDesc:DXGI_SAMPLE_DESC{Count:1,Quality:0},Usage:D3D11_USAGE_DEFAULT,BindFlags:(D3D11_BIND_SHADER_RESOURCE|D3D11_BIND_RENDER_TARGET).0 as u32,..Default::default()};
        let(mut texture,mut srv,mut rtv)=(None,None,None);device.CreateTexture2D(&desc,None,Some(&mut texture))?;let texture=texture.unwrap();device.CreateShaderResourceView(&texture,None,Some(&mut srv))?;device.CreateRenderTargetView(&texture,None,Some(&mut rtv))?;Ok(Self{texture,srv:srv.unwrap(),rtv:rtv.unwrap(),w,h})
    }
}
struct Pipeline{device:ID3D11Device,ctx:ID3D11DeviceContext,vs:ID3D11VertexShader,ps:ID3D11PixelShader,full_vs:ID3D11VertexShader,down:ID3D11PixelShader,up:ID3D11PixelShader,cb:ID3D11Buffer,blur_cb:ID3D11Buffer,sampler:ID3D11SamplerState,raster:ID3D11RasterizerState}
impl Pipeline{
    unsafe fn new(device:&ID3D11Device)->Result<Self>{
        let(mut vs,mut ps,mut full_vs,mut down,mut up,mut cb,mut blur_cb,mut sampler,mut raster)=(None,None,None,None,None,None,None,None,None);
        device.CreateVertexShader(include_bytes!("../shaders/glass-vs.cso"),None,Some(&mut vs))?;device.CreatePixelShader(include_bytes!("../shaders/glass-ps.cso"),None,Some(&mut ps))?;
        device.CreateVertexShader(include_bytes!("../shaders/fullscreen-vs.cso"),None,Some(&mut full_vs))?;device.CreatePixelShader(include_bytes!("../shaders/blur-down-ps.cso"),None,Some(&mut down))?;device.CreatePixelShader(include_bytes!("../shaders/blur-up-ps.cso"),None,Some(&mut up))?;
        let desc=D3D11_BUFFER_DESC{ByteWidth:416,Usage:D3D11_USAGE_DEFAULT,BindFlags:D3D11_BIND_CONSTANT_BUFFER.0 as u32,..Default::default()};device.CreateBuffer(&desc,None,Some(&mut cb))?;device.CreateBuffer(&D3D11_BUFFER_DESC{ByteWidth:16,..desc},None,Some(&mut blur_cb))?;
        let desc=D3D11_SAMPLER_DESC{Filter:D3D11_FILTER_MIN_MAG_MIP_LINEAR,AddressU:D3D11_TEXTURE_ADDRESS_CLAMP,AddressV:D3D11_TEXTURE_ADDRESS_CLAMP,AddressW:D3D11_TEXTURE_ADDRESS_CLAMP,ComparisonFunc:D3D11_COMPARISON_NEVER,MaxLOD:f32::MAX,..Default::default()};device.CreateSamplerState(&desc,Some(&mut sampler))?;
        device.CreateRasterizerState(&D3D11_RASTERIZER_DESC{FillMode:D3D11_FILL_SOLID,CullMode:D3D11_CULL_NONE,DepthClipEnable:true.into(),..Default::default()},Some(&mut raster))?;
        Ok(Self{device:device.clone(),ctx:device.GetImmediateContext()?,vs:vs.unwrap(),ps:ps.unwrap(),full_vs:full_vs.unwrap(),down:down.unwrap(),up:up.unwrap(),cb:cb.unwrap(),blur_cb:blur_cb.unwrap(),sampler:sampler.unwrap(),raster:raster.unwrap()})
    }
    unsafe fn target(&self,t:&Texture){self.ctx.PSSetShaderResources(0,Some(&[None,None]));self.ctx.OMSetRenderTargets(Some(&[Some(t.rtv.clone())]),None);self.ctx.RSSetViewports(Some(&[D3D11_VIEWPORT{Width:t.w as f32,Height:t.h as f32,MaxDepth:1.,..Default::default()}]));self.ctx.ClearRenderTargetView(&t.rtv,&[0.;4]);}
    unsafe fn blur(&self,from:&Texture,to:&Texture,up:bool,spread:f32){self.target(to);let cb=[1./from.w as f32,1./from.h as f32,spread,0.];self.ctx.UpdateSubresource(&self.blur_cb,0,None,cb.as_ptr().cast(),0,0);self.ctx.VSSetShader(&self.full_vs,None);self.ctx.PSSetShader(if up{&self.up}else{&self.down},None);self.ctx.PSSetConstantBuffers(0,Some(&[Some(self.blur_cb.clone())]));self.ctx.PSSetShaderResources(0,Some(&[Some(from.srv.clone())]));self.ctx.Draw(3,0);}
    unsafe fn draw(&self,input:&Texture,soft:&Texture,small:&Texture,heavy:&Texture,out:&Texture,origin:[f32;2],radius:f32,scale:f32,tint:u32,black:bool,dense:bool,time:f32){
        self.ctx.ClearState();self.ctx.RSSetState(&self.raster);self.ctx.IASetInputLayout(None);self.ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);self.ctx.PSSetSamplers(0,Some(&[Some(self.sampler.clone())]));
        self.blur(input,soft,false,1.25*scale);self.blur(soft,small,false,4.*scale);self.blur(small,heavy,true,2.5);
        self.target(out);let mut c=[[0f32;4];26];let(w,h)=(out.w as f32,out.h as f32);
        c[0]=[w,h,input.w as f32,input.h as f32];c[1]=[origin[0],origin[1],-0.62,-0.78];c[2]=[w/2.,h/2.,w/2.,h/2.];c[3]=[radius.min(w.min(h)/2.),1.,if black{0.12}else if dense{0.85}else{0.25},1.08];
        c[4]=[((tint>>16)&255)as f32/255.,((tint>>8)&255)as f32/255.,(tint&255)as f32/255.,1.];
        // `dense` is the reading card: a heavier blur and a firmer tint keep a window edge on
        // the desktop from reading as a shadow band across the glass. The Mini capsule keeps
        // the lighter constants it was tuned with.
        c[5]=[if black{0.80}else if dense{0.58}else{0.40},-0.015,if dense{0.88}else{1.04},0.002];c[6]=[24.*scale,22.*scale,0.38,0.08];c[7]=[0.18,1.0*scale,0.,1.];c[8]=[0.,0.,time,0.];c[9]=[0.,0.,0.12,4.0*scale];
        c[14]=[2.5,0.22,0.08,44.];c[15]=[0.22,0.45,0.38,0.65];c[16]=[2.6,0.8,0.30,170.];c[17]=[0.,0.35,0.25,0.30];c[18]=[0.7,0.7,1.,1.];c[19]=[0.7,0.7,0.8,0.35];c[20]=[0.707,0.707,0.,0.];c[21]=[if black{0.10}else if dense{0.24}else{0.34},0.60,0.25,0.12];c[22]=[2.,0.,2.,0.];c[23]=[1.,1.,1.,0.];c[24]=[0.,0.1,0.9,0.];
        self.ctx.UpdateSubresource(&self.cb,0,None,c.as_ptr().cast(),0,0);self.ctx.VSSetShader(&self.vs,None);self.ctx.PSSetShader(&self.ps,None);self.ctx.VSSetConstantBuffers(0,Some(&[Some(self.cb.clone())]));self.ctx.PSSetConstantBuffers(0,Some(&[Some(self.cb.clone())]));self.ctx.PSSetShaderResources(0,Some(&[Some(heavy.srv.clone()),Some(soft.srv.clone())]));self.ctx.Draw(6,0);self.ctx.ClearState();
    }
}
struct Surfaces{input:Texture,soft:Texture,small:Texture,heavy:Texture,out:Texture,bitmap:ID2D1Bitmap1}
impl Surfaces{
    unsafe fn new(p:&Pipeline,dc:&ID2D1DeviceContext,crop:[u32;2],size:[u32;2],dpi:f32)->Result<Self>{
        let input=Texture::new(&p.device,crop[0],crop[1])?;let soft=Texture::new(&p.device,crop[0],crop[1])?;let small=Texture::new(&p.device,(crop[0]/4).max(1),(crop[1]/4).max(1))?;let heavy=Texture::new(&p.device,crop[0],crop[1])?;let out=Texture::new(&p.device,size[0],size[1])?;
        let surface:IDXGISurface=out.texture.cast()?;let props=D2D1_BITMAP_PROPERTIES1{pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED},dpiX:dpi,dpiY:dpi,..Default::default()};let bitmap=dc.CreateBitmapFromDxgiSurface(&surface,Some(&props))?;Ok(Self{input,soft,small,heavy,out,bitmap})
    }
}
struct Capture{dup:IDXGIOutputDuplication,monitor:RECT,bounds:[i32;4],ready:bool}
#[derive(Default)]pub struct Glass{pipeline:Option<Pipeline>,capture:Option<Capture>,surfaces:Option<Surfaces>,hwnd:HWND,excluded:bool,retry:Option<Instant>,pub frames:u64,pub available:bool,pub error:Option<String>}
impl Glass{
    pub unsafe fn stop(&mut self){if self.excluded{let _=SetWindowDisplayAffinity(self.hwnd,WDA_NONE);}self.excluded=false;self.available=false;self.capture=None;self.surfaces=None;}
    unsafe fn prepare_capture(&mut self,p:&ID3D11Device,bounds:[i32;4])->Result<()>{
        let dxgi:IDXGIDevice=p.cast()?;let adapter=dxgi.GetAdapter()?;let cx=(bounds[0]+bounds[2])/2;let cy=(bounds[1]+bounds[3])/2;
        if self.capture.as_ref().is_some_and(|c|cx>=c.monitor.left&&cx<c.monitor.right&&cy>=c.monitor.top&&cy<c.monitor.bottom){return Ok(())}
        self.capture=None;self.surfaces=None;let mut n=0;while let Ok(output)=adapter.EnumOutputs(n){n+=1;let d=output.GetDesc()?;let r=d.DesktopCoordinates;if cx<r.left||cx>=r.right||cy<r.top||cy>=r.bottom{continue}if d.Rotation!=DXGI_MODE_ROTATION_IDENTITY&&d.Rotation!=DXGI_MODE_ROTATION_UNSPECIFIED{return Err(windows::core::Error::new(windows::core::HRESULT(0x80004001u32 as i32),"旋转屏幕暂不支持实时折射"))}let dup=output.cast::<IDXGIOutput1>()?.DuplicateOutput(p).map_err(|e|windows::core::Error::new(e.code(),format!("DXGI DuplicateOutput: {e}")))?;self.capture=Some(Capture{dup,monitor:r,bounds:[0;4],ready:false});return Ok(())}Err(windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32),"当前显示器不属于渲染设备"))
    }
    pub unsafe fn render(&mut self,device:&ID3D11Device,dc:&ID2D1DeviceContext,hwnd:HWND,shape:crate::layout::Rect,radius:f64,dpi:f32,tint:u32,black:bool,dense:bool,time:f64)->Option<ID2D1Bitmap1>{
        self.hwnd=hwnd;if self.retry.is_some_and(|at|Instant::now()<at){return None}
        match self.render_inner(device,dc,hwnd,shape,radius,dpi,tint,black,dense,time){Ok(bitmap)=>{self.error=None;self.available=bitmap.is_some();bitmap},Err(e)=>{self.error=Some(e.to_string());self.retry=Some(Instant::now()+Duration::from_secs(2));self.stop();None}}
    }
    unsafe fn render_inner(&mut self,device:&ID3D11Device,dc:&ID2D1DeviceContext,hwnd:HWND,shape:crate::layout::Rect,radius:f64,dpi:f32,tint:u32,black:bool,dense:bool,time:f64)->Result<Option<ID2D1Bitmap1>>{
        if !self.excluded{SetWindowDisplayAffinity(hwnd,WDA_EXCLUDEFROMCAPTURE).map_err(|e|windows::core::Error::new(e.code(),format!("SetWindowDisplayAffinity: {e}")))?;self.excluded=true;}
        let mut host=RECT::default();GetWindowRect(hwnd,&mut host)?;let scale=dpi/96.;let bounds=[host.left+(shape.x*scale as f64).round()as i32,host.top+(shape.y*scale as f64).round()as i32,host.left+((shape.x+shape.w)*scale as f64).round()as i32,host.top+((shape.y+shape.h)*scale as f64).round()as i32];
        self.prepare_capture(device,bounds)?;if self.pipeline.is_none(){self.pipeline=Some(Pipeline::new(device)?)}let p=self.pipeline.as_ref().unwrap();let cap=self.capture.as_mut().unwrap();let m=cap.monitor;
        let crop=[(host.left-48).max(m.left),(host.top-48).max(m.top),(host.right+48).min(m.right),(host.bottom+48).min(m.bottom)];let size=[(bounds[2]-bounds[0]).max(1)as u32,(bounds[3]-bounds[1]).max(1)as u32];let crop_size=[(crop[2]-crop[0]).max(1)as u32,(crop[3]-crop[1]).max(1)as u32];
        if !self.surfaces.as_ref().is_some_and(|s|s.input.w==crop_size[0]&&s.input.h==crop_size[1]&&s.out.w==size[0]&&s.out.h==size[1]){self.surfaces=Some(Surfaces::new(p,dc,crop_size,size,dpi)?);cap.ready=false;}
        let s=self.surfaces.as_ref().unwrap();let mut info=DXGI_OUTDUPL_FRAME_INFO::default();let mut resource=None;
        match cap.dup.AcquireNextFrame(0,&mut info,&mut resource){Ok(())=>{let result=(||->Result<()>{let source=resource.unwrap().cast::<ID3D11Texture2D>()?;let b=D3D11_BOX{left:(crop[0]-m.left)as u32,top:(crop[1]-m.top)as u32,front:0,right:(crop[2]-m.left)as u32,bottom:(crop[3]-m.top)as u32,back:1};p.ctx.CopySubresourceRegion(&s.input.texture,0,0,0,0,&source,0,Some(&b));Ok(())})();let _=cap.dup.ReleaseFrame();result?;cap.ready=true;cap.bounds=crop;self.frames+=1;},Err(e)if e.code()==DXGI_ERROR_WAIT_TIMEOUT=>{},Err(e)=>return Err(e)}
        if !cap.ready{return Ok(None)}p.draw(&s.input,&s.soft,&s.small,&s.heavy,&s.out,[(bounds[0]-cap.bounds[0])as f32,(bounds[1]-cap.bounds[1])as f32],radius as f32*scale,scale,tint,black,dense,time as f32/1000.);Ok(Some(s.bitmap.clone()))
    }
    pub unsafe fn fixture(&mut self,device:&ID3D11Device,dc:&ID2D1DeviceContext,pixels:&[u8],w:u32,h:u32,dpi:f32,radius:f64,tint:u32,black:bool)->Result<ID2D1Bitmap1>{
        if self.pipeline.is_none(){self.pipeline=Some(Pipeline::new(device)?)}let p=self.pipeline.as_ref().unwrap();let s=Surfaces::new(p,dc,[w,h],[w,h],dpi)?;p.ctx.UpdateSubresource(&s.input.texture,0,None,pixels.as_ptr().cast(),w*4,0);p.draw(&s.input,&s.soft,&s.small,&s.heavy,&s.out,[0.;2],radius as f32*dpi/96.,dpi/96.,tint,black,!black,0.);let bitmap=s.bitmap.clone();self.surfaces=Some(s);Ok(bitmap)
    }
}
impl Drop for Glass{fn drop(&mut self){unsafe{self.stop();}}}
