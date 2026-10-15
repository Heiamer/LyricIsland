//! Use the Windows Shell registration for desktop applications whose WinRT
//! logo stream is unavailable. Work stays off the rendering thread.
use windows::{core::{PCWSTR,Result},Win32::{Foundation::SIZE,Graphics::Gdi::*,UI::Shell::*}};

pub fn shell_icon(id:&str)->Option<Vec<u8>>{unsafe{
    let path=format!("shell:AppsFolder\\{id}").encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let factory:IShellItemImageFactory=SHCreateItemFromParsingName(PCWSTR(path.as_ptr()),None).ok()?;
    let bitmap=factory.GetImage(SIZE{cx:64,cy:64},SIIGBF_ICONONLY|SIIGBF_BIGGERSIZEOK).ok()?;
    let result=bitmap_png(bitmap);let _=DeleteObject(bitmap.into());result.ok()
}}

unsafe fn bitmap_png(bitmap:HBITMAP)->Result<Vec<u8>>{
    let bad=||windows::core::Error::from_hresult(windows::core::HRESULT(0x80070057u32 as i32));
    let mut info=BITMAP::default();
    if GetObjectW(bitmap.into(),std::mem::size_of::<BITMAP>()as i32,Some((&mut info as *mut BITMAP).cast()))==0{return Err(bad())}
    let width=info.bmWidth;let height=info.bmHeight.abs();
    if width<=0||height<=0||width>1024||height>1024{return Err(bad())}
    let mut header=BITMAPINFO::default();header.bmiHeader=BITMAPINFOHEADER{biSize:std::mem::size_of::<BITMAPINFOHEADER>()as u32,biWidth:width,biHeight:-height,biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()};
    let mut pixels=vec![0u8;(width*height*4)as usize];let dc=GetDC(None);
    let rows=GetDIBits(dc,bitmap,0,height as u32,Some(pixels.as_mut_ptr().cast()),&mut header,DIB_RGB_COLORS);let _=ReleaseDC(None,dc);
    if rows!=height{return Err(bad())}
    let alpha=pixels.chunks_exact(4).any(|p|p[3]!=0);
    for p in pixels.chunks_exact_mut(4){p.swap(0,2);if !alpha{p[3]=255}else if p[3]>0{let a=p[3]as u32;for c in &mut p[..3]{*c=((*c as u32*255+a/2)/a).min(255)as u8;}}}
    let image=image::RgbaImage::from_raw(width as u32,height as u32,pixels).ok_or_else(bad)?;
    let mut output=std::io::Cursor::new(Vec::new());image::DynamicImage::ImageRgba8(image).write_to(&mut output,image::ImageFormat::Png).map_err(|_|bad())?;Ok(output.into_inner())
}
