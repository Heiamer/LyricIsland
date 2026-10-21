impl Renderer{
    unsafe fn tool_compact(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{
        if p.kind==3{return self.update_compact(s,p)}
        let c=s.layout.capsule;let cy=c.y+c.h/2.;let accent=s.tool_theme.fill;self.brush.SetColor(&color(accent));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(c.x+22.,cy),radiusX:10.,radiusY:10.},&self.brush,1.6,None);
        if p.kind==1{self.line((c.x+22.,cy),(c.x+22.,cy-6.),accent,1.5);self.line((c.x+22.,cy),(c.x+27.,cy+2.),accent,1.5);}else{self.line((c.x+17.,cy),(c.x+21.,cy+4.),accent,1.6);self.line((c.x+21.,cy+4.),(c.x+28.,cy-4.),accent,1.6);}
        self.text_family(&p.value,Rect::new(c.x+42.,cy-17.,c.w-72.,19.),14.,600,0xfff4f6f8,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(19.))?;
        self.text_family(&p.title,Rect::new(c.x+42.,cy+3.,c.w-72.,13.),9.,400,accent,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(13.))?;
        let glow=if p.running&&s.animate{0.7+0.3*(s.time_ms/2400.*std::f64::consts::TAU).sin()}else{0.65};self.fill(Rect::new(c.x+c.w-18.,cy-3.,6.,6.),3.,((255.*glow)as u32)<<24|(accent&0xffffff));Ok(())
    }
    unsafe fn tool_expanded(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{
        if p.kind==3{return self.update_expanded(s,p)}
        let w=s.layout.card.w;let accent=s.tool_theme.fill;
        self.text(&p.title,Rect::new(24.,17.,w-74.,22.),14.,600,0xfff4f6f8)?;
        if p.kind==2{
            for (t,r) in p.tasks.iter().zip(crate::layout::todo_rows(w,p.tasks.len())){let y=r.y;self.fill(r,9.,0xff000000|crate::cover_color::mix(0x171d24,t.color,0.10));self.fill(Rect::new(r.x+8.,y+7.,3.,18.),1.5,t.color);self.text(&t.title,Rect::new(r.x+19.,y+1.,w-100.,18.),11.5,500,0xfff4f6f8)?;self.text(&t.detail,Rect::new(r.x+19.,y+18.,w-100.,12.),8.5,400,0xff9faab6)?;self.brush.SetColor(&color(0xff79828c));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(w-38.,y+16.),radiusX:8.,radiusY:8.},&self.brush,1.3,None);}
            let footer=crate::layout::todo_footer(w);self.fill(footer,10.,0x08ffffff);self.text("打开待办清单",Rect::new(footer.x+10.,footer.y+2.,footer.w-42.,20.),10.5,500,accent)?;self.text("＋",Rect::new(footer.x+footer.w-27.,footer.y+1.,20.,20.),13.,400,accent)?;
        }else{
            let detail=p.timer.clone().unwrap_or_else(||crate::activities::TimerPresentation{label:"剩余时间".into(),elapsed:String::new(),footnote:p.subtitle.clone(),primary:if p.running{"暂停"}else{"继续"}.into(),expanded_value:p.value.clone(),indefinite:false});
            let accent=s.tool_theme.fill;self.fill(Rect::new(24.,49.,4.,4.),2.,accent);
            self.text(&detail.label,Rect::new(34.,44.,w/2.-42.,16.),10.,400,0xffa9b3bf)?;
            self.text_family(&p.subtitle,Rect::new(w/2.,44.,w/2.-24.,16.),9.,400,0xff9ca8b4,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
            // One layout paints every digit, including hundredths, on a single baseline.
            let clock=&detail.expanded_value;let size=if clock.len()>8{29.}else if clock.len()>5{34.}else{40.};
            self.text_family(clock,Rect::new(24.,65.,w-48.,48.),size,500,0xfff4f6f8,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,Some(48.))?;
            self.fill(Rect::new(24.,122.,w-48.,3.),1.5,0xff35404a);
            if !detail.indefinite{self.fill(Rect::new(24.,122.,(w-48.)*p.progress.clamp(0.,1.),3.),1.5,accent);}
            self.text(&detail.elapsed,Rect::new(24.,132.,w/2.-24.,14.),9.,400,0xffa9b3bf)?;
            self.text_family(&detail.footnote,Rect::new(w/2.,132.,w/2.-24.,14.),9.,400,0xffa9b3bf,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
            for(id,r)in crate::layout::timer_buttons(w){let hovered=s.hover==id;let pressed=s.pressed==id;
             if id==30{self.fill(r,12.,if pressed{s.tool_theme.pressed}else if hovered{s.tool_theme.hover}else{s.tool_theme.fill});}
             else{self.fill(r,10.,if pressed{0x22ffffff}else if hovered{0x14ffffff}else{0x08ffffff});}
             let label=if id==30{detail.primary.as_str()}else if id==31{"重置"}else{"设置"};
             let glass=self.reading_glass;if id==30{self.reading_glass=false;}let result=self.text_family(label,Rect::new(r.x,r.y+7.,r.w,18.),11.,if id==30{600}else{500},if id==30{s.tool_theme.ink}else{0xffbec7d1},"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(18.));self.reading_glass=glass;result?;
            }
        }Ok(())
    }
    unsafe fn compact_view(&mut self,s:&Scene<'_>)->Result<()>{if s.mini{return self.mini_compact(s)}if let Some(p)=s.panel{self.tool_compact(s,p)}else{self.compact_content(s)}}
    unsafe fn expanded_view(&mut self,s:&Scene<'_>)->Result<()>{self.reading_glass=s.material==Material::Glass;let result=if s.mini{self.mini_expanded(s)}else if let Some(p)=s.panel{self.tool_expanded(s,p)}else{self.expanded_content(s)};self.reading_glass=false;result}
    unsafe fn satellite(&mut self,s:&Scene<'_>)->Result<()>{let r=s.layout.satellite();self.fill(r,r.h/2.,0xff101419);self.brush.SetColor(&color(0x38ffffff));self.dc.DrawRoundedRectangle(&rounded(r,r.h/2.),&self.brush,1.,None);let cx=r.x+r.w/2.;let cy=r.y+r.h/2.;let c=0xffbac9d8;
        match s.auxiliary_kind{1=>{self.brush.SetColor(&color(c));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(cx,cy),radiusX:8.,radiusY:8.},&self.brush,1.4,None);self.line((cx,cy),(cx,cy-5.),c,1.4);self.line((cx,cy),(cx+4.,cy+2.),c,1.4);},2=>{self.line((cx-6.,cy),(cx-1.,cy+5.),c,1.8);self.line((cx-1.,cy+5.),(cx+7.,cy-5.),c,1.8);},_=>{for (i,h) in [8.,14.,10.].iter().enumerate(){self.fill(Rect::new(cx-6.+i as f64*5.,cy-h/2.,2.,*h),1.,c);}}}
        Ok(())
    }
}

impl Renderer{unsafe fn osd_content(&mut self,n:&crate::notifications::Notice,r:Rect)->Result<()>{
    let icon=Rect::new(r.x+12.,r.y+(r.h-30.)/2.,30.,30.);self.fill(icon,10.,0x1affffff);
    let kind=n.app_id.strip_prefix("LyricIsland.System.").unwrap_or("");
    let on=match kind{"Mic"=>n.body=="已静音","Touchpad"|"Caps"|"Num"=>n.body=="已开启",_=>false};
    let tint=if kind=="Mic"&&on{0xffff9b9b}else if on{0xffa9e6cd}else{0xffc3ccd6};
    self.system_icon(Rect::new(icon.x+5.,icon.y+5.,20.,20.),tint,kind,on)?;
    self.notification_text(&n.title,Rect::new(r.x+50.,r.y+13.,70.,18.),12.5,600,0xfff4f6f8)?;
    self.notification_text(&n.body,Rect::new(r.x+122.,r.y+14.,r.w-134.,15.),10.5,500,tint)?;Ok(())
}
unsafe fn system_icon(&mut self,r:Rect,tint:u32,kind:&str,on:bool)->Result<()>{
    self.brush.SetColor(&color(tint));
    match kind{
        "Caps"=>{
            self.line((r.x+3.,r.y+9.),(r.x+10.,r.y+2.),tint,1.6);self.line((r.x+10.,r.y+2.),(r.x+17.,r.y+9.),tint,1.6);
            self.line((r.x+13.,r.y+9.),(r.x+13.,r.y+14.),tint,1.6);self.line((r.x+13.,r.y+14.),(r.x+7.,r.y+14.),tint,1.6);self.line((r.x+7.,r.y+14.),(r.x+7.,r.y+9.),tint,1.6);
            self.line((r.x+4.,r.y+17.),(r.x+16.,r.y+17.),tint,1.6);
        }
        "Num"=>{
            self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+2.,r.y+1.5,16.,17.),3.),&self.brush,1.6,None);
            self.line((r.x+5.5,r.y+5.),(r.x+14.5,r.y+5.),tint,1.4);
            for row in 0..3{for col in 0..3{self.fill(Rect::new(r.x+6.2+col as f64*4.,r.y+8.2+row as f64*3.4,1.3,1.3),0.65,tint);}}
        }
        "Mic"=>{
            self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+7.,r.y+1.5,6.,11.),3.),&self.brush,1.5,None);
            let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;sink.BeginFigure(point(r.x+4.,r.y+9.),D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT{point:point(r.x+16.,r.y+9.),size:D2D_SIZE_F{width:6.,height:6.},rotationAngle:0.,sweepDirection:D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,arcSize:D2D1_ARC_SIZE_SMALL});
            sink.EndFigure(D2D1_FIGURE_END_OPEN);sink.Close()?;self.dc.DrawGeometry(&path,&self.brush,1.5,None);
            self.line((r.x+10.,r.y+15.),(r.x+10.,r.y+18.),tint,1.5);self.line((r.x+7.,r.y+18.),(r.x+13.,r.y+18.),tint,1.5);
            if on{self.line((r.x+3.,r.y+3.),(r.x+17.,r.y+17.),tint,1.7);}
        }
        _=>{
            self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+2.,r.y+2.5,16.,15.),3.),&self.brush,1.6,None);
            self.line((r.x+2.,r.y+12.5),(r.x+18.,r.y+12.5),tint,1.5);self.line((r.x+10.,r.y+12.5),(r.x+10.,r.y+17.5),tint,1.5);
            if !on{self.line((r.x+3.,r.y+3.),(r.x+17.,r.y+17.),tint,1.7);}
        }
    }
    Ok(())
}}
impl Renderer{unsafe fn device_icon(&mut self,r:Rect,kind:&str)->Result<()>{
    let asset=match kind{"Phone"=>"phone","Headphones"=>"headphones","Speaker"=>"speaker","Gamepad"=>"gamepad","Mouse"=>"mouse","Keyboard"=>"keyboard","Volume"=>"hard-drive","Usb"=>"usb-drive",_=>""};
    if !asset.is_empty(){
        if !self.device_bitmaps.contains_key(asset){
            let bytes:&[u8]=match asset{
                "headphones"=>include_bytes!("../../../../Assets/DeviceIcons/headphones-128.png"),
                "speaker"=>include_bytes!("../../../../Assets/DeviceIcons/speaker-128.png"),
                "gamepad"=>include_bytes!("../../../../Assets/DeviceIcons/gamepad-128.png"),
                "mouse"=>include_bytes!("../../../../Assets/DeviceIcons/mouse-128.png"),
                "keyboard"=>include_bytes!("../../../../Assets/DeviceIcons/keyboard-128.png"),
                "usb-drive"=>include_bytes!("../../../../Assets/DeviceIcons/usb-drive-128.png"),
                "hard-drive"=>include_bytes!("../../../../Assets/DeviceIcons/hard-drive-128.png"),
                "phone"=>include_bytes!("../../../../Assets/DeviceIcons/phone-128.png"),
                _=>unreachable!()};
            if let Ok(image)=image::load_from_memory(bytes){let mut pixels=image.to_rgba8();for pixel in pixels.chunks_exact_mut(4){let a=pixel[3]as u32;let red=pixel[0];pixel[0]=(pixel[2]as u32*a/255)as u8;pixel[1]=(pixel[1]as u32*a/255)as u8;pixel[2]=(red as u32*a/255)as u8;}
                let props=D2D1_BITMAP_PROPERTIES1{pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED},dpiX:96.,dpiY:96.,..Default::default()};
                let bitmap=self.dc.CreateBitmap(D2D_SIZE_U{width:pixels.width(),height:pixels.height()},Some(pixels.as_ptr()as _),pixels.width()*4,&props)?;self.device_bitmaps.insert(asset.into(),bitmap);
            }
        }
        if let Some(bitmap)=self.device_bitmaps.get(asset){self.dc.DrawBitmap(bitmap,Some(&rect(r)),1.,D2D1_INTERPOLATION_MODE_LINEAR,None,None);return Ok(())}
    }
    let ink=0xffd6e8ef;
    self.fill(r,10.,match kind{"Phone"|"Headphones"|"Speaker"=>0xff364b63,_=>0xff355a50});
    match kind{
        "Phone"=>{let body=Rect::new(r.x+11.,r.y+6.,18.,29.);self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(body,4.),&self.brush,1.8,None);self.line((r.x+17.,r.y+10.),(r.x+23.,r.y+10.),ink,1.4);self.fill(Rect::new(r.x+18.,r.y+30.,4.,2.),1.,ink);}
        "Headphones"=>{
            let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;sink.BeginFigure(point(r.x+12.,r.y+23.),D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT{point:point(r.x+28.,r.y+23.),size:D2D_SIZE_F{width:9.,height:9.},rotationAngle:0.,sweepDirection:D2D1_SWEEP_DIRECTION_CLOCKWISE,arcSize:D2D1_ARC_SIZE_SMALL});
            sink.EndFigure(D2D1_FIGURE_END_OPEN);sink.Close()?;self.brush.SetColor(&color(ink));self.dc.DrawGeometry(&path,&self.brush,1.8,None);
            self.fill(Rect::new(r.x+9.5,r.y+22.,5.,9.),2.5,ink);self.fill(Rect::new(r.x+25.5,r.y+22.,5.,9.),2.5,ink);
        }
        "Speaker"=>{self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+13.,r.y+8.,14.,24.),4.),&self.brush,1.7,None);
            self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(r.x+20.,r.y+24.),radiusX:4.2,radiusY:4.2},&self.brush,1.7,None);self.fill(Rect::new(r.x+19.,r.y+12.,2.,2.),1.,ink);}
        "Gamepad"=>{self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+8.,r.y+13.,24.,15.),7.5),&self.brush,1.7,None);
            self.line((r.x+14.,r.y+17.5),(r.x+14.,r.y+23.5),ink,1.6);self.line((r.x+11.,r.y+20.5),(r.x+17.,r.y+20.5),ink,1.6);
            self.fill(Rect::new(r.x+23.5,r.y+18.,2.6,2.6),1.3,ink);self.fill(Rect::new(r.x+27.,r.y+21.,2.6,2.6),1.3,ink);}
        "Mouse"=>{self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+13.,r.y+7.,14.,26.),7.),&self.brush,1.7,None);self.line((r.x+20.,r.y+7.5),(r.x+20.,r.y+17.),ink,1.5);}
        "Keyboard"=>{self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+8.,r.y+11.,24.,18.),3.5),&self.brush,1.7,None);
            for row in 0..2{for col in 0..5{self.fill(Rect::new(r.x+12.5+col as f64*4.,r.y+15.5+row as f64*4.2,1.7,1.7),0.8,ink);}}
            self.line((r.x+15.,r.y+24.8),(r.x+25.,r.y+24.8),ink,1.5);}
        _=>{self.fill(Rect::new(r.x+11.,r.y+15.,18.,20.),5.,0xffc9e7d9);self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+14.,r.y+5.,12.,14.),2.),&self.brush,1.6,None);self.fill(Rect::new(r.x+17.,r.y+8.,2.,4.),0.5,ink);self.fill(Rect::new(r.x+22.,r.y+8.,2.,4.),0.5,ink);}
    }
    Ok(())
}}

impl Renderer{
 unsafe fn update_ring(&mut self,x:f64,y:f64,radius:f64,progress:f64,time:f64,animate:bool,accent:u32)->Result<()>{
  self.brush.SetColor(&color(0xff2a3851));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(x,y),radiusX:radius as f32,radiusY:radius as f32},&self.brush,3.,None);
  let fraction=if progress<0.{0.22}else{progress.clamp(0.,1.)};let start=if progress<0.&&animate{time/1200.*std::f64::consts::TAU}else{-std::f64::consts::FRAC_PI_2};
  self.brush.SetColor(&color(accent));
  if fraction>=0.9999{self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(x,y),radiusX:radius as f32,radiusY:radius as f32},&self.brush,3.,None);}else if fraction>0.{let end=start+fraction*std::f64::consts::TAU;let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;sink.BeginFigure(point(x+radius*start.cos(),y+radius*start.sin()),D2D1_FIGURE_BEGIN_HOLLOW);sink.AddArc(&D2D1_ARC_SEGMENT{point:point(x+radius*end.cos(),y+radius*end.sin()),size:D2D_SIZE_F{width:radius as f32,height:radius as f32},rotationAngle:0.,sweepDirection:D2D1_SWEEP_DIRECTION_CLOCKWISE,arcSize:if fraction>0.5{D2D1_ARC_SIZE_LARGE}else{D2D1_ARC_SIZE_SMALL}});sink.EndFigure(D2D1_FIGURE_END_OPEN);sink.Close()?;self.dc.DrawGeometry(&path,&self.brush,3.,None);}Ok(())
 }
 unsafe fn update_compact(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{let c=s.layout.capsule;self.update_ring(c.x+23.,c.y+c.h/2.,11.,p.progress,s.time_ms,s.animate,s.tool_theme.fill)?;self.text(&p.title,Rect::new(c.x+46.,c.y+6.,c.w-58.,18.),11.,500,0xfff4f6f8)?;self.text(&p.value,Rect::new(c.x+46.,c.y+23.,c.w-58.,14.),9.,400,s.tool_theme.fill)?;Ok(())}
 unsafe fn update_expanded(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{let w=s.layout.card.w;self.text("软件更新",Rect::new(24.,20.,w-80.,24.),14.,600,0xfff4f6f8)?;self.text("×",Rect::new(w-43.,18.,24.,24.),18.,400,0xffa9b3bf)?;self.update_ring(65.,98.,27.,p.progress,s.time_ms,s.animate,s.tool_theme.fill)?;
  if p.progress>=0.{self.text_family(&p.value,Rect::new(36.,88.,58.,20.),12.,500,s.tool_theme.fill,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}
  self.text(&p.title,Rect::new(112.,71.,w-136.,30.),16.,500,0xfff4f6f8)?;self.text(&p.subtitle,Rect::new(112.,111.,w-136.,45.),10.5,400,0xffa9b3bf)?;
  if let Some(action)=p.tasks.first().filter(|t|!t.title.is_empty()){self.fill(Rect::new(24.,190.,w-48.,42.),16.,s.tool_theme.fill);self.text_family(&action.title,Rect::new(24.,201.,w-48.,22.),12.,600,s.tool_theme.ink,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;}Ok(())
 }
}
