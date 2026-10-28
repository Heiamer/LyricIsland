// Shared geometry owns every music control target; the buttons paint the same classic glyphs.
impl Renderer{
    unsafe fn transport_triangle(&mut self,r:Rect,back:bool,c:u32)->Result<()>{
        let key=format!("transport-{back}|{}|{}",r.w,r.h);
        if !self.icons.contains_key(&key){
            let path=self.factory.CreatePathGeometry()?;let sink=path.Open()?;
            let p=|x:f64,y:f64|point(if back{x*r.w}else{(1.-x)*r.w},y*r.h);
            sink.BeginFigure(p(0.92,0.04),D2D1_FIGURE_BEGIN_FILLED);
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT{point1:p(1.,0.),point2:p(1.,0.12)});sink.AddLine(p(1.,0.88));
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT{point1:p(1.,1.),point2:p(0.92,0.96)});sink.AddLine(p(0.08,0.56));
            sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT{point1:p(-0.02,0.5),point2:p(0.08,0.44)});sink.AddLine(p(0.92,0.04));sink.EndFigure(D2D1_FIGURE_END_CLOSED);sink.Close()?;
            let shape=self.factory.CreateTransformedGeometry(&path,&identity())?;
            self.icons.insert(key.clone(),shape);self.icon_order.push_back(key.clone());self.trim_icons();
        }
        self.brush.SetColor(&color(c));let mut transform=identity();self.dc.GetTransform(&mut transform);let mut shifted=transform;shifted.M31+=(r.x*transform.M11 as f64+r.y*transform.M21 as f64)as f32;shifted.M32+=(r.x*transform.M12 as f64+r.y*transform.M22 as f64)as f32;self.dc.SetTransform(&shifted);self.dc.FillGeometry(self.icons.get(&key).unwrap(),&self.brush,None);self.dc.SetTransform(&transform);Ok(())
    }
    unsafe fn music_button(&mut self,id:i32,x:f64,y:f64,size:f64,s:&Scene<'_>)->Result<()>{
        // Secondary actions keep their existing glyphs, colour and disabled semantics.
        if ![3,4,5].contains(&id){self.classic_button(id,x,y,size,s);return Ok(())}
        let disabled=!s.has_track;let alpha=if disabled{0x4d000000}else{0xff000000};let accent=alpha|(s.component_color&0xffffff);
        let press=if s.pressed==id&&!disabled{0.94}else{1.};let cx=x+size/2.;let cy=y+size/2.;
        if s.hover==id&&!disabled{self.fill(Rect::new(x+2.,y+2.,size-4.,size-4.),(size-4.)/2.,0x14ffffff);}
        if id==4{
            if s.playing{for dx in [-9.,3.]{self.fill(Rect::new(cx+dx*press,cy-12.*press,6.*press,24.*press),1.8*press,accent);}}
            else{self.transport_triangle(Rect::new(cx-7.*press,cy-12.*press,20.*press,24.*press),false,accent)?;}
        }else{
            let back=id==3;let w=14.*press;let h=20.*press;
            for dx in [-14.,0.]{self.transport_triangle(Rect::new(cx+dx*press,cy-h/2.,w,h),back,accent)?;}
        }Ok(())
    }
    unsafe fn button(&mut self,id:i32,x:f64,y:f64,size:f64,s:&Scene<'_>){
        if s.panel.is_some(){self.classic_button(id,x,y,size,s);return}
        // Music glyphs retain their own alpha and colour on glass; an opaque contrast
        // outline made disabled icons look like doubled, black wireframes.
        let glass=self.reading_glass;self.reading_glass=false;
        let _=self.music_button(id,x,y,size,s);self.reading_glass=glass;
    }
}
