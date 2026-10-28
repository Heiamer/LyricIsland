//! Shared interruptible Material motion.
pub use lyric_core::shared_motion::Spring;

/// Retarget the visible controls without restarting their springs.
pub fn advance_selections(values:&mut std::collections::HashMap<String,Spring>,targets:&[(String,f64)],dt:f64,reduced:bool)->bool{
 values.retain(|key,_|targets.iter().any(|(k,_)|k==key));let mut moving=false;for(key,target)in targets{let v=values.entry(key.clone()).or_insert_with(||Spring::new(*target));v.target=*target;moving|=v.tick(dt,reduced);}moving
}
