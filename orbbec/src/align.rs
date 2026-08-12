//! Depth↔color frame alignment filters.
//!
//! In this SDK build, depth→color alignment is applied with a dedicated
//! `"Align"` filter rather than the pipeline config's align mode. A
//! [`AlignFilter`] is created, told which stream to align to, and then fed
//! depth frames; each aligned frame is delivered to a callback.
//!
//! See `examples/aligned.rs` for a working usage.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

use orbbec_sys::{ob_frame, ob_filter, ob_stream_profile};

use crate::error::{check_error, Error};
use crate::pipeline::{Frame, Frameset, StreamType};

/// Process-global map of active align-filter callbacks, keyed by the filter's
/// raw pointer value. See the same pattern in `pipeline.rs`.
type AlignCallbackMap = HashMap<usize, Box<dyn FnMut(Frame) + Send>>;

static ALIGN_CALLBACKS: OnceLock<Mutex<AlignCallbackMap>> = OnceLock::new();

fn align_callbacks() -> &'static Mutex<AlignCallbackMap> {
    ALIGN_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn align_c_callback(frame: *mut ob_frame, user_data: *mut std::os::raw::c_void) {
    let key = user_data as usize;
    let Ok(mut map) = align_callbacks().lock() else {
        return;
    };
    // SAFETY: `frame` is a valid SDK frame reference owned by this invocation;
    // the registry keeps the callback alive for the filter's lifetime.
    if let (Some(cb), Some(frame)) = (map.get_mut(&key), unsafe { Frame::from_raw(frame) }) {
        cb(frame);
    }
}

/// Depth→color (or color→depth) alignment filter.
pub struct AlignFilter {
    raw: *mut ob_filter,
}

// SAFETY: The filter is an opaque SDK object; calls on it are not tied to a
// specific thread.
unsafe impl Send for AlignFilter {}

impl AlignFilter {
    /// Create a new alignment filter.
    pub fn new() -> Result<Self, Error> {
        let name = CString::new("Align").expect("Align is a valid filter name");
        // SAFETY: `name` is a valid C string; the SDK returns an owned filter.
        let raw =
            unsafe { check_error(|e| orbbec_sys::ob_create_filter(name.as_ptr(), e))? };
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK filter.
    pub fn as_raw(&self) -> *mut ob_filter {
        self.raw
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
    /// stay valid until the filter has processed the next pushed frame.
    pub unsafe fn set_align_profile(
        &self,
        profile: *const ob_stream_profile,
    ) -> Result<(), Error> {
        // SAFETY: guaranteed by the caller.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_align_filter_set_align_to_stream_profile(self.raw, profile, e)
            })
        }
    }

    /// Push a frame (e.g. a depth frame) to be aligned.
    ///
    /// The frame is referenced by the filter; the caller may keep or drop it.
    pub fn push_frame(&self, frame: &Frame) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid filter and `frame.raw` a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_filter_push_frame(self.raw, frame.as_raw(), e)) }
    }

    /// Register the callback that receives each aligned frame.
    ///
    /// The callback is invoked from an SDK internal thread; it must not block.
    /// Each [`Frame`] is owned by the callback and freed when dropped.
    pub fn set_callback<F>(&self, callback: F) -> Result<(), Error>
    where
        F: FnMut(Frame) + Send + 'static,
    {
        let key = self.raw as usize;
        align_callbacks()
            .lock()
            .expect("align callback registry poisoned")
            .insert(key, Box::new(callback));

        let user_data = key as *mut std::os::raw::c_void;
        // SAFETY: `self.raw` is a valid filter; the callback stays alive in the
        // registry and is removed in `Drop`.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_filter_set_callback(self.raw, Some(align_c_callback), user_data, e)
            })
        }
    }
}

impl Drop for AlignFilter {
    fn drop(&mut self) {
        align_callbacks()
            .lock()
            .expect("align callback registry poisoned")
            .remove(&(self.raw as usize));
        // SAFETY: `raw` is a valid filter we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_filter(self.raw, e)) };
    }
}
