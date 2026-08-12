//! Frame pipeline: stream configuration, capture, RGB/depth synchronization.
//!
//! This module wraps the SDK `ob_pipeline` API. A [`Pipeline`] is created from
//! a [`Context`], configured with the desired streams, started, and then yields
//! [`Frameset`]s containing per-sensor [`Frame`]s.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use orbbec_sys::{ob_config, ob_frame, ob_pipeline, ob_stream_profile, OBFrameType, OBStreamType};

use crate::camera::CameraParam;
use crate::error::{check_error, Error};

/// Process-global map of active frameset callbacks, keyed by the owning
/// pipeline's raw pointer value.
///
/// The C callback only receives a raw `user_data` pointer; storing the actual
/// Rust closures here (instead of handing raw pointers to the SDK) keeps them
/// alive safely and lets us remove them on stop without any use-after-free.
type CallbackMap = HashMap<usize, Box<dyn FnMut(Frameset) + Send>>;

static CALLBACKS: OnceLock<Mutex<CallbackMap>> = OnceLock::new();

fn callbacks() -> &'static Mutex<CallbackMap> {
    CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn frameset_c_callback(frameset: *mut ob_frame, user_data: *mut std::os::raw::c_void) {
    let key = user_data as usize;
    let Ok(mut map) = callbacks().lock() else {
        return;
    };
    // SAFETY: `frameset` is a valid SDK frameset reference owned by this
    // invocation; the registry holds the callback for the lifetime of the
    // pipeline, so the pointer stays valid while we are in here.
    if let (Some(cb), Some(fs)) = (map.get_mut(&key), unsafe { Frameset::from_raw(frameset) }) {
        cb(fs);
    }
}

/// Depth-to-color / color-to-depth alignment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    /// No alignment.
    Disable,
    /// Hardware depth→color alignment (on-device ASIC).
    D2cHardware,
    /// Software depth→color alignment.
    D2cSoftware,
    /// Software color→depth alignment.
    C2dSoftware,
}

impl AlignMode {
    pub fn from_raw(v: u32) -> Self {
        match v {
            orbbec_sys::OBAlignMode_ALIGN_D2C_HW_MODE => AlignMode::D2cHardware,
            orbbec_sys::OBAlignMode_ALIGN_D2C_SW_MODE => AlignMode::D2cSoftware,
            orbbec_sys::OBAlignMode_ALIGN_C2D_SW_MODE => AlignMode::C2dSoftware,
            _ => AlignMode::Disable,
        }
    }

    pub fn as_raw(&self) -> u32 {
        match self {
            AlignMode::Disable => orbbec_sys::OBAlignMode_ALIGN_DISABLE,
            AlignMode::D2cHardware => orbbec_sys::OBAlignMode_ALIGN_D2C_HW_MODE,
            AlignMode::D2cSoftware => orbbec_sys::OBAlignMode_ALIGN_D2C_SW_MODE,
            AlignMode::C2dSoftware => orbbec_sys::OBAlignMode_ALIGN_C2D_SW_MODE,
        }
    }
}

/// A sensor stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Unknown,
    Video,
    Ir,
    Color,
    Depth,
    Accel,
    Gyro,
    IrLeft,
    IrRight,
    RawPhase,
    Confidence,
    Lidar,
    ColorLeft,
    ColorRight,
}

impl StreamType {
    pub fn from_raw(v: i32) -> Self {
        match v {
            orbbec_sys::OBStreamType_OB_STREAM_VIDEO => StreamType::Video,
            orbbec_sys::OBStreamType_OB_STREAM_IR => StreamType::Ir,
            orbbec_sys::OBStreamType_OB_STREAM_COLOR => StreamType::Color,
            orbbec_sys::OBStreamType_OB_STREAM_DEPTH => StreamType::Depth,
            orbbec_sys::OBStreamType_OB_STREAM_ACCEL => StreamType::Accel,
            orbbec_sys::OBStreamType_OB_STREAM_GYRO => StreamType::Gyro,
            orbbec_sys::OBStreamType_OB_STREAM_IR_LEFT => StreamType::IrLeft,
            orbbec_sys::OBStreamType_OB_STREAM_IR_RIGHT => StreamType::IrRight,
            orbbec_sys::OBStreamType_OB_STREAM_RAW_PHASE => StreamType::RawPhase,
            orbbec_sys::OBStreamType_OB_STREAM_CONFIDENCE => StreamType::Confidence,
            orbbec_sys::OBStreamType_OB_STREAM_LIDAR => StreamType::Lidar,
            orbbec_sys::OBStreamType_OB_STREAM_COLOR_LEFT => StreamType::ColorLeft,
            orbbec_sys::OBStreamType_OB_STREAM_COLOR_RIGHT => StreamType::ColorRight,
            _ => StreamType::Unknown,
        }
    }

    pub fn as_raw(&self) -> OBStreamType {
        match self {
            StreamType::Unknown => orbbec_sys::OBStreamType_OB_STREAM_UNKNOWN,
            StreamType::Video => orbbec_sys::OBStreamType_OB_STREAM_VIDEO,
            StreamType::Ir => orbbec_sys::OBStreamType_OB_STREAM_IR,
            StreamType::Color => orbbec_sys::OBStreamType_OB_STREAM_COLOR,
            StreamType::Depth => orbbec_sys::OBStreamType_OB_STREAM_DEPTH,
            StreamType::Accel => orbbec_sys::OBStreamType_OB_STREAM_ACCEL,
            StreamType::Gyro => orbbec_sys::OBStreamType_OB_STREAM_GYRO,
            StreamType::IrLeft => orbbec_sys::OBStreamType_OB_STREAM_IR_LEFT,
            StreamType::IrRight => orbbec_sys::OBStreamType_OB_STREAM_IR_RIGHT,
            StreamType::RawPhase => orbbec_sys::OBStreamType_OB_STREAM_RAW_PHASE,
            StreamType::Confidence => orbbec_sys::OBStreamType_OB_STREAM_CONFIDENCE,
            StreamType::Lidar => orbbec_sys::OBStreamType_OB_STREAM_LIDAR,
            StreamType::ColorLeft => orbbec_sys::OBStreamType_OB_STREAM_COLOR_LEFT,
            StreamType::ColorRight => orbbec_sys::OBStreamType_OB_STREAM_COLOR_RIGHT,
        }
    }

    /// The frame type produced by this stream, for frameset lookups.
    pub fn as_frame_type(&self) -> FrameType {
        match self {
            StreamType::Unknown => FrameType::Unknown,
            StreamType::Video => FrameType::Video,
            StreamType::Ir => FrameType::Ir,
            StreamType::Color => FrameType::Color,
            StreamType::Depth => FrameType::Depth,
            StreamType::Accel => FrameType::Accel,
            StreamType::Gyro => FrameType::Gyro,
            StreamType::IrLeft => FrameType::IrLeft,
            StreamType::IrRight => FrameType::IrRight,
            StreamType::RawPhase => FrameType::RawPhase,
            StreamType::Confidence => FrameType::Confidence,
            StreamType::Lidar => FrameType::LidarPoints,
            StreamType::ColorLeft => FrameType::ColorLeft,
            StreamType::ColorRight => FrameType::ColorRight,
        }
    }
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StreamType::Unknown => "unknown",
            StreamType::Video => "video",
            StreamType::Ir => "ir",
            StreamType::Color => "color",
            StreamType::Depth => "depth",
            StreamType::Accel => "accel",
            StreamType::Gyro => "gyro",
            StreamType::IrLeft => "ir-left",
            StreamType::IrRight => "ir-right",
            StreamType::RawPhase => "raw-phase",
            StreamType::Confidence => "confidence",
            StreamType::Lidar => "lidar",
            StreamType::ColorLeft => "color-left",
            StreamType::ColorRight => "color-right",
        })
    }
}

/// A frame type as reported by [`Frame::frame_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Unknown,
    Video,
    Ir,
    Color,
    Depth,
    Accel,
    FrameSet,
    Points,
    Gyro,
    IrLeft,
    IrRight,
    RawPhase,
    Confidence,
    LidarPoints,
    ColorLeft,
    ColorRight,
}

impl FrameType {
    pub fn from_raw(v: i32) -> Self {
        match v {
            orbbec_sys::OBFrameType_OB_FRAME_VIDEO => FrameType::Video,
            orbbec_sys::OBFrameType_OB_FRAME_IR => FrameType::Ir,
            orbbec_sys::OBFrameType_OB_FRAME_COLOR => FrameType::Color,
            orbbec_sys::OBFrameType_OB_FRAME_DEPTH => FrameType::Depth,
            orbbec_sys::OBFrameType_OB_FRAME_ACCEL => FrameType::Accel,
            orbbec_sys::OBFrameType_OB_FRAME_SET => FrameType::FrameSet,
            orbbec_sys::OBFrameType_OB_FRAME_POINTS => FrameType::Points,
            orbbec_sys::OBFrameType_OB_FRAME_GYRO => FrameType::Gyro,
            orbbec_sys::OBFrameType_OB_FRAME_IR_LEFT => FrameType::IrLeft,
            orbbec_sys::OBFrameType_OB_FRAME_IR_RIGHT => FrameType::IrRight,
            orbbec_sys::OBFrameType_OB_FRAME_RAW_PHASE => FrameType::RawPhase,
            orbbec_sys::OBFrameType_OB_FRAME_CONFIDENCE => FrameType::Confidence,
            orbbec_sys::OBFrameType_OB_FRAME_LIDAR_POINTS => FrameType::LidarPoints,
            orbbec_sys::OBFrameType_OB_FRAME_COLOR_LEFT => FrameType::ColorLeft,
            orbbec_sys::OBFrameType_OB_FRAME_COLOR_RIGHT => FrameType::ColorRight,
            _ => FrameType::Unknown,
        }
    }

    pub fn as_raw(&self) -> OBFrameType {
        match self {
            FrameType::Unknown => orbbec_sys::OBFrameType_OB_FRAME_UNKNOWN,
            FrameType::Video => orbbec_sys::OBFrameType_OB_FRAME_VIDEO,
            FrameType::Ir => orbbec_sys::OBFrameType_OB_FRAME_IR,
            FrameType::Color => orbbec_sys::OBFrameType_OB_FRAME_COLOR,
            FrameType::Depth => orbbec_sys::OBFrameType_OB_FRAME_DEPTH,
            FrameType::Accel => orbbec_sys::OBFrameType_OB_FRAME_ACCEL,
            FrameType::FrameSet => orbbec_sys::OBFrameType_OB_FRAME_SET,
            FrameType::Points => orbbec_sys::OBFrameType_OB_FRAME_POINTS,
            FrameType::Gyro => orbbec_sys::OBFrameType_OB_FRAME_GYRO,
            FrameType::IrLeft => orbbec_sys::OBFrameType_OB_FRAME_IR_LEFT,
            FrameType::IrRight => orbbec_sys::OBFrameType_OB_FRAME_IR_RIGHT,
            FrameType::RawPhase => orbbec_sys::OBFrameType_OB_FRAME_RAW_PHASE,
            FrameType::Confidence => orbbec_sys::OBFrameType_OB_FRAME_CONFIDENCE,
            FrameType::LidarPoints => orbbec_sys::OBFrameType_OB_FRAME_LIDAR_POINTS,
            FrameType::ColorLeft => orbbec_sys::OBFrameType_OB_FRAME_COLOR_LEFT,
            FrameType::ColorRight => orbbec_sys::OBFrameType_OB_FRAME_COLOR_RIGHT,
        }
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FrameType::Unknown => "unknown",
            FrameType::Video => "video",
            FrameType::Ir => "ir",
            FrameType::Color => "color",
            FrameType::Depth => "depth",
            FrameType::Accel => "accel",
            FrameType::FrameSet => "frameset",
            FrameType::Points => "points",
            FrameType::Gyro => "gyro",
            FrameType::IrLeft => "ir-left",
            FrameType::IrRight => "ir-right",
            FrameType::RawPhase => "raw-phase",
            FrameType::Confidence => "confidence",
            FrameType::LidarPoints => "lidar-points",
            FrameType::ColorLeft => "color-left",
            FrameType::ColorRight => "color-right",
        })
    }
}

/// A video frame buffer obtained from the SDK.
///
/// The frame owns a reference-counted SDK frame object; dropping it releases
/// the reference.
pub struct Frame {
    raw: *mut ob_frame,
}

// SAFETY: The frame is an opaque SDK object owned exclusively by this wrapper;
// the SDK delivers frames on internal threads but the reference count keeps the
// buffer alive while we hold it.
unsafe impl Send for Frame {}

impl Frame {
    /// Wrap a non-null SDK frame. Takes ownership.
    ///
    /// # Safety
    ///
    /// `raw` must be a valid `ob_frame*` whose reference is owned by the caller.
    pub(crate) unsafe fn from_raw(raw: *mut ob_frame) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self { raw })
        }
    }

    /// Raw pointer to the underlying SDK frame.
    pub fn as_raw(&self) -> *mut ob_frame {
        self.raw
    }

    /// Frame sequence index.
    pub fn index(&self) -> u64 {
        // SAFETY: `self.raw` is a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_frame_get_index(self.raw, e)).unwrap_or(0) }
    }

    /// Frame format (pixel format for video frames).
    pub fn format(&self) -> i32 {
        // SAFETY: `self.raw` is a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_frame_get_format(self.raw, e)).unwrap_or(0) }
    }

    /// Frame type.
    pub fn frame_type(&self) -> FrameType {
        // SAFETY: `self.raw` is a valid frame.
        let t = unsafe { check_error(|e| orbbec_sys::ob_frame_get_type(self.raw, e)).unwrap_or(0) };
        FrameType::from_raw(t)
    }

    /// Hardware timestamp in microseconds.
    pub fn timestamp_us(&self) -> u64 {
        // SAFETY: `self.raw` is a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_frame_get_timestamp_us(self.raw, e)).unwrap_or(0) }
    }

    /// Host system timestamp (CLOCK_MONOTONIC) in microseconds.
    pub fn system_timestamp_us(&self) -> u64 {
        // SAFETY: `self.raw` is a valid frame.
        unsafe {
            check_error(|e| orbbec_sys::ob_frame_get_system_timestamp_us(self.raw, e)).unwrap_or(0)
        }
    }

    /// Raw frame data (for video frames: pixel data).
    pub fn data(&self) -> &[u8] {
        let len = self.data_size();
        // SAFETY: the SDK guarantees the data buffer is valid for `data_size`
        // bytes while the frame reference is held.
        unsafe {
            let ptr = check_error(|e| orbbec_sys::ob_frame_get_data(self.raw, e)).unwrap_or(
                std::ptr::null_mut(),
            );
            if ptr.is_null() {
                &[]
            } else {
                std::slice::from_raw_parts(ptr, len as usize)
            }
        }
    }

    /// Size of [`Frame::data`] in bytes.
    pub fn data_size(&self) -> u32 {
        // SAFETY: `self.raw` is a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_frame_get_data_size(self.raw, e)).unwrap_or(0) }
    }

    /// Video frame width (0 for non-video frames).
    pub fn width(&self) -> u32 {
        self.video_profile()
            .map(|p| {
                // SAFETY: `p` is a valid video stream profile owned by the frame.
                unsafe { check_error(|e| orbbec_sys::ob_video_stream_profile_get_width(p, e)) }
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    /// Video frame height (0 for non-video frames).
    pub fn height(&self) -> u32 {
        self.video_profile()
            .map(|p| {
                // SAFETY: `p` is a valid video stream profile owned by the frame.
                unsafe { check_error(|e| orbbec_sys::ob_video_stream_profile_get_height(p, e)) }
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    /// The stream profile of this frame (owned by the frame; must not be freed).
    pub fn stream_profile(&self) -> Option<*mut ob_stream_profile> {
        self.video_profile()
    }

    fn video_profile(&self) -> Option<*mut ob_stream_profile> {
        // SAFETY: `self.raw` is a valid frame; the returned profile is owned by
        // the frame and must not be freed.
        unsafe {
            check_error(|e| orbbec_sys::ob_frame_get_stream_profile(self.raw, e))
                .ok()
                .filter(|p| !p.is_null())
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid frame whose reference we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_frame(self.raw, e)) };
    }
}

/// A collection of synchronized frames (depth + color + ...) from the pipeline.
pub struct Frameset {
    raw: *mut ob_frame,
}

unsafe impl Send for Frameset {}

impl Frameset {
    /// Wrap a non-null SDK frameset. Takes ownership.
    ///
    /// # Safety
    ///
    /// `raw` must be a valid `ob_frame*` frameset whose reference is owned by
    /// the caller.
    pub(crate) unsafe fn from_raw(raw: *mut ob_frame) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self { raw })
        }
    }

    /// Take ownership of a filter output that is actually a frameset.
    ///
    /// This is used for e.g. [`AlignFilter::process`](crate::align::AlignFilter::process),
    /// whose synchronous result is a frameset containing the aligned frames.
    pub fn from_frame(frame: Frame) -> Self {
        let raw = frame.as_raw();
        std::mem::forget(frame);
        Self { raw }
    }

    /// Raw pointer to the underlying SDK frameset.
    pub fn as_raw(&self) -> *mut ob_frame {
        self.raw
    }

    /// Number of frames in the set.
    pub fn len(&self) -> u32 {
        // SAFETY: `self.raw` is a valid frameset.
        unsafe { check_error(|e| orbbec_sys::ob_frameset_get_count(self.raw, e)).unwrap_or(0) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a frame of the given type, if present in the set.
    pub fn frame(&self, frame_type: FrameType) -> Option<Frame> {
        // SAFETY: `self.raw` is a valid frameset; the returned frame has its own
        // reference that we take ownership of.
        unsafe {
            Frame::from_raw(check_error(|e| {
                orbbec_sys::ob_frameset_get_frame(self.raw, frame_type.as_raw(), e)
            })
            .unwrap_or(std::ptr::null_mut()))
        }
    }
}

impl Drop for Frameset {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid frameset whose reference we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_frame(self.raw, e)) };
    }
}

/// Pipeline configuration: which streams to enable and how.
pub struct Config {
    raw: *mut ob_config,
}

impl Config {
    /// Create a new, empty pipeline configuration.
    pub fn new() -> Result<Self, Error> {
        // SAFETY: `ob_create_config` returns an owned config object.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_create_config(e))? };
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK config object.
    pub fn as_raw(&self) -> *mut ob_config {
        self.raw
    }

    /// Enable a stream using its default profile.
    pub fn enable_stream(&mut self, stream_type: StreamType) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid config object.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_config_enable_stream(self.raw, stream_type.as_raw(), e)
            })
        }
    }

    /// Enable all streams supported by the device.
    pub fn enable_all_streams(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid config object.
        unsafe { check_error(|e| orbbec_sys::ob_config_enable_all_stream(self.raw, e)) }
    }

    /// Enable a video stream with explicit parameters.
    pub fn enable_video_stream(
        &mut self,
        stream_type: StreamType,
        width: u32,
        height: u32,
        fps: u32,
        format: i32,
    ) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid config object.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_config_enable_video_stream(
                    self.raw,
                    stream_type.as_raw(),
                    width,
                    height,
                    fps,
                    format,
                    e,
                )
            })
        }
    }

    /// Set the depth↔color alignment mode.
    pub fn set_align_mode(&mut self, mode: AlignMode) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid config object.
        unsafe { check_error(|e| orbbec_sys::ob_config_set_align_mode(self.raw, mode.as_raw(), e)) }
    }

    /// Whether the depth frame should be scaled to the color resolution after
    /// depth→color alignment. Defaults to false (depth keeps its resolution).
    pub fn set_depth_scale_after_align_require(&mut self, enable: bool) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid config object.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_config_set_depth_scale_after_align_require(self.raw, enable, e)
            })
        }
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid config object we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_config(self.raw, e)) };
    }
}

/// The frame pipeline: configure, start, and capture frames.
pub struct Pipeline {
    raw: *mut ob_pipeline,
}

// SAFETY: The pipeline is an opaque SDK object; calls on it are serialized and
// not tied to a specific thread.
unsafe impl Send for Pipeline {}

impl Pipeline {
    /// Create a new pipeline for the default (first) device.
    pub fn new() -> Result<Self, Error> {
        // SAFETY: `ob_create_pipeline` returns an owned pipeline object.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_create_pipeline(e))? };
        Ok(Self { raw })
    }

    /// Create a pipeline bound to a specific device.
    ///
    /// # Safety
    ///
    /// `device` must be a valid SDK device handle that stays alive while the
    /// returned pipeline exists.
    pub unsafe fn with_device(device: *mut orbbec_sys::ob_device) -> Result<Self, Error> {
        // SAFETY: guaranteed by the caller; `device` is a valid SDK device handle.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_create_pipeline_with_device(device, e))? };
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK pipeline.
    pub fn as_raw(&self) -> *mut ob_pipeline {
        self.raw
    }

    /// Start the pipeline with default parameters.
    pub fn start(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid pipeline.
        unsafe { check_error(|e| orbbec_sys::ob_pipeline_start(self.raw, e)) }
    }

    /// Start the pipeline with an explicit configuration.
    pub fn start_with_config(&mut self, config: &Config) -> Result<(), Error> {
        // SAFETY: `self.raw` and `config.raw` are valid SDK objects.
        unsafe { check_error(|e| orbbec_sys::ob_pipeline_start_with_config(self.raw, config.raw, e)) }
    }

    /// Start the pipeline, delivering each frameset to `callback`.
    ///
    /// The callback is invoked from an SDK internal thread; it must not block.
    /// The frameset is owned by the callback and is released automatically when
    /// it goes out of scope.
    ///
    /// Only one callback can be registered per pipeline; starting again replaces
    /// it. The callback is removed when the pipeline is stopped.
    pub fn start_with_callback<F>(
        &mut self,
        config: Option<&Config>,
        callback: F,
    ) -> Result<(), Error>
    where
        F: FnMut(Frameset) + Send + 'static,
    {
        let key = self.raw as usize;
        let mut map = callbacks().lock().expect("callback registry poisoned");
        map.insert(key, Box::new(callback));
        drop(map);

        let user_data = key as *mut std::os::raw::c_void;
        // SAFETY: `self.raw` and `config.raw` are valid SDK objects; `callback`
        // stays alive in the registry and is removed in `stop`.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_pipeline_start_with_callback(
                    self.raw,
                    config.map_or(std::ptr::null(), |c| c.raw),
                    Some(frameset_c_callback),
                    user_data,
                    e,
                )
            })
        }
    }

    /// Start the pipeline and receive framesets over a channel.
    ///
    /// The returned receiver yields one [`Frameset`] per SDK callback. Framesets
    /// that reach the receiver are fully owned; dropping them releases the SDK
    /// frame references. Drop the receiver to stop receiving.
    pub fn start_capture(
        &mut self,
        config: Option<&Config>,
    ) -> Result<std::sync::mpsc::Receiver<Frameset>, Error> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.start_with_callback(config, move |frameset| {
            let _ = tx.send(frameset);
        })?;
        Ok(rx)
    }

    /// Stop the pipeline.
    pub fn stop(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid pipeline.
        let result = unsafe { check_error(|e| orbbec_sys::ob_pipeline_stop(self.raw, e)) };
        callbacks().lock().expect("callback registry poisoned").remove(&(self.raw as usize));
        result
    }

    /// Enable frame synchronization between the enabled streams.
    pub fn enable_frame_sync(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid pipeline.
        unsafe { check_error(|e| orbbec_sys::ob_pipeline_enable_frame_sync(self.raw, e)) }
    }

    /// Read the calibration parameters for the current stream configuration.
    ///
    /// If D2C alignment is enabled, these parameters reflect the aligned
    /// (color-sized) depth frame.
    pub fn camera_param(&self) -> Result<CameraParam, Error> {
        // SAFETY: `self.raw` is a valid pipeline; the parameter struct is
        // returned by value and needs no release.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_pipeline_get_camera_param(self.raw, e))? };
        Ok(CameraParam::from_raw(raw))
    }

    /// Block until the next frameset arrives, or until `timeout_ms` elapses.
    pub fn wait_for_frameset(&mut self, timeout_ms: u32) -> Result<Option<Frameset>, Error> {
        // SAFETY: `self.raw` is a valid pipeline; the returned frameset has its
        // own reference that we take ownership of.
        let raw = unsafe {
            check_error(|e| {
                orbbec_sys::ob_pipeline_wait_for_frameset(self.raw, timeout_ms, e)
            })?
        };
        // SAFETY: `raw` is a valid frameset if non-null.
        Ok(unsafe { Frameset::from_raw(raw) })
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid pipeline we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_pipeline(self.raw, e)) };
    }
}
