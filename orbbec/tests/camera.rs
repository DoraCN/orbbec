//! Integration tests against a real Orbbec camera.
//!
//! These tests require the Orbbec SDK shared library to be loadable and a
//! camera to be connected. They are gated behind the `ORBBEC_TEST=1`
//! environment variable so a plain `cargo test` still passes on machines
//! without the hardware:
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! export ORBBEC_TEST=1
//! cargo test -p orbbec --release --test camera
//! ```

use std::time::Duration;

use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::{AlignFilter, CameraParam, Context, PointCloud, PointCloudFilter, PointFormat};

/// Returns `true` if hardware tests should run.
fn hardware_enabled() -> bool {
    std::env::var("ORBBEC_TEST").is_ok_and(|v| v == "1" || v == "true")
}

fn context() -> Context {
    Context::new().expect("failed to create Orbbec context")
}

#[test]
fn context_creation() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let ctx = context();
    assert!(!ctx.as_raw().is_null());
}

#[test]
fn enumerate_devices() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let ctx = context();
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "expected at least one Orbbec device");
    let first = &devices[0];
    assert_eq!(first.vid, 0x2bc5, "unexpected vendor id");
    assert!(!first.serial_number.is_empty());
    println!("device: {} vid={:04X} pid={:04X} sn={}", first.name, first.vid, first.pid, first.serial_number);
}

#[test]
fn open_and_read_device_info() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let ctx = context();
    let device = ctx.open_device(0).expect("failed to open device");
    let info = device.info().expect("failed to read device info");
    assert_eq!(info.vid, 0x2bc5);
    println!("opened: {} ({})", info.name, info.connection_type);
}

#[test]
fn capture_depth_and_color_frames() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let _ctx = context();
    let mut config = Config::new().expect("failed to create config");
    config.enable_stream(StreamType::Depth).expect("depth");
    config.enable_stream(StreamType::Color).expect("color");

    let mut pipeline = Pipeline::new().expect("failed to create pipeline");
    pipeline.enable_frame_sync().expect("frame sync");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut got_depth = false;
    let mut got_color = false;
    while std::time::Instant::now() < deadline {
        let frameset = frames
            .recv_timeout(Duration::from_secs(2))
            .expect("timed out waiting for frameset");
        if let Some(depth) = frameset.frame(FrameType::Depth) {
            assert!(depth.width() > 0 && depth.height() > 0);
            assert!(depth.data_size() >= depth.width() * depth.height() * 2);
            assert!(depth.timestamp_us() > 0);
            assert!(depth.system_timestamp_us() > 0);
            got_depth = true;
            println!("depth {}x{} idx={} bytes={}", depth.width(), depth.height(), depth.index(), depth.data_size());
        }
        if let Some(color) = frameset.frame(FrameType::Color) {
            assert!(color.width() > 0 && color.height() > 0);
            assert!(color.data_size() > 0);
            got_color = true;
            println!("color {}x{} idx={} bytes={}", color.width(), color.height(), color.index(), color.data_size());
        }
        if got_depth && got_color {
            break;
        }
    }
    assert!(got_depth, "no depth frame received");
    assert!(got_color, "no color frame received");
    pipeline.stop().expect("failed to stop pipeline");
}

#[test]
fn camera_params_are_valid() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let _ctx = context();
    let pipeline = Pipeline::new().expect("failed to create pipeline");
    let param: CameraParam = pipeline.camera_param().expect("failed to read camera params");
    assert!(param.depth.fx > 0.0 && param.depth.fy > 0.0);
    assert!(param.rgb.fx > 0.0 && param.rgb.fy > 0.0);
    assert!(param.depth.width > 0 && param.rgb.width > 0);
    println!("depth fx={} fy={} cx={} cy={}", param.depth.fx, param.depth.fy, param.depth.cx, param.depth.cy);
    println!("rgb   fx={} fy={} cx={} cy={}", param.rgb.fx, param.rgb.fy, param.rgb.cx, param.rgb.cy);
}

#[test]
fn align_depth_to_color() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let _ctx = context();
    let mut config = Config::new().expect("config");
    config.enable_stream(StreamType::Depth).expect("depth");
    config.enable_stream(StreamType::Color).expect("color");

    let mut pipeline = Pipeline::new().expect("pipeline");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let align = AlignFilter::new().expect("align filter");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut aligned_ok = false;
    while std::time::Instant::now() < deadline {
        let frameset = frames
            .recv_timeout(Duration::from_secs(2))
            .expect("timed out");
        if frameset.frame(FrameType::Color).is_none() {
            continue;
        }
        align
            .set_align_target(&frameset, StreamType::Color)
            .expect("set align target");
        let aligned = align
            .process(&frameset)
            .expect("align process")
            .expect("align produced no frame");
        // Aligned depth matches the color resolution.
        assert!(aligned.width() > 0 && aligned.height() > 0);
        println!("aligned depth {}x{} (color {}x{})", aligned.width(), aligned.height(), frameset.frame(FrameType::Color).unwrap().width(), frameset.frame(FrameType::Color).unwrap().height());
        aligned_ok = true;
        break;
    }
    assert!(aligned_ok, "no aligned depth frame produced");
    pipeline.stop().expect("stop");
}

#[test]
fn generate_rgb_pointcloud() {
    if !hardware_enabled() {
        eprintln!("skipped: set ORBBEC_TEST=1 to run hardware tests");
        return;
    }
    let _ctx = context();
    let mut config = Config::new().expect("config");
    config.enable_stream(StreamType::Depth).expect("depth");
    config.enable_stream(StreamType::Color).expect("color");

    let mut pipeline = Pipeline::new().expect("pipeline");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let align = AlignFilter::new().expect("align filter");
    let pc = PointCloudFilter::new().expect("point cloud filter");
    pc.set_point_format(PointFormat::XyzRgb).expect("format");
    pc.set_coordinate_scale(0.001).expect("scale");

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut generated = false;
    while std::time::Instant::now() < deadline {
        let frameset = frames
            .recv_timeout(Duration::from_secs(2))
            .expect("timed out");
        if frameset.frame(FrameType::Depth).is_none() || frameset.frame(FrameType::Color).is_none() {
            continue;
        }
        align
            .set_align_target(&frameset, StreamType::Color)
            .expect("set align target");
        let aligned = align
            .process(&frameset)
            .expect("align")
            .expect("no aligned frame");
        let cloud = pc
            .generate_frame(&aligned)
            .expect("point cloud")
            .expect("no cloud frame");
        let cloud = PointCloud::from_frame(cloud, PointFormat::XyzRgb);
        let valid: Vec<[f32; 3]> = cloud
            .points()
            .into_iter()
            .filter(|p| p[2] > 0.0)
            .collect();
        assert!(!valid.is_empty(), "point cloud had no valid points");
        println!("point cloud: {} points, {} valid", cloud.len(), valid.len());
        generated = true;
        break;
    }
    assert!(generated, "no point cloud generated");
    pipeline.stop().expect("stop");
}
