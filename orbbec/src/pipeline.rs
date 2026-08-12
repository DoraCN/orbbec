//! Frame pipeline: stream configuration, capture, RGB/depth synchronization.
//!
//! TODO(implement after OrbbecSDK is installed and bindings are generated):
//!
//! - `Pipeline::new(&Context)` wrapping `ob_create_pipeline`
//! - `Pipeline::start` / `Pipeline::stop`
//! - Per-sensor frame callbacks (`ob_set_frame_ready_callback`) that forward
//!   frames over a `crossbeam_channel`, so callbacks never block.
//! - Stream config: `OB_SENSOR_COLOR` (1920x1080@30) and `OB_SENSOR_DEPTH`
//!   (1280x800@30), frame sync enabled.
//! - D2C alignment (`OB_AlignMode`), camera intrinsics (`ob_camera_param`),
//!   point cloud generation, hardware timestamps.
