//! Capture frames from the depth and color streams over a channel and print
//! per-frame info (using the typed DepthFrame / ColorFrame wrappers) for a few
//! seconds.
//!
//! Run from the repo root with the SDK env vars set (see docs/install-sdk.md §6):
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example frames
//! ```

use std::time::Duration;

use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::{ColorFrame, Context, DepthFrame};

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

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut count = 0u32;
    while std::time::Instant::now() < deadline {
        match frames.recv_timeout(Duration::from_millis(2000)) {
            Ok(frameset) => {
                if let Some(frame) = frameset.frame(FrameType::Depth) {
                    if let Some(d) = DepthFrame::try_new(frame) {
                        println!(
                            "[{:>5}] depth: {}x{} idx={} center={}mm",
                            count,
                            d.width(),
                            d.height(),
                            d.center_depth_mm().map(|v| v as i32).unwrap_or(-1),
                            d.pixel(10, 10).unwrap_or(0),
                        );
                    }
                }
                if let Some(frame) = frameset.frame(FrameType::Color) {
                    let (w, h, fmt) = (frame.width(), frame.height(), frame.format());
                    match ColorFrame::try_new(frame) {
                        Some(c) => {
                            let (r, g, b) = c
                                .pixel_rgb(c.width() / 2, c.height() / 2)
                                .unwrap_or((0, 0, 0));
                            println!(
                                "[{:>5}] color: {}x{} fmt={} center_rgb=({r},{g},{b})",
                                count,
                                c.width(),
                                c.height(),
                                c.format(),
                            );
                        }
                        None => {
                            println!("[{:>5}] color: {w}x{h} fmt={fmt} (uncompressed color)", count);
                        }
                    }
                }
                count += 1;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("timeout waiting for frameset");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("capture channel disconnected");
                break;
            }
        }
    }

    pipeline.stop().expect("failed to stop pipeline");
    println!("\ncaptured {count} frameset(s)");
}
