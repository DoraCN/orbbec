//! Real-time distance measurement: place a sheet of paper (or any large flat
//! surface) in front of the camera and this example continuously prints its
//! distance in centimetres.
//!
//! Three measurement bases:
//!
//! * **default**: whole-frame depth histogram mode — the dominant surface,
//!   wherever it appears.
//! * **`--center`**: a small patch at the centre of the image.
//! * **`--rect=<l>,<t>,<r>,<b>`**: an arbitrary rectangle given as fractions
//!   (0.0..1.0) of the frame, e.g. `--rect=0.1,0.2,0.5,0.6` measures the box
//!   from 10% left / 20% top to 50% / 60%. Great for e.g. the bottom-right
//!   corner: `--rect=0.7,0.7,1.0,1.0`.
//!
//! The region distance is the median of valid depth pixels inside the box,
//! decided by a sliding-window consistency check. Note that depth cameras have
//! a model-dependent minimum working range (e.g. the Gemini 335: 0.1–20 m,
//! best 0.26–3 m); closer objects produce no reliable depth and are reported
//! as `out-of-range`.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example distance                  # dominant surface
//! cargo run --release --example distance -- --center      # centre of image
//! cargo run --release --example distance -- --rect=0.7,0.7,1.0,1.0   # bottom-right
//! ```

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::{Context, DepthFrame};

/// Histogram bin width in millimetres.
const BIN_MM: u16 = 10;
/// Depth values below this (10 cm, the sensor minimum) are treated as invalid —
/// the SDK emits garbage instead of 0 for objects too close to measure.
const MIN_MM: u16 = 100;
/// Depth range considered (up to 8 m).
const MAX_MM: u16 = 8000;
/// Minimum fraction of pixels supporting the mode, otherwise report no paper.
const MIN_SUPPORT: f32 = 0.04;
/// Below this fraction of valid depth pixels the scene is considered
/// out-of-range (e.g. object closer than the 10 cm minimum).
const MIN_VALID_RATIO: f32 = 0.01;
/// Half-size of the central patch used in `--center` mode (patch = 2*r+1).
const CENTER_RADIUS: u32 = 10;
/// Minimum fraction of the region carrying valid depth before a region reading
/// is trusted. Below-min objects fill the region with scattered garbage, which
/// keeps support low.
const REGION_MIN_SUPPORT: f32 = 0.3;
/// Minimum number of valid pixels a region must contain.
const MIN_REGION_PIXELS: usize = 20;
/// Smoothing factor for the moving average (0..1, higher = more responsive).
const SMOOTHING: f32 = 0.3;
/// Sliding window used to decide a measurement from a batch of frames.
const WINDOW_LEN: usize = 7;
/// Minimum number of valid readings required inside the window.
const WINDOW_MIN_VALID: usize = 4;
/// Minimum number of readings that must lie within tolerance of the window
/// median for the measurement to be trusted.
const WINDOW_MIN_CONSISTENT: usize = 3;
/// Tolerance (metres) around the median: readings outside this count as
/// inconsistent. Below-min garbage is scattered over a wide range, so few
/// readings fall inside the tolerance and the window is rejected.
const WINDOW_TOL_M: f32 = 0.10;

/// What part of the frame to measure.
enum Mode {
    /// Whole-frame depth histogram mode.
    Dominant,
    /// Small patch around the image centre.
    Center,
    /// Rectangle given as fractions (left, top, right, bottom) of the frame.
    Rect(f32, f32, f32, f32),
}

/// A rectangle in depth-frame pixel coordinates.
struct Region {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

fn main() {
    let mode = parse_mode();

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
    // Decides each measurement from a sliding window of raw readings.
    let mut window = WindowFilter::new();

    match mode {
        Mode::Dominant => println!("mode: dominant surface (whole-frame depth histogram mode)"),
        Mode::Center => println!(
            "mode: centre patch ({}x{} @ image centre)",
            CENTER_RADIUS * 2 + 1,
            CENTER_RADIUS * 2 + 1
        ),
        Mode::Rect(l, t, r, b) => println!(
            "mode: rect from left {:.0}% top {:.0}% to right {:.0}% bottom {:.0}%",
            l * 100.0,
            t * 100.0,
            r * 100.0,
            b * 100.0
        ),
    }
    println!("distance updated in real time (cm)\n");
    println!("{:>6}  {:>9}  {:>9}  {:>10}", "#", "dist(cm)", "avg(cm)", "support");

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

                let measurement = match mode {
                    Mode::Dominant => dominant_distance(&depth, n_bins),
                    Mode::Center => {
                        let region = Region {
                            x: depth.width() / 2 - CENTER_RADIUS,
                            y: depth.height() / 2 - CENTER_RADIUS,
                            w: CENTER_RADIUS * 2,
                            h: CENTER_RADIUS * 2,
                        };
                        region_distance(&depth, &region)
                    }
                    Mode::Rect(l, t, r, b) => {
                        let region = rect_from_fractions(&depth, l, t, r, b);
                        region_distance(&depth, &region)
                    }
                };
                frames_seen += 1;

                // Raw reading (metres), if the frame itself looks plausible.
                let raw_m = measurement.and_then(|(m, support)| {
                    // Reject the reading unless the region is mostly one surface
                    // (below-min garbage is scattered, hence low support).
                    if matches!(mode, Mode::Center | Mode::Rect(..))
                        && support < REGION_MIN_SUPPORT
                    {
                        None
                    } else {
                        Some(m)
                    }
                });
                let confirmed_m = window.update(raw_m);

                let Some(distance_m) = confirmed_m else {
                    // Not stable across frames: object too close (<10 cm) or
                    // moving / no signal.
                    smoothed = None;
                    if last_render.elapsed() >= Duration::from_millis(200) {
                        println!(
                            "{:>6}  {:>9}  {:>9}  {:>10}",
                            frames_seen, "out-of-range", "-", "-"
                        );
                        last_render = Instant::now();
                    }
                    continue;
                };

                let support = measurement.map(|(_, s)| s).unwrap_or(0.0);
                // Exponential moving average for a stable readout (in cm).
                let distance_cm = distance_m * 100.0;
                smoothed = Some(match smoothed {
                    Some(prev) => prev + SMOOTHING * (distance_cm - prev),
                    None => distance_cm,
                });

                let avg = smoothed.unwrap();
                // Refresh the console line a few times per second.
                if last_render.elapsed() >= Duration::from_millis(200) {
                    println!(
                        "{:>6}  {:>8.1}  {:>8.1}  {:>9.1}%",
                        frames_seen,
                        distance_cm,
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
///
/// Returns `None` when too few pixels carry valid depth (object closer than the
/// 10 cm minimum, or no signal at all).
fn dominant_distance(depth: &DepthFrame, n_bins: usize) -> Option<(f32, f32)> {
    let pixels = depth.pixels();
    let total = pixels.len();
    let mut hist = vec![0u32; n_bins];
    let mut valid = 0u32;

    for v in pixels {
        // Exclude invalid (0) and values below the 10 cm minimum (the SDK emits
        // garbage for objects too close to measure), plus out-of-range values.
        if (MIN_MM..MAX_MM).contains(&v) {
            hist[(v / BIN_MM) as usize] += 1;
            valid += 1;
        }
    }

    // Almost nothing has valid depth -> too close / out of range.
    let valid_ratio = valid as f32 / total as f32;
    if valid_ratio < MIN_VALID_RATIO {
        return None;
    }

    let (best_bin, best_count) = hist
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .map(|(b, c)| (b, *c))
        .unwrap_or((0, 0));

    let support = best_count as f32 / valid as f32;

    if support < MIN_SUPPORT {
        // Nothing large and flat dominates: fall back to the median of the
        // central region so the output still tracks *something* in front.
        let cx = depth.width() / 2;
        let cy = depth.height() / 2;
        let mut vals = vec![];
        for y in (cy.saturating_sub(40))..(cy + 40).min(depth.height()) {
            for x in (cx.saturating_sub(40))..(cx + 40).min(depth.width()) {
                if let Some(v) = depth.pixel(x, y) {
                    if (MIN_MM..MAX_MM).contains(&v) {
                        vals.push(v);
                    }
                }
            }
        }
        if vals.is_empty() {
            return None;
        }
        vals.sort_unstable();
        return Some((vals[vals.len() / 2] as f32 / 1000.0, support));
    }

    Some((best_bin as f32 * BIN_MM as f32 / 1000.0, support))
}

/// Distance of a rectangular region: median depth of valid pixels inside it.
///
/// Returns `Some((metres, support))` where support is the fraction of valid
/// samples in the region, or `None` if the region is empty / has too few valid
/// pixels.
fn region_distance(depth: &DepthFrame, region: &Region) -> Option<(f32, f32)> {
    let x_hi = (region.x + region.w).min(depth.width());
    let y_hi = (region.y + region.h).min(depth.height());
    if region.x >= x_hi || region.y >= y_hi {
        return None;
    }

    let mut vals = vec![];
    for y in region.y..y_hi {
        for x in region.x..x_hi {
            if let Some(v) = depth.pixel(x, y) {
                if (MIN_MM..MAX_MM).contains(&v) {
                    vals.push(v);
                }
            }
        }
    }

    if vals.len() < MIN_REGION_PIXELS {
        return None;
    }
    let total = (x_hi - region.x) as f32 * (y_hi - region.y) as f32;
    let support = vals.len() as f32 / total;
    vals.sort_unstable();
    Some((vals[vals.len() / 2] as f32 / 1000.0, support))
}

/// Convert fractional bounds (0..1 of the frame) into a pixel region.
fn rect_from_fractions(depth: &DepthFrame, l: f32, t: f32, r: f32, b: f32) -> Region {
    let w = depth.width() as f32;
    let h = depth.height() as f32;
    let l = (l.clamp(0.0, 1.0) * w) as u32;
    let t = (t.clamp(0.0, 1.0) * h) as u32;
    let r = (r.clamp(0.0, 1.0) * w) as u32;
    let b = (b.clamp(0.0, 1.0) * h) as u32;
    Region {
        x: l,
        y: t,
        w: r.saturating_sub(l),
        h: b.saturating_sub(t),
    }
}

/// Parse the measurement mode from `--center` / `--rect=l,t,r,b` arguments.
fn parse_mode() -> Mode {
    let args: Vec<String> = std::env::args().collect();
    for arg in args.iter().skip(1) {
        if arg == "--center" {
            return Mode::Center;
        }
        if let Some(v) = arg.strip_prefix("--rect=") {
            let parts: Vec<f32> = v
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if parts.len() == 4 {
                let (l, t, r, b) = (parts[0], parts[1], parts[2], parts[3]);
                if l < r && t < b && r <= 1.0 && b <= 1.0 && l >= 0.0 && t >= 0.0 {
                    return Mode::Rect(l, t, r, b);
                }
            }
            eprintln!("invalid --rect value, expected --rect=left,top,right,bottom (0..1 fractions)");
            std::process::exit(2);
        }
    }
    Mode::Dominant
}

/// Decides each measurement from a sliding window of raw readings.
///
/// The window keeps the last [`WINDOW_LEN`] raw readings and reports the
/// **median** (robust against single-frame spikes, so the readout does not
/// jump). A measurement is only trusted when at least
/// [`WINDOW_MIN_CONSISTENT`] readings lie within [`WINDOW_TOL_M`] of that
/// median. Below-minimum objects produce depth garbage scattered over the whole
/// range: the median has few neighbours inside the tolerance, so the window is
/// rejected and the readout stays "out-of-range". A moving surface shifts by a
/// few cm per frame, which stays well inside the tolerance.
struct WindowFilter {
    window: VecDeque<Option<f32>>, // raw measurements in metres
}

impl WindowFilter {
    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(WINDOW_LEN),
        }
    }

    /// Feed one raw reading; returns the window's decision in metres.
    fn update(&mut self, m: Option<f32>) -> Option<f32> {
        self.window.push_back(m);
        while self.window.len() > WINDOW_LEN {
            self.window.pop_front();
        }

        let valid: Vec<f32> = self.window.iter().filter_map(|v| *v).collect();
        if valid.len() < WINDOW_MIN_VALID {
            return None;
        }

        let mut sorted = valid.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted[sorted.len() / 2];

        let consistent = valid
            .iter()
            .filter(|&&v| (v - median).abs() <= WINDOW_TOL_M)
            .count();
        if consistent >= WINDOW_MIN_CONSISTENT {
            Some(median)
        } else {
            None
        }
    }
}
