//! A bounded cubic-bezier easing curve shared by the visual editor and native animation.
pub const EMPHASIZED:[f64;4]=[0.2,0.,0.,1.];
pub fn valid(c:[f64;4])->bool{c.iter().all(|v|v.is_finite()&&(0.0..=1.0).contains(v))}
pub fn sample(c:[f64;4],time:f64)->(f64,f64){
    let c=if valid(c){c}else{EMPHASIZED};let x=time.clamp(0.,1.);
    let value=|a:f64,b:f64,t:f64|3.*(1.-t).powi(2)*t*a+3.*(1.-t)*t*t*b+t.powi(3);
    let derivative=|a:f64,b:f64,t:f64|3.*(1.-t).powi(2)*a+6.*(1.-t)*t*(b-a)+3.*t*t*(1.-b);
    let(mut lo,mut hi)=(0.,1.);for _ in 0..28{let t=(lo+hi)/2.;if value(c[0],c[2],t)<x{lo=t}else{hi=t}}
    let t=if x<=0.{0.}else if x>=1.{1.}else{(lo+hi)/2.};let dx=derivative(c[0],c[2],t);let dy=derivative(c[1],c[3],t);let slope=if dx.abs()>1e-8{dy/dx}else{let t=if t<0.5{0.00001}else{0.99999};let dx=derivative(c[0],c[2],t);let dy=derivative(c[1],c[3],t);if dx.abs()>1e-12{dy/dx}else{0.}};
    (value(c[1],c[3],t).clamp(0.,1.),slope.clamp(0.,1000.))
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn linear_and_emphasized_curves_have_correct_endpoints_and_are_monotonic(){for c in [[0.,0.,1.,1.],EMPHASIZED,[1.,0.,0.,1.],[0.4,0.,0.2,1.]]{assert_eq!(sample(c,0.).0,0.);assert_eq!(sample(c,1.).0,1.);let mut last=0.;for i in 0..=1000{let(y,v)=sample(c,i as f64/1000.);assert!(y+1e-6>=last&&v.is_finite());last=y;}}for i in 1..100{let t=i as f64/100.;assert!((sample([0.,0.,1.,1.],t).0-t).abs()<1e-6);}}
    #[test]fn invalid_control_points_are_rejected(){assert!(!valid([f64::NAN,0.,0.,1.]));assert!(!valid([-0.1,0.,0.,1.]));}
}
