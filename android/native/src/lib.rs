//! Android native module; protocol rules remain in core_engine.
pub use core_engine::ffi::PROTOCOL_VERSION;
static PERSONAL_CAPTURE: std::sync::OnceLock<personal_input::capture_endpoint::Endpoint> =
    std::sync::OnceLock::new();
static PERSONAL_PREDICTOR: std::sync::OnceLock<Option<personal_input::predictor::Predictor>> =
    std::sync::OnceLock::new();
static PERSONAL_LEARNING: std::sync::OnceLock<Option<personal_input::background::Worker>> =
    std::sync::OnceLock::new();

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_personalRuntime(
    env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        let request: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let value = match request["op"].as_str()? {
            "Learning" => {
                let worker = PERSONAL_LEARNING
                    .get_or_init(|| personal_input::background::Worker::new().ok())
                    .as_ref()?;
                worker.update(
                    std::path::PathBuf::from(request["config"].as_str()?),
                    request["allowed"].as_bool()?,
                    request["busy"].as_bool()?,
                );
                serde_json::to_value(worker.status()).ok()?
            }
            "Submit" => {
                let worker = PERSONAL_PREDICTOR
                    .get_or_init(|| personal_input::predictor::Predictor::new().ok())
                    .as_ref()?;
                worker.submit(
                    std::path::PathBuf::from(request["root"].as_str()?),
                    serde_json::from_value(request["request"].clone()).ok()?,
                    request["right"].as_str().map(str::to_owned),
                );
                serde_json::Value::Null
            }
            "Poll" => {
                let worker = PERSONAL_PREDICTOR
                    .get_or_init(|| personal_input::predictor::Predictor::new().ok())
                    .as_ref()?;
                serde_json::to_value(
                    worker.take(serde_json::from_value(request["key"].clone()).ok()?),
                )
                .ok()?
            }
            "Invalidate" => {
                if let Some(Some(worker)) = PERSONAL_PREDICTOR.get() {
                    worker.invalidate();
                }
                serde_json::Value::Null
            }
            _ => return None,
        };
        let version = PERSONAL_PREDICTOR
            .get()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.version());
        env.byte_array_from_slice(
            &serde_json::to_vec(
                &serde_json::json!({"ok":true,"value":value,"model_version":version}),
            )
            .ok()?,
        )
        .ok()
        .map(|a| a.into_raw())
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_personalManage(
    env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        env.byte_array_from_slice(&personal_input::api::exchange(&bytes))
            .ok()
            .map(|a| a.into_raw())
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_personalCapture(
    env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        let reply = PERSONAL_CAPTURE
            .get_or_init(Default::default)
            .exchange(&bytes);
        env.byte_array_from_slice(&reply).ok().map(|a| a.into_raw())
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}
static INFERENCE: std::sync::OnceLock<model_input::service::Endpoint> = std::sync::OnceLock::new();
static DOWNLOADS: std::sync::OnceLock<model_input::download_manager::DownloadManager> =
    std::sync::OnceLock::new();

use jni::{
    objects::{JByteArray, JClass, JString},
    sys::{jbyteArray, jint},
    JNIEnv,
};

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_modelDownloads(
    mut env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        let reply = DOWNLOADS.get_or_init(Default::default).exchange(&bytes);
        env.byte_array_from_slice(&reply).ok().map(|a| a.into_raw())
    }));
    match result {
        Ok(Some(array)) => array,
        _ => {
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    "download control failed",
                );
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_runtimeDirectory(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Ok(path) = env.get_string(&path) {
            let path: String = path.into();
            #[cfg(feature = "onnx")]
            model_input::onnx::set_runtime_directory(std::path::Path::new(&path));
            #[cfg(not(feature = "onnx"))]
            let _ = path;
        }
    }));
}

/// Protocol is carried in byte arrays: standard UTF-8, never JNI Modified UTF-8.
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_version(_env: JNIEnv, _class: JClass) -> jint {
    PROTOCOL_VERSION as jint
}

/// Separate short-lived registry lock; native inference never holds the core lock.
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_inference(
    mut env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        let response = INFERENCE.get_or_init(Default::default).exchange(&bytes);
        env.byte_array_from_slice(&response)
            .ok()
            .map(|a| a.into_raw())
    }));
    match result {
        Ok(Some(array)) => array,
        _ => {
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    "inference transport failed",
                );
            }
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_modelPackages(
    mut env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 64 * 1024 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        env.byte_array_from_slice(&model_input::catalog::exchange(&bytes))
            .ok()
            .map(|a| a.into_raw())
    }));
    match result {
        Ok(Some(array)) => array,
        _ => {
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    "model catalog transport failed",
                );
            }
            std::ptr::null_mut()
        }
    }
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

/// Explicit samples run independently of the live IME endpoint and input registry.
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_modelTrial(
    mut env: JNIEnv,
    _class: JClass,
    request: JByteArray,
) -> jbyteArray {
    static TRIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if env.get_array_length(&request).ok()? > 65536 {
            return None;
        }
        let bytes = env.convert_byte_array(&request).ok()?;
        let reply = match TRIAL.try_lock() {
            Ok(_guard) => model_input::trial::exchange(&bytes),
            Err(_) => br#"{"ok":false,"error":"Another trial is running"}"#.to_vec(),
        };
        env.byte_array_from_slice(&reply).ok().map(|a| a.into_raw())
    }));
    match result {
        Ok(Some(array)) => array,
        _ => {
            if !env.exception_check().unwrap_or(true) {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    "model trial transport failed",
                );
            }
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_org_mywubi_NativeBridge_modelGuide(
    env: JNIEnv,
    _class: JClass,
) -> jni::sys::jstring {
    env.new_string(model_input::trial::GUIDE)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
