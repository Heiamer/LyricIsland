//! Deterministic background fixtures for GPU material verification.
//! Live desktops are sampled directly into GPU textures by native_glass.
#[derive(Clone,Copy,Debug,PartialEq)]pub struct Request{pub bounds:[i32;4],pub radius:f64,pub scale:f64,pub tint:u32}
pub struct Frame{pub request:Request,pub pixels:Vec<u8>}
