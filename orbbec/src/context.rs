use orbbec_sys::{ob_context, ob_device_list};

use crate::device::{Device, DeviceInfo};
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

    /// Enumerate all connected devices and return their static information.
    pub fn query_devices(&self) -> Result<Vec<DeviceInfo>, Error> {
        // SAFETY: `self.raw` is a valid context.
        let list =
            unsafe { check_error(|e| orbbec_sys::ob_query_device_list(self.raw, e))? };
        let result = self.devices_from_list(list);
        // SAFETY: `list` was returned by the SDK and is owned by us.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_device_list(list, e)) };
        result
    }

    /// Open the device at `index` in the current device list.
    pub fn open_device(&self, index: u32) -> Result<Device, Error> {
        // SAFETY: `self.raw` is a valid context.
        let list =
            unsafe { check_error(|e| orbbec_sys::ob_query_device_list(self.raw, e))? };
        let device = Device::from_list(self.raw, list, index);
        // SAFETY: `list` was returned by the SDK and is owned by us.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_device_list(list, e)) };
        device
    }

    fn devices_from_list(&self, list: *const ob_device_list) -> Result<Vec<DeviceInfo>, Error> {
        // SAFETY: `list` is a valid SDK device list.
        let count = unsafe { check_error(|e| orbbec_sys::ob_device_list_get_count(list, e))? };
        let mut devices = Vec::with_capacity(count as usize);
        for i in 0..count {
            let device = Device::from_list(self.raw, list, i)?;
            let info = device.info()?;
            devices.push(info);
        }
        Ok(devices)
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
