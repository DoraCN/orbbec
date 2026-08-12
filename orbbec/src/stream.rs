//! Stream profiles: querying and matching the resolutions/formats a sensor
//! supports, and enabling a specific profile in the pipeline config.
//!
//! A [`StreamProfile`] describes one supported stream (resolution, fps,
//! format, type). Lists are obtained from a running
//! [`Pipeline`](crate::pipeline::Pipeline) via
//! [`stream_profiles`](crate::pipeline::Pipeline::stream_profiles), and a
//! matched profile can be enabled through
//! [`Config::enable_stream_with_profile`](crate::pipeline::Config::enable_stream_with_profile).

use orbbec_sys::{ob_stream_profile, ob_stream_profile_list};

use crate::error::{check_error, Error};
use crate::pipeline::StreamType;

/// The SDK "any" sentinel values used when matching profiles.
const ANY_WIDTH: u32 = 0;
const ANY_HEIGHT: u32 = 0;
const ANY_FPS: u32 = 0;
/// `OB_FORMAT_ANY` is `OB_FORMAT_UNKNOWN` (-1).
const ANY_FORMAT: i32 = -1;

/// One supported stream profile (owned).
pub struct StreamProfile {
    raw: *mut ob_stream_profile,
}

unsafe impl Send for StreamProfile {}

impl StreamProfile {
    /// Wrap an owned SDK stream profile.
    ///
    /// # Safety
    ///
    /// `raw` must be a non-null `ob_stream_profile*` owned by the caller.
    pub(crate) unsafe fn from_raw(raw: *mut ob_stream_profile) -> Self {
        Self { raw }
    }

    /// Raw pointer to the underlying SDK profile (owned by this wrapper).
    pub fn as_raw(&self) -> *const ob_stream_profile {
        self.raw
    }

    /// Stream type of this profile.
    pub fn stream_type(&self) -> StreamType {
        // SAFETY: `self.raw` is a valid profile.
        let t = unsafe { check_error(|e| orbbec_sys::ob_stream_profile_get_type(self.raw, e)) }
            .unwrap_or(0);
        StreamType::from_raw(t)
    }

    /// Pixel format (e.g. 28 = Z16, 5 = MJPG, 0 = YUYV).
    pub fn format(&self) -> i32 {
        // SAFETY: `self.raw` is a valid profile.
        unsafe { check_error(|e| orbbec_sys::ob_stream_profile_get_format(self.raw, e)) }
            .unwrap_or(0)
    }

    /// Video width (0 for non-video profiles).
    pub fn width(&self) -> u32 {
        // SAFETY: `self.raw` is a valid profile.
        unsafe { check_error(|e| orbbec_sys::ob_video_stream_profile_get_width(self.raw, e)) }
            .unwrap_or(0)
    }

    /// Video height (0 for non-video profiles).
    pub fn height(&self) -> u32 {
        // SAFETY: `self.raw` is a valid profile.
        unsafe { check_error(|e| orbbec_sys::ob_video_stream_profile_get_height(self.raw, e)) }
            .unwrap_or(0)
    }

    /// Video frames per second (0 for non-video profiles).
    pub fn fps(&self) -> u32 {
        // SAFETY: `self.raw` is a valid profile.
        unsafe { check_error(|e| orbbec_sys::ob_video_stream_profile_get_fps(self.raw, e)) }
            .unwrap_or(0)
    }
}

impl std::fmt::Debug for StreamProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamProfile")
            .field("stream_type", &self.stream_type())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("fps", &self.fps())
            .field("format", &self.format())
            .finish()
    }
}

impl Drop for StreamProfile {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid profile whose reference we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_stream_profile(self.raw, e)) };
    }
}

/// A list of stream profiles supported by a sensor (owned).
pub struct StreamProfileList {
    raw: *mut ob_stream_profile_list,
}

unsafe impl Send for StreamProfileList {}

impl StreamProfileList {
    /// Wrap an owned SDK stream profile list.
    ///
    /// # Safety
    ///
    /// `raw` must be a non-null `ob_stream_profile_list*` owned by the caller.
    pub(crate) unsafe fn from_raw(raw: *mut ob_stream_profile_list) -> Self {
        Self { raw }
    }

    /// Number of profiles in the list.
    pub fn count(&self) -> u32 {
        // SAFETY: `self.raw` is a valid profile list.
        unsafe { check_error(|e| orbbec_sys::ob_stream_profile_list_get_count(self.raw, e)) }
            .unwrap_or(0)
    }

    /// Get the profile at `index`.
    pub fn profile(&self, index: u32) -> Result<StreamProfile, Error> {
        // SAFETY: `self.raw` is a valid list; the returned profile is owned.
        let raw = unsafe {
            check_error(|e| {
                orbbec_sys::ob_stream_profile_list_get_profile(self.raw, index as i32, e)
            })?
        };
        // SAFETY: `raw` is a valid owned profile if non-null.
        Ok(unsafe { StreamProfile::from_raw(raw) })
    }

    /// Match a video profile by resolution / format / fps.
    ///
    /// Pass `None` for any field to leave it unconstrained. Returns `None` if no
    /// profile matches.
    pub fn match_video(
        &self,
        width: Option<u32>,
        height: Option<u32>,
        format: Option<i32>,
        fps: Option<u32>,
    ) -> Result<Option<StreamProfile>, Error> {
        // SAFETY: `self.raw` is a valid list; the returned profile is owned.
        let raw = unsafe {
            check_error(|e| {
                orbbec_sys::ob_stream_profile_list_get_video_stream_profile(
                    self.raw,
                    width.unwrap_or(ANY_WIDTH) as i32,
                    height.unwrap_or(ANY_HEIGHT) as i32,
                    format.unwrap_or(ANY_FORMAT),
                    fps.unwrap_or(ANY_FPS) as i32,
                    e,
                )
            })?
        };
        if raw.is_null() {
            Ok(None)
        } else {
            // SAFETY: `raw` is a valid owned profile.
            Ok(Some(unsafe { StreamProfile::from_raw(raw) }))
        }
    }

    /// Collect all profiles as a vector.
    pub fn collect(&self) -> Vec<StreamProfile> {
        (0..self.count())
            .filter_map(|i| self.profile(i).ok())
            .collect()
    }
}

impl Drop for StreamProfileList {
    fn drop(&mut self) {
        // SAFETY: `raw` is a valid list whose reference we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_stream_profile_list(self.raw, e)) };
    }
}
