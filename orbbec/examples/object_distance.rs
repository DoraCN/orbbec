//! Measure the distance of objects inside detection boxes in real time.
//!
//! This is the bridge between a YOLO detector and the depth camera:
//!
//! 1. The depth stream is D2C-aligned to the color resolution, so the aligned
//!    depth frame has the same pixel size as the color frame.
//! 2. YOLO boxes are given in color-frame pixels (e.g. `--box=100,200,300,400`).
//! 3. For each box we read the median of valid depth pixels inside it and print
//!    the distance in real time, smoothed by a sliding window.
//!
//! If your YOLO model letterboxes/resizes to e.g. 640×640 internally, scale the
//! boxes back to the color frame's native resolution before passing them here.
//! The default color profile is 1280×720, so the aligned depth is 1280×720 too.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! # measure two boxes: one at top-left, one at bottom-right
//! cargo run --release --example object_distance -- \
//!     --box=100,80,300,220 --box=900,450,1200,650
//! ```

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use orbbec::pipeline::{Config, FrameType, Frameset, Pipeline, StreamType};
use orbbec::{AlignFilter, BoundingBox, Context, DepthFrame};

const MIN_SUPPORT: f32 = 0.2;
const WINDOW_LEN: usize = 7;
const WINDOW_MIN_VALID: usize = 4;
const WINDOW_MIN_CONSISTENT: usize = 3;
const WINDOW_TOL_M: f32 = 0.10;
const SMOOTHING: f32 = 0.3;

fn main() {
    let boxes = parse_boxes();

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

    // Align depth to the color frame so boxes map 1:1.
    let align = AlignFilter::new().expect("failed to create align filter");

    let mut smoothers: Vec<Smoother> = (0..boxes.len()).map(|_| Smoother::new()).collect();

    println!(
        "aligned depth frame size is the color resolution; {} box(es):",
        boxes.len()
    );
    for (i, b) in boxes.iter().enumerate() {
        println!(
            "  box[{i}]: x={} y={} w={} h={}",
            b.x, b.y, b.w, b.h
        );
    }
    println!("\n{:>5}  {:>8}  {:>8}  {:>9}", "box", "dist(cm)", "avg(cm)", "support");

    let mut last_render = Instant::now();
    loop {
        match frames.recv_timeout(Duration::from_millis(1000)) {
            Ok(frameset) => {
                if !ready(&frameset) {
                    continue;
                }

                // Depth → color alignment (synchronous).
                align
                    .set_align_target(&frameset, StreamType::Color)
                    .expect("set align target");
                let aligned = match align.process(&frameset) {
                    Ok(Some(f)) => f,
                    _ => continue,
                };
                let aligned = Frameset::from_frame(aligned);
                let Some(depth) = aligned.frame(FrameType::Depth) else {
                    continue;
                };
                let Some(depth) = DepthFrame::try_new(depth) else {
                    continue;
                };

                // Each box gets an independent distance + smoothing.
                let mut line = String::new();
                let mut any = false;
                for (i, b) in boxes.iter().enumerate() {
                    let (m, support) = depth
                        .box_distance(b)
                        .filter(|(_, s)| *s >= MIN_SUPPORT)
                        .unwrap_or((0.0, 0.0));
                    let confirmed = smoothers[i].update((m > 0.0).then_some(m));
                    let dist_cm = confirmed.map(|m| m * 100.0);
                    let avg_cm = smoothers[i].avg().map(|m| m * 100.0);

                    let cell = match (dist_cm, avg_cm) {
                        (Some(d), Some(a)) => {
                            format!("{d:>7.1}  {a:>7.1}  {:>7.1}%", support * 100.0)
                        }
                        _ => format!("{:>7}  {:>7}  {:>7}", "o-r", "-", "-"),
                    };
                    line.push_str(&format!("  box[{i}] {cell}"));
                    any = true;
                }
                if any && last_render.elapsed() >= Duration::from_millis(200) {
                    println!("{line}");
                    last_render = Instant::now();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("timed out waiting for frameset");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    pipeline.stop().expect("failed to stop pipeline");
}

/// A frameset must carry both depth and color for alignment.
fn ready(frameset: &Frameset) -> bool {
    frameset.frame(FrameType::Depth).is_some() && frameset.frame(FrameType::Color).is_some()
}

/// Per-box sliding-window decision + moving average.
struct Smoother {
    window: VecDeque<Option<f32>>,
    avg: Option<f32>,
}

impl Smoother {
    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(WINDOW_LEN),
            avg: None,
        }
    }

    fn update(&mut self, m: Option<f32>) -> Option<f32> {
        self.window.push_back(m);
        while self.window.len() > WINDOW_LEN {
            self.window.pop_front();
        }

        let valid: Vec<f32> = self.window.iter().filter_map(|v| *v).collect();
        let confirmed = if valid.len() >= WINDOW_MIN_VALID {
            let mut sorted = valid.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = sorted[sorted.len() / 2];
            let consistent = valid
                .iter()
                .filter(|&&v| (v - median).abs() <= WINDOW_TOL_M)
                .count();
            (consistent >= WINDOW_MIN_CONSISTENT).then_some(median)
        } else {
            None
        };

        self.avg = Some(match (self.avg, confirmed) {
            (Some(a), Some(m)) => a + SMOOTHING * (m - a),
            (_, Some(m)) => m,
            _ => return None,
        });
        confirmed
    }

    fn avg(&self) -> Option<f32> {
        self.avg
    }
}

/// Parse `--box=x,y,w,h` arguments (repeatable, color-frame pixels).
fn parse_boxes() -> Vec<BoundingBox> {
    let mut boxes = vec![];
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--box=") {
            let parts: Vec<u32> = v
                .split(',')
                .filter_map(|s| s.trim().parse::<u32>().ok())
                .collect();
            if parts.len() == 4 && parts[2] > 0 && parts[3] > 0 {
                boxes.push(BoundingBox::new(parts[0], parts[1], parts[2], parts[3]));
            } else {
                eprintln!("invalid --box, expected --box=x,y,w,h (pixels)");
                std::process::exit(2);
            }
        }
    }
    if boxes.is_empty() {
        eprintln!("usage: cargo run --release --example object_distance -- --box=x,y,w,h [...]");
        std::process::exit(2);
    }
    boxes
}
