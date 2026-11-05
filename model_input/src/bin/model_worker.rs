#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() {
    if std::env::args().any(|arg| arg == "--trial") {
        use std::io::{Read, Write};
        let mut input = Vec::new();
        if std::io::stdin()
            .take(65537)
            .read_to_end(&mut input)
            .is_err()
        {
            return;
        }
        #[cfg(feature = "ort")]
        if let Ok(exe) = std::env::current_exe() {
            let _ = model_input::onnx::initialize_runtime(&exe.with_file_name("onnxruntime.dll"));
        }
        let _ = std::io::stdout().write_all(&model_input::trial::exchange(&input));
        return;
    }
    if std::env::args().any(|arg| arg == "--features") {
        let features = model_input::service::BackendAvailability::compiled();
        #[cfg(feature = "ort")]
        let features = {
            let mut features = features;
            features.onnx = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("onnxruntime.dll")))
                .is_some_and(|p| model_input::onnx::initialize_runtime(&p).is_ok());
            features
        };
        if let Ok(bytes) = serde_json::to_vec(&features) {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&bytes);
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--check-backends") {
        #[cfg(all(feature = "llama-cpp-2", feature = "ort"))]
        let ready = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("onnxruntime.dll")))
            .is_some_and(|p| model_input::onnx::initialize_runtime(&p).is_ok());
        #[cfg(not(all(feature = "llama-cpp-2", feature = "ort")))]
        let ready = false;
        std::process::exit(if ready { 0 } else { 1 });
    }
    use std::time::{Duration, Instant};
    // Settings may exit immediately after spawning us. Read the atomically
    // saved config here, so neither cold startup nor forwarding depends on it.
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let saved = if args.len() == 2 && args[0] == "--config" {
        // Keep the same absolute spelling used by the settings/TIP path
        // resolver. canonicalize adds a Win32 verbatim prefix, which would
        // make the first ordinary host look like a different model store.
        let path = std::path::PathBuf::from(&args[1]);
        let path = if path.is_absolute() {
            path
        } else {
            let Ok(base) = std::env::current_dir() else {
                return;
            };
            base.join(path)
        };
        let Ok(config) = core_engine::Config::load(&path) else {
            return;
        };
        Some((
            core_engine::config_path::model_store_path(&path),
            config.prediction,
        ))
    } else if args.is_empty() {
        None
    } else {
        return;
    };
    let saved_request = saved.as_ref().and_then(|(root, config)| {
        serde_json::to_vec(&model_input::service::Command::Prepare {
            root: root.clone(),
            config: config.clone(),
        })
        .ok()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut server = loop {
        if let Ok(server) = model_ipc::Server::new() {
            break server;
        }
        let Some(bytes) = &saved_request else {
            return;
        };
        if let Ok(reply) = model_ipc::exchange(bytes, Duration::from_millis(300)) {
            if serde_json::from_slice::<serde_json::Value>(&reply).is_ok_and(|v| v["ok"] == true) {
                return;
            }
        }
        if Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let endpoint = model_input::service::Endpoint::default();
    if let Some((root, config)) = saved {
        // With no running service, an on-demand/disabled save needs no process.
        if !config.enabled
            || config.model.is_none()
            || config.loading != core_engine::config::ModelLoading::PreloadKeep
        {
            return;
        }
        let Ok(bytes) =
            serde_json::to_vec(&model_input::service::Command::Prepare { root, config })
        else {
            return;
        };
        endpoint.exchange(&bytes);
    }
    let mut last = Instant::now();
    let mut owner = 0u32;
    let memory = model_ipc::MemoryPressure::new().ok();
    let mut pressure_check = Instant::now() - Duration::from_secs(2);
    loop {
        if pressure_check.elapsed() >= Duration::from_secs(1) {
            if let Some(low) = memory.as_ref().and_then(|m| m.is_low().ok()) {
                endpoint.resource_pressure(low);
            }
            pressure_check = Instant::now();
        }
        let served = server
            .poll(|pid, bytes| {
                let Ok(mut request) = serde_json::from_slice::<serde_json::Value>(bytes) else {
                    return br#"{"ok":false,"error":"invalid request"}"#.to_vec();
                };
                let op = request["op"].as_str().unwrap_or("").to_owned();
                if matches!(op.as_str(), "Poll" | "Invalidate" | "Pause" | "Close") && owner != pid
                {
                    return br#"{"ok":true,"value":null}"#.to_vec();
                }
                if op == "Configure" || op == "Submit" {
                    if owner != pid {
                        endpoint.exchange(br#"{"op":"Invalidate"}"#);
                    }
                    owner = pid;
                }
                let pointer = if op == "Submit" {
                    "/request/key/edit/session"
                } else {
                    "/key/edit/session"
                };
                if let Some(value) = request.pointer_mut(pointer) {
                    if let Some(session) = value.as_u64() {
                        *value = serde_json::json!(session ^ (u64::from(pid) << 32));
                    }
                }
                let mut response: serde_json::Value = serde_json::from_slice(
                    &endpoint.exchange(&serde_json::to_vec(&request).unwrap_or_default()),
                )
                .unwrap_or_default();
                if let Some(value) = response.pointer_mut("/value/Ok/key/edit/session") {
                    if let Some(session) = value.as_u64() {
                        *value = serde_json::json!(session ^ (u64::from(pid) << 32));
                    }
                }
                if op == "Status" {
                    response["process_id"] = serde_json::json!(std::process::id());
                }
                serde_json::to_vec(&response).unwrap_or_default()
            })
            .unwrap_or(false);
        if served {
            last = Instant::now();
        }
        if last.elapsed() > Duration::from_secs(60) && !endpoint.keep_alive() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(not(windows))]
fn main() {}
