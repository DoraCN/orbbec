# Orbbec Rust SDK

Safe, idiomatic Rust bindings for [Orbbec depth cameras](https://www.orbbec.com/),
built on the official **OrbbecSDK v2** C API via FFI. It works with any
OrbbecSDK-v2-compatible device (Gemini, Femto, Astra series, ...); a **Gemini 335**
is used for development and testing.

The crate gives you a type-safe, memory-safe interface to capture synchronized
RGB + depth streams, align them, read camera intrinsics, generate point clouds
and measure distances — without writing any C/C++ or `unsafe` in application
code.

## Features

- **Device management** — enumerate, inspect and open devices; list sensors.
- **Frame capture** — synchronized RGB + depth + IR streams over a Rust channel,
  with hardware timestamps.
- **D2C alignment** — align depth to the color frame (hardware or software) via
  a dedicated `AlignFilter`.
- **Camera model** — intrinsics, distortion and depth↔RGB extrinsics with
  pixel-to-3D unprojection.
- **Point clouds** — RGB point cloud generation and distance-range outlier
  filtering.
- **Distance measurement** — robust region / detection-box distance in real
  time (used by the YOLO- and color-block examples).
- **Stream profiles** — query supported resolutions/formats and enable a
  specific profile.

## Architecture

A two-crate Cargo workspace:

```
orbbec/
├── Cargo.toml                 # workspace
├── orbbec-sys/                # low-level: bindgen-generated FFI bindings
│   ├── build.rs               # locates the installed SDK, links libOrbbecSDK
│   ├── wrapper.h              # #include <libobsensor/ObSensor.h>
│   └── src/lib.rs             # generated bindings
└── orbbec/                    # high-level: safe, idiomatic Rust API
    ├── src/
    │   ├── context.rs         # SDK context, device enumeration/opening
    │   ├── device.rs          # device info, sensor list
    │   ├── pipeline.rs        # capture pipeline, config, frames
    │   ├── align.rs           # D2C alignment filter
    │   ├── camera.rs          # intrinsics, distortion, extrinsics
    │   ├── pointcloud.rs      # point cloud generation & filtering
    │   ├── frame.rs           # typed DepthFrame / ColorFrame
    │   ├── stream.rs          # stream profile querying & matching
    │   ├── filter.rs          # generic SDK filter wrapper
    │   └── error.rs           # error type + FFI error handling
    ├── examples/              # runnable demos (see below)
    └── tests/camera.rs        # hardware-gated integration tests
```

`orbbec-sys` is generated with `bindgen` at build time and links the
system-installed `libOrbbecSDK.so`. `orbbec` wraps every raw pointer in an
RAII type and converts SDK `ob_error**` out-parameters into a typed [`Error`].

## Requirements

- **Ubuntu 22.04+ x86_64** (other Linux should work)
- **OrbbecSDK v2** installed — follow [`docs/install-sdk.md`](docs/install-sdk.md)
  (system packages, source build, udev rules, environment variables)
- An OrbbecSDK-v2-compatible Orbbec depth camera connected on a **USB 3.0** port
  (developed/tested on a Gemini 335)
- `clang` + `libclang-dev` (bindgen), `cmake`, C/C++ toolchain

## Installation

### 1. Install the Orbbec SDK

```bash
# see docs/install-sdk.md for full details (udev rules, etc.)
sudo apt install -y build-essential git cmake pkg-config \
  libusb-1.0-0-dev libgoogle-glog-dev libopencv-dev \
  libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev libglew-dev \
  clang libclang-dev
```

### 2. Set up the environment

```bash
export OB_SDK_ROOT=/opt/OrbbecSDK        # where the SDK is installed
export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
```

### 3. Build

```bash
cargo build --release
```

## Quick start

```rust,no_run
use orbbec::pipeline::{Config, FrameType, Pipeline, StreamType};
use orbbec::Context;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = Context::new()?;

    // 1. Make sure a camera is connected.
    let devices = ctx.query_devices()?;
    if devices.is_empty() {
        panic!("no Orbbec device connected");
    }
    println!("found {}", devices[0].name);

    // 2. Configure depth + color.
    let mut config = Config::new()?;
    config.enable_stream(StreamType::Depth)?;
    config.enable_stream(StreamType::Color)?;

    // 3. Start the pipeline and receive framesets over a channel.
    let mut pipeline = Pipeline::new()?;
    pipeline.enable_frame_sync()?;
    let frames = pipeline.start_capture(Some(&config))?;

    // 4. Read one frameset and inspect the depth frame.
    let frameset = frames.recv_timeout(std::time::Duration::from_secs(2))?;
    if let Some(depth) = frameset.frame(FrameType::Depth) {
        println!(
            "depth {}x{} bytes={}",
            depth.width(),
            depth.height(),
            depth.data_size()
        );
    }

    pipeline.stop()?;
    Ok(())
}
```

## Examples

Run any example from the repo root with the environment set:

| Example | What it does |
|---|---|
| [`enumerate`](orbbec/examples/enumerate.rs) | Enumerate devices and print info |
| [`frames`](orbbec/examples/frames.rs) | Capture synchronized RGB + depth frames |
| [`aligned`](orbbec/examples/aligned.rs) | D2C align depth to color, read intrinsics, pixel→3D |
| [`pointcloud`](orbbec/examples/pointcloud.rs) | Generate an RGB point cloud, filter by range |
| [`streams`](orbbec/examples/streams.rs) | Query stream profiles, match and enable one |
| [`distance`](orbbec/examples/distance.rs) | Measure distance of a region / the whole frame (`--center`, `--rect=`) |
| [`object_distance`](orbbec/examples/object_distance.rs) | Measure distance of YOLO-style detection boxes |
| [`color_block`](orbbec/examples/color_block.rs) | Track a colored block and measure its distance |

```bash
cargo run --release --example frames
cargo run --release --example aligned
cargo run --release --example pointcloud
cargo run --release --example distance -- --center
cargo run --release --example object_distance -- --box=100,80,300,220
cargo run --release --example color_block            # green block
```

## Testing

Unit tests and doc-tests always run. Integration tests exercise the real
camera and are gated behind `ORBBEC_TEST=1` so the suite passes on machines
without hardware:

```bash
cargo test --release                                  # unit + doc tests
export ORBBEC_TEST=1
cargo test -p orbbec --release --test camera          # hardware integration tests
```

The 10 integration tests cover context creation, enumeration, opening devices,
sensor lists, synchronized frame capture, camera intrinsics, D2C alignment,
point cloud generation, typed depth frames and stream-profile matching.

## Documentation

- [`docs/install-sdk.md`](docs/install-sdk.md) — installing the Orbbec SDK
  (source or prebuilt), udev rules, verification and the build environment
- [`docs/gemini335_rust_plan.md`](docs/gemini335_rust_plan.md) — an internal
  integration plan written for the Gemini 335 test device
- Crate docs: `cargo doc --open`

## Notes & limitations

- Depth cameras have a **minimum working range** that varies by model (e.g. the
  Gemini 335: 0.1–20 m, best 0.26–3 m); objects closer produce no reliable
  depth (the SDK emits garbage values).
- Default color profile is MJPG; use an uncompressed profile (e.g.
  `1280x720@30` RGB) when you need per-pixel color access.
- The binding targets the installed SDK ABI. Re-run `cargo build` after an SDK
  upgrade so `orbbec-sys` regenerates the bindings.

## License

MIT
