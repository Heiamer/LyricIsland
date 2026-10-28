// Non-music components share Material colour roles and the same geometry as hit testing.
impl Renderer{
    fn device_glyph(&mut self,r:Rect,kind:&str,ink:u32){
        let name=match kind{"Mouse"=>"mouse","Keyboard"=>"keyboard","Headphones"=>"headphones","Speaker"=>"speaker","Phone"=>"smartphone","Gamepad"=>"sports_esports","Volume"=>"hard_drive","Caps"=>"keyboard_capslock","Num"=>"dialpad","Mic"=>"mic","MicOff"=>"mic_off","Touchpad"=>"touchpad_mouse",_=>"usb"};
        self.clip(r,0.,(ink>>24)as f32/255.);dotisle_render::material_symbols::paint(&mut self.canvas,name,r.x,r.y,r.w,ink);self.canvas.pop_layer();
    }
    fn m3(&mut self,s:&Scene<'_>)->dotisle_domain::monet::Scheme{let mut c=self.dynamic_resolved.map(|r|r.roles.ui()).unwrap_or_else(||dotisle_domain::dynamic_color::roles(s.dynamic_custom_seed,true,false,s.dynamic_high_contrast).ui());if !s.expanded_cover_background{c.primary=s.component_color&0xffffff;c.on_primary=if crate::cover_color::contrast(c.primary,0xffffff)>=crate::cover_color::contrast(c.primary,0){0xffffff}else{0};}c}
    fn m3_button(&mut self,id:i32,r:Rect,label:&str,s:&Scene<'_>,primary:bool){
        let c=self.m3(s);let bg=if primary{c.primary}else{c.secondary_container};let fg=if primary{c.on_primary}else{c.on_secondary_container};
        let radius=r.h/2.;self.fill(r,radius,0xff000000|bg);
        let feedback=s.feedback.get(id as usize).copied().unwrap_or(0.);if feedback>0.001{self.fill(r,radius,((feedback*255.)as u32)<<24|fg);}
        self.text_at(label,Rect::new(r.x+8.,r.y+(r.h-20.)/2.,r.w-16.,22.),13.,600,0xff000000|fg,0.5);
    }
    fn expressive_tools(&mut self,s:&Scene<'_>,p:&crate::activities::Panel){
    let w=s.layout.card.w;let h=s.layout.card.h;let c=self.m3(s);let ink=0xff000000|c.on_surface;let muted=0xff000000|c.on_surface_variant;let accent=0xff000000|c.primary;
    if p.kind==3{let style=dotisle_render::update_card::Style{primary:c.primary,on_primary:c.on_primary,on_surface:c.on_surface,muted:c.on_surface_variant,outline:c.outline};let view=dotisle_render::update_card::Presentation{title:&p.title,value:&p.value,subtitle:&p.subtitle,progress:p.progress,time_ms:s.time_ms,animate:s.animate,close:!matches!(s.update_phase,"applying"|"restarting"),action:p.tasks.first().map(|t|t.title.as_str()),feedback:s.feedback[50]};dotisle_render::update_card::paint(&mut self.canvas,&mut self.text,w,h,style,&view);return}
    let inset=20.;let header=if p.kind==2{Icon::Todo}else{Icon::Focus};
    let title_width=self.text.measure_weighted(&p.title,14.,600).min(w-120.);let group=18.+8.+title_width;let hx=(w-group)/2.;
    self.icon(header,Rect::new(hx,18.,18.,18.),accent);
    self.text_at(&p.title,Rect::new(hx+26.,17.,title_width+2.,24.),14.,600,ink,0.);
    if p.kind==2{
        let rows=crate::layout::todo_rows(w,p.tasks.len()).collect::<Vec<_>>();
        if p.tasks.is_empty(){self.text_at("现在没有待办",Rect::new(inset,80.,w-40.,24.),13.,500,muted,0.5);}
        for(t,r)in p.tasks.iter().zip(rows){self.fill(r,14.,0x12ffffff);self.ring(r.x+18.,r.y+r.h/2.,7.,accent,1.6);self.text_line(&t.title,Rect::new(r.x+36.,r.y+4.,r.w-48.,20.),12.,500,ink);self.text_line(&t.detail,Rect::new(r.x+36.,r.y+25.,r.w-48.,16.),10.,400,muted);}
        let footer=crate::layout::todo_footer(w,h);let shown=2;let label=if p.tasks.len()>shown{format!("还有 {} 项 · 打开清单",p.tasks.len()-shown)}else{"打开待办清单".into()};self.m3_button(7,footer,&label,s,false);return
    }
    let detail=p.timer.as_ref();let clock=detail.map(|d|d.expanded_value.as_str()).unwrap_or(&p.value);let primary=detail.map(|d|d.primary.as_str()).unwrap_or(if p.running{"暂停"}else{"继续"});
        self.text_at(clock,Rect::new(20.,51.,w-40.,60.),if clock.len()>8{32.}else{44.},600,ink,0.5);
        self.text_at(&p.subtitle,Rect::new(20.,112.,w-40.,20.),12.,400,muted,0.5);
        if !detail.is_some_and(|d|d.indefinite){let track=Rect::new(24.,h-80.,w-48.,6.);self.fill(track,3.,0x22ffffff);let amount=track.w*p.progress.clamp(0.,1.);if amount>0.{self.fill(Rect::new(track.x,track.y,amount,track.h),3.,accent);}}
    let buttons=crate::layout::timer_buttons(w,h);for(id,r)in buttons{self.m3_button(id,r,if id==30{primary}else if id==31{"重置"}else{"设置"},s,id==30);}
}

}
