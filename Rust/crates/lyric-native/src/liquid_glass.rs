//! Cover-space optical lens. Text is composited afterwards and never refracted.
pub struct Cover {pub pixels:Vec<u8>,pub width:u32,pub height:u32}
fn bevel(x:f64,y:f64,w:f64,h:f64,r:f64)->(f64,f64,f64){
    let r=r.min(w/2.).min(h/2.).max(1.);let qx=(x-w/2.).abs()-(w/2.-r);let qy=(y-h/2.).abs()-(h/2.-r);
    let ax=qx.max(0.);let ay=qy.max(0.);let length=ax.hypot(ay);let distance=length+qx.max(qy).min(0.)-r;
    let(nx,ny)=if length>0.0001{(ax/length,ay/length)}else if qx>qy{(1.,0.)}else{(0.,1.)};(distance,nx*(x-w/2.).signum(),ny*(y-h/2.).signum())
}
fn sample(c:&Cover,u:f64,v:f64,ch:usize)->f64{
    let x=u.clamp(0.,c.width.saturating_sub(1)as f64);let y=v.clamp(0.,c.height.saturating_sub(1)as f64);let x0=x.floor()as u32;let y0=y.floor()as u32;let x1=(x0+1).min(c.width-1);let y1=(y0+1).min(c.height-1);
    let at=|x:u32,y:u32|c.pixels[((y*c.width+x)*4)as usize+ch]as f64;let a=at(x0,y0)+(at(x1,y0)-at(x0,y0))*(x-x0 as f64);let b=at(x0,y1)+(at(x1,y1)-at(x0,y1))*(x-x0 as f64);a+(b-a)*(y-y0 as f64)
}
fn displacement(depth:f64,thickness:f64)->f64{thickness*1.8*(1.-depth.clamp(0.,1.)).powf(0.58)}
pub fn texture(w:u32,h:u32,r:f64,cover:Option<&Cover>,tint:u32)->Vec<u8>{
    let mut pixels=vec![0;w as usize*h as usize*4];let thickness=22_f64.min(h as f64*0.22).max(6.);
    let scale=cover.map(|c|(w as f64/c.width as f64).max(h as f64/c.height as f64)).unwrap_or(1.);
    for y in 0..h{for x in 0..w{
        let(d,nx,ny)=bevel(x as f64+0.5,y as f64+0.5,w as f64,h as f64,r);if d>0.{continue}
        let depth=(-d/thickness).clamp(0.,1.);let edge=1.-depth;let bend=displacement(depth,thickness);
        let alpha=(0.5-d).clamp(0.,1.);let light=(-nx*0.55-ny*0.83).max(0.);
        let fresnel=edge.powf(3.5)*(12.+30.*light.powi(4));let caustic=(depth*std::f64::consts::PI).sin().powi(10)*(8.+14.*light);
        let i=((y*w+x)*4)as usize;
        for ch in 0..3{
            let base=((tint>>(ch*8))&255)as f64;
            let source=if let Some(c)=cover{
                let dispersion=1.+(ch as f64-1.)*0.055;
                let u=((x as f64-w as f64/2.)/1.025-nx*bend*dispersion)/scale+c.width as f64/2.;
                let v=((y as f64-h as f64/2.)/1.025-ny*bend*dispersion)/scale+c.height as f64/2.;sample(c,u,v,ch)
            }else{base};
            // The independent glyph layer always has a bounded-luminance reading area.
            // Keep large artwork shapes out of the reading plane; refraction is
            // concentrated in the bevel rather than a magnified poster centre.
            let value=base*0.48+source*(0.025+0.38*edge)+fresnel+caustic;
            pixels[i+ch]=(value.min(if depth>=1.{66.}else{112.})*alpha).round()as u8;
        }pixels[i+3]=(alpha*255.).round()as u8;
    }}pixels
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn lens_is_premultiplied_and_readable(){let c=Cover{width:32,height:32,pixels:[255,255,255,255].repeat(1024)};let p=texture(320,160,30.,Some(&c),0x808080);assert_eq!(p[3],0);let i=((80*320+160)*4)as usize;assert_eq!(p[i+3],255);assert!(p[i..i+3].iter().all(|v|*v<=66));for v in p.chunks_exact(4){assert!(v[0]<=v[3]&&v[1]<=v[3]&&v[2]<=v[3]);}}
    #[test]fn refraction_displaces_texture_and_joins_centre(){assert!(displacement(0.,22.)>30.);assert_eq!(displacement(1.,22.),0.);let c=Cover{width:32,height:32,pixels:(0..1024).flat_map(|i|[(i%32*8)as u8,(i/32*8)as u8,100,255]).collect()};assert_ne!(texture(320,160,30.,Some(&c),0x303030),texture(320,160,30.,None,0x303030));let(_,x,y)=bevel(319.,80.,320.,160.,30.);assert!(x>0.99&&y.abs()<0.001);}
}
