//! Original, mutually tuned UI cues. IDs are shared by selection, validation and playback.
#[derive(Clone,Copy,Debug)]pub struct Cue{pub id:&'static str,pub name:&'static str,pub detail:&'static str,pub file:&'static str,pub millis:u32}
pub const CUES:&[Cue]=&[
 Cue{id:"Soft",name:"柔点",detail:"轻柔圆润 · 推荐通知",file:"notification-soft.wav",millis:180},
 Cue{id:"Glass",name:"澄光",detail:"通透轻亮 · 不尖锐",file:"notification-glass.wav",millis:220},
 Cue{id:"Bell",name:"轻铃",detail:"温暖和声 · 推荐待办",file:"todo-bell.wav",millis:300},
 Cue{id:"Connect",name:"相遇",detail:"轻快上行 · 推荐设备",file:"device-connect.wav",millis:240},
 Cue{id:"Bubble",name:"气泡",detail:"圆润弹起 · 灵巧反馈",file:"bubble.wav",millis:160},
 Cue{id:"Bloom",name:"花开",detail:"三音绽放 · 轻盈舒展",file:"bloom.wav",millis:340},
 Cue{id:"Ripple",name:"涟漪",detail:"细小回响 · 安静提醒",file:"ripple.wav",millis:320},
 Cue{id:"Breeze",name:"微风",detail:"缓入轻落 · 柔和低频",file:"breeze.wav",millis:280},
 Cue{id:"Wood",name:"木语",detail:"短促木质 · 克制清晰",file:"wood.wav",millis:180},
 Cue{id:"Rise",name:"上扬",detail:"三阶轻跃 · 积极反馈",file:"rise.wav",millis:320},
 Cue{id:"Drop",name:"落点",detail:"两音收束 · 自然落下",file:"drop.wav",millis:240},
 Cue{id:"Finish",name:"和鸣",detail:"完整和弦 · 完成提醒",file:"finish.wav",millis:400},
];
pub fn find(id:&str)->Option<&'static Cue>{CUES.iter().find(|c|c.id==id)}
pub fn default_for(prefix:&str)->&'static str{match prefix{"Todo"=>"Bell","Device"=>"Connect",_=>"Soft"}}
pub fn valid_prefix(prefix:&str)->bool{matches!(prefix,"Notification"|"Todo"|"Device")}
