//! Point cloud generation and access.
//!
//! [`PointCloudFilter`] wraps the SDK's `"PointCloudFilter"` to generate a
//! point cloud frame from a (depth, color) frameset. [`PointCloud`] interprets
//! such a frame as an array of 3D points, optionally with colors.

use crate::error::Error;
use crate::filter::Filter;
use crate::pipeline::{Frame, Frameset};

/// Output point format of the point cloud filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointFormat {
    /// XYZ only (3 × f32 per point), units in metres.
    Xyz,
    /// XYZ + RGB (3 × f32 + 3 × f32 per point), coordinates in metres.
    XyzRgb,
}

impl PointFormat {
    pub fn as_raw(&self) -> i32 {
        match self {
            PointFormat::Xyz => orbbec_sys::OBFormat_OB_FORMAT_POINT,
            PointFormat::XyzRgb => orbbec_sys::OBFormat_OB_FORMAT_RGB_POINT,
        }
    }
}

/// Generates point cloud frames from depth (and color) frames.
pub struct PointCloudFilter {
    inner: Filter,
}// SAFETY: The filter is an opaque SDK object; calls on it are not tied to a
// specific thread.
unsafe impl Send for PointCloudFilter {}

impl PointCloudFilter {
    /// Create a new point cloud filter.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Filter::new("PointCloudFilter")?,
        })
    }

    /// Raw pointer to the underlying SDK filter.
    pub fn as_raw(&self) -> *mut orbbec_sys::ob_filter {
        self.inner.as_raw()
    }

    /// Select whether the output contains color.
    pub fn set_point_format(&self, format: PointFormat) -> Result<(), Error> {
        self.inner
            .set_config_value("pointFormat", format.as_raw() as f64)
    }

    /// Downsample factor (1 = no decimation, up to 8).
    pub fn set_decimate(&self, factor: u32) -> Result<(), Error> {
        self.inner.set_config_value("decimate", factor.clamp(1, 8) as f64)
    }

    /// Coordinate scale: raw depth values (in mm) are multiplied by this factor.
    /// Use `0.001` to get coordinates in metres, `1.0` (default) for mm.
    pub fn set_coordinate_scale(&self, scale: f32) -> Result<(), Error> {
        self.inner.set_config_value("coordinateDataScale", scale as f64)
    }

    /// Generate a point cloud frame from a frameset containing depth (and
    /// color, for RGB points) synchronously.
    pub fn generate(&self, frameset: &Frameset) -> Result<Option<Frame>, Error> {
        self.inner.process_frameset(frameset)
    }

    /// Generate a point cloud frame from a single frame (e.g. an aligned
    /// frameset) synchronously.
    pub fn generate_frame(&self, frame: &Frame) -> Result<Option<Frame>, Error> {
        self.inner.process(frame)
    }

    /// Register the callback that receives each generated point cloud frame.
    pub fn set_callback<F>(&self, callback: F) -> Result<(), Error>
    where
        F: FnMut(Frame) + Send + 'static,
    {
        self.inner.set_callback(callback)
    }
}

/// A 3D point with optional color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Red in [0, 1].
    pub r: f32,
    /// Green in [0, 1].
    pub g: f32,
    /// Blue in [0, 1].
    pub b: f32,
}

/// Depth-frame threshold filter (removes depths outside a millimetre range).
///
/// Wraps the SDK `"ThresholdFilter"`. Applied to a depth frame before point
/// cloud generation to cut off too-near / too-far / invalid samples.
pub struct ThresholdFilter {
    inner: Filter,
}

impl ThresholdFilter {
    /// Create a new threshold filter.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Filter::new("ThresholdFilter")?,
        })
    }

    /// Lower bound in millimetres (default 0).
    pub fn set_min_mm(&self, value: u16) -> Result<(), Error> {
        self.inner.set_config_value("min", value as f64)
    }

    /// Upper bound in millimetres (default 16000).
    pub fn set_max_mm(&self, value: u16) -> Result<(), Error> {
        self.inner.set_config_value("max", value as f64)
    }

    /// Filter a depth frame synchronously.
    pub fn process(&self, frame: &Frame) -> Result<Option<Frame>, Error> {
        self.inner.process(frame)
    }
}

/// Depth-frame decimation filter (downsampling).
///
/// Wraps the SDK `"DecimationFilter"`.
pub struct DecimationFilter {
    inner: Filter,
}

impl DecimationFilter {
    /// Create a new decimation filter.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Filter::new("DecimationFilter")?,
        })
    }

    /// Downsample factor, 1..=8.
    pub fn set_decimate(&self, factor: u32) -> Result<(), Error> {
        self.inner.set_config_value("decimate", factor.clamp(1, 8) as f64)
    }

    /// Filter a depth frame synchronously.
    pub fn process(&self, frame: &Frame) -> Result<Option<Frame>, Error> {
        self.inner.process(frame)
    }
}

/// View over a point cloud [`Frame`].
///
/// The frame is owned by this type and released on drop.
pub struct PointCloud {
    frame: Frame,
    format: PointFormat,
}

impl PointCloud {
    /// Take ownership of a point cloud frame produced by [`PointCloudFilter`].
    pub fn from_frame(frame: Frame, format: PointFormat) -> Self {
        Self { frame, format }
    }

    /// Number of points.
    pub fn len(&self) -> usize {
        let stride = match self.format {
            PointFormat::Xyz => 12,
            PointFormat::XyzRgb => 24,
        };
        self.frame.data_size() as usize / stride
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Raw byte buffer of the point cloud.
    pub fn data(&self) -> &[u8] {
        self.frame.data()
    }

    /// The points as `[x, y, z]` triplets (metres).
    pub fn points(&self) -> Vec<[f32; 3]> {
        match self.format {
            PointFormat::Xyz => self
                .data()
                .chunks_exact(12)
                .map(|c| {
                    let x = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let y = f32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                    let z = f32::from_le_bytes([c[8], c[9], c[10], c[11]]);
                    [x, y, z]
                })
                .collect(),
            PointFormat::XyzRgb => self
                .data()
                .chunks_exact(24)
                .map(|c| {
                    let x = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let y = f32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                    let z = f32::from_le_bytes([c[8], c[9], c[10], c[11]]);
                    [x, y, z]
                })
                .collect(),
        }
    }

    /// Points whose depth `z` lies in `[min_m, max_m]` (metres).
    ///
    /// This is the practical "remove outliers" filter: zero-depth samples and
    /// points beyond a working distance are dropped.
    pub fn points_in_range(&self, min_m: f32, max_m: f32) -> Vec<[f32; 3]> {
        self.points()
            .into_iter()
            .filter(|p| p[2] >= min_m && p[2] <= max_m)
            .collect()
    }

    /// The points with colors (RGB_POINT format only).
    pub fn colored_points(&self) -> Vec<ColorPoint> {
        match self.format {
            PointFormat::Xyz => Vec::new(),
            PointFormat::XyzRgb => self
                .data()
                .chunks_exact(24)
                .map(|c| {
                    let x = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let y = f32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                    let z = f32::from_le_bytes([c[8], c[9], c[10], c[11]]);
                    let r = f32::from_le_bytes([c[12], c[13], c[14], c[15]]);
                    let g = f32::from_le_bytes([c[16], c[17], c[18], c[19]]);
                    let b = f32::from_le_bytes([c[20], c[21], c[22], c[23]]);
                    ColorPoint { x, y, z, r, g, b }
                })
                .collect(),
        }
    }
}
