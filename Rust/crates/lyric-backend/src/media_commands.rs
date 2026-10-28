//! Bounded command ordering. Only consecutive seeks for the same track coalesce.
use std::collections::VecDeque;
#[derive(Clone,Debug)]pub struct Request{pub id:u64,pub action:String,pub seconds:f64,pub track:String}
#[derive(Default)]pub struct Queue{pending:VecDeque<Request>}
impl Queue{
 pub fn push(&mut self,request:Request)->Result<(),String>{
    if request.action=="seek"{if let Some(last)=self.pending.back_mut().filter(|last|last.action=="seek"&&last.track==request.track){*last=request;return Ok(())}}
    if self.pending.len()>=16{return Err("播放操作较多，请稍后重试".into())}self.pending.push_back(request);Ok(())
 }
 pub fn pop(&mut self)->Option<Request>{self.pending.pop_front()}
}
#[cfg(test)]mod tests{use super::*;
 fn request(id:u64,action:&str,track:&str)->Request{Request{id,action:action.into(),seconds:id as f64,track:track.into()}}
 #[test]fn rapid_drag_keeps_latest_target_without_losing_pause(){let mut q=Queue::default();for id in 1..100{q.push(request(id,"seek","a")).unwrap();}q.push(request(100,"pause","a")).unwrap();q.push(request(101,"seek","a")).unwrap();assert_eq!(q.pop().unwrap().id,99);assert_eq!(q.pop().unwrap().id,100);assert_eq!(q.pop().unwrap().id,101);assert!(q.pop().is_none());}
 #[test]fn different_tracks_do_not_coalesce_and_queue_is_bounded(){let mut q=Queue::default();for id in 0..16{q.push(request(id,"seek",&id.to_string())).unwrap();}assert!(q.push(request(17,"next","a")).is_err());for id in 0..16{assert_eq!(q.pop().unwrap().id,id);}}
}
