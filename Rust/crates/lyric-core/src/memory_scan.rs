use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn scan_rip(path: &str, offset: u64, length: u64, section_rva: i64, signature: &str, displacement: usize) -> Result<i64, String> {
    let pattern: Vec<Option<u8>> = signature.split_whitespace().map(|s| if s == "?" { Ok(None) } else { u8::from_str_radix(s,16).map(Some).map_err(|e|e.to_string()) }).collect::<Result<_,_>>()?;
    if pattern.len() < displacement + 4 || pattern.len() > 65536 || pattern[0].is_none() || length < pattern.len() as u64 { return Err("invalid signature".into()); }
    let mut file=File::open(path).map_err(|e|e.to_string())?;
    if offset.checked_add(length).ok_or("section overflow")? > file.metadata().map_err(|e|e.to_string())?.len() { return Err("truncated section".into()); }
    file.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
    let overlap=pattern.len()-1;
    let mut buffer=vec![0u8;65536+overlap];
    let mut carry=0usize; let mut consumed=0u64; let mut found=None;
    while consumed<length {
        let count=(length-consumed).min(65536) as usize;
        file.read_exact(&mut buffer[carry..carry+count]).map_err(|e|e.to_string())?;
        let total=carry+count;
        for i in 0..=total-pattern.len() {
            if buffer[i]!=pattern[0].unwrap() { continue; }
            if pattern.iter().enumerate().any(|(j,p)|p.is_some_and(|byte|buffer[i+j]!=byte)) { continue; }
            if found.is_some() { return Err("ambiguous signature".into()); }
            let relative=i32::from_le_bytes(buffer[i+displacement..i+displacement+4].try_into().unwrap()) as i64;
            found=Some(section_rva+(consumed-carry as u64) as i64+i as i64+displacement as i64+4+relative);
        }
        consumed+=count as u64; carry=overlap.min(total);
        buffer.copy_within(total-carry..total,0);
    }
    found.ok_or_else(||"signature not found".into())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn detects_cross_block_signature_and_ambiguity() {
        let path=std::env::temp_dir().join(format!("lyric-scan-{}.bin",std::process::id()));
        let mut bytes=vec![0u8;131100]; let i=65533;
        bytes[i..i+3].copy_from_slice(&[0x48,0x8d,0x0d]); bytes[i+3..i+7].copy_from_slice(&(-20i32).to_le_bytes()); bytes[i+7]=0xe8;
        std::fs::write(&path,&bytes).unwrap();
        assert_eq!(scan_rip(path.to_str().unwrap(),0,bytes.len() as u64,1000,"48 8D 0D ? ? ? ? E8",3).unwrap(),1000+i as i64+7-20);
        bytes[20..28].copy_from_slice(&[0x48,0x8d,0x0d,0,0,0,0,0xe8]); std::fs::write(&path,&bytes).unwrap();
        assert!(scan_rip(path.to_str().unwrap(),0,bytes.len() as u64,1000,"48 8D 0D ? ? ? ? E8",3).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
