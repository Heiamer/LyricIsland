//! Original settings, grouped by purpose. All actions use the native validation contract.
use crate::{widgets::Ui,model::{Model,Action},draw::{self,rect,Icon}};
use serde_json::{Value,json};
fn heading(ui:&mut Ui,title:&str,x:f64,y:&mut f64,w:f64){ui.label(title,x+4.,*y+4.,21.,ui.s.on_surface,true,w-8.);*y+=46.;}
fn toggles(ui:&mut Ui,m:&Model,x:f64,y:&mut f64,w:f64,rows:&[(&str,&str,&'static str,bool)]){let s=ui.s;ui.panel(rect(x,*y,w,rows.len()as f64*76.+12.),s.container);for(i,(title,detail,key,default))in rows.iter().enumerate(){let on=m.setting(key).as_bool().unwrap_or(*default);ui.toggle(rect(x+24.,*y+6.+i as f64*76.,w-48.,70.),title,detail,on,Action::NativeEdit(key,json!(!on)));}*y+=rows.len()as f64*76.+28.;}
pub fn range(ui:&mut Ui,m:&Model,x:f64,y:&mut f64,w:f64,title:&str,key:&'static str,default:f64,min:f64,max:f64,unit:&str){let s=ui.s;let v=m.setting(key).as_f64().unwrap_or(default);ui.panel(rect(x,*y,w,96.),s.container);ui.label(title,x+24.,*y+14.,15.,s.on_surface,false,w-160.);ui.label(&format!("{v:.1} {unit}"),x+w-132.,*y+14.,13.,s.primary,true,108.);let r=rect(x+28.,*y+52.,w-56.,28.);let p=((v-min)/(max-min)).clamp(0.,1.);draw::fill(ui.scene,rect(r.x0,r.center().y-4.,r.width(),8.),4.,s.secondary_container);if p>0.{draw::fill(ui.scene,rect(r.x0,r.center().y-4.,r.width()*p,8.),4.,s.primary);}draw::fill(ui.scene,rect(r.x0+r.width()*p-2.,r.y0,4.,28.),2.,s.primary);ui.hit(r.inflate(8.,8.),title,Action::RangeHandle(key,min,max,[r.x0,r.y0,r.width(),r.height()]),true);*y+=108.;}
fn choices(ui:&mut Ui,m:&Model,x:f64,y:&mut f64,w:f64,title:&str,key:&'static str,default:&str,options:&[(&str,&str)]){let s=ui.s;ui.panel(rect(x,*y,w,108.),s.container);ui.label(title,x+24.,*y+14.,16.,s.on_surface,true,w-48.);let v=m.setting(key);let v=v.as_str().unwrap_or(default);let cw=(w-48.-(options.len()-1)as f64*8.)/options.len()as f64;for(i,(label,value))in options.iter().enumerate(){ui.chip(rect(x+24.+i as f64*(cw+8.),*y+52.,cw,38.),label,v==*value,Action::NativeEdit(key,json!(value)));}*y+=124.;}
fn action(ui:&mut Ui,x:f64,y:&mut f64,w:f64,title:&str,detail:&str,label:&str,a:Action){let s=ui.s;ui.panel(rect(x,*y,w,96.),s.container);ui.label(title,x+24.,*y+16.,16.,s.on_surface,true,w-222.);ui.label(detail,x+24.,*y+48.,12.,s.on_surface_variant,false,w-222.);ui.button(rect(x+w-176.,*y+26.,152.,44.),label,None,a,true,true);*y+=112.;}
pub fn extra(ui:&mut Ui,m:&Model,x:f64,mut y:f64,w:f64,tab:u8)->f64{
 match tab{
  0=>{action(ui,x,&mut y,w,"还原控制中心主题色","恢复原版浅色和深色主题色。","还原配色",Action::NativeCommand("resetSettingsColors",Value::Null));},
  1=>{action(ui,x,&mut y,w,"播放器与歌词来源","管理播放器排除列表、在线补全与同步偏移。","前往灵动岛",Action::IslandSettings(4));},
  2=>{
   heading(ui,"应用通知",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("接收应用通知","在灵动岛显示 Windows 通知。","NotificationsEnabled",true),("暂停通知","暂时暂停应用通知与到期提醒。","PauseNotifications",false),("点击通知打开应用","点击通知正文时打开来源。","NotificationClickOpenApp",true),("只接收允许列表","开启后仅接收列表中的应用。","NotificationWhitelistEnabled",false),("屏蔽指定应用","开启后忽略屏蔽列表中的应用。","NotificationBlacklistEnabled",false)]);
   action(ui,x,&mut y,w,"通知允许列表","应用标识用分号分隔。","编辑列表",Action::EditSetting("NotificationAllowedApps","通知允许列表 · 分号分隔"));
   action(ui,x,&mut y,w,"通知屏蔽列表","应用标识用分号分隔。","编辑列表",Action::EditSetting("NotificationBlockedApps","通知屏蔽列表 · 分号分隔"));
   action(ui,x,&mut y,w,"通知预览","用本地示例检查横幅外观。","预览通知",Action::NativeCommand("notificationPreview",Value::Null));
   heading(ui,"待办与专注",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("待办到期提醒","到期后显示提醒。","TodoRemindersEnabled",true),("待办灵动岛组件","有未完成待办时可切换到清单。","TodoIslandEnabled",true),("专注正向计时","为新建专注计时使用正向显示。","FocusCountUp",false),("锁屏暂停专注","解锁后可逐项继续计时。","FocusPauseOnLock",false)]);
   action(ui,x,&mut y,w,"新待办默认颜色","单个待办仍保留各自颜色。","选择颜色",Action::PickColor("TodoDefaultColor","新待办默认颜色"));
   heading(ui,"通知记录",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("保留通知历史","仅保留点屿实际接收的记录。","NotificationHistoryEnabled",true)]);
   range(ui,m,x,&mut y,w,"历史保留时间","NotificationHistoryDays",7.,1.,30.,"天");
   let events=m.native_state["notificationHistory"].as_array();
   if let Some(events)=events{for event in events.iter().take(50){let s=ui.s;ui.panel(rect(x,y,w,112.),s.container);let title=event["title"].as_str().unwrap_or("通知");ui.label(title,x+24.,y+14.,16.,s.on_surface,true,w-220.);ui.label(event["body"].as_str().unwrap_or(""),x+24.,y+44.,13.,s.on_surface_variant,false,w-48.);ui.label(&format!("{}  {} · {}",event["date"].as_str().unwrap_or(""),event["clock"].as_str().unwrap_or(""),event["appName"].as_str().unwrap_or("")),x+24.,y+80.,11.,s.on_surface_variant,false,w-48.);if event["kind"]=="notice"{ui.button(rect(x+w-160.,y+12.,136.,36.),"打开来源",None,Action::NativeCommand("historyOpen",event["id"].clone()),true,true);}y+=124.;}}
  },
  4=>{
   heading(ui,"启动与窗口",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("开机启动","登录 Windows 后驻留托盘。","AutoStart",true),("灵动岛保持置顶","显示在其他窗口上方，不抢占键盘焦点。","AlwaysOnTop",true)]);
   choices(ui,m,x,&mut y,w,"控制中心打开页面","ControlCenterStartPage","overview",&[("概览","overview"),("灵动岛","island"),("计时","timers"),("待办","todos"),("设置","settings")]);
   choices(ui,m,x,&mut y,w,"界面主题","SettingsTheme","System",&[("跟随系统","System"),("浅色","Light"),("深色","Dark")]);
   heading(ui,"快速提醒",x,&mut y,w);ui.button(rect(x,y,180.,44.),"一句话创建提醒",Some(Icon::Plus),Action::QuickReminder,false,true);y+=64.;
   action(ui,x,&mut y,w,"全局快捷键",m.setting("QuickReminderHotkey").as_str().filter(|v|!v.is_empty()).unwrap_or("尚未设置"),"修改快捷键",Action::EditSetting("QuickReminderHotkey","例如 Ctrl+Alt+R · 留空关闭"));
   action(ui,x,&mut y,w,"紧凑尺寸","恢复宽度、字号与固定位置。","恢复尺寸",Action::NativeCommand("resetAppearance",Value::Null));
  },
  5=>{
   heading(ui,"设备连接",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("设备连接提醒","识别设备接入和移除。","DeviceRemindersEnabled",true),("耳机连接","有线与蓝牙耳机接入时提醒。","HeadphoneConnectionRemindersEnabled",true),("音响连接","音响或扬声器接入时提醒。","SpeakerConnectionRemindersEnabled",true),("手柄连接","XInput 与 HID 手柄接入时提醒。","GamepadConnectionRemindersEnabled",true),("鼠标连接","休眠唤醒可能重复连接，原版默认关闭。","MouseConnectionRemindersEnabled",false),("键盘连接","休眠唤醒可能重复连接，原版默认关闭。","KeyboardConnectionRemindersEnabled",false),("设备断开","移除设备时也显示提醒。","DeviceDisconnectRemindersEnabled",true)]);
   action(ui,x,&mut y,w,"设备提醒预览","显示本地耳机连接示例。","预览设备",Action::NativeCommand("devicePreview",json!({"kind":"headphones","connected":true})));
   action(ui,x,&mut y,w,"系统状态提示","大小写、小键盘、麦克风和触控板状态。","前往设置",Action::IslandSettings(2));
  },
  6=>{let s=ui.s;
   ui.label("12 款原创短音 · 柔和起落、统一音量",x+4.,y+4.,13.,s.on_surface_variant,false,w-210.);
   ui.button(rect(x+w-190.,y-4.,190.,40.),"恢复推荐音色",None,Action::NativeCommand("resetBuiltinSounds",Value::Null),true,true);y+=56.;
   for(index,title,prefix,enabled,kind,volume,path,level)in [
    (0,"应用通知","Notification","NotificationSoundEnabled","NotificationSound","NotificationVolume","NotificationSoundPath",35.),
    (1,"待办与计时","Todo","TodoSoundEnabled","TodoSound","TodoVolume","TodoSoundPath",45.),
    (2,"设备连接","Device","DeviceSoundEnabled","DeviceSound","DeviceVolume","DeviceSoundPath",55.)]{
    y=sound_section(ui,m,x,y,w,index,title,prefix,enabled,kind,volume,path,level);
   }
  },
  7=>{
   heading(ui,"按应用提醒",x,&mut y,w);
   toggles(ui,m,x,&mut y,w,&[("进入应用时提醒","进入指定应用时显示预设提醒。","AppEntryRemindersEnabled",false),("前台使用时长提醒","按每天累计前台使用时间提醒。","AppUsageRemindersEnabled",false)]);
   ui.button(rect(x,y,176.,44.),"新建应用提醒",Some(Icon::Plus),Action::EditRule(None),false,true);y+=64.;
   if let Some(rules)=m.native_state["appRules"].as_array(){for r in rules{let s=ui.s;ui.panel(rect(x,y,w,164.),s.container);ui.label(r["name"].as_str().unwrap_or("应用提醒"),x+24.,y+14.,18.,s.on_surface,true,w-260.);ui.label(r["message"].as_str().unwrap_or(""),x+24.,y+46.,13.,s.on_surface_variant,false,w-48.);ui.label(r["executablePath"].as_str().unwrap_or(""),x+24.,y+74.,11.,s.on_surface_variant,false,w-48.);let id=r["id"].as_str().unwrap_or("").to_owned();ui.button(rect(x+24.,y+110.,100.,36.),"编辑",None,Action::EditRule(Some(id.clone())),true,true);ui.button(rect(x+136.,y+110.,120.,36.),if r["enabled"]==true{"停用"}else{"启用"},None,Action::NativeCommand("appRuleAction",json!({"id":id,"action":if r["enabled"]==true{"disable"}else{"enable"},"revision":r["revision"]})),true,true);y+=180.;}}
  },_=>{}
 }y
}

fn sound_section(ui:&mut Ui,m:&Model,x:f64,mut y:f64,w:f64,index:u8,title:&str,prefix:&'static str,enabled:&'static str,kind:&'static str,volume:&'static str,path:&'static str,level:f64)->f64{
 let s=ui.s;let open=m.details[index as usize];let current=m.setting(kind);let current=current.as_str().unwrap_or(dotisle_domain::sound_catalog::default_for(prefix));let custom_path=m.setting(path);let custom_path=custom_path.as_str().unwrap_or("");let custom=!custom_path.is_empty()&&m.setting(&format!("{prefix}SoundSource"))!="Builtin";
 let cue=dotisle_domain::sound_catalog::find(current).unwrap_or(&dotisle_domain::sound_catalog::CUES[0]);
 let header=rect(x,y,w,76.);ui.panel(header,if open{s.secondary_container}else{s.container});
 let hit=ui.hit(rect(x,y,w-100.,76.),title,Action::Details(index),true);ui.state(rect(x,y,w-100.,76.),24.,hit,if open{s.on_secondary_container}else{s.on_surface});
 ui.label(title,x+22.,y+11.,17.,if open{s.on_secondary_container}else{s.on_surface},true,w-250.);
 ui.label(&format!("{} · {}%",if custom{"自定义 WAV"}else{cue.name},m.setting(volume).as_f64().unwrap_or(level).round()as u32),x+22.,y+40.,12.,if open{s.on_secondary_container}else{s.on_surface_variant},false,w-250.);
 ui.label(if open{"收起 ∧"}else{"展开 ∨"},x+w-190.,y+26.,12.,if open{s.on_secondary_container}else{s.on_surface_variant},false,80.);
 let preview=if prefix=="Device"{"deviceSoundPreview"}else{"soundPreview"};
 ui.button(rect(x+w-90.,y+18.,70.,40.),"试听",None,Action::NativeCommand(preview,json!({"todo":prefix=="Todo"})),true,!m.demo);y+=88.;
 if !open{return y}
 toggles(ui,m,x,&mut y,w,&[("启用提示音","温和提醒，不改变其他应用或系统音量。",enabled,true)]);
 let h=dotisle_domain::sound_catalog::CUES.len()as f64*56.+16.;ui.panel(rect(x,y,w,h),s.container);
 for(i,cue)in dotisle_domain::sound_catalog::CUES.iter().enumerate(){let row=rect(x+8.,y+8.+i as f64*56.,w-16.,52.);let selected=!custom&&current==cue.id;if selected{draw::fill(ui.scene,row,16.,s.secondary_container);}
  let action=Action::NativeCommand("soundSelect",json!({"category":prefix,"source":"Builtin","kind":cue.id}));let hit=ui.hit(rect(row.x0,row.y0,row.width()-96.,row.height()),cue.name,action,true);ui.state(rect(row.x0,row.y0,row.width()-96.,row.height()),16.,hit,if selected{s.on_secondary_container}else{s.on_surface});
  draw::circle(ui.scene,x+32.,row.center().y,9.,if selected{s.primary}else{s.outline});draw::circle(ui.scene,x+32.,row.center().y,if selected{4.}else{7.},if selected{s.on_primary}else{s.container});
  ui.label(cue.name,x+54.,row.y0+7.,14.,if selected{s.on_secondary_container}else{s.on_surface},true,w-200.);ui.label(cue.detail,x+54.,row.y0+29.,11.,if selected{s.on_secondary_container}else{s.on_surface_variant},false,w-200.);
  ui.button(rect(x+w-92.,row.y0+7.,68.,38.),"试听",None,Action::NativeCommand(preview,json!({"todo":prefix=="Todo","kind":cue.id})),true,!m.demo);
 }y+=h+16.;
 range(ui,m,x,&mut y,w,"音量",volume,level,0.,100.,"%");
 action(ui,x,&mut y,w,"自定义 WAV",if custom_path.is_empty(){"本地 PCM WAV · 可选"}else{custom_path},"设置文件",Action::EditSetting(path,"PCM WAV 完整路径 · 留空使用内置音色"));
 if !custom_path.is_empty(){ui.button(rect(x,y,w.min(220.),40.),"使用已保存的自定义文件",None,Action::NativeCommand("soundSelect",json!({"category":prefix,"source":"Custom"})),true,true);y+=56.;}
 y+12.
}
