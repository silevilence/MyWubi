//! Android native module; protocol rules remain in core_engine.
pub use core_engine::ffi::PROTOCOL_VERSION;

use jni::{
    objects::{JByteArray, JClass},
    sys::{jbyteArray, jint},
    JNIEnv,
};

/// Protocol is carried in byte arrays: standard UTF-8, never JNI Modified UTF-8.
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_version(_env: JNIEnv, _class: JClass) -> jint {
    PROTOCOL_VERSION as jint
}

/// JNI owns local references; returned arrays are copied into JVM memory.
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_exchange(
    mut env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let len = env.get_array_length(&request).map_err(|e| e.to_string())?;
        if len as usize > core_engine::ffi::MAX_REQUEST {
            return Err("request too large".to_owned());
        }
        let bytes = env
            .convert_byte_array(&request)
            .map_err(|e| e.to_string())?;
        env.byte_array_from_slice(&core_engine::ffi::call(&bytes))
            .map(|a| a.into_raw())
            .map_err(|e| e.to_string())
    }));
    match result {
        Ok(Ok(array)) => array,
        _ => {
            // Preserve a pending JVM exception (including OOM), never replace it.
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    "native protocol transport failed",
                );
            }
            std::ptr::null_mut()
        }
    }
}
