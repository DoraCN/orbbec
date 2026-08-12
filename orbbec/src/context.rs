use orbbec_sys::ob_context;

use crate::error::{check_error, Error};

/// Root context for the Orbbec SDK.
///
/// A [`Context`] owns the SDK global state. It is required before querying
/// devices or creating pipelines.
pub struct Context {
    raw: *mut ob_context,
}

impl Context {
    /// Create a new SDK context.
    pub fn new() -> Result<Self, Error> {
        // SAFETY: `ob_create_context` returns an owned, heap-allocated context
        // and reports failure through the error out-param.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_create_context(e))? };
        if raw.is_null() {
            return Err(Error {
                status: orbbec_sys::OBStatus_OB_STATUS_ERROR,
                message: "ob_create_context returned a null context".to_string(),
            });
        }
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK context.
    pub fn as_raw(&self) -> *mut ob_context {
        self.raw
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `raw` was created by `ob_create_context` and not yet freed.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_context(self.raw, e)) };
    }
}

// SAFETY: The context is an opaque handle; calls on it are not tied to a thread.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}
