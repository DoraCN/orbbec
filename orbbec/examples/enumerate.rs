//! Enumerate connected Orbbec devices and print their info.
//!
//! Run from the repo root with the SDK env vars set (see docs/install-sdk.md §6):
//!
//! ```text
//! export OB_SDK_ROOT=/opt/OrbbecSDK
//! export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
//! cargo run --release --example enumerate
//! ```

use orbbec::Context;

fn main() {
    let ctx = Context::new().expect("failed to create Orbbec context");

    let devices = ctx.query_devices().expect("failed to enumerate devices");
    println!("found {} device(s)\n", devices.len());

    for (i, d) in devices.iter().enumerate() {
        println!("[{i}] name:            {}", d.name);
        println!("    vid:pid:         {:04X}:{:04X}", d.vid, d.pid);
        println!("    serial:          {}", d.serial_number);
        println!("    uid:             {}", d.uid);
        println!("    firmware:        {}", d.firmware_version);
        println!("    connection:      {}", d.connection_type);
        println!("    ip:              {}", d.ip_address);
        println!();
    }

    if let Some(first) = devices.first() {
        let device = ctx.open_device(0).expect("failed to open device");
        let info = device.info().expect("failed to read device info");
        assert_eq!(info.serial_number, first.serial_number);
        println!("opened device 0 OK: {}", info.name);
    }
}
