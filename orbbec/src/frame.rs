//! Typed frame wrappers for convenient pixel access.
//!
//! [`DepthFrame`] interprets a Z16 depth frame as 16-bit millimetre depth
//! values; [`ColorFrame`] exposes RGB(A)/BGR(A) pixel access for the common
//! uncompressed color formats.

use crate::pipeline::Frame;

/// A rectangular region in pixel coordinates (e.g. a YOLO detection box).
///
/// Coordinates are in the depth frame's pixel space. When the depth frame is
/// D2C-aligned to the color resolution, this matches the color frame 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundingBox {
    /// Left edge (pixels).
    pub x: u32,
    /// Top edge (pixels).
    pub y: u32,
    /// Width (pixels).
    pub w: u32,
    /// Height (pixels).
    pub h: u32,
}

impl BoundingBox {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> u32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }

    pub fn area(&self) -> u32 {
        self.w * self.h
    }
}

/// Typed depth frame (Z16, 16-bit values in millimetres).
pub struct DepthFrame {
    frame: Frame,
    width: u32,
    height: u32,
}

/// Minimum depth (10 cm, the sensor's lower limit) considered valid for
/// distance measurement; below it the SDK emits garbage.
const MIN_VALID_MM: u16 = 100;
/// Maximum depth considered valid (8 m).
const MAX_VALID_MM: u16 = 8000;
/// Minimum number of valid pixels a box must contain before a distance is
/// reported.
const MIN_BOX_PIXELS: usize = 16;

impl DepthFrame {
    /// Adopt `frame` as a depth frame if its buffer looks like a Z16 image.
    ///
    /// Returns `None` if the frame has no dimensions or its data size does not
    /// match `width * height * 2`.
    pub fn try_new(frame: Frame) -> Option<Self> {
        let width = frame.width();
        let height = frame.height();
        if width == 0 || height == 0 {
            return None;
        }
        if frame.data_size() as usize != (width * height * 2) as usize {
            return None;
        }
        Some(Self {
            frame,
            width,
            height,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// All depth values in row-major order (millimetres).
    pub fn pixels(&self) -> Vec<u16> {
        self.frame
            .data()
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    /// Depth at pixel `(x, y)` in millimetres, or `None` if out of range.
    pub fn pixel(&self, x: u32, y: u32) -> Option<u16> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y * self.width + x) as usize * 2;
        let data = self.frame.data();
        if idx + 1 >= data.len() {
            return None;
        }
        Some(u16::from_le_bytes([data[idx], data[idx + 1]]))
    }

    /// Depth at the image centre, in millimetres.
    pub fn center_depth_mm(&self) -> Option<u16> {
        self.pixel(self.width / 2, self.height / 2)
    }

    /// Depth at the image centre, in metres.
    pub fn center_depth_m(&self) -> Option<f32> {
        self.center_depth_mm().map(|v| v as f32 / 1000.0)
    }

    /// Distance of a rectangular region (e.g. a YOLO detection box).
    ///
    /// Returns `Some((metres, support))`: the median of valid depth samples
    /// inside the box, and the fraction of the box covered by valid depth.
    /// Returns `None` if the box is empty or contains too few valid samples
    /// (e.g. the object is closer than the 10 cm minimum).
    ///
    /// The box must be in this depth frame's pixel space — use a D2C-aligned
    /// depth frame (color resolution) to match YOLO boxes on the color frame
    /// 1:1.
    pub fn box_distance(&self, bbox: &BoundingBox) -> Option<(f32, f32)> {
        let x_hi = bbox.right().min(self.width);
        let y_hi = bbox.bottom().min(self.height);
        if bbox.x >= x_hi || bbox.y >= y_hi {
            return None;
        }

        let mut vals = Vec::with_capacity(bbox.area() as usize);
        for y in bbox.y..y_hi {
            for x in bbox.x..x_hi {
                if let Some(v) = self.pixel(x, y) {
                    if (MIN_VALID_MM..MAX_VALID_MM).contains(&v) {
                        vals.push(v);
                    }
                }
            }
        }

        if vals.len() < MIN_BOX_PIXELS {
            return None;
        }
        let total = (x_hi - bbox.x) as f32 * (y_hi - bbox.y) as f32;
        let support = vals.len() as f32 / total;
        vals.sort_unstable();
        Some((vals[vals.len() / 2] as f32 / 1000.0, support))
    }

    /// Unwrap back into the underlying generic frame.
    pub fn into_frame(self) -> Frame {
        self.frame
    }
}

/// Typed color frame with RGB(A)/BGR(A) pixel access.
pub struct ColorFrame {
    frame: Frame,
    width: u32,
    height: u32,
    format: i32,
}

impl ColorFrame {
    /// Adopt `frame` as a color frame if it has dimensions and a known
    /// uncompressed pixel format (RGB, BGR, RGBA, BGRA, YUYV, Y8).
    pub fn try_new(frame: Frame) -> Option<Self> {
        let width = frame.width();
        let height = frame.height();
        if width == 0 || height == 0 {
            return None;
        }
        let format = frame.format();
        let bytes = (width * height) as usize;
        let expected = match format {
            // RGB / BGR / Y8 / GRAY
            orbbec_sys::OBFormat_OB_FORMAT_RGB
            | orbbec_sys::OBFormat_OB_FORMAT_BGR
            | orbbec_sys::OBFormat_OB_FORMAT_Y8
            | orbbec_sys::OBFormat_OB_FORMAT_GRAY => Some(bytes),
            // RGBA / BGRA
            orbbec_sys::OBFormat_OB_FORMAT_RGBA | orbbec_sys::OBFormat_OB_FORMAT_BGRA => {
                Some(bytes * 4)
            }
            // YUYV / YUY2 / UYVY (2 bytes per pixel)
            orbbec_sys::OBFormat_OB_FORMAT_YUYV
            | orbbec_sys::OBFormat_OB_FORMAT_YUY2
            | orbbec_sys::OBFormat_OB_FORMAT_UYVY => Some(bytes * 2),
            _ => None,
        };
        match expected {
            Some(n) if n == frame.data_size() as usize => Some(Self {
                frame,
                width,
                height,
                format,
            }),
            _ => None,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Raw pixel format (e.g. 22 = RGB, 23 = BGR, 31 = RGBA, 25 = BGRA).
    pub fn format(&self) -> i32 {
        self.format
    }

    /// Raw byte data.
    pub fn data(&self) -> &[u8] {
        self.frame.data()
    }

    /// RGB triplet at pixel `(x, y)`, for RGB/BGR/RGBA/BGRA/Y8 frames.
    pub fn pixel_rgb(&self, x: u32, y: u32) -> Option<(u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let data = self.frame.data();
        match self.format {
            orbbec_sys::OBFormat_OB_FORMAT_RGB => {
                let i = (y * self.width + x) as usize * 3;
                Some((data[i], data[i + 1], data[i + 2]))
            }
            orbbec_sys::OBFormat_OB_FORMAT_BGR => {
                let i = (y * self.width + x) as usize * 3;
                Some((data[i + 2], data[i + 1], data[i]))
            }
            orbbec_sys::OBFormat_OB_FORMAT_RGBA => {
                let i = (y * self.width + x) as usize * 4;
                Some((data[i], data[i + 1], data[i + 2]))
            }
            orbbec_sys::OBFormat_OB_FORMAT_BGRA => {
                let i = (y * self.width + x) as usize * 4;
                Some((data[i + 2], data[i + 1], data[i]))
            }
            orbbec_sys::OBFormat_OB_FORMAT_Y8 | orbbec_sys::OBFormat_OB_FORMAT_GRAY => {
                let i = (y * self.width + x) as usize;
                let v = data[i];
                Some((v, v, v))
            }
            _ => None,
        }
    }

    /// Unwrap back into the underlying generic frame.
    pub fn into_frame(self) -> Frame {
        self.frame
    }
}
