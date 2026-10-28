//! CPU path preparation and GPU rasterization, presented with premultiplied alpha.
use std::{path::Path,cell::RefCell};
use windows::{core::Interface,Win32::{Foundation::HWND,Graphics::DirectComposition::*}};
use vello::{Scene,Renderer,wgpu};

pub fn composition_instance()->wgpu::Instance{
    let mut descriptor=wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends=wgpu::Backends::DX12;
    descriptor.backend_options.dx12.shader_compiler=wgpu::Dx12Compiler::Fxc;
    wgpu::Instance::new(descriptor)
}
fn device(instance:&wgpu::Instance,surface:Option<&wgpu::Surface<'_>>)->Result<(wgpu::Adapter,wgpu::Device,wgpu::Queue),String>{
    let adapter=pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions{compatible_surface:surface,power_preference:wgpu::PowerPreference::LowPower,..Default::default()})).map_err(|e|e.to_string())?;
    let(device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor{label:Some("DotIsle Hybrid"),required_limits:wgpu::Limits{max_non_sampler_bindings:4096,..Default::default()},memory_hints:wgpu::MemoryHints::Manual{suballocated_device_memory_block_size:4*1024*1024..4*1024*1024},..Default::default()})).map_err(|e|e.to_string())?;
    if std::env::var_os("DOTISLE_TRACE_GPU").is_some(){eprintln!("hybrid adapter {:?}",adapter.get_info());}
    Ok((adapter,device,queue))
}
fn texture(device:&wgpu::Device,width:u32,height:u32,format:wgpu::TextureFormat)->wgpu::Texture{
    device.create_texture(&wgpu::TextureDescriptor{label:Some("DotIsle hybrid alpha plane"),size:wgpu::Extent3d{width,height,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format,usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_SRC,view_formats:&[]})
}
fn save_texture(device:&wgpu::Device,queue:&wgpu::Queue,texture:&wgpu::Texture,path:&Path,straight_alpha:bool)->Result<(),String>{
    let w=texture.width();let h=texture.height();let row=(w*4).div_ceil(256)*256;
    let buffer=device.create_buffer(&wgpu::BufferDescriptor{label:Some("DotIsle readback"),size:row as u64*h as u64,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let mut encoder=device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo{texture,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},wgpu::TexelCopyBufferInfo{buffer:&buffer,layout:wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(row),rows_per_image:Some(h)}},wgpu::Extent3d{width:w,height:h,depth_or_array_layers:1});queue.submit([encoder.finish()]);
    let(tx,rx)=std::sync::mpsc::channel();buffer.slice(..).map_async(wgpu::MapMode::Read,move|r|{let _=tx.send(r);});
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e|e.to_string())?;rx.recv().map_err(|e|e.to_string())?.map_err(|e|e.to_string())?;
    let mapped=buffer.slice(..).get_mapped_range();let mut pixels=Vec::with_capacity((w*h*4)as usize);
    for line in mapped.chunks(row as usize){for p in line[..w as usize*4].chunks_exact(4){
        let a=p[3]as u32;let channel=|i:usize|if straight_alpha&&a>0{(p[i]as u32*255/a).min(255)as u8}else{p[i]};
        let bgra=texture.format()==wgpu::TextureFormat::Bgra8Unorm;
        pixels.extend_from_slice(&[channel(if bgra{2}else{0}),channel(1),channel(if bgra{0}else{2}),p[3]]);
    }}
    drop(mapped);buffer.unmap();image::save_buffer(path,&pixels,w,h,image::ColorType::Rgba8).map_err(|e|e.to_string())
}

pub struct Surface {
    surface:wgpu::Surface<'static>,config:wgpu::SurfaceConfiguration,
    target_texture:RefCell<Option<wgpu::Texture>>,last_scene:Option<(Scene,f64)>,
    renderer:RefCell<Renderer>,device:wgpu::Device,queue:wgpu::Queue,startup_cleanup:bool,
    _instance:wgpu::Instance,_composition:IDCompositionDevice,_target:IDCompositionTarget,_visual:IDCompositionVisual,
}
impl Surface {
    pub unsafe fn new(hwnd:HWND,width:u32,height:u32)->Result<Self,String>{
        let phase=std::time::Instant::now();
        let composition:IDCompositionDevice=DCompositionCreateDevice2(None::<&windows::core::IUnknown>).map_err(|e|e.to_string())?;
        let target=composition.CreateTargetForHwnd(hwnd,true).map_err(|e|e.to_string())?;
        let visual=composition.CreateVisual().map_err(|e|e.to_string())?;target.SetRoot(&visual).map_err(|e|e.to_string())?;
        let instance=composition_instance();
        let raw=instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(visual.as_raw())).map_err(|e|e.to_string())?;
        let(adapter,device,queue)=device(&instance,Some(&raw))?;
        let caps=raw.get_capabilities(&adapter);
        let format=caps.formats.into_iter().find(|f|matches!(f,wgpu::TextureFormat::Rgba8Unorm|wgpu::TextureFormat::Bgra8Unorm)).ok_or("unsupported composition format")?;
        let config=wgpu::SurfaceConfiguration{usage:wgpu::TextureUsages::RENDER_ATTACHMENT,format,width:width.max(1),height:height.max(1),present_mode:wgpu::PresentMode::AutoVsync,desired_maximum_frame_latency:1,alpha_mode:wgpu::CompositeAlphaMode::PreMultiplied,view_formats:vec![]};
        raw.configure(&device,&config);
        let renderer=RefCell::new(Renderer::new(&device,format,config.width,config.height));
        composition.Commit().map_err(|e|e.to_string())?;
        if std::env::var_os("DOTISLE_TRACE_GPU").is_some(){eprintln!("hybrid surface initialized {:?}",phase.elapsed());}
        Ok(Self{surface:raw,config,target_texture:RefCell::new(None),last_scene:None,renderer,device,queue,startup_cleanup:true,_instance:instance,_composition:composition,_target:target,_visual:visual})
    }
    pub fn wait_idle(&self){let _=self.device.poll(wgpu::PollType::wait_indefinitely());}
    pub fn resize(&mut self,width:u32,height:u32){
        let width=width.max(1);let height=height.max(1);if width==self.config.width&&height==self.config.height{return}
        self.config.width=width;self.config.height=height;self.surface.configure(&self.device,&self.config);
        *self.target_texture.get_mut()=None;
    }
    pub fn draw(&mut self,scene:&Scene,scale:f64,present:bool)->Result<(),String>{
        let frame=if present{match self.surface.get_current_texture(){
            wgpu::CurrentSurfaceTexture::Success(frame)|wgpu::CurrentSurfaceTexture::Suboptimal(frame)=>Some(frame),
            wgpu::CurrentSurfaceTexture::Timeout|wgpu::CurrentSurfaceTexture::Occluded=>None,
            wgpu::CurrentSurfaceTexture::Outdated=>{self.surface.configure(&self.device,&self.config);return Ok(())},
            wgpu::CurrentSurfaceTexture::Lost=>{
                self.surface=unsafe{self._instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(self._visual.as_raw()))}.map_err(|e|e.to_string())?;
                self.surface.configure(&self.device,&self.config);return Ok(())
            }
            wgpu::CurrentSurfaceTexture::Validation=>return Err("composition surface validation failed".into()),
        }}else{None};
        self.last_scene=Some((scene.clone(),scale));
        let mut encoder=self.device.create_command_encoder(&Default::default());
        if let Some(frame)=frame{
            // Render straight into the transparent swapchain. Verification allocates
            // its own target lazily instead of keeping a duplicate full-window plane.
            self.renderer.get_mut().render(&self.device,&self.queue,&mut encoder,scene,scale,self.config.width,self.config.height,&frame.texture.create_view(&Default::default()))?;
            self.queue.submit([encoder.finish()]);frame.present();unsafe{self._composition.Commit().map_err(|e|e.to_string())?;}
        }else{
            let target=self.target_texture.get_mut().get_or_insert_with(||texture(&self.device,self.config.width,self.config.height,self.config.format));
            self.renderer.get_mut().render(&self.device,&self.queue,&mut encoder,scene,scale,self.config.width,self.config.height,&target.create_view(&Default::default()))?;
            self.queue.submit([encoder.finish()]);
        }
        if self.startup_cleanup{
            self.startup_cleanup=false;
            if std::env::var_os("DOTISLE_TRACE_GPU").is_some(){eprintln!("hybrid allocations {:?}",self.device.generate_allocator_report());}
            // Release unused allocator pages from one-time compilation without trimming live memory.
            #[repr(C)]struct HeapResources{version:u32,flags:u32}
            let info=HeapResources{version:1,flags:0};
            unsafe{let _=windows::Win32::System::Memory::HeapSetInformation(None,windows::Win32::System::Memory::HeapOptimizeResources,Some((&info as *const HeapResources).cast()),std::mem::size_of_val(&info));}
        }
        Ok(())
    }
    pub fn snapshot(&self,path:&Path)->Result<(),Box<dyn std::error::Error>>{
        let mut target=self.target_texture.borrow_mut();
        let texture=target.get_or_insert_with(||texture(&self.device,self.config.width,self.config.height,self.config.format));
        if let Some((scene,scale))=&self.last_scene{
            let mut encoder=self.device.create_command_encoder(&Default::default());
            self.renderer.borrow_mut().render(&self.device,&self.queue,&mut encoder,scene,*scale,self.config.width,self.config.height,&texture.create_view(&Default::default()))?;
            self.queue.submit([encoder.finish()]);
        }
        let result=save_texture(&self.device,&self.queue,texture,path,true).map_err(Into::into);
        // Diagnostic captures must not leave a full-window readback plane resident.
        *target=None;result
    }
}

pub struct Offscreen { renderer:Renderer,device:wgpu::Device,queue:wgpu::Queue,_instance:wgpu::Instance }
impl Offscreen {
    pub fn new(width:u32,height:u32)->Result<Self,String>{
        let instance=composition_instance();let(_,device,queue)=device(&instance,None)?;
        let renderer=Renderer::new(&device,wgpu::TextureFormat::Rgba8Unorm,width,height);
        Ok(Self{renderer,device,queue,_instance:instance})
    }
    pub fn capture(&mut self,scene:&Scene,width:u32,height:u32,path:&Path)->Result<(),String>{
        let texture=texture(&self.device,width,height,wgpu::TextureFormat::Rgba8Unorm);
        let mut encoder=self.device.create_command_encoder(&Default::default());
        self.renderer.render(&self.device,&self.queue,&mut encoder,scene,1.,width,height,&texture.create_view(&Default::default()))?;
        self.queue.submit([encoder.finish()]);save_texture(&self.device,&self.queue,&texture,path,false)
    }
}
