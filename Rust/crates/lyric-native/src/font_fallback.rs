//! Compatibility with WPF's Global User Interface composite font. The source
//! IslandRoot overrides the bundled URI with a family name, so if that family is
//! not installed, WPF uses this mapping and baseline=.9 / line spacing=1.2.
//! Read Windows' own mapping in place; do not redistribute system fonts or XML.
use windows::{core::{Interface,Result,PCWSTR,w},Win32::Graphics::DirectWrite::*};
pub unsafe fn create(factory:&IDWriteFactory,fonts:&IDWriteFontCollection)->Result<IDWriteFontFallback>{
    let f:IDWriteFactory2=factory.cast()?;let builder=f.CreateFontFallbackBuilder()?;
    let path=std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(||"C:\\Windows".into())).join("Microsoft.NET/Framework64/v4.0.30319/WPF/Fonts/GlobalUserInterface.CompositeFont");
    let contents=std::fs::read(path).ok().and_then(|bytes|if bytes.starts_with(&[255,254]){Some(String::from_utf16_lossy(&bytes[2..].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).collect::<Vec<_>>()))}else{String::from_utf8(bytes).ok()});
    let mut added=false;
    if let Some(text)=contents{if let Ok(doc)=roxmltree::Document::parse(&text){
        if let Some(family)=doc.descendants().find(|n|n.has_tag_name("FontFamily")){
            for map in family.descendants().filter(|n|n.has_tag_name("FontFamilyMap")){
                // The original TextBlocks have the WPF default Language=en-US.
                if map.attribute("Language").is_some_and(|s|!"en-US".to_ascii_lowercase().starts_with(&s.to_ascii_lowercase())){continue}
                let Some(target)=map.attribute("Target")else{continue};let mut ranges=vec![];
                for range in map.attribute("Unicode").unwrap_or("0-10FFFF").split(','){
                    let mut pair=range.trim().split('-');let first=u32::from_str_radix(pair.next().unwrap_or(""),16);let last=pair.next().map(|s|u32::from_str_radix(s,16));
                    if let Ok(first)=first{if let Ok(last)=last.unwrap_or(Ok(first)){ranges.push(DWRITE_UNICODE_RANGE{first,last});}}
                }
                if ranges.is_empty(){continue}
                let names=target.split(',').map(|s|s.trim().encode_utf16().chain(Some(0)).collect::<Vec<_>>()).collect::<Vec<_>>();let pointers=names.iter().map(|s|s.as_ptr()).collect::<Vec<_>>();
                builder.AddMapping(&ranges,&pointers,fonts,w!("en-US"),PCWSTR::null(),map.attribute("Scale").and_then(|s|s.parse().ok()).unwrap_or(1.))?;added=true;
            }
        }
    }}
    if !added{let names=[w!("Segoe UI").as_ptr(),w!("Yu Gothic UI").as_ptr(),w!("Microsoft JhengHei UI").as_ptr(),w!("Microsoft YaHei UI").as_ptr(),w!("Segoe UI Symbol").as_ptr()];builder.AddMapping(&[DWRITE_UNICODE_RANGE{first:0,last:0x10ffff}],&names,fonts,w!("en-US"),PCWSTR::null(),1.)?;}
    builder.AddMappings(&f.GetSystemFontFallback()?)?;builder.CreateFontFallback()
}
