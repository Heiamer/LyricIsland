//! Inspect DirectWrite's actual glyph runs, rather than reporting a requested
//! family name. Used by the portable font verification command.
use std::{ffi::c_void,sync::{Arc,Mutex}};
use serde_json::{Value,json};
use windows::{core::{implement,Interface,Result,BOOL,IUnknown,Ref},Win32::Graphics::DirectWrite::*};
#[implement(IDWriteTextRenderer)]
struct Probe {rows:Arc<Mutex<Vec<Value>>>}
impl IDWritePixelSnapping_Impl for Probe_Impl{
    fn IsPixelSnappingDisabled(&self,_:*const c_void)->Result<BOOL>{Ok(BOOL(1))}
    fn GetCurrentTransform(&self,_:*const c_void,m:*mut DWRITE_MATRIX)->Result<()>{unsafe{*m=DWRITE_MATRIX{m11:1.,m22:1.,..Default::default()};}Ok(())}
    fn GetPixelsPerDip(&self,_:*const c_void)->Result<f32>{Ok(1.)}
}
impl IDWriteTextRenderer_Impl for Probe_Impl{
    fn DrawGlyphRun(&self,_:*const c_void,_:f32,_:f32,_:DWRITE_MEASURING_MODE,run:*const DWRITE_GLYPH_RUN,_:*const DWRITE_GLYPH_RUN_DESCRIPTION,_:Ref<'_,IUnknown>)->Result<()>{unsafe{
        let run=&*run;let Some(face)=run.fontFace.as_ref()else{return Ok(())};let mut count=0;face.GetFiles(&mut count,None)?;
        let mut files=vec![None;count as usize];face.GetFiles(&mut count,Some(files.as_mut_ptr()))?;let mut paths=vec![];
        for file in files.into_iter().flatten(){let mut key=std::ptr::null_mut();let mut length=0;file.GetReferenceKey(&mut key,&mut length)?;
            if let Ok(local)=file.GetLoader()?.cast::<IDWriteLocalFontFileLoader>(){let len=local.GetFilePathLengthFromKey(key,length)?;let mut path=vec![0u16;len as usize+1];local.GetFilePathFromKey(key,length,&mut path)?;paths.push(String::from_utf16_lossy(&path[..len as usize]));}
        }
        let mut axes=vec![];if let Ok(face)=face.cast::<IDWriteFontFace5>(){let mut values=vec![DWRITE_FONT_AXIS_VALUE::default();face.GetFontAxisValueCount()as usize];face.GetFontAxisValues(&mut values)?;for v in values{axes.push(json!({"tag":String::from_utf8_lossy(&v.axisTag.0.to_le_bytes()),"value":v.value}));}}
        if let Ok(mut rows)=self.rows.lock(){rows.push(json!({"files":paths,"axes":axes,"glyphs":run.glyphCount,"emSize":run.fontEmSize}));}Ok(())
    }}
    fn DrawUnderline(&self,_:*const c_void,_:f32,_:f32,_:*const DWRITE_UNDERLINE,_:Ref<'_,IUnknown>)->Result<()>{Ok(())}
    fn DrawStrikethrough(&self,_:*const c_void,_:f32,_:f32,_:*const DWRITE_STRIKETHROUGH,_:Ref<'_,IUnknown>)->Result<()>{Ok(())}
    fn DrawInlineObject(&self,_:*const c_void,_:f32,_:f32,_:Ref<'_,IDWriteInlineObject>,_:BOOL,_:BOOL,_:Ref<'_,IUnknown>)->Result<()>{Ok(())}
}
pub unsafe fn inspect(layout:&IDWriteTextLayout)->Result<Value>{
    let rows=Arc::new(Mutex::new(vec![]));let renderer:IDWriteTextRenderer=Probe{rows:rows.clone()}.into();layout.Draw(None,&renderer,0.,0.)?;
    Ok(json!(rows.lock().map(|v|v.clone()).unwrap_or_default()))
}
