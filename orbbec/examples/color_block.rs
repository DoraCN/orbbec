//! Find a block of a specified color (default: green) in the color frame and
//! measure its distance in real time.
//!
//! Pipeline:
//!  1. Color stream at 1280×720 RGB (uncompressed, for per-pixel access).
//!  2. Depth is D2C-aligned to the color resolution, so depth pixels match
//!     color pixels 1:1.
//!  3. Green pixels are segmented by HSV thresholding, the largest connected
//!     blob is located, and its bounding box distance is measured via
//!     [`DepthFrame::box_distance`].
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example color_block
//! # custom hue range (degrees, 0..360): --hue-min=200 --hue-max=260  (blue)
//! ```

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use orbbec::pipeline::{Config, FrameType, Frameset, Pipeline, StreamType};
use orbbec::{AlignFilter, BoundingBox, ColorFrame, Context, DepthFrame};

/// Default green hue range in degrees (0..360).
const HUE_MIN: f32 = 75.0;
const HUE_MAX: f32 = 145.0;
const SAT_MIN: f32 = 0.30;
const VAL_MIN: f32 = 0.20;
/// Grid step for blob detection (1 = every pixel, slower).
const STEP: u32 = 2;
/// Minimum blob size in grid cells to be considered (grid cell = STEP² px).
/// Tune down for small blocks, up to ignore colour noise.
const MIN_COMPONENT: u32 = 200;
const MIN_SUPPORT: f32 = 0.2;
const WINDOW_LEN: usize = 7;
const WINDOW_MIN_VALID: usize = 4;
const WINDOW_MIN_CONSISTENT: usize = 3;
const WINDOW_TOL_M: f32 = 0.10;
const SMOOTHING: f32 = 0.3;

fn main() {
    // Optional custom hue range, e.g. --hue-min=200 --hue-max=260.
    let mut hue_min = HUE_MIN;
    let mut hue_max = HUE_MAX;
    let mut min_component = MIN_COMPONENT;
    let args: Vec<String> = std::env::args().collect();
    for w in args.windows(2) {
        if w[0] == "--hue-min" {
            hue_min = w[1].parse().expect("bad --hue-min");
        }
        if w[0] == "--hue-max" {
            hue_max = w[1].parse().expect("bad --hue-max");
        }
        if w[0] == "--min-size" {
            min_component = w[1].parse().expect("bad --min-size");
        }
    }

    let ctx = Context::new().expect("failed to create context");
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "no Orbbec device connected");

    let pipeline0 = Pipeline::new().expect("failed to create pipeline");
    // Match an uncompressed 1280x720@30 color profile (RGB preferred).
    let color_profiles = pipeline0
        .stream_profiles(StreamType::Color)
        .expect("color profiles");
    let color_profile = [22, 31, 23, 25, 0]
        .iter()
        .find_map(|fmt| {
            color_profiles
                .match_video(Some(1280), Some(720), Some(*fmt), Some(30))
                .ok()
                .flatten()
        })
        .expect("no uncompressed 1280x720@30 color profile");

    let mut config = Config::new().expect("failed to create config");
    config
        .enable_stream_with_profile(&color_profile)
        .expect("failed to enable color profile");
    config
        .enable_stream(StreamType::Depth)
        .expect("failed to enable depth stream");

    let mut pipeline = Pipeline::new().expect("failed to create pipeline");
    pipeline
        .enable_frame_sync()
        .expect("failed to enable frame sync");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    let align = AlignFilter::new().expect("failed to create align filter");
    let mut smoother = Smoother::new();

    println!(
        "tracking hue {hue_min:.0}..{hue_max:.0} degrees on {}x{} RGB",
        color_profile.width(),
        color_profile.height()
    );
    println!("{:>9}  {:>9}  {:>9}  {:>14}", "dist(cm)", "avg(cm)", "support", "bbox");

    let mut last_render = Instant::now();
    loop {
        match frames.recv_timeout(Duration::from_millis(1000)) {
            Ok(frameset) => {
                if frameset.frame(FrameType::Color).is_none()
                    || frameset.frame(FrameType::Depth).is_none()
                {
                    continue;
                }

                // Align depth to the color frame.
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

                // Color frame for segmentation.
                let Some(color) = frameset.frame(FrameType::Color) else {
                    continue;
                };
                let Some(color) = ColorFrame::try_new(color) else {
                    eprintln!("unexpected color format, wanted uncompressed");
                    break;
                };

                // Find the largest green blob.
                let bbox = largest_blob(&color, hue_min, hue_max, min_component);
                let Some(bbox) = bbox else {
                    smoother.update(None);
                    if last_render.elapsed() >= Duration::from_millis(200) {
                        println!("{:>9}  {:>9}  {:>9}  {:>14}", "no-block", "-", "-", "-");
                        last_render = Instant::now();
                    }
                    continue;
                };

                let (m, support) = depth
                    .box_distance(&bbox)
                    .filter(|(_, s)| *s >= MIN_SUPPORT)
                    .unwrap_or((0.0, 0.0));
                let confirmed = smoother.update((m > 0.0).then_some(m));
                let dist_cm = confirmed.map(|m| m * 100.0);
                let avg_cm = smoother.avg().map(|m| m * 100.0);

                if last_render.elapsed() >= Duration::from_millis(200) {
                    match (dist_cm, avg_cm) {
                        (Some(d), Some(a)) => println!(
                            "{d:>9.1}  {a:>9.1}  {:>8.1}%  {:>14}",
                            support * 100.0,
                            format!("{}x{}@{},{}", bbox.w, bbox.h, bbox.x, bbox.y)
                        ),
                        _ => println!("{:>9}  {:>9}  {:>9}  {:>14}", "o-r", "-", "-", "-"),
                    }
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

/// Find the bounding box of the largest connected blob whose colour falls in
/// the given hue range (degrees) with sufficient saturation and value.
fn largest_blob(
    color: &ColorFrame,
    hue_min: f32,
    hue_max: f32,
    min_component: u32,
) -> Option<BoundingBox> {
    let w = color.width();
    let h = color.height();
    let gw = w / STEP;
    let gh = h / STEP;
    if gw == 0 || gh == 0 {
        return None;
    }

    let mut visited = vec![false; (gw * gh) as usize];
    let mut best: Option<(u32, BoundingBox)> = None;

    for gy in 0..gh {
        for gx in 0..gw {
            let i = (gy * gw + gx) as usize;
            if visited[i] {
                continue;
            }
            visited[i] = true;
            if !is_target(color, gx * STEP, gy * STEP, hue_min, hue_max) {
                continue;
            }

            // Flood fill this component.
            let mut stack = vec![(gx, gy)];
            let (mut minx, mut maxx, mut miny, mut maxy) = (gx, gx, gy, gy);
            let mut area = 0u32;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                minx = minx.min(cx);
                maxx = maxx.max(cx);
                miny = miny.min(cy);
                maxy = maxy.max(cy);
                for (nx, ny) in [(cx + 1, cy), (cx.wrapping_sub(1), cy), (cx, cy + 1), (cx, cy.wrapping_sub(1))]
                {
                    if nx >= gw || ny >= gh {
                        continue;
                    }
                    let ni = (ny * gw + nx) as usize;
                    if visited[ni] {
                        continue;
                    }
                    visited[ni] = true;
                    if is_target(color, nx * STEP, ny * STEP, hue_min, hue_max) {
                        stack.push((nx, ny));
                    }
                }
            }

            if area >= min_component {
                let bbox = BoundingBox::new(
                    minx * STEP,
                    miny * STEP,
                    (maxx - minx + 1) * STEP,
                    (maxy - miny + 1) * STEP,
                );
                if best.as_ref().is_none_or(|(a, _)| area > *a) {
                    best = Some((area, bbox));
                }
            }
        }
    }

    best.map(|(_, b)| b)
}

/// True if the pixel at `(x, y)` is within the target hue range.
fn is_target(color: &ColorFrame, x: u32, y: u32, hue_min: f32, hue_max: f32) -> bool {
    let Some((r, g, b)) = color.pixel_rgb(x, y) else {
        return false;
    };
    let (h, s, v) = rgb_to_hsv(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    h >= hue_min && h <= hue_max && s >= SAT_MIN && v >= VAL_MIN
}

/// Convert RGB (each in 0..1) to HSV: hue in degrees (0..360), sat/val in 0..1.
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;

    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max <= 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// Sliding-window decision + moving average (same as `object_distance`).
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
