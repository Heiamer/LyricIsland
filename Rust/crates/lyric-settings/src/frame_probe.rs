//! Opt-in measurement of real window frames, using the same render path as normal UI motion.
use super::*;
struct Probe{path:PathBuf,last:Instant,intervals:Vec<f64>,warmup:usize}
pub fn start(window:&mut Window,_cx:&mut App,path:PathBuf){
    let probe=Rc::new(RefCell::new(Probe{path,last:Instant::now(),intervals:Vec::new(),warmup:30}));
    window.on_next_frame(move|window,cx|tick(window,cx,probe));
}
fn tick(window:&mut Window,cx:&mut App,probe:Rc<RefCell<Probe>>){
    let mut state=probe.borrow_mut();let now=Instant::now();let elapsed=now.duration_since(state.last).as_secs_f64()*1000.;state.last=now;
    if state.warmup>0{state.warmup-=1}else{state.intervals.push(elapsed);}
    let n=state.intervals.len();
    if n>=240{
        let refresh=window.refresh_interval(cx).map(|v|1./v.as_secs_f64());let mut values=state.intervals.clone();values.sort_by(f64::total_cmp);
        let mean=values.iter().sum::<f64>()/values.len()as f64;
        let report=json!({"displayHz":refresh,"sampleFrames":n,"meanCallbackFps":1000./mean,"medianIntervalMs":values[n/2],"p95IntervalMs":values[n*95/100],"maxIntervalMs":values[n-1],"scheduling":"display compositor; no fixed FPS cap","note":"Callback intervals include real UI rendering and presentation; this is a local measurement, not a GPU-independent frame-rate guarantee."});
        if let Some(parent)=state.path.parent(){let _=std::fs::create_dir_all(parent);}let _=std::fs::write(&state.path,report.to_string());return
    }
    drop(state);
    window.refresh();
    window.on_next_frame(move|window,cx|tick(window,cx,probe));
}
