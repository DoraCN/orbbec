//! Safe, idiomatic Rust bindings for the Orbbec Gemini 335 depth camera.
//!
//! This crate wraps the official OrbbecSDK v2 (C API) via the `orbbec-sys`
//! crate. It links against the system-installed `libOrbbecSDK.so`.
//!
//! Make sure the Orbbec SDK v2 is installed first, see `docs/install-sdk.md`.
//!
//! ```
//! use orbbec::Context;
//!
//! let ctx = Context::new().expect("failed to create Orbbec context");
//! ```

pub mod align;
pub mod camera;
pub mod context;
pub mod device;
pub mod error;
pub mod filter;
pub mod frame;
pub mod pipeline;
pub mod pointcloud;
pub mod stream;

pub use align::AlignFilter;
pub use camera::{CameraParam, Distortion, Extrinsic, Intrinsic};
pub use context::Context;
pub use device::{Device, DeviceInfo, SensorType};
pub use error::Error;
pub use filter::Filter;
pub use frame::{ColorFrame, DepthFrame};
pub use pipeline::{AlignMode, Config, Frame, FrameType, Frameset, Pipeline, StreamType};
pub use pointcloud::{
    ColorPoint, DecimationFilter, PointCloud, PointCloudFilter, PointFormat, ThresholdFilter,
};
pub use stream::{StreamProfile, StreamProfileList};
