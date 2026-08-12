//! Real-time distance measurement: place a sheet of paper (or any large flat
//! surface) in front of the camera and this example continuously prints its
//! distance in centimetres.
//!
//! Two measurement bases, selected with `--center`:
//!
//! * **default (histogram mode)**: builds a depth histogram over the whole
//!   frame and takes the most common depth — the dominant surface (the paper)
//!   wherever it appears in the image.
//! * **`--center`**: measures a small patch at the centre of the image (median
//!   of a 21×21 patch), e.g. for grabbing the object in the middle.
//!
//! The Gemini 335 depth range is **0.1 m .. 20 m** (best 0.26 m .. 3 m).
//! Objects closer than the 10 cm minimum produce no valid depth; the example
//! then reports `out-of-range` instead of a bogus reading.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example distance          # dominant surface
//! cargo run --release --example distance --center  # centre of image
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
/// Minimum fraction of the centre patch carrying valid depth before the centre
/// reading is trusted. Below-min objects fill the patch with scattered garbage,
/// which keeps support low.
const CENTER_MIN_SUPPORT: f32 = 0.5;
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

fn main() {
    let center_mode = std::env::args().any(|a| a == "--center");

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

    if center_mode {
        println!("mode: centre patch ({}x{} @ image centre)", CENTER_RADIUS * 2 + 1, CENTER_RADIUS * 2 + 1);
    } else {
        println!("mode: dominant surface (whole-frame depth histogram mode)");
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

                let measurement = if center_mode {
                    center_distance(&depth)
                } else {
                    dominant_distance(&depth, n_bins)
                };
                frames_seen += 1;

                // Raw reading (metres), if the frame itself looks plausible.
                let raw_m = measurement.map(|(m, support)| {
                    // Reject the centre reading unless the patch is mostly one
                    // surface (below-min garbage is scattered, hence low support).
                    if center_mode && support < CENTER_MIN_SUPPORT {
                        None
                    } else {
                        Some(m)
                    }
                });
                let confirmed_m = window.update(raw_m.flatten());

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

/// Distance at the centre of the image: median depth of a small central patch.
///
/// Returns `Some((metres, support))` where support is the fraction of valid
/// samples in the patch, or `None` if the patch has no valid depth.
fn center_distance(depth: &DepthFrame) -> Option<(f32, f32)> {
    let cx = depth.width() / 2;
    let cy = depth.height() / 2;
    let r = CENTER_RADIUS;

    let mut vals = vec![];
    let x_lo = cx.saturating_sub(r);
    let y_lo = cy.saturating_sub(r);
    for y in y_lo..(cy + r).min(depth.height()) {
        for x in x_lo..(cx + r).min(depth.width()) {
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
    let patch = ((r * 2 + 1) * (r * 2 + 1)) as f32;
    let support = vals.len() as f32 / patch.min(
        ((cy + r).min(depth.height()) - y_lo) as f32 * ((cx + r).min(depth.width()) - x_lo) as f32,
    );
    vals.sort_unstable();
    Some((vals[vals.len() / 2] as f32 / 1000.0, support))
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
