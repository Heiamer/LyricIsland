//! A cached cover-space lens. Only the material is displaced; glyphs never enter the texture.
//! A rounded-rectangle normal bends the cover at the bevel with slight RGB dispersion.
pub struct Cover { pub pixels:Vec<u8>, pub width:u32, pub height:u32 }

fn bevel(x:f64,y:f64,w:f64,h:f64,r:f64)->(f64,f64,f64){
    let r=r.min(w/2.).min(h/2.).max(1.);
    let qx=(x-w/2.).abs()-(w/2.-r);let qy=(y-h/2.).abs()-(h/2.-r);
    let ax=qx.max(0.);let ay=qy.max(0.);let length=ax.hypot(ay);
    let distance=length+qx.max(qy).min(0.)-r;
    let (nx,ny)=if length>0.0001{(ax/length,ay/length)}else if qx>qy{(1.,0.)}else{(0.,1.)};
    (distance,nx*(x-w/2.).signum(),ny*(y-h/2.).signum())
}

fn sample(cover:&Cover,u:f64,v:f64,ch:usize)->f64{
    let x=u.clamp(0.,cover.width.saturating_sub(1)as f64);let y=v.clamp(0.,cover.height.saturating_sub(1)as f64);
    let x0=x.floor()as u32;let y0=y.floor()as u32;let x1=(x0+1).min(cover.width-1);let y1=(y0+1).min(cover.height-1);
    let at=|x:u32,y:u32|cover.pixels[((y*cover.width+x)*4)as usize+ch]as f64;
    let a=at(x0,y0)+(at(x1,y0)-at(x0,y0))*(x-x0 as f64);
    let b=at(x0,y1)+(at(x1,y1)-at(x0,y1))*(x-x0 as f64);
    a+(b-a)*(y-y0 as f64)
}

pub fn texture(w:u32,h:u32,r:f64,cover:Option<&Cover>,tint:u32)->Vec<u8>{
    let softened=cover.and_then(|c|image::RgbaImage::from_raw(c.width,c.height,c.pixels.clone()).map(|im|Cover{pixels:image::imageops::blur(&im,2.2).into_raw(),width:c.width,height:c.height}));
    let cover=softened.as_ref();
    let mut pixels=vec![0;w as usize*h as usize*4];
    let thickness=10_f64.min(h as f64*0.13).max(4.);
    let scale=cover.map(|c|(w as f64/c.width as f64).max(h as f64/c.height as f64)).unwrap_or(1.);
    for y in 0..h{for x in 0..w{
        let (d,nx,ny)=bevel(x as f64+0.5,y as f64+0.5,w as f64,h as f64,r);
        if d>0.||d < -thickness{continue}
        let depth=(-d/thickness).clamp(0.,1.);
        let edge=(1.-depth).powf(1.6);
        // The convex bevel transitions continuously into the flat, frosted centre.
        let bend=thickness*1.65*(1.-depth).sqrt();
        let alpha=(0.50*edge*(0.5-d).clamp(0.,1.)).clamp(0.,1.);
        let specular=(-nx*0.6-ny*0.8).max(0.).powi(5)*edge*24.;
        let caustic=(depth*std::f64::consts::PI).sin().powi(6)*4.;
        let i=((y*w+x)*4)as usize;
        for ch in 0..3{
            let base=((tint>>(ch*8))&255)as f64;
            let c=if let Some(c)=cover{
                let dispersion=1.+(ch as f64-1.)*0.045;
                let u=(x as f64-w as f64/2.-nx*bend*dispersion)/scale+c.width as f64/2.;
                let v=(y as f64-h as f64/2.-ny*bend*dispersion)/scale+c.height as f64/2.;
                sample(c,u,v,ch)*0.19+base*0.30
            }else{base*0.8};
            // Even a white poster cannot wash out white text near the rounded corners.
            pixels[i+ch]=((c+specular+caustic).min(90.)*alpha).round()as u8;
        }
        pixels[i+3]=(alpha*255.).round()as u8;
    }}
    pixels
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn lens_preserves_clear_center_and_premultiplied_alpha(){
        let c=Cover{width:32,height:32,pixels:[255,255,255,255].repeat(1024)};
        let p=texture(320,160,30.,Some(&c),0x303030);
        assert_eq!(p[((80*320+160)*4+3)as usize],0);
        assert_eq!(p[3],0);
        assert!(p.chunks_exact(4).any(|v|v[3]>0));
        for v in p.chunks_exact(4){assert!(v[0]<=v[3]&&v[1]<=v[3]&&v[2]<=v[3]);}
    }
    #[test]fn bevel_is_continuous_and_points_outward(){
        let (d,x,y)=bevel(160.,1.,320.,160.,30.);assert!((d+1.).abs()<0.001&&x.abs()<0.001&&y < -0.99);
        let (_,x,y)=bevel(319.,80.,320.,160.,30.);assert!(x>0.99&&y.abs()<0.001);
        let c=Cover{width:32,height:32,pixels:(0..1024).flat_map(|i|[(i%32*8)as u8,(i/32*8)as u8,100,255]).collect()};
        assert_ne!(texture(320,160,30.,Some(&c),0x303030),texture(320,160,30.,None,0x303030));
    }
}
