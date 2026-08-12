//! Generate an RGB point cloud from synchronized depth+color frames and report
//! point statistics (count, bounding box, a few samples).
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example pointcloud
//! ```

use std::time::Duration;

use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::{AlignFilter, Context, PointCloud, PointCloudFilter, PointFormat};

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
    pipeline
        .enable_frame_sync()
        .expect("failed to enable frame sync");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let align = AlignFilter::new().expect("failed to create align filter");
    let pointcloud = PointCloudFilter::new().expect("failed to create point cloud filter");
    pointcloud
        .set_point_format(PointFormat::XyzRgb)
        .expect("failed to set point format");
    pointcloud
        .set_coordinate_scale(0.001)
        .expect("failed to set coordinate scale");

    let mut shown = 0u32;
    while shown < 3 {
        let frameset = match frames.recv_timeout(Duration::from_millis(2000)) {
            Ok(fs) => fs,
            Err(e) => {
                eprintln!("recv error: {e}");
                break;
            }
        };
        // Only proceed when the frameset carries both depth and color.
        if frameset.frame(FrameType::Depth).is_none() || frameset.frame(FrameType::Color).is_none() {
            continue;
        }

        align
            .set_align_target(&frameset, StreamType::Color)
            .expect("failed to set align target");
        let aligned = align
            .process(&frameset)
            .expect("failed to align frameset")
            .expect("align produced no frame");
        let cloud = pointcloud
            .generate_frame(&aligned)
            .expect("failed to generate point cloud")
            .expect("point cloud filter produced no frame");

        let cloud = PointCloud::from_frame(cloud, PointFormat::XyzRgb);
        println!("==== point cloud #{shown} ====");
        let valid: Vec<[f32; 3]> = cloud
            .points()
            .into_iter()
            .filter(|p| p[2] > 0.0 && p[2] < 10.0)
            .collect();
        println!(
            "  points: {} (valid <10m: {})",
            cloud.len(),
            valid.len()
        );
        if !valid.is_empty() {
            let mut min = valid[0];
            let mut max = valid[0];
            for p in valid.iter().skip(1) {
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
            println!("  bbox min: {min:?}");
            println!("  bbox max: {max:?}");
            let n = valid.len().min(3);
            for p in valid.iter().take(n) {
                println!("  sample: {p:?}");
            }
        }

        let colored = cloud.colored_points();
        if !colored.is_empty() {
            let c = colored[colored.len() / 2];
            println!("  center color: r={:.2} g={:.2} b={:.2}", c.r, c.g, c.b);
        }
        shown += 1;
    }

    pipeline.stop().expect("failed to stop pipeline");
}
