use std::ffi::CStr;
use std::fmt;

/// An error reported by the Orbbec SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Raw `ob_status` code from the SDK.
    pub status: u32,
    /// Human-readable message from the SDK.
    pub message: String,
}

impl Error {
    /// Take ownership of an `ob_error*` returned by the SDK and convert it into
    /// a Rust [`Error`].
    ///
    /// # Safety
    ///
    /// `ptr` must be a non-null pointer to an `ob_error` object returned by the
    /// SDK and not previously freed. The error object is freed by this function.
    pub unsafe fn from_raw(ptr: *mut orbbec_sys::ob_error) -> Self {
        // SAFETY: guaranteed by the caller.
        let status = unsafe { orbbec_sys::ob_error_get_status(ptr) };
        let message = unsafe {
            let c = orbbec_sys::ob_error_get_message(ptr);
            if c.is_null() {
                String::new()
            } else {
                CStr::from_ptr(c).to_string_lossy().into_owned()
            }
        };
        // SAFETY: the error object is owned by the caller of this function.
        unsafe { orbbec_sys::ob_delete_error(ptr) };
        Self { status, message }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Orbbec error {}: {}", self.status, self.message)
    }
}

impl std::error::Error for Error {}

/// Call a SDK function that reports errors through an `ob_error**` out-param.
///
/// If the SDK sets the error pointer, the error is converted and returned as
/// `Err`. Otherwise the function's return value is wrapped in `Ok`.
///
/// # Safety
///
/// `f` must call exactly one SDK function that uses the `ob_error**` error
/// out-param convention and must pass it the provided pointer as-is.
pub unsafe fn check_error<T>(
    f: impl FnOnce(*mut *mut orbbec_sys::ob_error) -> T,
) -> Result<T, Error> {
    let mut err: *mut orbbec_sys::ob_error = std::ptr::null_mut();
    // SAFETY: `err` is a valid pointer to a local `ob_error*` the SDK may write.
    let ret = f(&mut err);
    if err.is_null() {
        Ok(ret)
    } else {
        // SAFETY: `err` is a non-null `ob_error*` owned by us.
        Err(unsafe { Error::from_raw(err) })
    }
}
