//! The device-query symbols were added in API 29. Resolve them at runtime so
//! the same library remains loadable on the application's API 26 minimum.
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{c_char, c_void, CStr};

pub fn devices() -> Result<Vec<Value>, String> {
    // SAFETY: Android NDK NeuralNetworks.h signatures. Device handles and
    // returned strings are NNAPI-owned and used only while the library is live.
    unsafe {
        let library = Library::new("libneuralnetworks.so").map_err(|_| "NNAPI_LIBRARY")?;
        let count = library
            .get::<unsafe extern "C" fn(*mut u32) -> i32>(b"ANeuralNetworks_getDeviceCount\0");
        let get = library.get::<unsafe extern "C" fn(u32, *mut *mut c_void) -> i32>(
            b"ANeuralNetworks_getDevice\0",
        );
        type IntProperty = unsafe extern "C" fn(*const c_void, *mut i32) -> i32;
        type TextProperty = unsafe extern "C" fn(*const c_void, *mut *const c_char) -> i32;
        let kind = library.get::<IntProperty>(b"ANeuralNetworksDevice_getType\0");
        let name = library.get::<TextProperty>(b"ANeuralNetworksDevice_getName\0");
        let version = library.get::<TextProperty>(b"ANeuralNetworksDevice_getVersion\0");
        let (Ok(count), Ok(get), Ok(kind), Ok(name), Ok(version)) =
            (count, get, kind, name, version)
        else {
            return Err("NNAPI_ENUM_API".into());
        };
        let check = |code| {
            if code == 0 {
                Ok(())
            } else {
                Err(format!("NNAPI_STATUS_{code}"))
            }
        };
        let mut n = 0;
        check(count(&mut n))?;
        let mut devices = Vec::new();
        for index in 0..n {
            let mut device = std::ptr::null_mut();
            check(get(index, &mut device))?;
            if device.is_null() {
                return Err("NNAPI_NULL_DEVICE".into());
            }
            let mut device_type = 0;
            check(kind(device, &mut device_type))?;
            let property = |read: TextProperty| -> Result<String, String> {
                let mut ptr = std::ptr::null();
                check(read(device, &mut ptr))?;
                if ptr.is_null() {
                    return Err("NNAPI_NULL_PROPERTY".into());
                }
                Ok(CStr::from_ptr(ptr).to_string_lossy().into_owned())
            };
            devices.push(
                json!({"type":device_type,"name":property(*name)?,"driver":property(*version)?}),
            );
        }
        Ok(devices)
    }
}
