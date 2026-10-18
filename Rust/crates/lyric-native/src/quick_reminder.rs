//! A deliberately small, local grammar for one-sentence reminders.
//! Anything outside the four documented prefixes is rejected instead of guessed.
use crate::todos::{self,Now};
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Draft{pub title:String,pub date:String,pub time:String}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ParseError{pub code:&'static str,pub message:String}
impl ParseError{fn new(code:&'static str,message:&str)->Self{Self{code,message:message.into()}}}
fn fail<T>(code:&'static str,message:&str)->Result<T,ParseError>{Err(ParseError::new(code,message))}
/// Parse one `H:mm` clock value and return the number of bytes it consumed.
fn clock(rest:&str)->Option<(u32,u32,usize)>{
 let bytes=rest.as_bytes();let mut hour=0usize;let mut digits=0;
 while digits<2&&hour<bytes.len()&&bytes[hour].is_ascii_digit(){digits+=1;hour+=1}
 if digits==0||hour>=bytes.len()||bytes[hour]!=b':'{return None}
 let minute_start=hour+1;if minute_start+2>bytes.len()||!bytes[minute_start].is_ascii_digit()||!bytes[minute_start+1].is_ascii_digit(){return None}
 let h=rest[..hour].parse::<u32>().ok()?;let m=rest[minute_start..minute_start+2].parse::<u32>().ok()?;
 if h>=24||m>=60{return None}
 Some((h,m,minute_start+2))
}
fn title(rest:&str)->Result<String,ParseError>{
 let title=rest.trim();
 if title.is_empty(){return fail("empty_title","请补充提醒标题，例如：20分钟后取快递")}
 if title.chars().count()>120{return fail("too_long","标题最多 120 个字符")}
 if title.chars().any(char::is_control){return fail("invalid_characters","标题包含无效字符")}
 Ok(title.into())
}
fn future(date:&str,time:&str,now:&Now)->bool{format!("{date} {time}")>format!("{} {}",now.date,now.time)}
fn relative(number:u64,hours:bool,title_text:&str,now:&Now)->Result<Draft,ParseError>{
 let minutes=if hours{number*60}else{number};
 if minutes==0||(!hours&&number>10080)||(hours&&number>168){return fail("out_of_range",if hours{"小时后仅支持 1 到 168"}else{"分钟后仅支持 1 到 10080"})}
 let stamp=todos::add_minutes(&now.date,&now.time,minutes as u32);
 Ok(Draft{title:title(title_text)?,date:stamp[..10].into(),time:stamp[11..].into()})
}
pub fn parse(input:&str,now:&Now)->Result<Draft,ParseError>{
 let input=input.trim();
 if input.is_empty(){return fail("empty_title","请输入提醒内容")}
 if input.chars().count()>512{return fail("too_long","输入最多 512 个字符")}
 if input.chars().any(char::is_control){return fail("invalid_characters","输入包含无效字符")}
 for (word,next_day) in [("今天",false),("明天",true)]{
  if let Some(rest)=input.strip_prefix(word){
   let Some((h,m,used))=clock(rest)else{return fail("invalid_time","请填写有效的 24 小时制时刻，例如 9:00")};
   let date=if next_day{todos::add_minutes(&now.date,"00:00",1440)[..10].to_owned()}else{now.date.clone()};
   let time=format!("{h:02}:{m:02}");
   if !future(&date,&time,now){return fail("past_time","提醒时间已经过去，请重新填写")}
   return Ok(Draft{title:title(&rest[used..])?,date,time})
  }
 }
 if input.len()>=10{
  let bytes=input.as_bytes();let looks=bytes.iter().take(10).enumerate().all(|(i,b)|if i==4||i==7{*b==b'-'}else{b.is_ascii_digit()});
  if looks{
   let date=&input[..10];if !todos::valid_date(date){return fail("invalid_date","日期无效，请填写有效的 YYYY-MM-DD")}
   let rest=input[10..].trim_start();let Some((h,m,used))=clock(rest)else{return fail("invalid_time","日期后请填写有效的 24 小时制时刻")};
   let time=format!("{h:02}:{m:02}");
   if !future(date,&time,now){return fail("past_time","提醒时间已经过去，请重新填写")}
   return Ok(Draft{title:title(&rest[used..])?,date:date.into(),time})
  }
 }
 let digits=input.bytes().take_while(|b|b.is_ascii_digit()).count();
 if digits>0{
  let number=input[..digits].parse::<u64>().unwrap_or(u64::MAX);let rest=&input[digits..];
  if let Some(tail)=rest.strip_prefix("分钟后"){return relative(number,false,tail,now)}
  if let Some(tail)=rest.strip_prefix("小时后"){return relative(number,true,tail,now)}
 }
 fail("unsupported_grammar","暂不支持这种写法。可用：N分钟后 / N小时后 / 今天H:mm / 明天H:mm / YYYY-MM-DD H:mm")
}
#[cfg(test)]mod tests{use super::*;
 fn now(date:&str,time:&str)->Now{Now{date:date.into(),time:time.into()}}
 fn check(input:&str,at:&Now,date:&str,time:&str,title_text:&str){assert_eq!(parse(input,at).unwrap(),Draft{title:title_text.into(),date:date.into(),time:time.into()},"{}",input)}
 fn code(input:&str,at:&Now)->&'static str{parse(input,at).unwrap_err().code}
 #[test]fn relative_minutes_cross_month_and_year(){check("20分钟后取快递",&now("2026-10-17","09:00"),"2026-10-17","09:20","取快递");check("2小时后备份",&now("2026-12-31","23:30"),"2027-01-01","01:30","备份");check("10080分钟后最长",&now("2026-10-17","00:00"),"2026-10-24","00:00","最长");}
 #[test]fn tomorrow_and_today_use_the_local_calendar(){check("明天9:00交稿",&now("2028-02-28","22:00"),"2028-02-29","09:00","交稿");check("今天9:01开会",&now("2026-10-17","09:00"),"2026-10-17","09:01","开会");check("2026-10-18 09:00 开会",&now("2026-10-17","09:00"),"2026-10-18","09:00","开会");}
 #[test]fn designed_error_vectors(){assert_eq!(code("今天9:00开会",&now("2026-10-17","09:01")),"past_time");assert_eq!(code("2026-02-29 9:00 开会",&now("2026-02-01","09:00")),"invalid_date");assert_eq!(code("0分钟后测试",&now("2026-10-17","09:00")),"out_of_range");assert_eq!(code("20分钟后",&now("2026-10-17","09:00")),"empty_title");assert_eq!(code("一会儿喝水",&now("2026-10-17","09:00")),"unsupported_grammar");assert_eq!(code("24:00 开会",&now("2026-10-17","09:00")),"unsupported_grammar");assert_eq!(code("今天24:00开会",&now("2026-10-17","09:00")),"invalid_time");assert_eq!(code("169小时后",&now("2026-10-17","09:00")),"out_of_range");}
 #[test]fn titles_are_trimmed_and_limited(){assert_eq!(parse("  5分钟后   喝水  ",&now("2026-10-17","09:00")).unwrap().title,"喝水");let long="字".repeat(121);assert_eq!(parse(&format!("5分钟后{long}"),&now("2026-10-17","09:00")).unwrap_err().code,"too_long");let wide="字".repeat(512);assert_eq!(parse(&format!("5分钟后{wide}"),&now("2026-10-17","09:00")).unwrap_err().code,"too_long");}
}
