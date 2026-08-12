//! Real-time distance measurement: place a sheet of paper (or any large flat
//! surface) in front of the camera and this example continuously prints its
//! distance in metres.
//!
//! How it works: for every depth frame it builds a histogram of depth values
//! and takes the *mode* (the most common depth). A large flat surface such as
//! paper dominates the frame, so the mode tracks the paper's distance. A moving
//! average smooths frame-to-frame jitter.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example distance
//! ```

use std::time::{Duration, Instant};

use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::{Context, DepthFrame};

/// Histogram bin width in millimetres.
const BIN_MM: u16 = 10;
/// Depth range considered (0.1 m .. 8 m).
const MAX_MM: u16 = 8000;
/// Minimum fraction of pixels supporting the mode, otherwise report no paper.
const MIN_SUPPORT: f32 = 0.04;
/// Smoothing factor for the moving average (0..1, higher = more responsive).
const SMOOTHING: f32 = 0.3;

fn main() {
    let ctx = Context::new().expect("failed to create context");
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "no Orbbec device connected");

    let mut config = Config::new().expect("failed to create pipeline config");
    config
        .enable_stream(StreamType::Depth)
        .expect("failed to enable depth stream");

    let mut pipeline = Pipeline::new().expect("failed to create pipeline");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let n_bins = (MAX_MM / BIN_MM) as usize;
    let mut smoothed: Option<f32> = None;
    let mut frames_seen = 0u32;

    println!("point the camera at a sheet of paper (or any large flat surface)");
    println!("distance = dominant surface depth, updated in real time\n");
    println!("{:>6}  {:>8}  {:>8}  {:>10}", "#", "dist(m)", "avg(m)", "support");

    let mut last_render = Instant::now();
    loop {
        match frames.recv_timeout(Duration::from_millis(1000)) {
            Ok(frameset) => {
                let Some(frame) = frameset.frame(FrameType::Depth) else {
                    continue;
                };
                let Some(depth) = DepthFrame::try_new(frame) else {
                    eprintln!("unexpected depth format");
                    break;
                };

                let (distance, support) = dominant_distance(&depth, n_bins);
                frames_seen += 1;

                // Exponential moving average for a stable readout.
                smoothed = Some(match smoothed {
                    Some(prev) => prev + SMOOTHING * (distance - prev),
                    None => distance,
                });

                let avg = smoothed.unwrap();
                // Refresh the console line a few times per second.
                if last_render.elapsed() >= Duration::from_millis(200) {
                    println!(
                        "{:>6}  {:>8.3}  {:>8.3}  {:>9.1}%",
                        frames_seen,
                        distance,
                        avg,
                        support * 100.0
                    );
                    last_render = Instant::now();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("timed out waiting for depth frame");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    pipeline.stop().expect("failed to stop pipeline");
}

/// Find the most frequent valid depth value and its support ratio.
fn dominant_distance(depth: &DepthFrame, n_bins: usize) -> (f32, f32) {
    let pixels = depth.pixels();
    let mut hist = vec![0u32; n_bins];
    let mut valid = 0u32;

    for v in pixels {
        // Exclude 0 (invalid/no signal) and out-of-range values.
        if (1..MAX_MM).contains(&v) {
            hist[(v / BIN_MM) as usize] += 1;
            valid += 1;
        }
    }

    let (best_bin, best_count) = hist
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .map(|(b, c)| (b, *c))
        .unwrap_or((0, 0));

    let support = if valid > 0 {
        best_count as f32 / valid as f32
    } else {
        0.0
    };

    if support < MIN_SUPPORT {
        // Nothing large and flat dominates: fall back to the median of the
        // central region so the output still tracks *something* in front.
        let cx = depth.width() / 2;
        let cy = depth.height() / 2;
        let mut vals = vec![];
        for y in (cy.saturating_sub(40))..(cy + 40).min(depth.height()) {
            for x in (cx.saturating_sub(40))..(cx + 40).min(depth.width()) {
                if let Some(v) = depth.pixel(x, y) {
                    if v > 0 {
                        vals.push(v);
                    }
                }
            }
        }
        if vals.is_empty() {
            return (0.0, 0.0);
        }
        vals.sort_unstable();
        return (vals[vals.len() / 2] as f32 / 1000.0, support);
    }

    (best_bin as f32 * BIN_MM as f32 / 1000.0, support)
}
