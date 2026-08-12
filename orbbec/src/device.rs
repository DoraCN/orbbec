//! Device enumeration and device handles.
//!
//! A [`Device`] wraps an SDK `ob_device` handle. It is obtained from a
//! [`Context`] and will be the entry point for opening streams (via the
//! pipeline API) in a later milestone.

use std::ffi::CStr;
use std::os::raw::c_char;

use orbbec_sys::{ob_context, ob_device, ob_device_info, ob_device_list};

use crate::error::{check_error, Error};

/// Static information about a connected Orbbec device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device model name, e.g. "Gemini 335".
    pub name: String,
    /// Product ID.
    pub pid: u32,
    /// Vendor ID (0x2bc5 for Orbbec).
    pub vid: u32,
    /// Device UID.
    pub uid: String,
    /// Serial number.
    pub serial_number: String,
    /// Firmware version string.
    pub firmware_version: String,
    /// Connection type, e.g. "USB3.0", "Ethernet".
    pub connection_type: String,
    /// IP address ("0.0.0.0" for USB devices).
    pub ip_address: String,
}

/// A handle to an opened Orbbec device.
pub struct Device {
    raw: *mut ob_device,
}

// SAFETY: The device handle is an opaque SDK object; calls on it are not tied
// to a specific thread.
unsafe impl Send for Device {}

impl Device {
    /// Open the device at `index` in the device list.
    pub(crate) fn from_list(
        _context: *mut ob_context,
        list: *const ob_device_list,
        index: u32,
    ) -> Result<Self, Error> {
        // SAFETY: `list` is a valid SDK device list owned by the caller.
        let raw = unsafe { check_error(|e| orbbec_sys::ob_device_list_get_device(list, index, e))? };
        Ok(Self { raw })
    }

    /// Raw pointer to the underlying SDK device.
    pub fn as_raw(&self) -> *mut ob_device {
        self.raw
    }

    /// Read the static information of this device.
    pub fn info(&self) -> Result<DeviceInfo, Error> {
        // SAFETY: `self.raw` is a valid SDK device handle.
        let info =
            unsafe { check_error(|e| orbbec_sys::ob_device_get_device_info(self.raw, e))? };
        let result = Self::device_info(info);
        // SAFETY: `info` was returned by the SDK and is owned by us.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_device_info(info, e)) };
        result
    }

    fn device_info(info: *const ob_device_info) -> Result<DeviceInfo, Error> {
        // SAFETY: `info` is a valid device info object.
        let name = unsafe { get_string(info, orbbec_sys::ob_device_info_get_name) };
        // SAFETY: `info` is a valid device info object.
        let pid = unsafe { check_error(|e| orbbec_sys::ob_device_info_get_pid(info, e))? };
        // SAFETY: `info` is a valid device info object.
        let vid = unsafe { check_error(|e| orbbec_sys::ob_device_info_get_vid(info, e))? };
        // SAFETY: `info` is a valid device info object.
        let uid = unsafe { get_string(info, orbbec_sys::ob_device_info_get_uid) };
        // SAFETY: `info` is a valid device info object.
        let serial_number =
            unsafe { get_string(info, orbbec_sys::ob_device_info_get_serial_number) };
        // SAFETY: `info` is a valid device info object.
        let firmware_version =
            unsafe { get_string(info, orbbec_sys::ob_device_info_get_firmware_version) };
        // SAFETY: `info` is a valid device info object.
        let connection_type =
            unsafe { get_string(info, orbbec_sys::ob_device_info_get_connection_type) };
        // `ob_device_info_get_ip_address` returns a garbage pointer for non-network
        // devices (SDK quirk), so only read it for network connections.
        let ip_address = if connection_type.starts_with("USB") {
            String::new()
        } else {
            // SAFETY: `info` is a valid device info object.
            unsafe { get_string(info, orbbec_sys::ob_device_info_get_ip_address) }
        };

        Ok(DeviceInfo {
            name,
            pid: pid as u32,
            vid: vid as u32,
            uid,
            serial_number,
            firmware_version,
            connection_type,
            ip_address,
        })
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: `raw` was created by `ob_device_list_get_device` and not yet
        // freed.
        let _ = unsafe { check_error(|e| orbbec_sys::ob_delete_device(self.raw, e)) };
    }
}

/// Read a string field from an SDK device-info getter.
///
/// # Safety
///
/// `info` must be a valid device-info object and `getter` one of the
/// `ob_device_info_get_*` functions that return a `const char*`.
unsafe fn get_string(
    info: *const ob_device_info,
    getter: unsafe extern "C" fn(*const ob_device_info, *mut *mut orbbec_sys::ob_error)
        -> *const c_char,
) -> String {
    // SAFETY: `info` is valid per the caller; the returned string is owned by
    // the SDK and must not be freed.
    unsafe {
        check_error(|e| getter(info, e))
            .map(|ptr| {
                if ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(ptr).to_string_lossy().into_owned()
                }
            })
            .unwrap_or_default()
    }
}
