//! Camera intrinsics, distortion and extrinsics (depth↔RGB).
//!
//! [`CameraParam`] is a copy of the SDK's calibration data. It is obtained from
//! a running [`Pipeline`](crate::pipeline::Pipeline) via
//! [`camera_param`](crate::pipeline::Pipeline::camera_param).

use orbbec_sys::{ob_camera_param, ob_camera_distortion, ob_camera_intrinsic};

/// Camera intrinsics (pinhole model).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intrinsic {
    /// Focal length in x, pixels.
    pub fx: f32,
    /// Focal length in y, pixels.
    pub fy: f32,
    /// Principal point x, pixels.
    pub cx: f32,
    /// Principal point y, pixels.
    pub cy: f32,
    /// Image width, pixels.
    pub width: i16,
    /// Image height, pixels.
    pub height: i16,
}

impl Intrinsic {
    pub(crate) fn from_raw(raw: ob_camera_intrinsic) -> Self {
        Self {
            fx: raw.fx,
            fy: raw.fy,
            cx: raw.cx,
            cy: raw.cy,
            width: raw.width,
            height: raw.height,
        }
    }

    /// Unproject a pixel to a 3D point in the camera coordinate frame.
    ///
    /// `z_mm` is the depth in millimetres (the raw depth value). The returned
    /// point is in metres, with `+Z` pointing away from the camera.
    pub fn unproject(&self, u: f32, v: f32, z_mm: f32) -> [f32; 3] {
        let z = z_mm / 1000.0;
        let x = (u - self.cx) * z / self.fx;
        let y = (v - self.cy) * z / self.fy;
        [x, y, z]
    }
}

/// Radial-tangential lens distortion coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distortion {
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
    pub k4: f32,
    pub k5: f32,
    pub k6: f32,
    pub p1: f32,
    pub p2: f32,
    /// Distortion model (0 = none, 1 = modified Brown–Conrady, ...).
    pub model: u32,
}

impl Distortion {
    pub(crate) fn from_raw(raw: ob_camera_distortion) -> Self {
        Self {
            k1: raw.k1,
            k2: raw.k2,
            k3: raw.k3,
            k4: raw.k4,
            k5: raw.k5,
            k6: raw.k6,
            p1: raw.p1,
            p2: raw.p2,
            model: raw.model,
        }
    }
}

/// Rigid transform between the depth and RGB sensors.
///
/// `rot` is a 3×3 row-major rotation matrix; `trans_mm` is a translation in
/// millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extrinsic {
    pub rot: [f32; 9],
    pub trans_mm: [f32; 3],
}

impl Extrinsic {
    pub(crate) fn from_raw(raw: orbbec_sys::ob_extrinsic) -> Self {
        Self {
            rot: raw.rot,
            trans_mm: raw.trans,
        }
    }

    /// Apply the rotation only, returning the rotated point in the same unit as
    /// the input.
    pub fn rotate(&self, p: [f32; 3]) -> [f32; 3] {
        let r = &self.rot;
        [
            r[0] * p[0] + r[1] * p[1] + r[2] * p[2],
            r[3] * p[0] + r[4] * p[1] + r[5] * p[2],
            r[6] * p[0] + r[7] * p[1] + r[8] * p[2],
        ]
    }

    /// Apply the full rigid transform (rotation + translation).
    ///
    /// `p` is in metres; the translation is converted from millimetres.
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let r = self.rotate(p);
        [
            r[0] + self.trans_mm[0] / 1000.0,
            r[1] + self.trans_mm[1] / 1000.0,
            r[2] + self.trans_mm[2] / 1000.0,
        ]
    }
}

/// Calibration data for the current stream configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraParam {
    /// Depth camera intrinsics.
    pub depth: Intrinsic,
    /// RGB camera intrinsics.
    pub rgb: Intrinsic,
    /// Depth camera distortion.
    pub depth_distortion: Distortion,
    /// RGB camera distortion.
    pub rgb_distortion: Distortion,
    /// Depth→RGB transform (points from depth frame to color frame).
    pub transform: Extrinsic,
    /// Whether the frames for these parameters are mirrored.
    pub is_mirrored: bool,
}

impl CameraParam {
    pub(crate) fn from_raw(raw: ob_camera_param) -> Self {
        Self {
            depth: Intrinsic::from_raw(raw.depthIntrinsic),
            rgb: Intrinsic::from_raw(raw.rgbIntrinsic),
            depth_distortion: Distortion::from_raw(raw.depthDistortion),
            rgb_distortion: Distortion::from_raw(raw.rgbDistortion),
            transform: Extrinsic::from_raw(raw.transform),
            is_mirrored: raw.isMirrored,
        }
    }
}
