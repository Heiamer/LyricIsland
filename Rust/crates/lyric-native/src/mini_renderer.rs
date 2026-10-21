// Mini keeps artwork and two centred text rows above compact playback controls.
impl Renderer{
    unsafe fn mini_compact(&mut self,s:&Scene<'_>)->Result<()>{
        let c=s.layout.capsule;let playing=s.panel.map(|p|p.running).unwrap_or(s.playing);
        let breath=if playing&&s.animate{0.68+0.32*(s.time_ms/2800.*std::f64::consts::TAU).sin()}else{0.8};
        let dot=3.2+breath*1.2;let accent=if s.panel.is_some(){s.tool_theme.fill}else{s.progress_color};
        self.fill(Rect::new(c.x+11.-dot/2.,c.y+(c.h-dot)/2.,dot,dot),dot/2.,((255.*breath)as u32)<<24|(accent&0xffffff));
        let label=s.panel.map(|p|format!("{}  {}",p.title,p.value)).unwrap_or_else(||s.lyric.to_owned());
        let font=crate::compact::mini_font_size(s.font_size);let area=Rect::new(c.x+22.,c.y,c.w-44.,c.h);
        // Symmetric gutters keep the words on the capsule centre, independently of the dot.
        if s.mini_glass{for(dx,dy)in[(-0.65,0.),(0.65,0.),(0.,-0.65),(0.,0.65)]{self.text_family(&label,Rect::new(area.x+dx,area.y+dy,area.w,area.h),font,500,s.mini_outline,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(-1.))?;}}
        self.text_family(&label,area,font,500,s.mini_ink,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,Some(-1.))?;Ok(())
    }
    unsafe fn mini_expanded(&mut self,s:&Scene<'_>)->Result<()>{
        let w=s.layout.card.w;let h=s.layout.card.h;let accent=s.tool_theme.fill;
        if let Some(p)=s.panel{self.text(&p.title,Rect::new(20.,9.,w-108.,20.),13.,600,0xfff4f6f8)?;}
        if let Some(p)=s.panel{
            if p.kind==2{
                for(t,r)in p.tasks.iter().zip(crate::layout::mini_todos(w)){
                    self.fill(r,6.,0x0cffffff);self.fill(Rect::new(r.x+7.,r.y+6.,3.,8.),1.5,t.color);
                    self.text(&t.title,Rect::new(r.x+17.,r.y+1.,r.w-44.,18.),10.5,400,0xffe9eef6)?;
                    self.brush.SetColor(&color(accent));self.dc.DrawEllipse(&D2D1_ELLIPSE{point:point(r.x+r.w-13.,r.y+10.),radiusX:5.,radiusY:5.},&self.brush,1.2,None);
                }
                self.text("打开待办清单",Rect::new(20.,h-23.,w-40.,18.),10.,500,accent)?;
            }else if p.kind==3{
                self.text(&p.value,Rect::new(20.,33.,w-40.,22.),16.,500,accent)?;
                self.fill(Rect::new(20.,60.,w-40.,3.),1.5,0xff354052);
                self.fill(Rect::new(20.,60.,(w-40.)*p.progress.clamp(0.,1.),3.),1.5,accent);
                self.text(&p.subtitle,Rect::new(20.,69.,w-40.,18.),9.,400,0xffbac5d4)?;
                if let Some(action)=p.tasks.first(){let r=crate::layout::mini_primary(w,h,3);self.mini_text_button(50,r,&action.title,true,s)?;}
            }else{
                let detail=p.timer.as_ref();let clock=detail.map(|d|d.expanded_value.as_str()).unwrap_or(&p.value);
                self.text(clock,Rect::new(20.,30.,w*0.56,32.),25.,500,0xfff4f6f8)?;
                self.text_family(&p.subtitle,Rect::new(w*0.58,36.,w*0.42-20.,18.),9.5,400,0xffbac5d4,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
                for(id,r)in crate::layout::mini_timer_buttons(w,h){let label=if id==30{detail.map(|d|d.primary.as_str()).unwrap_or(if p.running{"暂停"}else{"开始"})}else if id==31{"重置"}else{"设置"};self.mini_text_button(id,r,label,id==30,s)?;}
            }
        }else{
            let cover=Rect::new(20.,8.,40.,40.);self.flipping_cover(cover,s)?;
            let text_x=cover.x+cover.w+12.;let text_w=(w-text_x-108.).max(32.);
            let lyric=if s.lyric.is_empty(){s.title}else{s.lyric};
            let line=Rect::new(text_x,9.,text_w,22.);
            let boundary=line.x+self.measure_line(lyric,13.)?.min(line.w)*s.highlight.clamp(0.,1.);
            for(clip,ink)in[(Rect::new(line.x,line.y,(boundary-line.x).max(0.),line.h),s.progress_color),(Rect::new(boundary,line.y,(line.x+line.w-boundary).max(0.),line.h),s.text_color)]{if clip.w>0.{self.dc.PushAxisAlignedClip(&rect(clip),D2D1_ANTIALIAS_MODE_ALIASED);self.text_family(lyric,line,13.,500,ink,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_LEADING,Some(line.h))?;self.dc.PopAxisAlignedClip();}}
            self.text(&format!("{}-{}",s.title,s.artist),Rect::new(text_x,31.,text_w,15.),9.,400,0xb8000000|(s.text_color&0xffffff))?;
            let track=crate::layout::mini_track(w,h);self.fill(track,1.5,0xff354052);self.fill(Rect::new(track.x,track.y,track.w*s.progress.clamp(0.,1.),track.h),1.5,s.component_color);
            self.text_family(&format!("{} / {}",time(s.elapsed),time(s.duration)),Rect::new(w-104.,33.,84.,14.),8.,400,0xffb8c5d3,"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_TRAILING,None)?;
            for(id,r)in crate::layout::mini_music_buttons(w,h){self.button(id,r.x,r.y,r.w,s);}
        }Ok(())
    }
    unsafe fn mini_text_button(&mut self,id:i32,r:Rect,label:&str,primary:bool,s:&Scene<'_>)->Result<()>{
        let fill=if primary{if s.pressed==id{s.tool_theme.pressed}else if s.hover==id{s.tool_theme.hover}else{s.tool_theme.fill}}else if s.hover==id{0x20ffffff}else{0x0dffffff};self.fill(r,8.,fill);
        let glass=self.reading_glass;if primary{self.reading_glass=false;}let result=self.text_family(label,Rect::new(r.x,r.y+3.,r.w,r.h-4.),10.,500,if primary{s.tool_theme.ink}else{0xffd3ddeb},"Noto Sans SC",false,DWRITE_TEXT_ALIGNMENT_CENTER,None);self.reading_glass=glass;result
    }
}
