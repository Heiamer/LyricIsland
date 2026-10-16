impl Renderer{
    unsafe fn tool_compact(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{
        let c=s.layout.capsule;let cy=c.y+c.h/2.;let accent=p.accent;self.brush.SetColor(&color(accent));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(c.x+22.,cy),radiusX:10.,radiusY:10.},&self.brush,1.6,None);
        if p.kind==1{self.line((c.x+22.,cy),(c.x+22.,cy-6.),accent,1.5);self.line((c.x+22.,cy),(c.x+27.,cy+2.),accent,1.5);}else{self.line((c.x+17.,cy),(c.x+21.,cy+4.),accent,1.6);self.line((c.x+21.,cy+4.),(c.x+28.,cy-4.),accent,1.6);}
        self.text_family(&p.value,Rect::new(c.x+42.,cy-17.,c.w-72.,19.),14.,600,0xfff4f6f8,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(19.))?;
        self.text_family(&p.title,Rect::new(c.x+42.,cy+3.,c.w-72.,13.),9.,400,accent,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(13.))?;
        let glow=if p.running&&s.animate{0.7+0.3*(s.time_ms/2400.*std::f64::consts::TAU).sin()}else{0.65};self.fill(Rect::new(c.x+c.w-18.,cy-3.,6.,6.),3.,((255.*glow)as u32)<<24|(accent&0xffffff));Ok(())
    }
    unsafe fn tool_expanded(&mut self,s:&Scene<'_>,p:&crate::activities::Panel)->Result<()>{
        let w=s.layout.card.w;let accent=p.accent;
        self.text(&p.title,Rect::new(24.,17.,w-74.,22.),14.,600,0xfff4f6f8)?;self.text("⌄",Rect::new(w-37.,15.,22.,22.),18.,400,0xffc1c9d1)?;
        if p.kind==2{
            for (t,r) in p.tasks.iter().zip(crate::layout::todo_rows(w,p.tasks.len())){let y=r.y;self.fill(r,9.,0xff000000|crate::cover_color::mix(0x171d24,t.color,0.10));self.fill(Rect::new(r.x+8.,y+7.,3.,18.),1.5,t.color);self.text(&t.title,Rect::new(r.x+19.,y+1.,w-100.,18.),11.5,500,0xfff4f6f8)?;self.text(&t.detail,Rect::new(r.x+19.,y+18.,w-100.,12.),8.5,400,0xff9faab6)?;self.brush.SetColor(&color(0xff79828c));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(w-38.,y+16.),radiusX:8.,radiusY:8.},&self.brush,1.3,None);}
            let footer=crate::layout::todo_footer(w);self.fill(footer,10.,0x08ffffff);self.text("打开待办清单",Rect::new(footer.x+10.,footer.y+2.,footer.w-42.,20.),10.5,500,accent)?;self.text("＋",Rect::new(footer.x+footer.w-27.,footer.y+1.,20.,20.),13.,400,accent)?;
        }else{
            let cx=69.;let cy=91.;let radius=38.;
            self.fill(Rect::new(cx-44.,cy-44.,88.,88.),44.,0x08ffffff);
            self.brush.SetColor(&color(0xff343e49));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(cx,cy),radiusX:radius as f32,radiusY:radius as f32},&self.brush,3.,None);
            let progress=p.progress.clamp(0.,1.);let end=-std::f64::consts::FRAC_PI_2+progress*std::f64::consts::TAU;
            if progress>0.{let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;sink.BeginFigure(point(cx,cy-radius),D2D1_FIGURE_BEGIN_HOLLOW);sink.AddArc(&D2D1_ARC_SEGMENT{point:point(cx+radius*end.cos(),cy+radius*end.sin()),size:D2D_SIZE_F{width:radius as f32,height:radius as f32},rotationAngle:0.,sweepDirection:D2D1_SWEEP_DIRECTION_CLOCKWISE,arcSize:if progress>0.5{D2D1_ARC_SIZE_LARGE}else{D2D1_ARC_SIZE_SMALL}});sink.EndFigure(D2D1_FIGURE_END_OPEN);sink.Close()?;self.brush.SetColor(&color(accent));if progress>=0.9999{self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(cx,cy),radiusX:radius as f32,radiusY:radius as f32},&self.brush,3.,None);}else{self.dc.DrawGeometry(&path,&self.brush,3.,None);}}
            self.fill(Rect::new(cx+radius*end.cos()-3.,cy+radius*end.sin()-3.,6.,6.),3.,accent);
            for i in 0..60{let a=i as f64/60.*std::f64::consts::TAU;let inner=if i%5==0{26.}else{29.};self.line((cx+inner*a.sin(),cy-inner*a.cos()),(cx+32.*a.sin(),cy-32.*a.cos()),if i%5==0{0xff9aa5b0}else{0xff53606c},if i%5==0{1.2}else{0.7});}
            let a=progress*std::f64::consts::TAU;self.line((cx,cy),(cx+21.*a.sin(),cy-21.*a.cos()),accent,1.8);self.line((cx,cy),(cx-11.,cy-7.),0xffd5dce2,2.);self.fill(Rect::new(cx-2.5,cy-2.5,5.,5.),2.5,accent);
            self.text_family(&p.value,Rect::new(128.,62.,w-148.,44.),if p.value.len()>5{28.}else{36.},400,0xfff4f6f8,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(44.))?;self.text_family(&p.subtitle,Rect::new(125.,109.,w-145.,20.),10.,400,0xffa9b3bf,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;
            self.fill(Rect::new(w/2.-50.,139.,100.,38.),19.,accent);self.text_family(if p.running{"暂停"}else{"开始"},Rect::new(w/2.-50.,148.,100.,20.),12.,600,0xff162128,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None)?;
            self.text("重置",Rect::new(20.,151.,52.,22.),11.,500,0xffbec7d1)?;self.text("设置",Rect::new(w-66.,151.,52.,22.),11.,500,0xffbec7d1)?;
        }Ok(())
    }
    unsafe fn compact_view(&mut self,s:&Scene<'_>)->Result<()>{if let Some(p)=s.panel{self.tool_compact(s,p)}else{self.compact_content(s)}}
    unsafe fn expanded_view(&mut self,s:&Scene<'_>)->Result<()>{if let Some(p)=s.panel{self.tool_expanded(s,p)}else{self.expanded_content(s)}}
    unsafe fn satellite(&mut self,s:&Scene<'_>)->Result<()>{let r=s.layout.satellite();self.fill(r,r.h/2.,0xff101419);self.brush.SetColor(&color(0x38ffffff));self.dc.DrawRoundedRectangle(&rounded(r,r.h/2.),&self.brush,1.,None);let cx=r.x+r.w/2.;let cy=r.y+r.h/2.;let c=0xffbac9d8;
        match s.auxiliary_kind{1=>{self.brush.SetColor(&color(c));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(cx,cy),radiusX:8.,radiusY:8.},&self.brush,1.4,None);self.line((cx,cy),(cx,cy-5.),c,1.4);self.line((cx,cy),(cx+4.,cy+2.),c,1.4);},2=>{self.line((cx-6.,cy),(cx-1.,cy+5.),c,1.8);self.line((cx-1.,cy+5.),(cx+7.,cy-5.),c,1.8);},_=>{for (i,h) in [8.,14.,10.].iter().enumerate(){self.fill(Rect::new(cx-6.+i as f64*5.,cy-h/2.,2.,*h),1.,c);}}}
        Ok(())
    }
}

impl Renderer{unsafe fn device_icon(&mut self,r:Rect,phone:bool)->Result<()>{self.fill(r,10.,if phone{0xff364b63}else{0xff355a50});let ink=0xffd6e8ef;if phone{let body=Rect::new(r.x+11.,r.y+6.,18.,29.);self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(body,4.),&self.brush,1.8,None);self.line((r.x+17.,r.y+10.),(r.x+23.,r.y+10.),ink,1.4);self.fill(Rect::new(r.x+18.,r.y+30.,4.,2.),1.,ink);}else{self.fill(Rect::new(r.x+11.,r.y+15.,18.,20.),5.,0xffc9e7d9);self.brush.SetColor(&color(ink));self.dc.DrawRoundedRectangle(&rounded(Rect::new(r.x+14.,r.y+5.,12.,14.),2.),&self.brush,1.6,None);self.fill(Rect::new(r.x+17.,r.y+8.,2.,4.),0.5,ink);self.fill(Rect::new(r.x+22.,r.y+8.,2.,4.),0.5,ink);}Ok(())}}
