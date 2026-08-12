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

pub mod context;
pub mod error;
pub mod pipeline;

pub use context::Context;
pub use error::Error;
