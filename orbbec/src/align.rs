//! Depth↔color frame alignment filters.
//!
//! In this SDK build, depth→color alignment is applied with a dedicated
//! `"Align"` filter rather than the pipeline config's align mode. An
//! [`AlignFilter`] is told which stream to align to and then fed depth frames
//! (synchronously or via callback).
//!
//! See `examples/aligned.rs` for a working usage.

use orbbec_sys::ob_stream_profile;

use crate::error::Error;
use crate::filter::Filter;
use crate::pipeline::{Frame, Frameset, StreamType};

/// Depth→color (or color→depth) alignment filter.
pub struct AlignFilter {
    inner: Filter,
}

// SAFETY: The filter is an opaque SDK object; calls on it are not tied to a
// specific thread.
unsafe impl Send for AlignFilter {}

impl AlignFilter {
    /// Create a new alignment filter.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: Filter::new("Align")?,
        })
    }

    /// Raw pointer to the underlying SDK filter.
    pub fn as_raw(&self) -> *mut orbbec_sys::ob_filter {
        self.inner.as_raw()
    }

    /// Set the align target from a frame of the target stream.
    ///
    /// Returns `Ok(true)` if a frame of `target` type was found in `frameset`
    /// and used as the alignment target, `Ok(false)` otherwise.
    pub fn set_align_target(&self, frameset: &Frameset, target: StreamType) -> Result<bool, Error> {
        let Some(frame) = frameset.frame(target.as_frame_type()) else {
            return Ok(false);
        };
        let Some(profile) = frame.stream_profile() else {
            return Ok(false);
        };
        // SAFETY: `profile` is owned by the still-alive `frame`.
        unsafe { self.set_align_profile(profile)? };
        Ok(true)
    }

    /// Set the alignment target stream profile explicitly.
    ///
    /// # Safety
    ///
    /// `profile` must be a valid SDK stream profile owned by its frame and must
    /// stay valid until the filter has processed the next frame.
    pub unsafe fn set_align_profile(
        &self,
        profile: *const ob_stream_profile,
    ) -> Result<(), Error> {
        // SAFETY: guaranteed by the caller; `self.inner` is a valid align filter
        // and the target profile is stored by the filter.
        unsafe {
            crate::error::check_error(|e| {
                orbbec_sys::ob_align_filter_set_align_to_stream_profile(
                    self.inner.as_raw(),
                    profile,
                    e,
                )
            })
        }
    }

    /// Align the frameset synchronously (depth warped to the color view).
    pub fn process(&self, frameset: &Frameset) -> Result<Option<Frame>, Error> {
        self.inner.process_frameset(frameset)
    }

    /// Push a frame (e.g. a depth frame) to be aligned asynchronously.
    pub fn push_frame(&self, frame: &Frame) -> Result<(), Error> {
        self.inner.push_frame(frame)
    }

    /// Register the callback that receives each aligned frame (async path).
    pub fn set_callback<F>(&self, callback: F) -> Result<(), Error>
    where
        F: FnMut(Frame) + Send + 'static,
    {
        self.inner.set_callback(callback)
    }
}
