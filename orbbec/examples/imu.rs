//! Stream the camera's built-in IMU (accelerometer + gyroscope) and print the
//! samples in real time. Acceleration is in m/s², angular rate in deg/s.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example imu
//! ```

use std::time::{Duration, Instant};

use orbbec::pipeline::{Config, FrameType, Pipeline};
use orbbec::Context;

fn main() {
    let ctx = Context::new().expect("failed to create context");
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "no Orbbec device connected");

    let mut config = Config::new().expect("failed to create pipeline config");
    // Accelerometer: ±4 g @ 100 Hz (the supported ranges/rates vary by device).
    config
        .enable_accel_stream(
            orbbec_sys::OBAccelFullScaleRange_OB_ACCEL_FS_4g,
            orbbec_sys::OBIMUSampleRate_OB_SAMPLE_RATE_100_HZ,
        )
        .expect("failed to enable accelerometer");
    // Gyroscope: ±1000 deg/s @ 200 Hz.
    config
        .enable_gyro_stream(
            orbbec_sys::OBGyroFullScaleRange_OB_GYRO_FS_1000dps,
            orbbec_sys::OBIMUSampleRate_OB_SAMPLE_RATE_200_HZ,
        )
        .expect("failed to enable gyroscope");

    let mut pipeline = Pipeline::new().expect("failed to create pipeline");
    let frames = pipeline
        .start_capture(Some(&config))
        .expect("failed to start pipeline");

    println!("accel: ±4g @ 100Hz (m/s²)  |  gyro: ±1000dps @ 200Hz (deg/s)\n");
    println!("{:>10}  {:>28}  {:>28}", "#", "accel x/y/z", "gyro  x/y/z");

    let mut last_render = Instant::now();
    let mut count = 0u32;
    loop {
        match frames.recv_timeout(Duration::from_millis(1000)) {
            Ok(frameset) => {
                let accel = frameset.frame(FrameType::Accel);
                let gyro = frameset.frame(FrameType::Gyro);

                let accel_str = accel
                    .as_ref()
                    .and_then(|f| f.imu_values().last().copied())
                    .map(|v| format!("{:+.3}  {:+.3}  {:+.3}", v[0], v[1], v[2]))
                    .unwrap_or_else(|| "  -   -   -  ".to_string());
                let gyro_str = gyro
                    .as_ref()
                    .and_then(|f| f.imu_values().last().copied())
                    .map(|v| format!("{:+.3}  {:+.3}  {:+.3}", v[0], v[1], v[2]))
                    .unwrap_or_else(|| "  -   -   -  ".to_string());

                if last_render.elapsed() >= Duration::from_millis(100) {
                    println!("{count:>10}  {accel_str}  {gyro_str}");
                    last_render = Instant::now();
                }
                count += 1;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("timed out waiting for IMU frame");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    pipeline.stop().expect("failed to stop pipeline");
}
