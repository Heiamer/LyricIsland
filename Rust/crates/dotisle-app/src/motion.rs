//! One Material motion scheme, shared with the native island.
pub use lyric_core::shared_motion::{Spring,Scheme,Kind,Speed};
pub fn advance_selections(values:&mut std::collections::HashMap<String,Spring>,targets:&[(String,f64)],dt:f64,reduced:bool)->bool{advance_selections_with(values,targets,dt,reduced,Scheme::Expressive,1.)}
pub fn advance_selections_with(values:&mut std::collections::HashMap<String,Spring>,targets:&[(String,f64)],dt:f64,reduced:bool,scheme:Scheme,pace:f64)->bool{
 values.retain(|key,_|targets.iter().any(|(k,_)|k==key));let mut moving=false;for(key,target)in targets{let v=values.entry(key.clone()).or_insert_with(||Spring::new(*target));v.configure(scheme,if key.starts_with("effects:"){Kind::Effects}else{Kind::Spatial},Speed::Fast,pace);v.target=*target;moving|=v.tick(dt,reduced);}moving
}
