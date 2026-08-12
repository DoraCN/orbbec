//! Query supported stream profiles for each sensor and enable a specific
//! resolution by matching a profile.
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example streams
//! ```

use orbbec::pipeline::{Config, Pipeline, StreamType};
use orbbec::Context;

fn main() {
    let ctx = Context::new().expect("failed to create context");
    let devices = ctx.query_devices().expect("failed to enumerate devices");
    assert!(!devices.is_empty(), "no Orbbec device connected");

    let pipeline = Pipeline::new().expect("failed to create pipeline");

    // 1. List sensors on the device.
    let device = ctx.open_device(0).expect("failed to open device");
    println!("sensors: {:?}", device.sensors().expect("failed to list sensors"));

    // 2. Print every depth / color profile.
    for sensor in [StreamType::Depth, StreamType::Color] {
        let list = pipeline
            .stream_profiles(sensor)
            .expect("failed to get stream profiles");
        println!("\n{} supports {} profiles:", sensor, list.count());
        let mut seen = 0u32;
        for p in list.collect() {
            println!(
                "  {}x{}@{} fmt={}",
                p.width(),
                p.height(),
                p.fps(),
                p.format()
            );
            seen += 1;
            if seen >= 8 {
                println!("  ...");
                break;
            }
        }
    }

    // 3. Match a specific 640x400@30 depth profile and enable it.
    let depth_list = pipeline
        .stream_profiles(StreamType::Depth)
        .expect("depth profiles");
    let matched = depth_list
        .match_video(Some(640), Some(400), None, Some(30))
        .expect("failed to match profile")
        .expect("no 640x400@30 depth profile");

    let mut config = Config::new().expect("failed to create config");
    config
        .enable_stream_with_profile(&matched)
        .expect("failed to enable matched profile");
    println!("\nmatched & enabled: {}x{}@{} fmt={}", matched.width(), matched.height(), matched.fps(), matched.format());
}
