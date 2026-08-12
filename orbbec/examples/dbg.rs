use orbbec::pipeline::{Config, FrameType, Frameset, Pipeline, StreamType};
use orbbec::{AlignFilter, BoundingBox, Context, DepthFrame};
fn main() {
    let _ctx = Context::new().unwrap();
    let mut cfg = Config::new().unwrap();
    cfg.enable_stream(StreamType::Depth).unwrap();
    cfg.enable_stream(StreamType::Color).unwrap();
    let mut p = Pipeline::new().unwrap();
    p.enable_frame_sync().unwrap();
    let rx = p.start_capture(Some(&cfg)).unwrap();
    let align = AlignFilter::new().unwrap();
    for k in 0..20 {
        let fs = match rx.recv_timeout(std::time::Duration::from_secs(2)) { Ok(f) => f, Err(_) => break };
        if fs.frame(FrameType::Color).is_none() || fs.frame(FrameType::Depth).is_none() { continue; }
        align.set_align_target(&fs, StreamType::Color).unwrap();
        let aligned = match align.process(&fs) { Ok(Some(f)) => f, _ => continue };
        let afs = Frameset::from_frame(aligned);
        let Some(d) = afs.frame(FrameType::Depth) else { continue };
        let d = DepthFrame::try_new(d).unwrap();
        let b = BoundingBox::new(100, 80, 300, 220);
        if k % 3 == 0 {
            println!("k={k} box_distance={:?}", d.box_distance(&b));
        }
    }
    p.stop().unwrap();
}
