//! Verify depth→color alignment via the Align filter and read camera
//! intrinsics; unproject a few aligned depth pixels to 3D camera-frame points.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example aligned
//! ```

use std::sync::mpsc;
use std::time::Duration;

use orbbec::pipeline::{Config, Frame, FrameType, Pipeline, StreamType};
use orbbec::{AlignFilter, Context};

fn main() {
    let ctx = Context::new().expect("failed to create context");
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "no Orbbec device connected");

    let mut config = Config::new().expect("failed to create pipeline config");
    config
        .enable_stream(StreamType::Depth)
        .expect("failed to enable depth stream");
    config
        .enable_stream(StreamType::Color)
        .expect("failed to enable color stream");

    let mut pipeline = Pipeline::new().expect("failed to create pipeline");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let align = AlignFilter::new().expect("failed to create align filter");
    let (tx, rx) = mpsc::channel::<Frame>();
    align
        .set_callback(move |frame| {
            let _ = tx.send(frame);
        })
        .expect("failed to set align callback");

    let mut depth: Option<Frame> = None;
    let mut shown = 0u32;
    while shown < 3 {
        match frames.recv_timeout(Duration::from_millis(2000)) {
            Ok(frameset) => {
                // Set the alignment target (color stream) on the first frameset
                // that carries a color frame, then push the depth frame.
                if shown == 0 {
                    align
                        .set_align_target(&frameset, StreamType::Color)
                        .expect("failed to set align target");
                }
                if let Some(f) = frameset.frame(FrameType::Depth) {
                    align
                        .push_frame(&f)
                        .expect("failed to push depth frame to align filter");
                    depth = Some(f);
                    let _ = frameset;
                }
            }
            Err(e) => {
                eprintln!("recv error: {e}");
                break;
            }
        }

        if let Ok(aligned) = rx.try_recv() {
            let param = pipeline.camera_param().expect("failed to read camera params");
            if shown == 0 {
                print_intrinsics(&param);
            }
            let w = aligned.width();
            let h = aligned.height();
            let data = aligned.data();
            let stride = w as usize * 2; // Z16
            let cx = w as f32 / 2.0;
            let cy = h as f32 / 2.0;
            println!("aligned depth: {w}x{h}");
            for (du, dv) in [(cx, cy), (cx - 10.0, cy), (cx + 10.0, cy)] {
                let idx = (dv as usize) * stride + (du as usize) * 2;
                if idx + 1 < data.len() {
                    let z_mm = u16::from_le_bytes([data[idx], data[idx + 1]]) as f32;
                    if z_mm > 0.0 {
                        let p = param.rgb.unproject(du, dv, z_mm);
                        println!("  ({du:6.1},{dv:6.1}) z={z_mm:6.1}mm -> xyz={p:?} (m)");
                    } else {
                        println!("  ({du:6.1},{dv:6.1}) invalid depth");
                    }
                }
            }
            shown += 1;
        }
        if shown > 0 {
            depth = None;
        }
    }
    drop(depth);
    drop(rx);

    pipeline.stop().expect("failed to stop pipeline");
}

fn print_intrinsics(param: &orbbec::CameraParam) {
    let d = param.depth;
    let rgb = param.rgb;
    println!(
        "depth intrinsics: fx={:.2} fy={:.2} cx={:.2} cy={:.2}  {}x{}",
        d.fx, d.fy, d.cx, d.cy, d.width, d.height
    );
    println!(
        "rgb   intrinsics: fx={:.2} fy={:.2} cx={:.2} cy={:.2}  {}x{}",
        rgb.fx, rgb.fy, rgb.cx, rgb.cy, rgb.width, rgb.height
    );
    println!(
        "distortion model: depth={} rgb={}",
        param.depth_distortion.model, param.rgb_distortion.model
    );
    println!("transform trans_mm={:?}\n", param.transform.trans_mm);
}
