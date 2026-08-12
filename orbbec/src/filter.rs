//! Generic SDK filter wrapper.
//!
//! OrbbecSDK exposes a set of named filters (e.g. `"Align"`,
//! `"PointCloudFilter"`, `"SpatialFilter"`, ...) that process frames. A
//! [`Filter`] can process a frame or frameset either synchronously with
//! [`process`](Filter::process) or asynchronously via a callback.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

use orbbec_sys::{ob_filter, ob_frame};

use crate::error::{check_error, Error};
use crate::pipeline::{Frame, Frameset};

/// Process-global map of active filter callbacks, keyed by the filter's raw
/// pointer value. See the same pattern in `pipeline.rs`.
type FilterCallbackMap = HashMap<usize, Box<dyn FnMut(Frame) + Send>>;

static FILTER_CALLBACKS: OnceLock<Mutex<FilterCallbackMap>> = OnceLock::new();

fn filter_callbacks() -> &'static Mutex<FilterCallbackMap> {
    FILTER_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

extern "C" fn filter_c_callback(frame: *mut ob_frame, user_data: *mut std::os::raw::c_void) {
    let key = user_data as usize;
    let Ok(mut map) = filter_callbacks().lock() else {
        return;
    };
    // SAFETY: `frame` is a valid SDK frame reference owned by this invocation;
    // the registry keeps the callback alive for the filter's lifetime.
    if let (Some(cb), Some(frame)) = (map.get_mut(&key), unsafe { Frame::from_raw(frame) }) {
        cb(frame);
    }
}

/// A named SDK frame-processing filter.
pub struct Filter {
    raw: *mut ob_filter,
}

// SAFETY: The filter is an opaque SDK object; calls on it are not tied to a
// specific thread.
unsafe impl Send for Filter {}

impl Filter {
    /// Create a filter by its registered name, e.g. `"Align"` or
    /// `"PointCloudFilter"`.
    pub fn new(name: &str) -> Result<Self, Error> {
        let name = CString::new(name).map_err(|_| Error {
            status: orbbec_sys::OBStatus_OB_STATUS_ERROR,
            message: "filter name contains a NUL byte".to_string(),
        })?;
        // SAFETY: `name` is a valid C string; the SDK returns an owned filter.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_create_filter(name.as_ptr(), e))? };
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK filter.
    pub fn as_raw(&self) -> *mut ob_filter {
        self.raw
    }

    /// Process a single frame synchronously and return the result frame.
    pub fn process(&self, frame: &Frame) -> Result<Option<Frame>, Error> {
        self.process_raw(frame.as_raw())
    }

    /// Process a frameset synchronously and return the result frame.
    pub fn process_frameset(&self, frameset: &Frameset) -> Result<Option<Frame>, Error> {
        self.process_raw(frameset.as_raw())
    }

    fn process_raw(&self, raw: *const ob_frame) -> Result<Option<Frame>, Error> {
        // SAFETY: `self.raw` is a valid filter and `raw` a valid SDK frame; the
        // result frame (if any) is owned by us.
        let out = unsafe { check_error(|e| orbbec_sys::ob_filter_process(self.raw, raw, e))? };
        // SAFETY: `out` is a valid SDK frame if non-null.
        Ok(unsafe { Frame::from_raw(out) })
    }

    /// Push a frame for asynchronous processing (see [`Filter::set_callback`]).
    pub fn push_frame(&self, frame: &Frame) -> Result<(), Error> {
        // SAFETY: `self.raw` is a valid filter and `frame.raw` a valid frame.
        unsafe { check_error(|e| orbbec_sys::ob_filter_push_frame(self.raw, frame.as_raw(), e)) }
    }

    /// Register the callback that receives each processed frame.
    ///
    /// The callback runs on an SDK internal thread and must not block. Each
    /// [`Frame`] is owned by the callback and freed when dropped.
    pub fn set_callback<F>(&self, callback: F) -> Result<(), Error>
    where
        F: FnMut(Frame) + Send + 'static,
    {
        let key = self.raw as usize;
        filter_callbacks()
            .lock()
            .expect("filter callback registry poisoned")
            .insert(key, Box::new(callback));

        let user_data = key as *mut std::os::raw::c_void;
        // SAFETY: `self.raw` is a valid filter; the callback stays alive in the
        // registry and is removed in `Drop`.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_filter_set_callback(self.raw, Some(filter_c_callback), user_data, e)
            })
        }
    }

    /// Set a numeric filter configuration value by name.
    pub fn set_config_value(&self, name: &str, value: f64) -> Result<(), Error> {
        let name = CString::new(name).map_err(|_| Error {
            status: orbbec_sys::OBStatus_OB_STATUS_ERROR,
            message: "config name contains a NUL byte".to_string(),
        })?;
        // SAFETY: `self.raw` is a valid filter and `name` a valid C string.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_filter_set_config_value(self.raw, name.as_ptr(), value, e)
            })
        }
    }

    /// Get a numeric filter configuration value by name.
    pub fn get_config_value(&self, name: &str) -> Result<f64, Error> {
        let name = CString::new(name).map_err(|_| Error {
            status: orbbec_sys::OBStatus_OB_STATUS_ERROR,
            message: "config name contains a NUL byte".to_string(),
        })?;
        // SAFETY: `self.raw` is a valid filter and `name` a valid C string.
        unsafe {
            check_error(|e| {
                orbbec_sys::ob_filter_get_config_value(self.raw, name.as_ptr(), e)
            })
        }
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        filter_callbacks()
            .lock()
            .expect("filter callback registry poisoned")
            .remove(&(self.raw as usize));
        // SAFETY: `raw` is a valid filter we own.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_filter(self.raw, e)) };
    }
}
