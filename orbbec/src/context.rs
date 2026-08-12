use orbbec_sys::ob_context;

/// Root context for the Orbbec SDK.
///
/// A [`Context`] owns the SDK global state. It is required before querying
/// devices or creating pipelines.
pub struct Context {
    raw: *mut ob_context,
}

impl Context {
    /// Create a new SDK context.
    ///
    /// Returns `None` if the SDK could not create a context (e.g. SDK not
    /// initialized / not installed).
    pub fn new() -> Option<Self> {
        // SAFETY: `ob_create_context` returns an owned, heap-allocated context.
        let raw = unsafe { orbbec_sys::ob_create_context() };
        if raw.is_null() {
            None
        } else {
            Some(Self { raw })
        }
    }

    /// Raw pointer to the underlying SDK context.
    pub fn as_raw(&self) -> *mut ob_context {
        self.raw
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `raw` was created by `ob_create_context` and not yet freed.
        unsafe { orbbec_sys::ob_delete_context(self.raw) };
    }
}

// SAFETY: The context is an opaque handle; calls on it are not tied to a thread.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}
