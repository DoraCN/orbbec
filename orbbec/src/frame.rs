//! Typed frame wrappers for convenient pixel access.
//!
//! [`DepthFrame`] interprets a Z16 depth frame as 16-bit millimetre depth
//! values; [`ColorFrame`] exposes RGB(A)/BGR(A) pixel access for the common
//! uncompressed color formats.

use crate::pipeline::Frame;

/// Typed depth frame (Z16, 16-bit values in millimetres).
pub struct DepthFrame {
    frame: Frame,
    width: u32,
    height: u32,
}

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
